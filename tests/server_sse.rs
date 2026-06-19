//! Integration tests for the SSE-based HTTP audit server.
//!
//! These tests spin up the axum server on an OS-assigned port and verify that
//! the SSE stream emits the expected events.
//!
//! Redis-dependent tests (depth > 0) are gated with `#[ignore]`.

use std::time::Duration;

use reqwest::Client;
use sandslash::server::{AppState, router};
use tokio::net::TcpListener;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

const SIMPLE_HTML: &str = include_str!("fixtures/sse_test.html");

/// Start the axum server on a random port and return its base URL.
async fn start_server() -> String {
    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .expect("invariant: OS must assign a free port");
    let addr = listener
        .local_addr()
        .expect("invariant: bound listener has local addr");
    let state = AppState::new();
    let app = router(state);

    tokio::spawn(async move {
        axum::serve(listener, app)
            .await
            .expect("invariant: server should not fail in tests");
    });

    format!("http://{addr}")
}

/// Parse SSE frames from raw text.
///
/// Each frame looks like:
/// ```
/// event: Started\n
/// data: {...}\n
/// \n
/// ```
fn parse_sse_frames(body: &str) -> Vec<(String, String)> {
    let mut frames = Vec::new();
    let mut event_name = String::new();
    let mut event_data = String::new();

    for line in body.lines() {
        if let Some(name) = line.strip_prefix("event:") {
            event_name = name.trim().to_owned();
        } else if let Some(data) = line.strip_prefix("data:") {
            event_data = data.trim().to_owned();
        } else if line.is_empty() && !event_name.is_empty() {
            frames.push((event_name.clone(), event_data.clone()));
            event_name.clear();
            event_data.clear();
        }
    }
    frames
}

fn collect_event_names(body: &str) -> Vec<String> {
    parse_sse_frames(body)
        .into_iter()
        .map(|(name, _)| name)
        .collect()
}

/// AC: depth=0 audit → SSE stream emits Started event, then Done (or PageDone).
///
/// Uses wiremock to serve a simple HTML page so no real network is hit.
#[tokio::test]
async fn sse_depth0_emits_started_and_completion() {
    // 1. Spin up wiremock to serve the target HTML.
    let mock_server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/"))
        .respond_with(ResponseTemplate::new(200).set_body_string(SIMPLE_HTML))
        .mount(&mock_server)
        .await;
    // Serve robots.txt as 404 so robots-check passes quickly.
    Mock::given(method("GET"))
        .and(path("/robots.txt"))
        .respond_with(ResponseTemplate::new(404))
        .mount(&mock_server)
        .await;

    // 2. Start the axum server.
    let base = start_server().await;

    // 3. POST /api/audits — request depth=0 so Redis is not needed.
    let client = Client::builder()
        .timeout(Duration::from_secs(30))
        .build()
        .expect("invariant: reqwest client must build");

    let resp = client
        .post(format!("{base}/api/audits"))
        .json(&serde_json::json!({
            "url": format!("{}/", mock_server.uri()),
            "depth": 0,
            "max_pages": 1,
        }))
        .send()
        .await
        .expect("POST /api/audits must succeed");

    assert_eq!(
        resp.status(),
        reqwest::StatusCode::ACCEPTED,
        "expected 202 Accepted from /api/audits"
    );

    let body: serde_json::Value = resp.json().await.expect("response must be JSON");
    let job_id = body["job_id"]
        .as_str()
        .expect("response must have job_id string");

    // 4. Subscribe to the SSE stream.
    let mut sse_resp = client
        .get(format!("{base}/api/audits/{job_id}/events"))
        .send()
        .await
        .expect("GET /api/audits/:id/events must succeed");

    assert_eq!(
        sse_resp.status(),
        reqwest::StatusCode::OK,
        "SSE endpoint must return 200"
    );

    // 5. Collect SSE bytes until Done or Error arrives, or timeout.
    let deadline = tokio::time::Instant::now() + Duration::from_secs(20);
    let mut collected = String::new();
    let mut done = false;

    while tokio::time::Instant::now() < deadline && !done {
        // Read the next chunk (non-blocking with timeout).
        match tokio::time::timeout(Duration::from_secs(5), sse_resp.chunk()).await {
            Ok(Ok(Some(chunk))) => {
                let text = String::from_utf8_lossy(&chunk);
                collected.push_str(&text);
                // Stop when we see a Done or Error event.
                if collected.contains("event: Done") || collected.contains("event: Error") {
                    done = true;
                }
            }
            Ok(Ok(None)) => {
                // Stream closed.
                break;
            }
            Ok(Err(e)) => {
                panic!("SSE stream error: {e}");
            }
            Err(_) => {
                // Timeout on this chunk — keep looping if overall deadline not reached.
            }
        }
    }

    // 6. Assert expected events.
    let frames = parse_sse_frames(&collected);
    let event_names: Vec<&str> = frames.iter().map(|(n, _)| n.as_str()).collect();

    assert!(
        event_names.contains(&"Started"),
        "SSE stream must contain Started; got: {event_names:?}\nraw: {collected}"
    );
    assert!(
        event_names.contains(&"PageDone"),
        "SSE stream must contain at least one PageDone (depth=0 emits 1); got: {event_names:?}\nraw: {collected}"
    );
    assert!(
        event_names.contains(&"Done"),
        "SSE stream must contain Done (not just Error); got: {event_names:?}\nraw: {collected}"
    );

    // Verify Done event carries a valid AuditReport with site_score.
    let done_data = frames
        .iter()
        .find(|(n, _)| n == "Done")
        .map(|(_, d)| d.as_str())
        .expect("Done event must have data");
    let done_json: serde_json::Value =
        serde_json::from_str(done_data).expect("Done data must be valid JSON");
    let report = done_json
        .get("report")
        .expect("Done data must have 'report' field");
    let site_score = report
        .get("site_score")
        .and_then(|s| s.as_u64())
        .expect("report must have numeric site_score");
    assert!(
        site_score <= 100,
        "site_score must be 0–100, got {site_score}"
    );
}

/// AC: unknown job_id returns 404.
#[tokio::test]
async fn sse_unknown_job_returns_404() {
    let base = start_server().await;
    let client = Client::new();

    let resp = client
        .get(format!("{base}/api/audits/nonexistent-job-id/events"))
        .send()
        .await
        .expect("GET must not error");

    assert_eq!(
        resp.status(),
        reqwest::StatusCode::NOT_FOUND,
        "unknown job_id must return 404"
    );
}

/// AC: /healthz returns 200.
#[tokio::test]
async fn healthz_returns_200() {
    let base = start_server().await;
    let client = Client::new();

    let resp = client
        .get(format!("{base}/healthz"))
        .send()
        .await
        .expect("GET /healthz must succeed");

    assert_eq!(resp.status(), reqwest::StatusCode::OK);
}

/// AC: depth=1 crawl via server emits Started then Done (requires Redis).
///
/// Marked ignore because it needs a local Redis instance.
#[tokio::test]
#[ignore = "requires local Redis on 127.0.0.1:6379"]
async fn sse_depth1_with_redis_emits_started_and_done() {
    let mock_server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/"))
        .respond_with(ResponseTemplate::new(200).set_body_string(SIMPLE_HTML))
        .mount(&mock_server)
        .await;
    Mock::given(method("GET"))
        .and(path("/robots.txt"))
        .respond_with(ResponseTemplate::new(404))
        .mount(&mock_server)
        .await;

    let base = start_server().await;
    let client = Client::builder()
        .timeout(Duration::from_secs(60))
        .build()
        .expect("invariant: client must build");

    let resp = client
        .post(format!("{base}/api/audits"))
        .json(&serde_json::json!({
            "url": format!("{}/", mock_server.uri()),
            "depth": 1,
            "max_pages": 2,
        }))
        .send()
        .await
        .expect("POST must succeed");

    assert_eq!(resp.status(), reqwest::StatusCode::ACCEPTED);
    let body: serde_json::Value = resp.json().await.expect("must be JSON");
    let job_id = body["job_id"].as_str().expect("must have job_id");

    let mut sse_resp = client
        .get(format!("{base}/api/audits/{job_id}/events"))
        .send()
        .await
        .expect("SSE GET must succeed");

    let deadline = tokio::time::Instant::now() + Duration::from_secs(30);
    let mut collected = String::new();
    let mut done = false;

    while tokio::time::Instant::now() < deadline && !done {
        match tokio::time::timeout(Duration::from_secs(5), sse_resp.chunk()).await {
            Ok(Ok(Some(chunk))) => {
                let text = String::from_utf8_lossy(&chunk);
                collected.push_str(&text);
                if collected.contains("event: Done") || collected.contains("event: Error") {
                    done = true;
                }
            }
            Ok(Ok(None)) => break,
            Ok(Err(e)) => panic!("SSE stream error: {e}"),
            Err(_) => {}
        }
    }

    let events = collect_event_names(&collected);
    assert!(
        events.contains(&"Started".to_owned()),
        "must have Started event; got: {events:?}"
    );
    let has_end = events.contains(&"Done".to_owned()) || events.contains(&"Error".to_owned());
    assert!(has_end, "must have Done or Error event; got: {events:?}");
}
