# `Semaphore` for Bounded Concurrency

## What it is

`tokio::sync::Semaphore` is an async counting semaphore: it holds a fixed pool of
N *permits*. A task must acquire a permit before doing the gated work, and the
permit is returned to the pool when it is dropped. Tasks that call `acquire()`
while all N permits are taken suspend without blocking the thread — they yield to
the Tokio runtime and are woken up as soon as a permit becomes available again.

## Why it exists

When many tasks want to do the same expensive operation — outbound HTTP requests,
database connections, file I/O — launching all of them at once causes two
problems:

1. **Resource exhaustion.** A server with 10 000 pages spawns 10 000 concurrent
   TCP connections. The OS has limits on open sockets and file descriptors; the
   remote host has limits on simultaneous connections; the local network interface
   saturates. Requests start failing or timing out en masse.

2. **Rate limiting.** Remote servers detect connection floods and respond with
   429 Too Many Requests or outright bans.

A worker pool (N long-lived tasks, each pulling from a shared queue) is one
solution. A `Semaphore` is the other: spawn as many tasks as you like, but gate
the expensive operation so at most N run concurrently. The choice between the two
is discussed below.

## How it works under the hood

Internally `tokio::sync::Semaphore` stores an atomic counter and a wait-queue of
suspended futures (a `LinkedList` inside the Tokio internals). The fast path —
permit available — is a single compare-and-swap on the counter and returns
immediately. The slow path adds the caller's `Waker` to the wait-queue and
suspends; when a permit holder drops its `SemaphorePermit`, Tokio decrements the
counter and wakes the front of the queue.

Key types:

| Type | Lifetime | Created by |
|---|---|---|
| `SemaphorePermit<'_>` | Borrowed — tied to the `Semaphore` | `sem.acquire().await` |
| `OwnedSemaphorePermit` | `'static` — holds an `Arc<Semaphore>` | `sem.acquire_owned().await` |

`OwnedSemaphorePermit` is the async version of a "ticket" you can move into a
`tokio::spawn` closure without lifetime issues. `SemaphorePermit<'_>` cannot
cross a `spawn` boundary because the borrow would escape the original scope.

`Semaphore::close()` poisons the semaphore: all pending and future `acquire`
calls return `Err(AcquireError)`. This is the canonical graceful-drain signal —
close first, then `join_next` until empty.

Runtime cost: O(1) on the fast path (CAS). Context switches only happen when the
slow path is taken (all permits held). There is no per-task allocation on the
fast path.

## This project — where it appears

### `BrokenLinksAuditor` — `src/audit/links.rs:5,13,117–131`

`links.rs` probes every URL found on a page to detect broken links. Without a
cap, auditing a page with 1 000 links would fire 1 000 simultaneous HEAD
requests.

```
src/audit/links.rs:5
use tokio::sync::Semaphore;

src/audit/links.rs:13
const MAX_CONCURRENT_PROBES: usize = 32;
```

The constant sets the ceiling. The semaphore is created once per `audit()` call,
wrapped in `Arc` so every spawned task can hold a clone:

```
src/audit/links.rs:117–131
let sem = Arc::new(Semaphore::new(MAX_CONCURRENT_PROBES));
let fetcher = Arc::clone(&ctx.fetcher);
let mut join_set: JoinSet<(Url, ProbeOutcome)> = JoinSet::new();

for url in to_probe {
    let sem = Arc::clone(&sem);
    let fetcher = Arc::clone(&fetcher);
    join_set.spawn(async move {
        let _permit = sem
            .acquire_owned()
            .await
            .expect("invariant: semaphore is never closed");
        let outcome = probe(&fetcher, &url).await;
        (url, outcome)
    });
}
```

The key details:

- `acquire_owned()` is used, not `acquire()`. The returned `OwnedSemaphorePermit`
  is `'static` and can move into the `tokio::spawn` closure. `acquire()` would
  return a `SemaphorePermit<'_>` that borrows the `Semaphore` and cannot cross
  the spawn boundary.
- The permit is bound to `_permit`. Rust drops named variables at end of scope,
  so the permit lives for the entire duration of `probe()` and is released when
  the spawned future exits.
- The `expect` is safe: the semaphore is never explicitly closed, so
  `acquire_owned()` only returns `Err` if `close()` was called, which it is not.

All tasks are spawned before any result is awaited; Tokio schedules up to 32
`probe()` calls at a time and queues the rest. `JoinSet::join_next` drains them
in completion order (not spawn order).

### `SitemapAuditor` — `src/audit/sitemap.rs:15,197–234`

The sitemap auditor samples up to `MAX_SAMPLED_URLS` (50) entries from
`sitemap.xml` and probes each one. This path uses `futures::StreamExt::buffer_unordered`
instead of an explicit `Semaphore + JoinSet`:

```
src/audit/sitemap.rs:15
const MAX_CONCURRENT_PROBES: usize = 32;

src/audit/sitemap.rs:197–234
let probe_findings: Vec<Finding> = stream::iter(loc_urls)
    .map(|url| async move { /* HEAD probe */ })
    .buffer_unordered(MAX_CONCURRENT_PROBES)
    .filter_map(|opt| async move { opt })
    .collect()
    .await;
```

`buffer_unordered(N)` is a stream combinator that drives at most N futures
concurrently, buffering the rest — semantically equivalent to a `Semaphore` with
N permits, but expressed as a pipeline. The two approaches are interchangeable
for this pattern; `links.rs` uses `Semaphore + JoinSet` because it needs
heterogeneous task lifetimes, while `sitemap.rs` uses `buffer_unordered` because
all probes are homogeneous and the results are collected in one pass.

### Worker pool — `src/crawler/engine.rs:47–116`

The crawl engine uses a different concurrency model: a fixed pool of
`config.concurrency` long-lived worker tasks, each pulling URLs from a shared
Redis-backed frontier. This is not a `Semaphore`; it is a static worker pool.

```
src/crawler/engine.rs:77
for _ in 0..config.concurrency {
    // ...
    let handle = tokio::spawn(async move { worker_loop(...).await });
    handles.push(handle);
}
```

The concurrency ceiling is enforced structurally: only `config.concurrency` tasks
exist. There is no need for a semaphore because the pool is fixed at startup. A
`Semaphore` would be appropriate here if tasks were spawned dynamically (e.g., on
each URL discovery event) rather than pre-allocated.

## Common mistakes

**Using `acquire()` inside `tokio::spawn`**

```rust
// COMPILE ERROR — SemaphorePermit<'_> borrows sem; cannot move into spawn
let permit = sem.acquire().await.unwrap();
tokio::spawn(async move {
    do_work().await;
    drop(permit);  // error: `permit` borrows `sem` which doesn't live long enough
});
```

Fix: use `acquire_owned()` so the permit is `'static` and can move into the
closure:

```rust
let sem = Arc::new(Semaphore::new(8));
// ...
let sem_clone = Arc::clone(&sem);
tokio::spawn(async move {
    let _permit = sem_clone.acquire_owned().await.unwrap();
    do_work().await;
    // _permit dropped here — permit returned to pool
});
```

**Dropping the permit too early**

```rust
// BAD — permit released before work completes
let _permit = sem.acquire_owned().await.unwrap();
drop(_permit);          // immediately returns permit
expensive_work().await; // now unguarded — N+1 tasks can run concurrently
```

Rust drops `_permit` at the `drop()` call, not at end of scope. Keep the permit
bound to a named variable that outlives the gated work, or rely on scope-based
drop.

**Not using `Arc<Semaphore>` when sharing across tasks**

```rust
// COMPILE ERROR — Semaphore is not Clone; cannot move into multiple closures
let sem = Semaphore::new(8);
for _ in 0..100 {
    tokio::spawn(async move {
        let _p = sem.acquire_owned().await.unwrap(); // can't move sem here
    });
}
```

Fix: wrap in `Arc` first, then clone the `Arc` per task (as in `links.rs:117–131`).

**Forgetting that `Semaphore::close()` poisons acquire**

If you call `Semaphore::close()` for graceful drain, subsequent `acquire()` /
`acquire_owned()` calls return `Err(AcquireError)`. The `.expect("semaphore is
never closed")` pattern used in `links.rs:128` is correct precisely because the
semaphore in that scope is never closed. If you do close it, handle the error:

```rust
match sem.acquire_owned().await {
    Ok(permit) => { /* work */ drop(permit); }
    Err(_) => { /* semaphore closed — drain path */ }
}
```

**Semaphore vs worker pool: choosing the wrong tool**

| Scenario | Better choice |
|---|---|
| Fixed set of long-lived, homogeneous workers (crawl engine) | Worker pool (`tokio::spawn` × N) |
| Ad-hoc burst of heterogeneous tasks with a concurrency cap | `Semaphore` |
| Ordered pipeline over a stream of items | `buffer_unordered(N)` |
| Dynamic concurrency cap that changes at runtime | `Semaphore` (re-add permits with `add_permits`) |

## Quick reference

```rust
use std::sync::Arc;
use tokio::sync::Semaphore;

// Create
let sem = Arc::new(Semaphore::new(32));          // 32 permits

// Borrow permit (within one async scope — not movable into spawn)
let permit = sem.acquire().await.unwrap();
do_work().await;
drop(permit); // or let scope end

// Owned permit ('static — safe inside tokio::spawn)
let sem_clone = Arc::clone(&sem);
tokio::spawn(async move {
    let _permit = sem_clone.acquire_owned().await.unwrap();
    do_work().await;
    // _permit dropped here
});

// Graceful drain
sem.close();                 // poison: future acquire() returns Err
// join all in-flight tasks — they will finish their current work and then fail
// to re-acquire, which is the exit signal

// Query remaining permits
let available = sem.available_permits();

// Dynamically increase permits (rare — e.g. config reload)
sem.add_permits(8);
```

| API | Returns | Notes |
|---|---|---|
| `Semaphore::new(N)` | `Semaphore` | N must be ≤ `usize::MAX >> 3` |
| `sem.acquire()` | `SemaphorePermit<'_>` | Borrows `sem`; cannot move into `spawn` |
| `sem.acquire_owned()` | `OwnedSemaphorePermit` | `'static`; holds `Arc<Semaphore>` internally |
| `sem.try_acquire()` | `Result<SemaphorePermit, TryAcquireError>` | Non-blocking; `Err` if no permits available |
| `sem.close()` | `()` | Poisons semaphore; all pending/future acquires return `Err` |
| `sem.available_permits()` | `usize` | Instantaneous snapshot; may be stale |
| `sem.add_permits(N)` | `()` | Increase the cap dynamically |
