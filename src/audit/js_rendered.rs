// Thresholds for JS-rendered detection are configurable via environment
// variables. See `.env.example` at the project root for the available
// variables and their default values:
//
//   JS_RENDERED_TEXT_RATIO_THRESHOLD  (f64, default 0.05)
//   JS_RENDERED_MIN_CONTENT_TAGS      (usize, default 3)
//
// Both variables are read at call time so that tests and runtime overrides
// take effect without restarting the process.

use crate::audit::{PageAuditor, finding};
use crate::model::{Category, Finding, PageData, Severity};
use crate::parser::Dom;

pub struct JsRenderedAuditor;

/// Default text-to-HTML byte ratio below which a page is considered JS-rendered.
const DEFAULT_RATIO_THRESHOLD: f64 = 0.05;
/// Default minimum number of semantic content tags expected in a real page.
const DEFAULT_MIN_CONTENT_TAGS: usize = 3;

impl PageAuditor for JsRenderedAuditor {
    fn id(&self) -> &'static str {
        "js-rendered"
    }

    fn category(&self) -> Category {
        Category::Structure
    }

    fn audit(&self, page: &PageData, dom: &Dom) -> Vec<Finding> {
        // Read thresholds from env at call time; fall back to defaults on parse error.
        let ratio_threshold: f64 = std::env::var("JS_RENDERED_TEXT_RATIO_THRESHOLD")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(DEFAULT_RATIO_THRESHOLD);

        let min_content_tags: usize = std::env::var("JS_RENDERED_MIN_CONTENT_TAGS")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(DEFAULT_MIN_CONTENT_TAGS);

        let total_html_bytes = page.html.len();

        // Guard against zero-length documents to avoid division by zero / NaN.
        if total_html_bytes == 0 {
            return vec![];
        }

        let visible_text_bytes = dom.visible_text_len();
        let ratio = visible_text_bytes as f64 / total_html_bytes as f64;
        let tag_count = dom.content_tag_count();

        // Both conditions must hold to trigger the finding.
        if ratio < ratio_threshold && tag_count < min_content_tags {
            vec![finding(
                "js-rendered.likely",
                Category::Structure,
                Severity::Warning,
                10,
                "page appears to require JS rendering — results may be incomplete",
            )]
        } else {
            vec![]
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::Headers;

    fn page(html: &str) -> PageData {
        PageData {
            url: "https://example.com/"
                .parse()
                .expect("invariant: valid URL"),
            status: 200,
            redirect_chain: vec![],
            html: html.to_owned(),
            headers: Headers::default(),
            depth: 0,
        }
    }

    fn audit(html: &str) -> Vec<Finding> {
        let p = page(html);
        JsRenderedAuditor.audit(&p, &Dom::parse(html))
    }

    fn ids(findings: &[Finding]) -> Vec<&str> {
        findings.iter().map(|f| f.check_id).collect()
    }

    const EMPTY_ROOT: &str = include_str!("../../tests/fixtures/js_rendered_empty_root.html");
    const BASIC: &str = include_str!("../../tests/fixtures/basic.html");

    #[test]
    fn empty_root_triggers_warning() {
        let f = audit(EMPTY_ROOT);
        assert!(
            ids(&f).contains(&"js-rendered.likely"),
            "expected js-rendered.likely in {:?}",
            ids(&f)
        );
        let finding = f
            .iter()
            .find(|x| x.check_id == "js-rendered.likely")
            .unwrap();
        assert_eq!(finding.severity, Severity::Warning);
        assert_eq!(
            finding.message,
            "page appears to require JS rendering — results may be incomplete"
        );
    }

    #[test]
    fn content_rich_page_does_not_trigger() {
        let f = audit(BASIC);
        assert!(
            !ids(&f).contains(&"js-rendered.likely"),
            "did not expect js-rendered.likely for basic.html"
        );
    }

    #[test]
    fn zero_byte_html_returns_empty_no_panic() {
        // PageData with empty html — guard must prevent division by zero.
        let p = page("");
        let f = JsRenderedAuditor.audit(&p, &Dom::parse(""));
        assert!(f.is_empty(), "expected no findings for empty HTML");
    }
}
