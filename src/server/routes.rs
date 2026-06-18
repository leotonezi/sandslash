use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::{
        Json,
        sse::{Event, KeepAlive, Sse},
    },
};
use serde::Deserialize;
use std::{convert::Infallible, sync::Arc, time::Duration};
use tokio::sync::broadcast;
use tokio_stream::wrappers::BroadcastStream;
use uuid::Uuid;

use crate::{
    config::CrawlConfig,
    error::SeoError,
    server::{AppState, JobHandle, progress::ProgressEvent},
};

pub async fn healthz() -> StatusCode {
    StatusCode::OK
}

#[derive(Deserialize)]
pub struct StartAuditRequest {
    pub url: String,
    pub depth: Option<u32>,
    pub concurrency: Option<usize>,
    pub max_pages: Option<usize>,
}

pub async fn start_audit(
    State(state): State<AppState>,
    Json(req): Json<StartAuditRequest>,
) -> Result<(StatusCode, Json<serde_json::Value>), SeoError> {
    let job_id = Uuid::new_v4().to_string();
    let (tx, _rx) = broadcast::channel::<ProgressEvent>(256);
    let terminal: Arc<std::sync::Mutex<Option<ProgressEvent>>> =
        Arc::new(std::sync::Mutex::new(None));

    // Parse root URL before inserting the job handle so the root is available.
    let root_url: url::Url = req.url.parse().map_err(SeoError::Url)?;

    state.jobs.insert(
        job_id.clone(),
        JobHandle {
            tx: tx.clone(),
            created_at: std::time::Instant::now(),
            root: root_url.to_string(),
            job_id: job_id.clone(),
            terminal: terminal.clone(),
        },
    );

    let config = CrawlConfig {
        root: root_url.clone(),
        depth: req.depth.unwrap_or(1),
        concurrency: req.concurrency.unwrap_or(4),
        rate_per_host: 2,
        redis_url: None,
        user_agent: CrawlConfig::DEFAULT_UA.to_owned(),
        timeout_secs: 30,
        max_pages: req.max_pages,
        global_timeout_secs: None,
        respect_robots: true,
        validate_sitemap: false,
        quiet: true,
        no_color: true,
        verbose: false,
        output_json: None,
        check_external_links: false,
    };

    // Spawn the audit pipeline as a background task.
    // The Started event is synthesized per-subscriber in stream_events.
    // Terminal events (Done/Error) are stored in `terminal` BEFORE broadcasting
    // so late SSE subscribers can replay them via history.
    tokio::spawn(async move {
        let event = match crate::pipeline::run_for_server(config, tx.clone()).await {
            Ok(report) => ProgressEvent::Done { report },
            Err(e) => ProgressEvent::Error {
                message: e.to_string(),
            },
        };
        // Store terminal event for history replay before broadcasting.
        if let Ok(mut guard) = terminal.lock() {
            *guard = Some(event.clone());
        }
        let _ = tx.send(event);
    });

    Ok((
        StatusCode::ACCEPTED,
        Json(serde_json::json!({ "job_id": job_id })),
    ))
}

pub async fn stream_events(
    Path(id): Path<String>,
    State(state): State<AppState>,
) -> Result<
    Sse<impl tokio_stream::Stream<Item = Result<Event, Infallible>> + Send + 'static>,
    SeoError,
> {
    use futures::StreamExt as FutStreamExt;

    // Subscribe to broadcast AND snapshot the terminal event atomically
    // (no .await between these two operations so no race window).
    let (tx, synthetic_started, terminal_snapshot) = {
        let handle = state
            .jobs
            .get(&id)
            .ok_or_else(|| SeoError::JobNotFound(id.clone()))?;
        let rx_tx = handle.tx.clone();
        let started = ProgressEvent::Started {
            job_id: handle.job_id.clone(),
            root: handle.root.clone(),
        };
        // Snapshot terminal before dropping the guard.
        let terminal = handle.terminal.lock().ok().and_then(|g| g.clone());
        (rx_tx, started, terminal)
        // DashMap guard dropped here — before any .await
    };

    // Prepend a synthetic Started event (always sent, even to late subscribers).
    let synthetic_sse = synthetic_started.to_sse_event().unwrap_or_else(|e| {
        Event::default().comment(format!("serialize error in synthetic Started: {e}"))
    });
    let prefix_stream = tokio_stream::once(Ok::<Event, Infallible>(synthetic_sse));

    // If the audit already finished before this subscriber arrived, replay the
    // terminal event directly from the history snapshot and skip the broadcast.
    let stream: futures::stream::BoxStream<'static, Result<Event, Infallible>> =
        if let Some(terminal_event) = terminal_snapshot {
            let terminal_sse = terminal_event.to_sse_event().unwrap_or_else(|e| {
                Event::default().comment(format!("serialize error in terminal replay: {e}"))
            });
            let terminal_stream = tokio_stream::once(Ok::<Event, Infallible>(terminal_sse));
            FutStreamExt::boxed(tokio_stream::StreamExt::chain(
                prefix_stream,
                terminal_stream,
            ))
        } else {
            // Audit still in progress — subscribe to broadcast for live events.
            // scan terminates the stream after Done or Error is forwarded.
            let rx = tx.subscribe();
            let broadcast_stream =
                FutStreamExt::scan(BroadcastStream::new(rx), false, |done, item| {
                    let result = if *done {
                        None
                    } else {
                        match item {
                            Ok(ref event) => {
                                if matches!(
                                    event,
                                    ProgressEvent::Done { .. } | ProgressEvent::Error { .. }
                                ) {
                                    *done = true;
                                }
                                Some(Ok(event.to_sse_event().unwrap_or_else(|e| {
                                    Event::default().comment(format!("serialize error: {e}"))
                                })))
                            }
                            Err(
                                tokio_stream::wrappers::errors::BroadcastStreamRecvError::Lagged(n),
                            ) => Some(Ok(Event::default().comment(format!("lagged {n} frames")))),
                        }
                    };
                    std::future::ready(result)
                });
            FutStreamExt::boxed(tokio_stream::StreamExt::chain(
                prefix_stream,
                broadcast_stream,
            ))
        };

    Ok(Sse::new(stream).keep_alive(KeepAlive::new().interval(Duration::from_secs(15))))
}
