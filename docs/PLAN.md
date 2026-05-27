<project name="seo-rs" language="rust" type="cli">

<goal>
A CLI tool that accepts a URL, crawls the page and optionally its internal links up to a
configurable depth, runs a battery of SEO checks, and emits a structured JSON report plus a
colored human-readable terminal summary. Pipeline: fetch -> parse -> audit -> score -> report.
</goal>

<constraints>
  <constraint>Respect robots.txt and crawl-delay.</constraint>
  <constraint>Identify honestly via a configurable User-Agent.</constraint>
  <constraint>Rate-limit per host; never bypass CAPTCHAs or bot protection.</constraint>
  <constraint>Intended for sites the operator owns or is authorized to audit.</constraint>
</constraints>

<!-- ============================================================ -->
<architecture>
  <data_flow>fetch -> parse -> audit -> score -> report (crawling wraps this in a worker pool fed by a Redis frontier)</data_flow>

  <file_tree>
seo-rs/
├── Cargo.toml
├── src/
│   ├── main.rs              # entry: parse args, build config, run
│   ├── cli.rs               # clap command/argument definitions
│   ├── config.rs            # CrawlConfig: depth, concurrency, rate, redis_url, user_agent
│   ├── error.rs             # thiserror error enum + Result alias
│   ├── model.rs             # shared types: PageData, Finding, Severity, Category, Report
│   ├── crawler/
│   │   ├── mod.rs           # Crawler facade
│   │   ├── engine.rs        # worker pool, termination detection, orchestration
│   │   └── frontier.rs      # Redis FIFO queue + visited set
│   ├── fetcher/
│   │   ├── mod.rs
│   │   ├── client.rs        # reqwest wrapper: GET, manual redirect-chain capture, encoding
│   │   └── rate_limiter.rs  # per-host governor token buckets
│   ├── parser/
│   │   ├── mod.rs
│   │   ├── dom.rs           # scraper wrapper: typed element extraction
│   │   └── links.rs         # link discovery + URL normalization
│   ├── audit/
│   │   ├── mod.rs           # Auditor traits, AuditContext, registry
│   │   ├── metadata.rs      # title, description, canonical
│   │   ├── opengraph.rs     # og:* / twitter:* tags
│   │   ├── headings.rs      # h1..h6 hierarchy
│   │   ├── images.rs        # alt attributes
│   │   ├── links.rs         # broken-link detection (network)
│   │   ├── robots.rs        # robots.txt (network)
│   │   ├── sitemap.rs       # sitemap.xml (network)
│   │   ├── https.rs         # scheme, mixed content
│   │   └── redirects.rs     # redirect chain length / loops
│   ├── score/
│   │   └── mod.rs           # category weights, penalty math, 0–100 rollup
│   └── report/
│       ├── mod.rs
│       ├── json.rs          # serde_json serialization
│       └── terminal.rs      # colored summary + tables
└── tests/
    └── integration.rs       # end-to-end against fixtures / wiremock
  </file_tree>

  <module name="model" file="src/model.rs" role="shared types">
    <code lang="rust">
use serde::Serialize;
use url::Url;

#[derive(Debug, Clone, Serialize)]
pub struct PageData {
    pub url: Url,
    pub status: u16,
    pub redirect_chain: Vec&lt;Url&gt;,
    pub html: String,            // decoded to UTF-8
    pub headers: Headers,
    pub depth: u32,
}

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
pub enum Severity { Critical, Warning, Info }

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq, Hash)]
pub enum Category { Metadata, SocialTags, Structure, Links, Media, Crawlability, Security }

#[derive(Debug, Clone, Serialize)]
pub struct Finding {
    pub check_id: &amp;'static str,
    pub category: Category,
    pub severity: Severity,
    pub message: String,
    pub penalty: u8,             // points subtracted within its category
}

#[derive(Debug, Serialize)]
pub struct PageReport {
    pub url: Url,
    pub findings: Vec&lt;Finding&gt;,
    pub category_scores: std::collections::HashMap&lt;Category, u8&gt;,
    pub score: u8,               // 0–100
}

#[derive(Debug, Serialize)]
pub struct AuditReport {
    pub root: Url,
    pub pages: Vec&lt;PageReport&gt;,
    pub site_score: u8,
    pub crawled_at: String,
}
    </code>
  </module>

  <module name="audit traits" file="src/audit/mod.rs" role="auditor abstraction">
    <note>Two traits separate pure DOM checks (sync, fast, fixture-testable) from network checks (async). The registry holds Vec&lt;Box&lt;dyn PageAuditor&gt;&gt; and Vec&lt;Box&lt;dyn SiteAuditor&gt;&gt;. Adding a check = implement a trait + register.</note>
    <code lang="rust">
pub trait PageAuditor: Send + Sync {
    fn id(&amp;self) -> &amp;'static str;
    fn category(&amp;self) -> Category;
    fn audit(&amp;self, page: &amp;PageData, dom: &amp;Dom) -> Vec&lt;Finding&gt;;
}

#[async_trait::async_trait]
pub trait SiteAuditor: Send + Sync {
    fn id(&amp;self) -> &amp;'static str;
    fn category(&amp;self) -> Category;
    async fn audit(&amp;self, page: &amp;PageData, ctx: &amp;AuditContext) -> Vec&lt;Finding&gt;;
}
    </code>
  </module>
</architecture>

<!-- ============================================================ -->
<tech_stack>
  <note>Verify exact versions on crates.io at scaffold time via `cargo add`.</note>
  <crate name="tokio" version="1.x" features="full" why="async runtime, tasks, semaphores, channels, timers"/>
  <crate name="reqwest" version="0.12" why="HTTP client on hyper; tokio-native; TLS + redirect control"/>
  <crate name="scraper" version="0.2x" why="HTML parsing + CSS-selector queries (html5ever)"/>
  <crate name="url" version="2.x" why="RFC-compliant URL parse/join/normalize"/>
  <crate name="redis" version="0.2x" features="tokio-comp,connection-manager" why="frontier queue + visited set"/>
  <crate name="governor" version="0.x" why="token-bucket rate limiting per host"/>
  <crate name="clap" version="4.x" features="derive" why="CLI parsing"/>
  <crate name="serde" version="1.x" why="report serialization"/>
  <crate name="serde_json" version="1.x" why="JSON output"/>
  <crate name="thiserror" version="1.x" why="library error enums"/>
  <crate name="anyhow" version="1.x" why="error context at binary boundary"/>
  <crate name="async-trait" version="0.1" why="async fn in SiteAuditor trait"/>
  <crate name="encoding_rs" version="0.8" why="decode non-UTF-8 responses"/>
  <crate name="owo-colors" version="4.x" why="terminal colors, no global state"/>
  <crate name="comfy-table" version="7.x" why="CLI summary tables"/>
  <crate name="indicatif" version="0.17" why="crawl progress bar"/>
  <crate name="tracing" version="0.1" why="structured logging / --verbose"/>
  <crate name="dashmap" version="6.x" why="concurrent map for per-host limiters"/>
  <crate name="wiremock" version="latest" scope="dev" why="mock HTTP server for tests"/>
  <crate name="chromiumoxide" version="latest" scope="optional" why="headless-browser rendering for JS pages (post-MVP)"/>
  <crate name="deadpool-redis" version="latest" scope="optional" why="connection pooling if single ConnectionManager saturates"/>
</tech_stack>

<!-- ============================================================ -->
<frontier file="src/crawler/frontier.rs" backend="redis">
  <structure type="LIST" key="seo:{job}:frontier" purpose="FIFO queue" ops="RPUSH enqueue, LPOP dequeue"/>
  <structure type="SET" key="seo:{job}:visited" purpose="dedup" ops="SADD atomic check-and-insert"/>
  <structure type="STRING" key="seo:{job}:inflight" purpose="termination detection" ops="INCR / DECR"/>

  <entry_encoding>"{depth}|{normalized_url}" so depth travels with the URL</entry_encoding>

  <key_trick>SADD returns 1 if member was new, 0 if it already existed -> "have I seen this?" and "mark seen" become one atomic op, eliminating the check-then-insert race across concurrent workers.</key_trick>

  <code lang="rust">
pub struct Frontier {
    conn: redis::aio::ConnectionManager,
    job: String,
}

impl Frontier {
    /// Returns true if newly enqueued (false = duplicate).
    pub async fn enqueue(&amp;mut self, url: &amp;Url, depth: u32) -> Result&lt;bool&gt; {
        let norm = normalize(url);
        let visited = format!("seo:{}:visited", self.job);
        let added: i64 = redis::cmd("SADD").arg(&amp;visited).arg(&amp;norm)
            .query_async(&amp;mut self.conn).await?;
        if added == 0 { return Ok(false); }
        let frontier = format!("seo:{}:frontier", self.job);
        redis::cmd("RPUSH").arg(&amp;frontier).arg(format!("{depth}|{norm}"))
            .query_async(&amp;mut self.conn).await?;
        Ok(true)
    }

    pub async fn dequeue(&amp;mut self) -> Result&lt;Option&lt;(Url, u32)&gt;&gt; {
        let frontier = format!("seo:{}:frontier", self.job);
        let entry: Option&lt;String&gt; = redis::cmd("LPOP").arg(&amp;frontier)
            .query_async(&amp;mut self.conn).await?;
        Ok(entry.map(parse_entry))
    }
}
  </code>

  <normalization location="src/parser/links.rs" run="before enqueue">
    <rule>lowercase scheme + host</rule>
    <rule>drop fragment</rule>
    <rule>strip tracking params (utm_*, fbclid, ...)</rule>
    <rule>sort remaining query params</rule>
    <rule>apply consistent trailing-slash policy</rule>
    <rule>resolve relative links via base.join(href)</rule>
  </normalization>

  <scope_control>Enqueue only same-host links with depth+1 &lt;= max_depth. External links are checked for liveness by the broken-link auditor but never enqueued.</scope_control>
</frontier>

<!-- ============================================================ -->
<seo_checks>
  <check id="title" type="page" category="Metadata" flags="missing, empty, len&lt;30 or &gt;60, duplicate across pages"/>
  <check id="description" type="page" category="Metadata" flags="missing, len&lt;50 or &gt;160"/>
  <check id="canonical" type="page" category="Metadata" flags="missing, self-ref mismatch, off-host"/>
  <check id="opengraph" type="page" category="SocialTags" flags="missing og:title/description/image/url, missing twitter:card"/>
  <check id="headings" type="page" category="Structure" flags="0 or multiple h1, skipped levels (h2->h4), empty headings"/>
  <check id="images" type="page" category="Media" flags="img without alt, empty alt on content images"/>
  <check id="https" type="page" category="Security" flags="page over http, mixed content"/>
  <check id="redirects" type="page" category="Crawlability" flags="chain length &gt; N, redirect loop"/>
  <check id="broken_links" type="site" category="Links" flags="internal/external links returning 4xx/5xx (HEAD, fallback GET)"/>
  <check id="robots" type="site" category="Crawlability" flags="absent, blocks whole site, missing Sitemap: directive"/>
  <check id="sitemap" type="site" category="Crawlability" flags="absent, malformed XML, URLs non-200"/>

  <example id="headings" lang="rust">
impl PageAuditor for HeadingAuditor {
    fn id(&amp;self) -> &amp;'static str { "headings" }
    fn category(&amp;self) -> Category { Category::Structure }
    fn audit(&amp;self, _page: &amp;PageData, dom: &amp;Dom) -> Vec&lt;Finding&gt; {
        let mut f = vec![];
        match dom.select_all("h1").len() {
            0 => f.push(critical("headings", Category::Structure, "Page has no &lt;h1&gt;", 40)),
            1 => {}
            n => f.push(warning("headings", Category::Structure,
                    format!("Page has {n} &lt;h1&gt; elements; expected 1"), 20)),
        }
        // track last-seen level; flag jumps &gt; 1 as skipped levels
        f
    }
}
  </example>

  <note id="broken_links">Use HEAD first (cheap), fall back to GET for HEAD-hostile servers. Rate-limit and dedup probes so each URL is tested once.</note>
</seo_checks>

<!-- ============================================================ -->
<scoring file="src/score/mod.rs">
  <model>Each category starts at 100; findings subtract their penalty (clamped to 0). Page score = weighted sum of category scores. Site score = mean of page scores (optionally homepage-weighted).</model>

  <weights total="100">
    <category name="Metadata" weight="20" why="title/description/canonical drive SERP appearance"/>
    <category name="Crawlability" weight="20" why="robots/sitemap gate ranking at all"/>
    <category name="Structure" weight="15" why="heading hierarchy = semantic/accessibility signal"/>
    <category name="Links" weight="15" why="broken links hurt UX + crawl budget"/>
    <category name="Security" weight="15" why="HTTPS = ranking factor + trust"/>
    <category name="Media" weight="10" why="alt text = accessibility + image SEO"/>
    <category name="SocialTags" weight="5" why="OG/Twitter affect sharing, not core ranking"/>
  </weights>

  <code lang="rust">
pub fn score_page(findings: &amp;[Finding]) -> (u8, HashMap&lt;Category, u8&gt;) {
    let mut cat: HashMap&lt;Category, i32&gt; = Category::all().map(|c| (c, 100)).collect();
    for f in findings { *cat.get_mut(&amp;f.category).unwrap() -= f.penalty as i32; }
    for v in cat.values_mut() { *v = (*v).clamp(0, 100); }
    let weighted: f64 = cat.iter()
        .map(|(c, &amp;s)| s as f64 * c.weight() as f64 / 100.0).sum();
    let scores = cat.iter().map(|(&amp;c, &amp;s)| (c, s as u8)).collect();
    (weighted.round() as u8, scores)
}
  </code>

  <tuning>Penalties set so one Critical meaningfully dents its category while a few Info findings barely move it.</tuning>
</scoring>

<!-- ============================================================ -->
<output>
  <format name="json" file="src/report/json.rs" trigger="--format json or --output report.json">
    <note>Whole AuditReport derives Serialize; emit via serde_json::to_writer_pretty. Machine-readable / CI path.</note>
    <shape lang="json">
{
  "root": "https://example.com",
  "site_score": 78,
  "crawled_at": "2026-05-26T14:03:00Z",
  "pages": [
    { "url": "https://example.com/", "score": 82,
      "category_scores": { "Metadata": 90, "Security": 100, "Structure": 60 },
      "findings": [
        { "check_id": "headings", "category": "Structure",
          "severity": "Critical", "message": "Page has no h1", "penalty": 40 }
      ] }
  ]
}
    </shape>
  </format>

  <format name="terminal" file="src/report/terminal.rs" default="true">
    <element>header with site score + per-category bars</element>
    <element>comfy-table of pages sorted by worst score</element>
    <colors lib="owo-colors">score &gt;=90 green, 70-89 yellow, &lt;70 red; severity icons x/!/i</colors>
    <flag name="--quiet">print only final score for scripting</flag>
    <respect>NO_COLOR env var and non-TTY (disable colors when piped)</respect>
  </format>
</output>

<!-- ============================================================ -->
<concurrency runtime="tokio">
  <model>Spawn N worker tasks (--concurrency, default ~8). Each: dequeue -> rate-limit permit -> fetch -> parse -> audit -> enqueue new links -> send PageReport over mpsc. One collector task drains the channel into the report.</model>

  <code lang="rust">
pub async fn run(cfg: CrawlConfig) -> Result&lt;AuditReport&gt; {
    let frontier = Frontier::connect(&amp;cfg).await?;
    frontier.clone().enqueue(&amp;cfg.root, 0).await?;
    let limiter = HostRateLimiter::new(cfg.per_host_rps);
    let sem = Arc::new(Semaphore::new(cfg.concurrency));
    let (tx, mut rx) = mpsc::channel::&lt;PageReport&gt;(256);
    let inflight = Arc::new(AtomicUsize::new(0));

    let mut workers = JoinSet::new();
    for _ in 0..cfg.concurrency {
        let (mut f, lim, sem, tx, inflight, cfg) =
            (frontier.clone(), limiter.clone(), sem.clone(),
             tx.clone(), inflight.clone(), cfg.clone());
        workers.spawn(async move {
            loop {
                let Some((url, depth)) = f.dequeue().await? else {
                    if inflight.load(Ordering::SeqCst) == 0 { break; }
                    tokio::time::sleep(Duration::from_millis(50)).await;
                    continue;
                };
                inflight.fetch_add(1, Ordering::SeqCst);
                let _permit = sem.acquire().await?;
                lim.until_ready(url.host_str()).await;
                if let Ok(r) = process(&amp;url, depth, &amp;mut f, &amp;cfg).await {
                    let _ = tx.send(r).await;
                }
                inflight.fetch_sub(1, Ordering::SeqCst);
            }
            Result::&lt;()&gt;::Ok(())
        });
    }
    drop(tx);
    let mut pages = vec![];
    while let Some(p) = rx.recv().await { pages.push(p); }
    while workers.join_next().await.is_some() {}
    Ok(finalize(cfg.root, pages))
}
  </code>

  <rate_limiter file="src/fetcher/rate_limiter.rs">Wrap governor in DashMap&lt;Host, RateLimiter&gt; so each host gets its own token bucket (e.g. 2 req/s). until_ready(host) awaits a permit without blocking other hosts. Add a global Semaphore to bound total open connections.</rate_limiter>

  <termination_detection critical="true">Empty queue != done; a worker may be mid-fetch about to enqueue more links. Workers exit only when queue is empty AND inflight == 0. Add --timeout and --max-pages safety valves.</termination_detection>
</concurrency>

<!-- ============================================================ -->
<milestones>
  <phase id="0" name="Scaffolding" complexity="low" effort="0.5d">
    Cargo project, clap CLI, config.rs, error.rs, model.rs, logging.
  </phase>
  <phase id="1" name="MVP" complexity="medium" effort="2d" depends_on="0">
    Fetch one URL -> parse -> 3 pure auditors (metadata, headings, https) -> JSON output.
    <cut_line>`seo-rs https://example.com --format json` works end to end on a single page.</cut_line>
  </phase>
  <phase id="2" name="Full single-page auditor" complexity="medium-high" effort="3-4d" depends_on="1">
    All pure + network auditors, scoring system, colored terminal report.
  </phase>
  <phase id="3" name="Crawler" complexity="high" effort="4-5d" depends_on="2">
    Redis frontier, link discovery + normalization, Tokio worker pool, per-host rate limiter, robots/sitemap.
  </phase>
  <phase id="4" name="Polish" complexity="medium-high" effort="3-4d" depends_on="3">
    Broken-link checking at scale, redirect-chain reporting, encoding robustness, probe dedup, progress bar, --max-pages/--timeout, wiremock integration tests, README.
  </phase>
  <ordering_rationale>Get the pure pipeline correct and fixture-tested before adding network + concurrency, so Phase 3 bugs can be isolated from a trusted parsing/scoring layer.</ordering_rationale>
</milestones>

<!-- ============================================================ -->
<challenges>
  <challenge name="JS-rendered pages" severity="high">
    <problem>reqwest fetches static HTML only; SPAs return near-empty body.</problem>
    <mitigation>Detect (low text-to-markup ratio + large script payload) and emit a Warning. Optionally add opt-in --render mode via chromiumoxide; keep out of MVP (heavy dependency, much slower).</mitigation>
  </challenge>
  <challenge name="Bot detection" severity="medium">
    <problem>Sites return 403/429 or challenge pages to crawlers.</problem>
    <mitigation>Honest User-Agent, obey robots.txt + crawl-delay, per-host rate limit, exponential backoff on 429/503 respecting Retry-After. Do NOT defeat CAPTCHAs or impersonate browsers.</mitigation>
  </challenge>
  <challenge name="Redirect loops" severity="medium">
    <problem>A->B->A hangs a naive fetcher.</problem>
    <mitigation>Configure reqwest redirect::Policy::none() and follow hops manually; cap chain (~10); track visited URLs within one fetch; on repeat, abort + emit Critical loop finding; record full chain in PageData.redirect_chain.</mitigation>
  </challenge>
  <challenge name="Encoding issues" severity="medium">
    <problem>.text() assumes UTF-8 and mangles windows-1252 / Shift-JIS pages.</problem>
    <mitigation>Fetch .bytes(), read charset from Content-Type header and/or meta charset, decode with encoding_rs; guard malformed bytes so one bad page doesn't crash the crawl.</mitigation>
  </challenge>
  <challenge name="Other" severity="low">
    <item>Trailing-slash / query duplication -> normalization (tune strip-list per site).</item>
    <item>Huge/infinite responses -> max response size + per-request timeout.</item>
    <item>Redis backpressure -> move to deadpool-redis if one ConnectionManager saturates.</item>
    <item>HEAD-hostile servers -> GET fallback with byte cap for link checks.</item>
  </challenge>
</challenges>

</project>
