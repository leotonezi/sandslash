use std::collections::HashMap;
use std::sync::LazyLock;

use serde_json::Value;

use crate::audit::{PageAuditor, finding};
use crate::model::{Category, Finding, PageData, Severity};
use crate::parser::Dom;

pub struct StructuredDataAuditor;

/// Required fields per validated Schema.org @type.
///
/// Looked up once at first use; never rebuilt per call.
static REQUIRED_FIELDS: LazyLock<HashMap<&'static str, &'static [&'static str]>> =
    LazyLock::new(|| {
        let mut m: HashMap<&'static str, &'static [&'static str]> = HashMap::new();
        m.insert("Article", &["headline", "author", "datePublished"]);
        m.insert("NewsArticle", &["headline", "author", "datePublished"]);
        m.insert("BlogPosting", &["headline", "author", "datePublished"]);
        m.insert("Product", &["name"]);
        m.insert("BreadcrumbList", &["itemListElement"]);
        m.insert("FAQPage", &["mainEntity"]);
        m.insert("HowTo", &["name", "step"]);
        m.insert("Organization", &["name"]);
        m.insert("WebSite", &["name", "url"]);
        m.insert("LocalBusiness", &["name", "address"]);
        m
    });

impl PageAuditor for StructuredDataAuditor {
    fn id(&self) -> &'static str {
        "structured_data"
    }

    fn category(&self) -> Category {
        Category::Structure
    }

    fn audit(&self, _page: &PageData, dom: &Dom) -> Vec<Finding> {
        let mut out = Vec::new();
        for block in dom.json_ld_blocks() {
            // Skip empty or whitespace-only bodies — not an error.
            if block.trim().is_empty() {
                continue;
            }
            match serde_json::from_str::<Value>(&block) {
                Err(_) => out.push(finding(
                    "jsonld.malformed",
                    Category::Structure,
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
///
/// `@graph` arrays are flattened: the wrapper object is not validated as an
/// entity; each element is validated independently.
fn validate_node(node: &Value, out: &mut Vec<Finding>) {
    match node {
        Value::Array(arr) => {
            for item in arr {
                validate_node(item, out);
            }
        }
        Value::Object(obj) => {
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

/// Validate a single JSON-LD object node for missing `@type` and type-specific
/// required fields.  Types not in the validated set emit `jsonld.unknown-type`.
fn validate_object(obj: &serde_json::Map<String, Value>, out: &mut Vec<Finding>) {
    let type_value = match obj.get("@type") {
        None => {
            out.push(finding(
                "jsonld.missing-type",
                Category::Structure,
                Severity::Warning,
                10,
                "JSON-LD block is missing @type",
            ));
            return;
        }
        Some(v) => v,
    };

    // @type may be a string or an array of strings; other shapes are silently ignored.
    let types: Vec<&str> = match type_value {
        Value::String(s) => vec![s.as_str()],
        Value::Array(arr) => arr.iter().filter_map(|v| v.as_str()).collect(),
        _ => vec![],
    };

    for type_name in &types {
        if REQUIRED_FIELDS.contains_key(type_name) {
            check_required_fields(type_name, obj, out);
        } else {
            out.push(finding(
                "jsonld.unknown-type",
                Category::Structure,
                Severity::Info,
                0,
                format!("JSON-LD @type \"{type_name}\" is not in the validated set"),
            ));
        }
    }
}

/// Emit `jsonld.missing-required` for each required field absent from `obj`,
/// based on the given `type_name`.  Empty arrays are treated as absent.
fn check_required_fields(
    type_name: &str,
    obj: &serde_json::Map<String, Value>,
    out: &mut Vec<Finding>,
) {
    let required = match REQUIRED_FIELDS.get(type_name) {
        Some(fields) => *fields,
        None => return,
    };

    for &field in required {
        let missing = match obj.get(field) {
            None => true,
            // Empty array counts as missing (e.g. BreadcrumbList.itemListElement).
            Some(Value::Array(arr)) if arr.is_empty() => true,
            _ => false,
        };

        if missing {
            out.push(finding(
                "jsonld.missing-required",
                Category::Structure,
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

    fn html_with_jsonld(json: &str) -> String {
        format!(
            r#"<html><head><script type="application/ld+json">{json}</script></head><body></body></html>"#
        )
    }

    // --- no JSON-LD blocks ---
    #[test]
    fn no_json_ld_blocks_zero_findings() {
        let f = audit("<html><head></head><body></body></html>");
        assert!(f.is_empty(), "expected no findings, got: {f:?}");
    }

    // --- empty / whitespace-only block — skip, no finding ---
    #[test]
    fn empty_block_zero_findings() {
        let f = audit(
            r#"<html><head><script type="application/ld+json">   </script></head><body></body></html>"#,
        );
        assert!(
            f.is_empty(),
            "empty block must produce no findings, got: {f:?}"
        );
    }

    // --- jsonld.malformed ---
    #[test]
    fn malformed_json_finding() {
        let f = audit(&html_with_jsonld("{ this is not json }"));
        assert_eq!(f.len(), 1);
        assert_eq!(f[0].check_id, "jsonld.malformed");
        assert_eq!(f[0].severity, Severity::Warning);
        assert_eq!(f[0].penalty, 10);
        assert_eq!(f[0].category, Category::Structure);
    }

    // --- jsonld.missing-type ---
    #[test]
    fn missing_type_finding() {
        let json = r#"{"headline": "H"}"#;
        let f = audit(&html_with_jsonld(json));
        assert_eq!(ids(&f), vec!["jsonld.missing-type"]);
        assert_eq!(f[0].severity, Severity::Warning);
        assert_eq!(f[0].penalty, 10);
        assert_eq!(f[0].category, Category::Structure);
    }

    // --- missing @type returns early — no required-field check ---
    #[test]
    fn missing_type_no_required_field_findings() {
        let json = r#"{"headline": "H"}"#;
        let f = audit(&html_with_jsonld(json));
        assert!(
            !ids(&f).contains(&"jsonld.missing-required"),
            "must not emit jsonld.missing-required when @type is absent"
        );
    }

    // --- jsonld.unknown-type ---
    #[test]
    fn unknown_type_finding() {
        let json = r#"{"@type": "WebPage"}"#;
        let f = audit(&html_with_jsonld(json));
        assert_eq!(ids(&f), vec!["jsonld.unknown-type"]);
        assert_eq!(f[0].severity, Severity::Info);
        assert_eq!(f[0].penalty, 0);
        assert_eq!(f[0].category, Category::Structure);
        assert!(
            f[0].message.contains("WebPage"),
            "message must name the type"
        );
    }

    // --- unknown @type — no required-field findings ---
    #[test]
    fn unknown_type_no_required_field_findings() {
        let json = r#"{"@type": "WebPage"}"#;
        let f = audit(&html_with_jsonld(json));
        assert!(
            !ids(&f).contains(&"jsonld.missing-required"),
            "unknown @type must not emit jsonld.missing-required"
        );
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

    // --- jsonld.missing-required: Product missing name ---
    #[test]
    fn product_missing_name_finding() {
        let json = r#"{"@type": "Product"}"#;
        let f = audit(&html_with_jsonld(json));
        let rf: Vec<_> = f
            .iter()
            .filter(|f| f.check_id == "jsonld.missing-required")
            .collect();
        assert_eq!(rf.len(), 1, "expected exactly 1 jsonld.missing-required");
        assert!(
            rf[0].message.contains("Product"),
            "message must name the type"
        );
        assert!(
            rf[0].message.contains("name"),
            "message must name the field"
        );
        assert_eq!(rf[0].severity, Severity::Warning);
        assert_eq!(rf[0].penalty, 10);
        assert_eq!(rf[0].category, Category::Structure);
    }

    // --- @type as array of strings — all required fields present ---
    #[test]
    fn type_as_array_zero_findings() {
        let json = r#"{"@type": ["Organization", "LocalBusiness"], "name": "Acme Corp", "address": "123 Main St"}"#;
        let f = audit(&html_with_jsonld(json));
        assert!(
            f.is_empty(),
            "all required fields present for both types, expected zero findings, got: {f:?}"
        );
    }

    // --- @type as array — one type missing a required field ---
    #[test]
    fn type_as_array_missing_field() {
        // Organization requires name (present); LocalBusiness requires name+address (address absent).
        let json = r#"{"@type": ["Organization", "LocalBusiness"], "name": "Acme Corp"}"#;
        let f = audit(&html_with_jsonld(json));
        let rf: Vec<_> = f
            .iter()
            .filter(|f| f.check_id == "jsonld.missing-required")
            .collect();
        assert_eq!(
            rf.len(),
            1,
            "expected exactly 1 missing-required for address"
        );
        assert!(rf[0].message.contains("LocalBusiness"));
        assert!(rf[0].message.contains("address"));
    }

    // --- @graph: each member validated independently ---
    #[test]
    fn graph_validates_each_element() {
        let json = r#"{
            "@context": "https://schema.org",
            "@graph": [
                {"@type": "Product"},
                {"@type": "BreadcrumbList", "itemListElement": []},
                {"@type": "FAQPage"}
            ]
        }"#;
        let f = audit(&html_with_jsonld(json));
        let rf: Vec<_> = f
            .iter()
            .filter(|f| f.check_id == "jsonld.missing-required")
            .collect();
        assert_eq!(
            rf.len(),
            3,
            "expected 3 jsonld.missing-required findings, got: {rf:?}"
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
            .filter(|f| f.check_id == "jsonld.missing-required")
            .collect();
        assert_eq!(
            rf.len(),
            2,
            "expected 2 jsonld.missing-required (headline + author)"
        );
        let msgs: Vec<&str> = rf.iter().map(|f| f.message.as_str()).collect();
        assert!(msgs.iter().any(|m| m.contains("headline")));
        assert!(msgs.iter().any(|m| m.contains("author")));
    }

    // --- BreadcrumbList with empty itemListElement ---
    #[test]
    fn breadcrumb_empty_list_element_finding() {
        let json = r#"{"@type": "BreadcrumbList", "itemListElement": []}"#;
        let f = audit(&html_with_jsonld(json));
        let rf: Vec<_> = f
            .iter()
            .filter(|f| f.check_id == "jsonld.missing-required")
            .collect();
        assert_eq!(rf.len(), 1);
        assert!(rf[0].message.contains("itemListElement"));
    }

    // --- all findings carry Category::Structure ---
    #[test]
    fn all_findings_category_structure() {
        let html = r#"<html><head>
            <script type="application/ld+json">{bad json}</script>
            <script type="application/ld+json">{"@type":"Product"}</script>
            </head><body></body></html>"#;
        let f = audit(html);
        assert!(!f.is_empty());
        for finding in &f {
            assert_eq!(
                finding.category,
                Category::Structure,
                "finding {:?} has wrong category",
                finding.check_id
            );
        }
    }

    // --- fixture: valid Article → zero findings ---
    #[test]
    fn fixture_valid_article_zero_findings() {
        let html = include_str!("../../tests/fixtures/jsonld_valid_article.html");
        let f = audit(html);
        assert!(
            f.is_empty(),
            "valid Article fixture must produce no findings, got: {f:?}"
        );
    }

    // --- fixture: malformed → jsonld.malformed ---
    #[test]
    fn fixture_malformed_finding() {
        let html = include_str!("../../tests/fixtures/jsonld_malformed.html");
        let f = audit(html);
        assert_eq!(f.len(), 1);
        assert_eq!(f[0].check_id, "jsonld.malformed");
        assert_eq!(f[0].category, Category::Structure);
    }

    // --- fixture: missing @type → jsonld.missing-type ---
    #[test]
    fn fixture_missing_type_finding() {
        let html = include_str!("../../tests/fixtures/jsonld_missing_type.html");
        let f = audit(html);
        assert!(
            ids(&f).contains(&"jsonld.missing-type"),
            "expected jsonld.missing-type, got: {ids:?}",
            ids = ids(&f)
        );
        assert!(
            f.iter()
                .filter(|f| f.check_id == "jsonld.missing-type")
                .all(|f| f.category == Category::Structure)
        );
    }

    // --- fixture: unknown @type → jsonld.unknown-type ---
    #[test]
    fn fixture_unknown_type_finding() {
        let html = include_str!("../../tests/fixtures/jsonld_unknown_type.html");
        let f = audit(html);
        assert!(
            ids(&f).contains(&"jsonld.unknown-type"),
            "expected jsonld.unknown-type, got: {ids:?}",
            ids = ids(&f)
        );
    }

    // --- fixture: Article missing required fields → jsonld.missing-required ---
    #[test]
    fn fixture_missing_required_findings() {
        let html = include_str!("../../tests/fixtures/jsonld_missing_required.html");
        let f = audit(html);
        let rf: Vec<_> = f
            .iter()
            .filter(|f| f.check_id == "jsonld.missing-required")
            .collect();
        assert!(
            rf.len() >= 2,
            "expected at least 2 jsonld.missing-required findings, got: {rf:?}"
        );
    }

    // --- fixture: @graph → 3 jsonld.missing-required ---
    #[test]
    fn fixture_graph_mixed() {
        let html = include_str!("../../tests/fixtures/jsonld_graph.html");
        let f = audit(html);
        let rf: Vec<_> = f
            .iter()
            .filter(|f| f.check_id == "jsonld.missing-required")
            .collect();
        assert!(
            rf.len() >= 3,
            "expected at least 3 jsonld.missing-required from @graph fixture, got: {rf:?}"
        );
    }

    // --- fixture: no JSON-LD blocks → zero findings ---
    #[test]
    fn fixture_none_zero_findings() {
        let html = include_str!("../../tests/fixtures/jsonld_none.html");
        let f = audit(html);
        assert!(
            f.is_empty(),
            "no-JSON-LD fixture must produce no findings, got: {f:?}"
        );
    }
}
