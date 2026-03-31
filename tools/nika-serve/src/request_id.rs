//! X-Request-Id middleware.
//!
//! Ensures every response includes an `x-request-id` header.
//! If the client provides one, it is echoed back. Otherwise, a new UUID v4 is generated.

use axum::http::{HeaderValue, Request};
use axum::middleware::Next;
use axum::response::Response;

/// Header name for request ID.
const REQUEST_ID_HEADER: &str = "x-request-id";

/// Middleware that propagates or generates an `x-request-id` header.
pub async fn request_id_middleware(req: Request<axum::body::Body>, next: Next) -> Response {
    // Extract existing request ID or generate a new one
    let id = req
        .headers()
        .get(REQUEST_ID_HEADER)
        .and_then(|v| v.to_str().ok())
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
}
