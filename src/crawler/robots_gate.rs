//! Per-host robots.txt gating for the crawl engine.
//!
//! `RobotsCache` fetches and parses a robots.txt on the first encounter with each
//! host, then caches the result in an `Arc<OnceCell<...>>` keyed by lowercase
//! host name.  Subsequent lookups for the same host hit the cache with no I/O.
//!
//! # DashMap / async safety
//!
//! DashMap entry guards are **always** dropped before any `.await` point.  The
//! pattern is:
//! ```ignore
//! let cell = { map.entry(key).or_insert_with(|| ...).clone() }; // guard dropped here
//! cell.get_or_init(|| async { ... }).await;
//! ```

use std::sync::Arc;
use std::time::Duration;

use dashmap::DashMap;
use tokio::sync::OnceCell;
use url::Url;

use crate::audit::robots::ParsedRules;
use crate::fetcher::{Fetcher, HostRateLimiter};

// ── Cached entry ─────────────────────────────────────────────────────────────

/// What we store in the cache after the first robots.txt lookup.
enum CachedEntry {
    /// robots.txt was fetched and parsed successfully.
    Rules(ParsedRules),
    /// robots.txt was missing, returned an error status, or was unreachable.
    /// In all these cases we treat every path as allowed (RFC-compliant).
    Missing,
}

// ── RobotsCache ──────────────────────────────────────────────────────────────

/// Thread-safe, per-host robots.txt cache.
///
/// Internally backed by a `DashMap<host, Arc<OnceCell<Arc<CachedEntry>>>>`.
/// The `OnceCell` ensures that exactly one robots.txt fetch is issued per host
/// even under high concurrency.
pub struct RobotsCache {
    cells: DashMap<String, Arc<OnceCell<Arc<CachedEntry>>>>,
}

impl RobotsCache {
    /// Create an empty cache.
    pub fn new() -> Self {
        Self {
            cells: DashMap::new(),
        }
    }

    /// Check whether `url` is allowed to be fetched under `user_agent`.
    ///
    /// Returns `true` (allowed) or `false` (disallowed).
    ///
    /// Side effects on first call for a host:
    /// - Fetches `{scheme}://{host}/robots.txt` via `fetcher`.
    /// - Registers any `Crawl-delay` with `rate_limiter`.
    /// - Logs `info!` for the fetch outcome.
    /// - Logs `debug!` if a URL is disallowed.
    ///
    /// # DashMap safety
    ///
    /// The DashMap guard is dropped **before** any `.await`.
    pub async fn allowed(
        &self,
        url: &Url,
        fetcher: &Fetcher,
        user_agent: &str,
        rate_limiter: &HostRateLimiter,
    ) -> bool {
        // ── 1. Derive the cache key ───────────────────────────────────────────
        let host = match url.host_str() {
            Some(h) => h.to_ascii_lowercase(),
            None => return true, // no host → cannot gate, allow
        };

        // ── 2. Get-or-create the OnceCell for this host ───────────────────────
        // IMPORTANT: the DashMap guard must be dropped before any `.await`.
        let cell: Arc<OnceCell<Arc<CachedEntry>>> = {
            let entry = self
                .cells
                .entry(host.clone())
                .or_insert_with(|| Arc::new(OnceCell::new()));
            Arc::clone(entry.value())
            // DashMap guard dropped here.
        };

        // ── 3. Initialise (fetch + parse) exactly once per host ───────────────
        let entry: Arc<CachedEntry> = cell
            .get_or_init(|| async { fetch_and_parse(&host, url, fetcher, rate_limiter).await })
            .await
            .clone();

        // ── 4. Evaluate allow/disallow ────────────────────────────────────────
        match entry.as_ref() {
            CachedEntry::Missing => true, // treat as allow-all
            CachedEntry::Rules(rules) => {
                let allowed = is_allowed_by_rules(rules, user_agent, url.path());
                if !allowed {
                    tracing::debug!(url = %url, "robots.txt disallows this URL; skipping");
                }
                allowed
            }
        }
    }
}

impl Default for RobotsCache {
    fn default() -> Self {
        Self::new()
    }
}

// ── Internal helpers ─────────────────────────────────────────────────────────

/// Fetch `{scheme}://{host}/robots.txt`, parse it, register `Crawl-delay`,
/// and return the cached entry.
///
/// Any error (network, non-2xx, parse) results in `CachedEntry::Missing`.
async fn fetch_and_parse(
    host: &str,
    original_url: &Url,
    fetcher: &Fetcher,
    rate_limiter: &HostRateLimiter,
) -> Arc<CachedEntry> {
    // Build the robots.txt URL using the scheme from the original URL.
    let scheme = original_url.scheme();
    let robots_url_str = format!("{scheme}://{host}/robots.txt");
    let robots_url = match Url::parse(&robots_url_str) {
        Ok(u) => u,
        Err(e) => {
            tracing::info!(host = %host, error = %e, "robots.txt URL construction failed; treating as allow-all");
            return Arc::new(CachedEntry::Missing);
        }
    };

    // Fetch directly — NOT through the cache.
    let page_data = match fetcher.fetch(&robots_url).await {
        Ok(pd) => pd,
        Err(e) => {
            tracing::info!(host = %host, error = %e, "robots.txt fetch failed; treating as allow-all");
            return Arc::new(CachedEntry::Missing);
        }
    };

    if !(200..300).contains(&page_data.status) {
        tracing::info!(host = %host, status = page_data.status, "robots.txt returned non-2xx; treating as allow-all");
        return Arc::new(CachedEntry::Missing);
    }

    tracing::info!(host = %host, "robots.txt fetched and parsed successfully");

    let rules = crate::audit::robots::parse_rules(&page_data.html);

    // Register Crawl-delay with the rate limiter.
    // We register for the `*` block as a baseline, then for any UA-specific block.
    for (ua_token, delay_secs) in &rules.crawl_delays {
        // Convert seconds (f64) to Duration.
        let nanos = (*delay_secs * 1_000_000_000.0) as u64;
        let min = Duration::from_nanos(nanos);
        rate_limiter.set_min_interval(host, min);
        tracing::debug!(host = %host, ua = %ua_token, delay_secs = %delay_secs, "registered Crawl-delay");
    }

    Arc::new(CachedEntry::Rules(rules))
}

/// Evaluate whether `path` is allowed for `user_agent` given the parsed rules.
///
/// Algorithm (from the spec):
/// 1. Find the applicable UA block: case-insensitive substring match of any
///    User-agent token against `user_agent`.  Fall back to the `*` block.
/// 2. Collect all matching Disallow and Allow prefixes (longest-match wins):
///    - For each Disallow prefix that matches `path` and each Allow prefix that
///      matches `path`, keep only the longest one.
///    - If the longest matching rule is an Allow → allowed.
///    - If the longest matching rule is a Disallow → disallowed.
///    - If no rule matches → allowed.
pub(crate) fn is_allowed_by_rules(rules: &ParsedRules, user_agent: &str, path: &str) -> bool {
    let ua_lower = user_agent.to_ascii_lowercase();

    // Gather the disallow and allow prefixes that apply to this UA.
    // Priority: UA-specific block first, then * block.
    let (disallows, allows) = gather_ua_prefixes(rules, &ua_lower);

    // Longest-match wins.
    let best_disallow = disallows
        .iter()
        .filter(|prefix| path.starts_with(prefix.as_str()))
        .map(|p| p.len())
        .max();

    let best_allow = allows
        .iter()
        .filter(|prefix| path.starts_with(prefix.as_str()))
        .map(|p| p.len())
        .max();

    match (best_disallow, best_allow) {
        (None, _) => true,            // no disallow matches → allowed
        (Some(_), None) => false,     // disallow matches, no allow → disallowed
        (Some(d), Some(a)) => a >= d, // longest match wins; tie goes to Allow
    }
}

/// Gather the disallow and allow prefix lists applicable to `ua_lower`.
///
/// If there is a UA-specific block (case-insensitive substring match), use it;
/// otherwise fall back to the `*` block.
fn gather_ua_prefixes<'r>(
    rules: &'r ParsedRules,
    ua_lower: &str,
) -> (Vec<&'r String>, Vec<&'r String>) {
    // Check if any stored UA token is a substring of the configured UA (case-insensitive).
    // We also accept the reverse: the configured UA being a substring of the token.
    // Robots.txt convention: "Sandslash/0.4" should match "sandslash" token.
    let specific_disallow: Vec<&String> = rules
        .disallow_prefixes
        .iter()
        .filter(|(ua_token, _)| ua_token != "*" && ua_lower.contains(ua_token.as_str()))
        .flat_map(|(_, paths)| paths.iter())
        .collect();

    let specific_allow: Vec<&String> = rules
        .allow_prefixes
        .iter()
        .filter(|(ua_token, _)| ua_token != "*" && ua_lower.contains(ua_token.as_str()))
        .flat_map(|(_, paths)| paths.iter())
        .collect();

    // If there are UA-specific rules, use them exclusively.
    if !specific_disallow.is_empty() || !specific_allow.is_empty() {
        return (specific_disallow, specific_allow);
    }

    // Fall back to the `*` block.
    let star_disallow: Vec<&String> = rules
        .disallow_prefixes
        .iter()
        .filter(|(ua_token, _)| ua_token == "*")
        .flat_map(|(_, paths)| paths.iter())
        .collect();

    let star_allow: Vec<&String> = rules
        .allow_prefixes
        .iter()
        .filter(|(ua_token, _)| ua_token == "*")
        .flat_map(|(_, paths)| paths.iter())
        .collect();

    (star_disallow, star_allow)
}

// ── Unit tests ────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::audit::robots::parse_rules;

    // ── is_allowed_by_rules unit tests ────────────────────────────────────────

    /// `User-agent: *\nDisallow: /private` → path `/private` must be disallowed.
    #[test]
    fn star_disallow_private_blocks_path() {
        let rules = parse_rules("User-agent: *\nDisallow: /private\n");
        assert!(
            !is_allowed_by_rules(&rules, "Sandslash/0.4", "/private"),
            "/private must be disallowed"
        );
    }

    /// `User-agent: *\nDisallow: /private` → path `/public` must be allowed.
    #[test]
    fn star_disallow_private_allows_public() {
        let rules = parse_rules("User-agent: *\nDisallow: /private\n");
        assert!(
            is_allowed_by_rules(&rules, "Sandslash/0.4", "/public"),
            "/public must be allowed"
        );
    }

    /// UA-specific Allow overrides `*` Disallow for that UA.
    ///
    /// ```
    /// User-agent: *
    /// Disallow: /private
    ///
    /// User-agent: Sandslash
    /// Allow: /private
    /// ```
    ///
    /// Sandslash should be allowed to access /private.
    #[test]
    fn ua_specific_allow_overrides_star_disallow() {
        let body = "User-agent: *\nDisallow: /private\n\nUser-agent: Sandslash\nAllow: /private\n";
        let rules = parse_rules(body);
        assert!(
            is_allowed_by_rules(&rules, "Sandslash/0.4", "/private"),
            "UA-specific Allow must override * Disallow"
        );
    }

    /// UA-specific Disallow should apply to the matching UA and not to others.
    #[test]
    fn ua_specific_disallow_applies_to_matching_ua() {
        let body = "User-agent: Sandslash\nDisallow: /admin\n";
        let rules = parse_rules(body);
        assert!(
            !is_allowed_by_rules(&rules, "Sandslash/0.4", "/admin"),
            "Sandslash UA-specific Disallow /admin must be enforced"
        );
        // A different UA (not matching "sandslash") should be allowed.
        assert!(
            is_allowed_by_rules(&rules, "Googlebot/2.1", "/admin"),
            "Googlebot should be allowed (no * block)"
        );
    }

    /// Allow with longer prefix than Disallow → allowed (longest-match wins).
    #[test]
    fn allow_longer_prefix_wins_over_disallow() {
        let body = "User-agent: *\nDisallow: /private\nAllow: /private/public\n";
        let rules = parse_rules(body);
        assert!(
            is_allowed_by_rules(&rules, "Sandslash/0.4", "/private/public/page"),
            "longer Allow prefix must win over shorter Disallow prefix"
        );
        assert!(
            !is_allowed_by_rules(&rules, "Sandslash/0.4", "/private/secret"),
            "/private/secret should still be disallowed"
        );
    }

    /// Empty rules → everything allowed.
    #[test]
    fn empty_rules_allows_all() {
        let rules = parse_rules("");
        assert!(is_allowed_by_rules(&rules, "Sandslash/0.4", "/anything"));
    }

    /// No path match → allowed.
    #[test]
    fn no_matching_disallow_allows_path() {
        let rules = parse_rules("User-agent: *\nDisallow: /private\n");
        assert!(is_allowed_by_rules(&rules, "Sandslash/0.4", "/other"));
    }
}
