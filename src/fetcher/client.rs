use reqwest::header::HeaderMap;
use std::time::Duration;
use url::Url;

use crate::config::CrawlConfig;
use crate::error::{Result, SeoError};
use crate::model::{Headers, PageData};

pub struct Fetcher {
    client: reqwest::Client,
}

impl Fetcher {
    pub fn new(config: &CrawlConfig) -> Result<Self> {
        let client = reqwest::Client::builder()
            .user_agent(&config.user_agent)
            .timeout(Duration::from_secs(config.timeout_secs))
            .redirect(reqwest::redirect::Policy::limited(10))
            .build()
            .map_err(|e| SeoError::Fetch {
                url: config.root.to_string(),
                source: e,
            })?;
        Ok(Self { client })
    }

    pub async fn fetch(&self, url: &Url) -> Result<PageData> {
        let resp = self
            .client
            .get(url.clone())
            .send()
            .await
            .map_err(|e| SeoError::Fetch {
                url: url.to_string(),
                source: e,
            })?;

        let status = resp.status().as_u16();
        let headers = extract_headers(resp.headers());

        if !headers
            .get("content-type")
            .map(|ct| ct.contains("utf-8") || ct.contains("UTF-8") || ct.contains("text/html"))
            .unwrap_or(true)
        {
            tracing::warn!(url = %url, "non-UTF-8 content-type detected; decoding may be lossy");
        }

        let html = resp.text().await.map_err(|e| SeoError::Fetch {
            url: url.to_string(),
            source: e,
        })?;

        Ok(PageData {
            url: url.clone(),
            status,
            redirect_chain: vec![],
            html,
            headers,
            depth: 0,
        })
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
}
