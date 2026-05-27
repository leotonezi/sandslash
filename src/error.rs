use thiserror::Error;

#[derive(Error, Debug)]
#[allow(dead_code)]
pub enum SeoError {
    #[error("HTTP error fetching {url}: {source}")]
    Fetch {
        url: String,
        #[source]
        source: reqwest::Error,
    },
    #[error("URL parse error: {0}")]
    Url(#[from] url::ParseError),
    #[error("HTML parse error: {0}")]
    Parse(String),
    #[error("Redis error: {0}")]
    Redis(#[from] redis::RedisError),
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    #[error("Config error: {0}")]
    Config(String),
    #[error("Redirect loop detected at {url} after {hops} hops")]
    RedirectLoop { url: String, hops: usize },
    #[error("Robots.txt disallows {0}")]
    RobotsDisallowed(String),
}

pub type Result<T> = std::result::Result<T, SeoError>;
