use async_trait::async_trait;

use crate::audit::{AuditContext, SiteAuditor, finding};
use crate::model::{Category, Finding, PageData, Severity};

pub struct RobotsAuditor;

/// Parsed representation of a robots.txt file, covering the fields used both
/// by `RobotsAuditor` (for SEO findings) and by `RobotsCache` (for crawl gating).
#[derive(Debug, Default, Clone)]
pub struct ParsedRules {
    /// `(ua_token_lowercased, disallow_path_prefixes)` per User-agent block.
    pub disallow_prefixes: Vec<(String, Vec<String>)>,
    /// `(ua_token_lowercased, allow_path_prefixes)` per User-agent block.
    pub allow_prefixes: Vec<(String, Vec<String>)>,
    /// `(ua_token_lowercased, crawl_delay_seconds)` per User-agent block.
    pub crawl_delays: Vec<(String, f64)>,
    /// `true` when `User-agent: *` has `Disallow: /`.
    pub star_disallow_all: bool,
    /// `true` when at least one `Sitemap:` directive is present.
    pub has_sitemap: bool,
}

/// Parse a robots.txt body into [`ParsedRules`].
///
/// This is a pure, synchronous function — no I/O.
///
/// Rules:
/// - Directives are case-insensitive.
/// - `User-agent` tokens are stored lowercased.
/// - A blank line ends the current record block (a new `User-agent` starts a new block).
/// - Inline `#` comments are stripped.
pub(crate) fn parse_rules(body: &str) -> ParsedRules {
    // Accumulated per-block data (may have multiple UA tokens per block).
    // We normalise to one entry per UA token in the output vecs.
    let mut result = ParsedRules::default();

    // Track the current block's UA tokens (lowercased) and their directives.
    let mut current_uas: Vec<String> = Vec::new();
    let mut current_disallows: Vec<String> = Vec::new();
    let mut current_allows: Vec<String> = Vec::new();
    let mut current_delay: Option<f64> = None;

    let flush_block = |uas: &mut Vec<String>,
                       disallows: &mut Vec<String>,
                       allows: &mut Vec<String>,
                       delay: &mut Option<f64>,
                       result: &mut ParsedRules| {
        for ua in uas.drain(..) {
            if !disallows.is_empty() {
                result
                    .disallow_prefixes
                    .push((ua.clone(), disallows.clone()));
            }
            if !allows.is_empty() {
                result.allow_prefixes.push((ua.clone(), allows.clone()));
            }
            if let Some(d) = *delay {
                result.crawl_delays.push((ua.clone(), d));
            }
        }
        disallows.clear();
        allows.clear();
        *delay = None;
    };

    for raw_line in body.lines() {
        // Strip inline comment and surrounding whitespace.
        let line = match raw_line.split('#').next() {
            Some(s) => s.trim(),
            None => continue,
        };

        if line.is_empty() {
            // Blank line ends the current record block.
            flush_block(
                &mut current_uas,
                &mut current_disallows,
                &mut current_allows,
                &mut current_delay,
                &mut result,
            );
            continue;
        }

        // Split into directive and value on the first colon.
        let (directive, value) = match line.split_once(':') {
            Some((d, v)) => (d.trim(), v.trim()),
            None => continue,
        };

        match directive.to_ascii_lowercase().as_str() {
            "user-agent" => {
                let ua_lower = value.to_ascii_lowercase();
                // If we're already accumulating a block AND this is a fresh User-agent
                // line (not part of the ongoing UA list at the top of a block), we need
                // to handle consecutive UA lines (common pattern: multiple UAs share rules).
                // The convention is: consecutive User-agent lines before any Disallow/Allow
                // belong to the same block. A blank line separates blocks.
                // We simply accumulate all UA tokens until we see the first non-UA directive.
                current_uas.push(ua_lower.clone());

                // Track star_disallow_all for the auditor (set lazily below).
            }
            "disallow" => {
                if !value.is_empty() {
                    current_disallows.push(value.to_owned());
                    // Check star_disallow_all: any current UA is "*" and value == "/"
                    if value == "/" && current_uas.iter().any(|ua| ua == "*") {
                        result.star_disallow_all = true;
                    }
                }
            }
            "allow" => {
                if !value.is_empty() {
                    current_allows.push(value.to_owned());
                }
            }
            "crawl-delay" => {
                if let Ok(secs) = value.parse::<f64>() {
                    current_delay = Some(secs);
                }
            }
            "sitemap" if !value.is_empty() => {
                result.has_sitemap = true;
            }
            _ => {}
        }
    }

    // Flush the final block (no trailing blank line required).
    flush_block(
        &mut current_uas,
        &mut current_disallows,
        &mut current_allows,
        &mut current_delay,
        &mut result,
    );

    result
}

#[async_trait]
impl SiteAuditor for RobotsAuditor {
    fn id(&self) -> &'static str {
        "robots"
    }

    fn category(&self) -> Category {
        Category::Crawlability
    }

    async fn audit(&self, page: &PageData, ctx: &AuditContext) -> Vec<Finding> {
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

        let parsed = parse_rules(&fetched.html);
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
        let result = parse_rules(body);
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

    // ── Additional tests for the new parsed fields ─────────────────────────

    #[test]
    fn disallow_prefix_captured_in_star_block() {
        let body = "User-agent: *\nDisallow: /private\n";
        let rules = parse_rules(body);
        let star = rules
            .disallow_prefixes
            .iter()
            .find(|(ua, _)| ua == "*")
            .expect("expected * disallow entry");
        assert!(star.1.contains(&"/private".to_owned()));
    }

    #[test]
    fn allow_prefix_captured() {
        let body = "User-agent: *\nAllow: /public\nDisallow: /\n";
        let rules = parse_rules(body);
        let star_allow = rules
            .allow_prefixes
            .iter()
            .find(|(ua, _)| ua == "*")
            .expect("expected * allow entry");
        assert!(star_allow.1.contains(&"/public".to_owned()));
    }

    #[test]
    fn crawl_delay_parsed() {
        let body = "User-agent: *\nCrawl-delay: 2\n";
        let rules = parse_rules(body);
        let (_, delay) = rules
            .crawl_delays
            .iter()
            .find(|(ua, _)| ua == "*")
            .expect("expected * crawl-delay");
        assert!((*delay - 2.0f64).abs() < f64::EPSILON);
    }

    #[test]
    fn ua_specific_block_captured() {
        let body = "User-agent: Sandslash\nDisallow: /admin\n\nUser-agent: *\nDisallow: /private\n";
        let rules = parse_rules(body);
        let sandslash = rules
            .disallow_prefixes
            .iter()
            .find(|(ua, _)| ua == "sandslash")
            .expect("expected sandslash disallow entry");
        assert!(sandslash.1.contains(&"/admin".to_owned()));
        let star = rules
            .disallow_prefixes
            .iter()
            .find(|(ua, _)| ua == "*")
            .expect("expected * disallow entry");
        assert!(star.1.contains(&"/private".to_owned()));
    }
}
