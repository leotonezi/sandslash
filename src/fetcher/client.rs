use reqwest::header::{HeaderMap, LOCATION};
use std::collections::HashSet;
use std::time::Duration;
use url::Url;

use crate::config::CrawlConfig;
use crate::error::{Result, SeoError};
use crate::model::{Headers, PageData};

const MAX_REDIRECTS: usize = 10;

pub struct Fetcher {
    client: reqwest::Client,
}

impl Fetcher {
    pub fn new(config: &CrawlConfig) -> Result<Self> {
        let client = reqwest::Client::builder()
            .user_agent(&config.user_agent)
            .timeout(Duration::from_secs(config.timeout_secs))
            .redirect(reqwest::redirect::Policy::none())
            .build()
            .map_err(|e| SeoError::Fetch {
                url: config.root.to_string(),
                source: e,
            })?;
        Ok(Self { client })
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

            let resp =
                self.client
                    .get(current.clone())
                    .send()
                    .await
                    .map_err(|e| SeoError::Fetch {
                        url: current.to_string(),
                        source: e,
                    })?;

            let status_code = resp.status();
            let status = status_code.as_u16();

            if let (true, Some(loc_header)) =
                (status_code.is_redirection(), resp.headers().get(LOCATION))
            {
                let loc_str = loc_header
                    .to_str()
                    .map_err(|_| SeoError::Parse("non-ASCII Location header".into()))?;

                let next = current.join(loc_str).map_err(SeoError::from)?;

                chain.push(current.clone());
                current = next;
                drop(resp);
                continue;
            }

            // Terminal response (non-redirect, or redirect without Location).
            let headers = extract_headers(resp.headers());

            if !headers
                .get("content-type")
                .map(|ct| ct.contains("utf-8") || ct.contains("UTF-8") || ct.contains("text/html"))
                .unwrap_or(true)
            {
                tracing::warn!(url = %current, "non-UTF-8 content-type detected; decoding may be lossy");
            }

            let html = resp.text().await.map_err(|e| SeoError::Fetch {
                url: current.to_string(),
                source: e,
            })?;

            return Ok(PageData {
                url: current,
                status,
                redirect_chain: chain,
                html,
                headers,
                depth: 0,
            });
        }
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
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    async fn test_fetcher(server: &MockServer) -> Fetcher {
        let config = CrawlConfig {
            root: format!("{}/", server.uri()).parse().unwrap(),
            depth: 0,
            concurrency: 1,
            rate_per_host: 2,
            redis_url: None,
            user_agent: "test-agent".to_owned(),
            timeout_secs: 10,
            max_pages: None,
            respect_robots: false,
            quiet: false,
            no_color: true,
            output_json: None,
        };
        Fetcher::new(&config).unwrap()
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
}
