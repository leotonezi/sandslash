use std::collections::HashSet;

use url::Url;

use crate::parser::dom::Dom;

/// Query-string keys that must be stripped unconditionally (exact, case-sensitive match).
const EXACT_STRIP: &[&str] = &["fbclid", "gclid", "mc_eid", "ref", "igshid"];

/// Prefix of tracking keys that must be stripped (case-sensitive).
const PREFIX_STRIP: &str = "utm_";

/// Returns `true` if the query key should be removed.
fn is_tracking_key(key: &str) -> bool {
    if key.starts_with(PREFIX_STRIP) {
        return true;
    }
    EXACT_STRIP.contains(&key)
}

/// Produce a canonical, dedup-safe URL from `href` resolved against `base`.
///
/// Normalisation steps applied (in order):
/// 1. Resolve `href` relative to `base` via [`Url::join`].
/// 2. Return `None` if the resulting scheme is not `http` or `https`.
/// 3. Drop the fragment.
/// 4. Strip tracking query keys (exact: `fbclid`, `gclid`, `mc_eid`, `ref`,
///    `igshid`; prefix: `utm_`).
/// 5. Sort remaining query pairs alphabetically by key (stable).
/// 6. If no query pairs remain, set query to `None`.
///
/// # Idempotency
/// `normalize(base, normalize(base, x)?.as_str()) == normalize(base, x)`
pub fn normalize(base: &Url, href: &str) -> Option<Url> {
    let mut u = base.join(href).ok()?;

    // Only crawl http(s) pages.
    if u.scheme() != "http" && u.scheme() != "https" {
        return None;
    }

    // Drop fragment — fragments are client-side only.
    u.set_fragment(None);

    // Collect, filter, and sort query pairs.
    let pairs: Vec<(String, String)> = u
        .query_pairs()
        .filter(|(k, _)| !is_tracking_key(k))
        .map(|(k, v)| (k.into_owned(), v.into_owned()))
        .collect();

    if pairs.is_empty() {
        u.set_query(None);
    } else {
        // Stable sort preserves relative order of equal keys.
        let mut sorted = pairs;
        sorted.sort_by(|a, b| a.0.cmp(&b.0));

        // Rebuild query string using url's built-in serializer so percent-encoding
        // is consistent with the rest of the url crate's output.
        let query_string = sorted
            .iter()
            .enumerate()
            .fold(String::new(), |mut acc, (i, (k, v))| {
                if i > 0 {
                    acc.push('&');
                }
                acc.push_str(
                    &url::form_urlencoded::byte_serialize(k.as_bytes()).collect::<String>(),
                );
                acc.push('=');
                acc.push_str(
                    &url::form_urlencoded::byte_serialize(v.as_bytes()).collect::<String>(),
                );
                acc
            });
        u.set_query(Some(&query_string));
    }

    Some(u)
}

/// Returns `true` only when both URLs share an exact host string.
///
/// # Limitation
/// Compares `host_str()` exactly: `www.foo.com` and `foo.com` are considered
/// different sites. eTLD+1 / public-suffix-aware comparison is out of scope.
/// Returns `false` if either URL has no host (e.g. `file://`, `data:`).
pub fn is_same_site(a: &Url, b: &Url) -> bool {
    match (a.host_str(), b.host_str()) {
        (Some(ah), Some(bh)) => ah == bh,
        _ => false,
    }
}

/// Walk `dom.links()`, normalize each href against `base`, filter to same-host
/// URLs, and return a deduplicated list preserving insertion order of first
/// occurrences.
///
/// Self-links (URLs equal to `base` after normalization) are kept.
pub fn discover_links(base: &Url, dom: &Dom) -> Vec<Url> {
    let mut seen: HashSet<Url> = HashSet::new();
    let mut result: Vec<Url> = Vec::new();

    for href in dom.links() {
        if let Some(url) = normalize(base, &href) {
            if is_same_site(base, &url) && seen.insert(url.clone()) {
                result.push(url);
            }
        }
    }

    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::dom::Dom;

    fn base() -> Url {
        Url::parse("https://example.com/").expect("invariant: valid base URL")
    }

    fn base_page() -> Url {
        Url::parse("https://example.com/blog/post").expect("invariant: valid base URL")
    }

    // ── Scheme filtering ──────────────────────────────────────────────────

    #[test]
    fn returns_none_for_mailto() {
        assert!(normalize(&base(), "mailto:user@example.com").is_none());
    }

    #[test]
    fn returns_none_for_tel() {
        assert!(normalize(&base(), "tel:+1234567890").is_none());
    }

    #[test]
    fn returns_none_for_javascript() {
        assert!(normalize(&base(), "javascript:void(0)").is_none());
    }

    #[test]
    fn returns_none_for_data_uri() {
        assert!(normalize(&base(), "data:text/html,<h1>hi</h1>").is_none());
    }

    // ── Relative resolution ───────────────────────────────────────────────

    #[test]
    fn resolves_absolute_path() {
        let result = normalize(&base(), "/about").expect("should normalize");
        assert_eq!(result.as_str(), "https://example.com/about");
    }

    #[test]
    fn resolves_relative_path() {
        let result = normalize(&base_page(), "../news").expect("should normalize");
        assert_eq!(result.as_str(), "https://example.com/news");
    }

    #[test]
    fn returns_none_for_unsupported_scheme_ftp() {
        assert!(normalize(&base(), "ftp://files.example.com/file.zip").is_none());
    }

    // ── Fragment handling ─────────────────────────────────────────────────

    #[test]
    fn fragment_is_dropped() {
        let result = normalize(&base(), "/page#section").expect("should normalize");
        assert!(result.fragment().is_none());
        assert_eq!(result.as_str(), "https://example.com/page");
    }

    #[test]
    fn fragment_only_href_normalizes_to_base() {
        let result = normalize(&base(), "#top").expect("should normalize");
        assert!(result.fragment().is_none());
    }

    // ── Tracking param stripping ──────────────────────────────────────────

    #[test]
    fn strips_utm_source() {
        let result = normalize(&base(), "/p?utm_source=newsletter").expect("should normalize");
        assert_eq!(result.as_str(), "https://example.com/p");
    }

    #[test]
    fn strips_utm_medium_and_campaign() {
        let result =
            normalize(&base(), "/p?utm_medium=cpc&utm_campaign=spring").expect("should normalize");
        assert_eq!(result.as_str(), "https://example.com/p");
    }

    #[test]
    fn strips_fbclid() {
        let result = normalize(&base(), "/p?fbclid=abc123").expect("should normalize");
        assert_eq!(result.as_str(), "https://example.com/p");
    }

    #[test]
    fn strips_gclid() {
        let result = normalize(&base(), "/p?gclid=xyz").expect("should normalize");
        assert_eq!(result.as_str(), "https://example.com/p");
    }

    #[test]
    fn strips_mc_eid() {
        let result = normalize(&base(), "/p?mc_eid=abc").expect("should normalize");
        assert_eq!(result.as_str(), "https://example.com/p");
    }

    #[test]
    fn strips_ref_param() {
        let result = normalize(&base(), "/p?ref=twitter").expect("should normalize");
        assert_eq!(result.as_str(), "https://example.com/p");
    }

    #[test]
    fn strips_igshid() {
        let result = normalize(&base(), "/p?igshid=abc").expect("should normalize");
        assert_eq!(result.as_str(), "https://example.com/p");
    }

    #[test]
    fn preserves_non_tracking_params() {
        let result = normalize(&base(), "/search?q=rust&page=2").expect("should normalize");
        assert_eq!(result.as_str(), "https://example.com/search?page=2&q=rust");
    }

    #[test]
    fn mixes_tracking_and_real_params() {
        let result =
            normalize(&base(), "/p?id=42&utm_source=google&ref=home").expect("should normalize");
        assert_eq!(result.as_str(), "https://example.com/p?id=42");
    }

    // ── Query param sorting ───────────────────────────────────────────────

    #[test]
    fn query_params_sorted_alphabetically() {
        let result = normalize(&base(), "/s?z=last&a=first&m=mid").expect("should normalize");
        assert_eq!(
            result.as_str(),
            "https://example.com/s?a=first&m=mid&z=last"
        );
    }

    // ── Trailing slash / root path ────────────────────────────────────────

    #[test]
    fn root_path_always_has_trailing_slash() {
        let result = normalize(&base(), "https://example.com").expect("should normalize");
        assert_eq!(result.path(), "/");
    }

    #[test]
    fn non_root_path_without_slash_preserved() {
        let result = normalize(&base(), "/about").expect("should normalize");
        assert_eq!(result.path(), "/about");
    }

    #[test]
    fn non_root_path_with_trailing_slash_preserved() {
        let result = normalize(&base(), "/about/").expect("should normalize");
        assert_eq!(result.path(), "/about/");
    }

    // ── Protocol-relative URLs ────────────────────────────────────────────

    #[test]
    fn protocol_relative_adopts_base_scheme() {
        let result = normalize(&base(), "//other.example.com/bar").expect("should normalize");
        assert_eq!(result.scheme(), "https");
        assert_eq!(result.as_str(), "https://other.example.com/bar");
    }

    #[test]
    fn protocol_relative_http_base() {
        let http_base = Url::parse("http://example.com/").expect("invariant: valid base URL");
        let result = normalize(&http_base, "//cdn.example.com/img.png").expect("should normalize");
        assert_eq!(result.scheme(), "http");
    }

    // ── Idempotency ───────────────────────────────────────────────────────

    #[test]
    fn idempotent_simple() {
        let once = normalize(&base(), "/page?z=1&a=2#frag").expect("first normalize");
        let twice = normalize(&base(), once.as_str()).expect("second normalize");
        assert_eq!(once, twice);
    }

    #[test]
    fn idempotent_with_tracking_params() {
        let once = normalize(&base(), "/p?utm_source=g&id=5&fbclid=x").expect("first normalize");
        let twice = normalize(&base(), once.as_str()).expect("second normalize");
        assert_eq!(once, twice);
    }

    #[test]
    fn absolute_url_resolved_correctly() {
        let result =
            normalize(&base(), "https://other.com/path?b=2&a=1").expect("should normalize");
        assert_eq!(result.as_str(), "https://other.com/path?a=1&b=2");
    }

    // ── is_same_site ─────────────────────────────────────────────────────

    fn url(s: &str) -> Url {
        Url::parse(s).expect("invariant: test URL must be valid")
    }

    #[test]
    fn is_same_site_identical_host_true() {
        assert!(is_same_site(
            &url("https://example.com/page"),
            &url("https://example.com/other")
        ));
    }

    #[test]
    fn is_same_site_www_vs_apex_false() {
        assert!(!is_same_site(
            &url("https://www.foo.com/"),
            &url("https://foo.com/")
        ));
    }

    #[test]
    fn is_same_site_different_hosts_false() {
        assert!(!is_same_site(
            &url("https://example.com/"),
            &url("https://other.com/")
        ));
    }

    #[test]
    fn is_same_site_http_vs_https_same_host_true() {
        assert!(is_same_site(
            &url("http://example.com/page"),
            &url("https://example.com/page")
        ));
    }

    #[test]
    fn is_same_site_no_host_false() {
        let file_url =
            Url::parse("file:///tmp/index.html").expect("invariant: file URL must parse");
        assert!(!is_same_site(&file_url, &url("https://example.com/")));
        assert!(!is_same_site(&url("https://example.com/"), &file_url));
    }

    // ── discover_links ────────────────────────────────────────────────────

    fn dom_for(html: &str) -> Dom {
        Dom::parse(html)
    }

    fn page_with_links(links_html: &str) -> String {
        format!(
            r#"<!DOCTYPE html><html><head><title>T</title></head><body>{links_html}</body></html>"#
        )
    }

    #[test]
    fn discover_keeps_same_host_drops_external_and_invalid_schemes() {
        // #anchor resolves to the base URL (fragment stripped) → same-host, kept.
        // External host, mailto:, javascript: → all dropped.
        let html = page_with_links(concat!(
            r#"<a href="/about">About</a>"#,
            r#"<a href="https://external.com/page">Ext</a>"#,
            r#"<a href="mailto:user@example.com">Mail</a>"#,
            r#"<a href="javascript:void(0)">JS</a>"#,
            "<a href=\"#anchor\">Anchor</a>",
        ));
        let b = base();
        let dom = dom_for(&html);
        let links = discover_links(&b, &dom);

        assert!(
            links.iter().any(|u| u.path() == "/about"),
            "expected /about in {links:?}"
        );
        assert!(
            links.iter().all(|u| u.host_str() == Some("example.com")),
            "external host leaked into {links:?}"
        );
        // /about + base URL (from #anchor resolution) = 2 same-host links
        assert_eq!(links.len(), 2, "unexpected link count: {links:?}");
    }

    #[test]
    fn discover_deduplicates_links() {
        let html = page_with_links(concat!(
            r#"<a href="/a">First</a>"#,
            r#"<a href="/a">Duplicate</a>"#,
            r#"<a href="/a?utm_source=x">UTM</a>"#,
        ));
        // normalize() strips utm_ params, so /a?utm_source=x → /a.
        // All three hrefs collapse to the same canonical URL → single result.
        let b = base();
        let dom = dom_for(&html);
        let links = discover_links(&b, &dom);

        assert_eq!(
            links.len(),
            1,
            "expected 1 deduplicated link, got {links:?}"
        );
        assert_eq!(links[0].path(), "/a");
    }

    #[test]
    fn discover_resolves_relative_href() {
        let html = page_with_links(r#"<a href="about">About relative</a>"#);
        let b = Url::parse("https://example.com/dir/").expect("invariant: valid base URL");
        let dom = dom_for(&html);
        let links = discover_links(&b, &dom);

        assert_eq!(links.len(), 1, "expected 1 link, got {links:?}");
        assert_eq!(
            links[0].as_str(),
            "https://example.com/dir/about",
            "relative href not resolved correctly"
        );
    }

    #[test]
    fn discover_keeps_self_link() {
        let html = page_with_links(r#"<a href="https://example.com/">Home</a>"#);
        let b = base();
        let dom = dom_for(&html);
        let links = discover_links(&b, &dom);

        assert_eq!(links.len(), 1, "self-link should be kept: {links:?}");
        assert_eq!(links[0].host_str(), Some("example.com"));
    }
}
