use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::{
        Json,
        sse::{Event, KeepAlive, Sse},
    },
};
use serde::Deserialize;
use std::{convert::Infallible, time::Duration};
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

    // Parse root URL before inserting the job handle so the root is available.
    let root_url: url::Url = req.url.parse().map_err(SeoError::Url)?;

    state.jobs.insert(
        job_id.clone(),
        JobHandle {
            tx: tx.clone(),
            created_at: std::time::Instant::now(),
            root: root_url.to_string(),
            job_id: job_id.clone(),
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
    // Note: the Started event is synthesized per-subscriber in stream_events,
    // so we don't need to broadcast it here.
    tokio::spawn(async move {
        match crate::pipeline::run_for_server(config, tx.clone()).await {
            Ok(report) => {
                let _ = tx.send(ProgressEvent::Done { report });
            }
            Err(e) => {
                let _ = tx.send(ProgressEvent::Error {
                    message: e.to_string(),
                });
            }
        }
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
    let (tx, synthetic_started) = {
        let handle = state
            .jobs
            .get(&id)
            .ok_or_else(|| SeoError::JobNotFound(id.clone()))?;
        // Subscribe before dropping the guard so no events are missed between
        // reading the handle and subscribing.
        let rx_tx = handle.tx.clone();
        let started = ProgressEvent::Started {
            job_id: handle.job_id.clone(),
            root: handle.root.clone(),
        };
        (rx_tx, started)
        // DashMap guard dropped here — before any .await
    };

    let rx = tx.subscribe();

    // Prepend a synthetic Started event so late subscribers (who missed the
    // initial broadcast) always receive it as the first frame.
    let synthetic_sse = synthetic_started.to_sse_event().unwrap_or_else(|e| {
        Event::default().comment(format!("serialize error in synthetic Started: {e}"))
    });
    let prefix_stream = tokio_stream::once(Ok::<Event, Infallible>(synthetic_sse));

    // Use futures::StreamExt::scan (async version) to terminate the stream
    // after a Done or Error event is forwarded to the client.
    // tokio_stream::StreamExt does not provide scan, hence the qualified import.
    use futures::StreamExt as FutStreamExt;
    let broadcast_stream = FutStreamExt::scan(BroadcastStream::new(rx), false, |terminal, item| {
        let result = if *terminal {
            None
        } else {
            match item {
                Ok(ref event) => {
                    if matches!(
                        event,
                        ProgressEvent::Done { .. } | ProgressEvent::Error { .. }
                    ) {
                        *terminal = true;
                    }
                    Some(Ok(event.to_sse_event().unwrap_or_else(|e| {
                        Event::default().comment(format!("serialize error: {e}"))
                    })))
                }
                Err(tokio_stream::wrappers::errors::BroadcastStreamRecvError::Lagged(n)) => {
                    Some(Ok(Event::default().comment(format!("lagged {n} frames"))))
                }
            }
        };
        std::future::ready(result)
    });

    let stream = tokio_stream::StreamExt::chain(prefix_stream, broadcast_stream);

    Ok(Sse::new(stream).keep_alive(KeepAlive::new().interval(Duration::from_secs(15))))
}
