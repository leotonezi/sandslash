//! Integration tests for safety valve features:
//! - `--max-pages` enforcement (enqueue gate)
//! - `--global-timeout` (wall-clock crawl timeout → partial report)
//!
//! Tests that require Redis are marked `#[ignore]`.  Run with:
//!   cargo test --test safety_valves -- --ignored

use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

const REDIS_URL: &str = "redis://127.0.0.1:6379/";

/// Helper: build HTML with a list of `<a href>` links.
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
  <meta name="description" content="Safety-valve test page.">
</head>
<body>
  <h1>{title}</h1>
  {link_tags}
</body>
</html>"#
    )
}

/// 10-page wiremock site, `--max-pages 2`, depth ≥ 2.
///
/// Root links to /p1 … /p9.  With `max_pages = 2`, only root + one child
/// may be fetched — the counter is incremented at enqueue time, so root
/// accounts for page 1 and at most one child (page 2) is admitted.
///
/// Asserts: `report.pages.len() == 2`.
///
/// Requires a live Redis instance at 127.0.0.1:6379.
#[tokio::test]
#[ignore = "requires local Redis on 127.0.0.1:6379"]
async fn max_pages_2_on_10_page_site_yields_exactly_2_pages() {
    let server = MockServer::start().await;

    // Root links to 9 children.
    let children: Vec<&str> = vec![
        "/p1", "/p2", "/p3", "/p4", "/p5", "/p6", "/p7", "/p8", "/p9",
    ];
    Mock::given(method("GET"))
        .and(path("/"))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("content-type", "text/html; charset=utf-8")
                .set_body_string(html_with_links("Home", &children)),
        )
        .mount(&server)
        .await;

    // Mount all child pages (they link back to more children to ensure depth works).
    for i in 1..=9u32 {
        Mock::given(method("GET"))
            .and(path(format!("/p{i}")))
            .respond_with(
                ResponseTemplate::new(200)
                    .insert_header("content-type", "text/html; charset=utf-8")
                    .set_body_string(html_with_links(&format!("Page {i}"), &[])),
            )
            .mount(&server)
            .await;
    }

    // robots.txt / sitemap.xml — 404 to avoid auditor side-effects.
    Mock::given(method("GET"))
        .and(path("/robots.txt"))
        .respond_with(ResponseTemplate::new(404))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/sitemap.xml"))
        .respond_with(ResponseTemplate::new(404))
        .mount(&server)
        .await;

    let root_url = url::Url::parse(&format!("{}/", server.uri()))
        .expect("mock server URI must be a valid URL");

    let config = sandslash::config::CrawlConfig {
        root: root_url,
        depth: 2,
        concurrency: 2,
        rate_per_host: 1000,
        redis_url: Some(REDIS_URL.to_owned()),
        user_agent: "seo-rs-test/0.1".to_owned(),
        timeout_secs: 10,
        max_pages: Some(2),
        global_timeout_secs: None,
        respect_robots: false,
        quiet: true,
        no_color: true,
        output_json: None,
        verbose: false,
        check_external_links: false,
        validate_sitemap: false,
    };

    let report = sandslash::pipeline::run(config)
        .await
        .expect("pipeline must not fail");

    assert_eq!(
        report.pages.len(),
        2,
        "max_pages=2 must yield exactly 2 pages, got {}: {:?}",
        report.pages.len(),
        report
            .pages
            .iter()
            .map(|p| p.url.as_str())
            .collect::<Vec<_>>()
    );
}

/// Slow root page + `--global-timeout 1` → `Ok(partial report)`.
///
/// The root page has a 3-second response delay.  With `global_timeout_secs = 1`,
/// the crawl should time out and return a partial (possibly empty) report
/// rather than an error.
///
/// Requires a live Redis instance at 127.0.0.1:6379.
#[tokio::test]
#[ignore = "requires local Redis on 127.0.0.1:6379"]
async fn global_timeout_returns_partial_report_not_error() {
    let server = MockServer::start().await;

    // Root responds after a 3-second delay — longer than the 1-second global timeout.
    Mock::given(method("GET"))
        .and(path("/"))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("content-type", "text/html; charset=utf-8")
                .set_body_string(html_with_links("Slow Home", &[]))
                .set_delay(std::time::Duration::from_secs(3)),
        )
        .mount(&server)
        .await;

    // robots.txt / sitemap.xml — 404.
    Mock::given(method("GET"))
        .and(path("/robots.txt"))
        .respond_with(ResponseTemplate::new(404))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/sitemap.xml"))
        .respond_with(ResponseTemplate::new(404))
        .mount(&server)
        .await;

    let root_url = url::Url::parse(&format!("{}/", server.uri()))
        .expect("mock server URI must be a valid URL");

    let config = sandslash::config::CrawlConfig {
        root: root_url,
        depth: 1,
        concurrency: 1,
        rate_per_host: 1000,
        redis_url: Some(REDIS_URL.to_owned()),
        user_agent: "seo-rs-test/0.1".to_owned(),
        // Per-request timeout: long enough so reqwest doesn't timeout first.
        timeout_secs: 30,
        max_pages: None,
        global_timeout_secs: Some(1),
        respect_robots: false,
        quiet: true,
        no_color: true,
        output_json: None,
        verbose: false,
        check_external_links: false,
        validate_sitemap: false,
    };

    let result = sandslash::pipeline::run(config).await;

    // Must be Ok (not Err) even though the crawl was cut short.
    let report = result.expect("global timeout must yield Ok(partial report), not Err");

    // Partial report: pages may be empty (root didn't finish before timeout).
    // The important invariant is that the result is Ok, not Err.
    // site_score must be valid (0–100).
    assert!(
        report.site_score <= 100,
        "site_score must be in 0..=100, got {}",
        report.site_score
    );
}
