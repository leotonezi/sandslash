use sandslash::fetcher::HostRateLimiter;
use std::num::NonZeroU32;
use std::time::{Duration, Instant};

/// 10 sequential acquires at qps=2 must take between 4 000 ms and 7 000 ms.
///
/// At 2 req/s the first two calls are "free" (burst), then each subsequent
/// pair costs ~500 ms.  10 calls consume 4 full "inter-bucket" intervals
/// (calls 3–10 → 4 × 500 ms = 4 000 ms minimum).  7 000 ms is a generous
/// upper bound to accommodate slow CI runners.
///
/// This test intentionally takes ~5 s; that is expected.
#[tokio::test]
async fn ten_acquires_at_qps_two_takes_between_4_and_7_seconds() {
    let qps = NonZeroU32::new(2).expect("invariant: 2 != 0");
    let limiter = HostRateLimiter::new(qps);

    let start = Instant::now();
    for _ in 0..10 {
        limiter.acquire("example.com").await;
    }
    let elapsed = start.elapsed();

    assert!(
        elapsed >= Duration::from_millis(4000),
        "expected elapsed >= 4000 ms, got {elapsed:?}"
    );
    assert!(
        elapsed < Duration::from_millis(7000),
        "expected elapsed < 7000 ms, got {elapsed:?}"
    );
}
