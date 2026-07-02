use serde_json::Value;

use crate::audit::{PageAuditor, finding};
use crate::model::{Category, Finding, PageData, Severity};
use crate::parser::Dom;

pub struct StructuredDataAuditor;

impl PageAuditor for StructuredDataAuditor {
    fn id(&self) -> &'static str {
        "structured_data"
    }

    fn category(&self) -> Category {
        Category::Metadata
    }

    fn audit(&self, _page: &PageData, dom: &Dom) -> Vec<Finding> {
        let mut out = Vec::new();
        for block in dom.json_ld_blocks() {
            match serde_json::from_str::<Value>(&block) {
                Err(_) => out.push(finding(
                    "structured_data.malformed_json",
                    Category::Metadata,
                    Severity::Warning,
                    10,
                    "JSON-LD block contains malformed JSON",
                )),
                Ok(value) => validate_node(&value, &mut out),
            }
        }
        out
    }
}

/// Walk a parsed JSON-LD value, dispatching to `validate_object` for each node.
fn validate_node(node: &Value, out: &mut Vec<Finding>) {
    match node {
        Value::Array(arr) => {
            for item in arr {
                validate_node(item, out);
            }
        }
        Value::Object(obj) => {
            // If object has a @graph key that is an array, validate each element.
            if let Some(Value::Array(graph)) = obj.get("@graph") {
                for item in graph {
                    validate_node(item, out);
                }
            } else {
                validate_object(obj, out);
            }
        }
        // Primitive JSON values are not JSON-LD nodes — nothing to validate.
        _ => {}
    }
}

/// Validate a single JSON-LD object node for missing @context, @type, and
/// type-specific required fields.
fn validate_object(obj: &serde_json::Map<String, Value>, out: &mut Vec<Finding>) {
    if !obj.contains_key("@context") {
        out.push(finding(
            "structured_data.missing_context",
            Category::Metadata,
            Severity::Info,
            5,
            "JSON-LD block is missing @context",
        ));
    }

    let type_value = match obj.get("@type") {
        None => {
            out.push(finding(
                "structured_data.missing_type",
                Category::Metadata,
                Severity::Warning,
                10,
                "JSON-LD block is missing @type",
            ));
            return;
        }
        Some(v) => v,
    };

    // @type may be a string or an array of strings.
    let types: Vec<&str> = match type_value {
        Value::String(s) => vec![s.as_str()],
        Value::Array(arr) => arr.iter().filter_map(|v| v.as_str()).collect(),
        _ => vec![],
    };

    for type_name in &types {
        check_required_fields(type_name, obj, out);
    }
}

/// Emit `structured_data.missing_required_field` for each required field absent
/// from the given object, based on its @type.
fn check_required_fields(
    type_name: &str,
    obj: &serde_json::Map<String, Value>,
    out: &mut Vec<Finding>,
) {
    let required: &[&str] = match type_name {
        "Article" | "BlogPosting" | "NewsArticle" => &["headline", "author", "datePublished"],
        "Product" => &["name"],
        "BreadcrumbList" => &["itemListElement"],
        "FAQPage" => &["mainEntity"],
        // Unknown @type — no required-field findings.
        _ => return,
    };

    for &field in required {
        let missing = match obj.get(field) {
            None => true,
            // itemListElement must be a non-empty array.
            Some(Value::Array(arr)) if arr.is_empty() => true,
            _ => false,
        };

        if missing {
            out.push(finding(
                "structured_data.missing_required_field",
                Category::Metadata,
                Severity::Warning,
                10,
                format!("{type_name} is missing required field: {field}"),
            ));
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

    // Helper: wrap JSON in a script block.
    fn html_with_jsonld(json: &str) -> String {
        format!(
            r#"<html><head><script type="application/ld+json">{json}</script></head><body></body></html>"#
        )
    }

    // --- no blocks ---
    #[test]
    fn no_json_ld_blocks_zero_findings() {
        let f = audit("<html><head></head><body></body></html>");
        assert!(f.is_empty(), "expected no findings, got: {f:?}");
    }

    // --- malformed JSON ---
    #[test]
    fn malformed_json_one_finding() {
        let f = audit(&html_with_jsonld("{ this is not json }"));
        assert_eq!(f.len(), 1);
        assert_eq!(ids(&f), vec!["structured_data.malformed_json"]);
        assert_eq!(f[0].severity, Severity::Warning);
        assert_eq!(f[0].penalty, 10);
        assert_eq!(f[0].category, Category::Metadata);
    }

    // --- valid Article — zero findings ---
    #[test]
    fn valid_article_zero_findings() {
        let json = r#"{
            "@context": "https://schema.org",
            "@type": "Article",
            "headline": "My Article",
            "author": {"@type": "Person", "name": "Alice"},
            "datePublished": "2024-01-01"
        }"#;
        let f = audit(&html_with_jsonld(json));
        assert!(
            f.is_empty(),
            "expected no findings for valid Article, got: {f:?}"
        );
    }

    // --- missing @context ---
    #[test]
    fn missing_context_info_finding() {
        let json =
            r#"{"@type": "Article", "headline": "H", "author": "A", "datePublished": "2024"}"#;
        let f = audit(&html_with_jsonld(json));
        assert!(
            ids(&f).contains(&"structured_data.missing_context"),
            "expected missing_context, got: {ids:?}",
            ids = ids(&f)
        );
        let ctx = f
            .iter()
            .find(|f| f.check_id == "structured_data.missing_context")
            .unwrap();
        assert_eq!(ctx.severity, Severity::Info);
        assert_eq!(ctx.penalty, 5);
        assert_eq!(ctx.category, Category::Metadata);
    }

    // --- missing @type ---
    #[test]
    fn missing_type_warning_finding() {
        let json = r#"{"@context": "https://schema.org", "headline": "H"}"#;
        let f = audit(&html_with_jsonld(json));
        assert_eq!(ids(&f), vec!["structured_data.missing_type"]);
        assert_eq!(f[0].severity, Severity::Warning);
        assert_eq!(f[0].penalty, 10);
        assert_eq!(f[0].category, Category::Metadata);
    }

    // --- missing @type also returns early, no required-field checks ---
    #[test]
    fn missing_type_no_required_field_findings() {
        let json = r#"{"@context": "https://schema.org"}"#;
        let f = audit(&html_with_jsonld(json));
        assert!(
            !ids(&f).contains(&"structured_data.missing_required_field"),
            "should not emit required_field when @type is absent"
        );
    }

    // --- Product missing name ---
    #[test]
    fn product_missing_name_one_finding() {
        let json = r#"{"@context": "https://schema.org", "@type": "Product"}"#;
        let f = audit(&html_with_jsonld(json));
        let rf: Vec<_> = f
            .iter()
            .filter(|f| f.check_id == "structured_data.missing_required_field")
            .collect();
        assert_eq!(rf.len(), 1, "expected exactly 1 required_field finding");
        assert!(rf[0].message.contains("Product"));
        assert!(rf[0].message.contains("name"));
        assert_eq!(rf[0].severity, Severity::Warning);
        assert_eq!(rf[0].penalty, 10);
        assert_eq!(rf[0].category, Category::Metadata);
    }

    // --- unknown @type — zero required-field findings ---
    #[test]
    fn unknown_type_no_required_field_findings() {
        let json = r#"{"@context": "https://schema.org", "@type": "WebPage"}"#;
        let f = audit(&html_with_jsonld(json));
        assert!(
            !ids(&f).contains(&"structured_data.missing_required_field"),
            "unknown @type must not emit required_field findings"
        );
    }

    // --- @graph with mixed nodes ---
    #[test]
    fn graph_validates_each_element() {
        let json = r#"{
            "@context": "https://schema.org",
            "@graph": [
                {"@type": "Product"},
                {"@type": "BreadcrumbList"},
                {"@type": "FAQPage"}
            ]
        }"#;
        let f = audit(&html_with_jsonld(json));
        let rf: Vec<_> = f
            .iter()
            .filter(|f| f.check_id == "structured_data.missing_required_field")
            .collect();
        // Product → name, BreadcrumbList → itemListElement, FAQPage → mainEntity
        assert_eq!(
            rf.len(),
            3,
            "expected 3 required_field findings, got: {rf:?}"
        );
        let msgs: Vec<&str> = rf.iter().map(|f| f.message.as_str()).collect();
        assert!(
            msgs.iter()
                .any(|m| m.contains("Product") && m.contains("name"))
        );
        assert!(
            msgs.iter()
                .any(|m| m.contains("BreadcrumbList") && m.contains("itemListElement"))
        );
        assert!(
            msgs.iter()
                .any(|m| m.contains("FAQPage") && m.contains("mainEntity"))
        );
    }

    // --- @graph nodes share @context from outer object, inner nodes missing @context ---
    #[test]
    fn graph_elements_missing_context_emit_finding() {
        let json = r#"{
            "@context": "https://schema.org",
            "@graph": [
                {"@type": "Product", "name": "Widget"}
            ]
        }"#;
        // The @graph elements don't have their own @context; validate_object checks each one.
        let f = audit(&html_with_jsonld(json));
        // Product/name present → no required_field. But the inner node lacks @context.
        assert!(
            ids(&f).contains(&"structured_data.missing_context"),
            "graph element without @context should emit missing_context"
        );
    }

    // --- Article missing headline and author ---
    #[test]
    fn article_missing_fields_findings() {
        let json = r#"{
            "@context": "https://schema.org",
            "@type": "Article",
            "datePublished": "2024-01-01"
        }"#;
        let f = audit(&html_with_jsonld(json));
        let rf: Vec<_> = f
            .iter()
            .filter(|f| f.check_id == "structured_data.missing_required_field")
            .collect();
        assert_eq!(
            rf.len(),
            2,
            "expected 2 missing field findings (headline + author)"
        );
        let msgs: Vec<&str> = rf.iter().map(|f| f.message.as_str()).collect();
        assert!(msgs.iter().any(|m| m.contains("headline")));
        assert!(msgs.iter().any(|m| m.contains("author")));
    }

    // --- BreadcrumbList with empty itemListElement array ---
    #[test]
    fn breadcrumb_empty_list_element_finding() {
        let json = r#"{
            "@context": "https://schema.org",
            "@type": "BreadcrumbList",
            "itemListElement": []
        }"#;
        let f = audit(&html_with_jsonld(json));
        let rf: Vec<_> = f
            .iter()
            .filter(|f| f.check_id == "structured_data.missing_required_field")
            .collect();
        assert_eq!(rf.len(), 1);
        assert!(rf[0].message.contains("itemListElement"));
    }

    // --- all findings have Category::Metadata ---
    #[test]
    fn all_findings_category_metadata() {
        let html = r#"<html><head>
            <script type="application/ld+json">{bad json}</script>
            <script type="application/ld+json">{"@context":"https://schema.org","@type":"Product"}</script>
            </head><body></body></html>"#;
        let f = audit(html);
        assert!(!f.is_empty());
        for finding in &f {
            assert_eq!(
                finding.category,
                Category::Metadata,
                "finding {:?} has wrong category",
                finding.check_id
            );
        }
    }

    // --- fixture-based tests ---
    #[test]
    fn fixture_article_valid_zero_findings() {
        let html = include_str!("../../tests/fixtures/jsonld_article_valid.html");
        let f = audit(html);
        assert!(
            f.is_empty(),
            "valid Article fixture should produce no findings, got: {f:?}"
        );
    }

    #[test]
    fn fixture_malformed_one_finding() {
        let html = include_str!("../../tests/fixtures/jsonld_malformed.html");
        let f = audit(html);
        assert_eq!(f.len(), 1);
        assert_eq!(f[0].check_id, "structured_data.malformed_json");
    }

    #[test]
    fn fixture_missing_context_finding() {
        let html = include_str!("../../tests/fixtures/jsonld_missing_context.html");
        let f = audit(html);
        assert!(ids(&f).contains(&"structured_data.missing_context"));
    }

    #[test]
    fn fixture_missing_type_finding() {
        let html = include_str!("../../tests/fixtures/jsonld_missing_type.html");
        let f = audit(html);
        assert!(ids(&f).contains(&"structured_data.missing_type"));
    }

    #[test]
    fn fixture_article_missing_fields() {
        let html = include_str!("../../tests/fixtures/jsonld_article_missing_fields.html");
        let f = audit(html);
        let rf: Vec<_> = f
            .iter()
            .filter(|f| f.check_id == "structured_data.missing_required_field")
            .collect();
        assert!(
            rf.len() >= 2,
            "expected at least 2 missing required field findings"
        );
    }

    #[test]
    fn fixture_graph_mixed() {
        let html = include_str!("../../tests/fixtures/jsonld_graph.html");
        let f = audit(html);
        let rf: Vec<_> = f
            .iter()
            .filter(|f| f.check_id == "structured_data.missing_required_field")
            .collect();
        assert!(
            rf.len() >= 3,
            "expected at least 3 required_field findings from @graph fixture, got: {rf:?}"
        );
    }
}
