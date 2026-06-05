# `spawn_blocking`

## What it is

`tokio::task::spawn_blocking` runs a synchronous, potentially blocking closure
on a dedicated **blocking thread pool** that is separate from the async worker
threads. It returns a `JoinHandle<T>` that the calling async task can `.await`,
receiving the closure's return value once it finishes. The closure signature is
`FnOnce() -> T + Send + 'static`, and `T` must be `Send`.

```rust
let handle: JoinHandle<String> = tokio::task::spawn_blocking(|| {
    // synchronous work here
    expensive_sync_computation()
});
let result: String = handle.await?; // awaits without blocking the async pool
```

## Why it exists

Tokio's async worker threads are a small, fixed-size pool (typically one thread
per CPU core). When an async task is polled, it is expected to return control
very quickly — ideally in microseconds. Any call that **blocks the OS thread**
(blocking file I/O, `std::thread::sleep`, CPU-intensive computation, or
constructing types with raw-pointer internals) holds up that thread for the
duration, which prevents every other task scheduled on that thread from making
progress. In a `multi_thread` runtime this causes latency spikes; in a
`current_thread` runtime it deadlocks the entire program.

`spawn_blocking` solves this by routing blocking work to a **separate pool**
whose threads are allowed to block indefinitely without affecting the async
pool. The default cap on this pool is 512 threads; tokio creates threads on
demand and reuses idle ones.

A second problem `spawn_blocking` solves in this project: `scraper::Html`
contains raw pointer internals and is therefore `!Send`. It cannot be
constructed on an async task and then sent across a thread boundary — the
compiler refuses it. By constructing `Dom` (which wraps `scraper::Html`)
*inside* the `spawn_blocking` closure, the value is created and consumed on the
same blocking thread and never crosses a thread boundary.

## How it works under the hood

**Thread pool management.** Tokio maintains two pools:

1. **Async pool** — `N` threads (default: number of CPU cores). Each runs the
   tokio scheduler loop, polling futures.
2. **Blocking pool** — up to 512 threads (configurable via
   `Builder::max_blocking_threads`). Threads are created lazily on first need
   and kept alive for a configurable idle timeout (default: 10 seconds).

When `spawn_blocking(f)` is called, tokio wraps `f` in a task and posts it to
the blocking pool's internal queue. If an idle blocking thread is available it
wakes up and runs `f`; otherwise a new OS thread is created. Meanwhile, the
calling async task suspends at the `.await` on the `JoinHandle` without
occupying an async worker thread.

**`Send + 'static` requirement.** The closure and its captured variables are
moved onto the blocking thread, which may be a different OS thread from the
caller. `Send` guarantees the move is safe. `'static` ensures no borrowed data
can be invalidated before the blocking thread finishes — the thread may outlive
the scope that spawned it.

**`JoinHandle<T>` and `JoinError`.** The handle resolves to
`Result<T, JoinError>`. A `JoinError` occurs when the blocking thread **panics**
— tokio catches the panic and surfaces it as `Err(JoinError)` rather than
unwinding the async task. The `JoinError` can be inspected with
`err.is_panic()` and the panic payload retrieved with `err.into_panic()`.

**`!Send` values.** Any type that is `!Send` (such as `scraper::Html`,
`Rc<T>`, or raw pointers) can still be used inside the closure — they just
cannot be *sent into* it from outside. The canonical pattern is to construct
the `!Send` value at the top of the closure and keep it local for the
closure's lifetime.

**Cost model.** Thread creation is O(1) amortised because threads are pooled.
The real cost is the OS context switch when the blocking thread is scheduled
and when the async task is woken up after the handle resolves. This overhead
is worth bearing for work that takes more than roughly 1 ms. For sub-millisecond
synchronous snippets, running inline in an async context (or using
`block_in_place`) is usually cheaper.

## This project — where it appears

### `src/crawler/engine.rs` — lines 264–291 (the canonical call site)

The module-level doc comment (line 9) states the design intent directly:

> "4. Runs all page-auditors **and** link discovery inside `spawn_blocking`
>    (because `Dom` is `!Send`)."

The actual call is at line 270:

```rust
// src/crawler/engine.rs:264–291
// ── Page audits + link discovery (spawn_blocking — Dom is !Send) ─────
let html = page_data.html.clone();
let page_snap = page_data.clone();
let auditors_snap = Arc::clone(&page_auditors);
let base_url = url.clone();

let blocking_result = tokio::task::spawn_blocking(move || {
    let dom = Dom::parse(&html);                          // Dom constructed here

    let findings: Vec<crate::model::Finding> = auditors_snap
        .iter()
        .flat_map(|a| a.audit(&page_snap, &dom))
        .collect();

    let child_urls: Vec<Url> = discover_links(&base_url, &dom);

    (findings, child_urls)
})
.await;

let (mut findings, child_urls) = match blocking_result {
    Ok(pair) => pair,
    Err(e) => {
        tracing::warn!(url = %url, error = %e, "spawn_blocking panicked; skipping page");
        mark_done_warn(&frontier).await;
        continue;
    }
};
```

Notice the structure:

- `html`, `page_snap`, `auditors_snap`, and `base_url` are all `Send` types
  (`String`, `PageData` clone, `Arc<Vec<...>>`, `Url`). They are moved into
  the closure.
- `Dom` is constructed *inside* the closure at line 271. It never crosses a
  thread boundary.
- Both CPU-heavy operations — running all page auditors and link discovery —
  happen inside the same closure, avoiding two separate `spawn_blocking` calls.
- Panic recovery is explicit: `JoinError` is matched on and logged, then the
  page is skipped gracefully (lines 286–290).

### `src/parser/dom.rs` — `Dom` struct (lines 50–52)

```rust
// src/parser/dom.rs:50–52
pub struct Dom {
    html: Html,
}
```

`Dom` wraps `scraper::Html`. The `scraper` crate represents the parsed HTML
tree using `ego_tree`, which stores nodes via raw pointers internally. Raw
pointers are `!Send`, so the compiler does not derive `Send` for `Html` or
`Dom`. This is the root cause that makes `spawn_blocking` necessary — it is
not possible to hold a `Dom` across an `.await` point or send it to a
`tokio::spawn` future.

## Common mistakes

**1. Constructing the `!Send` value outside and trying to move it in.**

```rust
// WRONG — compiler error: Dom is !Send
let dom = Dom::parse(&html);
tokio::task::spawn_blocking(move || {
    dom.title() // cannot move `dom` into `spawn_blocking`
});
```

Fix: construct `Dom` at the top of the closure body, as the engine does.

**2. Blocking inside a regular `tokio::spawn` future.**

```rust
// WRONG — blocks an async worker thread
tokio::spawn(async move {
    let dom = Dom::parse(&huge_html); // CPU-heavy + !Send
    ...
});
```

Fix: use `spawn_blocking` for any work involving `Dom` or CPU-intensive
parsing.

**3. Ignoring the `JoinError`.**

```rust
// BAD — silently loses pages on auditor panics
let (findings, child_urls) = blocking_result.unwrap();
```

Fix: match on `Err(JoinError)` and handle it explicitly, as in engine.rs
lines 285–290.

**4. Using `block_in_place` instead of `spawn_blocking`.**

`tokio::task::block_in_place` is superficially similar but runs the blocking
work on the *current* async thread, temporarily suspending the scheduler on
that thread. This is only appropriate under `current_thread` flavour or when
you specifically need to keep local state on the current thread. In a
`multi_thread` runtime `block_in_place` forces tokio to migrate all tasks
off the current thread before blocking, incurring extra overhead. Prefer
`spawn_blocking` for work that is fully independent of the current async
thread's state.

**5. Spawning many tiny `spawn_blocking` calls in a tight loop.**

```rust
// AVOID — creates a new task dispatch per iteration
for page in pages {
    tokio::task::spawn_blocking(move || parse(page)).await?;
}
```

Batch the work into a single closure when possible to amortise dispatch
overhead, or use a channel to feed a persistent blocking worker thread.

**6. Forgetting `'static` on captured references.**

```rust
let config: &CrawlConfig = ...;
tokio::task::spawn_blocking(move || {
    do_something(config) // error: `config` does not live long enough
});
```

The closure requires `'static`. Pass `Arc`-wrapped values into the closure
instead of bare references.

## Quick reference

| Situation | Use |
|-----------|-----|
| CPU-heavy work (> ~1 ms) | `spawn_blocking` |
| `!Send` type that must be constructed and consumed together | `spawn_blocking` (construct inside) |
| Synchronous blocking I/O (file, DB driver, etc.) | `spawn_blocking` |
| Pure async work, all types are `Send + 'static` | `tokio::spawn` |
| Must stay on current thread, `current_thread` runtime | `block_in_place` |
| Sub-millisecond synchronous work in an async context | Run inline (no spawn needed) |

**Closure contract:**

```
FnOnce() -> T + Send + 'static
         ^T must also be Send
```

**Panic recovery:**

```rust
match handle.await {
    Ok(value)  => { /* use value */ }
    Err(e) if e.is_panic() => { /* blocking thread panicked */ }
    Err(e)     => { /* task was cancelled (rare) */ }
}
```
