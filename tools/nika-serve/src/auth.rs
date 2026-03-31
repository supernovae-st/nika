//! Bearer token authentication middleware.
//!
//! Uses constant-time comparison via `subtle` to prevent timing attacks.
//! The `/health` endpoint bypasses authentication.

use axum::body::Body;
use axum::extract::State;
use axum::http::{Request, StatusCode};
use axum::middleware::Next;
use axum::response::Response;
use subtle::ConstantTimeEq;

use crate::state::AppState;

/// Axum middleware that requires a valid `Authorization: Bearer <token>` header
/// on all routes except `/health`.
pub async fn require_auth(
    State(state): State<AppState>,
    request: Request<Body>,
    next: Next,
) -> Result<Response, StatusCode> {
    // Health endpoint is always public
    if request.uri().path() == "/health" {
        return Ok(next.run(request).await);
    }

    let token = request
        .headers()
        .get("authorization")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "));

    match token {
        Some(t) if bool::from(t.as_bytes().ct_eq(state.config.auth_token.as_bytes())) => {
            Ok(next.run(request).await)
        }
        _ => Err(StatusCode::UNAUTHORIZED),
    }
}
