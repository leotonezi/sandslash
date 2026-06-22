use std::collections::HashMap;

use serde::{Deserialize, Serialize};
use url::Url;

use crate::model::Category;

/// The result of comparing two [`crate::model::AuditReport`] files.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiffReport {
    /// Site score from the "before" report.
    pub before_site_score: u8,
    /// Site score from the "after" report.
    pub after_site_score: u8,
    /// Signed delta: `after_site_score as i16 - before_site_score as i16`.
    pub site_score_delta: i16,
    /// Per-category mean score deltas (after − before).
    pub category_deltas: HashMap<Category, i16>,
    /// Per-page diff entries (all pages from both reports).
    pub pages: Vec<PageDiff>,
    /// Root URL of the "before" report.
    pub before_root: Url,
    /// Root URL of the "after" report.
    pub after_root: Url,
}

/// How a single page relates between the two reports.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum PageDiffKind {
    /// Present in "after" but not in "before".
    Added,
    /// Present in "before" but not in "after".
    Removed,
    /// Present in both; score changed.
    Changed,
    /// Present in both; score identical.
    Unchanged,
}

/// Score diff for one URL.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PageDiff {
    /// The page URL.
    pub url: Url,
    /// Classification of this page's diff.
    pub kind: PageDiffKind,
    /// Score in the "before" report (`None` for Added pages).
    pub before_score: Option<u8>,
    /// Score in the "after" report (`None` for Removed pages).
    pub after_score: Option<u8>,
    /// Signed delta (`after_score - before_score`; 0 for Added/Removed pages
    /// where the other side is absent).
    pub delta: i16,
}
