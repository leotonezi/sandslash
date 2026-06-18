use crate::audit::{PageAuditor, finding};
use crate::model::{Category, Finding, PageData, Severity};
use crate::parser::Dom;

pub struct CanonicalAuditor;

impl PageAuditor for CanonicalAuditor {
    fn id(&self) -> &'static str {
        "canonical"
    }

    fn category(&self) -> Category {
        Category::Metadata
    }

    fn audit(&self, page: &PageData, dom: &Dom) -> Vec<Finding> {
        let mut out = Vec::new();

        let hrefs = dom.canonicals();
        if hrefs.is_empty() {
            return out;
        }

        if hrefs.len() > 1 {
            out.push(finding(
                "canonical.multiple",
                Category::Metadata,
                Severity::Warning,
                10,
                format!(
                    "Page has {} <link rel=\"canonical\"> elements; only one is expected",
                    hrefs.len()
                ),
            ));
        }

        // Always work with the first canonical only.
        let href = &hrefs[0];

        let resolved = match page.url.join(href) {
            Ok(u) => u,
            Err(_) => return out,
        };

        // Check if the resolved canonical matches any URL in the redirect chain.
        if page.redirect_chain.contains(&resolved) {
            out.push(finding(
                "canonical.redirect-mismatch",
                Category::Metadata,
                Severity::Warning,
                15,
                format!("Canonical URL {resolved} matches a redirected URL in the chain"),
            ));
            return out;
        }

        // Same host but different path/query/fragment: not self-referential.
        if resolved.host() == page.url.host() {
            let same_path = resolved.path() == page.url.path();
            let same_query = resolved.query() == page.url.query();
            let same_fragment = resolved.fragment() == page.url.fragment();

            if !(same_path && same_query && same_fragment) {
                out.push(finding(
                    "canonical.not-self-referential",
                    Category::Metadata,
                    Severity::Info,
                    5,
                    format!(
                        "Canonical URL {resolved} differs from the page URL {} (same host, different path/query/fragment)",
                        page.url
                    ),
                ));
            }
        }
        // Off-host: MetadataAuditor owns that check — do nothing here.

        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::Headers;

    fn page_with_chain(url: &str, redirect_chain: Vec<&str>) -> PageData {
        PageData {
            url: url.parse().expect("invariant: valid URL in test"),
            status: 200,
            redirect_chain: redirect_chain
                .into_iter()
                .map(|u| u.parse().expect("invariant: valid URL in test"))
                .collect(),
            html: String::new(),
            headers: Headers::default(),
            depth: 0,
        }
    }

    fn page(url: &str) -> PageData {
        page_with_chain(url, vec![])
    }

    fn audit_with(html: &str, page: &PageData) -> Vec<Finding> {
        CanonicalAuditor.audit(page, &Dom::parse(html))
    }

    fn ids(findings: &[Finding]) -> Vec<&str> {
        findings.iter().map(|f| f.check_id.as_str()).collect()
    }

    // --- happy path: self-referential canonical ---
    #[test]
    fn self_referential_no_findings() {
        let html = include_str!("../../tests/fixtures/canonical_self_referential.html");
        let p = page("https://example.com/");
        let f = audit_with(html, &p);
        assert!(f.is_empty(), "expected no findings, got: {:?}", ids(&f));
    }

    // --- no canonical: no findings ---
    #[test]
    fn no_canonical_no_findings() {
        let html = "<html><head></head><body></body></html>";
        let p = page("https://example.com/");
        let f = audit_with(html, &p);
        assert!(f.is_empty());
    }

    // --- multiple canonicals ---
    #[test]
    fn multiple_canonicals_flagged() {
        let html = include_str!("../../tests/fixtures/canonical_multiple.html");
        // Page URL matches the first canonical so no not-self-referential.
        let p = page("https://example.com/page-a");
        let f = audit_with(html, &p);
        assert!(
            ids(&f).contains(&"canonical.multiple"),
            "expected canonical.multiple, got: {:?}",
            ids(&f)
        );
    }

    // --- redirect-mismatch ---
    #[test]
    fn redirect_mismatch_flagged() {
        let html = include_str!("../../tests/fixtures/canonical_points_to_redirected.html");
        // canonical = https://example.com/old — place that in the redirect chain
        let p = page_with_chain("https://example.com/new", vec!["https://example.com/old"]);
        let f = audit_with(html, &p);
        assert!(
            ids(&f).contains(&"canonical.redirect-mismatch"),
            "expected canonical.redirect-mismatch, got: {:?}",
            ids(&f)
        );
    }

    // --- redirect-mismatch stops further checks ---
    #[test]
    fn redirect_mismatch_no_not_self_referential() {
        let html = include_str!("../../tests/fixtures/canonical_points_to_redirected.html");
        let p = page_with_chain("https://example.com/new", vec!["https://example.com/old"]);
        let f = audit_with(html, &p);
        assert!(
            !ids(&f).contains(&"canonical.not-self-referential"),
            "should not have not-self-referential when redirect-mismatch fires"
        );
    }

    // --- not-self-referential ---
    #[test]
    fn not_self_referential_flagged() {
        let html = include_str!("../../tests/fixtures/canonical_multiple.html");
        // Page URL is /page-c, first canonical is /page-a → not self-referential (same host,
        // different path) and not in redirect chain.
        let p = page("https://example.com/page-c");
        let f = audit_with(html, &p);
        assert!(
            ids(&f).contains(&"canonical.not-self-referential"),
            "expected canonical.not-self-referential, got: {:?}",
            ids(&f)
        );
    }

    // --- relative href resolved correctly ---
    #[test]
    fn relative_canonical_resolved() {
        let html = include_str!("../../tests/fixtures/canonical_relative.html");
        // canonical href = "/landing", page URL = https://example.com/ → same host different path
        let p = page("https://example.com/");
        let f = audit_with(html, &p);
        assert!(
            ids(&f).contains(&"canonical.not-self-referential"),
            "relative canonical /landing should resolve and flag not-self-referential, got: {:?}",
            ids(&f)
        );
    }

    // --- off-host canonical: no findings from CanonicalAuditor ---
    #[test]
    fn off_host_canonical_no_findings() {
        let html =
            r#"<html><head><link rel="canonical" href="https://other.com/page"></head></html>"#;
        let p = page("https://example.com/page");
        let f = audit_with(html, &p);
        assert!(
            f.is_empty(),
            "off-host should produce no findings, got: {:?}",
            ids(&f)
        );
    }
}
