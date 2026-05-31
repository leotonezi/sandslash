use url::Url;

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

#[cfg(test)]
mod tests {
    use super::*;

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
        // ftp: is not http/https — normalize must return None
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
        // Params should be kept and sorted (page < q alphabetically)
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
}
