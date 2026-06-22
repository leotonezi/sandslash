use clap::{Parser, Subcommand};
use std::net::SocketAddr;
use std::path::PathBuf;

use sandslash::config::CrawlConfig;
use sandslash::diff::OutputFormat;
use sandslash::error::{Result, SeoError};

#[derive(Parser, Debug)]
#[command(name = "sandslash", version, about = "SEO audit CLI")]
pub struct Cli {
    #[command(subcommand)]
    pub command: Command,
}

#[derive(Subcommand, Debug)]
pub enum Command {
    /// Audit a URL (single-page or crawl).
    Audit(AuditArgs),
    /// Start the HTTP server for SSE-streamed audits.
    Serve(ServeArgs),
    /// Compare two AuditReport JSON files and print score deltas.
    Diff(DiffArgs),
}

#[derive(Parser, Debug)]
pub struct AuditArgs {
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

    /// Wall-clock timeout (seconds) for the entire crawl.
    /// When elapsed, a partial report is returned instead of an error.
    #[arg(long)]
    pub global_timeout: Option<u64>,

    /// Do not respect robots.txt.
    #[arg(long, default_value_t = false)]
    pub ignore_robots: bool,

    /// HEAD-probe every <loc> URL in the sitemap to find broken links.
    #[arg(long, default_value_t = false)]
    pub validate_sitemap: bool,

    /// Print only the final site score; JSON output is suppressed unless --output is set.
    #[arg(short, long)]
    pub quiet: bool,

    /// Disable ANSI color codes in the terminal report (also honors NO_COLOR env var).
    #[arg(long)]
    pub no_color: bool,

    /// Write JSON report to PATH. When set, the terminal report goes to stdout; otherwise JSON goes to stdout and the terminal report goes to stderr.
    #[arg(short = 'o', long)]
    pub output: Option<PathBuf>,

    /// Also check external (off-host) links for broken URLs.
    #[arg(long, default_value_t = false)]
    pub check_external_links: bool,

    /// Enable verbose tracing output (disables the progress bar).
    #[arg(long, default_value_t = false)]
    pub verbose: bool,
}

#[derive(Parser, Debug)]
pub struct ServeArgs {
    /// Address to bind the HTTP server to.
    #[arg(long, default_value = "127.0.0.1:7878")]
    pub bind: SocketAddr,
}

#[derive(Parser, Debug)]
pub struct DiffArgs {
    /// Path to the "before" AuditReport JSON file.
    pub before: PathBuf,

    /// Path to the "after" AuditReport JSON file.
    pub after: PathBuf,

    /// Output format (text or json). Default: text.
    #[arg(short = 'f', long)]
    pub output: Option<OutputFormat>,

    /// Disable ANSI color codes (also honors NO_COLOR env var).
    #[arg(long)]
    pub no_color: bool,
}

impl AuditArgs {
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
            global_timeout_secs: self.global_timeout,
            respect_robots: !self.ignore_robots,
            validate_sitemap: self.validate_sitemap,
            quiet: self.quiet,
            no_color: self.no_color,
            verbose: self.verbose,
            output_json: self.output,
            check_external_links: self.check_external_links,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::Parser;

    #[test]
    fn audit_subcommand_parses_url() {
        let cli =
            Cli::try_parse_from(["sandslash", "audit", "https://example.com", "--depth", "0"])
                .expect("audit subcommand must parse");
        assert!(matches!(cli.command, Command::Audit(_)));
    }

    #[test]
    fn serve_subcommand_parses_bind() {
        let cli = Cli::try_parse_from(["sandslash", "serve", "--bind", "127.0.0.1:7878"])
            .expect("serve subcommand must parse");
        assert!(matches!(cli.command, Command::Serve(_)));
    }

    #[test]
    fn audit_into_config_sets_root_url() {
        let cli =
            Cli::try_parse_from(["sandslash", "audit", "https://example.com/", "--depth", "0"])
                .expect("must parse");
        if let Command::Audit(args) = cli.command {
            let config = args.into_config().expect("must build config");
            assert_eq!(config.root.as_str(), "https://example.com/");
            assert_eq!(config.depth, 0);
        }
    }

    #[test]
    fn diff_subcommand_parses_paths() {
        let cli = Cli::try_parse_from([
            "sandslash",
            "diff",
            "before.json",
            "after.json",
            "--output",
            "json",
            "--no-color",
        ])
        .expect("diff subcommand must parse");
        if let Command::Diff(args) = cli.command {
            assert_eq!(args.before.as_os_str(), "before.json");
            assert_eq!(args.after.as_os_str(), "after.json");
            assert!(matches!(args.output, Some(OutputFormat::Json)));
            assert!(args.no_color);
        } else {
            panic!("expected Command::Diff");
        }
    }
}
