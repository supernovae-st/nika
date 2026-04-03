//! Per-token rate limiting middleware using `governor`.
//!
//! Each unique Bearer token gets its own rate limiter bucket.
//! Default: 10 requests/second with burst capacity of 30.
//!
//! Response headers:
//! - `X-RateLimit-Limit`: max requests per second
//! - `X-RateLimit-Remaining`: approximate remaining burst capacity
//! - `Retry-After`: seconds until a token is available (only on 429)

use std::num::NonZeroU32;
use std::sync::Arc;

use axum::extract::State;
use axum::http::{HeaderValue, Request, StatusCode};
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};
use governor::clock::DefaultClock;
use governor::state::keyed::DashMapStateStore;
use governor::{Quota, RateLimiter};

/// Per-token rate limiter type.
pub type KeyedRateLimiter = RateLimiter<String, DashMapStateStore<String>, DefaultClock>;

/// Default: 10 requests per second, burst of 30.
const DEFAULT_RATE_PER_SECOND: u32 = 10;
const DEFAULT_BURST_SIZE: u32 = 30;

/// State passed to the rate limit middleware (carries config + limiter).
#[derive(Clone)]
pub struct RateLimitState {
    pub limiter: Arc<KeyedRateLimiter>,
    pub rate_per_second: u32,
}

/// Create a new per-token rate limiter with default settings.
pub fn new_rate_limiter() -> RateLimitState {
    new_rate_limiter_with(DEFAULT_RATE_PER_SECOND, DEFAULT_BURST_SIZE)
}

/// Create a new per-token rate limiter with custom rate and burst.
pub fn new_rate_limiter_with(rate_per_second: u32, burst: u32) -> RateLimitState {
    let rps = NonZeroU32::new(rate_per_second.max(1)).unwrap();
    let burst_cap = NonZeroU32::new(burst.max(1)).unwrap();
    let quota = Quota::per_second(rps).allow_burst(burst_cap);
    RateLimitState {
        limiter: Arc::new(RateLimiter::dashmap(quota)),
        rate_per_second,
    }
}

/// Rate limiting middleware.
///
/// Extracts the Bearer token from the Authorization header and applies
/// per-token rate limiting. Unauthenticated requests (e.g., /health)
/// pass through without rate limiting.
pub async fn rate_limit_middleware(
    State(rl): State<RateLimitState>,
    req: Request<axum::body::Body>,
    next: Next,
) -> Response {
    // Extract token from Authorization header (if present)
    let token = req
        .headers()
        .get("authorization")
        .and_then(|v| v.to_str().ok())
        .and_then(|s| s.strip_prefix("Bearer "))
        .map(|s| s.to_string());

    // No token = no rate limiting (unauthenticated requests like /health)
    let Some(key) = token else {
        return next.run(req).await;
    };

    let limit_val = HeaderValue::from_str(&rl.rate_per_second.to_string())
        .unwrap_or(HeaderValue::from_static("10"));

    match rl.limiter.check_key(&key) {
        Ok(_) => {
            let mut resp = next.run(req).await;
            // Add rate limit headers
            let headers = resp.headers_mut();
            let _ = headers.insert("x-ratelimit-limit", limit_val);
            // Approximate remaining (governor doesn't expose exact count easily)
            let _ = headers.insert("x-ratelimit-remaining", HeaderValue::from_static("ok"));
            resp
        }
        Err(not_until) => {
            let wait = not_until.wait_time_from(governor::clock::Clock::now(
                &governor::clock::DefaultClock::default(),
            ));
            let retry_after = wait.as_secs().max(1);

            let mut resp = (
                StatusCode::TOO_MANY_REQUESTS,
                axum::Json(serde_json::json!({
                    "error": "rate limit exceeded",
                    "retry_after": retry_after,
                })),
            )
                .into_response();

            let headers = resp.headers_mut();
            let _ = headers.insert(
                "retry-after",
                HeaderValue::from_str(&retry_after.to_string())
                    .unwrap_or(HeaderValue::from_static("1")),
            );
            let _ = headers.insert("x-ratelimit-limit", limit_val);
            let _ = headers.insert("x-ratelimit-remaining", HeaderValue::from_static("0"));

            resp
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rate_limiter_allows_burst() {
        let rl = new_rate_limiter();
        let key = "test-token".to_string();

        // Should allow DEFAULT_BURST_SIZE requests immediately
        for i in 0..DEFAULT_BURST_SIZE {
            assert!(
                rl.limiter.check_key(&key).is_ok(),
                "request {i} should be allowed within burst"
            );
        }

        // Next request should be rate limited
        assert!(
            rl.limiter.check_key(&key).is_err(),
            "request after burst should be rate limited"
        );
    }

    #[test]
    fn rate_limiter_separate_keys() {
        let rl = new_rate_limiter();

        // Exhaust quota for token A
        for _ in 0..DEFAULT_BURST_SIZE {
            rl.limiter.check_key(&"token-a".to_string()).unwrap();
        }

        // Token B should still have full quota
        assert!(rl.limiter.check_key(&"token-b".to_string()).is_ok());
    }

    #[test]
    fn rate_limit_state_carries_config() {
        let rl = new_rate_limiter_with(50, 100);
        assert_eq!(rl.rate_per_second, 50);

        let rl_default = new_rate_limiter();
        assert_eq!(rl_default.rate_per_second, DEFAULT_RATE_PER_SECOND);
    }
}
