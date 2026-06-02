use dashmap::DashMap;
use governor::{DefaultDirectRateLimiter, Quota, RateLimiter};
use std::{num::NonZeroU32, sync::Arc, time::Duration};
use tokio::time::Instant;

/// Per-host rate limiter backed by `governor` token buckets stored in a `DashMap`.
///
/// # Host normalisation
///
/// The caller is responsible for normalising the `host` string before calling
/// [`acquire`][HostRateLimiter::acquire].  Callers should pass the bare
/// hostname (e.g. `"example.com"`), **not** a full URL.  Two strings that
/// differ only in case are treated as two distinct hosts.
///
/// # Concurrency
///
/// `HostRateLimiter` is `Send + Sync` and may be shared across tasks via
/// `Arc`.  Each call to `acquire` clones the inner `Arc<DefaultDirectRateLimiter>`
/// out of the `DashMap` entry *before* any `.await` point, so the entry guard
/// is never held across an await (which would otherwise deadlock the map).
pub struct HostRateLimiter {
    per_host: DashMap<String, Arc<DefaultDirectRateLimiter>>,
    qps: NonZeroU32,
    /// Minimum interval between successive `acquire()` returns for a host.
    /// Set by `set_min_interval` when a `Crawl-delay` is registered.
    min_intervals: DashMap<String, Duration>,
    /// Timestamp of the last `acquire()` completion for each host.
    last_acquire: DashMap<String, Instant>,
}

impl HostRateLimiter {
    /// Create a new `HostRateLimiter` that allows `qps` requests per second
    /// per host.  No allocations are made per host until `acquire` is called
    /// for that host for the first time.
    pub fn new(qps: NonZeroU32) -> Self {
        Self {
            per_host: DashMap::new(),
            qps,
            min_intervals: DashMap::new(),
            last_acquire: DashMap::new(),
        }
    }

    /// Register a minimum interval between successive requests for `host`.
    ///
    /// This is called when a `Crawl-delay` directive is discovered in robots.txt.
    /// The effective minimum interval is the **larger** of the current value and
    /// `min` — it can never be shrunk.
    ///
    /// # Interior mutability
    ///
    /// This method takes `&self` and uses `DashMap` for interior mutability.
    pub fn set_min_interval(&self, host: &str, min: Duration) {
        // Use entry API with interior mutability; no .await here so guard safety is fine.
        let mut entry = self
            .min_intervals
            .entry(host.to_owned())
            .or_insert(Duration::ZERO);
        if min > *entry {
            *entry = min;
        }
        // Guard drops here.
    }

    /// Block (asynchronously) until the token bucket for `host` allows the
    /// next request, and then additionally enforce any registered minimum
    /// interval (`Crawl-delay`).
    ///
    /// On the first call for a given `host` the rate limiter is created
    /// lazily.  Subsequent calls for the same `host` reuse the same limiter.
    ///
    /// The `DashMap` entry guard is released before the `.await` so that
    /// concurrent tasks are never blocked on the map itself.
    pub async fn acquire(&self, host: &str) {
        // ── 1. Governor token-bucket wait ─────────────────────────────────────
        // Lazily insert a new limiter for this host if one doesn't exist yet.
        // The entry guard is dropped at the end of this block — before any
        // `.await`.
        let limiter: Arc<DefaultDirectRateLimiter> = {
            let entry = self
                .per_host
                .entry(host.to_owned())
                .or_insert_with(|| Arc::new(RateLimiter::direct(Quota::per_second(self.qps))));
            Arc::clone(entry.value())
            // `entry` (the DashMap guard) is dropped here when it goes out of scope.
        };

        // Now that the guard is dropped, it is safe to `.await`.
        limiter.until_ready().await;

        // ── 2. Crawl-delay enforcement ────────────────────────────────────────
        // Extract min_interval and last_acquire BEFORE any `.await` —
        // DashMap guards must never be held across await points.
        let min_interval: Option<Duration> = {
            self.min_intervals.get(host).map(|v| *v)
            // Guard dropped here.
        };

        if let Some(min) = min_interval {
            if min > Duration::ZERO {
                let last: Option<Instant> = {
                    self.last_acquire.get(host).map(|v| *v)
                    // Guard dropped here.
                };

                if let Some(last_instant) = last {
                    let elapsed = last_instant.elapsed();
                    if elapsed < min {
                        tokio::time::sleep(min - elapsed).await;
                    }
                }
            }
        }

        // Update last_acquire timestamp — guard is short-lived, no await after this.
        self.last_acquire.insert(host.to_owned(), Instant::now());
    }

    /// Return the number of distinct hosts that have been seen so far.
    ///
    /// Primarily intended for testing.
    #[cfg(test)]
    pub(crate) fn len(&self) -> usize {
        self.per_host.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::num::NonZeroU32;

    #[tokio::test]
    async fn same_host_reuses_limiter() {
        let qps = NonZeroU32::new(100).expect("invariant: 100 != 0");
        let limiter = HostRateLimiter::new(qps);

        limiter.acquire("example.com").await;
        limiter.acquire("example.com").await;

        assert_eq!(
            limiter.len(),
            1,
            "two acquires for the same host should reuse a single limiter"
        );
    }

    #[tokio::test]
    async fn different_hosts_get_different_limiters() {
        let qps = NonZeroU32::new(100).expect("invariant: 100 != 0");
        let limiter = HostRateLimiter::new(qps);

        limiter.acquire("a.com").await;
        limiter.acquire("b.com").await;

        assert_eq!(
            limiter.len(),
            2,
            "two distinct hosts should each have their own limiter"
        );
    }

    /// `set_min_interval` with a smaller value after a larger value must keep
    /// the larger value (never shrink).
    #[tokio::test]
    async fn set_min_interval_never_shrinks() {
        let qps = NonZeroU32::new(1000).expect("invariant: 1000 != 0");
        let limiter = HostRateLimiter::new(qps);

        limiter.set_min_interval("h.com", Duration::from_secs(5));
        limiter.set_min_interval("h.com", Duration::from_secs(2));

        let stored = limiter
            .min_intervals
            .get("h.com")
            .map(|v| *v)
            .expect("should have entry");
        assert_eq!(
            stored,
            Duration::from_secs(5),
            "smaller value must not shrink the stored interval"
        );
    }

    /// `set_min_interval` with a larger value must update the stored value.
    #[tokio::test]
    async fn set_min_interval_grows() {
        let qps = NonZeroU32::new(1000).expect("invariant: 1000 != 0");
        let limiter = HostRateLimiter::new(qps);

        limiter.set_min_interval("h.com", Duration::from_secs(2));
        limiter.set_min_interval("h.com", Duration::from_secs(5));

        let stored = limiter
            .min_intervals
            .get("h.com")
            .map(|v| *v)
            .expect("should have entry");
        assert_eq!(
            stored,
            Duration::from_secs(5),
            "larger value must update the stored interval"
        );
    }

    /// Two sequential `acquire` calls with a 2-second `min_interval` must take
    /// at least 2 seconds of wall-clock time.
    #[tokio::test(flavor = "multi_thread")]
    async fn min_interval_enforced_between_acquires() {
        let qps = NonZeroU32::new(1000).expect("invariant: 1000 != 0");
        let limiter = HostRateLimiter::new(qps);

        limiter.set_min_interval("h.com", Duration::from_secs(2));

        let start = std::time::Instant::now();
        limiter.acquire("h.com").await; // first call, no prior last_acquire
        limiter.acquire("h.com").await; // second call, must wait ~2s
        let elapsed = start.elapsed();

        assert!(
            elapsed >= Duration::from_millis(1900),
            "expected ≥ 1.9s elapsed for 2s min_interval, got {elapsed:?}"
        );
    }
}
