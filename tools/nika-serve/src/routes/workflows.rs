//! Workflow execution endpoints.
//!
//! - `GET  /v1/workflows`     -- List available workflows
//! - `POST /v1/run`           -- Submit a workflow for execution
//! - `GET  /v1/status/{id}`   -- Poll job status
//! - `POST /v1/cancel/{id}`   -- Cancel a running job (ERRATA-3)
//! - `GET  /v1/events/{id}`   -- SSE streaming (see `events.rs`)
//!
//! TODO(v0.58): Add idempotency keys (Idempotency-Key header)

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

    /// Resume from a previous job's checkpoints. Tasks that completed in
    /// the source job are skipped, using their cached outputs.
    pub resume_from: Option<String>,
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

    // Validate inputs: must be object, keys alphanumeric+underscore, bounded size
    if let Some(inputs) = &req.inputs {
        // M5: Reject non-object inputs (arrays, strings, etc.)
        let obj = inputs
            .as_object()
            .ok_or_else(|| ServeError::InvalidWorkflow("inputs must be a JSON object".into()))?;
        for key in obj.keys() {
            if key.is_empty()
                || key.len() > 128 // M6: key length limit
                || !key.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
            {
                return Err(ServeError::InvalidWorkflow(format!(
                    "invalid input key: {key}"
                )));
            }
        }
        if obj.len() > 64 {
            return Err(ServeError::InvalidWorkflow(
                "too many inputs (max 64)".into(),
            ));
        }
    }

    // Atomic check-and-increment via CAS loop (race-free, no DB queries)
    let max_queued = state.config.max_concurrent * 3;
    try_acquire_job_slot(&state.active_jobs, max_queued)?;
    crate::metrics::record_active_jobs(
        state.active_jobs.load(std::sync::atomic::Ordering::Relaxed),
    );

    // Generate job ID — full 128-bit UUID in simple (no-hyphen) format
    let job_id = uuid::Uuid::new_v4().simple().to_string();

    // Persist job — decrement counter on failure (BUG-4)
    if let Err(e) = state.storage.create_job(&job_id, &req.workflow).await {
        state
            .active_jobs
            .fetch_sub(1, std::sync::atomic::Ordering::Relaxed);
        return Err(e.into());
    }

    info!(job_id = %job_id, workflow = %req.workflow, "job created");

    // Spawn worker + track handle (ERRATA-9)
    let wh = worker::spawn_worker(
        &state,
        job_id.clone(),
        req.workflow,
        req.inputs,
        req.resume_from,
    );
    state.workers.lock().await.insert(job_id.clone(), wh);

    Ok(Json(RunResponse {
        job_id,
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

    // Kill subprocess + abort the worker task (FIX-12: SIGTERM subprocess PID)
    if let Some(handle) = state.workers.lock().await.remove(&id) {
        let pid = handle.child_pid.load(std::sync::atomic::Ordering::Relaxed);
        if pid > 0 {
            #[cfg(unix)]
            {
                use nix::sys::signal::{kill, Signal};
                use nix::unistd::Pid;
                let _ = kill(Pid::from_raw(-(pid as i32)), Signal::SIGTERM);
            }
        }
        handle.join.abort();
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

    // Emit SSE event: cancelled
    let tx = state.event_bus.sender(&id).await;
    let _ = tx.send(crate::events::ServeEvent::Cancelled { job_id: id.clone() });
    state.event_bus.remove(&id).await;

    Ok(Json(json!({
        "job_id": id,
        "status": "cancelled",
    })))
}

// ═══════════════════════════════════════════════════════════════════════════
// LIST WORKFLOWS
// ═══════════════════════════════════════════════════════════════════════════

#[derive(Serialize)]
pub struct WorkflowInfo {
    /// Filename relative to workflows_dir (e.g. "translate-locale.nika.yaml")
    pub name: String,
    /// File size in bytes
    pub size: u64,
}

#[derive(Serialize)]
pub struct ListWorkflowsResponse {
    pub workflows: Vec<WorkflowInfo>,
    pub count: usize,
}

/// `GET /v1/workflows` -- List available workflows.
///
/// Recursively scans `workflows_dir` for `.nika.yaml` files.
/// Returns relative paths sorted alphabetically.
pub async fn list_workflows(
    State(state): State<AppState>,
) -> Result<Json<ListWorkflowsResponse>, ServeError> {
    let base = state
        .config
        .workflows_dir
        .canonicalize()
        .map_err(|e| ServeError::Config(format!("workflows_dir canonicalize: {e}")))?;

    let mut workflows = Vec::new();
    collect_workflows(&base, &base, &mut workflows).await?;
    workflows.sort_by(|a, b| a.name.cmp(&b.name));

    let count = workflows.len();
    Ok(Json(ListWorkflowsResponse { workflows, count }))
}

/// Recursively collect `.nika.yaml` files, skipping hidden directories.
async fn collect_workflows(
    base: &std::path::Path,
    dir: &std::path::Path,
    out: &mut Vec<WorkflowInfo>,
) -> Result<(), ServeError> {
    let mut entries = tokio::fs::read_dir(dir)
        .await
        .map_err(|e| ServeError::Internal(Box::new(e)))?;

    while let Some(entry) = entries
        .next_entry()
        .await
        .map_err(|e| ServeError::Internal(Box::new(e)))?
    {
        let path = entry.path();
        let ft = entry
            .file_type()
            .await
            .map_err(|e| ServeError::Internal(Box::new(e)))?;

        if ft.is_dir() {
            let name = entry.file_name();
            let name_str = name.to_string_lossy();
            if !name_str.starts_with('.') {
                Box::pin(collect_workflows(base, &path, out)).await?;
            }
        } else if ft.is_file() {
            let name = entry.file_name();
            let name_str = name.to_string_lossy();
            if name_str.ends_with(".nika.yaml") {
                let relative = path
                    .strip_prefix(base)
                    .unwrap_or(&path)
                    .to_string_lossy()
                    .to_string();

                let metadata = tokio::fs::metadata(&path)
                    .await
                    .map_err(|e| ServeError::Internal(Box::new(e)))?;

                out.push(WorkflowInfo {
                    name: relative,
                    size: metadata.len(),
                });
            }
        }
    }
    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════
// VALIDATION
// ═══════════════════════════════════════════════════════════════════════════

/// Atomic check-and-increment for job queue depth.
///
/// Uses a CAS loop to guarantee that `active_jobs` never exceeds `max_queued`,
/// even under concurrent access. Returns `Ok(())` if a slot was acquired,
/// or `Err(ServeError::QueueFull)` if the queue is full.
fn try_acquire_job_slot(
    active_jobs: &std::sync::atomic::AtomicUsize,
    max_queued: usize,
) -> Result<(), ServeError> {
    use std::sync::atomic::Ordering;
    loop {
        let current = active_jobs.load(Ordering::Acquire);
        if current >= max_queued {
            return Err(ServeError::QueueFull(current));
        }
        match active_jobs.compare_exchange(
            current,
            current + 1,
            Ordering::AcqRel,
            Ordering::Acquire,
        ) {
            Ok(_) => return Ok(()),
            Err(_) => continue, // Another thread won the race, retry
        }
    }
}

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

    /// Verify that the CAS-based queue check never exceeds max_queued
    /// under high contention (100 threads racing to acquire a slot).
    #[test]
    fn cas_queue_check_never_exceeds_max() {
        use std::sync::atomic::{AtomicUsize, Ordering};
        use std::sync::Arc;

        let active_jobs = Arc::new(AtomicUsize::new(0));
        let max_queued: usize = 3;
        let num_threads = 100;
        let accepted = Arc::new(AtomicUsize::new(0));

        let mut handles = Vec::new();
        for _ in 0..num_threads {
            let jobs = Arc::clone(&active_jobs);
            let acc = Arc::clone(&accepted);
            handles.push(std::thread::spawn(move || {
                if super::try_acquire_job_slot(&jobs, max_queued).is_ok() {
                    acc.fetch_add(1, Ordering::Relaxed);
                }
            }));
        }

        for h in handles {
            h.join().unwrap();
        }

        let final_accepted = accepted.load(Ordering::SeqCst);
        let final_count = active_jobs.load(Ordering::SeqCst);

        assert_eq!(
            final_accepted, max_queued,
            "exactly max_queued={max_queued} should be accepted, got {final_accepted}"
        );
        assert_eq!(
            final_count, max_queued,
            "active_jobs counter must equal max_queued={max_queued}, got {final_count}"
        );
    }

    #[tokio::test]
    async fn list_workflows_finds_nika_yaml() {
        let dir = tempfile::TempDir::new().unwrap();
        std::fs::write(
            dir.path().join("hello.nika.yaml"),
            "schema: nika/workflow@0.12",
        )
        .unwrap();
        std::fs::write(
            dir.path().join("test.nika.yaml"),
            "schema: nika/workflow@0.12",
        )
        .unwrap();
        std::fs::write(dir.path().join("not-a-workflow.yaml"), "data: 1").unwrap();
        std::fs::create_dir_all(dir.path().join("sub")).unwrap();
        std::fs::write(
            dir.path().join("sub/nested.nika.yaml"),
            "schema: nika/workflow@0.12",
        )
        .unwrap();

        let base = dir.path().canonicalize().unwrap();
        let mut result = Vec::new();
        collect_workflows(&base, &base, &mut result).await.unwrap();
        result.sort_by(|a, b| a.name.cmp(&b.name));

        assert_eq!(result.len(), 3);
        assert_eq!(result[0].name, "hello.nika.yaml");
        assert_eq!(result[1].name, "sub/nested.nika.yaml");
        assert_eq!(result[2].name, "test.nika.yaml");
        assert!(result.iter().all(|w| w.size > 0));
    }

    #[tokio::test]
    async fn list_workflows_skips_hidden_dirs() {
        let dir = tempfile::TempDir::new().unwrap();
        std::fs::write(
            dir.path().join("visible.nika.yaml"),
            "schema: nika/workflow@0.12",
        )
        .unwrap();
        std::fs::create_dir_all(dir.path().join(".nika")).unwrap();
        std::fs::write(
            dir.path().join(".nika/hidden.nika.yaml"),
            "schema: nika/workflow@0.12",
        )
        .unwrap();
        std::fs::create_dir_all(dir.path().join(".git")).unwrap();
        std::fs::write(
            dir.path().join(".git/hooks.nika.yaml"),
            "schema: nika/workflow@0.12",
        )
        .unwrap();

        let base = dir.path().canonicalize().unwrap();
        let mut result = Vec::new();
        collect_workflows(&base, &base, &mut result).await.unwrap();

        assert_eq!(result.len(), 1);
        assert_eq!(result[0].name, "visible.nika.yaml");
    }

    #[test]
    fn job_id_is_full_uuid_no_hyphens() {
        let id = uuid::Uuid::new_v4().simple().to_string();
        assert_eq!(id.len(), 32, "simple UUID should be 32 hex chars");
        assert!(!id.contains('-'), "simple UUID must not contain hyphens");
        assert!(id.chars().all(|c| c.is_ascii_hexdigit()));
    }
}
