//! Integration tests for robots.txt crawl gating.
//!
//! Tests (a), (b), (c), (d), (e) require a running Redis instance and are
//! marked `#[ignore]`.  Run with:
//!   REDIS_URL=redis://127.0.0.1:6379 cargo test --test crawler_robots_gate -- --ignored

#[cfg(test)]
mod tests {
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    fn make_config(
        server: &MockServer,
        depth: u32,
        redis_url: Option<String>,
        respect_robots: bool,
    ) -> sandslash::config::CrawlConfig {
        sandslash::config::CrawlConfig {
            root: format!("{}/", server.uri())
                .parse()
                .expect("mock URI must be valid"),
            depth,
            concurrency: 1,
            rate_per_host: 100,
            redis_url,
            user_agent: "Sandslash/0.4 (+https://github.com/leotonezi/sandslash)".to_owned(),
            timeout_secs: 5,
            max_pages: None,
            respect_robots,
            quiet: true,
            no_color: true,
            output_json: None,
            check_external_links: false,
        }
    }

    // ── (a) Disallowed path receives 0 requests ───────────────────────────────

    /// robots.txt `Disallow: /private`, root page links to both `/public` and
    /// `/private`, `respect_robots=true`, depth=1.
    ///
    /// Expected: `/private` receives 0 requests, `/public` receives ≥ 1.
    ///
    /// Requires a live Redis instance.  Set `REDIS_URL` env var to enable.
    #[tokio::test]
    #[ignore = "requires Redis (set REDIS_URL=redis://127.0.0.1:6379)"]
    async fn disallowed_path_receives_zero_requests() {
        let redis_url = match std::env::var("REDIS_URL") {
            Ok(u) => u,
            Err(_) => return,
        };

        let server = MockServer::start().await;

        // robots.txt: Disallow: /private
        Mock::given(method("GET"))
            .and(path("/robots.txt"))
            .respond_with(
                ResponseTemplate::new(200).set_body_string("User-agent: *\nDisallow: /private\n"),
            )
            .mount(&server)
            .await;

        let root_html = format!(
            r#"<!DOCTYPE html>
<html><head><title>Root</title><meta name="description" content="root"></head>
<body><h1>Root</h1>
<a href="{base}/public">Public</a>
<a href="{base}/private">Private</a>
</body></html>"#,
            base = server.uri()
        );

        Mock::given(method("GET"))
            .and(path("/"))
            .respond_with(
                ResponseTemplate::new(200)
                    .insert_header("content-type", "text/html; charset=utf-8")
                    .set_body_string(root_html),
            )
            .mount(&server)
            .await;

        Mock::given(method("GET"))
            .and(path("/public"))
            .respond_with(
                ResponseTemplate::new(200)
                    .insert_header("content-type", "text/html; charset=utf-8")
                    .set_body_string(
                        "<html><head><title>Public</title><meta name=\"description\" content=\"pub\"></head><body><h1>Public</h1></body></html>",
                    ),
            )
            .mount(&server)
            .await;

        // /private must NOT be requested.
        Mock::given(method("GET"))
            .and(path("/private"))
            .respond_with(ResponseTemplate::new(200).set_body_string(
                "<html><head><title>Private</title><meta name=\"description\" content=\"priv\"></head><body><h1>Private</h1></body></html>",
            ))
            .expect(0_u64)
            .mount(&server)
            .await;

        // sitemap.xml — 404 so auditor skips.
        Mock::given(method("GET"))
            .and(path("/sitemap.xml"))
            .respond_with(ResponseTemplate::new(404))
            .mount(&server)
            .await;

        let config = make_config(&server, 1, Some(redis_url), true);
        let report = sandslash::pipeline::run(config)
            .await
            .expect("pipeline must not fail");

        // /public must have been crawled (appears in pages).
        let urls: Vec<&str> = report.pages.iter().map(|p| p.url.as_str()).collect();
        let has_public = urls.iter().any(|u| u.contains("/public"));
        assert!(
            has_public,
            "/public must have been crawled; got pages: {urls:?}"
        );

        // wiremock will fail the test if /private received any request (expect(0)).
        server.verify().await;
    }

    // ── (b) Crawl-delay enforced ──────────────────────────────────────────────

    /// robots.txt `Crawl-delay: 1`, two same-host URLs → wall-clock ≥ 1s.
    ///
    /// Requires a live Redis instance.  Set `REDIS_URL` env var to enable.
    #[tokio::test]
    #[ignore = "requires Redis (set REDIS_URL=redis://127.0.0.1:6379)"]
    async fn crawl_delay_enforced() {
        let redis_url = match std::env::var("REDIS_URL") {
            Ok(u) => u,
            Err(_) => return,
        };

        let server = MockServer::start().await;

        Mock::given(method("GET"))
            .and(path("/robots.txt"))
            .respond_with(
                ResponseTemplate::new(200).set_body_string("User-agent: *\nCrawl-delay: 1\n"),
            )
            .mount(&server)
            .await;

        let root_html = format!(
            r#"<!DOCTYPE html>
<html><head><title>Root</title><meta name="description" content="root"></head>
<body><h1>Root</h1><a href="{base}/page2">Page 2</a></body></html>"#,
            base = server.uri()
        );

        Mock::given(method("GET"))
            .and(path("/"))
            .respond_with(
                ResponseTemplate::new(200)
                    .insert_header("content-type", "text/html; charset=utf-8")
                    .set_body_string(root_html),
            )
            .mount(&server)
            .await;

        Mock::given(method("GET"))
            .and(path("/page2"))
            .respond_with(
                ResponseTemplate::new(200)
                    .insert_header("content-type", "text/html; charset=utf-8")
                    .set_body_string(
                        "<html><head><title>P2</title><meta name=\"description\" content=\"p2\"></head><body><h1>P2</h1></body></html>",
                    ),
            )
            .mount(&server)
            .await;

        Mock::given(method("GET"))
            .and(path("/sitemap.xml"))
            .respond_with(ResponseTemplate::new(404))
            .mount(&server)
            .await;

        let config = make_config(&server, 1, Some(redis_url), true);

        let start = std::time::Instant::now();
        let _report = sandslash::pipeline::run(config)
            .await
            .expect("pipeline must not fail");
        let elapsed = start.elapsed();

        assert!(
            elapsed >= std::time::Duration::from_millis(900),
            "expected crawl_delay of 1s to be enforced; elapsed was only {elapsed:?}"
        );
    }

    // ── (c) respect_robots=false bypasses gating ──────────────────────────────

    /// `respect_robots=false` → disallowed path IS fetched.
    ///
    /// Requires a live Redis instance.  Set `REDIS_URL` env var to enable.
    #[tokio::test]
    #[ignore = "requires Redis (set REDIS_URL=redis://127.0.0.1:6379)"]
    async fn respect_robots_false_fetches_disallowed_path() {
        let redis_url = match std::env::var("REDIS_URL") {
            Ok(u) => u,
            Err(_) => return,
        };

        let server = MockServer::start().await;

        Mock::given(method("GET"))
            .and(path("/robots.txt"))
            .respond_with(
                ResponseTemplate::new(200).set_body_string("User-agent: *\nDisallow: /private\n"),
            )
            .mount(&server)
            .await;

        let root_html = format!(
            r#"<!DOCTYPE html>
<html><head><title>Root</title><meta name="description" content="root"></head>
<body><h1>Root</h1><a href="{base}/private">Private</a></body></html>"#,
            base = server.uri()
        );

        Mock::given(method("GET"))
            .and(path("/"))
            .respond_with(
                ResponseTemplate::new(200)
                    .insert_header("content-type", "text/html; charset=utf-8")
                    .set_body_string(root_html),
            )
            .mount(&server)
            .await;

        // /private MUST be requested (respect_robots=false bypasses gating).
        Mock::given(method("GET"))
            .and(path("/private"))
            .respond_with(
                ResponseTemplate::new(200)
                    .insert_header("content-type", "text/html; charset=utf-8")
                    .set_body_string(
                        "<html><head><title>Private</title><meta name=\"description\" content=\"priv\"></head><body><h1>Private</h1></body></html>",
                    ),
            )
            .expect(1_u64..)
            .mount(&server)
            .await;

        Mock::given(method("GET"))
            .and(path("/sitemap.xml"))
            .respond_with(ResponseTemplate::new(404))
            .mount(&server)
            .await;

        let config = make_config(&server, 1, Some(redis_url), false);
        let _report = sandslash::pipeline::run(config)
            .await
            .expect("pipeline must not fail");

        server.verify().await;
    }

    // ── (d) robots.txt 404 → all paths allowed ────────────────────────────────

    /// robots.txt returns 404 → all paths must be allowed (cache "allow-all").
    ///
    /// Requires a live Redis instance.  Set `REDIS_URL` env var to enable.
    #[tokio::test]
    #[ignore = "requires Redis (set REDIS_URL=redis://127.0.0.1:6379)"]
    async fn robots_404_allows_all_paths() {
        let redis_url = match std::env::var("REDIS_URL") {
            Ok(u) => u,
            Err(_) => return,
        };

        let server = MockServer::start().await;

        Mock::given(method("GET"))
            .and(path("/robots.txt"))
            .respond_with(ResponseTemplate::new(404))
            .mount(&server)
            .await;

        let root_html = format!(
            r#"<!DOCTYPE html>
<html><head><title>Root</title><meta name="description" content="root"></head>
<body><h1>Root</h1><a href="{base}/anything">Anything</a></body></html>"#,
            base = server.uri()
        );

        Mock::given(method("GET"))
            .and(path("/"))
            .respond_with(
                ResponseTemplate::new(200)
                    .insert_header("content-type", "text/html; charset=utf-8")
                    .set_body_string(root_html),
            )
            .mount(&server)
            .await;

        Mock::given(method("GET"))
            .and(path("/anything"))
            .respond_with(
                ResponseTemplate::new(200)
                    .insert_header("content-type", "text/html; charset=utf-8")
                    .set_body_string(
                        "<html><head><title>Anything</title><meta name=\"description\" content=\"any\"></head><body><h1>Anything</h1></body></html>",
                    ),
            )
            .expect(1_u64..)
            .mount(&server)
            .await;

        Mock::given(method("GET"))
            .and(path("/sitemap.xml"))
            .respond_with(ResponseTemplate::new(404))
            .mount(&server)
            .await;

        let config = make_config(&server, 1, Some(redis_url), true);
        let _report = sandslash::pipeline::run(config)
            .await
            .expect("pipeline must not fail");

        server.verify().await;
    }

    // ── (e) robots.txt fetched exactly once per host ──────────────────────────

    /// robots.txt must be fetched exactly once across multiple workers, even
    /// when many URLs from the same host are crawled concurrently.
    ///
    /// Requires a live Redis instance.  Set `REDIS_URL` env var to enable.
    #[tokio::test]
    #[ignore = "requires Redis (set REDIS_URL=redis://127.0.0.1:6379)"]
    async fn robots_txt_fetched_exactly_once_across_workers() {
        let redis_url = match std::env::var("REDIS_URL") {
            Ok(u) => u,
            Err(_) => return,
        };

        let server = MockServer::start().await;

        // robots.txt must be hit exactly 1 time.
        Mock::given(method("GET"))
            .and(path("/robots.txt"))
            .respond_with(ResponseTemplate::new(200).set_body_string("User-agent: *\nAllow: /\n"))
            .expect(1_u64)
            .mount(&server)
            .await;

        // Root links to 4 children.
        let root_html = format!(
            r#"<!DOCTYPE html>
<html><head><title>Root</title><meta name="description" content="root"></head>
<body><h1>Root</h1>
<a href="{base}/p1">P1</a>
<a href="{base}/p2">P2</a>
<a href="{base}/p3">P3</a>
<a href="{base}/p4">P4</a>
</body></html>"#,
            base = server.uri()
        );

        Mock::given(method("GET"))
            .and(path("/"))
            .respond_with(
                ResponseTemplate::new(200)
                    .insert_header("content-type", "text/html; charset=utf-8")
                    .set_body_string(root_html),
            )
            .mount(&server)
            .await;

        for i in 1..=4u32 {
            Mock::given(method("GET"))
                .and(path(format!("/p{i}")))
                .respond_with(
                    ResponseTemplate::new(200)
                        .insert_header("content-type", "text/html; charset=utf-8")
                        .set_body_string(format!(
                            "<html><head><title>P{i}</title><meta name=\"description\" content=\"p{i}\"></head><body><h1>P{i}</h1></body></html>"
                        )),
                )
                .mount(&server)
                .await;
        }

        Mock::given(method("GET"))
            .and(path("/sitemap.xml"))
            .respond_with(ResponseTemplate::new(404))
            .mount(&server)
            .await;

        // Use 4 concurrent workers, depth=1.
        let mut config = make_config(&server, 1, Some(redis_url), true);
        config.concurrency = 4;

        let _report = sandslash::pipeline::run(config)
            .await
            .expect("pipeline must not fail");

        // wiremock will fail the test if robots.txt was requested != 1 times.
        server.verify().await;
    }
}
