use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use url::Url;

pub type Headers = HashMap<String, String>;

#[derive(Debug, Clone, Serialize)]
pub struct PageData {
    pub url: Url,
    pub status: u16,
    pub redirect_chain: Vec<Url>,
    pub html: String,
    pub headers: Headers,
    pub depth: u32,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub enum Severity {
    Critical,
    Warning,
    Info,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub enum Category {
    Metadata,
    SocialTags,
    Structure,
    Links,
    Media,
    Crawlability,
    Security,
}

const ALL_CATEGORIES: [Category; 7] = [
    Category::Metadata,
    Category::SocialTags,
    Category::Structure,
    Category::Links,
    Category::Media,
    Category::Crawlability,
    Category::Security,
];

impl Category {
    pub fn all() -> impl Iterator<Item = Self> {
        ALL_CATEGORIES.iter().copied()
    }

    pub fn weight(self) -> u8 {
        match self {
            Category::Metadata => 20,
            Category::Crawlability => 20,
            Category::Structure => 15,
            Category::Links => 15,
            Category::Security => 15,
            Category::Media => 10,
            Category::SocialTags => 5,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Finding {
    pub check_id: String,
    pub category: Category,
    pub severity: Severity,
    pub message: String,
    pub penalty: u8,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PageReport {
    pub url: Url,
    pub findings: Vec<Finding>,
    pub category_scores: HashMap<Category, u8>,
    pub score: u8,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuditReport {
    pub root: Url,
    pub pages: Vec<PageReport>,
    pub site_score: u8,
    pub crawled_at: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn category_weights_sum_to_100() {
        let total: u32 = Category::all().map(|c| c.weight() as u32).sum();
        assert_eq!(total, 100);
    }

    #[test]
    fn category_all_has_seven_variants() {
        assert_eq!(Category::all().count(), 7);
    }
}
