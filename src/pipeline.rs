use chrono::Utc;

use crate::audit::AuditContext;
use crate::config::CrawlConfig;
use crate::fetcher::Fetcher;
use crate::model::AuditReport;
use crate::parser::Dom;
use crate::report::json::write_json;
use crate::score::{score_page, score_site};

pub async fn run(config: CrawlConfig) -> anyhow::Result<AuditReport> {
    let fetcher = Fetcher::new(&config)?;
    let page_data = fetcher.fetch(&config.root).await?;

    let findings = {
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

    let page_report = score_page(page_data.url.clone(), findings);

    let ctx = AuditContext {
        config: &config,
        fetcher: &fetcher,
    };
    let mut all_findings = page_report.findings.clone();
    for auditor in crate::audit::site_auditors() {
        let mut f = auditor.audit(&page_data, &ctx).await;
        all_findings.append(&mut f);
    }

    let page_report = score_page(page_data.url, all_findings);
    let site_score = score_site(&[page_report.clone()]);

    let report = AuditReport {
        root: config.root.clone(),
        pages: vec![page_report],
        site_score,
        crawled_at: Utc::now().to_rfc3339(),
    };

    match &config.output_json {
        Some(path) => {
            let file = std::fs::File::create(path)?;
            write_json(&report, file)?;
        }
        None => {
            write_json(&report, std::io::stdout())?;
        }
    }

    Ok(report)
}
