// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Per-domain rate limiting via governor.
//!
//! Ensures polite crawling by limiting request rate per domain.
//! Uses a keyed rate limiter with DashMap for lock-free concurrent access.

use governor::clock::DefaultClock;
use governor::state::keyed::DashMapStateStore;
use governor::{Quota, RateLimiter};
use std::num::NonZeroU32;

/// Per-domain rate limiter using token bucket algorithm.
///
/// Each domain gets its own independent rate limit. Requests to different
/// domains are not affected by each other.
pub struct DomainRateLimiter {
    limiter: RateLimiter<String, DashMapStateStore<String>, DefaultClock>,
}

impl DomainRateLimiter {
    /// Create a new rate limiter with the given requests per second.
    ///
    /// If `requests_per_second` is 0, it is clamped to 1 RPS.
    pub fn new(requests_per_second: u32) -> Self {
        let rps = NonZeroU32::new(requests_per_second).unwrap_or(NonZeroU32::MIN);
        let quota = Quota::per_second(rps);
        Self {
            limiter: RateLimiter::dashmap(quota),
        }
    }

    /// Wait until a request to the given domain is allowed.
    ///
    /// Blocks the current async task until the rate limit permits the request.
    pub async fn acquire(&self, domain: &str) {
        self.limiter.until_key_ready(&domain.to_string()).await;
    }

    /// Try to acquire a permit without waiting.
    ///
    /// Returns `true` if the request is immediately allowed, `false` if rate-limited.
    pub fn try_acquire(&self, domain: &str) -> bool {
        self.limiter.check_key(&domain.to_string()).is_ok()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn creates_with_valid_rate() {
        let _limiter = DomainRateLimiter::new(5);
    }

    #[test]
    fn try_acquire_first_request_succeeds() {
        let limiter = DomainRateLimiter::new(1);
        assert!(limiter.try_acquire("example.com"));
    }

    #[test]
    fn different_domains_independent() {
        let limiter = DomainRateLimiter::new(1);
        assert!(limiter.try_acquire("a.com"));
        assert!(limiter.try_acquire("b.com"));
        // Both should succeed because they're different domains
    }

    #[tokio::test]
    async fn acquire_allows_request() {
        let limiter = DomainRateLimiter::new(10);
        // Should not block with a generous rate
        limiter.acquire("example.com").await;
    }
}
