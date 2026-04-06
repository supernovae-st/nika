//! Bearer token authentication middleware.
//!
//! Supports two modes:
//! - **Legacy**: single `NIKA_SERVE_TOKEN` env var (backward compatible)
//! - **MultiKey**: named API keys from SQLite with BLAKE3 + moka cache
//!
//! The `/health` endpoint bypasses authentication.

use std::sync::Arc;

use axum::body::Body;
use axum::extract::State;
use axum::http::{Request, StatusCode};
use axum::middleware::Next;
use axum::response::Response;

use crate::token_store::AuthMode;

/// Axum middleware that requires a valid `Authorization: Bearer <token>` header
/// on all routes except `/health`.
///
/// Delegates to `AuthMode` for the actual authentication logic (Legacy or MultiKey).
pub async fn require_auth(
    State(auth_mode): State<Arc<AuthMode>>,
    request: Request<Body>,
    next: Next,
) -> Result<Response, StatusCode> {
    // Health endpoint is always public
    if request.uri().path() == "/health" {
        return Ok(next.run(request).await);
    }

    let raw_token = request
        .headers()
        .get("authorization")
        .and_then(|v| v.to_str().ok())
        .and_then(|s| s.strip_prefix("Bearer "));

    match raw_token {
        Some(token) => {
            if auth_mode.authenticate(token).await.is_some() {
                Ok(next.run(request).await)
            } else {
                Err(StatusCode::UNAUTHORIZED)
            }
        }
        None => Err(StatusCode::UNAUTHORIZED),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::token_store::{hash_token, AuthMode};

    const TEST_TOKEN: &str = "test-secret-token-42-long-enough";

    fn make_legacy_auth() -> Arc<AuthMode> {
        Arc::new(AuthMode::Legacy {
            expected_hash: hash_token(TEST_TOKEN),
        })
    }

    // =========================================================================
    // Token validation via AuthMode (unit tests — no middleware)
    // =========================================================================

    #[tokio::test]
    async fn accepts_valid_token() {
        let auth = make_legacy_auth();
        assert!(auth.authenticate(TEST_TOKEN).await.is_some());
    }

    #[tokio::test]
    async fn accepts_long_token() {
        let long = "a".repeat(256);
        let auth = Arc::new(AuthMode::Legacy {
            expected_hash: hash_token(&long),
        });
        assert!(auth.authenticate(&long).await.is_some());
    }

    #[tokio::test]
    async fn rejects_wrong_token() {
        let auth = make_legacy_auth();
        assert!(auth.authenticate("wrong-token").await.is_none());
    }

    #[tokio::test]
    async fn rejects_empty_token() {
        let auth = make_legacy_auth();
        assert!(auth.authenticate("").await.is_none());
    }

    #[tokio::test]
    async fn constant_time_comparison_via_blake3() {
        let auth = make_legacy_auth();
        // Tokens of different lengths both produce fixed 32-byte BLAKE3 digests
        let short = auth.authenticate("x").await;
        let long = auth.authenticate(&"x".repeat(1000)).await;
        assert!(short.is_none());
        assert!(long.is_none());
    }
}
