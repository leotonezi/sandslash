use crate::audit::{PageAuditor, finding};
use crate::model::{Category, Finding, PageData, Severity};
use crate::parser::Dom;

pub struct OpengraphAuditor;

impl PageAuditor for OpengraphAuditor {
    fn id(&self) -> &'static str {
        "opengraph"
    }
    fn category(&self) -> Category {
        Category::SocialTags
    }

    fn audit(&self, _page: &PageData, dom: &Dom) -> Vec<Finding> {
        let mut out = Vec::new();
        check_og_tag(dom, "og:title", "og.title.missing", &mut out);
        check_og_tag(dom, "og:description", "og.description.missing", &mut out);
        check_og_tag(dom, "og:image", "og.image.missing", &mut out);
        check_og_tag(dom, "og:url", "og.url.missing", &mut out);
        check_twitter_card(dom, &mut out);
        out
    }
}

fn check_og_tag(dom: &Dom, property: &str, check_id: &'static str, out: &mut Vec<Finding>) {
    let present = dom
        .meta_property(property)
        .map(|v| !v.trim().is_empty())
        .unwrap_or(false);
    if !present {
        out.push(finding(
            check_id,
            Category::SocialTags,
            Severity::Info,
            10,
            format!("Missing or empty OpenGraph tag: {property}"),
        ));
    }
}

fn check_twitter_card(dom: &Dom, out: &mut Vec<Finding>) {
    let present = dom
        .meta_name("twitter:card")
        .map(|v| !v.trim().is_empty())
        .unwrap_or(false);
    if !present {
        out.push(finding(
            "twitter.card.missing",
            Category::SocialTags,
            Severity::Info,
            10,
            "Missing or empty meta tag: twitter:card",
        ));
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
        OpengraphAuditor.audit(&page(), &Dom::parse(html))
    }

    fn ids(findings: &[Finding]) -> Vec<&str> {
        findings.iter().map(|f| f.check_id).collect()
    }

    const ALL_TAGS_HTML: &str = include_str!("../../tests/fixtures/opengraph_all.html");

    const NO_TAGS_HTML: &str = include_str!("../../tests/fixtures/opengraph_none.html");

    const MISSING_IMAGE_HTML: &str =
        include_str!("../../tests/fixtures/opengraph_missing_image.html");

    const EMPTY_CONTENT_HTML: &str = r#"<!DOCTYPE html>
<html>
<head>
  <meta property="og:title" content="">
  <meta property="og:description" content="">
  <meta property="og:image" content="">
  <meta property="og:url" content="">
  <meta name="twitter:card" content="">
</head>
<body></body>
</html>"#;

    #[test]
    fn all_tags_present_zero_findings() {
        let f = audit(ALL_TAGS_HTML);
        assert!(f.is_empty(), "expected no findings, got: {f:?}");
    }

    #[test]
    fn no_tags_five_findings() {
        let f = audit(NO_TAGS_HTML);
        assert_eq!(f.len(), 5, "expected 5 findings, got: {f:?}");
        let id_list = ids(&f);
        assert!(id_list.contains(&"og.title.missing"));
        assert!(id_list.contains(&"og.description.missing"));
        assert!(id_list.contains(&"og.image.missing"));
        assert!(id_list.contains(&"og.url.missing"));
        assert!(id_list.contains(&"twitter.card.missing"));
    }

    #[test]
    fn only_og_image_missing_one_finding() {
        let f = audit(MISSING_IMAGE_HTML);
        assert_eq!(f.len(), 1, "expected 1 finding, got: {f:?}");
        assert_eq!(f[0].check_id, "og.image.missing");
    }

    #[test]
    fn empty_content_treated_as_missing() {
        let f = audit(EMPTY_CONTENT_HTML);
        assert_eq!(
            f.len(),
            5,
            "expected 5 findings for empty content, got: {f:?}"
        );
    }

    #[test]
    fn all_findings_have_social_tags_category() {
        let f = audit(NO_TAGS_HTML);
        for finding in &f {
            assert_eq!(
                finding.category,
                Category::SocialTags,
                "finding {} has wrong category",
                finding.check_id
            );
        }
    }

    #[test]
    fn all_findings_have_info_severity_and_penalty_10() {
        let f = audit(NO_TAGS_HTML);
        for finding in &f {
            assert_eq!(
                finding.severity,
                Severity::Info,
                "finding {} has wrong severity",
                finding.check_id
            );
            assert_eq!(
                finding.penalty, 10,
                "finding {} has wrong penalty",
                finding.check_id
            );
        }
    }

    #[test]
    fn auditor_id_and_category() {
        assert_eq!(OpengraphAuditor.id(), "opengraph");
        assert_eq!(OpengraphAuditor.category(), Category::SocialTags);
    }
}
