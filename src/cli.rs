use clap::Parser;
use std::path::PathBuf;

use sandslash::config::CrawlConfig;
use sandslash::error::{Result, SeoError};

#[derive(Parser, Debug)]
#[command(name = "sandslash", version, about = "SEO audit CLI")]
pub struct Cli {
    /// Target URL to audit.
    pub url: String,

    /// Crawl depth (0 = single page).
    #[arg(short, long, default_value_t = 1)]
    pub depth: u32,

    /// Number of concurrent workers.
    #[arg(short = 'c', long, default_value_t = 8)]
    pub concurrency: usize,

    /// Requests per second per host.
    #[arg(long, default_value_t = 2)]
    pub rate: u32,

    /// Redis URL for the crawl frontier.
    #[arg(long, env = "REDIS_URL")]
    pub redis_url: Option<String>,

    /// Custom User-Agent string.
    #[arg(long)]
    pub user_agent: Option<String>,

    /// Request timeout in seconds.
    #[arg(long, default_value_t = 30)]
    pub timeout: u64,

    /// Maximum pages to crawl.
    #[arg(long)]
    pub max_pages: Option<usize>,

    /// Do not respect robots.txt.
    #[arg(long, default_value_t = false)]
    pub ignore_robots: bool,

    /// Print only the final score (machine-readable).
    #[arg(short, long)]
    pub quiet: bool,

    /// Disable colored output.
    #[arg(long)]
    pub no_color: bool,

    /// Write JSON report to file (stdout if omitted).
    #[arg(short = 'o', long)]
    pub output: Option<PathBuf>,
}

impl Cli {
    pub fn into_config(self) -> Result<CrawlConfig> {
        let root = self.url.parse().map_err(SeoError::Url)?;
        Ok(CrawlConfig {
            root,
            depth: self.depth,
            concurrency: self.concurrency,
            rate_per_host: self.rate,
            redis_url: self.redis_url,
            user_agent: self
                .user_agent
                .unwrap_or_else(|| CrawlConfig::DEFAULT_UA.to_owned()),
            timeout_secs: self.timeout,
            max_pages: self.max_pages,
            respect_robots: !self.ignore_robots,
            quiet: self.quiet,
            no_color: self.no_color,
            output_json: self.output,
        })
    }
}
