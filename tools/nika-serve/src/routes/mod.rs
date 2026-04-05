//! Route construction for the Nika HTTP API.

pub mod artifacts;
pub mod health;
pub mod workflows;

use std::sync::Arc;

use aide::axum::routing::{get_with, post_with};
use aide::axum::ApiRouter;
use aide::openapi::OpenApi;
use aide::scalar::Scalar;
use axum::routing::get;
use axum::{Extension, Router};

use crate::state::AppState;

/// Build the full API router with all v1 routes + OpenAPI spec.
///
/// SSE route (`/v1/events/{id}`) is returned separately via `build_sse_router`
/// so it can be excluded from the 30s TimeoutLayer (SSE streams are long-lived).
pub fn build_router(state: AppState) -> Router {
    let mut api = OpenApi::default();

    ApiRouter::new()
        .api_route("/health", get_with(health::health, health::docs))
        .api_route(
            "/v1/run",
            post_with(workflows::run_workflow, workflows::run_docs),
        )
        .api_route(
            "/v1/status/{id}",
            get_with(workflows::get_status, workflows::status_docs),
        )
        .api_route(
            "/v1/cancel/{id}",
            post_with(workflows::cancel_job, workflows::cancel_docs),
        )
        .api_route(
            "/v1/workflows",
            get_with(workflows::list_workflows, workflows::list_docs),
        )
        .api_route(
            "/v1/workflows/{name}/source",
            get_with(workflows::get_workflow_source, workflows::source_docs),
        )
        .api_route(
            "/v1/reload",
            post_with(workflows::reload_workflows, workflows::reload_docs),
        )
        .api_route(
            "/v1/batch/run",
            post_with(workflows::batch_run, workflows::batch_docs),
        )
        .api_route(
            "/v1/jobs",
            get_with(workflows::list_jobs, workflows::jobs_list_docs),
        )
        .api_route(
            "/v1/jobs/{id}/artifacts",
            get_with(artifacts::list_artifacts, artifacts::list_docs),
        )
        .api_route(
            "/v1/jobs/{id}/artifacts/{name}",
            get_with(artifacts::download_artifact, artifacts::download_docs),
        )
        .finish_api_with(&mut api, crate::openapi::configure)
        .route("/v1/openapi.json", get(crate::openapi::serve))
        .route(
            "/v1/docs",
            Scalar::new("/v1/openapi.json")
                .with_title("Nika Serve API")
                .axum_route()
                .into(),
        )
        .layer(Extension(Arc::new(api)))
        .with_state(state)
}

/// Build the SSE router (long-lived connections, NO TimeoutLayer).
pub fn build_sse_router(state: AppState) -> Router {
    Router::new()
        .route("/v1/events/{id}", get(crate::events::stream_events))
        .with_state(state)
}

/// Build the metrics router (separate — no auth required).
pub fn build_metrics_router(handle: metrics_exporter_prometheus::PrometheusHandle) -> Router {
    Router::new().route(
        "/metrics",
        get(move || {
            let h = handle.clone();
            async move {
                let output = h.render();
                (
                    axum::http::StatusCode::OK,
                    [("content-type", "text/plain; version=0.0.4")],
                    output,
                )
            }
        }),
    )
}
