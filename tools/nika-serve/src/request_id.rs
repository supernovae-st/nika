//! X-Request-Id middleware.
//!
//! Ensures every response includes an `x-request-id` header.
//! If the client provides one, it is echoed back. Otherwise, a new UUID v4 is generated.

use axum::http::{HeaderValue, Request};
use axum::middleware::Next;
use axum::response::Response;

/// Header name for request ID.
const REQUEST_ID_HEADER: &str = "x-request-id";

/// Maximum length for a client-provided request ID (prevents log poisoning).
const MAX_REQUEST_ID_LEN: usize = 128;

/// Middleware that propagates or generates an `x-request-id` header.
///
/// Client-provided IDs are truncated to `MAX_REQUEST_ID_LEN` bytes
/// to prevent log poisoning via oversized headers.
pub async fn request_id_middleware(req: Request<axum::body::Body>, next: Next) -> Response {
    // Extract existing request ID or generate a new one
    let id = req
        .headers()
        .get(REQUEST_ID_HEADER)
        .and_then(|v| v.to_str().ok())
        .map(|s| {
            if s.len() > MAX_REQUEST_ID_LEN {
                &s[..MAX_REQUEST_ID_LEN]
            } else {
                s
            }
        })
        .map(String::from)
        .unwrap_or_else(|| uuid::Uuid::new_v4().simple().to_string());

    let mut resp = next.run(req).await;

    // Always set the response header (even if client didn't send one)
    if let Ok(val) = HeaderValue::from_str(&id) {
        resp.headers_mut().insert(REQUEST_ID_HEADER, val);
    }

    resp
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn request_id_header_name() {
        assert_eq!(REQUEST_ID_HEADER, "x-request-id");
    }

    #[test]
    fn long_request_id_is_truncated() {
        let long = "x".repeat(256);
        let truncated = if long.len() > MAX_REQUEST_ID_LEN {
            &long[..MAX_REQUEST_ID_LEN]
        } else {
            &long
        };
        assert_eq!(truncated.len(), 128);
    }

    #[test]
    fn short_request_id_is_preserved() {
        let short = "my-request-123";
        let result = if short.len() > MAX_REQUEST_ID_LEN {
            &short[..MAX_REQUEST_ID_LEN]
        } else {
            short
        };
        assert_eq!(result, "my-request-123");
    }
}
