//! Health check endpoint (public -- no auth required).

use aide::transform::TransformOperation;
use axum::Json;
use schemars::JsonSchema;
use serde::Serialize;

#[derive(Serialize, JsonSchema)]
pub struct HealthResponse {
    pub status: String,
    pub version: String,
    pub service: String,
}

/// `GET /health` -- Returns 200 with version info.
pub async fn health() -> Json<HealthResponse> {
    Json(HealthResponse {
        status: "ok".into(),
        version: env!("CARGO_PKG_VERSION").into(),
        service: "nika-serve".into(),
    })
}

pub fn docs(op: TransformOperation) -> TransformOperation {
    op.id("healthCheck")
        .summary("Health check")
        .description("Returns 200 with version info. No authentication required.")
        .tag("system")
}
