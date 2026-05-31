// Link discovery and URL normalization.
use std::collections::HashSet;

use url::Url;
use url::form_urlencoded;

use crate::parser::dom::Dom;

/// Resolve and normalize `href` against `base`.
///
/// Returns `None` for non-navigable schemes (`mailto:`, `javascript:`, `data:`, etc.),
/// for fragment-only references, and for any href that cannot be parsed as a URL.
///
/// Normalization steps applied:
/// 1. Resolve relative hrefs against `base`.
/// 2. Strip the fragment component.
/// 3. Strip tracking params: keys starting with `utm_`; exact keys `fbclid`, `gclid`, `mc_eid`, `ref`, `igshid`.
/// 4. Sort remaining query params alphabetically.
/// 5. Strip a trailing `/` from the path when the path is longer than one character.
/// 6. Lowercase the scheme and host (handled by the `url` crate).
pub fn normalize(base: &Url, href: &str) -> Option<Url> {
    let href = href.trim();

    // Reject fragment-only references before any further work.
    if href.starts_with('#') {
        return None;
    }

    // Parse relative or absolute href.
    let mut url = base.join(href).ok()?;

    // Only keep navigable schemes.
    match url.scheme() {
        "http" | "https" => {}
        _ => return None,
    }

    // Strip the fragment.
    url.set_fragment(None);

    // Strip tracking query params and sort remaining ones alphabetically.
    const STRIP_EXACT: &[&str] = &["fbclid", "gclid", "mc_eid", "ref", "igshid"];
    {
        let filtered: Vec<(String, String)> = url
            .query_pairs()
            .filter(|(k, _)| !k.starts_with("utm_") && !STRIP_EXACT.contains(&k.as_ref()))
            .map(|(k, v)| (k.into_owned(), v.into_owned()))
            .collect();

        if filtered.is_empty() {
            url.set_query(None);
        } else {
            let mut sorted = filtered;
            sorted.sort_by(|a, b| a.0.cmp(&b.0));
            let query = form_urlencoded::Serializer::new(String::new())
                .extend_pairs(&sorted)
                .finish();
            url.set_query(Some(&query));
        }
    }

    // Lowercase scheme and host are handled by the `url` crate internally.
    // Strip trailing slash from a non-root path.
    {
        let path = url.path().to_owned();
        if path.len() > 1 && path.ends_with('/') {
            url.set_path(path.trim_end_matches('/'));
        }
    }

    Some(url)
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

    fn base(s: &str) -> Url {
        Url::parse(s).expect("invariant: test base URL must be valid")
    }

    // -------------------------------------------------------------------------
    // is_same_site
    // -------------------------------------------------------------------------

    #[test]
    fn is_same_site_identical_host_true() {
        let a = base("https://example.com/page");
        let b = base("https://example.com/other");
        assert!(is_same_site(&a, &b));
    }

    #[test]
    fn is_same_site_www_vs_apex_false() {
        let a = base("https://www.foo.com/");
        let b = base("https://foo.com/");
        assert!(!is_same_site(&a, &b));
    }

    #[test]
    fn is_same_site_different_hosts_false() {
        let a = base("https://example.com/");
        let b = base("https://other.com/");
        assert!(!is_same_site(&a, &b));
    }

    #[test]
    fn is_same_site_http_vs_https_same_host_true() {
        let a = base("http://example.com/page");
        let b = base("https://example.com/page");
        assert!(is_same_site(&a, &b));
    }

    #[test]
    fn is_same_site_no_host_false() {
        // `file:` URLs have no host component.
        let a = Url::parse("file:///tmp/index.html").expect("invariant: file URL must parse");
        let b = base("https://example.com/");
        assert!(!is_same_site(&a, &b));
        assert!(!is_same_site(&b, &a));
    }

    // -------------------------------------------------------------------------
    // discover_links
    // -------------------------------------------------------------------------

    fn dom_for(html: &str) -> Dom {
        Dom::parse(html)
    }

    /// Builds minimal HTML wrapping the provided `<a>` tags.
    fn page_with_links(links_html: &str) -> String {
        format!(
            r#"<!DOCTYPE html><html><head><title>T</title></head><body>{links_html}</body></html>"#
        )
    }

    #[test]
    fn discover_keeps_same_host_drops_external_and_invalid_schemes() {
        let html = page_with_links(concat!(
            r#"<a href="/about">About</a>"#,
            r#"<a href="https://external.com/page">Ext</a>"#,
            r#"<a href="mailto:user@example.com">Mail</a>"#,
            r#"<a href="javascript:void(0)">JS</a>"#,
            "<a href=\"#anchor\">Anchor</a>",
        ));
        let b = base("https://example.com/");
        let dom = dom_for(&html);
        let links = discover_links(&b, &dom);

        // /about on the same host must be present.
        assert!(
            links.iter().any(|u| u.path() == "/about"),
            "expected /about in {links:?}"
        );
        // External host must be absent.
        assert!(
            links.iter().all(|u| u.host_str() == Some("example.com")),
            "external host leaked into {links:?}"
        );
        // Only the one same-host link should be present.
        assert_eq!(links.len(), 1, "unexpected link count: {links:?}");
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
        let b = base("https://example.com/");
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
        let b = base("https://example.com/dir/");
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
        let b = base("https://example.com/");
        let dom = dom_for(&html);
        let links = discover_links(&b, &dom);

        assert_eq!(links.len(), 1, "self-link should be kept: {links:?}");
        assert_eq!(links[0].host_str(), Some("example.com"));
    }
}
