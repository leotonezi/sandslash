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
    /// Wall-clock timeout (seconds) for the entire crawl.  When elapsed the
    /// crawler is aborted and a partial `AuditReport` is returned.
    pub global_timeout_secs: Option<u64>,
    pub respect_robots: bool,
    pub validate_sitemap: bool,
    pub quiet: bool,
    pub no_color: bool,
    pub verbose: bool,
    pub output_json: Option<PathBuf>,
    pub check_external_links: bool,
}

impl CrawlConfig {
    pub const DEFAULT_UA: &'static str = concat!(
        "sandslash/",
        env!("CARGO_PKG_VERSION"),
        " (+https://github.com/leotonezi/sandslash)"
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_ua_contains_version() {
        assert!(CrawlConfig::DEFAULT_UA.starts_with("sandslash/"));
        assert!(CrawlConfig::DEFAULT_UA.contains(env!("CARGO_PKG_VERSION")));
    }
}
