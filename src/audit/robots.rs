use async_trait::async_trait;

use crate::audit::{AuditContext, SiteAuditor, finding};
use crate::model::{Category, Finding, PageData, Severity};

pub struct RobotsAuditor;

struct ParsedRobots {
    star_disallow_all: bool,
    has_sitemap: bool,
}

fn parse_robots(body: &str) -> ParsedRobots {
    let mut in_star_block = false;
    let mut star_disallow_all = false;
    let mut has_sitemap = false;

    for raw_line in body.lines() {
        // Strip inline comment and surrounding whitespace.
        let line = match raw_line.split('#').next() {
            Some(s) => s.trim(),
            None => continue,
        };

        if line.is_empty() {
            // Blank line ends the current record block.
            in_star_block = false;
            continue;
        }

        // Split into directive and value on the first colon.
        let (directive, value) = match line.split_once(':') {
            Some((d, v)) => (d.trim(), v.trim()),
            None => continue,
        };

        match directive.to_ascii_lowercase().as_str() {
            "user-agent" => {
                in_star_block = value == "*";
            }
            "disallow" => {
                if in_star_block && value == "/" {
                    star_disallow_all = true;
                }
            }
            "sitemap" if !value.is_empty() => {
                has_sitemap = true;
            }
            _ => {}
        }
    }

    ParsedRobots {
        star_disallow_all,
        has_sitemap,
    }
}

#[async_trait]
impl SiteAuditor for RobotsAuditor {
    fn id(&self) -> &'static str {
        "robots"
    }

    fn category(&self) -> Category {
        Category::Crawlability
    }

    async fn audit(&self, page: &PageData, ctx: &AuditContext<'_>) -> Vec<Finding> {
        let robots_missing = || {
            finding(
                "robots.missing",
                Category::Crawlability,
                Severity::Warning,
                15,
                "robots.txt not found or inaccessible",
            )
        };

        let robots_url = match page.url.join("/robots.txt") {
            Ok(u) => u,
            Err(_) => return vec![robots_missing()],
        };

        let fetched = match ctx.fetcher.fetch(&robots_url).await {
            Ok(page_data) => page_data,
            Err(_) => return vec![robots_missing()],
        };

        if !(200..300).contains(&fetched.status) {
            return vec![robots_missing()];
        }

        let parsed = parse_robots(&fetched.html);
        let mut out = Vec::new();

        if parsed.star_disallow_all {
            out.push(finding(
                "robots.disallow-all",
                Category::Crawlability,
                Severity::Critical,
                40,
                "robots.txt disallows all crawlers (Disallow: / for User-agent: *)",
            ));
        }

        if !parsed.has_sitemap {
            out.push(finding(
                "robots.no-sitemap",
                Category::Crawlability,
                Severity::Info,
                5,
                "robots.txt does not contain a Sitemap: directive",
            ));
        }

        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn assert_parsed(body: &str, star_disallow_all: bool, has_sitemap: bool) {
        let result = parse_robots(body);
        assert_eq!(
            result.star_disallow_all, star_disallow_all,
            "star_disallow_all mismatch for input: {body:?}"
        );
        assert_eq!(
            result.has_sitemap, has_sitemap,
            "has_sitemap mismatch for input: {body:?}"
        );
    }

    #[test]
    fn empty_input() {
        assert_parsed("", false, false);
    }

    #[test]
    fn comment_only_input() {
        assert_parsed("# just a comment\n# another line\n", false, false);
    }

    #[test]
    fn no_star_block() {
        let body = "User-agent: Googlebot\nDisallow: /\nSitemap: https://example.com/sitemap.xml\n";
        // Disallow: / is NOT in * block → should not trigger
        assert_parsed(body, false, true);
    }

    #[test]
    fn disallow_all_in_star_block() {
        let body = "User-agent: *\nDisallow: /\nSitemap: https://example.com/sitemap.xml\n";
        assert_parsed(body, true, true);
    }

    #[test]
    fn disallow_private_in_star_block_does_not_trigger() {
        let body = "User-agent: *\nDisallow: /private\n";
        assert_parsed(body, false, false);
    }

    #[test]
    fn sitemap_present() {
        let body = "User-agent: *\nAllow: /\nSitemap: https://example.com/sitemap.xml\n";
        assert_parsed(body, false, true);
    }

    #[test]
    fn sitemap_absent() {
        let body = "User-agent: *\nAllow: /\n";
        assert_parsed(body, false, false);
    }

    #[test]
    fn crlf_line_endings() {
        let body = "User-agent: *\r\nDisallow: /\r\nSitemap: https://example.com/sitemap.xml\r\n";
        assert_parsed(body, true, true);
    }

    #[test]
    fn case_insensitive_directives() {
        let body = "USER-AGENT: *\nDISALLOW: /\nSITEMAP: https://example.com/sitemap.xml\n";
        assert_parsed(body, true, true);
    }

    #[test]
    fn disallow_in_star_block_does_not_bleed_across_blank_line() {
        // Blank line ends the * block; subsequent Disallow: / is under a new agent
        let body = "User-agent: *\nAllow: /\n\nUser-agent: Badbot\nDisallow: /\n";
        assert_parsed(body, false, false);
    }
}
