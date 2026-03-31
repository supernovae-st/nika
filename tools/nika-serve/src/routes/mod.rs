//! Route construction for the Nika HTTP API.

pub mod health;
pub mod workflows;

use axum::routing::{get, post};
use axum::Router;

use crate::state::AppState;

/// Build the full API router with all v1 routes.
pub fn build_router(state: AppState) -> Router {
    Router::new()
        .route("/health", get(health::health))
        .route("/v1/run", post(workflows::run_workflow))
        .route("/v1/status/{id}", get(workflows::get_status))
        .route("/v1/cancel/{id}", post(workflows::cancel_job))
        .route("/v1/events/{id}", get(crate::events::stream_events))
        .with_state(state)
}
