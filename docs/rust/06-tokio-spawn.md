# `tokio::spawn` & `'static + Send`

## What it is

`tokio::spawn` submits an async future to the Tokio runtime for concurrent
execution.  It returns a `JoinHandle<T>` immediately — the future runs in the
background while the caller continues.  Unlike threads, spawned tasks are
lightweight green threads: switching between them costs only a context save
inside the runtime, not an OS context switch.

The full signature is:

```rust
pub fn spawn<F>(future: F) -> JoinHandle<F::Output>
where
    F: Future + Send + 'static,
    F::Output: Send + 'static,
```

Both bounds — `Send` and `'static` — are checked at compile time.  If either is
missing the compiler rejects the call with an error pointing to the captured
variable that violates the bound.

## Why it exists

### The `'static` bound

A spawned task has no guaranteed relationship to the stack frame that spawned
it.  The caller might return, the future might be moved to a different worker
thread, or the task might live until the runtime shuts down long after the
original scope exits.  None of those lifetimes can be expressed with a borrow:
the task must own everything it uses, or reference data whose lifetime is
`'static` (i.e. lives for the entire program, such as string literals or data
behind `Arc`).

Without `'static` you could hand a future a borrow of a local variable, the
variable could be dropped, and the future could later access freed memory.  The
bound prevents that at compile time.

### The `Send` bound

Tokio's default `Runtime::new()` / `#[tokio::main]` builds a multi-threaded
scheduler that moves tasks between OS threads to keep all cores busy.  A future
must therefore be safe to send across thread boundaries — exactly what `Send`
expresses.  Internally Tokio maintains a work-stealing deque per thread; tasks
that are `Send` can be stolen by any thread without data races.

If you only ever have one OS thread (e.g. a
`tokio::runtime::Builder::new_current_thread()` runtime), the `Send` bound is
not needed — that is why `tokio::task::spawn_local` exists (see the contrast
section below).

## How it works under the hood

1. **Task allocation** — `tokio::spawn` boxes the future and its waker
   infrastructure into a single heap allocation (a `Task<S>` where `S` is the
   scheduler).
2. **Scheduling** — the task is pushed onto the current thread's local run
   queue.  Other threads may steal it from there if they are idle.
3. **Polling** — the runtime calls `Future::poll` on the task whenever it is
   ready.  Between polls the task is parked; no OS thread is blocked.
4. **Completion** — when `poll` returns `Poll::Ready(value)`, the value is
   stored inside the `JoinHandle`.  Any task awaiting the handle is woken.
5. **Panic propagation** — if the spawned future panics, the panic is caught by
   the runtime.  The `JoinHandle` resolves to `Err(JoinError)` where
   `JoinError::is_panic()` returns `true`.  The panic is *not* propagated to the
   spawning task unless the caller explicitly calls `.unwrap()` or checks the
   error.  Calling `handle.abort()` cancels the task and causes the handle to
   resolve to `Err(JoinError)` where `JoinError::is_cancelled()` is `true`.

`JoinHandle<T>` itself implements `Future<Output = Result<T, JoinError>>`.
Awaiting it blocks the current task until the spawned task completes.

## This project — where it appears

### Worker pool spawn loop (`src/crawler/engine.rs`, lines 77–107)

```rust
for _ in 0..config.concurrency {
    let config        = Arc::clone(&config);
    let fetcher       = Arc::clone(&fetcher);
    let frontier      = Arc::clone(&frontier);
    let page_auditors = Arc::clone(&page_auditors);
    let site_auditors = Arc::clone(&site_auditors);
    let pages_fetched = Arc::clone(&pages_fetched);
    let rate_limiter  = Arc::clone(&rate_limiter);
    let robots_cache  = Arc::clone(&robots_cache);
    let tx            = tx.clone();
    let reporter      = reporter.clone();

    let handle = tokio::spawn(async move {       // line 91
        worker_loop(
            config, fetcher, frontier,
            page_auditors, site_auditors,
            pages_fetched, rate_limiter,
            robots_cache, tx, reporter,
        )
        .await;
    });

    handles.push(handle);
}
```

Every variable that enters the `async move` block is first replaced with an
`Arc`-cloned or channel-cloned copy.  The `move` keyword transfers ownership of
those clones into the future.  Because `Arc<T>` is `Send + 'static` (when
`T: Send + Sync`) and the channel sender is `Send`, the resulting future
satisfies both bounds.

None of the originals (`config`, `fetcher`, …) are passed directly — that would
transfer the only copy and leave the loop body unable to clone for the next
iteration.

### JoinHandle collection and await (`src/crawler/engine.rs`, lines 155–158)

```rust
for handle in handles {
    let _ = handle.await;
}
```

The underscore discard is intentional: by the time this loop runs, the channel
has already drained (the `while let Some(report) = rx.recv().await` loop above
returned `None`, meaning all `tx` clones were dropped, meaning all workers
exited their loops).  Awaiting the handles here is a clean-shutdown fence — it
ensures the tasks have fully returned before the frontier is cleared.

### Timeout path: `handle.abort()` (`src/pipeline.rs`, lines 144–146)

```rust
for handle in handles {
    handle.abort();
}
```

When the global timeout fires, every worker is cancelled via `abort()`.  This
drops each worker's `tx` clone, which closes the channel and allows the drain
loop below it to terminate.

### `spawn_blocking` contrast in the same file (`src/crawler/engine.rs`, line 270)

```rust
let blocking_result = tokio::task::spawn_blocking(move || {
    let dom = Dom::parse(&html);
    // …
})
.await;
```

`Dom` (the `scraper::Html` wrapper) is `!Send`, so it cannot be held across an
`.await` point in a `tokio::spawn` future.  Instead the synchronous, CPU-bound
parsing is offloaded to `spawn_blocking`, which runs on a dedicated thread pool
that does not require `Send` for the closure's local variables.  See
`docs/rust/10-spawn-blocking.md` for a full treatment.

## Common mistakes

### 1. Passing a local borrow into `async move`

```rust
// DOES NOT COMPILE — `data` is borrowed, not owned
let data = String::from("hello");
tokio::spawn(async move {
    println!("{}", &data); // &data captures data by reference — not 'static
});
drop(data); // could happen before the task runs
```

Fix: pass by value, or wrap in `Arc` and clone before spawning.

### 2. Capturing a `Mutex` guard across an `.await`

```rust
// DOES NOT COMPILE — MutexGuard is !Send
let guard = mutex.lock().unwrap();
tokio::spawn(async move {
    guard.do_something();  // guard is !Send — future is !Send
    some_async_fn().await; // the guard lives past this .await
});
```

Fix: drop the guard before the `.await`, or use `tokio::sync::Mutex` (whose
guard *is* `Send`) instead of `std::sync::Mutex`.

In this project the pattern appears in `src/crawler/engine.rs` at the dequeue
block (lines 192–202): the frontier lock guard is confined to an inner `{ }`
block and released before any `.await` call.

### 3. Ignoring `JoinError` from a panicking task

```rust
let _ = handle.await; // silently swallows a panic in the spawned task
```

In production code, check the result:

```rust
match handle.await {
    Ok(value) => { /* use value */ }
    Err(e) if e.is_panic() => { /* log / propagate */ }
    Err(e) if e.is_cancelled() => { /* task was aborted */ }
    Err(e) => unreachable!("unexpected JoinError: {e}"),
}
```

This project discards the result intentionally at the join fence (line 158)
because worker errors are already logged inside `worker_loop` and the reports
channel is the authoritative result.

### 4. Forgetting `move` and getting borrow-of-local errors

```rust
let name = String::from("crawler");
tokio::spawn(async {       // no `move` — `name` is borrowed
    println!("{}", name);  // error: `name` does not live long enough
});
```

`async { }` without `move` captures variables by reference.  Almost all
`tokio::spawn` call sites need `async move { }`.

### 5. Spawning `!Send` futures on the multi-threaded runtime

Types like `scraper::Html`, `std::rc::Rc`, and raw pointer wrappers are `!Send`.
If you accidentally capture one in an `async move` block passed to
`tokio::spawn`, the compiler error looks like:

```
error[E0277]: `*mut ...` cannot be sent between threads safely
    --> src/crawler/engine.rs:91:22
     |
91   |     let handle = tokio::spawn(async move {
     |                  ^^^^^^^^^^^^ `*mut ...` cannot be sent between threads safely
```

The fix is to ensure the `!Send` value is only used inside `spawn_blocking` or
`spawn_local`.

## Quick reference

| Need | Use |
|------|-----|
| Run an async future concurrently, value is `Send + 'static` | `tokio::spawn(async move { … })` |
| Await the result / check for panic | `handle.await` → `Result<T, JoinError>` |
| Cancel a running task | `handle.abort()` → handle resolves to `Err(JoinError)` |
| Run sync / CPU-heavy code without blocking the async thread | `tokio::task::spawn_blocking(move \|\| { … })` |
| Run an `!Send` future on the current thread | `tokio::task::spawn_local(async { … })` (requires `LocalSet`) |
| Share owned state across spawned tasks | `Arc::clone(&value)` before each `spawn` |
| Share mutable state across spawned tasks | `Arc<tokio::sync::Mutex<T>>` — guard is `Send` |
| Ensure all workers finish | collect `JoinHandle`s, `handle.await` each one |
