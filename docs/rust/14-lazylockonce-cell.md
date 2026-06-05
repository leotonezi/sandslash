# `LazyLock` / `once_cell`

## What it is

`std::sync::LazyLock<T>` is a standard-library synchronisation primitive that holds a value of type `T` which is initialised exactly once, on the first access, using a closure supplied at construction time. It implements `Deref<Target = T>`, so after initialisation it behaves like a plain `&T`. It is stable since Rust 1.80. Before that release, the same semantics were provided by the `once_cell` crate (`once_cell::sync::Lazy<T>`), which is functionally identical and still widely used in codebases that predate 1.80.

## Why it exists

Rust statics require `const`-evaluable values — the compiler must be able to compute them at compile time, before any code runs. Many useful types are not `const`-constructible. `scraper::Selector` is a prime example: it parses a CSS selector string, building an internal tree structure that involves heap allocation and non-trivial parsing logic. There is no way to write:

```rust
// Does not compile — Selector::parse is not const
static SEL_TITLE: Selector = Selector::parse("title").unwrap();
```

The naive alternative is to call `Selector::parse` inside every function that needs it:

```rust
fn title(html: &Html) -> Option<String> {
    let sel = Selector::parse("title").unwrap(); // allocates + parses every call
    html.select(&sel).next().map(|el| el.text().collect())
}
```

This works but wastes CPU and memory: the selector string is a compile-time constant and the parsed representation never changes across calls. `LazyLock` solves this by running the initialisation closure once, storing the result in a `static`, and returning a reference to it on every subsequent access — without requiring the value to be `const`.

## How it works under the hood

`LazyLock<T>` is internally a `UnsafeCell<LazyState<T>>` combined with a `Once` flag. `Once` is a thin wrapper over an OS primitive (futex on Linux, `pthread_once` on macOS) that guarantees a closure runs exactly once even when multiple threads race to be first. The state machine has three phases:

1. **Uninitialized** — `Once` is in its initial state; no value stored.
2. **Poisoned (initializing)** — a thread is running the closure (or it panicked midway). Other threads block on the `Once`.
3. **Ready** — the closure completed successfully; `T` is stored in the `UnsafeCell`.

On every access via `Deref`, `LazyLock` checks whether the `Once` has completed. After the first successful initialisation this check is a single atomic load with `Acquire` ordering — essentially free compared to the work the closure does. There is no lock taken on the hot path.

Memory layout: `LazyLock<T>` stores one `T` (its full size) plus one `Once` flag (8 bytes on 64-bit platforms, containing a `usize` state word). The `T` slot is uninitialised until the closure runs, achieved via `MaybeUninit<T>` inside the implementation.

Thread safety: the `Once` ensures that even if 100 threads dereference a `LazyLock` simultaneously for the first time, the closure runs on exactly one thread; the rest spin or block until it completes, then all read the now-initialised value.

`LazyLock<T>` requires `T: Send + Sync` to be itself `Sync` (safe to share across threads), which is checked by the compiler at the declaration site.

### `LazyLock` vs `OnceLock`

`OnceLock<T>` is a lower-level primitive in the same module. It has no closure — the caller must explicitly call `set(value)` to initialise it and `get()` to read it. This is useful when the initialisation value comes from runtime data (e.g., a command-line flag) rather than from a constant closure. `LazyLock` is the ergonomic wrapper: it takes the closure at construction time and calls `get_or_init` internally.

```
Use LazyLock  when: the value is always computed the same way (pure function of constants).
Use OnceLock  when: the value comes from a runtime source and is set imperatively.
```

### `once_cell::sync::Lazy<T>` — the predecessor

Before `LazyLock` was stabilised in Rust 1.80, the idiomatic solution was the `once_cell` crate:

```rust
use once_cell::sync::Lazy;

static SEL_TITLE: Lazy<Selector> =
    Lazy::new(|| Selector::parse("title").expect("invariant: valid CSS selector"));
```

The semantics are identical. `once_cell` also provides `once_cell::unsync::Lazy<T>` for single-threaded contexts. This project uses `std::sync::LazyLock` directly because the compiler version (stable, ≥1.80) supports it and `once_cell` is not a declared dependency in `Cargo.toml`.

## This project — where it appears

All CSS selector statics live at the top of `src/parser/dom.rs` (lines 1–41). Every `static` follows the same pattern:

```
src/parser/dom.rs:1–2
use scraper::{Html, Selector, node::Node};
use std::sync::LazyLock;
```

### Simple tag selector

```
src/parser/dom.rs:4–5
static SEL_TITLE: LazyLock<Selector> =
    LazyLock::new(|| Selector::parse("title").expect("invariant: valid CSS selector"));
```

Used at `dom.rs:63`:

```
src/parser/dom.rs:61–66
pub fn title(&self) -> Option<String> {
    self.html
        .select(&SEL_TITLE)
        .next()
        .map(|el| el.text().collect::<String>().trim().to_owned())
}
```

`&SEL_TITLE` auto-derefs through `LazyLock<Selector>` to `&Selector`, which is what `Html::select` expects.

### Attribute selectors

```
src/parser/dom.rs:6–8
static SEL_META_DESC: LazyLock<Selector> = LazyLock::new(|| {
    Selector::parse("meta[name='description']").expect("invariant: valid CSS selector")
});
```

```
src/parser/dom.rs:9–11
static SEL_CANONICAL: LazyLock<Selector> = LazyLock::new(|| {
    Selector::parse("link[rel='canonical']").expect("invariant: valid CSS selector")
});
```

### Bulk resource-URL extraction

`resource_urls` (lines 144–177) assembles a slice of `(&LazyLock<Selector>, &str)` tuples and iterates over it:

```
src/parser/dom.rs:145–155
let src_selectors: &[(&LazyLock<Selector>, &str)] = &[
    (&SEL_IMG, "src"),
    (&SEL_SCRIPT_SRC, "src"),
    (&SEL_IFRAME_SRC, "src"),
    (&SEL_AUDIO_SRC, "src"),
    (&SEL_VIDEO_SRC, "src"),
    (&SEL_SOURCE_SRC, "src"),
    (&SEL_TRACK_SRC, "src"),
    (&SEL_EMBED_SRC, "src"),
    (&SEL_OBJECT_DATA, "data"),
];
```

This pattern takes references to the `LazyLock` wrappers themselves. `Html::select` calls `Deref` on each entry automatically when iterating in the inner closure (line 160: `self.html.select(sel)`). All eleven selectors are initialised on the first request that exercises this code path and are reused for every subsequent page.

### Multi-selector for JS-rendered detection

```
src/parser/dom.rs:38–41
static SEL_CONTENT_TAGS: LazyLock<Selector> = LazyLock::new(|| {
    Selector::parse("p, article, section, main, li, h1, h2, h3, h4, h5, h6")
        .expect("invariant: valid CSS selector")
});
```

Used at `dom.rs:216`:

```
src/parser/dom.rs:215–217
pub fn content_tag_count(&self) -> usize {
    self.html.select(&SEL_CONTENT_TAGS).count()
}
```

### The `expect` convention

Every `.expect` message in this file follows the form `"invariant: valid CSS selector"`. The word "invariant" signals that the panic can only occur if a programmer introduced a typo in the selector literal — it is not a recoverable runtime error and not caused by user input. The selector strings are baked into the binary; if `Selector::parse` rejects one, the binary is broken and panicking immediately is the correct response. Using `.expect` instead of `.unwrap` makes the intent legible in backtraces.

### Dynamic selectors — the contrast

Not every selector in `dom.rs` is a static. `headings` (line 84) and `meta_property` / `meta_name` (lines 112–129) use runtime-computed selector strings and allocate a fresh `Selector` on each call:

```
src/parser/dom.rs:87–88
let sel = Selector::parse(&format!("h{level}"))
    .expect("invariant: h1–h6 are valid selectors");
```

```
src/parser/dom.rs:113
let sel = Selector::parse(&format!("meta[property='{key}']")).ok()?;
```

These cannot be static because the selector text is not known at compile time. The heading loop calls `Selector::parse` six times per page; for `meta_property` and `meta_name` the cost is one allocation per call. Both are uncommon hot paths compared to `title` or `canonical`, so the allocation is acceptable. If they became a bottleneck, a small `HashMap`-based cache (behind a `Mutex` or `DashMap`) could be layered on top without changing the call sites.

## Common mistakes

**Forgetting `T: Send + Sync`**

`LazyLock<T>` is only `Sync` (and therefore usable in a `static`) if `T: Send + Sync`. If you attempt to place a `!Sync` type in a `LazyLock`:

```rust
// Does not compile — RefCell is !Sync
static CACHE: LazyLock<RefCell<Vec<String>>> = LazyLock::new(|| RefCell::new(vec![]));
```

The compiler error is clear but can be surprising if you're unfamiliar with the `Sync` requirement. Use `Mutex<T>` for interior mutability inside a static.

**Panicking closure in a library**

A panic inside the `LazyLock` initialiser poisons the `Once` state, causing all subsequent accesses to also panic. This is acceptable for selector literals (programmer typo) but not for anything that could fail at runtime (e.g., reading a config file). Do not use `LazyLock` for fallible runtime initialisation — use `OnceLock` with explicit error handling instead.

**Holding a `Deref` reference across an `.await`**

`LazyLock::deref` returns a `&T` with the lifetime of the `static`. Holding it across an `.await` is fine *if* `T: Sync`, because a shared reference to a `Sync` type can be sent across threads. However, if you wrap `LazyLock` around a `!Sync` type and obtain a reference, the borrow checker will reject any attempt to hold it across an `.await`. The static CSS selectors in this project are `Sync`, so there is no issue; but be mindful of the distinction when introducing new statics.

**`LazyLock` vs initialisation in `main`**

An alternative to `LazyLock` is to build all expensive objects in `main` and pass them as function arguments or via `Arc`. This gives explicit control over when initialisation happens and avoids any lazy overhead. The trade-off: it requires threading the values through the call graph. `LazyLock` is idiomatic when the value is truly global and the initialisation moment is not important.

**Choosing `once_cell` when `std` suffices**

If your `rust-toolchain.toml` pins stable ≥1.80, reach for `std::sync::LazyLock` first — it avoids an extra dependency. Only add `once_cell` if you need `unsync::Lazy`, `unsync::OnceCell`, or the `once_cell::race` primitives, which are not in std.

## Quick reference

| Need | Tool |
|------|------|
| Process-lifetime value computed from constants, thread-safe | `std::sync::LazyLock<T>` |
| Same, on Rust < 1.80 | `once_cell::sync::Lazy<T>` |
| Value set once from runtime data (e.g., CLI flag) | `std::sync::OnceLock<T>` |
| Single-threaded lazy value | `once_cell::unsync::Lazy<T>` |
| Shared mutable state after init | `LazyLock<Mutex<T>>` or `LazyLock<RwLock<T>>` |

When to use `LazyLock` instead of `const`:

| | `const` | `LazyLock` |
|---|---|---|
| Value known at compile time | Yes | Not required |
| `const`-constructible type | Required | Not required |
| Heap allocation allowed | No | Yes |
| Initialisation cost | Zero (embedded in binary) | Once at first access |
| Example | integer, `&str`, small arrays | `Selector`, `Regex`, compiled config |

The `LazyLock` pattern at a glance:

```rust
use std::sync::LazyLock;
use scraper::Selector;

// Declared at module level — lives for the whole process.
// The closure runs exactly once, on first access, on whichever thread gets there first.
static SEL_TITLE: LazyLock<Selector> =
    LazyLock::new(|| Selector::parse("title").expect("invariant: valid CSS selector"));

// Usage — Deref coercion produces &Selector automatically.
fn title(html: &scraper::Html) -> Option<String> {
    html.select(&SEL_TITLE).next().map(|el| el.text().collect())
}
```
