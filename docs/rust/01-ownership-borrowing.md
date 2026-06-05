# Ownership & Borrowing

## What it is

Every Rust value has exactly one *owner* — the variable binding that holds it. When the owner goes out of scope, the value is dropped (freed). A value can be *moved* to a new owner or *borrowed* temporarily via a reference. Borrowing comes in two forms: shared (`&T`, any number simultaneously) and exclusive (`&mut T`, only one at a time, no other borrows live).

The borrow checker enforces these rules at compile time. No garbage collector, no runtime reference count for plain values.

## Why it exists

Memory safety without a GC. The rules prevent:
- **Use-after-free** — owner gone, value already dropped
- **Double-free** — two owners both try to drop the same value
- **Data races** — two threads simultaneously with `&mut` access
- **Iterator invalidation** — mutating a collection while iterating it

If the code compiles, these classes of bugs are impossible.

## How it works under the hood

### Move semantics

When you assign or pass a value, ownership transfers. The old binding is gone:

```rust
let s = String::from("hello");
let t = s;        // s is moved into t
println!("{s}");  // compile error: value used after move
```

Stack-copyable types (`i32`, `bool`, `u8`, `Url` if it implements `Copy`, etc.) are implicitly copied instead of moved — but most heap-owning types (`String`, `Vec`, `Box`) are not `Copy`.

### Borrow rules (the "XOR" rule)

At any point in time, for a given value, you can have **either**:
- Any number of `&T` (shared references), **or**
- Exactly one `&mut T` (exclusive reference)

Never both at once. The borrow checker verifies this statically.

### Lifetimes

Every reference has a *lifetime* — the span of code during which it is valid. The compiler infers most lifetimes via *lifetime elision rules*. Explicit `'a` annotations appear when the compiler cannot infer the relationship:

```rust
fn longest<'a>(x: &'a str, y: &'a str) -> &'a str { ... }
```

`'static` is the longest lifetime — it lives for the entire program. String literals (`"hello"`) are `&'static str`. `tokio::spawn` requires `'static` on captured values because the spawned task can outlive the current scope (see `docs/rust/06-tokio-spawn.md`).

### Clone vs move

`Clone` creates a deep copy of the value, giving both the original and the copy independent ownership. Use it deliberately — cloning heap data (`String`, `Vec`) allocates. The preferred pattern in this project for shared access is `Arc::clone`, which clones the *pointer*, not the *data*.

## This project — where it appears

### `Arc::clone` — cloning the pointer, not the data

```
src/crawler/engine.rs:78–89
for _ in 0..config.concurrency {
    let config = Arc::clone(&config);
    let fetcher = Arc::clone(&fetcher);
    let frontier = Arc::clone(&frontier);
    let page_auditors = Arc::clone(&page_auditors);
    ...
    let handle = tokio::spawn(async move { ... });
}
```

Each `Arc::clone` increments an atomic reference count — it is a cheap pointer copy. The spawned `async move` block takes ownership of the cloned `Arc`s. The original `Arc`s remain alive in the outer scope. When the last clone is dropped, the heap data is freed.

The comment at `rate_limiter.rs:84`:
> Each call to `acquire` clones the inner `Arc<DefaultDirectRateLimiter>` out of the `DashMap` entry *before* any `.await` point

```
src/fetcher/rate_limiter.rs:79–86
let limiter: Arc<DefaultDirectRateLimiter> = {
    let entry = self.per_host
        .entry(host.to_owned())
        .or_insert_with(|| Arc::new(RateLimiter::direct(...)));
    Arc::clone(entry.value())
    // entry (DashMap guard) dropped here
};
limiter.until_ready().await;  // safe: no guard held
```

This is the canonical Arc + DashMap pattern: clone the `Arc` out of the guard before any `.await`.

### `move` closures — transferring ownership into blocking threads

```
src/crawler/engine.rs:270–282
let html = page_data.html.clone();     // clone the String — owned copy
let page_snap = page_data.clone();     // clone PageData
let base_url = url.clone();

let blocking_result = tokio::task::spawn_blocking(move || {
    // html, page_snap, base_url are moved in — closure owns them
    let dom = Dom::parse(&html);       // borrow html inside the closure
    ...
});
```

`html` is cloned first because `page_data` is used again later in the same scope (`site_auditors` loop at line 298). The clone gives the closure its own independent `String`. The `move` keyword transfers ownership of `html`, `page_snap`, and `base_url` into the closure.

### `&self` with interior mutability vs `&mut self`

`HostRateLimiter::acquire` takes `&self`, not `&mut self`:

```
src/fetcher/rate_limiter.rs:74
pub async fn acquire(&self, host: &str) {
```

This looks read-only, but `DashMap` provides interior mutability — it uses fine-grained locking internally. Taking `&mut self` would prevent concurrent callers (only one `&mut` at a time). `&self` + `DashMap` allows many concurrent `acquire` calls across worker tasks simultaneously.

### `PageData: Clone` — explicit deep copy

```
src/model.rs:7–8
#[derive(Debug, Clone, Serialize)]
pub struct PageData { ... }
```

`PageData` derives `Clone` because the engine needs to hand one copy to `spawn_blocking` and keep another for `site_auditors`. The `html: String` field is heap-allocated, so this is a real allocation. It is deliberate: the cost is accepted to avoid the complexity of reference-counted `PageData` fields.

### `&str` vs `String` — borrowed vs owned

`PageAuditor::id` returns `&'static str`:

```
src/audit/mod.rs:23
fn id(&self) -> &'static str;
```

String literals like `"metadata"` are `&'static str` — pointer into the binary's read-only data segment. No allocation, no ownership. If the ID were dynamic (`String`), every call would allocate. `&'static str` is the right choice for fixed, compile-time-known identifiers.

The same pattern appears in `audit/metadata.rs:10`: `fn id(&self) -> &'static str { "metadata" }`.

### Owned `CrawlConfig` moved through the pipeline

```
src/pipeline.rs:19
pub async fn run(config: CrawlConfig) -> anyhow::Result<AuditReport> {
```

`run` takes ownership of `CrawlConfig` (not a reference). It then wraps it in `Arc` immediately:

```
src/pipeline.rs:23
let fetcher = Arc::new(Fetcher::new(&config, Arc::clone(&rate_limiter))?);
```

Actually `config` is wrapped into `Arc` at the point workers share it — the pipeline is the single entry point so it is the natural place to take ownership and convert to `Arc<CrawlConfig>` for sharing.

## Common mistakes

**Cloning the inner value instead of the `Arc`**

```rust
// Wrong: deep copies the data, defeats the purpose of Arc
let config2 = (*config).clone();

// Right: cheap pointer copy, shared ownership
let config2 = Arc::clone(&config);
```

**Taking `&mut self` when `&self` + interior mutability is correct**

If a type uses `Mutex`, `RwLock`, or `DashMap` internally, its mutating methods can and should take `&self`. Using `&mut self` unnecessarily prevents concurrent callers.

**Moving out of a reference**

```rust
fn process(data: &PageData) {
    let html = data.html;  // error: cannot move out of shared reference
}
```

Fix: clone explicitly (`data.html.clone()`) or take ownership in the signature (`data: PageData`).

**String literal type confusion**

```rust
fn bad(s: String) { ... }
bad("hello");            // error: expected String, found &str
bad("hello".to_owned()); // ok — allocates
bad(String::from("hello")); // ok — same
```

`&str` and `String` are distinct types. `&str` is a borrowed slice of bytes; `String` is an owned, heap-allocated buffer.

## Quick reference

| Form | Meaning | Cost |
|---|---|---|
| `T` | Owned value | Allocation determined by T |
| `&T` | Shared borrow | Pointer-sized, no allocation |
| `&mut T` | Exclusive borrow | Pointer-sized, no allocation |
| `Box<T>` | Owned heap pointer | One allocation |
| `Arc<T>` | Shared ownership (thread-safe) | One allocation + atomic refcount |
| `Arc::clone(&arc)` | New handle to same data | Atomic increment only |
| `value.clone()` | Deep copy | Full allocation of inner data |
| `&'static str` | Borrowed string in binary | Zero allocation |
| `String` | Owned heap string | Allocation |

When to clone vs borrow:

| Scenario | Pattern |
|---|---|
| Multiple tasks share large immutable config | `Arc<T>` — clone the Arc |
| Function only reads a value | Take `&T` |
| Function needs to mutate | Take `&mut T` or owned `T` |
| Moving into `spawn` / `spawn_blocking` | `move` closure, clone Arcs first |
| Fixed string identifier (id, label) | `&'static str` — no allocation |
| Dynamic string that must outlive its source | `String` (owned) or `Arc<str>` |
