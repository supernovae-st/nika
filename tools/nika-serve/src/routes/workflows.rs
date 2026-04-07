//! Workflow execution endpoints.
//!
//! - `GET  /v1/workflows`            -- List available workflows
//! - `GET  /v1/workflows/{name}/source` -- Raw YAML source
//! - `POST /v1/run`                  -- Submit a workflow for execution
//! - `GET  /v1/status/{id}`          -- Poll job status
//! - `POST /v1/cancel/{id}`          -- Cancel a running job (ERRATA-3)
//! - `GET  /v1/events/{id}`          -- SSE streaming (see `events.rs`)

use aide::transform::TransformOperation;
use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::Json;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use tracing::info;

use crate::error::ServeError;
use crate::state::AppState;
use crate::worker;

// ═══════════════════════════════════════════════════════════════════════════
// REQUEST / RESPONSE TYPES
// ═══════════════════════════════════════════════════════════════════════════

#[derive(Deserialize, JsonSchema)]
pub struct RunRequest {
    /// Workflow filename relative to the configured workflows directory.
    /// Example: `"my-pipeline.nika.yaml"`
    pub workflow: String,

    /// Optional workflow inputs (key-value pairs passed as `--input`).
    pub inputs: Option<Value>,

    /// Resume from a previous job's checkpoints. Tasks that completed in
    /// the source job are skipped, using their cached outputs.
    pub resume_from: Option<String>,

    /// Optional tags for job metadata (e.g. `{"env": "staging"}`).
    pub tags: Option<std::collections::HashMap<String, String>>,
}

#[derive(Serialize, JsonSchema)]
pub struct RunResponse {
    pub job_id: String,
    pub status: String,
}

#[derive(Serialize, JsonSchema)]
pub struct StatusResponse {
    pub job_id: String,
    pub status: String,
    pub workflow: String,
    pub created_at: String,
    pub started_at: Option<String>,
    pub completed_at: Option<String>,
    pub exit_code: Option<i32>,
    pub output: Option<String>,
    pub tags: Option<Value>,
}

/// Response for `GET /v1/jobs` — paginated job list.
#[derive(Serialize, JsonSchema)]
pub struct JobListResponse {
    pub jobs: Vec<StatusResponse>,
    /// Number of jobs in this response (page size, not total count).
    pub count: usize,
    pub limit: i64,
    pub offset: i64,
    /// True if more results exist beyond this page.
    pub has_more: bool,
}

/// Query parameters for `GET /v1/jobs`.
#[derive(Deserialize, JsonSchema)]
pub struct JobListQuery {
    pub state: Option<String>,
    pub workflow: Option<String>,
    /// Filter by tag: `tag=env:staging`
    pub tag: Option<String>,
    pub limit: Option<i64>,
    pub offset: Option<i64>,
}

/// Maximum number of jobs in a single batch submission.
const MAX_BATCH_SIZE: usize = 50;

// ═══════════════════════════════════════════════════════════════════════════
// HANDLERS
// ═══════════════════════════════════════════════════════════════════════════

/// `POST /v1/run` -- Submit a workflow for async execution.
///
/// Validates the workflow file exists (with path traversal protection),
/// creates a job in SQLite, spawns a subprocess worker.
pub async fn run_workflow(
    State(state): State<AppState>,
    principal: Option<axum::Extension<crate::token_store::Principal>>,
    Json(req): Json<RunRequest>,
) -> Result<Json<RunResponse>, ServeError> {
    // L3 RBAC: viewer cannot execute
    if let Some(axum::Extension(ref p)) = principal {
        if !p.can_execute() {
            return Err(ServeError::Forbidden(
                "viewer role cannot execute workflows".into(),
            ));
        }
        // L2 scope enforcement
        if !p.can_access(&req.workflow) {
            return Err(ServeError::Forbidden(format!(
                "token '{}' scope '{}' does not cover workflow '{}'",
                p.token_name, p.scope, req.workflow
            )));
        }
    }

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

    // Validate resume_from: UUID hex format, max 64 chars
    if let Some(ref resume_id) = req.resume_from {
        if resume_id.len() > 64
            || !resume_id
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '-')
        {
            return Err(ServeError::InvalidWorkflow(
                "resume_from must be a valid job ID (alphanumeric/hyphens, max 64 chars)".into(),
            ));
        }
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

    // Atomic check-and-increment via CAS loop (race-free, no DB queries).
    // SlotGuard ensures the counter is decremented if any code below panics
    // before the WorkerGuard (in the spawned task) takes over responsibility.
    let max_queued = state.config.max_concurrent * 3;
    try_acquire_job_slot(&state.active_jobs, max_queued)?;
    let mut slot_guard = SlotGuard::new(&state.active_jobs);
    crate::metrics::record_active_jobs(
        state.active_jobs.load(std::sync::atomic::Ordering::Relaxed),
    );

    // Generate job ID — full 128-bit UUID in simple (no-hyphen) format
    let job_id = uuid::Uuid::new_v4().simple().to_string();

    // Serialize tags to JSON string for storage
    let tags_json = req
        .tags
        .as_ref()
        .map(|t| serde_json::to_string(t).unwrap_or_default());

    // Persist job
    if let Err(e) = state
        .storage
        .create_job_with_tags(&job_id, &req.workflow, tags_json)
        .await
    {
        // SlotGuard will decrement on drop, no manual fetch_sub needed
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

    // Transfer counter responsibility to WorkerGuard (inside the spawned task).
    // Disarm the handler-scope guard so it doesn't double-decrement.
    slot_guard.disarm();

    Ok(Json(RunResponse {
        job_id,
        status: "pending".into(),
    }))
}

/// `GET /v1/status/{id}` -- Poll job status.
pub async fn get_status(
    State(state): State<AppState>,
    principal: Option<axum::Extension<crate::token_store::Principal>>,
    Path(id): Path<String>,
) -> Result<Json<StatusResponse>, ServeError> {
    let job = state
        .storage
        .get_job(&id)
        .await?
        .ok_or(ServeError::NotFound)?;

    // L2 scope enforcement
    if let Some(axum::Extension(ref p)) = principal {
        if !p.can_access(&job.workflow) {
            return Err(ServeError::Forbidden(format!(
                "token '{}' scope '{}' does not cover workflow '{}'",
                p.token_name, p.scope, job.workflow
            )));
        }
    }

    Ok(Json(job_to_status_response(job)))
}

/// Convert a Job to StatusResponse (shared by get_status, job list, cancel).
fn job_to_status_response(job: nika_storage::Job) -> StatusResponse {
    let tags = job.tags.as_ref().and_then(|t| serde_json::from_str(t).ok());
    StatusResponse {
        job_id: job.id,
        status: job.state.as_str().to_string(),
        workflow: job.workflow,
        created_at: job.created_at,
        started_at: job.started_at,
        completed_at: job.completed_at,
        exit_code: job.exit_code,
        output: job.output,
        tags,
    }
}

/// `POST /v1/cancel/{id}` -- Cancel a running job (ERRATA-3).
///
/// Aborts the worker task (which kills the subprocess via `kill_on_drop`)
/// and marks the job as cancelled.
pub async fn cancel_job(
    State(state): State<AppState>,
    principal: Option<axum::Extension<crate::token_store::Principal>>,
    Path(id): Path<String>,
) -> Result<Json<StatusResponse>, ServeError> {
    // L3 RBAC: viewer cannot cancel
    if let Some(axum::Extension(ref p)) = principal {
        if !p.can_execute() {
            return Err(ServeError::Forbidden(
                "viewer role cannot cancel jobs".into(),
            ));
        }
    }
    // Verify job exists
    let job = state
        .storage
        .get_job(&id)
        .await?
        .ok_or(ServeError::NotFound)?;

    // L2 scope enforcement
    if let Some(axum::Extension(ref p)) = principal {
        if !p.can_access(&job.workflow) {
            return Err(ServeError::Forbidden(format!(
                "token '{}' scope '{}' does not cover workflow '{}'",
                p.token_name, p.scope, job.workflow
            )));
        }
    }

    // Only pending/running jobs can be cancelled
    if !matches!(
        job.state,
        nika_storage::JobState::Pending | nika_storage::JobState::Running
    ) {
        // Return current state — job already finished
        return Ok(Json(job_to_status_response(job)));
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
    state
        .event_bus
        .publish(
            &id,
            crate::events::ServeEvent::Cancelled { job_id: id.clone() },
        )
        .await;
    state.event_bus.remove(&id).await;

    // Re-read job state to return complete StatusResponse
    let job = state
        .storage
        .get_job(&id)
        .await?
        .ok_or(ServeError::NotFound)?;

    Ok(Json(job_to_status_response(job)))
}

// ═══════════════════════════════════════════════════════════════════════════
// BATCH + JOB LIST
// ═══════════════════════════════════════════════════════════════════════════

/// `POST /v1/batch/run` -- Submit multiple workflows for execution.
///
/// Two-pass: validates ALL requests first, then submits. No partial failures.
pub async fn batch_run(
    State(state): State<AppState>,
    principal: Option<axum::Extension<crate::token_store::Principal>>,
    Json(requests): Json<Vec<RunRequest>>,
) -> Result<Json<Vec<RunResponse>>, ServeError> {
    let max_batch = std::env::var("NIKA_SERVE_BATCH_MAX")
        .ok()
        .and_then(|s| {
            let parsed = s.parse::<usize>();
            if parsed.is_err() {
                tracing::warn!(value = %s, "invalid NIKA_SERVE_BATCH_MAX, using default");
            }
            parsed.ok()
        })
        .unwrap_or(MAX_BATCH_SIZE);

    if requests.is_empty() {
        return Ok(Json(Vec::new()));
    }
    if requests.len() > max_batch {
        return Err(ServeError::InvalidWorkflow(format!(
            "batch too large: {} jobs (max {})",
            requests.len(),
            max_batch
        )));
    }

    // Pass 1: validate ALL requests before submitting any
    let canonical_base = state
        .config
        .workflows_dir
        .canonicalize()
        .map_err(|e| ServeError::Config(format!("workflows_dir canonicalize: {e}")))?;

    for (i, req) in requests.iter().enumerate() {
        validate_workflow_path(&req.workflow)
            .map_err(|e| ServeError::InvalidWorkflow(format!("batch[{}]: {}", i, e)))?;

        // L2 scope enforcement: reject out-of-scope workflows in pass 1
        // to prevent orphaned jobs from partial batch failure in pass 2
        if let Some(axum::Extension(ref p)) = principal {
            if !p.can_access(&req.workflow) {
                return Err(ServeError::Forbidden(format!(
                    "batch[{}]: token '{}' scope '{}' does not cover workflow '{}'",
                    i, p.token_name, p.scope, req.workflow
                )));
            }
        }

        let full_path = state.config.workflows_dir.join(&req.workflow);
        let canonical = full_path.canonicalize().map_err(|_| {
            ServeError::InvalidWorkflow(format!(
                "batch[{}]: workflow not found: {}",
                i, req.workflow
            ))
        })?;
        if !canonical.starts_with(&canonical_base) {
            return Err(ServeError::InvalidWorkflow(format!(
                "batch[{}]: path traversal rejected",
                i
            )));
        }
    }

    // Pass 2: submit all (validation already passed)
    // Queue capacity is enforced per-job by try_acquire_job_slot() CAS inside run_workflow.
    let mut responses = Vec::with_capacity(requests.len());
    for req in requests {
        let resp = run_workflow(State(state.clone()), principal.clone(), Json(req)).await?;
        responses.push(resp.0);
    }

    Ok(Json(responses))
}

/// Validate a tag key: alphanumeric + underscore, 1-128 chars.
fn validate_tag_key(key: &str) -> Result<(), ServeError> {
    if key.is_empty()
        || key.len() > 128
        || !key.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
    {
        return Err(ServeError::InvalidWorkflow(format!(
            "invalid tag key: '{}' (must be alphanumeric/underscore, 1-128 chars)",
            key
        )));
    }
    Ok(())
}

/// `GET /v1/jobs` -- List jobs with optional filtering.
pub async fn list_jobs(
    State(state): State<AppState>,
    principal: Option<axum::Extension<crate::token_store::Principal>>,
    Query(query): Query<JobListQuery>,
) -> Result<Json<JobListResponse>, ServeError> {
    let state_filter = query.state.as_deref().map(nika_storage::JobState::parse);

    let tag_filter = if let Some(ref t) = query.tag {
        let parts: Vec<&str> = t.splitn(2, ':').collect();
        if parts.len() == 2 {
            validate_tag_key(parts[0])?;
            Some((parts[0].to_string(), parts[1].to_string()))
        } else {
            return Err(ServeError::InvalidWorkflow(
                "tag filter must be key:value format (e.g. tag=env:staging)".into(),
            ));
        }
    } else {
        None
    };

    let limit = query.limit.unwrap_or(50).min(100);
    let offset = query.offset.unwrap_or(0);

    // When scope filtering is active, the DB doesn't know which rows pass the
    // filter, so we may need multiple fetches to fill the requested page.
    // Cap at 10 iterations (10 * page_size = 1000 rows max scanned).
    let need_scope_filter = principal
        .as_ref()
        .map(|axum::Extension(p)| p.scope != "*")
        .unwrap_or(false);

    let target = (limit + 1) as usize; // +1 to detect has_more
    let mut collected: Vec<nika_storage::Job> = Vec::new();
    let mut db_offset = offset;
    let page_size = limit + 1; // fetch slightly more than needed per round
    let max_rounds = if need_scope_filter { 10 } else { 1 };

    for _ in 0..max_rounds {
        let filter = nika_storage::JobFilter {
            state: state_filter.clone(),
            workflow: query.workflow.clone(),
            tag: tag_filter.clone(),
            limit: Some(page_size),
            offset: Some(db_offset),
        };

        let batch = state.storage.list_jobs_filtered(filter).await?;
        let batch_len = batch.len();

        if need_scope_filter {
            if let Some(axum::Extension(ref p)) = principal {
                collected.extend(batch.into_iter().filter(|j| p.can_access(&j.workflow)));
            }
        } else {
            collected.extend(batch);
        }

        // Stop if DB returned fewer rows than requested (no more data)
        // or we've collected enough results
        if (batch_len as i64) < page_size || collected.len() >= target {
            break;
        }
        db_offset += page_size;
    }

    let has_more = collected.len() > limit as usize;
    collected.truncate(limit as usize);
    let count = collected.len();

    let job_responses: Vec<StatusResponse> =
        collected.into_iter().map(job_to_status_response).collect();

    Ok(Json(JobListResponse {
        jobs: job_responses,
        count,
        limit,
        offset,
        has_more,
    }))
}

// ═══════════════════════════════════════════════════════════════════════════
// LIST WORKFLOWS
// ═══════════════════════════════════════════════════════════════════════════

#[derive(Serialize, JsonSchema)]
pub struct WorkflowInfo {
    /// Filename relative to workflows_dir (e.g. "translate-locale.nika.yaml")
    pub name: String,
    /// File size in bytes
    pub size: u64,
}

#[derive(Deserialize, JsonSchema)]
pub struct ListQuery {
    /// Max workflows to return. Default: all (no limit).
    pub limit: Option<usize>,
    /// Return workflows after this name (cursor for pagination).
    pub after: Option<String>,
}

#[derive(Serialize, JsonSchema)]
pub struct ListWorkflowsResponse {
    pub workflows: Vec<WorkflowInfo>,
    pub count: usize,
    /// Whether more results exist after the last item.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub has_more: Option<bool>,
}

/// `GET /v1/workflows` -- List available workflows.
///
/// Recursively scans `workflows_dir` for `.nika.yaml` files.
/// Returns relative paths sorted alphabetically.
pub async fn list_workflows(
    State(state): State<AppState>,
    principal: Option<axum::Extension<crate::token_store::Principal>>,
    Query(query): Query<ListQuery>,
) -> Result<Json<ListWorkflowsResponse>, ServeError> {
    let base = state
        .config
        .workflows_dir
        .canonicalize()
        .map_err(|e| ServeError::Config(format!("workflows_dir canonicalize: {e}")))?;

    let mut workflows = Vec::new();
    collect_workflows(&base, &base, &mut workflows).await?;
    workflows.sort_by(|a, b| a.name.cmp(&b.name));

    // L2 scope enforcement: filter out workflows outside token scope
    if let Some(axum::Extension(ref p)) = principal {
        workflows.retain(|w| p.can_access(&w.name));
    }

    // Apply cursor: skip everything up to and including `after`
    if let Some(ref after) = query.after {
        if let Some(pos) = workflows
            .iter()
            .position(|w| w.name.as_str() > after.as_str())
        {
            workflows = workflows.split_off(pos);
        } else {
            workflows.clear();
        }
    }

    // Apply limit
    let has_more = if let Some(limit) = query.limit {
        let has_more = workflows.len() > limit;
        workflows.truncate(limit);
        Some(has_more)
    } else {
        None
    };

    let count = workflows.len();
    Ok(Json(ListWorkflowsResponse {
        workflows,
        count,
        has_more,
    }))
}

/// `GET /v1/workflows/{name}/source` — Return raw YAML source.
///
/// Returns the workflow file contents as plain text.
/// Uses the same path validation as `run_workflow` (traversal protection).
pub async fn get_workflow_source(
    State(state): State<AppState>,
    principal: Option<axum::Extension<crate::token_store::Principal>>,
    Path(name): Path<String>,
) -> Result<Response, ServeError> {
    // L2 scope enforcement
    if let Some(axum::Extension(ref p)) = principal {
        if !p.can_access(&name) {
            return Err(ServeError::Forbidden(format!(
                "token '{}' scope '{}' does not cover workflow '{}'",
                p.token_name, p.scope, name
            )));
        }
    }

    validate_workflow_path(&name)?;

    let full_path = state.config.workflows_dir.join(&name);
    let canonical = full_path
        .canonicalize()
        .map_err(|_| ServeError::InvalidWorkflow(format!("workflow not found: {name}")))?;

    let canonical_base = state
        .config
        .workflows_dir
        .canonicalize()
        .map_err(|e| ServeError::Config(format!("workflows_dir canonicalize: {e}")))?;

    if !canonical.starts_with(&canonical_base) {
        return Err(ServeError::PathTraversal);
    }

    let content = tokio::fs::read_to_string(&canonical)
        .await
        .map_err(|_| ServeError::NotFound)?;

    Ok((
        StatusCode::OK,
        [("content-type", "text/plain; charset=utf-8")],
        content,
    )
        .into_response())
}

/// `POST /v1/reload` — Rescan workflows directory.
///
/// After `git pull` or adding new workflow files, call this endpoint
/// to confirm the serve sees the updated files. Returns the refreshed
/// workflow list (same format as `GET /v1/workflows`).
pub async fn reload_workflows(
    State(state): State<AppState>,
    principal: Option<axum::Extension<crate::token_store::Principal>>,
) -> Result<Json<ListWorkflowsResponse>, ServeError> {
    // L3 RBAC: admin only
    if let Some(axum::Extension(ref p)) = principal {
        if !p.can_admin() {
            return Err(ServeError::Forbidden(
                "admin role required for reload".into(),
            ));
        }
    }
    let base = state
        .config
        .workflows_dir
        .canonicalize()
        .map_err(|e| ServeError::Config(format!("workflows_dir canonicalize: {e}")))?;

    let mut workflows = Vec::new();
    collect_workflows(&base, &base, &mut workflows).await?;
    workflows.sort_by(|a, b| a.name.cmp(&b.name));

    // L2 scope enforcement: filter out workflows outside token scope
    if let Some(axum::Extension(ref p)) = principal {
        workflows.retain(|w| p.can_access(&w.name));
    }

    let count = workflows.len();
    tracing::info!(count, "workflows reloaded");
    Ok(Json(ListWorkflowsResponse {
        workflows,
        count,
        has_more: None,
    }))
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
// OPENAPI DOCS
// ═══════════════════════════════════════════════════════════════════════════

pub fn run_docs(op: TransformOperation) -> TransformOperation {
    op.id("submitWorkflow")
        .summary("Submit a workflow for async execution")
        .description(
            "Validates the workflow file, creates a job in SQLite, and spawns a worker.\n\n\
             ## Real-time events\n\n\
             After submission, subscribe to SSE at `GET /v1/events/{job_id}` with the same \
             Bearer token.\n\n\
             Event types: `started`, `task_start`, `task_complete`, `task_failed`, \
             `artifact_written`, `completed`, `failed`, `cancelled`.\n\n\
             Terminal events (`completed`, `failed`, `cancelled`) close the stream.",
        )
        .tag("jobs")
}

pub fn status_docs(op: TransformOperation) -> TransformOperation {
    op.id("getJobStatus")
        .summary("Poll job status")
        .description(
            "Returns the current state of a job (pending/running/completed/failed/cancelled).",
        )
        .tag("jobs")
}

pub fn cancel_docs(op: TransformOperation) -> TransformOperation {
    op.id("cancelJob")
        .summary("Cancel a running job")
        .description("Kills the worker subprocess and marks the job as cancelled.")
        .tag("jobs")
}

pub fn list_docs(op: TransformOperation) -> TransformOperation {
    op.id("listWorkflows")
        .summary("List available workflows")
        .description("Recursively scans the workflows directory for .nika.yaml files.")
        .tag("workflows")
}

pub fn source_docs(op: TransformOperation) -> TransformOperation {
    op.id("getWorkflowSource")
        .summary("Get workflow YAML source")
        .description("Returns the raw YAML content of a workflow file as plain text.")
        .tag("workflows")
}

pub fn reload_docs(op: TransformOperation) -> TransformOperation {
    op.id("reloadWorkflows")
        .summary("Reload workflows from disk")
        .description("Rescans the workflows directory and returns the refreshed list.")
        .tag("workflows")
}

pub fn batch_docs(op: TransformOperation) -> TransformOperation {
    op.id("batchRun")
        .summary("Submit multiple workflows for execution")
        .description(
            "Accepts a JSON array of RunRequests. Returns array of RunResponses.\n\
             Max batch size: 50 (configurable via NIKA_SERVE_BATCH_MAX).",
        )
        .tag("jobs")
}

pub fn jobs_list_docs(op: TransformOperation) -> TransformOperation {
    op.id("listJobs")
        .summary("List jobs with filtering")
        .description(
            "Returns paginated job list. Filter by state, workflow, or tag.\n\
             Tag format: `tag=key:value` (e.g. `tag=env:staging`).",
        )
        .tag("jobs")
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

/// RAII guard for the active_jobs counter.
///
/// Decrements on drop unless `disarm()` is called. This ensures the counter
/// is always decremented if the handler panics between `try_acquire_job_slot`
/// and the `WorkerGuard` taking over responsibility in the spawned task.
struct SlotGuard<'a> {
    counter: &'a std::sync::atomic::AtomicUsize,
    armed: bool,
}

impl<'a> SlotGuard<'a> {
    fn new(counter: &'a std::sync::atomic::AtomicUsize) -> Self {
        Self {
            counter,
            armed: true,
        }
    }

    fn disarm(&mut self) {
        self.armed = false;
    }
}

impl Drop for SlotGuard<'_> {
    fn drop(&mut self) {
        if self.armed {
            self.counter
                .fetch_sub(1, std::sync::atomic::Ordering::Relaxed);
        }
    }
}

/// Reject workflow paths that attempt directory traversal.
///
/// Checks both literal separators and percent-encoded variants to prevent
/// bypass via `%2e%2e` (..),  `%2f` (/), `%5c` (\). Matches the SDK's
/// `validate_path_segment` defense (nika-sdk/src/remote.rs).
fn validate_workflow_path(workflow: &str) -> Result<(), ServeError> {
    if workflow.contains('\0')
        || workflow.contains("..")
        || workflow.starts_with('/')
        || workflow.starts_with('\\')
    {
        return Err(ServeError::PathTraversal);
    }

    // Percent-encoded traversal: %2F (/), %5C (\), %2E%2E (..)
    let lower = workflow.to_ascii_lowercase();
    if lower.contains("%2f") || lower.contains("%5c") || lower.contains("%2e%2e") {
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
    fn rejects_percent_encoded_traversal() {
        // %2e%2e = ".." (directory traversal)
        assert!(validate_workflow_path("%2e%2e/secret.nika.yaml").is_err());
        assert!(validate_workflow_path("%2E%2E/secret.nika.yaml").is_err());
        // %2f = "/" (path separator)
        assert!(validate_workflow_path("foo%2fetc%2fpasswd.nika.yaml").is_err());
        assert!(validate_workflow_path("foo%2Fetc.nika.yaml").is_err());
        // %5c = "\" (Windows path separator)
        assert!(validate_workflow_path("foo%5cbar.nika.yaml").is_err());
        assert!(validate_workflow_path("foo%5Cbar.nika.yaml").is_err());
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
    fn source_validation_rejects_non_nika_yaml() {
        assert!(validate_workflow_path("readme.md").is_err());
        assert!(validate_workflow_path("config.yaml").is_err());
        assert!(validate_workflow_path("../secret.nika.yaml").is_err());
        assert!(validate_workflow_path("/absolute.nika.yaml").is_err());
        // Subdirectory paths are allowed
        assert!(validate_workflow_path("sub/valid.nika.yaml").is_ok());
        assert!(validate_workflow_path("deep/nested/flow.nika.yaml").is_ok());
    }

    #[test]
    fn rejects_null_bytes() {
        assert!(validate_workflow_path("evil\0.nika.yaml").is_err());
        assert!(validate_workflow_path("sub/\0path.nika.yaml").is_err());
    }

    #[tokio::test]
    async fn list_workflows_with_limit() {
        let dir = tempfile::TempDir::new().unwrap();
        for name in [
            "a.nika.yaml",
            "b.nika.yaml",
            "c.nika.yaml",
            "d.nika.yaml",
            "e.nika.yaml",
        ] {
            std::fs::write(dir.path().join(name), "schema: nika/workflow@0.12").unwrap();
        }
        let base = dir.path().canonicalize().unwrap();
        let mut workflows = Vec::new();
        collect_workflows(&base, &base, &mut workflows)
            .await
            .unwrap();
        workflows.sort_by(|a, b| a.name.cmp(&b.name));
        let has_more = workflows.len() > 2;
        workflows.truncate(2);
        assert_eq!(workflows.len(), 2);
        assert_eq!(workflows[0].name, "a.nika.yaml");
        assert!(has_more);
    }

    #[tokio::test]
    async fn list_workflows_with_cursor() {
        let dir = tempfile::TempDir::new().unwrap();
        for name in ["a.nika.yaml", "b.nika.yaml", "c.nika.yaml"] {
            std::fs::write(dir.path().join(name), "schema: nika/workflow@0.12").unwrap();
        }
        let base = dir.path().canonicalize().unwrap();
        let mut workflows = Vec::new();
        collect_workflows(&base, &base, &mut workflows)
            .await
            .unwrap();
        workflows.sort_by(|a, b| a.name.cmp(&b.name));
        let after = "a.nika.yaml";
        if let Some(pos) = workflows.iter().position(|w| w.name.as_str() > after) {
            workflows = workflows.split_off(pos);
        }
        assert_eq!(workflows.len(), 2);
        assert_eq!(workflows[0].name, "b.nika.yaml");
    }

    #[test]
    fn job_id_is_full_uuid_no_hyphens() {
        let id = uuid::Uuid::new_v4().simple().to_string();
        assert_eq!(id.len(), 32, "simple UUID should be 32 hex chars");
        assert!(!id.contains('-'), "simple UUID must not contain hyphens");
        assert!(id.chars().all(|c| c.is_ascii_hexdigit()));
    }

    // ── validate_tag_key ────────────────────────────────────────

    #[test]
    fn tag_key_valid_alphanumeric() {
        assert!(validate_tag_key("env").is_ok());
        assert!(validate_tag_key("team_name").is_ok());
        assert!(validate_tag_key("v2").is_ok());
    }

    #[test]
    fn tag_key_rejects_empty() {
        assert!(validate_tag_key("").is_err());
    }

    #[test]
    fn tag_key_rejects_special_chars() {
        assert!(validate_tag_key("env.prod").is_err());
        assert!(validate_tag_key("../etc").is_err());
        assert!(validate_tag_key("key=val").is_err());
        assert!(validate_tag_key("a b").is_err());
    }

    #[test]
    fn tag_key_rejects_too_long() {
        let long_key = "a".repeat(129);
        assert!(validate_tag_key(&long_key).is_err());
        // 128 is the limit
        let ok_key = "a".repeat(128);
        assert!(validate_tag_key(&ok_key).is_ok());
    }

    // ── batch size validation ───────────────────────────────────

    #[test]
    fn batch_max_size_default() {
        assert_eq!(MAX_BATCH_SIZE, 50);
    }
}
