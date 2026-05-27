use crate::audit::{finding, PageAuditor};
use crate::model::{Category, Finding, PageData, Severity};
use crate::parser::Dom;

pub struct HeadingsAuditor;

impl PageAuditor for HeadingsAuditor {
    fn id(&self) -> &'static str { "headings" }
    fn category(&self) -> Category { Category::Structure }

    fn audit(&self, _page: &PageData, dom: &Dom) -> Vec<Finding> {
        let mut out = Vec::new();
        let headings = dom.headings();

        let h1_count = headings.iter().filter(|(l, _)| *l == 1).count();
        match h1_count {
            0 => out.push(finding(
                "headings.no-h1", Category::Structure, Severity::Critical, 40,
                "Page has no <h1>",
            )),
            1 => {}
            n => out.push(finding(
                "headings.multiple-h1", Category::Structure, Severity::Warning, 20,
                format!("Page has {n} <h1> elements; expected exactly 1"),
            )),
        }

        // Detect empty headings
        for (level, text) in &headings {
            if text.trim().is_empty() {
                out.push(finding(
                    "headings.empty", Category::Structure, Severity::Warning, 10,
                    format!("<h{level}> is empty"),
                ));
            }
        }

        // Detect skipped levels: collect unique levels present, sorted
        let mut levels: Vec<u8> = headings.iter().map(|(l, _)| *l).collect::<std::collections::BTreeSet<_>>().into_iter().collect();
        levels.sort();
        for window in levels.windows(2) {
            if window[1] > window[0] + 1 {
                out.push(finding(
                    "headings.skipped-level", Category::Structure, Severity::Warning, 10,
                    format!("Heading level skipped: h{} → h{}", window[0], window[1]),
                ));
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
        HeadingsAuditor.audit(&page(), &Dom::parse(html))
    }

    fn ids(findings: &[Finding]) -> Vec<&str> {
        findings.iter().map(|f| f.check_id).collect()
    }

    #[test]
    fn no_h1_is_critical() {
        let f = audit("<html><body><h2>Sub</h2></body></html>");
        assert!(ids(&f).contains(&"headings.no-h1"));
    }

    #[test]
    fn multiple_h1_is_warning() {
        let f = audit("<html><body><h1>A</h1><h1>B</h1></body></html>");
        assert!(ids(&f).contains(&"headings.multiple-h1"));
    }

    #[test]
    fn single_h1_ok() {
        let f = audit("<html><body><h1>Main</h1><h2>Sub</h2></body></html>");
        assert!(!ids(&f).contains(&"headings.no-h1"));
        assert!(!ids(&f).contains(&"headings.multiple-h1"));
    }

    #[test]
    fn empty_heading_flagged() {
        let f = audit("<html><body><h1></h1></body></html>");
        assert!(ids(&f).contains(&"headings.empty"));
    }

    #[test]
    fn skipped_level_flagged() {
        let f = audit("<html><body><h1>A</h1><h3>C</h3></body></html>");
        assert!(ids(&f).contains(&"headings.skipped-level"));
    }

    #[test]
    fn no_skip_when_sequential() {
        let f = audit("<html><body><h1>A</h1><h2>B</h2><h3>C</h3></body></html>");
        assert!(!ids(&f).contains(&"headings.skipped-level"));
    }
}
