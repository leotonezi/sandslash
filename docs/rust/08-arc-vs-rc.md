# `Arc` vs `Rc`

## What it is

`Rc<T>` and `Arc<T>` are Rust's two reference-counted smart pointers. Both provide shared ownership of a heap-allocated value: cloning either pointer increments a counter, and the value is dropped when the last pointer goes out of scope. The difference is in how the counter is maintained: `Rc<T>` uses a plain integer (single-threaded only), while `Arc<T>` uses an atomic integer (safe across threads).

- **`Rc<T>`** — Reference Counted. `!Send + !Sync`. Single-threaded shared ownership.
- **`Arc<T>`** — Atomically Reference Counted. `Send + Sync` (when `T: Send + Sync`). Multi-threaded shared ownership.

## Why it exists

Rust's ownership model requires exactly one owner per value. That rule is often too restrictive: a configuration struct, a connection pool, or a list of auditors may need to be read from many places at once, none of which is the natural "owner". The classic solution — clone the value — is expensive when the value is large. Reference counting solves this by storing one copy on the heap and letting multiple pointers share it, with the allocation freed only when all pointers are gone.

`Rc` was added first. It is smaller and cheaper because it does not need atomics. `Arc` was added once Rust's threading story matured: moving an `Rc` to another thread would race on the counter, so `Rc` implements `!Send`. `Arc` pays for thread-safety with atomic instructions and is the correct choice whenever a value crosses a `tokio::spawn` boundary.

Without `Arc`, sharing the same `Fetcher` or `CrawlConfig` across dozens of concurrent workers would require either cloning the entire struct for each worker (expensive) or giving each worker a raw pointer (unsafe, no lifetime tracking).

## How it works under the hood

Both types store a single heap allocation with three fields:

```
┌───────────────────────────────────┐
│  strong_count: usize / AtomicUsize│
│  weak_count:   usize / AtomicUsize│
│  value:        T                  │
└───────────────────────────────────┘
```

The pointer-sized header lives before `T` in the same allocation. Cloning a pointer copies only the pointer (8 bytes on 64-bit), not the value, and increments `strong_count`.

**`Rc<T>`** uses plain `usize` for the counters. Increment/decrement are non-atomic loads and stores — roughly one nanosecond. The compiler marks `Rc<T>` as `!Send + !Sync` so it can never be moved to another thread, making the non-atomic access safe.

**`Arc<T>`** uses `AtomicUsize` with `Relaxed` ordering for `clone` and `Acquire`/`Release` for `drop`. An atomic increment costs approximately 5–10 ns on modern hardware (compared to ~1 ns for `Rc`). That cost is almost always irrelevant — it is dominated by the work the shared value actually does.

**`Arc::clone(&arc)` vs `arc.clone()`** — both call the same implementation. The explicit form is idiomatic in multi-threaded code because it makes the intent visible at the call site: "I am creating a new owner, not deep-copying the value." The compiler generates identical code either way.

**`Weak<T>`** — both `Rc` and `Arc` have a `Weak` variant that increments only `weak_count`. A `Weak` reference does not keep the value alive; `upgrade()` returns `None` once all strong references are gone. The primary use-case is breaking reference cycles: if `A` holds an `Arc<B>` and `B` holds an `Arc<A>`, neither is ever freed. Making one link a `Weak` breaks the cycle. This project does not use `Weak` — the ownership graph is acyclic (workers hold `Arc`s to long-lived shared state; nothing holds a back-pointer to the workers).

## This project — where it appears

### `pipeline.rs` — constructing the shared values

Every long-lived object that workers need is wrapped in `Arc` at the top of `pipeline::run`:

```
src/pipeline.rs:22–24
let rate_limiter = Arc::new(HostRateLimiter::new(qps));
let fetcher      = Arc::new(Fetcher::new(&config, Arc::clone(&rate_limiter))?);
let robots_cache = Arc::new(RobotsCache::new());
```

`Arc::new` performs one heap allocation and returns a pointer with `strong_count = 1`. All subsequent `Arc::clone` calls share that single allocation.

For the multi-page crawler path, the auditor lists are also wrapped:

```
src/pipeline.rs:89–90
let page_auditors = Arc::new(crate::audit::page_auditors());
let site_auditors = Arc::new(crate::audit::site_auditors());
```

Note the wrapping strategy: `Arc<Vec<Box<dyn PageAuditor>>>` puts the entire `Vec` in one allocation. The `Arc` is cloned once per worker; no per-auditor allocation is added. If each auditor were individually wrapped as `Arc<Box<dyn PageAuditor>>` there would be one extra allocation per auditor per clone — pointless, because the list is read-only.

### `engine.rs` — cloning into workers

`spawn_crawl_workers` clones every `Arc` once per worker before calling `tokio::spawn`:

```
src/crawler/engine.rs:78–85
let config        = Arc::clone(&config);
let fetcher       = Arc::clone(&fetcher);
let frontier      = Arc::clone(&frontier);
let page_auditors = Arc::clone(&page_auditors);
let site_auditors = Arc::clone(&site_auditors);
let pages_fetched = Arc::clone(&pages_fetched);
let rate_limiter  = Arc::clone(&rate_limiter);
let robots_cache  = Arc::clone(&robots_cache);
```

Each `Arc::clone` is one atomic increment. Eight increments per worker. With `config.concurrency` workers (default 4), that is 32 atomic increments total at startup — a rounding error compared to any network I/O.

`tokio::spawn` requires `Future + Send + 'static`. The cloned `Arc<T>` values satisfy `'static` (no borrowed references) and `Send + Sync` (because every `T` in them — `CrawlConfig`, `Fetcher`, `HostRateLimiter`, etc. — is `Send + Sync`).

The function signatures make these requirements explicit:

```
src/crawler/engine.rs:47–55
pub async fn spawn_crawl_workers(
    config:        Arc<CrawlConfig>,
    fetcher:       Arc<Fetcher>,
    frontier:      Frontier,
    page_auditors: Arc<Vec<Box<dyn PageAuditor>>>,
    site_auditors: Arc<Vec<Box<dyn SiteAuditor>>>,
    rate_limiter:  Arc<HostRateLimiter>,
    robots_cache:  Arc<RobotsCache>,
    reporter:      ProgressReporter,
```

Every shared value arrives as an `Arc`. Nothing is borrowed. The caller retains its own `Arc` clone and can outlive the workers safely.

### `engine.rs` — `AuditContext` inline clone

Inside the worker loop, a fresh `AuditContext` is built for each page's site-auditor pass:

```
src/crawler/engine.rs:294–297
let ctx = AuditContext {
    config:  Arc::clone(&config),
    fetcher: Arc::clone(&fetcher),
};
```

Two atomic increments per page — negligible. The `AuditContext` is short-lived and dropped at the end of the site-audit loop iteration.

### `rate_limiter.rs` — cloning out of DashMap before `.await`

`HostRateLimiter` stores one `Arc<DefaultDirectRateLimiter>` per host in a `DashMap`:

```
src/fetcher/rate_limiter.rs:22
per_host: DashMap<String, Arc<DefaultDirectRateLimiter>>,
```

The `acquire` method clones the `Arc` out of the map entry *before* any `.await`:

```
src/fetcher/rate_limiter.rs:79–86
let limiter: Arc<DefaultDirectRateLimiter> = {
    let entry = self.per_host
        .entry(host.to_owned())
        .or_insert_with(|| Arc::new(RateLimiter::direct(Quota::per_second(self.qps))));
    Arc::clone(entry.value())
    // entry (DashMap guard) dropped here
};
// Safe to .await now — no guard held
limiter.until_ready().await;
```

Without the clone, the `DashMap` entry guard would be held across the `.await` — a deadlock risk (see `docs/rust/09-guards-across-await.md`). The `Arc::clone` costs one atomic increment; it gives the task an independently-owned reference that is valid for the duration of the `await`.

### `engine.rs` — `Arc<AtomicUsize>` for the page counter

```
src/crawler/engine.rs:71
let pages_fetched: Arc<AtomicUsize> = Arc::new(AtomicUsize::new(1));
```

`AtomicUsize` itself provides interior mutability with atomic guarantees, so no `Mutex` is needed. The `Arc` just distributes ownership across workers. This is the canonical pattern for a shared counter in a multi-threaded async program.

## Common mistakes

**Using `Rc` in async code**

```rust
// COMPILE ERROR in tokio::spawn
let data = Rc::new(expensive_value);
tokio::spawn(async move {
    use_data(&data);  // error: Rc cannot be sent between threads safely
});
```

Fix: replace `Rc` with `Arc`. The only reason to prefer `Rc` in Rust is the ~4 ns per-clone savings, which almost never matters.

**Cloning the inner value instead of the pointer**

```rust
// Clones the entire Vec<Box<dyn PageAuditor>> — expensive and wrong
let auditors_copy = (*page_auditors).clone();

// Correct — clones the Arc pointer (one atomic increment)
let auditors_snap = Arc::clone(&page_auditors);
```

When you have `Arc<T>` and you write `arc.clone()`, Rust calls `<Arc<T> as Clone>::clone`, not `T::clone`. But if you accidentally dereference first (`(*arc).clone()`), you clone `T`, which may be expensive. The explicit `Arc::clone(&arc)` form makes the intent unambiguous and prevents this mistake.

**Wrapping mutable state in `Arc<T>` without interior mutability**

```rust
// Does not compile — Arc<T> gives only &T, not &mut T
let counter: Arc<usize> = Arc::new(0);
*counter += 1;  // error: cannot assign through an immutable reference
```

`Arc<T>` provides shared *immutable* access. For shared mutable state use `Arc<Mutex<T>>`, `Arc<RwLock<T>>`, or `Arc<AtomicUsize>` (for primitive counters). This project uses `Arc<AtomicUsize>` for `pages_fetched` (engine.rs:71) and `Arc<Mutex<Frontier>>` for the crawl frontier (engine.rs:62).

**Creating reference cycles**

```rust
struct Node { next: Option<Arc<Node>> }
let a = Arc::new(Node { next: None });
let b = Arc::new(Node { next: Some(Arc::clone(&a)) });
// If a.next = Some(Arc::clone(&b)) — leak!
```

If `A` holds an `Arc<B>` and `B` holds an `Arc<A>`, neither `strong_count` ever reaches zero and both allocations leak. Break cycles with `Weak<T>`. This project's ownership graph is acyclic by design (long-lived state is never back-linked to tasks), so `Weak` is not needed here.

**Assuming `Arc::clone` is expensive**

Developers sometimes introduce unnecessary clones of the inner value to "avoid Arc overhead". In practice, an `Arc::clone` is ~5 ns — far cheaper than a context switch, a memory allocation, or a DNS lookup. Optimise after profiling, not by intuition.

## Quick reference

| | `Rc<T>` | `Arc<T>` |
|---|---|---|
| Thread-safe | No | Yes (if T: Send + Sync) |
| `Send` | No | Yes (if T: Send + Sync) |
| `Sync` | No | Yes (if T: Send + Sync) |
| Clone cost | ~1 ns (non-atomic) | ~5–10 ns (atomic) |
| Use inside `tokio::spawn` | No | Yes |
| Use in single-threaded code | Yes | Yes (but pays atomic cost needlessly) |
| Break cycles with | `Weak<Rc<T>>` | `Weak<Arc<T>>` |

When to choose each:

| Scenario | Choice |
|---|---|
| Shared read-only state across tokio workers | `Arc<T>` |
| Shared counter across workers | `Arc<AtomicUsize>` |
| Shared mutable state across workers | `Arc<Mutex<T>>` or `Arc<RwLock<T>>` |
| DashMap entry guard across `.await` | Clone `Arc` out of entry, drop guard, then `.await` |
| Single-threaded parser tree / graph nodes | `Rc<T>` (never needed in this project) |
| Sharing across `tokio::spawn` | `Arc<T>` — `Rc<T>` is rejected at compile time |
