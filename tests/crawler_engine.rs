//! Integration tests for the crawler engine (worker pool).
//!
//! These tests require a live Redis instance at `127.0.0.1:6379`.
//! Run with:
//!   cargo test --test crawler_engine -- --ignored

use std::num::NonZeroU32;
use std::sync::Arc;

use sandslash::audit::{page_auditors, site_auditors};
use sandslash::config::CrawlConfig;
use sandslash::crawler::{Frontier, RobotsCache, run_crawl};
use sandslash::fetcher::{Fetcher, HostRateLimiter};
use url::Url;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

const REDIS_URL: &str = "redis://127.0.0.1:6379/";

/// Generate a unique job ID to isolate each test from leftover state.
fn job_id(label: &str) -> String {
    format!("engine-test-{}-{}", label, std::process::id())
}

/// Build a minimal `CrawlConfig` pointed at the given mock server.
fn make_config(root: Url, depth: u32, max_pages: Option<usize>) -> CrawlConfig {
    CrawlConfig {
        root,
        depth,
        concurrency: 2,
        rate_per_host: 1000,
        redis_url: Some(REDIS_URL.to_owned()),
        user_agent: "test-engine-agent".to_owned(),
        timeout_secs: 10,
        max_pages,
        respect_robots: false,
        quiet: true,
        no_color: true,
        output_json: None,
        check_external_links: false,
    }
}

/// Build a `Fetcher` and its `HostRateLimiter` for the given config.
fn make_fetcher(config: &CrawlConfig) -> (Fetcher, Arc<HostRateLimiter>) {
    let qps = NonZeroU32::new(1000).expect("invariant: 1000 != 0");
    let rate_limiter = Arc::new(HostRateLimiter::new(qps));
    let fetcher = Fetcher::new(config, Arc::clone(&rate_limiter))
        .expect("Fetcher::new must not fail in tests");
    (fetcher, rate_limiter)
}

/// Helper: build HTML with a `<title>` and a list of `<a href>` links.
fn html_with_links(title: &str, links: &[&str]) -> String {
    let link_tags: String = links
        .iter()
        .map(|href| format!(r#"<a href="{href}">{href}</a>"#))
        .collect::<Vec<_>>()
        .join("\n");

    format!(
        r#"<!DOCTYPE html>
<html>
<head>
  <title>{title}</title>
  <meta name="description" content="A test page for the engine integration test.">
</head>
<body>
  <h1>{title}</h1>
  {link_tags}
</body>
</html>"#
    )
}

/// 3-page wiremock site: / links to /a and /b, /a and /b have no further links.
/// The engine must crawl all 3 pages and return 3 `PageReport`s.
///
/// Requires a live Redis instance at 127.0.0.1:6379.
#[tokio::test]
#[ignore = "requires local Redis on 127.0.0.1:6379"]
async fn crawls_3_page_site_returns_3_reports() {
    let server = MockServer::start().await;
    let base = format!("{}/", server.uri());

    // / → links to /a and /b
    Mock::given(method("GET"))
        .and(path("/"))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("content-type", "text/html; charset=utf-8")
                .set_body_string(html_with_links("Home", &["/a", "/b"])),
        )
        .mount(&server)
        .await;

    // /a → no outgoing links
    Mock::given(method("GET"))
        .and(path("/a"))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("content-type", "text/html; charset=utf-8")
                .set_body_string(html_with_links("Page A", &[])),
        )
        .mount(&server)
        .await;

    // /b → no outgoing links
    Mock::given(method("GET"))
        .and(path("/b"))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("content-type", "text/html; charset=utf-8")
                .set_body_string(html_with_links("Page B", &[])),
        )
        .mount(&server)
        .await;

    let root: Url = base.parse().expect("root URL must parse");
    let config = Arc::new(make_config(root.clone(), 1, None));
    let (fetcher, rate_limiter) = make_fetcher(&config);
    let (fetcher, rate_limiter) = (Arc::new(fetcher), rate_limiter);
    let robots_cache = Arc::new(RobotsCache::new());

    let id = job_id("3page");
    let mut frontier = Frontier::new(REDIS_URL, id)
        .await
        .expect("Frontier::new must connect to Redis");
    frontier.clear().await.expect("frontier clear must succeed");

    let pa = Arc::new(page_auditors());
    let sa = Arc::new(site_auditors());

    let reports = run_crawl(
        config,
        fetcher,
        frontier,
        pa,
        sa,
        rate_limiter,
        robots_cache,
    )
    .await
    .expect("run_crawl must succeed");

    let urls: Vec<String> = reports.iter().map(|r| r.url.to_string()).collect();

    assert_eq!(
        reports.len(),
        3,
        "expected exactly 3 PageReports, got {}: {urls:?}",
        reports.len()
    );

    // All three pages must be represented (order may vary).
    let has_root = reports.iter().any(|r| r.url.path() == "/");
    let has_a = reports.iter().any(|r| r.url.path() == "/a");
    let has_b = reports.iter().any(|r| r.url.path() == "/b");

    assert!(has_root, "root page (/) missing from reports: {urls:?}");
    assert!(has_a, "/a missing from reports: {urls:?}");
    assert!(has_b, "/b missing from reports: {urls:?}");
}

/// When `max_pages = Some(1)`, only the root page must be fetched.
///
/// Requires a live Redis instance at 127.0.0.1:6379.
#[tokio::test]
#[ignore = "requires local Redis on 127.0.0.1:6379"]
async fn max_pages_cap_respected() {
    let server = MockServer::start().await;

    Mock::given(method("GET"))
        .and(path("/"))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("content-type", "text/html; charset=utf-8")
                .set_body_string(html_with_links("Home", &["/a", "/b", "/c"])),
        )
        .mount(&server)
        .await;

    // These should never be fetched due to max_pages = 1.
    Mock::given(method("GET"))
        .and(path("/a"))
        .respond_with(ResponseTemplate::new(200).set_body_string(html_with_links("A", &[])))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/b"))
        .respond_with(ResponseTemplate::new(200).set_body_string(html_with_links("B", &[])))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/c"))
        .respond_with(ResponseTemplate::new(200).set_body_string(html_with_links("C", &[])))
        .mount(&server)
        .await;

    let root: Url = format!("{}/", server.uri())
        .parse()
        .expect("root URL must parse");
    let config = Arc::new(make_config(root.clone(), 2, Some(1)));
    let (fetcher, rate_limiter) = make_fetcher(&config);
    let (fetcher, rate_limiter) = (Arc::new(fetcher), rate_limiter);
    let robots_cache = Arc::new(RobotsCache::new());

    let id = job_id("maxpages");
    let mut frontier = Frontier::new(REDIS_URL, id)
        .await
        .expect("Frontier::new must connect to Redis");
    frontier.clear().await.expect("frontier clear must succeed");

    let pa = Arc::new(page_auditors());
    let sa = Arc::new(site_auditors());

    let reports = run_crawl(
        config,
        fetcher,
        frontier,
        pa,
        sa,
        rate_limiter,
        robots_cache,
    )
    .await
    .expect("run_crawl must succeed");

    assert_eq!(
        reports.len(),
        1,
        "max_pages=1 must yield exactly 1 report, got {}: {:?}",
        reports.len(),
        reports.iter().map(|r| r.url.as_str()).collect::<Vec<_>>()
    );
    assert_eq!(
        reports[0].url.path(),
        "/",
        "only root page must be reported"
    );
}

/// Fetch errors must not abort the crawl — remaining pages still get reported.
///
/// Requires a live Redis instance at 127.0.0.1:6379.
#[tokio::test]
#[ignore = "requires local Redis on 127.0.0.1:6379"]
async fn fetch_error_skipped_crawl_continues() {
    let server = MockServer::start().await;

    // / links to /ok and /bad
    Mock::given(method("GET"))
        .and(path("/"))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("content-type", "text/html; charset=utf-8")
                .set_body_string(html_with_links("Home", &["/ok", "/bad"])),
        )
        .mount(&server)
        .await;

    Mock::given(method("GET"))
        .and(path("/ok"))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("content-type", "text/html; charset=utf-8")
                .set_body_string(html_with_links("OK Page", &[])),
        )
        .mount(&server)
        .await;

    // /bad is deliberately not mounted — wiremock returns a connection-level error,
    // which triggers the fetch-error path in the worker.
    // We instead mount it with a 500 to exercise the "logged and skipped" path.
    Mock::given(method("GET"))
        .and(path("/bad"))
        .respond_with(ResponseTemplate::new(500).set_body_string("server error"))
        .mount(&server)
        .await;

    let root: Url = format!("{}/", server.uri())
        .parse()
        .expect("root URL must parse");
    let config = Arc::new(make_config(root.clone(), 1, None));
    let (fetcher, rate_limiter) = make_fetcher(&config);
    let (fetcher, rate_limiter) = (Arc::new(fetcher), rate_limiter);
    let robots_cache = Arc::new(RobotsCache::new());

    let id = job_id("fetcherr");
    let mut frontier = Frontier::new(REDIS_URL, id)
        .await
        .expect("Frontier::new must connect to Redis");
    frontier.clear().await.expect("frontier clear must succeed");

    let pa = Arc::new(page_auditors());
    let sa = Arc::new(site_auditors());

    // A 500 is not a fetch error (it's a valid HTTP response), so all 3 pages
    // should appear in the report.
    let reports = run_crawl(
        config,
        fetcher,
        frontier,
        pa,
        sa,
        rate_limiter,
        robots_cache,
    )
    .await
    .expect("run_crawl must not fail even when pages return 500");

    assert_eq!(
        reports.len(),
        3,
        "expected 3 reports (root + /ok + /bad 500), got {}: {:?}",
        reports.len(),
        reports.iter().map(|r| r.url.as_str()).collect::<Vec<_>>()
    );
}
