# Error Handling

## What it is

Rust error handling is built on two standard library types: `Result<T, E>` (a value that is either `Ok(T)` or `Err(E)`) and the `?` operator, which desugars to an early return plus a `From` conversion. Two crates layer ergonomics on top of this foundation:

- **`thiserror`** — a derive macro for library code that generates `std::error::Error`, `Display`, and `From` implementations from annotations on an enum.
- **`anyhow`** — a type-erased error wrapper for binary/application code that accepts any `std::error::Error` and adds `.context()` for layered messages.

The project pin versions are `thiserror = "2"` and `anyhow = "1"` (see `Cargo.toml` lines 18–19).

## Why it exists

Without `thiserror`, writing a custom error type means manually implementing `std::error::Error`, `fmt::Display`, and one `From<ForeignError>` impl per wrapped type — dozens of lines of boilerplate per enum. `thiserror` generates all of that from a single annotation.

Without `anyhow`, binary code that calls multiple library functions with different error types must either:

1. Map every error to a common type manually, or
2. Return `Box<dyn std::error::Error>` — which loses the concrete type and makes downcasting awkward.

`anyhow::Error` accepts any `std::error::Error`, chains contexts cleanly, and lets you downcast back to the original concrete type when needed (as the test at `src/pipeline.rs:303` demonstrates).

The split between the two crates exists for a specific reason: library code that returns `anyhow::Error` is impossible for callers to pattern-match on. Callers cannot write `Err(MyLibError::NotFound)` — they get an opaque blob. `thiserror` keeps the error type part of the public API; `anyhow` is appropriate only where the caller is the end user or the binary itself.

## How it works under the hood

### `?` desugaring

```rust
let x = some_fn()?;
```

expands to approximately:

```rust
let x = match some_fn() {
    Ok(v) => v,
    Err(e) => return Err(From::from(e)),
};
```

`From::from(e)` is the key — it invokes whatever `From<SourceError>` impl is in scope. If the current function returns `Result<T, SeoError>` and `some_fn()` returns `Result<T, url::ParseError>`, then `SeoError: From<url::ParseError>` must exist. `#[from]` on an enum variant auto-generates that impl.

### `#[from]` on a tuple variant

```rust
#[error("URL parse error: {0}")]
Url(#[from] url::ParseError),
```

`thiserror` generates:

```rust
impl From<url::ParseError> for SeoError {
    fn from(e: url::ParseError) -> Self {
        SeoError::Url(e)
    }
}
```

Now `?` on any expression returning `url::ParseError` inside a function returning `Result<_, SeoError>` compiles automatically.

### `#[source]` on a named-field variant

```rust
Fetch {
    url: String,
    #[source]
    source: reqwest::Error,
},
```

`#[source]` (without `#[from]`) does two things:

1. Implements `std::error::Error::source()` to return `Some(&self.source)` — callers can traverse the error chain.
2. Does **not** generate a `From` impl — the caller must construct `SeoError::Fetch { url, source: e }` manually.

This is intentional for `Fetch`: the URL context is required to build the error, so an automatic `From<reqwest::Error>` conversion cannot exist — the URL is not part of `reqwest::Error`.

### `anyhow::Error` internals

`anyhow::Error` is a heap-allocated fat pointer wrapping any `dyn std::error::Error + Send + Sync + 'static`. Because it stores the original concrete type, `.downcast::<SeoError>()` works at runtime (as in `src/pipeline.rs:303`). `.context("message")` prepends a new message layer without losing the original error in the chain.

`anyhow::Result<T>` is an alias for `Result<T, anyhow::Error>`. The `?` operator converts any `E: Into<anyhow::Error>` automatically — which covers all types implementing `std::error::Error`.

### `pub type Result<T>` alias

```rust
// src/error.rs:28
pub type Result<T> = std::result::Result<T, SeoError>;
```

All library modules import `crate::error::Result` and write `Result<T>` instead of `Result<T, SeoError>`. The alias is purely ergonomic — no runtime effect.

## This project — where it appears

### The error enum — `src/error.rs:1–28`

The entire project's library error surface:

```rust
use thiserror::Error;

#[derive(Error, Debug)]
pub enum SeoError {
    #[error("HTTP error fetching {url}: {source}")]
    Fetch {
        url: String,
        #[source]
        source: reqwest::Error,
    },
    #[error("URL parse error: {0}")]
    Url(#[from] url::ParseError),
    #[error("Redis error: {0}")]
    Redis(#[from] redis::RedisError),
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    // ...
}

pub type Result<T> = std::result::Result<T, SeoError>;
```

- `Url`, `Redis`, and `Io` use `#[from]` — `?` converts foreign errors automatically.
- `Fetch` uses `#[source]` only — requires manual construction because the `url: String` context field must be supplied by the caller.
- `Parse`, `Config`, `RedirectLoop`, and `RobotsDisallowed` carry no foreign error at all — they are constructed directly.

### Manual `Fetch` construction — `src/fetcher/client.rs:101–103`

```rust
.build()
.map_err(|e| SeoError::Fetch {
    url: config.root.to_string(),
    source: e,
})?;
```

Because `#[from]` is absent on `Fetch`, `.map_err` threads the URL context in before `?` fires.

### `#[from]` letting `?` convert automatically — `src/fetcher/client.rs:182`

```rust
let next = current.join(loc_str).map_err(SeoError::from)?;
```

`current.join(loc_str)` returns `Result<Url, url::ParseError>`. `SeoError::from` is the `From` impl generated by `#[from]` on `SeoError::Url`. The `.map_err(SeoError::from)?` pattern is equivalent to plain `?` here — both invoke `From::from`.

### `anyhow::Result` at the binary boundary — `src/pipeline.rs:19`

```rust
pub async fn run(config: CrawlConfig) -> anyhow::Result<AuditReport> {
```

`run` is called from `main.rs` — the binary boundary. It bridges library errors into `anyhow` via `.into()` or `?` (which calls `From<SeoError> for anyhow::Error`, a blanket impl provided by `anyhow`).

### Mixing `SeoError` and `anyhow` in the same function — `src/pipeline.rs:47–52`

```rust
let page_data = match fetcher.fetch(&config.root).await {
    Ok(pd) => pd,
    Err(SeoError::RedirectLoop { url, hops }) => {
        return handle_redirect_loop(url, hops, &config);
    }
    Err(e) => return Err(e.into()),
};
```

The function returns `anyhow::Result<AuditReport>`. `fetcher.fetch` returns `crate::error::Result<PageData>` (i.e. `Result<PageData, SeoError>`). The `match` arm pattern-matches on the concrete `SeoError` variant first — this is only possible because the library code used `thiserror`, not `anyhow`. Unknown errors go through `.into()`, which wraps them in `anyhow::Error`.

### Downcasting `anyhow::Error` back to `SeoError` — `src/pipeline.rs:299–306`

```rust
let seo_err = err
    .downcast::<SeoError>()
    .expect("error must downcast to SeoError");
assert!(matches!(seo_err, SeoError::Config(_)), ...);
```

This test verifies that `SeoError::Config` propagates correctly through `anyhow`. Downcasting works because `anyhow` stores the original concrete type internally. If the library had returned `anyhow::Error` directly, this test could not pattern-match on `SeoError::Config(_)`.

### Error propagation in the crawler — `src/crawler/engine.rs:133`

```rust
pub async fn run_crawl(...) -> Result<Vec<PageReport>> {
```

The return type is `crate::error::Result` — the library alias. Inside `worker_loop` (lines 178–350), individual page errors are logged and swallowed rather than propagated:

```rust
Err(e) => {
    tracing::warn!(url = %url, error = %e, "fetch error; skipping page");
    mark_done_warn(&frontier).await;
    continue;
}
```

This is a deliberate design choice: a single page fetch failure should not abort the entire crawl. Only frontier-level errors (Redis connectivity) propagate upward via `?`, since those affect the whole job.

## Common mistakes

**Using `anyhow` in library code**

```rust
// src/some_auditor.rs — WRONG
pub fn audit(&self, page: &PageData) -> anyhow::Result<Vec<Finding>> { ... }
```

Callers cannot pattern-match on `anyhow::Error`. If the caller needs to handle specific error cases differently, the concrete type information is gone. Use `SeoError` (or a module-specific `thiserror` enum) in all `src/**/*.rs` except `main.rs`.

**Omitting `#[from]` and forgetting `.map_err`**

```rust
// WRONG — url::ParseError does not auto-convert to SeoError without #[from]
// if you removed #[from] from the Url variant
let url = Url::parse(s)?;  // compile error: mismatched types
```

If you add a new foreign error to `SeoError` without `#[from]`, every `?` usage on that error type requires an explicit `.map_err(SeoError::TheVariant)`.

**Expecting `#[source]` to generate `From`**

`#[source]` only wires up `Error::source()` for chain traversal. It does not create a `From` impl. If you want both automatic conversion and chain traversal, use `#[from]` — it implies `#[source]`.

**Dropping context before converting to `anyhow`**

```rust
// Loses the URL that caused the error
return Err(anyhow::anyhow!("fetch failed"));
```

Prefer propagating the typed `SeoError` (which already carries context in its fields) and letting `?` convert it to `anyhow::Error` at the boundary. If additional context is needed at the boundary, use `.context("doing X")`:

```rust
fetcher.fetch(&url).await.context("fetching root page")?;
```

**Pattern-matching on `anyhow::Error` directly**

```rust
// Does not compile — anyhow::Error has no variants
match err {
    SeoError::Config(_) => ...  // error
}
```

To match variants from an `anyhow::Error`, downcast first: `err.downcast::<SeoError>()`.

**Holding a `SeoError` across `.await` in async code**

`SeoError` variants that contain `reqwest::Error` (the `Fetch` variant) or other non-`Send` types would make the future `!Send`. All `SeoError` variants in this project hold only `Send` types, so `Result<T, SeoError>` is `Send` and safe to use across `.await` points.

## Quick reference

| Scenario | Use |
|---|---|
| Library module returning errors | `crate::error::Result<T>` (alias for `Result<T, SeoError>`) |
| Binary boundary / `main.rs` | `anyhow::Result<T>` |
| Auto-convert a foreign error via `?` | `#[from]` on the enum variant |
| Attach context to a foreign error | `#[source]` (no `From`), construct manually with `.map_err` |
| Add message context at the call site | `.context("doing X")?` (anyhow only) |
| Pattern-match on variant after `anyhow` | `err.downcast::<SeoError>()` |
| Propagate only fatal errors, skip page errors | `match result { Err(e) => { tracing::warn!(...); continue; } }` |

`#[from]` vs `#[source]` at a glance:

| Attribute | Generates `From` impl | Wires `Error::source()` | Requires manual construction |
|---|---|---|---|
| `#[from]` | Yes | Yes (implies `#[source]`) | No |
| `#[source]` | No | Yes | Yes |
| Neither | No | No | Yes |
