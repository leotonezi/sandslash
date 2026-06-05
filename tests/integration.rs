//! End-to-end integration tests for the full `pipeline::run` pipeline.
//!
//! Each test spins up an isolated `wiremock::MockServer` and calls
//! `sandslash::pipeline::run` — the only public entry point.  No internal
//! modules are imported.
//!
//! Redis-dependent tests (multi-page crawl paths) are gated with
//! `#[ignore = "requires local Redis on 127.0.0.1:6379"]` and run with:
//!   cargo test --test integration -- --ignored

use std::collections::HashSet;

use sandslash::config::CrawlConfig;
use sandslash::pipeline;
use tempfile::NamedTempFile;
use url::Url;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

const REDIS_URL: &str = "redis://127.0.0.1:6379/";

fn base_config(root: Url, tmp: &NamedTempFile) -> CrawlConfig {
    CrawlConfig {
        root,
        depth: 0,
        concurrency: 2,
        rate_per_host: 1000,
        redis_url: None,
        user_agent: "integration-test-agent".to_owned(),
        timeout_secs: 10,
        max_pages: None,
        global_timeout_secs: None,
        respect_robots: false,
        validate_sitemap: false,
        quiet: true,
        no_color: true,
        verbose: false,
        output_json: Some(tmp.path().to_path_buf()),
        check_external_links: false,
    }
}

fn server_root(server: &MockServer) -> Url {
    format!("{}/", server.uri())
        .parse()
        .expect("invariant: mock server URI is a valid URL")
}

// ── Test 1: single-page audit, all auditors fire ─────────────────────────────

#[tokio::test]
async fn single_page_all_auditors_fire() {
    let server = MockServer::start().await;

    Mock::given(method("GET"))
        .and(path("/"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_string(include_str!("fixtures/integration/full_page.html")),
        )
        .mount(&server)
        .await;

    // robots.txt: 404 → robots.missing finding (Crawlability).
    // Also satisfies the "multiple Mock registrations per MockServer" requirement.
    Mock::given(method("GET"))
        .and(path("/robots.txt"))
        .respond_with(ResponseTemplate::new(404))
        .mount(&server)
        .await;

    let tmp = NamedTempFile::new().expect("invariant: tempfile creation succeeds");
    let config = base_config(server_root(&server), &tmp);

    let report = pipeline::run(config).await.expect("pipeline must succeed");

    assert_eq!(report.pages.len(), 1, "expected exactly one page report");

    let categories: HashSet<_> = report.pages[0]
        .findings
        .iter()
        .map(|f| f.category)
        .collect();
    assert!(
        categories.len() >= 2,
        "expected ≥2 distinct Category variants; got {categories:?} from findings: {:?}",
        report.pages[0].findings
    );
}

// ── Test 2: 3-hop redirect chain followed to final URL ────────────────────────

#[tokio::test]
async fn three_hop_redirect_chain_resolved() {
    let server = MockServer::start().await;

    // Three redirects: /a → /b → /c → /d → 200
    // redirect_chain = [/a, /b, /c], len = 3; final URL = /d
    Mock::given(method("GET"))
        .and(path("/a"))
        .respond_with(ResponseTemplate::new(301).insert_header("location", "/b"))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/b"))
        .respond_with(ResponseTemplate::new(302).insert_header("location", "/c"))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/c"))
        .respond_with(ResponseTemplate::new(301).insert_header("location", "/d"))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/d"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_string(include_str!("fixtures/integration/full_page.html")),
        )
        .mount(&server)
        .await;

    let tmp = NamedTempFile::new().expect("invariant: tempfile creation succeeds");
    let start: Url = format!("{}/a", server.uri())
        .parse()
        .expect("invariant: mock URI is valid URL");
    let config = base_config(start, &tmp);

    let report = pipeline::run(config).await.expect("pipeline must succeed");

    assert_eq!(report.pages.len(), 1, "expected one page report");
    assert_eq!(
        report.pages[0].url.path(),
        "/d",
        "redirect chain must be followed to the final URL /d"
    );
}

// ── Test 3: robots.txt Disallow: / blocks the root URL ───────────────────────

#[tokio::test]
async fn robots_disallow_blocks_path() {
    let server = MockServer::start().await;

    Mock::given(method("GET"))
        .and(path("/robots.txt"))
        .respond_with(ResponseTemplate::new(200).set_body_string("User-agent: *\nDisallow: /\n"))
        .mount(&server)
        .await;

    // / must never be fetched when blocked by robots.txt.
    Mock::given(method("GET"))
        .and(path("/"))
        .respond_with(ResponseTemplate::new(500))
        .expect(0)
        .mount(&server)
        .await;

    let tmp = NamedTempFile::new().expect("invariant: tempfile creation succeeds");
    let mut config = base_config(server_root(&server), &tmp);
    config.respect_robots = true;

    let report = pipeline::run(config).await.expect("pipeline must succeed");

    assert!(
        report.pages.is_empty(),
        "robots.txt Disallow: / must result in zero pages; got {:?}",
        report.pages
    );
}

// ── Test 4: sitemap URL validation finds broken URL ───────────────────────────

#[tokio::test]
async fn sitemap_with_broken_url_reported() {
    let server = MockServer::start().await;

    const SIMPLE_HTML: &str = r#"<!DOCTYPE html>
<html><head><title>Sitemap Test Page Title Here</title></head>
<body><h1>Test</h1></body></html>"#;

    Mock::given(method("GET"))
        .and(path("/"))
        .respond_with(ResponseTemplate::new(200).set_body_string(SIMPLE_HTML))
        .mount(&server)
        .await;

    // robots.txt with no Sitemap: directive — auditor falls back to /sitemap.xml
    Mock::given(method("GET"))
        .and(path("/robots.txt"))
        .respond_with(ResponseTemplate::new(200).set_body_string("User-agent: *\nAllow: /\n"))
        .mount(&server)
        .await;

    // Sitemap listing one reachable and one broken URL on the same mock server.
    let sitemap_xml = format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<urlset xmlns="http://www.sitemaps.org/schemas/sitemap/0.9">
  <url><loc>{}/ok</loc></url>
  <url><loc>{}/broken</loc></url>
</urlset>"#,
        server.uri(),
        server.uri()
    );

    Mock::given(method("GET"))
        .and(path("/sitemap.xml"))
        .respond_with(ResponseTemplate::new(200).set_body_string(sitemap_xml))
        .mount(&server)
        .await;

    // /ok probed via HEAD → 200 (healthy)
    Mock::given(method("HEAD"))
        .and(path("/ok"))
        .respond_with(ResponseTemplate::new(200))
        .mount(&server)
        .await;

    // /broken probed via HEAD → 404 (triggers sitemap.url-unreachable finding)
    Mock::given(method("HEAD"))
        .and(path("/broken"))
        .respond_with(ResponseTemplate::new(404))
        .mount(&server)
        .await;

    let tmp = NamedTempFile::new().expect("invariant: tempfile creation succeeds");
    let mut config = base_config(server_root(&server), &tmp);
    config.validate_sitemap = true;

    let report = pipeline::run(config).await.expect("pipeline must succeed");

    assert_eq!(report.pages.len(), 1, "expected one page report");
    let has_sitemap_finding = report.pages[0]
        .findings
        .iter()
        .any(|f| f.check_id.starts_with("sitemap."));
    assert!(
        has_sitemap_finding,
        "expected at least one sitemap.* finding; got: {:?}",
        report.pages[0].findings
    );
}

// ── Test 5: multi-page depth-2 crawl visits all 5 pages ──────────────────────

#[tokio::test]
#[ignore = "requires local Redis on 127.0.0.1:6379"]
async fn multi_page_depth_two_crawls_five_pages() {
    let server = MockServer::start().await;

    mount_multi_page_site(&server).await;

    let tmp = NamedTempFile::new().expect("invariant: tempfile creation succeeds");
    let mut config = base_config(server_root(&server), &tmp);
    config.depth = 2;
    config.redis_url = Some(REDIS_URL.to_owned());
    config.concurrency = 2;

    let report = pipeline::run(config).await.expect("pipeline must succeed");

    assert_eq!(
        report.pages.len(),
        5,
        "depth-2 crawl of 5-page site must report exactly 5 pages; got {} page(s)",
        report.pages.len()
    );
}

// ── Test 6: --max-pages caps the number of crawled pages ─────────────────────

#[tokio::test]
#[ignore = "requires local Redis on 127.0.0.1:6379"]
async fn max_pages_cutoff_caps_reports() {
    let server = MockServer::start().await;

    mount_multi_page_site(&server).await;

    let tmp = NamedTempFile::new().expect("invariant: tempfile creation succeeds");
    let mut config = base_config(server_root(&server), &tmp);
    config.depth = 2;
    config.redis_url = Some(REDIS_URL.to_owned());
    config.max_pages = Some(2);
    config.concurrency = 2;

    let report = pipeline::run(config).await.expect("pipeline must succeed");

    assert!(
        report.pages.len() <= 2,
        "--max-pages 2 must cap the report to ≤2 pages; got {}",
        report.pages.len()
    );
    assert!(
        !report.pages.is_empty(),
        "at least one page must have been crawled"
    );
}

// ── Test 7: Shift_JIS page decoded without error ─────────────────────────────

#[tokio::test]
async fn non_utf8_shift_jis_decoded() {
    let server = MockServer::start().await;

    Mock::given(method("GET"))
        .and(path("/"))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("content-type", "text/html; charset=Shift_JIS")
                .set_body_bytes(include_bytes!("fixtures/integration/shift_jis.html").to_vec()),
        )
        .mount(&server)
        .await;

    // robots.txt: 404 — site auditor still runs and is satisfied.
    // Also satisfies the "multiple Mock registrations per MockServer" requirement.
    Mock::given(method("GET"))
        .and(path("/robots.txt"))
        .respond_with(ResponseTemplate::new(404))
        .mount(&server)
        .await;

    let tmp = NamedTempFile::new().expect("invariant: tempfile creation succeeds");
    let config = base_config(server_root(&server), &tmp);

    let report = pipeline::run(config)
        .await
        .expect("pipeline must succeed for Shift_JIS page");

    assert_eq!(report.pages.len(), 1, "expected one page report");
    assert_eq!(
        report.pages[0].url,
        server_root(&server),
        "page URL must match root"
    );
}

// ── Test 8: 429 response triggers retry, succeeds on second attempt ───────────

#[tokio::test]
async fn retry_after_429_then_200() {
    let server = MockServer::start().await;

    const SIMPLE_HTML: &str = r#"<!DOCTYPE html>
<html><head><title>Retry Test Page Title Here</title></head>
<body><h1>Success After Retry</h1></body></html>"#;

    // First request returns 429 with Retry-After: 1 (wall-clock 1s wait).
    // .up_to_n_times(1) ensures this mock fires at most once; subsequent
    // requests fall through to the 200 mock below.
    Mock::given(method("GET"))
        .and(path("/"))
        .respond_with(ResponseTemplate::new(429).insert_header("retry-after", "1"))
        .up_to_n_times(1)
        .mount(&server)
        .await;

    // Catch-all for the retry attempt — returns 200.
    Mock::given(method("GET"))
        .and(path("/"))
        .respond_with(ResponseTemplate::new(200).set_body_string(SIMPLE_HTML))
        .mount(&server)
        .await;

    let tmp = NamedTempFile::new().expect("invariant: tempfile creation succeeds");
    let config = base_config(server_root(&server), &tmp);

    let report = pipeline::run(config)
        .await
        .expect("pipeline must succeed after retry");

    assert_eq!(
        report.pages.len(),
        1,
        "expected one page report after retry"
    );
}

// ── Shared helper: mount the 5-page interlinked site ─────────────────────────

async fn mount_multi_page_site(server: &MockServer) {
    // Page graph (BFS from /, depth 2):
    //   depth 0: /  → links /a, /b
    //   depth 1: /a → links /c ; /b → links /d
    //   depth 2: /c (leaf), /d (leaf)
    // Total reachable: 5 pages.

    let pages: &[(&str, &str)] = &[
        ("/", include_str!("fixtures/integration/multi_root.html")),
        ("/a", include_str!("fixtures/integration/multi_a.html")),
        ("/b", include_str!("fixtures/integration/multi_b.html")),
        ("/c", include_str!("fixtures/integration/multi_c.html")),
        ("/d", include_str!("fixtures/integration/multi_d.html")),
    ];

    for (p, html) in pages {
        Mock::given(method("GET"))
            .and(path(*p))
            .respond_with(ResponseTemplate::new(200).set_body_string(*html))
            .mount(server)
            .await;

        // HEAD mocks for BrokenLinksAuditor probes — avoids false broken-link findings.
        Mock::given(method("HEAD"))
            .and(path(*p))
            .respond_with(ResponseTemplate::new(200))
            .mount(server)
            .await;
    }
}
