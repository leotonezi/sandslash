use crate::audit::{PageAuditor, finding};
use crate::model::{Category, Finding, PageData, Severity};
use crate::parser::Dom;

pub struct RedirectsAuditor;

impl PageAuditor for RedirectsAuditor {
    fn id(&self) -> &'static str {
        "redirects"
    }

    fn category(&self) -> Category {
        Category::Links
    }

    fn audit(&self, page: &PageData, _dom: &Dom) -> Vec<Finding> {
        let len = page.redirect_chain.len();

        if len > 5 {
            return vec![finding(
                "redirects.chain-excessive",
                Category::Links,
                Severity::Critical,
                30,
                format!(
                    "Redirect chain is {len} hops (max 5); excessive redirects hurt crawlability"
                ),
            )];
        }

        if len > 3 {
            return vec![finding(
                "redirects.chain-long",
                Category::Links,
                Severity::Warning,
                15,
                format!("Redirect chain is {len} hops (recommended max 3)"),
            )];
        }

        vec![]
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::Headers;
    use url::Url;

    fn page_with_chain(len: usize) -> PageData {
        let url: Url = "https://example.com/"
            .parse()
            .expect("invariant: valid URL");
        let redirect_chain = vec![url.clone(); len];
        PageData {
            url,
            status: 200,
            redirect_chain,
            html: String::new(),
            headers: Headers::default(),
            depth: 0,
        }
    }

    fn audit(len: usize) -> Vec<Finding> {
        let page = page_with_chain(len);
        RedirectsAuditor.audit(&page, &Dom::parse(""))
    }

    fn ids(findings: &[Finding]) -> Vec<&str> {
        findings.iter().map(|f| f.check_id).collect()
    }

    #[test]
    fn chain_0_no_findings() {
        assert!(audit(0).is_empty());
    }

    #[test]
    fn chain_3_no_findings() {
        assert!(audit(3).is_empty());
    }

    #[test]
    fn chain_4_warning() {
        let f = audit(4);
        assert_eq!(f.len(), 1);
        assert!(ids(&f).contains(&"redirects.chain-long"));
        assert_eq!(f[0].severity, Severity::Warning);
        assert!(f[0].penalty > 0);
        assert_eq!(f[0].category, Category::Links);
    }

    #[test]
    fn chain_5_warning() {
        let f = audit(5);
        assert_eq!(f.len(), 1);
        assert!(ids(&f).contains(&"redirects.chain-long"));
        assert!(!ids(&f).contains(&"redirects.chain-excessive"));
    }

    #[test]
    fn chain_6_critical_only() {
        let f = audit(6);
        assert_eq!(f.len(), 1);
        assert!(ids(&f).contains(&"redirects.chain-excessive"));
        assert!(!ids(&f).contains(&"redirects.chain-long"));
        assert_eq!(f[0].severity, Severity::Critical);
        assert!(f[0].penalty > 0);
        assert_eq!(f[0].category, Category::Links);
    }

    #[test]
    fn chain_10_critical_only() {
        let f = audit(10);
        assert_eq!(f.len(), 1);
        assert!(ids(&f).contains(&"redirects.chain-excessive"));
        assert!(!ids(&f).contains(&"redirects.chain-long"));
    }
}
