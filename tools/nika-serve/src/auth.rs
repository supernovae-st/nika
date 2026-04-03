//! Bearer token authentication middleware.
//!
//! Uses constant-time comparison via `subtle` to prevent timing attacks.
//! The `/health` endpoint bypasses authentication.

use axum::body::Body;
use axum::extract::State;
use axum::http::{Request, StatusCode};
use axum::middleware::Next;
use axum::response::Response;
use sha2::{Digest, Sha256};
use subtle::ConstantTimeEq;

use crate::state::AppState;

/// Axum middleware that requires a valid `Authorization: Bearer <token>` header
/// on all routes except `/health`.
///
/// Both tokens are SHA-256 hashed before constant-time comparison so that
/// inputs of different lengths still compare in fixed time (32 bytes),
/// preventing a timing side-channel that leaks token length.
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
        Some(t) => {
            let expected = Sha256::digest(state.config.auth_token.as_bytes());
            let provided = Sha256::digest(t.as_bytes());
            if bool::from(expected.ct_eq(&provided)) {
                Ok(next.run(request).await)
            } else {
                Err(StatusCode::UNAUTHORIZED)
            }
        }
        _ => Err(StatusCode::UNAUTHORIZED),
    }
}
