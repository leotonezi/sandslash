//! Integration tests for the Redis-backed crawl frontier.
//!
//! These tests require a live Redis instance at `127.0.0.1:6379`.
//! Run with:
//!   cargo test --test frontier_redis -- --ignored

use sandslash::crawler::Frontier;

const REDIS_URL: &str = "redis://127.0.0.1:6379/";

/// Generate a unique job ID to isolate each test from leftover state.
fn job_id(label: &str) -> String {
    format!("test-{}-{}", label, std::process::id())
}

/// Enqueuing the same URL twice must only insert it once (dedup).
#[tokio::test]
#[ignore = "requires local Redis on 127.0.0.1:6379"]
async fn dedup() {
    let id = job_id("dedup");
    let mut f = Frontier::new(REDIS_URL, id).await.unwrap();
    f.clear().await.unwrap();

    let first = f.enqueue("https://example.com/", 0).await.unwrap();
    let second = f.enqueue("https://example.com/", 0).await.unwrap();

    assert!(first, "first enqueue should return true (new URL)");
    assert!(!second, "second enqueue should return false (duplicate)");

    // Only one item should be in the queue.
    let item = f.dequeue().await.unwrap();
    assert!(item.is_some(), "expected one dequeued item");
    let empty = f.dequeue().await.unwrap();
    assert!(empty.is_none(), "queue must be empty after deduplication");

    f.clear().await.unwrap();
}

/// URLs must be returned in FIFO order.
#[tokio::test]
#[ignore = "requires local Redis on 127.0.0.1:6379"]
async fn fifo_order() {
    let id = job_id("fifo");
    let mut f = Frontier::new(REDIS_URL, id).await.unwrap();
    f.clear().await.unwrap();

    let urls = [
        "https://example.com/a",
        "https://example.com/b",
        "https://example.com/c",
    ];

    for (depth, url) in urls.iter().enumerate() {
        f.enqueue(url, depth as u32).await.unwrap();
    }

    for (expected_depth, expected_url) in urls.iter().enumerate() {
        let (depth, url) = f.dequeue().await.unwrap().expect("expected item in queue");
        assert_eq!(depth, expected_depth as u32);
        assert_eq!(url, *expected_url);
    }

    assert!(
        f.dequeue().await.unwrap().is_none(),
        "queue must be empty after consuming all items"
    );

    f.clear().await.unwrap();
}

/// The inflight counter must track the number of enqueued-but-incomplete items.
#[tokio::test]
#[ignore = "requires local Redis on 127.0.0.1:6379"]
async fn inflight_counter() {
    let id = job_id("inflight");
    let mut f = Frontier::new(REDIS_URL, id).await.unwrap();
    f.clear().await.unwrap();

    // Enqueue two distinct URLs — inflight should be 2.
    f.enqueue("https://example.com/x", 0).await.unwrap();
    f.enqueue("https://example.com/y", 1).await.unwrap();

    // Dequeue one item — inflight is still 2 (mark_done not yet called).
    let item = f.dequeue().await.unwrap();
    assert!(item.is_some());

    // After mark_done, inflight should drop to 1.
    f.mark_done().await.unwrap();

    // is_complete must be false: one item still in queue, one still inflight.
    assert!(
        !f.is_complete().await.unwrap(),
        "frontier should not be complete while items remain"
    );

    // Dequeue and complete the second item.
    let item2 = f.dequeue().await.unwrap();
    assert!(item2.is_some());
    f.mark_done().await.unwrap();

    // Now queue is empty and inflight == 0.
    assert!(
        f.is_complete().await.unwrap(),
        "frontier should be complete when queue empty and inflight == 0"
    );

    f.clear().await.unwrap();
}

/// `is_complete` must return false while there is pending work and true only
/// when both the queue is empty and inflight has reached zero.
#[tokio::test]
#[ignore = "requires local Redis on 127.0.0.1:6379"]
async fn is_complete_semantics() {
    let id = job_id("complete");
    let mut f = Frontier::new(REDIS_URL, id).await.unwrap();
    f.clear().await.unwrap();

    // Brand-new (empty) frontier: queue is empty and inflight key doesn't
    // exist yet, so unwrap_or(0) == 0 → should be complete.
    assert!(
        f.is_complete().await.unwrap(),
        "empty frontier should report complete"
    );

    // Enqueue one URL — now not complete.
    f.enqueue("https://example.com/only", 0).await.unwrap();
    assert!(
        !f.is_complete().await.unwrap(),
        "frontier with queued item must not be complete"
    );

    // Dequeue but do NOT call mark_done — still not complete (inflight == 1).
    let _item = f.dequeue().await.unwrap().expect("should have item");
    assert!(
        !f.is_complete().await.unwrap(),
        "frontier must not be complete while inflight > 0"
    );

    // Call mark_done — now complete.
    f.mark_done().await.unwrap();
    assert!(
        f.is_complete().await.unwrap(),
        "frontier must be complete after mark_done with empty queue"
    );

    f.clear().await.unwrap();
}
