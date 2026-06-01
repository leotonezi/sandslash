use dashmap::DashMap;
use governor::{DefaultDirectRateLimiter, Quota, RateLimiter};
use std::{num::NonZeroU32, sync::Arc};

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
}

impl HostRateLimiter {
    /// Create a new `HostRateLimiter` that allows `qps` requests per second
    /// per host.  No allocations are made per host until `acquire` is called
    /// for that host for the first time.
    pub fn new(qps: NonZeroU32) -> Self {
        Self {
            per_host: DashMap::new(),
            qps,
        }
    }

    /// Block (asynchronously) until the token bucket for `host` allows the
    /// next request.
    ///
    /// On the first call for a given `host` the rate limiter is created
    /// lazily.  Subsequent calls for the same `host` reuse the same limiter.
    ///
    /// The `DashMap` entry guard is released before the `.await` so that
    /// concurrent tasks are never blocked on the map itself.
    pub async fn acquire(&self, host: &str) {
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
}
