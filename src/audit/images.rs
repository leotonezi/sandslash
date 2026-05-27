use crate::audit::{PageAuditor, finding};
use crate::model::{Category, Finding, PageData, Severity};
use crate::parser::Dom;

pub struct ImagesAuditor;

impl PageAuditor for ImagesAuditor {
    fn id(&self) -> &'static str {
        "images"
    }
    fn category(&self) -> Category {
        Category::Media
    }

    fn audit(&self, _page: &PageData, dom: &Dom) -> Vec<Finding> {
        let mut out = Vec::new();

        for img in dom.images() {
            // Skip data URIs — decorative by convention
            if img.src.starts_with("data:") {
                continue;
            }

            match img.alt {
                None => out.push(finding(
                    "images.missing-alt",
                    Category::Media,
                    Severity::Warning,
                    5,
                    format!("Image is missing alt attribute: {}", img.src),
                )),
                Some(ref text) if text.is_empty() => out.push(finding(
                    "images.empty-alt",
                    Category::Media,
                    Severity::Info,
                    2,
                    format!("Image has empty alt attribute: {}", img.src),
                )),
                _ => {}
            }
        }

        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::Headers;

    fn page() -> PageData {
        PageData {
            url: "https://example.com/".parse().unwrap(),
            status: 200,
            redirect_chain: vec![],
            html: String::new(),
            headers: Headers::default(),
            depth: 0,
        }
    }

    fn audit(html: &str) -> Vec<Finding> {
        ImagesAuditor.audit(&page(), &Dom::parse(html))
    }

    fn ids(findings: &[Finding]) -> Vec<&str> {
        findings.iter().map(|f| f.check_id).collect()
    }

    const MISSING_ALT: &str = include_str!("../../tests/fixtures/images_missing_alt.html");
    const EMPTY_ALT: &str = include_str!("../../tests/fixtures/images_empty_alt.html");
    const OK: &str = include_str!("../../tests/fixtures/images_ok.html");
    const DATA_URI: &str = include_str!("../../tests/fixtures/images_data_uri.html");
    const SVG: &str = include_str!("../../tests/fixtures/images_svg.html");

    #[test]
    fn missing_alt_is_warning() {
        let f = audit(MISSING_ALT);
        assert!(ids(&f).contains(&"images.missing-alt"));
        let finding = f
            .iter()
            .find(|f| f.check_id == "images.missing-alt")
            .unwrap();
        assert_eq!(finding.severity, Severity::Warning);
        assert_eq!(finding.penalty, 5);
    }

    #[test]
    fn empty_alt_is_info() {
        let f = audit(EMPTY_ALT);
        assert!(ids(&f).contains(&"images.empty-alt"));
        let finding = f.iter().find(|f| f.check_id == "images.empty-alt").unwrap();
        assert_eq!(finding.severity, Severity::Info);
        assert_eq!(finding.penalty, 2);
    }

    #[test]
    fn descriptive_alt_produces_no_findings() {
        let f = audit(OK);
        assert!(f.is_empty());
    }

    #[test]
    fn data_uri_without_alt_is_skipped() {
        let f = audit(DATA_URI);
        assert!(f.is_empty());
    }

    #[test]
    fn svg_only_page_produces_no_findings() {
        let f = audit(SVG);
        assert!(f.is_empty());
    }

    #[test]
    fn per_image_finding_one_per_offending_img() {
        let html = r#"<html><body>
            <img src="a.jpg">
            <img src="b.jpg">
            <img src="c.jpg" alt="ok">
        </body></html>"#;
        let f = audit(html);
        let missing: Vec<_> = f
            .iter()
            .filter(|f| f.check_id == "images.missing-alt")
            .collect();
        assert_eq!(missing.len(), 2);
    }

    #[test]
    fn mixed_missing_and_empty_alt() {
        let html = r#"<html><body>
            <img src="a.jpg">
            <img src="b.jpg" alt="">
            <img src="c.jpg" alt="Photo of a cat">
        </body></html>"#;
        let f = audit(html);
        assert!(ids(&f).contains(&"images.missing-alt"));
        assert!(ids(&f).contains(&"images.empty-alt"));
        assert_eq!(f.len(), 2);
    }

    #[test]
    fn category_is_media() {
        let f = audit(MISSING_ALT);
        assert!(f.iter().all(|f| f.category == Category::Media));
    }
}
