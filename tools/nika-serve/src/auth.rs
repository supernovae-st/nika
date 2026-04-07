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
use axum::http::{header, Request, StatusCode};
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};

use crate::token_store::AuthMode;

/// Return 401 with `WWW-Authenticate: Bearer realm="nika"` per RFC 7235.
fn unauthorized() -> Response {
    (
        StatusCode::UNAUTHORIZED,
        [(header::WWW_AUTHENTICATE, "Bearer realm=\"nika\"")],
    )
        .into_response()
}

/// Axum middleware that requires a valid `Authorization: Bearer <token>` header
/// on all routes except `/health`.
///
/// Delegates to `AuthMode` for the actual authentication logic (Legacy or MultiKey).
pub async fn require_auth(
    State(auth_mode): State<Arc<AuthMode>>,
    mut request: Request<Body>,
    next: Next,
) -> Response {
    // Health endpoint is always public
    if request.uri().path() == "/health" {
        return next.run(request).await;
    }

    let raw_token = request
        .headers()
        .get("authorization")
        .and_then(|v| v.to_str().ok())
        .and_then(|s| s.strip_prefix("Bearer "));

    match raw_token {
        Some(token) => {
            if let Some(principal) = auth_mode.authenticate(token).await {
                request.extensions_mut().insert(principal);
                next.run(request).await
            } else {
                unauthorized()
            }
        }
        None => unauthorized(),
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

    #[test]
    fn unauthorized_response_has_www_authenticate_header() {
        let resp = unauthorized();
        assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
        let header = resp.headers().get(header::WWW_AUTHENTICATE).unwrap();
        assert_eq!(header.to_str().unwrap(), "Bearer realm=\"nika\"");
    }

    // =========================================================================
    // Middleware integration tests (full request → response cycle)
    // =========================================================================

    use axum::middleware;
    use axum::routing::get;
    use axum::Router;
    use tower::ServiceExt;

    fn test_app(auth: Arc<AuthMode>) -> Router {
        Router::new()
            .route("/api/test", get(|| async { "ok" }))
            .route("/health", get(|| async { "healthy" }))
            .layer(middleware::from_fn_with_state(auth.clone(), require_auth))
            .with_state(auth)
    }

    #[tokio::test]
    async fn middleware_rejects_without_token() {
        let app = test_app(make_legacy_auth());
        let resp = app
            .oneshot(
                Request::builder()
                    .uri("/api/test")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
        let www_auth = resp.headers().get(header::WWW_AUTHENTICATE).unwrap();
        assert_eq!(www_auth.to_str().unwrap(), "Bearer realm=\"nika\"");
    }

    #[tokio::test]
    async fn middleware_accepts_valid_bearer() {
        let app = test_app(make_legacy_auth());
        let resp = app
            .oneshot(
                Request::builder()
                    .uri("/api/test")
                    .header("Authorization", format!("Bearer {TEST_TOKEN}"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn middleware_rejects_wrong_bearer() {
        let app = test_app(make_legacy_auth());
        let resp = app
            .oneshot(
                Request::builder()
                    .uri("/api/test")
                    .header("Authorization", "Bearer wrong-token")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
        assert!(resp.headers().get(header::WWW_AUTHENTICATE).is_some());
    }

    #[tokio::test]
    async fn middleware_bypasses_health_endpoint() {
        let app = test_app(make_legacy_auth());
        let resp = app
            .oneshot(
                Request::builder()
                    .uri("/health")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
    }
}
