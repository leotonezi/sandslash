# seo-rs — Detailed Implementation Plan

A phase-by-phase, session-sized breakdown. Each micro-step fits in one focused coding session and ends with a concrete verification step. Rust-specific learning targets flagged **[RUST]**; gotchas flagged **[GOTCHA]**.

---

## ✓ Phase 0 — Scaffolding (DONE)

Goal: `cargo run -- https://example.com` parses args, builds config, emits structured logs. No HTTP yet.

### ✓ 0.1 Cargo project + dependency manifest
- **Files**: `Cargo.toml`, `src/main.rs` (stub), `.gitignore`, `rust-toolchain.toml`
- Actions:
  1. `cargo init --name seo-rs` (binary crate).
  2. Pin toolchain: `rust-toolchain.toml` with `channel = "stable"`.
  3. Add ALL deps upfront — MVP in `[dependencies]`, dev-only in `[dev-dependencies]`:
     - `tokio = { version = "1", features = ["full"] }`
     - `reqwest = { version = "0.12", default-features = false, features = ["rustls-tls", "gzip", "brotli", "stream"] }`
     - `scraper = "0.20"`
     - `url = { version = "2", features = ["serde"] }`
     - `redis = { version = "0.27", features = ["tokio-comp", "connection-manager"] }`
     - `governor = "0.7"`
     - `clap = { version = "4", features = ["derive", "env"] }`
     - `serde = { version = "1", features = ["derive"] }`
     - `serde_json = "1"`
     - `thiserror = "1"`
     - `anyhow = "1"`
     - `async-trait = "0.1"`
     - `encoding_rs = "0.8"`
     - `owo-colors = "4"`
     - `comfy-table = "7"`
     - `indicatif = "0.17"`
     - `tracing = "0.1"`
     - `tracing-subscriber = { version = "0.3", features = ["env-filter", "fmt"] }`
     - `dashmap = "6"`
     - `chrono = { version = "0.4", features = ["serde"] }`
     - `quick-xml = "0.36"`
     - dev: `wiremock = "0.6"`, `pretty_assertions = "1"`, `tempfile = "3"`, `insta = "1"`
  4. Add `[profile.dev.package."*"] opt-level = 1` (deps compile optimized, your code stays debuggable).
  5. Add `[lints.rust]` section: `unsafe_code = "forbid"`, `unused_must_use = "deny"`.
- **[RUST]** Feature flags, `default-features = false`, why `rustls-tls` (no OpenSSL system dep).
- **[GOTCHA]** `reqwest` defaults to OpenSSL — always use `rustls-tls` for portability.
- Verify: `cargo build` succeeds. `cargo tree | head -50` sanity-check.

### ✓ 0.2 Error type (`src/error.rs`)
- **Files**: `src/error.rs`, register `mod error;` in `main.rs`.
- Define:
  ```rust
  #[derive(thiserror::Error, Debug)]
  pub enum SeoError {
      #[error("HTTP error fetching {url}: {source}")]
      Fetch { url: String, #[source] source: reqwest::Error },
      #[error("URL parse error: {0}")]
      Url(#[from] url::ParseError),
      #[error("HTML parse error: {0}")]
      Parse(String),
      #[error("Redis error: {0}")]
      Redis(#[from] redis::RedisError),
      #[error("IO error: {0}")]
      Io(#[from] std::io::Error),
      #[error("Config error: {0}")]
      Config(String),
      #[error("Redirect loop detected at {url} after {hops} hops")]
      RedirectLoop { url: String, hops: usize },
      #[error("Robots.txt disallows {0}")]
      RobotsDisallowed(String),
  }
  pub type Result<T> = std::result::Result<T, SeoError>;
  ```
- **[RUST]** `#[from]` = auto-convert via `?` (no context). `#[source]` = manual wrapping, preserves cause chain. Library code uses `thiserror`; binary boundary uses `anyhow`.
- **[GOTCHA]** Don't `#[from] reqwest::Error` directly — you lose the URL context. Use a struct variant with `#[source]`.
- Verify: `fn _check() -> Result<()> { Err(SeoError::Config("x".into())) }` + `cargo check`.

### ✓ 0.3 Model types (`src/model.rs`)
- **Files**: `src/model.rs`, register in `main.rs`.
- Define:
  ```rust
  use url::Url;
  use serde::{Serialize, Deserialize};
  use std::collections::HashMap;

  pub type Headers = HashMap<String, String>;

  #[derive(Debug, Clone, Serialize)]
  pub struct PageData {
      pub url: Url,
      pub status: u16,
      pub redirect_chain: Vec<Url>,
      pub html: String,
      pub headers: Headers,
      pub depth: u32,
  }

  #[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
  pub enum Severity { Critical, Warning, Info }

  #[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
  pub enum Category { Metadata, SocialTags, Structure, Links, Media, Crawlability, Security }

  impl Category {
      pub fn all() -> impl Iterator<Item = Self> { /* slice + .iter().copied() */ }
      pub fn weight(self) -> u8 { /* 20/20/15/15/15/10/5 */ }
  }

  #[derive(Debug, Clone, Serialize)]
  pub struct Finding {
      pub check_id: &'static str,
      pub category: Category,
      pub severity: Severity,
      pub message: String,
      pub penalty: u8,
  }

  #[derive(Debug, Clone, Serialize)]
  pub struct PageReport {
      pub url: Url,
      pub findings: Vec<Finding>,
      pub category_scores: HashMap<Category, u8>,
      pub score: u8,
  }

  #[derive(Debug, Clone, Serialize)]
  pub struct AuditReport {
      pub root: Url,
      pub pages: Vec<PageReport>,
      pub site_score: u8,
      pub crawled_at: String,
  }
  ```
- **[RUST]** `Category` is `Copy` (small enum — ergonomic pattern matching without partial moves). `#[derive(Hash, Eq)]` required for `HashMap` key. `&'static str` for `check_id` = string interning, zero allocation. `Serialize`-only (no need to deserialize output reports).
- **[GOTCHA]** `Url` already implements `Serialize` via the `serde` feature. Verify with a tiny unit test serializing a `Url` to JSON.
- Verify: `#[test] fn category_weights_sum_to_100()` — assert sum == 100.

### ✓ 0.4 Config (`src/config.rs`)
- **Files**: `src/config.rs`.
- Define:
  ```rust
  #[derive(Debug, Clone)]
  pub struct CrawlConfig {
      pub root: Url,
      pub depth: u32,
      pub concurrency: usize,
      pub rate_per_host: u32,
      pub redis_url: Option<String>,
      pub user_agent: String,
      pub timeout_secs: u64,
      pub max_pages: Option<usize>,
      pub respect_robots: bool,
      pub quiet: bool,
      pub no_color: bool,
      pub output_json: Option<std::path::PathBuf>,
  }
  impl CrawlConfig {
      pub const DEFAULT_UA: &'static str =
          concat!("seo-rs/", env!("CARGO_PKG_VERSION"), " (+https://github.com/you/seo-rs)");
  }
  ```
- **[RUST]** `concat!` and `env!` are compile-time macros. Config is `Clone` but not `Copy` (contains `String`).
- Verify: trivial `#[test]` constructing the struct.

### ✓ 0.5 CLI (`src/cli.rs`) — clap derive
- **Files**: `src/cli.rs`.
- Define:
  ```rust
  #[derive(clap::Parser, Debug)]
  #[command(name = "seo-rs", version, about = "SEO audit CLI")]
  pub struct Cli {
      pub url: String,
      #[arg(short, long, default_value_t = 1)]
      pub depth: u32,
      #[arg(short = 'c', long, default_value_t = 8)]
      pub concurrency: usize,
      #[arg(long, default_value_t = 2)]
      pub rate: u32,
      #[arg(long, env = "REDIS_URL")]
      pub redis_url: Option<String>,
      #[arg(long)]
      pub user_agent: Option<String>,
      #[arg(long, default_value_t = 30)]
      pub timeout: u64,
      #[arg(long)]
      pub max_pages: Option<usize>,
      #[arg(long, default_value_t = false)]
      pub ignore_robots: bool,
      #[arg(short, long)]
      pub quiet: bool,
      #[arg(long)]
      pub no_color: bool,
      #[arg(short = 'o', long)]
      pub output: Option<std::path::PathBuf>,
  }
  impl Cli {
      pub fn into_config(self) -> crate::Result<CrawlConfig> { /* parse url, fill defaults */ }
  }
  ```
- **[RUST]** Clap derive vs builder. `env = "..."` for env-var fallback. Consuming `self` in `into_config` avoids clones.
- Verify: `cargo run -- --help` shows usage; `cargo run -- https://example.com -d 2` prints config via `tracing::info!`.

### ✓ 0.6 Logging + main wiring (`src/main.rs`)
- **Files**: `src/main.rs`.
- Wire up:
  ```rust
  mod cli; mod config; mod error; mod model;
  use anyhow::Context;

  #[tokio::main(flavor = "multi_thread")]
  async fn main() -> anyhow::Result<()> {
      tracing_subscriber::fmt()
          .with_env_filter(tracing_subscriber::EnvFilter::try_from_default_env()
              .unwrap_or_else(|_| "seo_rs=info".into()))
          .init();
      let cli = <cli::Cli as clap::Parser>::parse();
      let config = cli.into_config().context("invalid configuration")?;
      tracing::info!(?config, "starting audit");
      Ok(())
  }
  ```
- **[RUST]** `#[tokio::main]` macro expansion. `anyhow::Result<()>` at binary boundary. `Context::context` for error annotation. `?config` uses Debug; `%config` uses Display.
- **[GOTCHA]** `tracing::info!(?config, ...)` requires `Debug` impl.
- Verify: `RUST_LOG=seo_rs=debug cargo run -- https://example.com` prints structured log with full config.

**Phase 0 exit criteria**: compiles, `--help` works, config prints via tracing, `cargo test` green.

---

## ✓ Phase 1 — MVP: one URL, three pure auditors, JSON output (DONE)

Goal: `seo-rs https://example.com` fetches, parses, runs metadata/headings/https audits, emits JSON.

### ✓ 1.1 Minimal HTTP fetcher (`src/fetcher/mod.rs`, `src/fetcher/client.rs`)
- **Files**: `src/fetcher/mod.rs` (re-exports), `src/fetcher/client.rs`.
- Build:
  ```rust
  pub struct Fetcher { client: reqwest::Client, user_agent: String }
  impl Fetcher {
      pub fn new(config: &CrawlConfig) -> Result<Self> {
          let client = reqwest::Client::builder()
              .user_agent(&config.user_agent)
              .timeout(std::time::Duration::from_secs(config.timeout_secs))
              .redirect(reqwest::redirect::Policy::limited(10))  // Phase 2 replaces with manual
              .build()?;
          Ok(Self { client, user_agent: config.user_agent.clone() })
      }
      pub async fn fetch(&self, url: &Url) -> Result<PageData> { /* ... */ }
  }
  ```
- For Phase 1: let reqwest follow redirects automatically. `redirect_chain` = empty vec.
- Capture `status`, `headers` (convert `HeaderMap` -> `HashMap<String, String>` lowercasing keys), `html` (`.text().await` — UTF-8 only for now).
- **[RUST]** `reqwest::Client` is `Arc` internally — cheap to clone. `async fn` in inherent impls.
- **[GOTCHA]** `.text()` mangles non-UTF-8. Log warning if `Content-Type` charset isn't UTF-8, continue. Fix in Phase 4.
- Verify: unit test via `wiremock` — mock server returns fixed HTML, assert `Fetcher::fetch` returns expected `PageData`.

### ✓ 1.2 DOM parser (`src/parser/mod.rs`, `src/parser/dom.rs`)
- **Files**: `src/parser/mod.rs`, `src/parser/dom.rs`.
- Build:
  ```rust
  pub struct Dom { pub html: scraper::Html }
  impl Dom {
      pub fn parse(html: &str) -> Self { Self { html: scraper::Html::parse_document(html) } }
      pub fn title(&self) -> Option<String> { /* select("title") */ }
      pub fn meta_description(&self) -> Option<String> { /* meta[name=description] content */ }
      pub fn canonical(&self) -> Option<String> { /* link[rel=canonical] href */ }
      pub fn headings(&self) -> Vec<(u8, String)> { /* h1..h6 with text content */ }
      pub fn images(&self) -> Vec<ImgInfo> { /* src, alt presence */ }
      pub fn meta_property(&self, key: &str) -> Option<String> { /* meta[property=key] content */ }
      pub fn links(&self) -> Vec<String> { /* all a[href] raw strings */ }
  }
  pub struct ImgInfo { pub src: String, pub alt: Option<String> }
  ```
- Pre-compile selectors as `std::sync::LazyLock<scraper::Selector>` statics (Rust 1.80+) or `once_cell::sync::Lazy`.
- **[RUST]** `scraper::Html` is `!Send` (underlying `Node` uses `Rc`). Lives on one task; cannot cross `.await`. Confine to synchronous functions.
- **[GOTCHA]** The `!Send` issue WILL bite in Phase 3. Don't hold `Dom` across any `.await` point.
- Verify: unit tests on fixture HTML strings in `tests/fixtures/`. Test every accessor.

### ✓ 1.3 Audit trait + registry (`src/audit/mod.rs`)
- **Files**: `src/audit/mod.rs`.
- Define both traits:
  ```rust
  pub trait PageAuditor: Send + Sync {
      fn id(&self) -> &'static str;
      fn category(&self) -> Category;
      fn audit(&self, page: &PageData, dom: &Dom) -> Vec<Finding>;
  }

  #[async_trait::async_trait]
  pub trait SiteAuditor: Send + Sync {
      fn id(&self) -> &'static str;
      fn category(&self) -> Category;
      async fn audit(&self, page: &PageData, ctx: &AuditContext) -> Vec<Finding>;
  }

  pub struct AuditContext<'a> {
      pub config: &'a CrawlConfig,
      pub fetcher: &'a crate::fetcher::Fetcher,
  }

  pub fn page_auditors() -> Vec<Box<dyn PageAuditor>> {
      vec![
          Box::new(super::metadata::MetadataAuditor),
          Box::new(super::headings::HeadingsAuditor),
          Box::new(super::https::HttpsAuditor),
      ]
  }
  ```
- **[RUST]** `Box<dyn Trait + Send + Sync>` for heterogeneous collections. Trait object safety: no generic methods, no `Self` by value in return. Zero-sized structs (`pub struct MetadataAuditor;`) are idiomatic for stateless auditors.
- Create stub modules `metadata.rs`, `headings.rs`, `https.rs` returning `vec![]`.
- Verify: `cargo check` green with stubs.

### ✓ 1.4 Metadata auditor (`src/audit/metadata.rs`)
- Checks: title missing/empty/short(<30)/long(>60); description missing/short(<50)/long(>160); canonical missing/off-host/self-mismatch.
- Pattern:
  ```rust
  pub struct MetadataAuditor;
  impl PageAuditor for MetadataAuditor {
      fn id(&self) -> &'static str { "metadata" }
      fn category(&self) -> Category { Category::Metadata }
      fn audit(&self, page: &PageData, dom: &Dom) -> Vec<Finding> {
          let mut out = Vec::new();
          match dom.title() {
              None => out.push(finding("title.missing", Severity::Critical, 30, "title tag missing")),
              Some(t) if t.trim().is_empty() => out.push(...),
              Some(t) if t.chars().count() < 30 => out.push(...),
              Some(t) if t.chars().count() > 60 => out.push(...),
              _ => {}
          }
          out
      }
  }
  ```
- **[RUST]** Match guards (`Some(t) if t.len() < 30`), exhaustive matching.
- **[GOTCHA]** `.len()` is bytes, not chars. Use `.chars().count()` for Unicode. Document limitation.
- Verify: unit test per branch (missing, short, long, ok) with fixture HTML.

### ✓ 1.5 Headings + HTTPS auditors
- **Files**: `src/audit/headings.rs`, `src/audit/https.rs`.
- Headings: count h1 (0=Critical, >1=Warning), detect skipped levels, flag empty headings.
- HTTPS: check `page.url` scheme; scan `<img>`, `<script>`, `<link>` srcs for `http://` on https pages (mixed content, no network).
- **[RUST]** Iterator pattern: `windows(2)` over heading levels to detect skips.
- Verify: unit tests per case.

### ✓ 1.6 Scoring (`src/score/mod.rs`)
- Implement `score_page` from plan. Add:
  ```rust
  pub fn score_site(reports: &[PageReport]) -> u8 {
      if reports.is_empty() { return 0; }
      (reports.iter().map(|p| p.score as u32).sum::<u32>() / reports.len() as u32) as u8
  }
  ```
- **[RUST]** `HashMap::from_iter`, `Iterator::sum`, integer-cast pitfalls (`as u8` truncates — clamp first).
- Verify: zero findings → score 100; one Critical penalty-30 Metadata finding → expected computed score.

### ✓ 1.7 JSON reporter (`src/report/mod.rs`, `src/report/json.rs`)
- `pub fn write_json<W: std::io::Write>(report: &AuditReport, w: W) -> Result<()>`
- Use `serde_json::to_writer_pretty`. Use `chrono::Utc::now().to_rfc3339()` for `crawled_at`.
- **[RUST]** `Write` trait (sync), generic functions vs trait objects.
- Verify: unit test serializing a tiny `AuditReport`, assert expected JSON keys exist.

### ✓ 1.8 Single-page pipeline (`src/pipeline.rs` or `src/lib.rs`)
- Expose `pub async fn run(config: CrawlConfig) -> anyhow::Result<AuditReport>`.
- Sequence: build Fetcher → fetch root → parse Dom → run page_auditors → score_page → wrap in AuditReport → write JSON.
- Update `main.rs` to call `run(config).await`.
- **[RUST]** Use sync `std::io::stdout` for JSON writer (serde_json is sync; no await inside). If `--output` set, write to file; otherwise to stdout.
- Verify: end-to-end test against wiremock server — assert JSON output structure.

**Phase 1 exit criteria**: `cargo run -- https://example.com -o report.json` produces valid JSON with one page and three auditors' findings.

---

## ✓ Phase 2 — Full single-page auditor + colored terminal report (DONE)

Goal: all single-page auditors, polished scoring, colored terminal output, robots/sitemap for root URL.

### ✓ 2.1 OpenGraph + Twitter auditor (`src/audit/opengraph.rs`)
- Check presence of `og:title`, `og:description`, `og:image`, `og:url`, `twitter:card`.
- One finding per missing tag; all in `Category::SocialTags`.
- Verify: fixture-based unit tests.

### ✓ 2.2 Images auditor (`src/audit/images.rs`)
- Check each `<img>` has `alt`. Rules: missing `alt` attr = Warning; empty `alt=""` on content images = Info. Skip data URIs.
- Verify: fixture tests including svg/data-uri/no-alt cases.

### ✓ 2.3-ui Next.js audit UI
- **Files**: `frontend/` (Next.js 14, App Router, TypeScript)
- URL input + Run Audit button → POSTs to `/api/audit` → shells out to binary via temp file → renders AuditReport
- Binary path: `SEO_RS_BIN` env (default `../target/release/seo-rs`), temp file for JSON output
- Verify: `cd frontend && npm install && npm run build && npm run lint` passes; with binary built, `npm run dev` on :3000 and submitting https://example.com returns rendered report with site_score and page blocks

### ✓ 2.3 Manual redirect handling (REFACTOR `src/fetcher/client.rs`)
- Switch `Policy::limited(10)` to `Policy::none()`.
- Implement manual follow loop:
  ```rust
  async fn fetch_with_chain(&self, url: &Url) -> Result<PageData> {
      let mut chain = Vec::new();
      let mut current = url.clone();
      let mut seen = std::collections::HashSet::new();
      for hop in 0..=10 {
          if !seen.insert(current.clone()) {
              return Err(SeoError::RedirectLoop { url: current.to_string(), hops: hop });
          }
          let resp = self.client.get(current.clone()).send().await.map_err(...)?;
          let status = resp.status().as_u16();
          if (300..400).contains(&status) {
              chain.push(current.clone());
              let loc = resp.headers().get(reqwest::header::LOCATION)
                  .and_then(|h| h.to_str().ok())
                  .ok_or_else(|| SeoError::Parse("redirect without Location".into()))?;
              current = current.join(loc)?;
              continue;
          }
          return Ok(PageData { url: current, status, redirect_chain: chain, ... });
      }
      Err(SeoError::RedirectLoop { url: current.to_string(), hops: 10 })
  }
  ```
- **[RUST]** Explicit loops vs `while let`, `HashSet` for visited detection, `Url` clone cost.
- **[GOTCHA]** `Location` headers can be relative (`/foo`) — resolve via `Url::join`.
- **[GOTCHA]** Non-ASCII bytes in `Location` → `to_str()` fails. Decide: fail or skip.
- Verify: wiremock 3-hop chain test; self-redirect loop test.

### ✓ 2.4 Redirects auditor (`src/audit/redirects.rs`)
- chain length > 3 = Warning; > 5 = Critical.
- `SeoError::RedirectLoop` caught at pipeline level → synthetic `PageData` + Critical finding.
- Verify: unit test on synthetic `PageData { redirect_chain: vec![...] }`.

### ✓ 2.5 Mixed content extension (`src/audit/https.rs`)
- Flag any `http://` resource referenced from an `https://` page.
- Extend existing `HttpsAuditor`.
- Verify: fixture test with mixed-content srcs.

### ✓ 2.6 Robots auditor (`src/audit/robots.rs`) — first SiteAuditor
- Fetch `{root.origin()}/robots.txt`. Findings: 404 = Warning; `Disallow: /` for `*` = Critical; no `Sitemap:` = Info.
- Parse manually — look for `User-agent: *` block, `Disallow: /`, any `Sitemap:` lines.
- **[RUST]** `async_trait` usage, `&dyn SiteAuditor` — async trait boxing cost is acceptable here.
- Verify: wiremock-served robots.txt for all three branches.

### ✓ 2.7 Sitemap auditor (`src/audit/sitemap.rs`) — root only
- Fetch from robots.txt `Sitemap:` line OR fallback to `/sitemap.xml`.
- Findings: absent = Warning; malformed XML = Critical.
- Use `quick-xml` for parsing. Phase 2: check presence + well-formedness only (URL validation in Phase 4).
- Verify: wiremock valid + malformed sitemap.xml.

### ✓ 2.8 Wire all auditors into pipeline
- Update `audit/mod.rs::page_auditors()`, add `site_auditors()`.
- Pipeline: run page audits first, then site audits sequentially (Phase 3 parallelizes).
- Verify: integration test with wiremock site (HTML + robots.txt + sitemap.xml) → fully populated `AuditReport`.

### ✓ 2.9 Terminal reporter (`src/report/terminal.rs`)
- Components:
  - Header: site URL + score in colored text.
  - Per-category bar: `Metadata  ██████████ 92`.
  - Page table via `comfy-table`: URL (truncated), Score, Critical, Warning, Info counts. Sorted by score ascending.
  - Score colors: ≥90 green, 70–89 yellow, <70 red.
- Honor `--quiet` (score only), `--no-color`, `NO_COLOR` env, non-TTY (`std::io::IsTerminal` — Rust 1.70+).
- **[RUST]** `std::io::IsTerminal` trait; `owo-colors::if_supports_color` API.
- **[GOTCHA]** If `--output` set: JSON to file, human to stdout. Otherwise: JSON to stdout, human to stderr. Document in `--help`.
- Verify: `NO_COLOR=1` snapshot test asserting output contains expected substrings.

**Phase 2 exit criteria**: single URL produces rich colored terminal report + full JSON with all auditors, robots, sitemap.

---

## ✓ Phase 3 — Crawler: Redis frontier, worker pool, rate limiter (DONE)

Goal: `seo-rs https://example.com -d 3 -c 8` crawls multiple pages, merges into one report.

### ✓ 3.1 URL normalization (`src/parser/links.rs`)
- `pub fn normalize(base: &Url, href: &str) -> Option<Url>`:
  1. `base.join(href).ok()?`
  2. Lowercase scheme + host.
  3. Drop fragment: `u.set_fragment(None)`.
  4. Strip tracking params: keys starting with `utm_`, exact matches `fbclid`, `gclid`, `mc_eid`, `ref`, `igshid`.
  5. Sort remaining query params alphabetically via `form_urlencoded::Serializer`.
  6. Trailing slash: enforce on root path (`/`), preserve elsewhere.
  7. Return `None` for non-http(s) schemes (mailto, tel, javascript, data).
- **[RUST]** `Url::query_pairs`, `form_urlencoded`, owned vs borrowed `&str` iteration.
- **[GOTCHA]** Don't `.to_lowercase()` on raw URL text — `url` handles IDN internally.
- Verify: ≥20 unit test cases covering fragment, utm, relative, schemeless `//foo`, mailto rejection, query sorting, idempotence.

### ✓ 3.2 Link discovery
- `pub fn discover_links(base: &Url, dom: &Dom) -> Vec<Url>` — `dom.links()` + `normalize()`, deduped, same-host only.
- `pub fn is_same_site(a: &Url, b: &Url) -> bool` — compare exact hosts (document `www.foo.com` ≠ `foo.com` limitation).
- Verify: unit tests.

### ✓ 3.3 Redis frontier (`src/crawler/frontier.rs`)
- Build:
  ```rust
  pub struct Frontier {
      conn: redis::aio::ConnectionManager,
      job_id: String,
  }
  impl Frontier {
      pub async fn new(redis_url: &str, job_id: String) -> Result<Self>;
      pub async fn enqueue(&mut self, url: &Url, depth: u32) -> Result<bool>;
      pub async fn dequeue(&mut self) -> Result<Option<(Url, u32)>>;
      pub async fn mark_done(&mut self) -> Result<()>;
      pub async fn is_complete(&mut self) -> Result<bool>;
      pub async fn clear(&mut self) -> Result<()>;
  }
  ```
- Entry encoding: `format!("{depth}|{url}")`.
- **[GOTCHA]** SADD + RPUSH must be atomic — use a Lua script:
  ```lua
  local added = redis.call('SADD', KEYS[1], ARGV[1])
  if added == 1 then
    redis.call('RPUSH', KEYS[2], ARGV[2])
    redis.call('INCR', KEYS[3])
  end
  return added
  ```
- **[RUST]** `ConnectionManager` clones cheaply (each clone = independent multiplexed handle). `&mut self` on every method — no `Mutex` needed.
- **[GOTCHA]** Termination race: empty queue ≠ done. A worker may be mid-fetch about to enqueue children. Only exit when queue empty AND inflight == 0.
- Verify: integration test against local Redis (skip with `#[ignore]` if env var absent). Cover dedup, FIFO order, inflight counter, completion check.

### ✓ 3.4 Per-host rate limiter (`src/fetcher/rate_limiter.rs`)
- Use `governor` + `dashmap`:
  ```rust
  pub struct HostRateLimiter {
      per_host: dashmap::DashMap<String, Arc<governor::DefaultDirectRateLimiter>>,
      qps: u32,
  }
  impl HostRateLimiter {
      pub fn new(qps: u32) -> Self;
      pub async fn acquire(&self, host: &str) {
          let lim = self.per_host
              .entry(host.to_owned())
              .or_insert_with(|| Arc::new(governor::RateLimiter::direct(
                  governor::Quota::per_second(NonZeroU32::new(self.qps).unwrap())
              )))
              .clone();  // drop the entry guard HERE
          lim.until_ready().await;
      }
  }
  ```
- **[RUST]** `dashmap::Entry` API, `Arc<RateLimiter>` needed because guard must be dropped before `await`.
- **[GOTCHA]** Holding a DashMap entry guard across `.await` DEADLOCKS the map. Always `.clone()` the `Arc` out before awaiting. This is the #1 trap in this file.
- Verify: integration test — 10 requests to one mock host with `qps=2`, assert wall time ≥ ~4s.

### ✓ 3.5 Fetcher + rate limiter integration (REFACTOR `src/fetcher/`)
- Inject `Arc<HostRateLimiter>` into `Fetcher`.
- Call `rate_limiter.acquire(host).await` before each network call.
- Add backoff on 429/503: respect `Retry-After`; otherwise exponential (1s, 2s, 4s, max 3 retries).
- Verify: wiremock test returning 429 once then 200 — assert one retry.

### ✓ 3.6 Worker pool engine (`src/crawler/engine.rs`)
- Architecture:
  ```rust
  pub async fn run_crawl(
      config: Arc<CrawlConfig>,
      fetcher: Arc<Fetcher>,
      frontier: Frontier,
      page_auditors: Arc<Vec<Box<dyn PageAuditor>>>,
      site_auditors: Arc<Vec<Box<dyn SiteAuditor>>>,
  ) -> Result<Vec<PageReport>>;
  ```
- Worker loop:
  1. `dequeue` — if None, sleep 50ms, check `is_complete`, break or retry.
  2. Fetch URL.
  3. Run page audits inside `tokio::task::spawn_blocking` (avoids `!Send` `Dom` across `.await`).
  4. Run site audits (await).
  5. Score → `PageReport` → `tx.send`.
  6. Discover + enqueue children if `depth < config.depth`.
  7. `mark_done`.
- **[RUST]** "Drop the sender" pattern — `rx.recv()` only returns `None` when ALL `tx` clones dropped. Forget `drop(tx)` = hang forever. `Arc` for shared immutable state. `tokio::spawn` requires `'static + Send`.
- **[GOTCHA]** `scraper::Html` is `!Send`. Use `spawn_blocking`:
  ```rust
  let html = page_data.html.clone();
  let findings = tokio::task::spawn_blocking(move || {
      let dom = Dom::parse(&html);
      auditors.iter().flat_map(|a| a.audit(&page_data_snapshot, &dom)).collect::<Vec<_>>()
  }).await?;
  ```
  OR: keep `Dom` in a sync block, finish all sync work, drop it, THEN await site auditors.
- Verify: integration test crawling 3-page mock site, assert all three in report.

### ✓ 3.7 Wire crawler into pipeline
- `pipeline.rs`: `depth == 0` → single-page path; `depth > 0` → crawler engine.
- Generate unique `job_id` (timestamp or uuid).
- After completion: `frontier.clear()` or set EXPIRE on keys.
- Verify: end-to-end test against 5-page wiremock site.

### ✓ 3.8 Robots integration into crawl gating
- Before fetching any URL, consult cached robots.txt per host via `DashMap<String, RobotsRules>`.
- If `respect_robots` and disallowed: skip URL.
- Respect `Crawl-delay:` — feed back into rate limiter for that host.
- Verify: wiremock test with `Disallow: /private` — assert `/private` never fetched.

**Phase 3 exit criteria**: multi-page crawl works with Redis, rate-limited, robots-aware, merged report.

---

## Phase 4 — Polish (target: 3–4 days)

### ✓ 4.1 Broken-link auditor at scale (`src/audit/links.rs`)
- Site auditor. Collect all `<a href>` URLs across all pages. HEAD each unique URL; 405/error → GET fallback.
- Use `Semaphore` for bounded concurrency (e.g. 32 in-flight):
  ```rust
  let sem = Arc::new(tokio::sync::Semaphore::new(32));
  let mut futs = futures::stream::FuturesUnordered::new();
  for url in unique_urls {
      let sem = sem.clone();
      futs.push(async move {
          let _permit = sem.acquire_owned().await?;
          check_link(url).await
      });
  }
  while let Some(result) = futs.next().await { ... }
  ```
- Gate behind `--check-external-links` flag (default off) for external links.
- **[RUST]** `Semaphore::acquire_owned`, `FuturesUnordered` for streaming completion.
- **[GOTCHA]** HEAD-hostile servers return 405 or close connection — always have GET fallback.
- Verify: wiremock with one 200, one 404, one 500 link — assert correct findings.

### ✓ 4.2 Encoding robustness (`src/fetcher/client.rs`)
- Replace `.text().await` with:
  1. `let bytes = resp.bytes().await?;`
  2. Read charset from `Content-Type` header.
  3. If absent, sniff `<meta charset>` from first 1024 bytes.
  4. `encoding_rs::Encoding::for_label(charset.as_bytes()).decode(&bytes).0.into_owned()`
- **[RUST]** `Cow<str>` — `encoding_rs` returns `(Cow<str>, &'static Encoding, bool)`.
- Verify: Shift_JIS and Windows-1252 fixtures decode correctly.

### 4.3 Progress bar (`src/report/terminal.rs`)
- `indicatif::ProgressBar` — update total dynamically as URLs are discovered. Wire via channel from engine.
- Hide under `--quiet` or non-TTY.
- **[GOTCHA]** `tracing` output and `indicatif` fight over lines. Use `indicatif_log_bridge` or route tracing to a file under `--verbose`.
- Verify: manual smoke test.

### 4.4 Safety valves: `--timeout` and `--max-pages`
- Wrap crawl in `tokio::time::timeout(Duration::from_secs(global_timeout), run_crawl(...))`. On timeout, return partial report.
- `--max-pages`: `Arc<AtomicUsize>` counter, increment before enqueue.
- **[RUST]** `AtomicUsize::fetch_add(1, Ordering::Relaxed)` — `Relaxed` is correct for a counter without ordering dependencies.
- Verify: `--max-pages 2` against 10-page site → exactly 2 pages in report.

### 4.5 Sitemap URL validation pass
- Extend `sitemap.rs` to sample sitemap URLs (HEAD with semaphore-bounded concurrency), flag non-200s.
- Gate behind `--validate-sitemap` flag (default off).

### 4.6 JS-rendered page detection
- Heuristic: `visible_text_bytes / total_html_bytes < 0.05` AND very few content tags → Warning under `Category::Structure`: "page appears to require JS rendering — results may be incomplete."
- Verify: fixture with mostly empty `<div id="root"></div>` triggers it.

### 4.7 Wiremock integration test suite (`tests/integration.rs`)
- Requires refactoring to `lib.rs` exposing `pub async fn run(config) -> Result<AuditReport>` (tests compile as separate crates — only `pub` API accessible).
- Cover:
  1. Single-page audit — all auditors firing.
  2. 3-hop redirect chain.
  3. Robots.txt blocking a path.
  4. Sitemap with one broken URL.
  5. Multi-page crawl (5 pages, depth 2).
  6. `--max-pages` cutoff.
  7. Non-UTF-8 page (Shift_JIS).
  8. 429 backoff + retry.
- **[RUST]** `#[tokio::test]`, `include_str!` for fixtures, sharing `MockServer` across multiple `Mock` registrations.

### 4.8 README
- Install, quick start, all flags, sample output, scoring methodology.

### 4.9 CI (`.github/workflows/ci.yml`)
- `cargo fmt --check`, `cargo clippy -- -D warnings`, `cargo test`, `cargo build --release`.
- Add `services: redis` block for Redis-dependent tests.

**Phase 4 exit criteria**: handles real-world edge cases, bounded runtime, integration test coverage, usable progress UX.

---

## Cross-phase Rust learning checklist

1. **Ownership & borrowing**: `&T` vs `&mut T` vs owned `T`; when to clone `Arc` vs inner value.
2. **`Send` + `Sync`**: why `scraper::Html` is `!Send` — the single most likely compiler fight.
3. **Trait objects**: `Box<dyn Trait + Send + Sync>`, object safety.
4. **`async_trait`**: when needed and its allocation cost.
5. **Error handling**: `thiserror` in library code, `anyhow` at binary boundary, `?` + `#[from]`.
6. **`tokio::spawn` and `'static + Send` bounds**.
7. **Channels (`mpsc`) and the "drop the sender" pattern**.
8. **`Arc` vs `Rc`** — when each applies.
9. **Holding mutex/dashmap guards across `.await` — the cardinal async sin**.
10. **`spawn_blocking` for CPU-heavy or `!Send` work**.
11. **`Semaphore` for bounded concurrency**.
12. **Atomics and memory ordering** — `Relaxed` for simple counters.
13. **`Cow<str>`** — borrowed-or-owned strings from `encoding_rs`.
14. **`LazyLock` / `once_cell`** — expensive statics like compiled `Selector`s.

---

## Session sizing summary

| Phase | Step | Est. session |
|---|---|---|
| 0 | 0.1–0.6 | 30–60 min each |
| 1 | 1.1, 1.2, 1.4, 1.5, 1.8 | 1 session each |
| 1 | 1.3, 1.6, 1.7 | ~30 min each, pair two |
| 2 | 2.1–2.9 | 1 session each; 2.3 may need 1.5 |
| 3 | 3.3 (frontier), 3.6 (engine) | Full session each — meatiest |
| 3 | 3.1, 3.2, 3.4, 3.5, 3.7, 3.8 | 1 session each |
| 4 | 4.1, 4.7 | Full session each |
| 4 | 4.2–4.6, 4.8, 4.9 | Smaller, pair up |

**Total: ~30–35 focused coding sessions.**
