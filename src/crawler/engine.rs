//! Worker-pool crawl engine.
//!
//! `run_crawl` seeds a Redis frontier with the configured root URL, then spawns
//! `config.concurrency` async workers.  Each worker:
//!
//! 1. Dequeues a `(depth, url)` pair.
//! 2. Fetches the page.
//! 3. Runs all page-auditors **and** link discovery inside `spawn_blocking`
//!    (because `Dom` is `!Send`).
//! 4. Runs site-auditors (async, outside `spawn_blocking`).
//! 5. Scores the page and sends the `PageReport` over an unbounded channel.
//! 6. Enqueues child URLs if `depth < config.depth` and `max_pages` not reached.
//! 7. Calls `mark_done` on every exit path after a successful dequeue.

use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};

use tokio::sync::Mutex;
use url::Url;

use crate::{
    audit::{AuditContext, PageAuditor, SiteAuditor},
    config::CrawlConfig,
    error::Result,
    fetcher::Fetcher,
    model::{PageData, PageReport},
    parser::{Dom, links::discover_links},
    score::score_page,
};

use super::Frontier;

/// Run a bounded worker-pool crawl starting from `config.root`.
///
/// # Errors
/// Returns `SeoError` if seeding the frontier fails or the channel collapses
/// unexpectedly.  Individual page fetch / audit errors are logged and skipped.
pub async fn run_crawl(
    config: Arc<CrawlConfig>,
    fetcher: Arc<Fetcher>,
    frontier: Frontier,
    page_auditors: Arc<Vec<Box<dyn PageAuditor>>>,
    site_auditors: Arc<Vec<Box<dyn SiteAuditor>>>,
) -> Result<Vec<PageReport>> {
    // ── 1. Seed frontier ─────────────────────────────────────────────────────
    let frontier = Arc::new(Mutex::new(frontier));
    {
        let mut f = frontier.lock().await;
        f.enqueue(config.root.as_str(), 0).await?;
    }

    // ── 2. Shared state ──────────────────────────────────────────────────────
    let pages_fetched: Arc<AtomicUsize> = Arc::new(AtomicUsize::new(0));
    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel::<PageReport>();

    // ── 3. Spawn workers ─────────────────────────────────────────────────────
    for _ in 0..config.concurrency {
        let config = Arc::clone(&config);
        let fetcher = Arc::clone(&fetcher);
        let frontier = Arc::clone(&frontier);
        let page_auditors = Arc::clone(&page_auditors);
        let site_auditors = Arc::clone(&site_auditors);
        let pages_fetched = Arc::clone(&pages_fetched);
        // Each worker holds its own tx clone; they are all dropped when the
        // worker finishes, which closes the channel once the last worker exits.
        let tx = tx.clone();

        tokio::spawn(async move {
            worker_loop(
                config,
                fetcher,
                frontier,
                page_auditors,
                site_auditors,
                pages_fetched,
                tx,
            )
            .await;
        });
    }

    // ── 4. Drop the original sender ──────────────────────────────────────────
    // This is critical: the channel closes when all per-worker tx clones are
    // dropped.  If we keep this sender alive, `rx.recv()` never returns `None`.
    drop(tx);

    // ── 5. Collect results ───────────────────────────────────────────────────
    let mut reports = Vec::new();
    while let Some(report) = rx.recv().await {
        reports.push(report);
    }

    // ── 6. Clean up Redis keys ───────────────────────────────────────────────
    {
        let mut f = frontier.lock().await;
        if let Err(e) = f.clear().await {
            tracing::warn!(error = %e, "failed to clear frontier after crawl");
        }
    }

    Ok(reports)
}

/// Main loop executed by each worker task.
///
/// The worker never propagates errors to the caller — all errors are logged
/// via `tracing::warn!` and skipped.  The worker exits when the frontier
/// reports completion (`is_complete() == true`) while the queue is empty.
async fn worker_loop(
    config: Arc<CrawlConfig>,
    fetcher: Arc<Fetcher>,
    frontier: Arc<Mutex<Frontier>>,
    page_auditors: Arc<Vec<Box<dyn PageAuditor>>>,
    site_auditors: Arc<Vec<Box<dyn SiteAuditor>>>,
    pages_fetched: Arc<AtomicUsize>,
    tx: tokio::sync::mpsc::UnboundedSender<PageReport>,
) {
    loop {
        // ── Dequeue ──────────────────────────────────────────────────────────
        let dequeued = {
            let mut f = frontier.lock().await;
            match f.dequeue().await {
                Ok(item) => item,
                Err(e) => {
                    tracing::warn!(error = %e, "frontier dequeue error; retrying");
                    continue;
                }
            }
            // Lock released here — guard dropped at end of block
        };

        let (depth, url_str) = match dequeued {
            Some(item) => item,
            None => {
                // Queue is empty — sleep first, then check whether all inflight work is done.
                tokio::time::sleep(std::time::Duration::from_millis(50)).await;

                let complete = {
                    let mut f = frontier.lock().await;
                    match f.is_complete().await {
                        Ok(v) => v,
                        Err(e) => {
                            tracing::warn!(error = %e, "frontier is_complete error");
                            false
                        }
                    }
                };

                if complete {
                    break;
                }

                continue;
            }
        };

        // ── Parse URL ────────────────────────────────────────────────────────
        let url = match Url::parse(&url_str) {
            Ok(u) => u,
            Err(e) => {
                tracing::warn!(url = %url_str, error = %e, "skipping unparseable URL");
                mark_done_warn(&frontier).await;
                continue;
            }
        };

        // ── max_pages cap ────────────────────────────────────────────────────
        if let Some(max) = config.max_pages {
            // fetch_add returns the value *before* the increment.
            let prev = pages_fetched.fetch_add(1, Ordering::SeqCst);
            if prev >= max {
                // Undo the increment — this slot was wasted.
                pages_fetched.fetch_sub(1, Ordering::SeqCst);
                tracing::warn!(url = %url, "max_pages cap reached; skipping");
                mark_done_warn(&frontier).await;
                continue;
            }
        } else {
            pages_fetched.fetch_add(1, Ordering::SeqCst);
        }

        // ── Fetch ────────────────────────────────────────────────────────────
        let page_data: PageData = match fetcher.fetch(&url).await {
            Ok(pd) => pd,
            Err(e) => {
                tracing::warn!(url = %url, error = %e, "fetch error; skipping page");
                mark_done_warn(&frontier).await;
                continue;
            }
        };

        // Overwrite the depth field that fetch() always sets to 0.
        let page_data = PageData { depth, ..page_data };

        // ── Page audits + link discovery (spawn_blocking — Dom is !Send) ─────
        let html = page_data.html.clone();
        let page_snap = page_data.clone();
        let auditors_snap = Arc::clone(&page_auditors);
        let base_url = url.clone();

        let blocking_result = tokio::task::spawn_blocking(move || {
            let dom = Dom::parse(&html);

            let findings: Vec<crate::model::Finding> = auditors_snap
                .iter()
                .flat_map(|a| a.audit(&page_snap, &dom))
                .collect();

            let child_urls: Vec<Url> = discover_links(&base_url, &dom);

            (findings, child_urls)
        })
        .await;

        let (mut findings, child_urls) = match blocking_result {
            Ok(pair) => pair,
            Err(e) => {
                tracing::warn!(url = %url, error = %e, "spawn_blocking panicked; skipping page");
                mark_done_warn(&frontier).await;
                continue;
            }
        };

        // ── Site audits (async — must be outside spawn_blocking) ─────────────
        let ctx = AuditContext {
            config: &config,
            fetcher: &fetcher,
        };
        for auditor in site_auditors.iter() {
            let mut f = auditor.audit(&page_data, &ctx).await;
            findings.append(&mut f);
        }

        // ── Score & send ─────────────────────────────────────────────────────
        let report = score_page(url.clone(), findings);
        // Ignore send errors — receiver may have dropped after a panic.
        let _ = tx.send(report);

        // ── Enqueue children ─────────────────────────────────────────────────
        if depth < config.depth {
            let next_depth = depth + 1;
            for child in child_urls {
                let mut f = frontier.lock().await;
                match f.enqueue(child.as_str(), next_depth).await {
                    Ok(_) => {}
                    Err(e) => {
                        tracing::warn!(url = %child, error = %e, "failed to enqueue child URL");
                    }
                }
            }
        }

        // ── Mark done ────────────────────────────────────────────────────────
        mark_done_warn(&frontier).await;
    }
}

/// Call `mark_done` on the frontier, logging any error but never propagating it.
async fn mark_done_warn(frontier: &Arc<Mutex<Frontier>>) {
    let mut f = frontier.lock().await;
    if let Err(e) = f.mark_done().await {
        tracing::warn!(error = %e, "failed to call mark_done on frontier");
    }
}
