use sandslash::audit::links::BrokenLinksAuditor;
use sandslash::audit::{AuditContext, SiteAuditor};
use sandslash::config::CrawlConfig;
use sandslash::fetcher::{Fetcher, HostRateLimiter};
use sandslash::model::{Headers, PageData, Severity};
use std::num::NonZeroU32;
use std::sync::Arc;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

fn make_config(server: &MockServer) -> CrawlConfig {
    CrawlConfig {
        root: format!("{}/", server.uri()).parse().unwrap(),
        depth: 0,
        concurrency: 1,
        rate_per_host: 1000,
        redis_url: None,
        user_agent: "test-agent".to_owned(),
        timeout_secs: 10,
        max_pages: None,
        respect_robots: false,
        quiet: false,
        no_color: true,
        output_json: None,
        check_external_links: false,
    }
}

fn make_fetcher(config: &CrawlConfig) -> Fetcher {
    let qps = NonZeroU32::new(1000).expect("invariant: 1000 != 0");
    let rl = Arc::new(HostRateLimiter::new(qps));
    Fetcher::new(config, rl).expect("Fetcher::new must succeed in tests")
}

fn make_page(base_url: &str, html: &str) -> PageData {
    PageData {
        url: base_url.parse().unwrap(),
        status: 200,
        redirect_chain: vec![],
        html: html.to_owned(),
        headers: Headers::default(),
        depth: 0,
    }
}

/// /ok → 200 (no finding), /missing → 404 (links.broken-4xx), /broken → 500 (links.broken-5xx)
#[tokio::test]
async fn test_basic_link_statuses() {
    let server = MockServer::start().await;

    Mock::given(method("HEAD"))
        .and(path("/ok"))
        .respond_with(ResponseTemplate::new(200))
        .mount(&server)
        .await;
    Mock::given(method("HEAD"))
        .and(path("/missing"))
        .respond_with(ResponseTemplate::new(404))
        .mount(&server)
        .await;
    Mock::given(method("HEAD"))
        .and(path("/broken"))
        .respond_with(ResponseTemplate::new(500))
        .mount(&server)
        .await;

    let config = make_config(&server);
    let base_url = format!("{}/", server.uri());
    let html = format!(
        r#"<a href="{base}/ok">ok</a><a href="{base}/missing">miss</a><a href="{base}/broken">broken</a>"#,
        base = server.uri()
    );
    let page = make_page(&base_url, &html);
    let fetcher = make_fetcher(&config);
    let ctx = AuditContext {
        config: Arc::new(config),
        fetcher: Arc::new(fetcher),
    };

    let findings = BrokenLinksAuditor.audit(&page, &ctx).await;

    assert_eq!(
        findings.len(),
        2,
        "expected exactly 2 findings, got: {findings:?}"
    );

    let has_4xx = findings.iter().any(|f| {
        f.check_id == "links.broken-4xx" && f.severity == Severity::Warning && f.penalty == 10
    });
    let has_5xx = findings.iter().any(|f| {
        f.check_id == "links.broken-5xx" && f.severity == Severity::Critical && f.penalty == 20
    });

    assert!(has_4xx, "expected links.broken-4xx finding: {findings:?}");
    assert!(has_5xx, "expected links.broken-5xx finding: {findings:?}");
}

/// /head-hostile → 405 on HEAD, 200 on GET → no finding; both requests must be received
#[tokio::test]
async fn test_head_hostile_fallback() {
    let server = MockServer::start().await;

    Mock::given(method("HEAD"))
        .and(path("/head-hostile"))
        .respond_with(ResponseTemplate::new(405))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/head-hostile"))
        .respond_with(ResponseTemplate::new(200).set_body_string("<html></html>"))
        .mount(&server)
        .await;

    let config = make_config(&server);
    let base_url = format!("{}/", server.uri());
    let html = format!(r#"<a href="{}/head-hostile">hostile</a>"#, server.uri());
    let page = make_page(&base_url, &html);
    let fetcher = make_fetcher(&config);
    let ctx = AuditContext {
        config: Arc::new(config),
        fetcher: Arc::new(fetcher),
    };

    let findings = BrokenLinksAuditor.audit(&page, &ctx).await;

    assert!(
        findings.is_empty(),
        "expected no findings for HEAD-hostile/GET-200 link: {findings:?}"
    );

    // Verify both HEAD and GET requests were issued to the mock server.
    let received = server.received_requests().await.unwrap_or_default();
    let head_count = received
        .iter()
        .filter(|r| r.method == wiremock::http::Method::HEAD && r.url.path() == "/head-hostile")
        .count();
    let get_count = received
        .iter()
        .filter(|r| r.method == wiremock::http::Method::GET && r.url.path() == "/head-hostile")
        .count();
    assert_eq!(
        head_count, 1,
        "expected exactly 1 HEAD request to /head-hostile"
    );
    assert_eq!(
        get_count, 1,
        "expected exactly 1 GET request (fallback) to /head-hostile"
    );
}

/// Fragment, mailto, and external links (check_external_links=false) produce no findings
#[tokio::test]
async fn test_skipped_link_types_produce_no_findings() {
    let server = MockServer::start().await;

    let config = make_config(&server);
    let base_url = format!("{}/", server.uri());
    let html = r##"<a href="#section">section</a><a href="mailto:foo@bar.com">email</a><a href="http://external.example.org/page">external</a>"##;
    let page = make_page(&base_url, html);
    let fetcher = make_fetcher(&config);
    let ctx = AuditContext {
        config: Arc::new(config),
        fetcher: Arc::new(fetcher),
    };

    let findings = BrokenLinksAuditor.audit(&page, &ctx).await;

    assert!(
        findings.is_empty(),
        "fragment/mailto/external links should produce no findings: {findings:?}"
    );
}
