# `async_trait`

## What it is

`#[async_trait]` is a procedural macro from the [`async-trait`](https://crates.io/crates/async-trait) crate that enables `async fn` methods inside trait definitions and their implementations. Without it, writing `async fn` in a trait compiles only in limited contexts and is not usable with trait objects (`dyn Trait`). The macro rewrites each `async fn` in the trait and every `impl` block into a form the Rust compiler can handle, while keeping the call-site syntax identical to a regular `async fn`.

## Why it exists

Rust does not natively support `async fn` in traits in a way that is compatible with dynamic dispatch (trait objects). The core issue is that `async fn` desugars to a function returning `impl Future<Output = T>`, and `impl Future` is an *opaque type* — a unique, anonymous type generated per implementation. This means:

```rust
// What you write:
async fn audit(&self, page: &PageData, ctx: &AuditContext) -> Vec<Finding>;

// What the compiler sees (approximately):
fn audit(&self, page: &PageData, ctx: &AuditContext) -> impl Future<Output = Vec<Finding>>;
```

Every concrete implementor returns a *different* concrete type for `impl Future`. A vtable, which is how `dyn Trait` works, requires every method slot to have a fixed, known return type. With a different opaque `Future` type per impl, the compiler cannot build a single vtable — so the trait is **not object-safe**.

The consequence: without `#[async_trait]`, you cannot write `Box<dyn SiteAuditor>` where `SiteAuditor` has any `async fn` methods, and the compiler rejects it.

Before Rust 1.75, `async fn` in traits was not stable at all for non-trivial use. As of 1.75 it is stable for static dispatch, but the object-safety limitation remains: native `async fn` in traits still produces opaque return types that cannot be placed in a vtable. `#[async_trait]` solves both the pre-1.75 stability gap and the ongoing object-safety problem.

## How it works under the hood

The macro transforms every `async fn` in the trait definition to return an explicit, boxed, type-erased future:

```rust
// Before macro expansion (what you write):
#[async_trait]
pub trait SiteAuditor: Send + Sync {
    async fn audit(&self, page: &PageData, ctx: &AuditContext) -> Vec<Finding>;
}

// After macro expansion (what the compiler sees):
pub trait SiteAuditor: Send + Sync {
    fn audit<'life0, 'life1, 'async_trait>(
        &'life0 self,
        page: &'life1 PageData,
        ctx: &'life1 AuditContext,
    ) -> Pin<Box<dyn Future<Output = Vec<Finding>> + Send + 'async_trait>>
    where
        'life0: 'async_trait,
        'life1: 'async_trait,
        Self: 'async_trait;
}
```

Every `impl` block annotated with `#[async_trait]` is rewritten to match: the `async` body is moved into a `Box::pin(async move { … })` closure, and the function returns that pinned box.

### Why `Pin<Box<dyn Future>>`?

- `Box<dyn Future>` erases the concrete future type. All impls now return the same type — a heap-allocated, type-erased future — so the compiler can build a vtable.
- `Pin` is required because futures are self-referential (they may hold references to their own stack frame). Moving them after pinning would invalidate those internal pointers. `Pin<Box<T>>` prevents moves once pinned.

### The `+ Send` bound

By default `#[async_trait]` adds `+ Send` to the returned future. This is necessary for the future to be scheduled across threads by Tokio's multi-thread runtime. Without `Send`, the future cannot be moved from one thread to another between `.await` suspension points, which would break Tokio's work-stealing scheduler.

To opt out — for single-threaded runtimes or when the future captures `!Send` data — use `#[async_trait(?Send)]`. This drops the `+ Send` bound, producing `Pin<Box<dyn Future<Output = T> + 'async_trait>>` instead.

### Allocation cost

Every call to an `async fn` defined in an `#[async_trait]` trait performs one heap allocation to construct the `Box`. This is the single cost the macro adds over native `async fn`. For this project's auditors — called once per crawled page, after a full HTTP round-trip — this allocation is completely negligible compared to the I/O latency. It would only matter in extremely hot, allocation-sensitive inner loops, which an SEO auditor's audit method is not.

## This project — where it appears

### Trait definition — `src/audit/mod.rs:29–34`

```rust
#[async_trait]
pub trait SiteAuditor: Send + Sync {
    fn id(&self) -> &'static str;
    fn category(&self) -> Category;
    async fn audit(&self, page: &PageData, ctx: &AuditContext) -> Vec<Finding>;
}
```

`SiteAuditor` is used as `Box<dyn SiteAuditor>` throughout the project (see below). Without `#[async_trait]`, `audit` returning `impl Future` would make this trait non-object-safe and the `dyn SiteAuditor` bound would not compile.

The sync methods `id` and `category` do not need the macro — only `async fn` triggers the rewrite. `#[async_trait]` is applied to the whole trait block but only transforms the `async` methods.

### Auditor registration — `src/audit/mod.rs:56–61`

```rust
pub fn site_auditors() -> Vec<Box<dyn SiteAuditor>> {
    vec![
        Box::new(robots::RobotsAuditor),
        Box::new(sitemap::SitemapAuditor),
        Box::new(links::BrokenLinksAuditor),
    ]
}
```

This is the pay-off: three different concrete types — each with its own unique future type — can all be boxed into `Box<dyn SiteAuditor>` and stored in the same `Vec` because `#[async_trait]` made the trait object-safe.

### Concrete implementations

All three site auditors follow the same pattern: `#[async_trait]` on the `impl` block, `async fn audit`, and network I/O via `ctx.fetcher` inside the body.

**`src/audit/robots.rs:143–203`**

```rust
#[async_trait]
impl SiteAuditor for RobotsAuditor {
    fn id(&self) -> &'static str { "robots" }
    fn category(&self) -> Category { Category::Crawlability }

    async fn audit(&self, page: &PageData, ctx: &AuditContext) -> Vec<Finding> {
        // fetches /robots.txt and checks for disallow-all and missing Sitemap: directive
        let fetched = match ctx.fetcher.fetch(&robots_url).await { ... };
        ...
    }
}
```

**`src/audit/sitemap.rs:129–241`**

```rust
#[async_trait]
impl SiteAuditor for SitemapAuditor {
    async fn audit(&self, page: &PageData, ctx: &AuditContext) -> Vec<Finding> {
        // fetches robots.txt, resolves sitemap URL, validates XML, optionally probes URLs
        ...
    }
}
```

**`src/audit/links.rs:94–174`**

```rust
#[async_trait]
impl SiteAuditor for BrokenLinksAuditor {
    async fn audit(&self, page: &PageData, ctx: &AuditContext) -> Vec<Finding> {
        // extracts hrefs, partitions internal/external, probes each URL concurrently
        ...
    }
}
```

### Call site — `src/crawler/engine.rs:298–301`

```rust
for auditor in site_auditors.iter() {
    let mut f = auditor.audit(&page_data, &ctx).await;
    findings.append(&mut f);
}
```

Here `site_auditors` is `Arc<Vec<Box<dyn SiteAuditor>>>`. The `.await` on the trait-object call is what triggers the allocation and polls the boxed future to completion. The `+ Send` bound on the future means the future can be safely polled by any thread in Tokio's multi-thread pool between suspension points.

### Dependency — `Cargo.toml:20`

```toml
async-trait = "0.1"
```

## Common mistakes

**Holding a `!Send` value across an `.await` inside an `#[async_trait]` method**

`#[async_trait]` adds `+ Send` to the future by default. If you hold a value that is `!Send` — like a raw pointer, `Rc`, or `scraper::Html` — across an `.await` point, the compiler rejects the `impl` block with a `Send` error. The fix is to drop the `!Send` value *before* the `.await`, or to refactor the work into a `spawn_blocking` closure.

In `src/audit/links.rs:48–64`, `scraper::Html` (which is `!Send`) is handled by wrapping the parsing in `tokio::task::spawn_blocking`, which runs it in a separate blocking thread and returns a `Send`-able result:

```rust
async fn extract_hrefs(html: String) -> Vec<String> {
    tokio::task::spawn_blocking(move || {
        let doc = Html::parse_document(&html);
        // Html is !Send but never crosses an .await boundary here
        ...
    }).await.unwrap_or_default()
}
```

**Forgetting `#[async_trait]` on the `impl` block**

If you annotate the trait but not the `impl`, the trait signature (expecting `Pin<Box<dyn Future>>`) and the impl signature (`async fn`, which returns `impl Future`) mismatch. The compiler error looks like a type mismatch on the return type.

Always annotate both:

```rust
#[async_trait]          // on the trait
pub trait SiteAuditor { ... }

#[async_trait]          // also on every impl
impl SiteAuditor for RobotsAuditor { ... }
```

**Using `#[async_trait]` when you don't need dynamic dispatch**

If your trait is never used as `dyn Trait` — only with generics — you may not need `#[async_trait]` at all. On Rust 1.75+ you can write `async fn` in a trait and use it via static dispatch without any macro. The macro's only benefit is the type erasure that enables `dyn Trait`. Adding it unnecessarily introduces one heap allocation per call for no gain.

**Assuming `#[async_trait(?Send)]` is always safer**

`?Send` drops the `Send` requirement on the returned future. This prevents the future from being scheduled across threads by Tokio's multi-thread runtime. Most async applications — including this one — run on Tokio's multi-thread pool, so dropping `+ Send` would make the futures incompatible with `tokio::spawn`. Only use `?Send` when you are certain the trait will only ever be used in a single-threaded runtime or called directly without spawning.

**Lifetime errors with borrowed arguments**

`#[async_trait]` introduces explicit lifetime parameters (`'life0`, `'life1`, `'async_trait`) and adds bounds like `'life0: 'async_trait`. Most of the time the macro handles this transparently, but if you write helper functions that take `&self` references and are called inside an `async fn`, the borrow checker may surface these synthetic lifetimes in error messages. The fix is usually to clone or `Arc`-wrap the data rather than hold a raw reference across an `.await`.

## Quick reference

| Situation | What to do |
|---|---|
| `async fn` in a trait used as `dyn Trait` | `#[async_trait]` on both the trait and every `impl` |
| `async fn` in a trait, static dispatch only (no `dyn`) | On Rust 1.75+: native `async fn` in traits, no macro needed |
| Future must cross thread boundaries (Tokio multi-thread) | Default `#[async_trait]` — adds `+ Send` automatically |
| Future captures `!Send` data (e.g. `Rc`, raw pointer) | `#[async_trait(?Send)]` — drops `+ Send` from the future |
| `!Send` type needed only during sync work inside an `async fn` | Drop it before the next `.await`, or wrap in `spawn_blocking` |
| One heap allocation per async trait call is too expensive | Redesign to avoid `dyn Trait`; use an enum or generics instead |

Desugaring summary:

```
async fn audit(&self, ...) -> T
    ↓  #[async_trait] rewrites to
fn audit(&self, ...) -> Pin<Box<dyn Future<Output = T> + Send + '_>>
    ↓  each call site allocates
Box::pin(async move { /* original body */ })
```
