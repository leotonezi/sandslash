# `Send` + `Sync`

## What it is

`Send` and `Sync` are auto-traits that the compiler derives automatically.

- **`Send`**: a type can be *moved* to another thread.
- **`Sync`**: a type can be *referenced* from another thread (`&T: Send` iff `T: Sync`).

Both are marker traits — no methods, no vtable. They are either derived automatically by the compiler or explicitly opted out with `impl !Send for T`.

## Why it exists

Rust's thread-safety guarantees are enforced at compile time, not runtime. Without `Send`/`Sync`, the compiler cannot reason about whether it is safe to move data across thread boundaries or share references between threads. Any type that is unsafe to transfer (`Send`) or share (`Sync`) must declare that by being `!Send` or `!Sync`, which prevents it from ever appearing inside a `tokio::spawn` closure or an `Arc<T>`.

## How it works under the hood

The compiler auto-derives `Send` for a type if all of its fields are `Send`, and `Sync` if all fields are `Sync`. The rules propagate structurally:

- `*const T` and `*mut T` are `!Send + !Sync` — raw pointers carry no ownership or synchronisation guarantees.
- Any type containing a raw pointer field inherits `!Send + !Sync` unless it explicitly implements the traits (with an `unsafe impl`).
- `Rc<T>` is `!Send + !Sync` — its reference count is not atomic.
- `RefCell<T>` is `Send` (if `T: Send`) but `!Sync` — interior mutability without a lock is not safe to share.
- `Arc<T>` is `Send + Sync` (if `T: Send + Sync`) — atomic reference count, safe to clone across threads.
- `Mutex<T>` is `Send + Sync` (if `T: Send`) — the lock mediates shared access.

`tokio::spawn` requires `Future + Send + 'static`. This is the boundary where `!Send` types are rejected: if a future holds a `!Send` value across an `.await` point, the compiler refuses to compile it.

## This project — where it appears

### `scraper::Html` is `!Send`

`scraper` builds a DOM tree using `ego-tree`, which stores nodes in a `Vec` and uses `*const` / `*mut` raw pointers for parent/child/sibling links. Because raw pointers are `!Send`, `Html` is `!Send`, and by extension `Dom` (which wraps `Html`) is also `!Send`.

```
src/parser/dom.rs:43–45
pub struct Dom {
    html: Html,   // Html is !Send — Dom inherits !Send
}
```

### `spawn_blocking` as the escape hatch

Because `Dom` is `!Send`, it cannot be sent into a `tokio::spawn` future. The solution is `tokio::task::spawn_blocking`, which runs a closure on a dedicated thread-pool thread. The closure is `'static + Send`, but `Dom` is constructed *inside* the closure — it never crosses a thread boundary.

```
src/crawler/engine.rs:270–282
let blocking_result = tokio::task::spawn_blocking(move || {
    let dom = Dom::parse(&html);         // Dom created here — never leaves the closure
    let findings = auditors_snap.iter()
        .flat_map(|a| a.audit(&page_snap, &dom))
        .collect();
    let child_urls = discover_links(&base_url, &dom);
    (findings, child_urls)
}).await;
```

The `html: String` is `Send` and moves into the closure. `Dom` is built from it inside the blocking thread, used, and dropped before the closure returns. Only `Vec<Finding>` and `Vec<Url>` — both `Send` — cross back to the async task.

The doc comment at `engine.rs:9–10` states this explicitly:
> Runs all page-auditors **and** link discovery inside `spawn_blocking` (because `Dom` is `!Send`).

### Trait bounds: `PageAuditor: Send + Sync`

```
src/audit/mod.rs:22
pub trait PageAuditor: Send + Sync { ... }
```

`PageAuditor` objects are stored as `Box<dyn PageAuditor>` inside `Arc<Vec<Box<dyn PageAuditor>>>` (`engine.rs:51`). For `Arc<T>` to be `Send + Sync`, `T` must be `Send + Sync`. Because `Box<dyn PageAuditor>` is a fat pointer to a heap-allocated trait object, the `Send + Sync` supertrait bounds propagate through the vtable, ensuring every concrete auditor satisfies them.

The same applies to `SiteAuditor`:

```
src/audit/mod.rs:29
pub trait SiteAuditor: Send + Sync { ... }
```

### `async_trait` and `Send`

`#[async_trait]` (used on `SiteAuditor`) desugars `async fn audit(...)` into a method returning `Pin<Box<dyn Future<Output = ...> + Send>>`. The `+ Send` bound is added automatically, which means every `await` point inside an `async_trait` method must also be `Send`. This is why `Dom` cannot appear inside a `SiteAuditor::audit` implementation — it would make the returned `Future` `!Send`.

## Common mistakes

**Holding `Dom` across `.await`**

```rust
// COMPILE ERROR — Dom is !Send; future becomes !Send
let dom = Dom::parse(&html);
some_async_call().await;     // dom lives here → !Send future
do_something(&dom);
```

Fix: use `spawn_blocking` as shown in `engine.rs:270–282`, or restructure so `Dom` is dropped before the first `.await`.

**`Arc<RefCell<T>>` instead of `Arc<Mutex<T>>`**

`RefCell<T>` is `!Sync` — putting it in `Arc` gives an `Arc` that is `!Send + !Sync`. The compiler rejects it in `tokio::spawn`. Use `Arc<Mutex<T>>` or `Arc<RwLock<T>>` for shared mutable state across tasks.

**Forgetting `+ Send` on trait object bounds**

```rust
// Works in single-threaded code, fails in spawn
let auditors: Vec<Box<dyn PageAuditor>> = ...;
tokio::spawn(async move { use_auditors(auditors) });
// ^^^ error: `dyn PageAuditor` cannot be sent between threads safely
```

The bound must be `Box<dyn PageAuditor + Send + Sync>` — or the trait definition must declare `Send + Sync` as supertraits (which is how `PageAuditor` is defined in this project).

**`impl !Send` propagation is invisible**

If a third-party crate opts out of `Send` for a type (`impl !Send for Foo`), the compiler error appears at your call site, not in the library. Always check third-party types in async code: raw pointers, `Rc`, `RefCell`, C FFI types, and anything DOM-shaped (HTML parsers, XML parsers) are the usual suspects.

## Quick reference

| Type | Send | Sync | Reason |
|---|---|---|---|
| `scraper::Html` | No | No | Contains `*const` / `*mut` raw pointers |
| `Dom` (this project) | No | No | Wraps `Html` |
| `Arc<T>` | Yes (if T: Send+Sync) | Yes (if T: Send+Sync) | Atomic refcount |
| `Rc<T>` | No | No | Non-atomic refcount |
| `Mutex<T>` | Yes (if T: Send) | Yes (if T: Send) | Lock mediates access |
| `RefCell<T>` | Yes (if T: Send) | No | Not safe to share |
| `Box<dyn Trait + Send + Sync>` | Yes | Yes | Explicit supertrait bounds |

When to use each escape hatch:

| Scenario | Solution |
|---|---|
| `!Send` type needed for CPU work | `spawn_blocking` — construct and use inside closure |
| Shared mutable state across tasks | `Arc<Mutex<T>>` or `Arc<RwLock<T>>` |
| Read-only shared state | `Arc<T>` (if T: Sync) |
| Single-threaded context only | `Rc<T>`, `RefCell<T>` — but cannot cross `tokio::spawn` |
