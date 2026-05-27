use crate::audit::{PageAuditor, finding};
use crate::model::{Category, Finding, PageData, Severity};
use crate::parser::Dom;

pub struct HttpsAuditor;

impl PageAuditor for HttpsAuditor {
    fn id(&self) -> &'static str {
        "https"
    }
    fn category(&self) -> Category {
        Category::Security
    }

    fn audit(&self, page: &PageData, dom: &Dom) -> Vec<Finding> {
        let mut out = Vec::new();

        if page.url.scheme() != "https" {
            out.push(finding(
                "https.insecure",
                Category::Security,
                Severity::Critical,
                40,
                format!("Page served over {}; HTTPS required", page.url.scheme()),
            ));
            return out; // mixed-content only relevant on https pages
        }

        let mixed: Vec<String> = dom
            .resource_urls()
            .into_iter()
            .filter(|u| u.starts_with("http://"))
            .collect();

        for url in mixed {
            out.push(finding(
                "https.mixed-content",
                Category::Security,
                Severity::Warning,
                20,
                format!("Mixed content: http resource on https page — {url}"),
            ));
        }

        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::Headers;

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
        HttpsAuditor.audit(&page(url), &Dom::parse(html))
    }

    fn ids(findings: &[Finding]) -> Vec<&str> {
        findings.iter().map(|f| f.check_id).collect()
    }

    #[test]
    fn http_page_is_critical() {
        let f = audit("<html></html>", "http://example.com/");
        assert!(ids(&f).contains(&"https.insecure"));
    }

    #[test]
    fn https_page_clean() {
        let f = audit(
            r#"<html><body><img src="https://cdn.example.com/img.jpg"></body></html>"#,
            "https://example.com/",
        );
        assert!(f.is_empty());
    }

    #[test]
    fn mixed_content_flagged() {
        let f = audit(
            r#"<html><body><img src="http://cdn.example.com/img.jpg"></body></html>"#,
            "https://example.com/",
        );
        assert!(ids(&f).contains(&"https.mixed-content"));
    }

    #[test]
    fn no_mixed_content_on_http_page() {
        // http page gets insecure finding but NOT mixed-content (irrelevant there)
        let f = audit(
            r#"<html><body><img src="http://cdn.example.com/img.jpg"></body></html>"#,
            "http://example.com/",
        );
        assert!(ids(&f).contains(&"https.insecure"));
        assert!(!ids(&f).contains(&"https.mixed-content"));
    }
}
