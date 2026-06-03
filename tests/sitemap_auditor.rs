use sandslash::audit::sitemap::SitemapAuditor;
use sandslash::audit::{AuditContext, SiteAuditor};
use sandslash::config::CrawlConfig;
use sandslash::fetcher::{Fetcher, HostRateLimiter};
use sandslash::model::{Headers, PageData, Severity};
use std::num::NonZeroU32;
use std::sync::Arc;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

fn make_fetcher(config: &CrawlConfig) -> Fetcher {
    let qps = NonZeroU32::new(1000).expect("invariant: 1000 != 0");
    let rl = Arc::new(HostRateLimiter::new(qps));
    Fetcher::new(config, rl).expect("Fetcher::new must succeed in tests")
}

fn make_page(base_url: &str) -> PageData {
    PageData {
        url: base_url.parse().unwrap(),
        status: 200,
        redirect_chain: vec![],
        html: String::new(),
        headers: Headers::default(),
        depth: 0,
    }
}

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
        verbose: false,
        output_json: None,
        check_external_links: false,
    }
}

const VALID_SITEMAP: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<urlset xmlns="http://www.sitemaps.org/schemas/sitemap/0.9">
  <url><loc>https://example.com/</loc></url>
</urlset>"#;

const TRUNCATED_SITEMAP: &str =
    r#"<?xml version="1.0" encoding="UTF-8"?><urlset><url><loc>https://example.com/</loc>"#;

/// Well-formed sitemap at /sitemap.xml → zero findings.
#[tokio::test]
async fn valid_sitemap_emits_no_findings() {
    let server = MockServer::start().await;

    // robots.txt returns 404 so auditor falls back to /sitemap.xml
    Mock::given(method("GET"))
        .and(path("/robots.txt"))
        .respond_with(ResponseTemplate::new(404))
        .mount(&server)
        .await;

    Mock::given(method("GET"))
        .and(path("/sitemap.xml"))
        .respond_with(ResponseTemplate::new(200).set_body_string(VALID_SITEMAP))
        .mount(&server)
        .await;

    let config = make_config(&server);
    let fetcher = make_fetcher(&config);
    let ctx = AuditContext {
        config: std::sync::Arc::new(config.clone()),
        fetcher: std::sync::Arc::new(fetcher),
    };
    let page = make_page(&format!("{}/", server.uri()));

    let findings = SitemapAuditor.audit(&page, &ctx).await;

    assert!(
        findings.is_empty(),
        "expected 0 findings for valid sitemap, got: {findings:?}"
    );
}

/// robots.txt contains `Sitemap:` pointing to a valid XML → correct URL used,
/// zero findings.
#[tokio::test]
async fn robots_sitemap_directive_is_used() {
    let server = MockServer::start().await;

    let robots_body = format!(
        "User-agent: *\nAllow: /\nSitemap: {}/custom-sitemap.xml\n",
        server.uri()
    );

    Mock::given(method("GET"))
        .and(path("/robots.txt"))
        .respond_with(ResponseTemplate::new(200).set_body_string(robots_body))
        .mount(&server)
        .await;

    Mock::given(method("GET"))
        .and(path("/custom-sitemap.xml"))
        .respond_with(ResponseTemplate::new(200).set_body_string(VALID_SITEMAP))
        .mount(&server)
        .await;

    // /sitemap.xml must NOT be called; leave it un-mocked so wiremock would
    // return 404 if called.

    let config = make_config(&server);
    let fetcher = make_fetcher(&config);
    let ctx = AuditContext {
        config: std::sync::Arc::new(config.clone()),
        fetcher: std::sync::Arc::new(fetcher),
    };
    let page = make_page(&format!("{}/", server.uri()));

    let findings = SitemapAuditor.audit(&page, &ctx).await;

    assert!(
        findings.is_empty(),
        "expected 0 findings when sitemap from robots.txt is valid, got: {findings:?}"
    );
}

/// sitemap.xml returns 404 → one `sitemap.missing` Warning finding.
#[tokio::test]
async fn missing_sitemap_emits_warning() {
    let server = MockServer::start().await;

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

    let config = make_config(&server);
    let fetcher = make_fetcher(&config);
    let ctx = AuditContext {
        config: std::sync::Arc::new(config.clone()),
        fetcher: std::sync::Arc::new(fetcher),
    };
    let page = make_page(&format!("{}/", server.uri()));

    let findings = SitemapAuditor.audit(&page, &ctx).await;

    assert_eq!(findings.len(), 1, "expected 1 finding, got: {findings:?}");
    assert_eq!(findings[0].check_id, "sitemap.missing");
    assert_eq!(findings[0].severity, Severity::Warning);
    assert_eq!(findings[0].penalty, 5);
}

/// Truncated (malformed) XML → one `sitemap.malformed` Critical finding.
#[tokio::test]
async fn truncated_sitemap_emits_critical() {
    let server = MockServer::start().await;

    Mock::given(method("GET"))
        .and(path("/robots.txt"))
        .respond_with(ResponseTemplate::new(404))
        .mount(&server)
        .await;

    Mock::given(method("GET"))
        .and(path("/sitemap.xml"))
        .respond_with(ResponseTemplate::new(200).set_body_string(TRUNCATED_SITEMAP))
        .mount(&server)
        .await;

    let config = make_config(&server);
    let fetcher = make_fetcher(&config);
    let ctx = AuditContext {
        config: std::sync::Arc::new(config.clone()),
        fetcher: std::sync::Arc::new(fetcher),
    };
    let page = make_page(&format!("{}/", server.uri()));

    let findings = SitemapAuditor.audit(&page, &ctx).await;

    assert_eq!(findings.len(), 1, "expected 1 finding, got: {findings:?}");
    assert_eq!(findings[0].check_id, "sitemap.malformed");
    assert_eq!(findings[0].severity, Severity::Critical);
    assert_eq!(findings[0].penalty, 20);
}

/// Non-2xx status (500) treated as missing, not malformed.
#[tokio::test]
async fn server_error_on_sitemap_emits_missing_not_malformed() {
    let server = MockServer::start().await;

    Mock::given(method("GET"))
        .and(path("/robots.txt"))
        .respond_with(ResponseTemplate::new(404))
        .mount(&server)
        .await;

    Mock::given(method("GET"))
        .and(path("/sitemap.xml"))
        .respond_with(ResponseTemplate::new(500).set_body_string("Internal Server Error"))
        .mount(&server)
        .await;

    let config = make_config(&server);
    let fetcher = make_fetcher(&config);
    let ctx = AuditContext {
        config: std::sync::Arc::new(config.clone()),
        fetcher: std::sync::Arc::new(fetcher),
    };
    let page = make_page(&format!("{}/", server.uri()));

    let findings = SitemapAuditor.audit(&page, &ctx).await;

    assert_eq!(findings.len(), 1, "expected 1 finding, got: {findings:?}");
    assert_eq!(findings[0].check_id, "sitemap.missing");
    assert_eq!(findings[0].severity, Severity::Warning);
}
