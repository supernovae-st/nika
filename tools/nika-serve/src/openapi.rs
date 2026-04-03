//! OpenAPI 3.1 spec generation and serving.

use std::sync::Arc;

use aide::openapi::OpenApi;
use aide::transform::TransformOpenApi;
use axum::Extension;
use axum::Json;

/// Configure the OpenAPI spec metadata.
pub fn configure(api: TransformOpenApi) -> TransformOpenApi {
    api.title("Nika Serve API")
        .version(env!("CARGO_PKG_VERSION"))
        .description("HTTP API for the Nika workflow engine. Schema: nika/workflow@0.12")
}

/// `GET /v1/openapi.json` — Serve the auto-generated OpenAPI 3.1 spec.
pub async fn serve(Extension(api): Extension<Arc<OpenApi>>) -> Json<OpenApi> {
    Json(api.as_ref().clone())
}
