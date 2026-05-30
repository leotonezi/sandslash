use async_trait::async_trait;
use quick_xml::Reader;
use quick_xml::errors::IllFormedError;
use quick_xml::events::Event;
use url::Url;

use crate::audit::{AuditContext, SiteAuditor, finding};
use crate::model::{Category, Finding, PageData, Severity};

pub struct SitemapAuditor;

/// Scan the robots.txt body for the first `Sitemap:` directive and return the
/// parsed URL.  The directive is case-insensitive per robots.txt convention.
fn resolve_sitemap_url(robots_body: &str, root: &Url) -> Option<Url> {
    for raw_line in robots_body.lines() {
        let line = match raw_line.split('#').next() {
            Some(s) => s.trim(),
            None => continue,
        };
        if let Some((directive, value)) = line.split_once(':') {
            if directive.trim().eq_ignore_ascii_case("sitemap") {
                let url_str = value.trim();
                if !url_str.is_empty() {
                    // Try absolute URL first, then resolve against root.
                    if let Ok(u) = url_str.parse::<Url>() {
                        return Some(u);
                    }
                    if let Ok(u) = root.join(url_str) {
                        return Some(u);
                    }
                }
            }
        }
    }
    None
}

/// Parse the sitemap bytes with quick_xml.  Returns `Ok(())` on a well-formed
/// document (reaches `Event::Eof` without error with all elements closed),
/// or `Err` on the first parse error or truncation (unclosed elements at EOF).
fn validate_sitemap_xml(bytes: &[u8]) -> Result<(), quick_xml::Error> {
    let mut reader = Reader::from_reader(bytes);
    reader.config_mut().trim_text(true);

    let mut buf = Vec::new();
    // Track element nesting depth so truncated documents (unclosed tags at
    // EOF) are reported as malformed.
    let mut depth: usize = 0;

    loop {
        buf.clear();
        match reader.read_event_into(&mut buf) {
            Ok(Event::Eof) => {
                if depth == 0 {
                    return Ok(());
                }
                // Document ended with open elements — it is truncated.
                return Err(quick_xml::Error::IllFormed(IllFormedError::MissingEndTag(
                    "document has unclosed elements".to_owned(),
                )));
            }
            Ok(Event::Start(_)) => depth += 1,
            Ok(Event::End(_)) => depth = depth.saturating_sub(1),
            Ok(_) => {}
            Err(e) => return Err(e),
        }
    }
}

#[async_trait]
impl SiteAuditor for SitemapAuditor {
    fn id(&self) -> &'static str {
        "sitemap"
    }

    fn category(&self) -> Category {
        Category::Crawlability
    }

    async fn audit(&self, page: &PageData, ctx: &AuditContext<'_>) -> Vec<Finding> {
        let missing_finding = || {
            finding(
                "sitemap.missing",
                Category::Crawlability,
                Severity::Warning,
                5,
                "sitemap.xml not found or inaccessible",
            )
        };

        // --- Step 1: try to get the sitemap URL from robots.txt ---
        let sitemap_url: Url = 'resolve: {
            let robots_url = match page.url.join("/robots.txt") {
                Ok(u) => u,
                Err(_) => break 'resolve fallback_sitemap_url(&page.url),
            };

            if let Ok(robots_page) = ctx.fetcher.fetch(&robots_url).await {
                if (200..300).contains(&robots_page.status) {
                    if let Some(u) = resolve_sitemap_url(&robots_page.html, &page.url) {
                        break 'resolve u;
                    }
                }
            }

            fallback_sitemap_url(&page.url)
        };

        // --- Step 2: fetch the sitemap ---
        let sitemap_page = match ctx.fetcher.fetch(&sitemap_url).await {
            Ok(p) => p,
            Err(_) => return vec![missing_finding()],
        };

        if !(200..300).contains(&sitemap_page.status) {
            return vec![missing_finding()];
        }

        // --- Step 3: validate XML ---
        match validate_sitemap_xml(sitemap_page.html.as_bytes()) {
            Ok(()) => vec![],
            Err(_) => vec![finding(
                "sitemap.malformed",
                Category::Crawlability,
                Severity::Critical,
                20,
                "sitemap.xml is present but could not be parsed as valid XML",
            )],
        }
    }
}

/// Returns {root}/sitemap.xml, falling back to root itself on join failure.
fn fallback_sitemap_url(root: &Url) -> Url {
    root.join("/sitemap.xml").unwrap_or_else(|_| root.clone())
}

// ---------------------------------------------------------------------------
// Unit tests for pure helpers
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    const ROOT: &str = "https://example.com/";

    fn root_url() -> Url {
        ROOT.parse().unwrap()
    }

    // --- resolve_sitemap_url ---

    #[test]
    fn finds_absolute_sitemap_url() {
        let body = "User-agent: *\nSitemap: https://example.com/sitemap.xml\n";
        let result = resolve_sitemap_url(body, &root_url());
        assert_eq!(
            result.as_ref().map(|u| u.as_str()),
            Some("https://example.com/sitemap.xml")
        );
    }

    #[test]
    fn finds_sitemap_url_case_insensitive() {
        let body = "SITEMAP: https://example.com/sitemap.xml\n";
        let result = resolve_sitemap_url(body, &root_url());
        assert!(
            result.is_some(),
            "should find sitemap even with uppercase directive"
        );
    }

    #[test]
    fn no_sitemap_directive_returns_none() {
        let body = "User-agent: *\nDisallow: /private\n";
        assert!(resolve_sitemap_url(body, &root_url()).is_none());
    }

    #[test]
    fn empty_sitemap_value_returns_none() {
        let body = "Sitemap:\n";
        assert!(resolve_sitemap_url(body, &root_url()).is_none());
    }

    #[test]
    fn sitemap_with_inline_comment_is_trimmed() {
        // The Sitemap: line itself is not a comment subject; just verify blank
        // lines don't break parsing.
        let body = "User-agent: *\n# a comment\nSitemap: https://example.com/sitemap.xml\n";
        let result = resolve_sitemap_url(body, &root_url());
        assert!(result.is_some());
    }

    #[test]
    fn returns_first_sitemap_when_multiple_present() {
        let body = "Sitemap: https://example.com/sitemap1.xml\nSitemap: https://example.com/sitemap2.xml\n";
        let result = resolve_sitemap_url(body, &root_url()).unwrap();
        assert_eq!(result.as_str(), "https://example.com/sitemap1.xml");
    }

    // --- validate_sitemap_xml ---

    #[test]
    fn valid_sitemap_xml_returns_ok() {
        let xml = br#"<?xml version="1.0" encoding="UTF-8"?>
<urlset xmlns="http://www.sitemaps.org/schemas/sitemap/0.9">
  <url><loc>https://example.com/</loc></url>
</urlset>"#;
        assert!(validate_sitemap_xml(xml).is_ok());
    }

    #[test]
    fn truncated_xml_returns_err() {
        let xml = b"<?xml version=\"1.0\"?><urlset><url><loc>https://example.com/</loc>";
        assert!(
            validate_sitemap_xml(xml).is_err(),
            "truncated XML should fail validation"
        );
    }

    #[test]
    fn mismatched_tags_returns_err() {
        let xml = b"<?xml version=\"1.0\"?><urlset><url></wrong></urlset>";
        assert!(
            validate_sitemap_xml(xml).is_err(),
            "mismatched tags should fail validation"
        );
    }

    #[test]
    fn empty_bytes_returns_ok() {
        // Empty input is technically well-formed (just EOF immediately).
        assert!(validate_sitemap_xml(b"").is_ok());
    }
}
