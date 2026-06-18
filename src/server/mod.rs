pub mod progress;
pub mod routes;

use axum::{Json, Router, http::StatusCode, response::IntoResponse};
use dashmap::DashMap;
use std::sync::Arc;
use std::time::Instant;
use tokio::sync::broadcast;
use tower_http::cors::CorsLayer;

use crate::server::progress::ProgressEvent;

pub type JobId = String;

#[derive(Clone)]
pub struct JobHandle {
    pub tx: broadcast::Sender<ProgressEvent>,
    pub created_at: Instant,
    /// Root URL captured at job creation; used to synthesize a `Started` event
    /// for late subscribers who missed the initial broadcast.
    pub root: String,
    /// Job ID, also used for the synthetic `Started` event.
    pub job_id: String,
}

#[derive(Clone)]
pub struct AppState {
    pub jobs: Arc<DashMap<JobId, JobHandle>>,
}

impl AppState {
    pub fn new() -> Self {
        Self {
            jobs: Arc::new(DashMap::new()),
        }
    }
}

impl Default for AppState {
    fn default() -> Self {
        Self::new()
    }
}

pub fn router(state: AppState) -> Router {
    use axum::routing::{get, post};

    Router::new()
        .route("/healthz", get(routes::healthz))
        .route("/api/audits", post(routes::start_audit))
        .route("/api/audits/:id/events", get(routes::stream_events))
        .layer(CorsLayer::permissive())
        .with_state(state)
}

pub async fn serve(bind: std::net::SocketAddr) -> anyhow::Result<()> {
    let state = AppState::new();

    // Spawn 10-minute TTL sweeper
    let jobs_clone = state.jobs.clone();
    tokio::spawn(async move {
        loop {
            tokio::time::sleep(std::time::Duration::from_secs(60)).await;
            jobs_clone.retain(|_, v| v.created_at.elapsed().as_secs() < 600);
        }
    });

    let app = router(state);
    let listener = tokio::net::TcpListener::bind(bind)
        .await
        .map_err(|e| anyhow::anyhow!("bind error: {e}"))?;
    axum::serve(listener, app)
        .await
        .map_err(|e| anyhow::anyhow!("serve error: {e}"))?;
    Ok(())
}

// axum IntoResponse for SeoError (only compiled when axum is in scope here)
impl IntoResponse for crate::error::SeoError {
    fn into_response(self) -> axum::response::Response {
        let status = match &self {
            crate::error::SeoError::JobNotFound(_) => StatusCode::NOT_FOUND,
            _ => StatusCode::INTERNAL_SERVER_ERROR,
        };
        (
            status,
            Json(serde_json::json!({ "error": self.to_string() })),
        )
            .into_response()
    }
}
