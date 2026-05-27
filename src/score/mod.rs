use std::collections::HashMap;

use crate::model::{Category, Finding, PageReport};
use url::Url;

pub fn score_page(url: Url, findings: Vec<Finding>) -> PageReport {
    let mut cat: HashMap<Category, i32> =
        Category::all().map(|c| (c, 100)).collect();

    for f in &findings {
        *cat.get_mut(&f.category).expect("all categories initialized") -= f.penalty as i32;
    }

    for v in cat.values_mut() {
        *v = (*v).clamp(0, 100);
    }

    let weighted: f64 = cat
        .iter()
        .map(|(c, &s)| s as f64 * c.weight() as f64 / 100.0)
        .sum();

    let score = (weighted.round() as i32).clamp(0, 100) as u8;
    let category_scores = cat.iter().map(|(&c, &s)| (c, s as u8)).collect();

    PageReport { url, findings, category_scores, score }
}

pub fn score_site(reports: &[PageReport]) -> u8 {
    if reports.is_empty() {
        return 0;
    }
    let sum: u32 = reports.iter().map(|p| p.score as u32).sum();
    (sum / reports.len() as u32) as u8
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{Category, Severity};

    fn url() -> Url { "https://example.com/".parse().unwrap() }

    #[test]
    fn no_findings_scores_100() {
        let report = score_page(url(), vec![]);
        assert_eq!(report.score, 100);
        assert!(report.category_scores.values().all(|&s| s == 100));
    }

    #[test]
    fn critical_metadata_penalty_reduces_score() {
        let findings = vec![Finding {
            check_id: "title.missing",
            category: Category::Metadata,
            severity: Severity::Critical,
            message: "no title".into(),
            penalty: 30,
        }];
        let report = score_page(url(), findings);
        assert!(report.score < 100);
        assert_eq!(*report.category_scores.get(&Category::Metadata).unwrap(), 70);
        // weighted: Metadata contributes 20% * 70 = 14, rest 80% * 100 = 80 → total 94
        assert_eq!(report.score, 94);
    }

    #[test]
    fn score_site_returns_mean() {
        let r1 = score_page(url(), vec![]);
        let r2 = score_page(url(), vec![Finding {
            check_id: "title.missing",
            category: Category::Metadata,
            severity: Severity::Critical,
            message: "no title".into(),
            penalty: 100,
        }]);
        let site = score_site(&[r1, r2]);
        assert_eq!(site, 90); // (100 + 80) / 2
    }

    #[test]
    fn score_site_empty_returns_0() {
        assert_eq!(score_site(&[]), 0);
    }

    #[test]
    fn penalty_clamped_at_zero() {
        let findings = vec![Finding {
            check_id: "title.missing",
            category: Category::Metadata,
            severity: Severity::Critical,
            message: "no title".into(),
            penalty: 255,
        }];
        let report = score_page(url(), findings);
        assert_eq!(*report.category_scores.get(&Category::Metadata).unwrap(), 0);
    }
}
