use chrono::Utc;
use url::Url;

use crate::audit::AuditContext;
use crate::config::CrawlConfig;
use crate::error::SeoError;
use crate::fetcher::Fetcher;
use crate::model::{AuditReport, Category, Finding, Headers, PageData, Severity};
use crate::parser::Dom;
use crate::report::json::write_json;
use crate::score::{score_page, score_site};

pub async fn run(config: CrawlConfig) -> anyhow::Result<AuditReport> {
    let fetcher = Fetcher::new(&config)?;

    // Attempt to fetch the root page; catch redirect loops before anything else.
    let page_data = match fetcher.fetch(&config.root).await {
        Ok(pd) => pd,
        Err(SeoError::RedirectLoop { url, hops }) => {
            return handle_redirect_loop(url, hops, &config);
        }
        Err(e) => return Err(e.into()),
    };

    let mut findings: Vec<Finding> = {
        let html = page_data.html.clone();
        let page_snap = page_data.clone();
        tokio::task::spawn_blocking(move || {
            let dom = Dom::parse(&html);
            crate::audit::page_auditors()
                .iter()
                .flat_map(|a| a.audit(&page_snap, &dom))
                .collect::<Vec<_>>()
        })
        .await?
    };

    let ctx = AuditContext {
        config: &config,
        fetcher: &fetcher,
    };
    for auditor in crate::audit::site_auditors() {
        let mut f = auditor.audit(&page_data, &ctx).await;
        findings.append(&mut f);
    }

    let page_report = score_page(page_data.url, findings);
    let site_score = score_site(std::slice::from_ref(&page_report));

    let report = AuditReport {
        root: config.root.clone(),
        pages: vec![page_report],
        site_score,
        crawled_at: Utc::now().to_rfc3339(),
    };

    emit_report(&report, &config)?;
    Ok(report)
}

/// Build a synthetic report for a redirect-loop URL: status=0, single `redirects.loop` Critical
/// finding, page auditors are skipped entirely.
fn handle_redirect_loop(
    url: String,
    hops: usize,
    config: &CrawlConfig,
) -> anyhow::Result<AuditReport> {
    let parsed_url: Url = Url::parse(&url).map_err(SeoError::from)?;

    // Populate redirect_chain with `hops` copies of the loop URL so that length == hops.
    let redirect_chain = vec![parsed_url.clone(); hops];

    let synthetic = PageData {
        url: parsed_url.clone(),
        status: 0,
        redirect_chain,
        html: String::new(),
        headers: Headers::default(),
        depth: 0,
    };

    let loop_finding = Finding {
        check_id: "redirects.loop",
        category: Category::Links,
        severity: Severity::Critical,
        message: format!("Redirect loop detected at {url} after {hops} hops"),
        penalty: 40,
    };

    // Skip page_auditors() — emit only the loop finding.
    let page_report = score_page(synthetic.url, vec![loop_finding]);
    let site_score = score_site(std::slice::from_ref(&page_report));

    let report = AuditReport {
        root: config.root.clone(),
        pages: vec![page_report],
        site_score,
        crawled_at: Utc::now().to_rfc3339(),
    };

    emit_report(&report, config)?;
    Ok(report)
}

fn emit_report(report: &AuditReport, config: &CrawlConfig) -> anyhow::Result<()> {
    match &config.output_json {
        Some(path) => {
            let file = std::fs::File::create(path)?;
            write_json(report, file)?;
        }
        None => {
            write_json(report, std::io::stdout())?;
        }
    }
    Ok(())
}
