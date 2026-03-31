//! Health check endpoint (public -- no auth required).

use axum::Json;
use serde_json::{json, Value};

/// `GET /health` -- Returns 200 with version info.
pub async fn health() -> Json<Value> {
    Json(json!({
        "status": "ok",
        "version": env!("CARGO_PKG_VERSION"),
        "service": "nika-serve",
    }))
}
