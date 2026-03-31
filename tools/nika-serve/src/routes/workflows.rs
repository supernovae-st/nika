//! Workflow execution endpoints.
//!
//! - `POST /v1/run`           -- Submit a workflow for execution
//! - `GET  /v1/status/{id}`   -- Poll job status
//! - `POST /v1/cancel/{id}`   -- Cancel a running job (ERRATA-3)
//!
//! TODO(v0.57): Add SSE streaming endpoint (/v1/events/{id})
//! TODO(v0.57): Add idempotency keys (Idempotency-Key header)

use axum::extract::{Path, State};
use axum::Json;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use tracing::info;

use crate::error::ServeError;
use crate::state::AppState;
use crate::worker;

// ═══════════════════════════════════════════════════════════════════════════
// REQUEST / RESPONSE TYPES
// ═══════════════════════════════════════════════════════════════════════════

#[derive(Deserialize)]
pub struct RunRequest {
    /// Workflow filename relative to the configured workflows directory.
    /// Example: `"my-pipeline.nika.yaml"`
    pub workflow: String,

    /// Optional workflow inputs (key-value pairs passed as `--input`).
    pub inputs: Option<Value>,
}

#[derive(Serialize)]
pub struct RunResponse {
    pub job_id: String,
    pub status: String,
}

#[derive(Serialize)]
pub struct StatusResponse {
    pub job_id: String,
    pub status: String,
    pub workflow: String,
    pub created_at: String,
    pub started_at: Option<String>,
    pub completed_at: Option<String>,
    pub exit_code: Option<i32>,
    pub output: Option<String>,
}

// ═══════════════════════════════════════════════════════════════════════════
// HANDLERS
// ═══════════════════════════════════════════════════════════════════════════

/// `POST /v1/run` -- Submit a workflow for async execution.
///
/// Validates the workflow file exists (with path traversal protection),
/// creates a job in SQLite, spawns a subprocess worker.
pub async fn run_workflow(
    State(state): State<AppState>,
    Json(req): Json<RunRequest>,
) -> Result<Json<RunResponse>, ServeError> {
    // Validate workflow path (prevent directory traversal)
    validate_workflow_path(&req.workflow)?;

    // Check the file actually exists
    let full_path = state.config.workflows_dir.join(&req.workflow);
    let canonical = full_path.canonicalize().map_err(|_| {
        ServeError::InvalidWorkflow(format!("workflow not found: {}", req.workflow))
    })?;

    let canonical_base = state
        .config
        .workflows_dir
        .canonicalize()
        .map_err(|e| ServeError::Config(format!("workflows_dir canonicalize: {e}")))?;

    if !canonical.starts_with(&canonical_base) {
        return Err(ServeError::PathTraversal);
    }

    if !canonical.exists() {
        return Err(ServeError::InvalidWorkflow(format!(
            "workflow not found: {}",
            req.workflow
        )));
    }

    // Check queue depth (reject if too many pending)
    let pending = state
        .storage
        .list_jobs(Some(nika_storage::JobState::Pending))
        .await?;
    let running = state
        .storage
        .list_jobs(Some(nika_storage::JobState::Running))
        .await?;
    let active = pending.len() + running.len();
    // Allow 3x the concurrency limit as queue depth before rejecting
    if active >= state.config.max_concurrent * 3 {
        return Err(ServeError::QueueFull(active));
    }

    // Generate job ID (ERRATA-14: 12 chars minimum to avoid collision at ~65K jobs)
    let job_id = &uuid::Uuid::new_v4().to_string()[..12];

    // Persist job
    state.storage.create_job(job_id, &req.workflow).await?;

    info!(job_id, workflow = %req.workflow, "job created");

    // Spawn worker + track handle (ERRATA-9)
    let handle = worker::spawn_worker(&state, job_id.to_string(), req.workflow);
    state
        .workers
        .lock()
        .await
        .insert(job_id.to_string(), handle);

    Ok(Json(RunResponse {
        job_id: job_id.to_string(),
        status: "pending".into(),
    }))
}

/// `GET /v1/status/{id}` -- Poll job status.
pub async fn get_status(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<StatusResponse>, ServeError> {
    let job = state
        .storage
        .get_job(&id)
        .await?
        .ok_or(ServeError::NotFound)?;

    Ok(Json(StatusResponse {
        job_id: job.id,
        status: job.state.as_str().to_string(),
        workflow: job.workflow,
        created_at: job.created_at,
        started_at: job.started_at,
        completed_at: job.completed_at,
        exit_code: job.exit_code,
        output: job.output,
    }))
}

/// `POST /v1/cancel/{id}` -- Cancel a running job (ERRATA-3).
///
/// Aborts the worker task (which kills the subprocess via `kill_on_drop`)
/// and marks the job as cancelled.
pub async fn cancel_job(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<Value>, ServeError> {
    // Verify job exists
    let job = state
        .storage
        .get_job(&id)
        .await?
        .ok_or(ServeError::NotFound)?;

    // Only pending/running jobs can be cancelled
    if !matches!(
        job.state,
        nika_storage::JobState::Pending | nika_storage::JobState::Running
    ) {
        return Ok(Json(json!({
            "job_id": id,
            "status": job.state.as_str(),
            "message": "job already finished",
        })));
    }

    // Abort the worker task (kills subprocess via kill_on_drop)
    if let Some(handle) = state.workers.lock().await.remove(&id) {
        handle.abort();
        info!(job_id = %id, "worker task aborted");
    }

    // Mark cancelled in storage
    state
        .storage
        .update_state(
            &id,
            nika_storage::JobState::Cancelled,
            None,
            Some("cancelled by API".into()),
        )
        .await?;

    Ok(Json(json!({
        "job_id": id,
        "status": "cancelled",
    })))
}

// ═══════════════════════════════════════════════════════════════════════════
// VALIDATION
// ═══════════════════════════════════════════════════════════════════════════

/// Reject workflow paths that attempt directory traversal.
fn validate_workflow_path(workflow: &str) -> Result<(), ServeError> {
    if workflow.contains("..") || workflow.starts_with('/') || workflow.starts_with('\\') {
        return Err(ServeError::PathTraversal);
    }
    if !workflow.ends_with(".nika.yaml") {
        return Err(ServeError::InvalidWorkflow(
            "workflow must have .nika.yaml extension".into(),
        ));
    }
    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════
// TESTS
// ═══════════════════════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_traversal_paths() {
        assert!(validate_workflow_path("../../etc/passwd").is_err());
        assert!(validate_workflow_path("../secret.nika.yaml").is_err());
        assert!(validate_workflow_path("/etc/passwd").is_err());
        assert!(validate_workflow_path("\\windows\\system32").is_err());
    }

    #[test]
    fn rejects_wrong_extension() {
        assert!(validate_workflow_path("script.sh").is_err());
        assert!(validate_workflow_path("workflow.yaml").is_err());
    }

    #[test]
    fn accepts_valid_paths() {
        assert!(validate_workflow_path("pipeline.nika.yaml").is_ok());
        assert!(validate_workflow_path("subdir/flow.nika.yaml").is_ok());
    }
}
