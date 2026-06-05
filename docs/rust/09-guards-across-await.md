# Mutex/DashMap Guards Across `.await`

## What it is

A *guard* is the value returned when you lock a mutex or borrow an entry from a
map: `std::sync::MutexGuard`, `tokio::sync::MutexGuard`, `dashmap::mapref::one::Ref`,
`dashmap::mapref::one::RefMut`, and similar RAII types. Each guard holds an
exclusive or shared lock for the duration of its lifetime, and releases it when
it is dropped.

Holding any of these guards across an `.await` point means the lock remains held
while the async task is suspended. That is either a compile error (for `!Send`
guards in `tokio::spawn` futures) or a logical bug / deadlock risk (for `Send`
guards that the compiler allows through).

## Why it matters (the problem)

An async task is a state machine that can be suspended at every `.await` point
and resumed — potentially on a different OS thread — later. The Rust compiler
encodes this state machine as a struct containing every local variable that must
survive across a suspension. That struct must implement `Send` for the future to
be usable inside `tokio::spawn`.

`std::sync::MutexGuard<'_, T>` and DashMap's `Ref`/`RefMut` are `!Send` because
their underlying lock primitives (`std::sync::Mutex`, the DashMap sharding
spinlocks) are designed for single-threaded hand-off — it is undefined behaviour
to unlock them from a different thread than the one that locked them. The
compiler encodes this restriction by making the guard `!Send`, which means:

```
error[E0277]: `MutexGuard<'_, …>` cannot be sent between threads safely
   --> src/…rs
    |
    |  tokio::spawn(async move {
    |               ^^^^^^^^^^^ future created by async block is not `Send`
```

Even when the compiler *does* allow it — `tokio::sync::MutexGuard` is `Send` —
holding a lock across `.await` is almost always a design mistake. While one task
is suspended waiting for a network response the lock remains held, preventing
every other task that needs the same lock from making any progress. On a
single-threaded Tokio runtime this becomes a hard deadlock.

## How it works under the hood

When the compiler desugars an `async fn` or `async {}` block into a `Future`
state machine, it inspects which locals are live at each `.await` point. A local
is *live* across an await if it could be accessed after the `.await` returns. Any
live local becomes a field in the generated state-machine struct.

`Send` propagates structurally: if any field of the struct is `!Send`, the entire
struct is `!Send`. A `MutexGuard` or DashMap `Ref`/`RefMut` in a live local
therefore infects the entire future with `!Send`, causing `tokio::spawn` to
reject it at the call site.

The fix is to ensure the guard is *dropped before* the nearest `.await`. In Rust
the drop order follows the end of the enclosing lexical scope. The idiomatic
pattern is an explicit block:

```rust
let value = {
    let guard = some_dashmap.entry(key).or_insert(default);
    let v = *guard;   // copy or clone out of the guard
    v
    // guard drops here — end of block
};

some_async_call().await;   // safe — guard is gone
```

The intermediate block is purely a scope-narrowing device. It costs nothing at
runtime; the compiler drops the guard exactly at the closing `}`.

A second idiom — used in the rate limiter — is to clone an `Arc` out of the
guard before awaiting:

```rust
let limiter: Arc<DefaultDirectRateLimiter> = {
    let entry = self.per_host.entry(host.to_owned())
        .or_insert_with(|| Arc::new(RateLimiter::direct(Quota::per_second(self.qps))));
    Arc::clone(entry.value())
    // entry (the DashMap guard) is dropped here
};

limiter.until_ready().await;   // safe — only an Arc<…> (Send) remains
```

`Arc<T>` is `Send` (when `T: Send + Sync`), so holding the clone across the
await is fine.

## This project — where it appears

### `src/fetcher/rate_limiter.rs` — three explicit guard-drop blocks

The `acquire` async method ([`rate_limiter.rs:74`](../../src/fetcher/rate_limiter.rs))
operates on three `DashMap`s. Each access is wrapped in its own block so that
the DashMap guard is released before the nearest `.await`.

**Block 1 — token-bucket limiter** (lines 79–86):

```rust
let limiter: Arc<DefaultDirectRateLimiter> = {
    let entry = self
        .per_host
        .entry(host.to_owned())
        .or_insert_with(|| Arc::new(RateLimiter::direct(Quota::per_second(self.qps))));
    Arc::clone(entry.value())
    // `entry` (the DashMap guard) is dropped here when it goes out of scope.
};

// Now that the guard is dropped, it is safe to `.await`.
limiter.until_ready().await;
```

`entry` is a `dashmap::mapref::entry::OccupiedEntry` (implements `Deref` but is
`!Send`). Cloning the `Arc` out converts `!Send` data into `Send` data before
the await.

**Block 2 — min_interval** (lines 94–97):

```rust
let min_interval: Option<Duration> = {
    self.min_intervals.get(host).map(|v| *v)
    // Guard dropped here.
};
```

`Duration` is `Copy`, so `*v` copies it out of the `Ref` immediately. The `Ref`
guard is dropped when the block closes.

**Block 3 — last_acquire** (lines 102–105):

```rust
let last: Option<Instant> = {
    self.last_acquire.get(host).map(|v| *v)
    // Guard dropped here.
};
```

Same pattern: `tokio::time::Instant` is `Copy`; the `Ref` is dropped before
`tokio::time::sleep(…).await` on line 110.

---

### `src/crawler/engine.rs` — `tokio::sync::Mutex` held intentionally

The crawler uses `tokio::sync::Mutex<Frontier>` (not `std::sync::Mutex`), and
`tokio::sync::MutexGuard` *is* `Send`. The guards are still kept as short-lived
as practical, but there is one place where a guard is held across an `.await` by
design.

**Frontier seed** (lines 63–66):

```rust
let frontier = Arc::new(Mutex::new(frontier));
{
    let mut f = frontier.lock().await;
    f.enqueue(config.root.as_str(), 0).await?;
}
```

`f.enqueue(…).await` is a Redis call — an async I/O operation — while `f` is
still live. This is intentional: `enqueue` must complete atomically relative to
the frontier state, and the Tokio mutex is the correct tool because it suspends
the task (rather than spinning a thread) while waiting.

The same pattern appears for the dequeue loop in `worker_loop` (lines 192–202),
the `is_complete` check (lines 211–219), the post-crawl `clear` (lines 163–167),
the child-enqueue loop (line 330), and `mark_done_warn` (lines 353–358). In
every case the lock scope is as narrow as the operation requires.

The key distinction from the rate-limiter pattern is:

| | `rate_limiter.rs` | `engine.rs` |
|---|---|---|
| Guard type | `dashmap::Ref` / `OccupiedEntry` | `tokio::sync::MutexGuard` |
| `Send`? | No — `!Send` | Yes — `Send` |
| Held across `.await`? | No — block drops it first | Yes — intentionally |
| Why? | Would fail to compile; and would block all map readers | Redis I/O must happen while the frontier is locked |

## Common mistakes

**1. Forgetting that the `if let` arm extends the guard's lifetime.**

```rust
// WRONG — guard lives across the .await inside the if arm
if let Some(v) = self.map.get("key") {
    some_async_fn(*v).await;   // guard still alive here
}
```

Fix: copy the value out first.

```rust
let maybe = self.map.get("key").map(|v| *v);
if let Some(v) = maybe {
    some_async_fn(v).await;   // guard already gone
}
```

**2. Using `std::sync::Mutex` where `tokio::sync::Mutex` is needed.**

If you truly need to hold a lock across an await, `std::sync::MutexGuard` is
`!Send` and will fail to compile inside `tokio::spawn`. Switch to
`tokio::sync::Mutex`, which parks the task instead of blocking the thread.

**3. Holding a `tokio::sync::MutexGuard` across a slow await unnecessarily.**

Even though it compiles, holding a Tokio mutex guard across a 200 ms network
request serialises all other tasks waiting for that lock. Extract the data you
need, drop the guard, then do the I/O.

**4. Relying on `drop(guard)` instead of a scoping block.**

```rust
let guard = map.get("key");
let v = *guard;
drop(guard);           // explicitly dropped
something().await;     // should be safe...
```

This works, but a scoping block is harder to get wrong: `drop(guard)` in the
middle of a complex function is easy to accidentally reorder or forget after a
refactor.

**5. DashMap entry guards in `match` arms.**

```rust
// WRONG — entry guard from or_insert_with lives through the match
match self.per_host.entry(key).or_insert_with(make) {
    ref e if condition => {
        e.value().something().await;   // guard alive
    }
    _ => {}
}
```

Use an intermediate block to extract the value before the `match`.

## Quick reference

| Guard type | `Send`? | Held across `.await`? | Consequence |
|---|---|---|---|
| `std::sync::MutexGuard` | No | Compiler error | Rewrite: use explicit block or `tokio::sync::Mutex` |
| `dashmap::Ref` / `RefMut` | No | Compiler error | Rewrite: copy/clone value out before `.await` |
| `dashmap::Entry` / `OccupiedEntry` | No | Compiler error | Rewrite: clone `Arc` or copy value out of entry |
| `tokio::sync::MutexGuard` | Yes | Compiles; avoid unless intentional | Serialises all waiters for the lock during the I/O |

**Rule of thumb**: If you find yourself writing `.await` inside a block that also
holds a DashMap or `std::sync` guard, wrap the guard access in its own inner
block and extract a `Copy`/`Clone` value before you reach the await.
