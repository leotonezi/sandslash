use std::num::NonZeroU32;
use std::sync::Arc;

use chrono::Utc;
use url::Url;

use crate::audit::AuditContext;
use crate::config::CrawlConfig;
use crate::crawler::RobotsCache;
use crate::error::SeoError;
use crate::fetcher::{Fetcher, HostRateLimiter};
use crate::model::{AuditReport, Category, Finding, Headers, PageData, PageReport, Severity};
use crate::parser::Dom;
use crate::report::ProgressReporter;
use crate::report::json::write_json;
use crate::report::terminal::{TerminalOpts, write_terminal};
use crate::score::{score_page, score_site};

pub async fn run(config: CrawlConfig) -> anyhow::Result<AuditReport> {
    let qps = NonZeroU32::new(config.rate_per_host)
        .unwrap_or_else(|| NonZeroU32::new(1).expect("invariant: 1 != 0"));
    let rate_limiter = Arc::new(HostRateLimiter::new(qps));
    let fetcher = Arc::new(Fetcher::new(&config, Arc::clone(&rate_limiter))?);
    let robots_cache = Arc::new(RobotsCache::new());

    let page_reports: Vec<PageReport> = if config.depth == 0 {
        // ── Single-page path (no Redis dependency) ────────────────────────

        // Robots gating for single-page path.
        if config.respect_robots {
            let allowed = robots_cache
                .allowed(&config.root, &fetcher, &config.user_agent, &rate_limiter)
                .await;
            if !allowed {
                tracing::debug!(url = %config.root, "robots.txt disallowed root URL; returning empty report");
                let report = AuditReport {
                    root: config.root.clone(),
                    pages: vec![],
                    site_score: 100,
                    crawled_at: Utc::now().to_rfc3339(),
                };
                emit_report(&report, &config)?;
                return Ok(report);
            }
        }

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
            config: Arc::new(config.clone()),
            fetcher: Arc::clone(&fetcher),
        };
        for auditor in crate::audit::site_auditors() {
            let mut f = auditor.audit(&page_data, &ctx).await;
            findings.append(&mut f);
        }

        let page_report = score_page(page_data.url, findings);
        vec![page_report]
    } else {
        // ── Multi-page crawler path ───────────────────────────────────────
        let redis_url = config
            .redis_url
            .as_deref()
            .ok_or_else(|| SeoError::Config("--redis-url is required when depth > 0".into()))?;

        let job_id = format!("seo-rs-{}", Utc::now().timestamp_millis());
        let frontier = crate::crawler::Frontier::new(redis_url, job_id).await?;

        let page_auditors = Arc::new(crate::audit::page_auditors());
        let site_auditors = Arc::new(crate::audit::site_auditors());

        let reporter = {
            use std::io::IsTerminal;
            ProgressReporter::new(config.quiet, std::io::stderr().is_terminal())
        };

        if let Some(global_secs) = config.global_timeout_secs {
            // ── Timeout path: spawn workers + wrap collection in timeout ──
            let reporter_finish = reporter.clone();
            let (handles, mut rx, crawl_frontier) = crate::crawler::spawn_crawl_workers(
                Arc::new(config.clone()),
                Arc::clone(&fetcher),
                frontier,
                page_auditors,
                site_auditors,
                Arc::clone(&rate_limiter),
                Arc::clone(&robots_cache),
                reporter,
            )
            .await?;

            let timeout_dur = std::time::Duration::from_secs(global_secs);
            let collect_fut = async {
                let mut reports = Vec::new();
                while let Some(report) = rx.recv().await {
                    reports.push(report);
                }
                reports
            };

            match tokio::time::timeout(timeout_dur, collect_fut).await {
                Ok(reports) => {
                    reporter_finish.finish();
                    // Normal completion — join handles for clean shutdown.
                    for handle in handles {
                        let _ = handle.await;
                    }
                    // Clean up Redis keys.
                    {
                        let mut f = crawl_frontier.lock().await;
                        if let Err(e) = f.clear().await {
                            tracing::warn!(error = %e, "failed to clear frontier after crawl");
                        }
                    }
                    reports
                }
                Err(_elapsed) => {
                    tracing::warn!(
                        global_timeout_secs = global_secs,
                        "global timeout elapsed; returning partial report"
                    );
                    reporter_finish.finish();
                    // Abort all workers so their tx clones are dropped.
                    for handle in handles {
                        handle.abort();
                    }
                    // Drain whatever completed before the timeout.
                    let mut reports = Vec::new();
                    while let Ok(report) = rx.try_recv() {
                        reports.push(report);
                    }
                    // Best-effort cleanup of Redis keys.
                    {
                        let mut f = crawl_frontier.lock().await;
                        if let Err(e) = f.clear().await {
                            tracing::warn!(error = %e, "failed to clear frontier after timeout");
                        }
                    }
                    reports
                }
            }
        } else {
            // ── No-timeout path: existing run_crawl ──────────────────────
            crate::crawler::run_crawl(
                Arc::new(config.clone()),
                Arc::clone(&fetcher),
                frontier,
                page_auditors,
                site_auditors,
                Arc::clone(&rate_limiter),
                Arc::clone(&robots_cache),
                reporter,
            )
            .await?
        }
    };

    let site_score = score_site(&page_reports);

    let report = AuditReport {
        root: config.root.clone(),
        pages: page_reports,
        site_score,
        crawled_at: Utc::now().to_rfc3339(),
    };

    emit_report(&report, &config)?;
    Ok(report)
}

fn handle_redirect_loop(
    url: String,
    hops: usize,
    config: &CrawlConfig,
) -> anyhow::Result<AuditReport> {
    let parsed_url: Url = Url::parse(&url).map_err(SeoError::from)?;

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
    use std::io::IsTerminal;

    if config.quiet && config.output_json.is_none() {
        let terminal_opts = TerminalOpts {
            quiet: true,
            no_color: config.no_color,
            is_tty: std::io::stdout().is_terminal(),
        };
        write_terminal(report, &terminal_opts, &mut std::io::stdout())?;
        return Ok(());
    }

    match &config.output_json {
        Some(path) => {
            let file = std::fs::File::create(path)?;
            write_json(report, file)?;
            let terminal_opts = TerminalOpts {
                quiet: config.quiet,
                no_color: config.no_color,
                is_tty: std::io::stdout().is_terminal(),
            };
            write_terminal(report, &terminal_opts, &mut std::io::stdout())?;
        }
        None => {
            write_json(report, std::io::stdout())?;
            let terminal_opts = TerminalOpts {
                quiet: config.quiet,
                no_color: config.no_color,
                is_tty: std::io::stderr().is_terminal(),
            };
            write_terminal(report, &terminal_opts, &mut std::io::stderr())?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use url::Url;

    fn make_depth_config(depth: u32, redis_url: Option<String>) -> CrawlConfig {
        CrawlConfig {
            root: Url::parse("http://example.com/").expect("invariant: valid URL"),
            depth,
            concurrency: 1,
            rate_per_host: 10,
            redis_url,
            user_agent: "test-agent".to_owned(),
            timeout_secs: 5,
            max_pages: None,
            global_timeout_secs: None,
            respect_robots: false,
            quiet: true,
            no_color: true,
            verbose: false,
            output_json: None,
            check_external_links: false,
        }
    }

    #[tokio::test]
    async fn depth_nonzero_without_redis_url_returns_config_error() {
        let config = make_depth_config(1, None);
        let result = run(config).await;
        let err = result.expect_err("expected an error when redis_url is None with depth > 0");

        let seo_err = err
            .downcast::<SeoError>()
            .expect("error must downcast to SeoError");
        assert!(
            matches!(seo_err, SeoError::Config(_)),
            "expected SeoError::Config, got {seo_err:?}"
        );
    }
}
