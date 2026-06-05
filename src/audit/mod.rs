pub mod headings;
pub mod https;
pub mod js_rendered;
pub mod links;
pub mod metadata;

// Phase 2+
pub mod images;
pub mod opengraph;
pub mod redirects;
pub mod robots;
pub mod sitemap;

use std::sync::Arc;

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
    async fn audit(&self, page: &PageData, ctx: &AuditContext) -> Vec<Finding>;
}

/// Context passed to every [`SiteAuditor`].
///
/// Uses `Arc` so auditors can clone `fetcher` into async tasks without lifetime constraints.
pub struct AuditContext {
    pub config: Arc<CrawlConfig>,
    pub fetcher: Arc<Fetcher>,
}

pub fn page_auditors() -> Vec<Box<dyn PageAuditor>> {
    vec![
        Box::new(metadata::MetadataAuditor),
        Box::new(headings::HeadingsAuditor),
        Box::new(https::HttpsAuditor),
        Box::new(opengraph::OpengraphAuditor),
        Box::new(images::ImagesAuditor),
        Box::new(redirects::RedirectsAuditor),
        Box::new(js_rendered::JsRenderedAuditor),
    ]
}

pub fn site_auditors() -> Vec<Box<dyn SiteAuditor>> {
    vec![
        Box::new(robots::RobotsAuditor),
        Box::new(sitemap::SitemapAuditor),
        Box::new(links::BrokenLinksAuditor),
    ]
}

pub(crate) fn finding(
    check_id: &'static str,
    category: Category,
    severity: Severity,
    penalty: u8,
    message: impl Into<String>,
) -> Finding {
    Finding {
        check_id,
        category,
        severity,
        message: message.into(),
        penalty,
    }
}
