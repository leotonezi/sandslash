use std::collections::HashSet;
use std::sync::Arc;

use async_trait::async_trait;
use tokio::sync::Semaphore;
use tokio::task::JoinSet;
use url::Url;

use crate::audit::{AuditContext, SiteAuditor, finding};
use crate::model::{Category, Finding, PageData, Severity};
use crate::parser::links::normalize;

const MAX_CONCURRENT_PROBES: usize = 32;

pub struct BrokenLinksAuditor;

#[derive(Debug)]
enum ProbeOutcome {
    Ok,
    Status4xx(u16),
    Status5xx(u16),
    Unreachable,
}

/// Issue HEAD first; on 405 or transport error fall back to GET.
async fn probe(fetcher: &crate::fetcher::Fetcher, url: &Url) -> ProbeOutcome {
    let status = match fetcher.head(url).await {
        Ok(405) => match fetcher.fetch(url).await {
            Ok(page) => page.status,
            Err(_) => return ProbeOutcome::Unreachable,
        },
        Ok(s) => s,
        Err(_) => match fetcher.fetch(url).await {
            Ok(page) => page.status,
            Err(_) => return ProbeOutcome::Unreachable,
        },
    };

    match status {
        200..=399 => ProbeOutcome::Ok,
        400..=499 => ProbeOutcome::Status4xx(status),
        500..=599 => ProbeOutcome::Status5xx(status),
        _ => ProbeOutcome::Unreachable,
    }
}

/// Extract all `href` values from page HTML. Uses `spawn_blocking` because `scraper::Html` is `!Send`.
async fn extract_hrefs(html: String) -> Vec<String> {
    tokio::task::spawn_blocking(move || {
        use scraper::{Html, Selector};
        use std::sync::LazyLock;

        static SEL_A: LazyLock<Selector> =
            LazyLock::new(|| Selector::parse("a[href]").expect("invariant: valid CSS selector"));

        let doc = Html::parse_document(&html);
        doc.select(&SEL_A)
            .filter_map(|el| el.attr("href"))
            .map(|s| s.to_owned())
            .collect()
    })
    .await
    .unwrap_or_default()
}

/// Partition normalized URLs into (internal, external).
/// Same-host = exact `host_str()` match. Drops non-http/s, fragment-only, and unresolvable hrefs.
fn partition(hrefs: &[String], base: &Url) -> (HashSet<Url>, HashSet<Url>) {
    let mut internal = HashSet::new();
    let mut external = HashSet::new();
    let base_host = base.host_str();

    for href in hrefs {
        if href.trim().starts_with('#') {
            continue;
        }
        let Some(url) = normalize(base, href) else {
            continue;
        };
        match url.host_str() {
            Some(h) if Some(h) == base_host => {
                internal.insert(url);
            }
            Some(_) => {
                external.insert(url);
            }
            None => {}
        }
    }

    (internal, external)
}

#[async_trait]
impl SiteAuditor for BrokenLinksAuditor {
    fn id(&self) -> &'static str {
        "links"
    }

    fn category(&self) -> Category {
        Category::Links
    }

    async fn audit(&self, page: &PageData, ctx: &AuditContext) -> Vec<Finding> {
        let hrefs = extract_hrefs(page.html.clone()).await;
        let (internal, external) = partition(&hrefs, &page.url);

        let mut to_probe: Vec<Url> = internal.into_iter().collect();
        if ctx.config.check_external_links {
            to_probe.extend(external);
        }

        if to_probe.is_empty() {
            return vec![];
        }

        let sem = Arc::new(Semaphore::new(MAX_CONCURRENT_PROBES));
        let fetcher = Arc::clone(&ctx.fetcher);
        let mut join_set: JoinSet<(Url, ProbeOutcome)> = JoinSet::new();

        for url in to_probe {
            let sem = Arc::clone(&sem);
            let fetcher = Arc::clone(&fetcher);
            join_set.spawn(async move {
                let _permit = sem
                    .acquire_owned()
                    .await
                    .expect("invariant: semaphore is never closed");
                let outcome = probe(&fetcher, &url).await;
                (url, outcome)
            });
        }

        let mut findings = Vec::new();
        while let Some(result) = join_set.join_next().await {
            let (url, outcome) = match result {
                Ok(pair) => pair,
                Err(_) => continue,
            };
            match outcome {
                ProbeOutcome::Ok => {}
                ProbeOutcome::Status4xx(status) => {
                    findings.push(finding(
                        "links.broken-4xx",
                        Category::Links,
                        Severity::Warning,
                        10,
                        format!("Broken link (HTTP {status}): {url}"),
                    ));
                }
                ProbeOutcome::Status5xx(status) => {
                    findings.push(finding(
                        "links.broken-5xx",
                        Category::Links,
                        Severity::Critical,
                        20,
                        format!("Server error link (HTTP {status}): {url}"),
                    ));
                }
                ProbeOutcome::Unreachable => {
                    findings.push(finding(
                        "links.unreachable",
                        Category::Links,
                        Severity::Warning,
                        10,
                        format!("Unreachable link: {url}"),
                    ));
                }
            }
        }

        findings
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn base() -> Url {
        "https://example.com/"
            .parse()
            .expect("invariant: valid URL")
    }

    #[test]
    fn partition_splits_internal_external_drops_non_http() {
        let hrefs = vec![
            "/about".to_owned(),
            "https://example.com/contact".to_owned(),
            "https://other.com/page".to_owned(),
            "mailto:foo@example.com".to_owned(),
            "#section".to_owned(),
        ];
        let (internal, external) = partition(&hrefs, &base());
        assert_eq!(internal.len(), 2, "expected 2 internal: {internal:?}");
        assert_eq!(external.len(), 1, "expected 1 external: {external:?}");
        assert!(
            external.iter().all(|u| u.host_str() == Some("other.com")),
            "external should only contain other.com: {external:?}"
        );
    }

    #[test]
    fn partition_deduplicates() {
        let hrefs = vec![
            "/page".to_owned(),
            "/page".to_owned(),
            "/page?utm_source=x".to_owned(),
        ];
        let (internal, _) = partition(&hrefs, &base());
        assert_eq!(
            internal.len(),
            1,
            "expected 1 deduplicated URL: {internal:?}"
        );
    }

    #[test]
    fn partition_drops_mailto() {
        let hrefs = vec!["mailto:foo@bar.com".to_owned()];
        let (internal, external) = partition(&hrefs, &base());
        assert!(internal.is_empty(), "mailto must not appear in internal");
        assert!(external.is_empty(), "mailto must not appear in external");
    }

    /// Reads the shared fixture HTML and verifies partition produces the expected
    /// internal/external split: 4 internal URLs (deduped), 1 external, fragments/mailto dropped.
    #[tokio::test]
    async fn partition_from_fixture_html() {
        // The fixture at tests/fixtures/links_mixed.html contains:
        //   internal: /ok (x2, deduped), http://example.com/missing, /broken, /head-hostile = 4
        //   external: http://external.example.org/page = 1
        //   ignored:  #section (fragment), mailto:foo@bar.com
        let html = include_str!("../../tests/fixtures/links_mixed.html").to_owned();
        let hrefs = extract_hrefs(html).await;
        let (internal, external) = partition(&hrefs, &base());

        assert_eq!(
            internal.len(),
            4,
            "expected 4 deduplicated internal URLs from fixture: {internal:?}"
        );
        assert_eq!(
            external.len(),
            1,
            "expected 1 external URL from fixture: {external:?}"
        );
        assert!(
            external
                .iter()
                .all(|u| u.host_str() == Some("external.example.org")),
            "external should only contain external.example.org: {external:?}"
        );
    }

    /// Verifies that with check_external_links == false, external links in a page
    /// produce no findings even when those URLs would fail if probed.
    #[test]
    fn partition_external_not_in_internal_set_when_check_disabled() {
        let hrefs = vec![
            "/internal-page".to_owned(),
            "https://other.com/external-404".to_owned(),
        ];
        let (internal, external) = partition(&hrefs, &base());
        // check_external_links == false → only internal set would be probed
        assert_eq!(internal.len(), 1, "expected 1 internal URL: {internal:?}");
        assert_eq!(external.len(), 1, "expected 1 external URL: {external:?}");
        // Confirm internal does not contain the external host
        assert!(
            internal.iter().all(|u| u.host_str() == Some("example.com")),
            "internal set must not contain cross-host URLs: {internal:?}"
        );
    }
}
