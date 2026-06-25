use std::collections::HashMap;
use std::sync::LazyLock;

use serde_json::Value;

use crate::audit::{PageAuditor, finding};
use crate::model::{Category, Finding, PageData, Severity};
use crate::parser::Dom;

/// Required fields for each known schema.org type.
static REQUIRED_FIELDS: LazyLock<HashMap<&'static str, &'static [&'static str]>> =
    LazyLock::new(|| {
        let mut m = HashMap::new();
        m.insert(
            "Article",
            &["headline", "author", "datePublished"] as &[&str],
        );
        m.insert(
            "NewsArticle",
            &["headline", "author", "datePublished"] as &[&str],
        );
        m.insert(
            "BlogPosting",
            &["headline", "author", "datePublished"] as &[&str],
        );
        m.insert("Product", &["name"] as &[&str]);
        m.insert("BreadcrumbList", &["itemListElement"] as &[&str]);
        m.insert("FAQPage", &["mainEntity"] as &[&str]);
        m.insert("HowTo", &["name", "step"] as &[&str]);
        m.insert("Organization", &["name"] as &[&str]);
        m.insert("WebSite", &["name", "url"] as &[&str]);
        m.insert("LocalBusiness", &["name", "address"] as &[&str]);
        m
    });

pub struct StructuredDataAuditor;

impl PageAuditor for StructuredDataAuditor {
    fn id(&self) -> &'static str {
        "structured-data"
    }

    fn category(&self) -> Category {
        Category::Structure
    }

    fn audit(&self, _page: &PageData, dom: &Dom) -> Vec<Finding> {
        let mut findings = Vec::new();

        for raw in dom.json_ld_blocks() {
            let value: Value = match serde_json::from_str(&raw) {
                Ok(v) => v,
                Err(_) => {
                    findings.push(finding(
                        "jsonld.malformed",
                        Category::Structure,
                        Severity::Warning,
                        10,
                        "JSON-LD block could not be parsed as valid JSON",
                    ));
                    continue;
                }
            };

            // Flatten @graph if present; otherwise treat root as single node.
            if let Some(graph) = value
                .as_object()
                .and_then(|o| o.get("@graph"))
                .and_then(|g| g.as_array())
            {
                for node in graph {
                    validate_node(node, &mut findings);
                }
            } else {
                validate_node(&value, &mut findings);
            }
        }

        findings
    }
}

fn validate_node(node: &Value, findings: &mut Vec<Finding>) {
    // Collect type strings.
    let types: Vec<&str> = match node.get("@type") {
        Some(Value::String(s)) => vec![s.as_str()],
        Some(Value::Array(arr)) => arr.iter().filter_map(|v| v.as_str()).collect(),
        _ => {
            findings.push(finding(
                "jsonld.missing-type",
                Category::Structure,
                Severity::Warning,
                10,
                "JSON-LD node has no @type",
            ));
            return;
        }
    };

    for type_str in types {
        match REQUIRED_FIELDS.get(type_str) {
            None => {
                findings.push(finding(
                    "jsonld.unknown-type",
                    Category::Structure,
                    Severity::Info,
                    0,
                    format!("unknown schema.org type: {type_str}"),
                ));
            }
            Some(required) => {
                for field in *required {
                    if node.as_object().and_then(|o| o.get(*field)).is_none() {
                        findings.push(finding(
                            "jsonld.missing-required",
                            Category::Structure,
                            Severity::Warning,
                            10,
                            format!("{type_str} missing required field: {field}"),
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
        let dom = Dom::parse(html);
        StructuredDataAuditor.audit(&page(), &dom)
    }

    fn ids(findings: &[Finding]) -> Vec<&str> {
        findings.iter().map(|f| f.check_id.as_str()).collect()
    }

    // --- fixture-based tests ---

    #[test]
    fn valid_article_zero_findings() {
        let html = include_str!("../../tests/fixtures/jsonld_valid_article.html");
        let f = audit(html);
        assert!(f.is_empty(), "expected zero findings, got: {:?}", ids(&f));
    }

    #[test]
    fn malformed_json_emits_malformed() {
        let html = include_str!("../../tests/fixtures/jsonld_malformed.html");
        let f = audit(html);
        assert!(
            ids(&f).contains(&"jsonld.malformed"),
            "expected jsonld.malformed, got: {:?}",
            ids(&f)
        );
        assert_eq!(f.len(), 1, "expected exactly one finding");
    }

    #[test]
    fn missing_type_emits_missing_type() {
        let html = include_str!("../../tests/fixtures/jsonld_missing_type.html");
        let f = audit(html);
        assert!(
            ids(&f).contains(&"jsonld.missing-type"),
            "expected jsonld.missing-type, got: {:?}",
            ids(&f)
        );
        assert_eq!(f.len(), 1, "expected exactly one finding");
    }

    #[test]
    fn unknown_type_emits_unknown_type() {
        let html = include_str!("../../tests/fixtures/jsonld_unknown_type.html");
        let f = audit(html);
        assert!(
            ids(&f).contains(&"jsonld.unknown-type"),
            "expected jsonld.unknown-type, got: {:?}",
            ids(&f)
        );
        assert_eq!(f.len(), 1, "expected exactly one finding");
    }

    #[test]
    fn missing_required_field_emits_missing_required() {
        let html = include_str!("../../tests/fixtures/jsonld_missing_required.html");
        let f = audit(html);
        assert!(
            ids(&f).contains(&"jsonld.missing-required"),
            "expected jsonld.missing-required, got: {:?}",
            ids(&f)
        );
        assert_eq!(f.len(), 1, "expected exactly one finding");
    }

    #[test]
    fn graph_valid_article_and_product_missing_name() {
        let html = include_str!("../../tests/fixtures/jsonld_graph.html");
        let f = audit(html);
        // Valid Article → no findings; Product missing name → one finding.
        assert!(
            ids(&f).contains(&"jsonld.missing-required"),
            "expected jsonld.missing-required, got: {:?}",
            ids(&f)
        );
        assert_eq!(f.len(), 1, "expected exactly one finding");
    }

    #[test]
    fn no_jsonld_zero_findings() {
        let html = include_str!("../../tests/fixtures/jsonld_none.html");
        let f = audit(html);
        assert!(
            f.is_empty(),
            "expected zero findings for page with no JSON-LD, got: {:?}",
            ids(&f)
        );
    }

    // --- inline test: @type as array form ---

    #[test]
    fn type_as_array_validates_each_type() {
        let html = r#"<!DOCTYPE html>
<html><head>
<script type="application/ld+json">
{
  "@context": "https://schema.org",
  "@type": ["Article", "NewsArticle"],
  "headline": "Multi-type Article",
  "author": {"@type": "Person", "name": "Alice"},
  "datePublished": "2024-01-15"
}
</script>
</head><body></body></html>"#;
        let f = audit(html);
        assert!(
            f.is_empty(),
            "expected zero findings for valid multi-type node, got: {:?}",
            ids(&f)
        );
    }

    #[test]
    fn type_as_array_emits_findings_for_missing_fields_per_type() {
        // BlogPosting requires headline, author, datePublished.
        // Organization requires name.
        // Neither is satisfied here.
        let html = r#"<!DOCTYPE html>
<html><head>
<script type="application/ld+json">
{
  "@context": "https://schema.org",
  "@type": ["BlogPosting", "Organization"]
}
</script>
</head><body></body></html>"#;
        let f = audit(html);
        let check_ids = ids(&f);
        // BlogPosting: headline, author, datePublished missing (3 findings)
        // Organization: name missing (1 finding)
        assert_eq!(
            check_ids
                .iter()
                .filter(|&&id| id == "jsonld.missing-required")
                .count(),
            4,
            "expected 4 missing-required findings, got: {check_ids:?}",
        );
    }
}
