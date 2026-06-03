//! Worker-pool crawl engine.
//!
//! `run_crawl` seeds a Redis frontier with the configured root URL, then spawns
//! `config.concurrency` async workers.  Each worker:
//!
//! 1. Dequeues a `(depth, url)` pair.
//! 2. Checks robots.txt gating (if `config.respect_robots`).
//! 3. Fetches the page.
//! 4. Runs all page-auditors **and** link discovery inside `spawn_blocking`
//!    (because `Dom` is `!Send`).
//! 5. Runs site-auditors (async, outside `spawn_blocking`).
//! 6. Scores the page and sends the `PageReport` over an unbounded channel.
//! 7. Enqueues child URLs if `depth < config.depth` and `max_pages` not reached.
//! 8. Calls `mark_done` on every exit path after a successful dequeue.

use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};

use tokio::sync::Mutex;
use tokio::task::JoinHandle;
use url::Url;

use crate::{
    audit::{AuditContext, PageAuditor, SiteAuditor},
    config::CrawlConfig,
    error::Result,
    fetcher::{Fetcher, HostRateLimiter},
    model::{PageData, PageReport},
    parser::{Dom, links::discover_links},
    report::ProgressReporter,
    score::score_page,
};

use super::{Frontier, RobotsCache};

/// Spawn the worker pool and return the JoinHandles and the result receiver.
///
/// The root URL is counted as page 1 before workers start.
/// Child URLs are counted at enqueue time (not at fetch time).
///
/// The caller is responsible for either:
/// - Awaiting all handles + draining the channel (normal path), or
/// - Aborting handles + draining the channel (timeout path).
#[allow(clippy::too_many_arguments)]
pub async fn spawn_crawl_workers(
    config: Arc<CrawlConfig>,
    fetcher: Arc<Fetcher>,
    frontier: Frontier,
    page_auditors: Arc<Vec<Box<dyn PageAuditor>>>,
    site_auditors: Arc<Vec<Box<dyn SiteAuditor>>>,
    rate_limiter: Arc<HostRateLimiter>,
    robots_cache: Arc<RobotsCache>,
    reporter: ProgressReporter,
) -> Result<(
    Vec<JoinHandle<()>>,
    tokio::sync::mpsc::UnboundedReceiver<PageReport>,
    Arc<Mutex<Frontier>>,
)> {
    // ── 1. Seed frontier ─────────────────────────────────────────────────────
    let frontier = Arc::new(Mutex::new(frontier));
    {
        let mut f = frontier.lock().await;
        f.enqueue(config.root.as_str(), 0).await?;
    }

    // ── 2. Shared state ──────────────────────────────────────────────────────
    // Root URL counts as page 1 — store 1 before workers start so child
    // enqueue gate starts at the right offset.
    let pages_fetched: Arc<AtomicUsize> = Arc::new(AtomicUsize::new(1));
    let (tx, rx) = tokio::sync::mpsc::unbounded_channel::<PageReport>();

    // ── 3. Spawn workers ─────────────────────────────────────────────────────
    let mut handles: Vec<JoinHandle<()>> = Vec::with_capacity(config.concurrency);

    for _ in 0..config.concurrency {
        let config = Arc::clone(&config);
        let fetcher = Arc::clone(&fetcher);
        let frontier = Arc::clone(&frontier);
        let page_auditors = Arc::clone(&page_auditors);
        let site_auditors = Arc::clone(&site_auditors);
        let pages_fetched = Arc::clone(&pages_fetched);
        let rate_limiter = Arc::clone(&rate_limiter);
        let robots_cache = Arc::clone(&robots_cache);
        // Each worker holds its own tx clone; they are all dropped when the
        // worker finishes, which closes the channel once the last worker exits.
        let tx = tx.clone();
        let reporter = reporter.clone();

        let handle = tokio::spawn(async move {
            worker_loop(
                config,
                fetcher,
                frontier,
                page_auditors,
                site_auditors,
                pages_fetched,
                rate_limiter,
                robots_cache,
                tx,
                reporter,
            )
            .await;
        });

        handles.push(handle);
    }

    // ── 4. Drop the original sender ──────────────────────────────────────────
    // This is critical: the channel closes when all per-worker tx clones are
    // dropped.  If we keep this sender alive, `rx.recv()` never returns `None`.
    drop(tx);

    Ok((handles, rx, frontier))
}

/// Run a bounded worker-pool crawl starting from `config.root`.
///
/// # Errors
/// Returns `SeoError` if seeding the frontier fails or the channel collapses
/// unexpectedly.  Individual page fetch / audit errors are logged and skipped.
#[allow(clippy::too_many_arguments)]
pub async fn run_crawl(
    config: Arc<CrawlConfig>,
    fetcher: Arc<Fetcher>,
    frontier: Frontier,
    page_auditors: Arc<Vec<Box<dyn PageAuditor>>>,
    site_auditors: Arc<Vec<Box<dyn SiteAuditor>>>,
    rate_limiter: Arc<HostRateLimiter>,
    robots_cache: Arc<RobotsCache>,
    reporter: ProgressReporter,
) -> Result<Vec<PageReport>> {
    let reporter_finish = reporter.clone();
    let (handles, mut rx, frontier) = spawn_crawl_workers(
        config,
        fetcher,
        frontier,
        page_auditors,
        site_auditors,
        rate_limiter,
        robots_cache,
        reporter,
    )
    .await?;

    // ── 5. Collect results ───────────────────────────────────────────────────
    let mut reports = Vec::new();
    while let Some(report) = rx.recv().await {
        reports.push(report);
    }

    reporter_finish.finish();

    // Wait for all workers to finish (they should have exited by now since the
    // channel drained — but we join to ensure clean shutdown).
    for handle in handles {
        let _ = handle.await;
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
#[allow(clippy::too_many_arguments)]
async fn worker_loop(
    config: Arc<CrawlConfig>,
    fetcher: Arc<Fetcher>,
    frontier: Arc<Mutex<Frontier>>,
    page_auditors: Arc<Vec<Box<dyn PageAuditor>>>,
    site_auditors: Arc<Vec<Box<dyn SiteAuditor>>>,
    pages_fetched: Arc<AtomicUsize>,
    rate_limiter: Arc<HostRateLimiter>,
    robots_cache: Arc<RobotsCache>,
    tx: tokio::sync::mpsc::UnboundedSender<PageReport>,
    reporter: ProgressReporter,
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

        // ── Robots gating (BEFORE fetch) ─────────────────────────────────────
        if config.respect_robots {
            let allowed = robots_cache
                .allowed(&url, &fetcher, &config.user_agent, &rate_limiter)
                .await;
            if !allowed {
                tracing::debug!(url = %url, "robots.txt disallowed; skipping");
                mark_done_warn(&frontier).await;
                continue;
            }
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
            config: Arc::clone(&config),
            fetcher: Arc::clone(&fetcher),
        };
        for auditor in site_auditors.iter() {
            let mut f = auditor.audit(&page_data, &ctx).await;
            findings.append(&mut f);
        }

        // ── Score & send ─────────────────────────────────────────────────────
        let report = score_page(url.clone(), findings);
        // Ignore send errors — receiver may have dropped after a panic.
        let _ = tx.send(report);
        // One page report delivered — advance the progress bar.
        reporter.inc_done();

        // ── Enqueue children ─────────────────────────────────────────────────
        // Count discovered children for the total, regardless of whether they
        // end up being enqueued (already-visited URLs are filtered by Redis).
        let child_count = child_urls.len();
        if depth < config.depth {
            let next_depth = depth + 1;
            for child in child_urls {
                // Enqueue gate: count at enqueue site, not at fetch site.
                // Root URL already accounts for page 1 (stored before workers
                // started), so children start at index 1.
                if let Some(max) = config.max_pages {
                    let prev = pages_fetched.fetch_add(1, Ordering::Relaxed);
                    if prev >= max {
                        // Rollback — this slot was not consumed.
                        pages_fetched.fetch_sub(1, Ordering::Relaxed);
                        tracing::warn!(url = %child, "max_pages cap reached; skipping enqueue");
                        continue;
                    }
                }

                let mut f = frontier.lock().await;
                match f.enqueue(child.as_str(), next_depth).await {
                    Ok(_) => {}
                    Err(e) => {
                        tracing::warn!(url = %child, error = %e, "failed to enqueue child URL");
                        // Rollback the counter since we didn't actually enqueue.
                        if config.max_pages.is_some() {
                            pages_fetched.fetch_sub(1, Ordering::Relaxed);
                        }
                    }
                }
            }
        }
        // Bump the total by the number of discovered children (outside the
        // depth check so it's always called after children are counted).
        reporter.update_total(child_count);

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

#[cfg(test)]
mod tests {
    use std::sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    };

    /// Enqueue gate admits exactly K of N candidates when max_pages = K.
    ///
    /// This is a pure unit test — no Redis required.
    #[test]
    fn enqueue_gate_admits_exactly_k_of_n() {
        const MAX: usize = 3;
        const CANDIDATES: usize = 10;

        // Root is page 1; simulate the pre-worker store(1).
        let pages_fetched = Arc::new(AtomicUsize::new(1));
        let mut admitted = 0usize;

        for _ in 0..CANDIDATES {
            let prev = pages_fetched.fetch_add(1, Ordering::Relaxed);
            if prev >= MAX {
                pages_fetched.fetch_sub(1, Ordering::Relaxed);
            } else {
                admitted += 1;
            }
        }

        assert_eq!(
            admitted,
            MAX - 1, // root already occupies slot 0, so children get MAX-1 slots
            "gate must admit exactly {} children when max_pages={MAX} and root=page1",
            MAX - 1
        );

        assert_eq!(
            pages_fetched.load(Ordering::Relaxed),
            MAX,
            "counter must settle at MAX after gate"
        );
    }
}
