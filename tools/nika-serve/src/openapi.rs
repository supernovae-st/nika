//! OpenAPI 3.1 spec generation and serving.

use std::sync::Arc;

use aide::openapi::{Contact, License, OpenApi, SecurityScheme};
use aide::transform::TransformOpenApi;
use axum::Extension;
use axum::Json;

/// Configure the OpenAPI spec metadata.
pub fn configure(api: TransformOpenApi) -> TransformOpenApi {
    api.title("Nika Serve API")
        .version(env!("CARGO_PKG_VERSION"))
        .description("HTTP API for the Nika workflow engine. Schema: nika/workflow@0.12")
        .license(License {
            name: "AGPL-3.0-or-later".into(),
            url: Some("https://www.gnu.org/licenses/agpl-3.0.html".into()),
            ..Default::default()
        })
        .contact(Contact {
            name: Some("SuperNovae Studio".into()),
            url: Some("https://supernovae.studio".into()),
            email: Some("nika@supernovae.studio".into()),
            ..Default::default()
        })
        .security_scheme(
            "bearerAuth",
            SecurityScheme::Http {
                scheme: "bearer".into(),
                bearer_format: Some("token".into()),
                description: Some("NIKA_SERVE_TOKEN (minimum 32 characters)".into()),
                extensions: Default::default(),
            },
        )
        .security_requirement("bearerAuth")
}

/// `GET /v1/openapi.json` — Serve the auto-generated OpenAPI 3.1 spec.
pub async fn serve(Extension(api): Extension<Arc<OpenApi>>) -> Json<OpenApi> {
    Json(api.as_ref().clone())
}
