# Channels (`mpsc`) & Drop-Sender Pattern

## What it is

`tokio::sync::mpsc` (multiple-producer, single-consumer) is an async channel that
lets many tasks send values to one receiver. Two constructors exist:

- `unbounded_channel::<T>()` — returns `(UnboundedSender<T>, UnboundedReceiver<T>)`;
  the queue grows without limit and `send` never blocks.
- `channel::<T>(n)` — returns `(Sender<T>, Receiver<T>)` with capacity `n`;
  `send` is async and suspends when the buffer is full (backpressure).

The key property that this engine relies on: **the channel is closed when every
`Sender` clone is dropped**. At that point `receiver.recv().await` returns `None`
instead of suspending — signalling "no more items will ever arrive".

## Why it exists

The alternative to a channel for collecting work from multiple concurrent tasks is
`tokio::task::JoinHandle`. Each handle resolves to exactly one `T` when the task
finishes. That works when you know in advance how many values you will collect, and
when you want to wait for all tasks before doing anything with the results.

A channel fits better when:

- Results should be **processed as they arrive** rather than batched at the end.
- The number of results is not known at spawn time (a crawler may find 1 page or
  10,000).
- The receiver needs to do useful work (progress reporting, early output) while
  workers are still running.

In seo-rs the crawler scores and sends one `PageReport` per page. The reports feed
the progress bar and accumulate in a `Vec` while workers continue crawling. If
`JoinHandle` were used instead, no report would be available until all workers had
completely finished.

## How it works under the hood

### Reference counting

`UnboundedSender<T>` (and `Sender<T>`) hold an `Arc`-backed reference count on a
shared channel state. Cloning a sender increments that count; dropping a sender
decrements it. When the count reaches zero the inner state is marked closed.

```
unbounded_channel()
  -> tx (refcount = 1)   rx

tx.clone()
  -> tx  (refcount = 2)   rx
  -> tx2 (same refcount)

drop(tx)   -> refcount = 1   (still open)
drop(tx2)  -> refcount = 0   (CLOSED — rx.recv() returns None)
```

### Async recv

`UnboundedReceiver::recv` is implemented as a `Future`. When the queue is empty but
the channel is still open the future registers a waker and returns `Poll::Pending` —
yielding control back to the Tokio runtime. The runtime re-polls the future when a
sender pushes a new item (it calls `waker.wake()` inside `send`). When the channel
is closed the future resolves to `Poll::Ready(None)`.

### UnboundedSender::send is NOT async

`UnboundedSender::send(&self, value: T) -> Result<(), SendError<T>>` is a plain
synchronous method. It appends to the queue and wakes the receiver's waker
immediately. Because the queue is unbounded it can never be full, so it never needs
to wait — hence no `.await` is required or possible. The `Result` only errors if the
receiver has already been dropped.

### Bounded channel backpressure

`Sender::send` from a bounded channel IS async: `async fn send(&self, value: T) ->
Result<(), SendError<T>>`. It suspends when the internal ring-buffer is at capacity
and resumes when the receiver has consumed an item. This creates natural backpressure
— producers slow down to match the consumer's throughput.

## This project — where it appears

All channel usage is concentrated in `src/crawler/engine.rs`.

### Channel creation — line 72

```rust
let (tx, rx) = tokio::sync::mpsc::unbounded_channel::<PageReport>();
```

An unbounded channel is chosen because the consumer (the result-collection loop
below) runs on the same thread as the spawner. A bounded channel would risk
deadlock: if the buffer filled up, workers would block trying to `send`, but the
consumer cannot drain the buffer until all workers are done — a cycle with no exit.

### Per-worker sender clone — lines 86–88

```rust
// Each worker holds its own tx clone; they are all dropped when the
// worker finishes, which closes the channel once the last worker exits.
let tx = tx.clone();
```

Inside the `for _ in 0..config.concurrency` loop each iteration clones `tx` and
moves the clone into its `tokio::spawn` closure. After `config.concurrency`
iterations there are that many live clones plus the original `tx`.

### The drop — line 113

```rust
// ── 4. Drop the original sender ──────────────────────────────────────────────
// This is critical: the channel closes when all per-worker tx clones are
// dropped.  If we keep this sender alive, `rx.recv()` never returns `None`.
drop(tx);
```

After the spawn loop completes, `tx` (the original sender) is explicitly dropped.
This is the "drop-sender" pattern. Without it the reference count never reaches zero
even after every worker exits, and `rx.recv().await` would suspend forever.

### Per-page send — line 306

```rust
let _ = tx.send(report);
```

Each worker calls `tx.send` once per successfully audited page. The call is
synchronous and infallible from the worker's perspective (the `_` discards the
`Result` because a dropped receiver after a panic is non-fatal).

### Drain loop — lines 149–151

```rust
while let Some(report) = rx.recv().await {
    reports.push(report);
}
```

`run_crawl` drives this loop. It suspends on each `rx.recv().await` call until
either a report arrives or the channel closes. When all worker tasks exit, the last
`tx` clone is dropped, the channel closes, `recv` returns `None`, and the `while
let` exits. The loop therefore finishes naturally — no polling, no `sleep`, no
sentinel value.

### Join after drain — lines 157–159

```rust
for handle in handles {
    let _ = handle.await;
}
```

Workers exit their `worker_loop` when the frontier reports completion, which happens
before they drop their `tx` clone. By the time the drain loop exits all workers have
already finished; the join is a no-op in practice but ensures clean shutdown if a
worker panicked.

## Common mistakes

### Forgetting `drop(tx)` after spawning

```rust
// BUG: tx is still alive — rx.recv() suspends forever
let (tx, mut rx) = unbounded_channel::<u32>();
for _ in 0..4 {
    let tx = tx.clone();
    tokio::spawn(async move { tx.send(1).unwrap(); });
}
while let Some(v) = rx.recv().await { /* never exits */ }
```

Fix: add `drop(tx);` after the spawn loop, before the drain loop.

### Dropping rx before all senders are done

If `rx` is dropped first, subsequent `tx.send(...)` calls return
`Err(SendError(...))`. In seo-rs this is why the drain loop is in `run_crawl` and
`rx` is kept alive until all reports are collected.

### Using a bounded channel in a single-threaded collect pattern

With a bounded channel, if the buffer fills and the producer is blocked inside
`send`, but the consumer only drains after all producers exit, you get a deadlock.
This is exactly why `unbounded_channel` is used here — the consumer and producers
run concurrently, but having an unbounded queue removes any possibility of a
deadlock at the channel boundary.

### Ignoring backpressure with unbounded channels under memory pressure

`unbounded_channel` can grow without limit. If workers produce much faster than the
consumer drains, memory usage grows unboundedly. In the crawler this is acceptable
because `PageReport` is small and the consumer (the drain loop in `run_crawl`) runs
as fast as items arrive. For a pipeline where the consumer is significantly slower
than producers, prefer `channel(n)` and accept the backpressure.

### Holding a `Sender` clone in a long-lived struct

If a `Sender` clone is stored in an `Arc`-shared struct that outlives the workers,
the channel will not close when workers finish. Always tie sender lifetimes to the
task lifetime.

## Quick reference

| Question | Answer |
|---|---|
| How many producers can share a channel? | Unlimited — clone `Sender` / `UnboundedSender` as many times as needed |
| How many consumers? | One — `Receiver` / `UnboundedReceiver` is not `Clone` |
| When does `recv()` return `None`? | When every sender clone has been dropped |
| Is `UnboundedSender::send` async? | No — synchronous, never blocks |
| Is `Sender::send` (bounded) async? | Yes — suspends when buffer is full |
| Which to choose for collect-after-spawn? | `unbounded_channel` to avoid deadlock |
| Which to choose when producers are much faster than consumers? | `channel(n)` with backpressure |
| What is the "drop-sender" pattern? | Drop the original `tx` after cloning into all workers so the channel closes when workers exit |
| Where in seo-rs? | `src/crawler/engine.rs` lines 72, 88, 113, 306, 149–151 |
