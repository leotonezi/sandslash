//! Integration test for the multi-page crawler pipeline.
//!
//! Requires a running Redis instance.  Run with:
//!   REDIS_URL=redis://127.0.0.1:6379 cargo test --test crawler_pipeline -- --ignored

#[cfg(test)]
mod tests {
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    /// Star topology: root links to /page1, /page2, /page3, /page4.
    /// With depth=1, all five pages must be visited and returned.
    ///
    /// Requires a live Redis instance.  Set `REDIS_URL` env var to enable.
    #[tokio::test]
    #[ignore = "requires Redis (set REDIS_URL=redis://127.0.0.1:6379)"]
    async fn crawler_visits_all_five_pages() {
        let redis_url = match std::env::var("REDIS_URL") {
            Ok(u) => u,
            Err(_) => return, // skip silently if env not set
        };

        let mock_server = MockServer::start().await;

        // Root page links to 4 children.
        let root_html = format!(
            r#"<!DOCTYPE html>
<html>
<head><title>Root</title><meta name="description" content="root page"></head>
<body>
  <h1>Root</h1>
  <a href="{base}/page1">P1</a>
  <a href="{base}/page2">P2</a>
  <a href="{base}/page3">P3</a>
  <a href="{base}/page4">P4</a>
</body>
</html>"#,
            base = mock_server.uri()
        );

        Mock::given(method("GET"))
            .and(path("/"))
            .respond_with(
                ResponseTemplate::new(200)
                    .insert_header("content-type", "text/html; charset=utf-8")
                    .set_body_string(root_html),
            )
            .mount(&mock_server)
            .await;

        for i in 1..=4u32 {
            Mock::given(method("GET"))
                .and(path(format!("/page{i}")))
                .respond_with(
                    ResponseTemplate::new(200)
                        .insert_header("content-type", "text/html; charset=utf-8")
                        .set_body_string(format!(
                            r#"<!DOCTYPE html>
<html>
<head><title>Page {i}</title><meta name="description" content="page {i}"></head>
<body><h1>Page {i}</h1></body>
</html>"#
                        )),
                )
                .mount(&mock_server)
                .await;
        }

        // robots.txt — return 404 so the robots auditor skips gracefully.
        Mock::given(method("GET"))
            .and(path("/robots.txt"))
            .respond_with(ResponseTemplate::new(404))
            .mount(&mock_server)
            .await;

        // sitemap.xml — return 404 so the sitemap auditor skips gracefully.
        Mock::given(method("GET"))
            .and(path("/sitemap.xml"))
            .respond_with(ResponseTemplate::new(404))
            .mount(&mock_server)
            .await;

        let root_url = url::Url::parse(&format!("{}/", mock_server.uri()))
            .expect("mock server URI must be a valid URL");

        let config = sandslash::config::CrawlConfig {
            root: root_url,
            depth: 1,
            concurrency: 2,
            rate_per_host: 10,
            redis_url: Some(redis_url),
            user_agent: "seo-rs-test/0.1".to_owned(),
            timeout_secs: 10,
            max_pages: None,
            respect_robots: false,
            quiet: true,
            no_color: true,
            verbose: false,
            output_json: None,
            check_external_links: false,
        };

        let report = sandslash::pipeline::run(config)
            .await
            .expect("pipeline must not fail");

        assert_eq!(
            report.pages.len(),
            5,
            "expected 5 pages (root + 4 children), got {}: {:?}",
            report.pages.len(),
            report
                .pages
                .iter()
                .map(|p| p.url.as_str())
                .collect::<Vec<_>>()
        );
    }
}
