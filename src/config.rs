use std::path::PathBuf;
use url::Url;

#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct CrawlConfig {
    pub root: Url,
    pub depth: u32,
    pub concurrency: usize,
    pub rate_per_host: u32,
    pub redis_url: Option<String>,
    pub user_agent: String,
    pub timeout_secs: u64,
    pub max_pages: Option<usize>,
    pub respect_robots: bool,
    pub quiet: bool,
    pub no_color: bool,
    pub output_json: Option<PathBuf>,
}

impl CrawlConfig {
    pub const DEFAULT_UA: &'static str = concat!(
        "seo-rs/",
        env!("CARGO_PKG_VERSION"),
        " (+https://github.com/leonardotonezi/seo-rs)"
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_ua_contains_version() {
        assert!(CrawlConfig::DEFAULT_UA.starts_with("seo-rs/"));
        assert!(CrawlConfig::DEFAULT_UA.contains(env!("CARGO_PKG_VERSION")));
    }
}
