use url::Url;

use crate::audit::{finding, PageAuditor};
use crate::model::{Category, Finding, PageData, Severity};
use crate::parser::Dom;

pub struct MetadataAuditor;

impl PageAuditor for MetadataAuditor {
    fn id(&self) -> &'static str { "metadata" }
    fn category(&self) -> Category { Category::Metadata }

    fn audit(&self, page: &PageData, dom: &Dom) -> Vec<Finding> {
        let mut out = Vec::new();
        audit_title(dom, &mut out);
        audit_description(dom, &mut out);
        audit_canonical(page, dom, &mut out);
        out
    }
}

fn audit_title(dom: &Dom, out: &mut Vec<Finding>) {
    match dom.title() {
        None => out.push(finding(
            "title.missing", Category::Metadata, Severity::Critical, 30,
            "Page has no <title> tag",
        )),
        Some(t) if t.trim().is_empty() => out.push(finding(
            "title.empty", Category::Metadata, Severity::Critical, 30,
            "Title tag is empty",
        )),
        Some(t) if t.chars().count() < 30 => out.push(finding(
            "title.short", Category::Metadata, Severity::Warning, 15,
            format!("Title is {} chars (min 30)", t.chars().count()),
        )),
        Some(t) if t.chars().count() > 60 => out.push(finding(
            "title.long", Category::Metadata, Severity::Warning, 10,
            format!("Title is {} chars (max 60)", t.chars().count()),
        )),
        _ => {}
    }
}

fn audit_description(dom: &Dom, out: &mut Vec<Finding>) {
    match dom.meta_description() {
        None => out.push(finding(
            "description.missing", Category::Metadata, Severity::Warning, 20,
            "Page has no meta description",
        )),
        Some(d) if d.trim().is_empty() => out.push(finding(
            "description.empty", Category::Metadata, Severity::Warning, 20,
            "Meta description is empty",
        )),
        Some(d) if d.chars().count() < 50 => out.push(finding(
            "description.short", Category::Metadata, Severity::Warning, 10,
            format!("Meta description is {} chars (min 50)", d.chars().count()),
        )),
        Some(d) if d.chars().count() > 160 => out.push(finding(
            "description.long", Category::Metadata, Severity::Info, 5,
            format!("Meta description is {} chars (max 160)", d.chars().count()),
        )),
        _ => {}
    }
}

fn audit_canonical(page: &PageData, dom: &Dom, out: &mut Vec<Finding>) {
    match dom.canonical() {
        None => out.push(finding(
            "canonical.missing", Category::Metadata, Severity::Warning, 10,
            "Page has no canonical link",
        )),
        Some(c) => {
            match Url::parse(&c) {
                Err(_) => out.push(finding(
                    "canonical.invalid", Category::Metadata, Severity::Warning, 10,
                    format!("Canonical URL is not valid: {c}"),
                )),
                Ok(canonical_url) => {
                    if canonical_url.host() != page.url.host() {
                        out.push(finding(
                            "canonical.off-host", Category::Metadata, Severity::Warning, 15,
                            format!("Canonical points to different host: {c}"),
                        ));
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{Headers, PageData};

    fn page(url: &str) -> PageData {
        PageData {
            url: url.parse().unwrap(),
            status: 200,
            redirect_chain: vec![],
            html: String::new(),
            headers: Headers::default(),
            depth: 0,
        }
    }

    fn audit(html: &str, url: &str) -> Vec<Finding> {
        let dom = Dom::parse(html);
        MetadataAuditor.audit(&page(url), &dom)
    }

    fn ids(findings: &[Finding]) -> Vec<&str> {
        findings.iter().map(|f| f.check_id).collect()
    }

    // --- title ---
    #[test]
    fn title_missing() {
        let f = audit("<html><head></head></html>", "https://example.com/");
        assert!(ids(&f).contains(&"title.missing"));
    }

    #[test]
    fn title_empty() {
        let f = audit("<html><head><title>  </title></head></html>", "https://example.com/");
        assert!(ids(&f).contains(&"title.empty"));
    }

    #[test]
    fn title_too_short() {
        let f = audit("<html><head><title>Hi</title></head></html>", "https://example.com/");
        assert!(ids(&f).contains(&"title.short"));
    }

    #[test]
    fn title_too_long() {
        let long = "A".repeat(61);
        let f = audit(&format!("<html><head><title>{long}</title></head></html>"), "https://example.com/");
        assert!(ids(&f).contains(&"title.long"));
    }

    #[test]
    fn title_ok() {
        let f = audit("<html><head><title>This Title Is Exactly Right Length</title></head></html>", "https://example.com/");
        assert!(!ids(&f).iter().any(|id| id.starts_with("title.")));
    }

    // --- description ---
    #[test]
    fn description_missing() {
        let f = audit("<html><head></head></html>", "https://example.com/");
        assert!(ids(&f).contains(&"description.missing"));
    }

    #[test]
    fn description_short() {
        let f = audit(r#"<html><head><meta name="description" content="Short"></head></html>"#, "https://example.com/");
        assert!(ids(&f).contains(&"description.short"));
    }

    #[test]
    fn description_long() {
        let long = "A".repeat(161);
        let f = audit(&format!(r#"<html><head><meta name="description" content="{long}"></head></html>"#), "https://example.com/");
        assert!(ids(&f).contains(&"description.long"));
    }

    // --- canonical ---
    #[test]
    fn canonical_missing() {
        let f = audit("<html><head></head></html>", "https://example.com/");
        assert!(ids(&f).contains(&"canonical.missing"));
    }

    #[test]
    fn canonical_off_host() {
        let f = audit(
            r#"<html><head><link rel="canonical" href="https://other.com/"></head></html>"#,
            "https://example.com/",
        );
        assert!(ids(&f).contains(&"canonical.off-host"));
    }

    #[test]
    fn canonical_ok() {
        let f = audit(
            r#"<html><head><link rel="canonical" href="https://example.com/"></head></html>"#,
            "https://example.com/",
        );
        assert!(!ids(&f).iter().any(|id| id.starts_with("canonical.")));
    }
}
