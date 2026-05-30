use sandslash::audit::https::HttpsAuditor;
use sandslash::audit::PageAuditor;
use sandslash::model::{Category, Headers, PageData, Severity};
use sandslash::parser::Dom;

const MIXED_CONTENT: &str = include_str!("fixtures/https_mixed_content.html");
const CLEAN: &str = include_str!("fixtures/https_clean.html");

// The mixed-content fixture contains exactly 10 http:// sub-resource URLs:
//   img[src], script[src], link[href], iframe[src], audio[src], video[src],
//   source[src] (inside video), track[src], embed[src], object[data]
const MIXED_HTTP_COUNT: usize = 10;

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

/// Every http:// sub-resource on an https page must produce an https.mixed-content finding.
#[test]
fn mixed_content_fixture_flags_each_http_resource() {
    let findings = HttpsAuditor.audit(&page("https://example.com/"), &Dom::parse(MIXED_CONTENT));

    assert_eq!(
        findings.len(),
        MIXED_HTTP_COUNT,
        "expected {MIXED_HTTP_COUNT} mixed-content findings, got {}",
        findings.len()
    );

    for f in &findings {
        assert_eq!(
            f.check_id, "https.mixed-content",
            "unexpected check_id: {}",
            f.check_id
        );
        assert_eq!(f.category, Category::Security);
        assert_eq!(f.severity, Severity::Warning);
    }
}

/// An https page with only https:// and protocol-relative resources must produce zero findings.
#[test]
fn clean_fixture_emits_no_findings() {
    let findings = HttpsAuditor.audit(&page("https://example.com/"), &Dom::parse(CLEAN));
    assert!(
        findings.is_empty(),
        "expected no findings for clean https fixture, got: {findings:?}"
    );
}

/// An http:// page must emit https.insecure and never https.mixed-content.
#[test]
fn http_page_flags_insecure_not_mixed() {
    let findings = HttpsAuditor.audit(&page("http://example.com/"), &Dom::parse(MIXED_CONTENT));

    assert_eq!(
        findings.len(),
        1,
        "expected exactly 1 finding for http page, got {}",
        findings.len()
    );
    assert_eq!(findings[0].check_id, "https.insecure");
    assert!(
        findings.iter().all(|f| f.check_id != "https.mixed-content"),
        "http page must not emit https.mixed-content"
    );
}
