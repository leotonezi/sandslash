use sandslash::audit::robots::RobotsAuditor;
use sandslash::audit::{AuditContext, SiteAuditor};
use sandslash::config::CrawlConfig;
use sandslash::fetcher::Fetcher;
use sandslash::model::{Headers, PageData, Severity};
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

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
        rate_per_host: 10,
        redis_url: None,
        user_agent: "test-agent".to_owned(),
        timeout_secs: 10,
        max_pages: None,
        respect_robots: false,
        quiet: false,
        no_color: true,
        output_json: None,
    }
}

/// robots.txt returns 404 → robots.missing Warning
#[tokio::test]
async fn robots_404_emits_missing_warning() {
    let server = MockServer::start().await;

    Mock::given(method("GET"))
        .and(path("/robots.txt"))
        .respond_with(ResponseTemplate::new(404).set_body_string("Not Found"))
        .mount(&server)
        .await;

    let config = make_config(&server);
    let fetcher = Fetcher::new(&config).unwrap();
    let ctx = AuditContext {
        config: &config,
        fetcher: &fetcher,
    };
    let page = make_page(&format!("{}/", server.uri()));

    let findings = RobotsAuditor.audit(&page, &ctx).await;

    assert_eq!(findings.len(), 1, "expected 1 finding, got: {findings:?}");
    assert_eq!(findings[0].check_id, "robots.missing");
    assert_eq!(findings[0].severity, Severity::Warning);
    assert_eq!(findings[0].penalty, 15);
}

/// robots.txt has Disallow: / for User-agent: * → robots.disallow-all Critical
#[tokio::test]
async fn robots_disallow_all_emits_critical() {
    let server = MockServer::start().await;

    let body = "User-agent: *\nDisallow: /\nSitemap: https://example.com/sitemap.xml\n";
    Mock::given(method("GET"))
        .and(path("/robots.txt"))
        .respond_with(ResponseTemplate::new(200).set_body_string(body))
        .mount(&server)
        .await;

    let config = make_config(&server);
    let fetcher = Fetcher::new(&config).unwrap();
    let ctx = AuditContext {
        config: &config,
        fetcher: &fetcher,
    };
    let page = make_page(&format!("{}/", server.uri()));

    let findings = RobotsAuditor.audit(&page, &ctx).await;

    let ids: Vec<&str> = findings.iter().map(|f| f.check_id).collect();
    assert!(
        ids.contains(&"robots.disallow-all"),
        "expected robots.disallow-all, got: {findings:?}"
    );

    let disallow_finding = findings
        .iter()
        .find(|f| f.check_id == "robots.disallow-all")
        .unwrap();
    assert_eq!(disallow_finding.severity, Severity::Critical);
    assert_eq!(disallow_finding.penalty, 40);

    // Sitemap is present so no robots.no-sitemap
    assert!(
        !ids.contains(&"robots.no-sitemap"),
        "must not emit robots.no-sitemap when Sitemap: is present"
    );
}

/// robots.txt is 200 but has no Sitemap: directive → robots.no-sitemap Info
#[tokio::test]
async fn robots_no_sitemap_emits_info() {
    let server = MockServer::start().await;

    let body = "User-agent: *\nAllow: /\n";
    Mock::given(method("GET"))
        .and(path("/robots.txt"))
        .respond_with(ResponseTemplate::new(200).set_body_string(body))
        .mount(&server)
        .await;

    let config = make_config(&server);
    let fetcher = Fetcher::new(&config).unwrap();
    let ctx = AuditContext {
        config: &config,
        fetcher: &fetcher,
    };
    let page = make_page(&format!("{}/", server.uri()));

    let findings = RobotsAuditor.audit(&page, &ctx).await;

    assert_eq!(findings.len(), 1, "expected 1 finding, got: {findings:?}");
    assert_eq!(findings[0].check_id, "robots.no-sitemap");
    assert_eq!(findings[0].severity, Severity::Info);
    assert_eq!(findings[0].penalty, 5);
}

/// Clean robots.txt (2xx, Sitemap: present, no Disallow: /) → zero findings
#[tokio::test]
async fn robots_clean_emits_no_findings() {
    let server = MockServer::start().await;

    let body = "User-agent: *\nAllow: /\nSitemap: https://example.com/sitemap.xml\n";
    Mock::given(method("GET"))
        .and(path("/robots.txt"))
        .respond_with(ResponseTemplate::new(200).set_body_string(body))
        .mount(&server)
        .await;

    let config = make_config(&server);
    let fetcher = Fetcher::new(&config).unwrap();
    let ctx = AuditContext {
        config: &config,
        fetcher: &fetcher,
    };
    let page = make_page(&format!("{}/", server.uri()));

    let findings = RobotsAuditor.audit(&page, &ctx).await;

    assert!(
        findings.is_empty(),
        "expected no findings for clean robots.txt, got: {findings:?}"
    );
}
