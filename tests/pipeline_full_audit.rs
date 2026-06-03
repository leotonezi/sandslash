use sandslash::config::CrawlConfig;
use sandslash::pipeline;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

const HTML_MISSING_TITLE: &str = r#"<!DOCTYPE html>
<html>
<head>
  <meta name="description" content="A short desc" />
</head>
<body>
  <h1>Hello</h1>
  <img src="/image.png" />
</body>
</html>"#;

const ROBOTS_NO_SITEMAP: &str = "User-agent: *\nAllow: /\n";

#[tokio::test]
async fn pipeline_runs_all_auditors() {
    let server = MockServer::start().await;

    Mock::given(method("GET"))
        .and(path("/"))
        .respond_with(ResponseTemplate::new(200).set_body_string(HTML_MISSING_TITLE))
        .mount(&server)
        .await;

    // robots.txt present but missing Sitemap: directive → triggers robots.no-sitemap
    Mock::given(method("GET"))
        .and(path("/robots.txt"))
        .respond_with(ResponseTemplate::new(200).set_body_string(ROBOTS_NO_SITEMAP))
        .mount(&server)
        .await;

    // sitemap.xml returns 404 → triggers sitemap.missing
    Mock::given(method("GET"))
        .and(path("/sitemap.xml"))
        .respond_with(ResponseTemplate::new(404))
        .mount(&server)
        .await;

    let tmp = tempfile::NamedTempFile::new().expect("invariant: tempfile creation succeeds");
    let json_path = tmp.path().to_path_buf();

    let config = CrawlConfig {
        root: format!("{}/", server.uri())
            .parse()
            .expect("invariant: mock uri is valid URL"),
        depth: 0,
        concurrency: 1,
        rate_per_host: 10,
        redis_url: None,
        user_agent: "test-agent".to_owned(),
        timeout_secs: 10,
        max_pages: None,
        global_timeout_secs: None,
        respect_robots: false,
        validate_sitemap: false,
        quiet: false,
        no_color: true,
        verbose: false,
        output_json: Some(json_path.clone()),
        check_external_links: false,
    };

    let report = pipeline::run(config).await.expect("pipeline must succeed");

    assert_eq!(report.pages.len(), 1, "expected exactly one page report");
    assert!(
        !report.crawled_at.is_empty(),
        "crawled_at must be non-empty"
    );

    let all_findings = &report.pages[0].findings;

    let has_page_finding = all_findings
        .iter()
        .any(|f| !f.check_id.starts_with("robots.") && !f.check_id.starts_with("sitemap."));
    assert!(
        has_page_finding,
        "expected at least one page-auditor finding; got: {all_findings:?}"
    );

    let has_robots_finding = all_findings
        .iter()
        .any(|f| f.check_id.starts_with("robots."));
    assert!(
        has_robots_finding,
        "expected at least one robots.* finding; got: {all_findings:?}"
    );

    let has_sitemap_finding = all_findings
        .iter()
        .any(|f| f.check_id.starts_with("sitemap."));
    assert!(
        has_sitemap_finding,
        "expected at least one sitemap.* finding; got: {all_findings:?}"
    );

    // Read back the JSON output and verify it has the expected top-level keys.
    let json_bytes = std::fs::read(&json_path).expect("invariant: json output file exists");
    let value: serde_json::Value =
        serde_json::from_slice(&json_bytes).expect("invariant: output is valid JSON");

    assert!(value.get("pages").is_some(), "JSON must have 'pages' key");
    assert!(
        value.get("site_score").is_some(),
        "JSON must have 'site_score' key"
    );
    assert!(
        value.get("crawled_at").is_some(),
        "JSON must have 'crawled_at' key"
    );
    assert!(value.get("root").is_some(), "JSON must have 'root' key");
}
