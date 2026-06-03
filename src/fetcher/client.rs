use reqwest::header::{HeaderMap, LOCATION};
use std::collections::HashSet;
use std::sync::Arc;
use std::time::Duration;
use tokio::time::sleep;
use url::Url;

use crate::config::CrawlConfig;
use crate::error::{Result, SeoError};
use crate::fetcher::HostRateLimiter;
use crate::model::{Headers, PageData};

/// Extract the `charset=` parameter from a Content-Type header value.
/// E.g. `"text/html; charset=windows-1252"` → `Some("windows-1252")`.
fn charset_from_content_type(ct: &str) -> Option<String> {
    let lower = ct.to_ascii_lowercase();
    let pos = lower.find("charset=")?;
    let after = lower[pos + "charset=".len()..].trim_start();
    // Strip optional quotes.
    let after = after.trim_start_matches('"').trim_start_matches('\'');
    // Stop at any token delimiter or ASCII whitespace.
    let end = after
        .find([';', '"', '\'', ' ', '\t', '\n', '\r'])
        .unwrap_or(after.len());
    let label = &after[..end];
    if label.is_empty() {
        None
    } else {
        Some(label.to_owned())
    }
}

/// Sniff the charset from the first 1024 bytes of an HTML body by scanning for
/// `<meta charset="..."` or `<meta http-equiv="content-type" content="...charset=..."`.
/// Uses `String::from_utf8_lossy` on the byte slice — we only look for ASCII patterns.
fn charset_from_meta_sniff(bytes: &[u8]) -> Option<String> {
    let sniff_len = bytes.len().min(1024);
    let snippet = String::from_utf8_lossy(&bytes[..sniff_len]).to_ascii_lowercase();

    // Pattern 1: <meta charset="VALUE" or <meta charset='VALUE'
    if let Some(pos) = snippet.find("charset=") {
        let after = snippet[pos + "charset=".len()..].trim_start().to_owned();
        let after = after.trim_start_matches('"').trim_start_matches('\'');
        let end = after
            .find(['"', '\'', ';', ' ', '>'])
            .unwrap_or(after.len());
        let label = &after[..end];
        if !label.is_empty() {
            return Some(label.to_owned());
        }
    }

    None
}

/// Decode `bytes` to a `String` using the charset resolved in this order:
/// 1. `charset=` param from Content-Type header value (`ct_header`).
/// 2. `<meta charset=...>` / `<meta http-equiv="content-type"...>` sniffed from first 1024 bytes.
/// 3. UTF-8 fallback.
///
/// Unknown charset labels fall back to UTF-8 without erroring.
/// Logs a warning when `encoding_rs` reports decoding errors (`had_errors == true`).
fn decode_body(bytes: &[u8], ct_header: Option<&str>, url: &Url) -> String {
    let charset = ct_header
        .and_then(charset_from_content_type)
        .or_else(|| charset_from_meta_sniff(bytes));

    let encoding = charset
        .as_deref()
        .and_then(|label| encoding_rs::Encoding::for_label(label.as_bytes()))
        .unwrap_or(encoding_rs::UTF_8);

    let (cow, _enc_used, had_errors) = encoding.decode(bytes);

    if had_errors {
        tracing::warn!(
            url = %url,
            charset = charset.as_deref().unwrap_or("utf-8"),
            "encoding_rs reported decoding errors; some characters may be replaced"
        );
    }

    cow.into_owned()
}

const MAX_REDIRECTS: usize = 10;
const MAX_RETRIES: u32 = 3;

pub struct Fetcher {
    client: reqwest::Client,
    rate_limiter: Arc<HostRateLimiter>,
}

impl Fetcher {
    pub fn new(config: &CrawlConfig, rate_limiter: Arc<HostRateLimiter>) -> Result<Self> {
        let client = reqwest::Client::builder()
            .user_agent(&config.user_agent)
            .timeout(Duration::from_secs(config.timeout_secs))
            .redirect(reqwest::redirect::Policy::none())
            .build()
            .map_err(|e| SeoError::Fetch {
                url: config.root.to_string(),
                source: e,
            })?;
        Ok(Self {
            client,
            rate_limiter,
        })
    }

    pub async fn fetch(&self, url: &Url) -> Result<PageData> {
        let mut current = url.clone();
        let mut chain: Vec<Url> = Vec::new();
        let mut seen: HashSet<Url> = HashSet::new();

        loop {
            // Detect cycle before fetching: if current was already visited, it's a loop.
            if !seen.insert(current.clone()) {
                return Err(SeoError::RedirectLoop {
                    url: current.to_string(),
                    hops: chain.len(),
                });
            }

            // Enforce hop limit before issuing the next request.
            if chain.len() >= MAX_REDIRECTS {
                return Err(SeoError::RedirectLoop {
                    url: current.to_string(),
                    hops: chain.len(),
                });
            }

            // Compute host once per hop; the token is acquired inside the retry loop.
            let host = current
                .host_str()
                .ok_or(SeoError::Url(url::ParseError::EmptyHost))?;

            // Retry loop for 429 / 503 responses — scoped to a single hop.
            let mut attempt = 0u32;
            let (resp_status, resp_headers, resp_html) =
                loop {
                    // Acquire rate-limit token before EACH attempt (initial + retries).
                    self.rate_limiter.acquire(host).await;

                    let resp = self.client.get(current.clone()).send().await.map_err(|e| {
                        SeoError::Fetch {
                            url: current.to_string(),
                            source: e,
                        }
                    })?;

                    let status_code = resp.status();
                    let status = status_code.as_u16();

                    // Handle redirects immediately — no retry logic for redirect responses.
                    if status_code.is_redirection()
                        && let Some(loc_header) = resp.headers().get(LOCATION)
                    {
                        let loc_str = loc_header
                            .to_str()
                            .map_err(|_| SeoError::Parse("non-ASCII Location header".into()))?;
                        let next = current.join(loc_str).map_err(SeoError::from)?;
                        chain.push(current.clone());
                        current = next;
                        drop(resp);
                        // Break out of the retry loop with a sentinel that signals "follow redirect".
                        break (0u16, Headers::default(), String::new());
                    }

                    // Check whether we should retry (429 / 503).
                    if (status == 429 || status == 503) && attempt < MAX_RETRIES {
                        // Read Retry-After header BEFORE consuming body.
                        let retry_after_secs = resp
                            .headers()
                            .get("retry-after")
                            .and_then(|v| v.to_str().ok())
                            .and_then(|s| s.parse::<u64>().ok())
                            .unwrap_or_else(|| 1u64 << attempt); // exponential: 1, 2, 4

                        drop(resp);
                        sleep(Duration::from_secs(retry_after_secs)).await;
                        attempt += 1;
                        continue;
                    }

                    // Terminal response (non-redirect, non-retried, or retries exhausted).
                    let headers = extract_headers(resp.headers());

                    // Capture Content-Type before consuming the body.
                    let ct_header: Option<String> = headers.get("content-type").cloned();

                    let bytes = resp.bytes().await.map_err(|e| SeoError::Fetch {
                        url: current.to_string(),
                        source: e,
                    })?;

                    let html = decode_body(&bytes, ct_header.as_deref(), &current);

                    break (status, headers, html);
                };

            // Sentinel value 0 means we followed a redirect — go back to the outer loop.
            if resp_status == 0 {
                continue;
            }

            return Ok(PageData {
                url: current,
                status: resp_status,
                redirect_chain: chain,
                html: resp_html,
                headers: resp_headers,
                depth: 0,
            });
        }
    }

    /// Issue a HEAD request to `url` and return the HTTP status code.
    ///
    /// Rate-limiting is applied before the request, same as [`fetch`].
    /// Does not consume the response body.
    pub async fn head(&self, url: &Url) -> Result<u16> {
        let host = url.host_str().unwrap_or("");
        self.rate_limiter.acquire(host).await;
        let resp = self
            .client
            .head(url.as_str())
            .send()
            .await
            .map_err(|e| SeoError::Fetch {
                url: url.to_string(),
                source: e,
            })?;
        Ok(resp.status().as_u16())
    }
}

fn extract_headers(map: &HeaderMap) -> Headers {
    map.iter()
        .filter_map(|(k, v)| {
            v.to_str()
                .ok()
                .map(|val| (k.as_str().to_lowercase(), val.to_owned()))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::num::NonZeroU32;
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    async fn test_fetcher(server: &MockServer) -> Fetcher {
        let config = CrawlConfig {
            root: format!("{}/", server.uri()).parse().unwrap(),
            depth: 0,
            concurrency: 1,
            rate_per_host: 1000,
            redis_url: None,
            user_agent: "test-agent".to_owned(),
            timeout_secs: 10,
            max_pages: None,
            global_timeout_secs: None,
            respect_robots: false,
            quiet: false,
            no_color: true,
            verbose: false,
            output_json: None,
            check_external_links: false,
        };
        let qps = NonZeroU32::new(1000).expect("invariant: 1000 != 0");
        let rate_limiter = Arc::new(HostRateLimiter::new(qps));
        Fetcher::new(&config, rate_limiter).unwrap()
    }

    #[tokio::test]
    async fn fetch_returns_status_and_html() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/"))
            .respond_with(
                ResponseTemplate::new(200)
                    .insert_header("content-type", "text/html; charset=utf-8")
                    .set_body_bytes(b"<html><body>hello</body></html>".as_ref()),
            )
            .mount(&server)
            .await;

        let fetcher = test_fetcher(&server).await;
        let url: Url = format!("{}/", server.uri()).parse().unwrap();
        let page = fetcher.fetch(&url).await.unwrap();

        assert_eq!(page.status, 200);
        assert!(page.html.contains("hello"));
        assert!(
            page.headers
                .get("content-type")
                .unwrap()
                .contains("text/html")
        );
    }

    #[tokio::test]
    async fn fetch_captures_non_200_status() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/missing"))
            .respond_with(ResponseTemplate::new(404).set_body_string("not found"))
            .mount(&server)
            .await;

        let fetcher = test_fetcher(&server).await;
        let url: Url = format!("{}/missing", server.uri()).parse().unwrap();
        let page = fetcher.fetch(&url).await.unwrap();

        assert_eq!(page.status, 404);
    }

    /// A 3-hop redirect chain (301 → 302 → 307 → 200) must populate redirect_chain
    /// with the three intermediate URLs and set the final page URL to /final.
    #[tokio::test]
    async fn fetch_follows_redirect_chain() {
        let server = MockServer::start().await;

        // /hop1 -301-> /hop2
        Mock::given(method("GET"))
            .and(path("/hop1"))
            .respond_with(ResponseTemplate::new(301).insert_header("location", "/hop2"))
            .mount(&server)
            .await;

        // /hop2 -302-> /hop3
        Mock::given(method("GET"))
            .and(path("/hop2"))
            .respond_with(ResponseTemplate::new(302).insert_header("location", "/hop3"))
            .mount(&server)
            .await;

        // /hop3 -307-> /final
        Mock::given(method("GET"))
            .and(path("/hop3"))
            .respond_with(ResponseTemplate::new(307).insert_header("location", "/final"))
            .mount(&server)
            .await;

        // /final -200->
        Mock::given(method("GET"))
            .and(path("/final"))
            .respond_with(
                ResponseTemplate::new(200)
                    .insert_header("content-type", "text/html; charset=utf-8")
                    .set_body_string("<html><body>done</body></html>"),
            )
            .mount(&server)
            .await;

        let fetcher = test_fetcher(&server).await;
        let start_url: Url = format!("{}/hop1", server.uri()).parse().unwrap();
        let page = fetcher.fetch(&start_url).await.unwrap();

        assert_eq!(page.status, 200);
        assert_eq!(page.redirect_chain.len(), 3, "expected 3 intermediate hops");
        assert!(
            page.url.path().ends_with("/final"),
            "final url should be /final, got {}",
            page.url
        );
        assert!(page.redirect_chain[0].path().ends_with("/hop1"));
        assert!(page.redirect_chain[1].path().ends_with("/hop2"));
        assert!(page.redirect_chain[2].path().ends_with("/hop3"));
    }

    /// A self-redirect (/loop → 301 Location: /loop) must return SeoError::RedirectLoop.
    #[tokio::test]
    async fn fetch_detects_redirect_loop() {
        let server = MockServer::start().await;

        Mock::given(method("GET"))
            .and(path("/loop"))
            .respond_with(ResponseTemplate::new(301).insert_header("location", "/loop"))
            .mount(&server)
            .await;

        let fetcher = test_fetcher(&server).await;
        let url: Url = format!("{}/loop", server.uri()).parse().unwrap();
        let result = fetcher.fetch(&url).await;

        assert!(
            matches!(result, Err(SeoError::RedirectLoop { .. })),
            "expected RedirectLoop, got {result:?}"
        );
    }

    /// wiremock returns 429 on the first call, 200 on the second call.
    /// Final status must be 200, and the mock must have been hit exactly 2 times.
    #[tokio::test]
    async fn fetch_retries_429_once_then_200() {
        let server = MockServer::start().await;

        // First call: 429, no Retry-After header (exponential backoff: 1s for attempt 0)
        // Second call: 200
        Mock::given(method("GET"))
            .and(path("/rate-limited"))
            .respond_with(ResponseTemplate::new(429))
            .up_to_n_times(1)
            .mount(&server)
            .await;

        Mock::given(method("GET"))
            .and(path("/rate-limited"))
            .respond_with(
                ResponseTemplate::new(200)
                    .insert_header("content-type", "text/html; charset=utf-8")
                    .set_body_string("<html><body>ok</body></html>"),
            )
            .mount(&server)
            .await;

        let fetcher = test_fetcher(&server).await;
        let url: Url = format!("{}/rate-limited", server.uri()).parse().unwrap();
        let page = fetcher.fetch(&url).await.unwrap();

        assert_eq!(page.status, 200);

        let received = server.received_requests().await.unwrap();
        assert_eq!(received.len(), 2, "expected exactly 2 requests (1 retry)");
    }

    /// wiremock returns 503 with Retry-After: 1 on first call, 200 on second.
    /// Final status must be 200.
    #[tokio::test]
    async fn fetch_retries_503_with_retry_after() {
        let server = MockServer::start().await;

        Mock::given(method("GET"))
            .and(path("/unavailable"))
            .respond_with(ResponseTemplate::new(503).insert_header("retry-after", "1"))
            .up_to_n_times(1)
            .mount(&server)
            .await;

        Mock::given(method("GET"))
            .and(path("/unavailable"))
            .respond_with(
                ResponseTemplate::new(200)
                    .insert_header("content-type", "text/html; charset=utf-8")
                    .set_body_string("<html><body>back</body></html>"),
            )
            .mount(&server)
            .await;

        let fetcher = test_fetcher(&server).await;
        let url: Url = format!("{}/unavailable", server.uri()).parse().unwrap();
        let start = std::time::Instant::now();
        let page = fetcher.fetch(&url).await.unwrap();
        let elapsed = start.elapsed();

        assert_eq!(page.status, 200);
        assert!(
            elapsed >= Duration::from_millis(900),
            "Retry-After: 1 must be honored; elapsed was only {elapsed:?}",
        );

        let received = server.received_requests().await.unwrap();
        assert_eq!(received.len(), 2, "expected exactly 2 requests (1 retry)");
    }

    /// wiremock returns 429 on all 4 attempts (initial + 3 retries).
    /// Must return Ok(PageData { status: 429 }). Mock must be called exactly 4 times.
    #[tokio::test]
    async fn fetch_exhausts_retries_returns_last_status() {
        let server = MockServer::start().await;

        Mock::given(method("GET"))
            .and(path("/always-limited"))
            .respond_with(ResponseTemplate::new(429))
            .mount(&server)
            .await;

        let fetcher = test_fetcher(&server).await;
        let url: Url = format!("{}/always-limited", server.uri()).parse().unwrap();
        let result = fetcher.fetch(&url).await;

        let page = result.expect("exhausted retries should return Ok, not Err");
        assert_eq!(page.status, 429, "final status should be 429");

        let received = server.received_requests().await.unwrap();
        assert_eq!(
            received.len(),
            4,
            "expected exactly 4 requests (1 initial + 3 retries)"
        );
    }

    // ── Charset / encoding tests ──────────────────────────────────────────────

    /// Shift_JIS body with no Content-Type charset — charset must be sniffed from
    /// the `<meta charset="Shift_JIS">` tag.
    #[tokio::test]
    async fn decodes_shift_jis_via_meta_sniff() {
        let server = MockServer::start().await;

        let html_str =
            "<html><head><meta charset=\"Shift_JIS\"></head><body>こんにちは</body></html>";
        let (encoded, _, _) = encoding_rs::SHIFT_JIS.encode(html_str);
        let body_bytes = encoded.into_owned();

        Mock::given(method("GET"))
            .and(path("/sjis"))
            .respond_with(
                ResponseTemplate::new(200)
                    .insert_header("content-type", "text/html")
                    .set_body_bytes(body_bytes),
            )
            .mount(&server)
            .await;

        let fetcher = test_fetcher(&server).await;
        let url: Url = format!("{}/sjis", server.uri()).parse().unwrap();
        let page = fetcher.fetch(&url).await.unwrap();

        assert!(
            page.html.contains("こんにちは"),
            "expected Japanese text in decoded html; got: {:?}",
            &page.html[..page.html.len().min(200)]
        );
    }

    /// Windows-1252 body with `charset=windows-1252` in Content-Type header.
    #[tokio::test]
    async fn decodes_windows_1252_via_content_type() {
        let server = MockServer::start().await;

        let html_str = "<html><body>café</body></html>";
        let (encoded, _, _) = encoding_rs::WINDOWS_1252.encode(html_str);
        let body_bytes = encoded.into_owned();

        Mock::given(method("GET"))
            .and(path("/win1252"))
            .respond_with(
                ResponseTemplate::new(200)
                    .insert_header("content-type", "text/html; charset=windows-1252")
                    .set_body_bytes(body_bytes),
            )
            .mount(&server)
            .await;

        let fetcher = test_fetcher(&server).await;
        let url: Url = format!("{}/win1252", server.uri()).parse().unwrap();
        let page = fetcher.fetch(&url).await.unwrap();

        assert!(
            page.html.contains("café"),
            "expected 'café' in decoded html; got: {:?}",
            &page.html[..page.html.len().min(200)]
        );
    }

    /// Plain UTF-8 body with no charset declared must decode correctly.
    #[tokio::test]
    async fn decodes_utf8_no_charset() {
        let server = MockServer::start().await;

        Mock::given(method("GET"))
            .and(path("/utf8"))
            .respond_with(
                ResponseTemplate::new(200)
                    .insert_header("content-type", "text/html")
                    .set_body_bytes(b"<html><body>hello world</body></html>".as_ref()),
            )
            .mount(&server)
            .await;

        let fetcher = test_fetcher(&server).await;
        let url: Url = format!("{}/utf8", server.uri()).parse().unwrap();
        let page = fetcher.fetch(&url).await.unwrap();

        assert!(page.html.contains("hello world"));
        assert_eq!(page.status, 200);
    }

    /// Body with an unrecognised charset label must not error; falls back to UTF-8.
    #[tokio::test]
    async fn unknown_charset_falls_back_to_utf8() {
        let server = MockServer::start().await;

        Mock::given(method("GET"))
            .and(path("/bogus"))
            .respond_with(
                ResponseTemplate::new(200)
                    .insert_header("content-type", "text/html; charset=bogus-encoding")
                    .set_body_bytes(b"<html><body>safe</body></html>".as_ref()),
            )
            .mount(&server)
            .await;

        let fetcher = test_fetcher(&server).await;
        let url: Url = format!("{}/bogus", server.uri()).parse().unwrap();
        // Must not return Err — unknown charset falls back gracefully.
        let page = fetcher.fetch(&url).await.unwrap();

        assert!(page.html.contains("safe"));
        assert_eq!(page.status, 200);
    }

    // ── Unit tests for helper functions ──────────────────────────────────────

    #[test]
    fn charset_from_content_type_extracts_label() {
        assert_eq!(
            charset_from_content_type("text/html; charset=windows-1252"),
            Some("windows-1252".to_owned())
        );
        assert_eq!(
            charset_from_content_type("text/html; charset=UTF-8"),
            Some("utf-8".to_owned())
        );
        assert_eq!(charset_from_content_type("text/html"), None);
    }

    #[test]
    fn charset_from_meta_sniff_finds_charset() {
        let html = b"<html><head><meta charset=\"Shift_JIS\"></head></html>";
        assert_eq!(charset_from_meta_sniff(html), Some("shift_jis".to_owned()));

        let html2 = b"<html><head><meta charset='windows-1252'></head></html>";
        assert_eq!(
            charset_from_meta_sniff(html2),
            Some("windows-1252".to_owned())
        );

        let html3 = b"<html><head></head><body>no charset here</body></html>";
        assert_eq!(charset_from_meta_sniff(html3), None);
    }
}
