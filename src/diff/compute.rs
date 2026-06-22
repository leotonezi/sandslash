use std::collections::{HashMap, HashSet};

use url::Url;

use crate::model::{AuditReport, Category};

use super::model::{DiffReport, PageDiff, PageDiffKind};

/// Compute a [`DiffReport`] from two [`AuditReport`] values.
///
/// This function is pure and synchronous — no I/O, no async.
pub fn diff_reports(before: &AuditReport, after: &AuditReport) -> DiffReport {
    // Build URL→PageReport maps keyed by exact URL string.
    let before_map: HashMap<&str, &crate::model::PageReport> =
        before.pages.iter().map(|p| (p.url.as_str(), p)).collect();

    let after_map: HashMap<&str, &crate::model::PageReport> =
        after.pages.iter().map(|p| (p.url.as_str(), p)).collect();

    // Union of all URL keys.
    let all_urls: HashSet<&str> = before_map
        .keys()
        .copied()
        .chain(after_map.keys().copied())
        .collect();

    // Build per-page diffs.
    let mut pages: Vec<PageDiff> = all_urls
        .iter()
        .map(|&url_str| {
            let b = before_map.get(url_str).copied();
            let a = after_map.get(url_str).copied();

            let url: Url = url_str
                .parse()
                .expect("invariant: URL came from a valid AuditReport");

            match (b, a) {
                (Some(bp), Some(ap)) => {
                    let before_score = bp.score;
                    let after_score = ap.score;
                    let delta = after_score as i16 - before_score as i16;
                    let kind = if delta == 0 {
                        PageDiffKind::Unchanged
                    } else {
                        PageDiffKind::Changed
                    };
                    PageDiff {
                        url,
                        kind,
                        before_score: Some(before_score),
                        after_score: Some(after_score),
                        delta,
                    }
                }
                (None, Some(ap)) => PageDiff {
                    url,
                    kind: PageDiffKind::Added,
                    before_score: None,
                    after_score: Some(ap.score),
                    delta: 0,
                },
                (Some(bp), None) => PageDiff {
                    url,
                    kind: PageDiffKind::Removed,
                    before_score: Some(bp.score),
                    after_score: None,
                    delta: 0,
                },
                (None, None) => {
                    // Cannot happen since we built all_urls from both maps.
                    unreachable!("URL in all_urls must exist in at least one map")
                }
            }
        })
        .collect();

    // Sort by delta ascending (worst regressions first), then by URL for stability.
    pages.sort_by(|a, b| {
        a.delta
            .cmp(&b.delta)
            .then_with(|| a.url.as_str().cmp(b.url.as_str()))
    });

    // Compute category deltas: mean(after scores) − mean(before scores) for each category.
    let category_deltas: HashMap<Category, i16> = Category::all()
        .filter_map(|cat| {
            let before_scores: Vec<u8> = before
                .pages
                .iter()
                .filter_map(|p| p.category_scores.get(&cat).copied())
                .collect();

            let after_scores: Vec<u8> = after
                .pages
                .iter()
                .filter_map(|p| p.category_scores.get(&cat).copied())
                .collect();

            // Skip categories absent from both sides.
            if before_scores.is_empty() && after_scores.is_empty() {
                return None;
            }

            let before_mean = if before_scores.is_empty() {
                0i16
            } else {
                let sum: u32 = before_scores.iter().map(|&s| s as u32).sum();
                (sum / before_scores.len() as u32) as i16
            };

            let after_mean = if after_scores.is_empty() {
                0i16
            } else {
                let sum: u32 = after_scores.iter().map(|&s| s as u32).sum();
                (sum / after_scores.len() as u32) as i16
            };

            Some((cat, after_mean - before_mean))
        })
        .collect();

    DiffReport {
        before_site_score: before.site_score,
        after_site_score: after.site_score,
        site_score_delta: after.site_score as i16 - before.site_score as i16,
        category_deltas,
        pages,
        before_root: before.root.clone(),
        after_root: after.root.clone(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{AuditReport, Category, PageReport};
    use std::collections::HashMap;

    fn make_page(url: &str, score: u8) -> PageReport {
        let category_scores: HashMap<Category, u8> = Category::all().map(|c| (c, score)).collect();
        PageReport {
            url: url.parse().expect("invariant: valid url"),
            findings: vec![],
            category_scores,
            score,
        }
    }

    fn make_report(root: &str, pages: Vec<PageReport>, site_score: u8) -> AuditReport {
        AuditReport {
            root: root.parse().expect("invariant: valid url"),
            pages,
            site_score,
            crawled_at: "2026-01-01T00:00:00Z".to_owned(),
        }
    }

    /// AC: all-zero deltas when before == after.
    #[test]
    fn identical_reports_produce_zero_deltas() {
        let page = make_page("https://example.com/", 80);
        let report = make_report("https://example.com/", vec![page], 80);

        let diff = diff_reports(&report, &report);

        assert_eq!(diff.site_score_delta, 0);
        assert_eq!(diff.before_site_score, 80);
        assert_eq!(diff.after_site_score, 80);
        for delta in diff.category_deltas.values() {
            assert_eq!(
                *delta, 0,
                "category delta must be zero for identical reports"
            );
        }
        assert_eq!(diff.pages.len(), 1);
        assert_eq!(diff.pages[0].kind, PageDiffKind::Unchanged);
        assert_eq!(diff.pages[0].delta, 0);
    }

    /// AC: correct Added/Removed/Changed/Unchanged classification.
    #[test]
    fn page_kinds_classified_correctly() {
        let before_report = make_report(
            "https://example.com/",
            vec![
                make_page("https://example.com/", 70),
                make_page("https://example.com/about", 60),
                make_page("https://example.com/removed", 50),
            ],
            60,
        );
        let after_report = make_report(
            "https://example.com/",
            vec![
                make_page("https://example.com/", 70),      // Unchanged
                make_page("https://example.com/about", 80), // Changed (+20)
                make_page("https://example.com/new", 90),   // Added
            ],
            80,
        );

        let diff = diff_reports(&before_report, &after_report);

        let find = |url: &str| {
            diff.pages
                .iter()
                .find(|p| p.url.as_str() == url)
                .expect("page must exist in diff")
        };

        let unchanged = find("https://example.com/");
        assert_eq!(unchanged.kind, PageDiffKind::Unchanged);
        assert_eq!(unchanged.delta, 0);
        assert_eq!(unchanged.before_score, Some(70));
        assert_eq!(unchanged.after_score, Some(70));

        let changed = find("https://example.com/about");
        assert_eq!(changed.kind, PageDiffKind::Changed);
        assert_eq!(changed.delta, 20);
        assert_eq!(changed.before_score, Some(60));
        assert_eq!(changed.after_score, Some(80));

        let removed = find("https://example.com/removed");
        assert_eq!(removed.kind, PageDiffKind::Removed);
        assert_eq!(removed.delta, 0);
        assert_eq!(removed.before_score, Some(50));
        assert_eq!(removed.after_score, None);

        let added = find("https://example.com/new");
        assert_eq!(added.kind, PageDiffKind::Added);
        assert_eq!(added.delta, 0);
        assert_eq!(added.before_score, None);
        assert_eq!(added.after_score, Some(90));
    }

    /// AC: site_score_delta = after_site_score as i16 - before_site_score as i16.
    #[test]
    fn site_score_delta_computed_correctly() {
        let before = make_report(
            "https://example.com/",
            vec![make_page("https://example.com/", 60)],
            60,
        );
        let after = make_report(
            "https://example.com/",
            vec![make_page("https://example.com/", 85)],
            85,
        );

        let diff = diff_reports(&before, &after);

        assert_eq!(diff.before_site_score, 60);
        assert_eq!(diff.after_site_score, 85);
        assert_eq!(diff.site_score_delta, 25);
    }

    /// AC: category delta = mean(after page category scores) - mean(before page category scores).
    #[test]
    fn category_deltas_computed_as_means() {
        // Two pages in before: category scores 40 and 60 → mean = 50.
        // Two pages in after:  category scores 70 and 90 → mean = 80.
        // Delta = 80 - 50 = 30.
        let mut before_pages = Vec::new();
        let mut p1 = make_page("https://example.com/a", 40);
        p1.category_scores = Category::all().map(|c| (c, 40u8)).collect();
        let mut p2 = make_page("https://example.com/b", 60);
        p2.category_scores = Category::all().map(|c| (c, 60u8)).collect();
        before_pages.push(p1);
        before_pages.push(p2);

        let mut after_pages = Vec::new();
        let mut p3 = make_page("https://example.com/a", 70);
        p3.category_scores = Category::all().map(|c| (c, 70u8)).collect();
        let mut p4 = make_page("https://example.com/b", 90);
        p4.category_scores = Category::all().map(|c| (c, 90u8)).collect();
        after_pages.push(p3);
        after_pages.push(p4);

        let before = make_report("https://example.com/", before_pages, 50);
        let after = make_report("https://example.com/", after_pages, 80);

        let diff = diff_reports(&before, &after);

        for cat in Category::all() {
            let delta = diff.category_deltas[&cat];
            assert_eq!(delta, 30, "expected delta=30 for {cat:?}, got {delta}");
        }
    }

    /// Pages sorted by delta ascending (worst regression first).
    #[test]
    fn pages_sorted_by_delta_ascending() {
        let before = make_report(
            "https://example.com/",
            vec![
                make_page("https://example.com/a", 90),
                make_page("https://example.com/b", 50),
                make_page("https://example.com/c", 70),
            ],
            70,
        );
        let after = make_report(
            "https://example.com/",
            vec![
                make_page("https://example.com/a", 70), // delta = -20
                make_page("https://example.com/b", 80), // delta = +30
                make_page("https://example.com/c", 70), // delta = 0
            ],
            73,
        );

        let diff = diff_reports(&before, &after);

        let deltas: Vec<i16> = diff.pages.iter().map(|p| p.delta).collect();
        // Must be sorted ascending: -20, 0, +30.
        assert_eq!(deltas, vec![-20, 0, 30]);
    }
}
