//! Route construction for the Nika HTTP API.

pub mod artifacts;
pub mod health;
pub mod workflows;

use axum::routing::{get, post};
use axum::Router;

use crate::state::AppState;

/// Build the full API router with all v1 routes.
///
/// SSE route (`/v1/events/{id}`) is returned separately via `build_sse_router`
/// so it can be excluded from the 30s TimeoutLayer (SSE streams are long-lived).
pub fn build_router(state: AppState) -> Router {
    Router::new()
        .route("/health", get(health::health))
        .route("/v1/run", post(workflows::run_workflow))
        .route("/v1/status/{id}", get(workflows::get_status))
        .route("/v1/cancel/{id}", post(workflows::cancel_job))
        .route(
            "/v1/jobs/{id}/artifacts",
            get(artifacts::list_artifacts),
        )
        .route(
            "/v1/jobs/{id}/artifacts/{name}",
            get(artifacts::download_artifact),
        )
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
