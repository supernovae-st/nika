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

/// Extract and validate a Bearer token from the Authorization header.
///
/// Returns `Ok(())` on valid token, `Err(StatusCode::UNAUTHORIZED)` otherwise.
/// Uses SHA-256 + constant-time comparison to prevent timing side-channels.
fn check_bearer_token(auth_header: Option<&str>, expected_token: &str) -> Result<(), StatusCode> {
    let token = auth_header
        .and_then(|v| v.strip_prefix("Bearer "));

    match token {
        Some(t) => {
            let expected = Sha256::digest(expected_token.as_bytes());
            let provided = Sha256::digest(t.as_bytes());
            if bool::from(expected.ct_eq(&provided)) {
                Ok(())
            } else {
                Err(StatusCode::UNAUTHORIZED)
            }
        }
        _ => Err(StatusCode::UNAUTHORIZED),
    }
}

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

    let auth_header = request
        .headers()
        .get("authorization")
        .and_then(|v| v.to_str().ok());

    check_bearer_token(auth_header, &state.config.auth_token)?;
    Ok(next.run(request).await)
}

#[cfg(test)]
mod tests {
    use super::*;

    const TEST_TOKEN: &str = "test-secret-token-42";

    fn bearer(token: &str) -> String {
        format!("Bearer {token}")
    }

    // =========================================================================
    // Valid token acceptance
    // =========================================================================

    #[test]
    fn accepts_valid_token() {
        let header = bearer(TEST_TOKEN);
        assert!(check_bearer_token(Some(&header), TEST_TOKEN).is_ok());
    }

    #[test]
    fn accepts_long_token() {
        let long = "a".repeat(256);
        let header = bearer(&long);
        assert!(check_bearer_token(Some(&header), &long).is_ok());
    }

    // =========================================================================
    // Invalid token rejection (401)
    // =========================================================================

    #[test]
    fn rejects_wrong_token() {
        let header = bearer("wrong-token");
        assert_eq!(
            check_bearer_token(Some(&header), TEST_TOKEN),
            Err(StatusCode::UNAUTHORIZED)
        );
    }

    #[test]
    fn rejects_missing_header() {
        assert_eq!(
            check_bearer_token(None, TEST_TOKEN),
            Err(StatusCode::UNAUTHORIZED)
        );
    }

    #[test]
    fn rejects_empty_header() {
        assert_eq!(
            check_bearer_token(Some(""), TEST_TOKEN),
            Err(StatusCode::UNAUTHORIZED)
        );
    }

    #[test]
    fn rejects_missing_bearer_prefix() {
        // Token without "Bearer " prefix
        assert_eq!(
            check_bearer_token(Some(TEST_TOKEN), TEST_TOKEN),
            Err(StatusCode::UNAUTHORIZED)
        );
    }

    #[test]
    fn rejects_basic_auth_scheme() {
        // Basic auth instead of Bearer
        assert_eq!(
            check_bearer_token(Some("Basic dXNlcjpwYXNz"), TEST_TOKEN),
            Err(StatusCode::UNAUTHORIZED)
        );
    }

    #[test]
    fn rejects_bearer_lowercase() {
        // "bearer " (lowercase) should NOT match — RFC 6750 is case-sensitive
        // for the "Bearer" scheme in practice
        let header = format!("bearer {TEST_TOKEN}");
        assert_eq!(
            check_bearer_token(Some(&header), TEST_TOKEN),
            Err(StatusCode::UNAUTHORIZED)
        );
    }

    #[test]
    fn rejects_empty_token_after_bearer() {
        assert_eq!(
            check_bearer_token(Some("Bearer "), TEST_TOKEN),
            Err(StatusCode::UNAUTHORIZED)
        );
    }

    #[test]
    fn rejects_token_with_extra_spaces() {
        // Extra space before token
        let header = format!("Bearer  {TEST_TOKEN}");
        assert_eq!(
            check_bearer_token(Some(&header), TEST_TOKEN),
            Err(StatusCode::UNAUTHORIZED),
            "leading space in token should not match"
        );
    }

    // =========================================================================
    // Timing attack resistance
    // =========================================================================

    #[test]
    fn constant_time_comparison_via_sha256() {
        // Verify SHA-256 is used: even tokens of different lengths
        // produce fixed 32-byte digests for constant-time comparison.
        // This test verifies the mechanism exists (can't truly test timing
        // in a unit test, but we verify the SHA-256 + ct_eq path).
        let short_wrong = bearer("x");
        let long_wrong = bearer(&"x".repeat(1000));

        // Both should fail with UNAUTHORIZED (not panic or differ)
        assert_eq!(
            check_bearer_token(Some(&short_wrong), TEST_TOKEN),
            Err(StatusCode::UNAUTHORIZED)
        );
        assert_eq!(
            check_bearer_token(Some(&long_wrong), TEST_TOKEN),
            Err(StatusCode::UNAUTHORIZED)
        );
    }
}
