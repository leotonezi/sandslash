pub mod headings;
pub mod https;
pub mod metadata;

// Phase 2+
pub mod images;
pub mod opengraph;
pub mod redirects;
pub mod robots;
pub mod sitemap;

use async_trait::async_trait;

use crate::config::CrawlConfig;
use crate::fetcher::Fetcher;
use crate::model::{Category, Finding, PageData, Severity};
use crate::parser::Dom;

pub trait PageAuditor: Send + Sync {
    fn id(&self) -> &'static str;
    fn category(&self) -> Category;
    fn audit(&self, page: &PageData, dom: &Dom) -> Vec<Finding>;
}

#[async_trait]
pub trait SiteAuditor: Send + Sync {
    fn id(&self) -> &'static str;
    fn category(&self) -> Category;
    async fn audit(&self, page: &PageData, ctx: &AuditContext<'_>) -> Vec<Finding>;
}

pub struct AuditContext<'a> {
    pub config: &'a CrawlConfig,
    pub fetcher: &'a Fetcher,
}

pub fn page_auditors() -> Vec<Box<dyn PageAuditor>> {
    vec![
        Box::new(metadata::MetadataAuditor),
        Box::new(headings::HeadingsAuditor),
        Box::new(https::HttpsAuditor),
    ]
}

pub fn site_auditors() -> Vec<Box<dyn SiteAuditor>> {
    vec![]  // Phase 2: robots, sitemap, links
}

pub(crate) fn finding(
    check_id: &'static str,
    category: Category,
    severity: Severity,
    penalty: u8,
    message: impl Into<String>,
) -> Finding {
    Finding { check_id, category, severity, message: message.into(), penalty }
}
