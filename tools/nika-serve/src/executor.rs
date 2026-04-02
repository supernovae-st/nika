//! Workflow execution abstraction.
//!
//! Defines the execution backend for `nika serve`, allowing runtime selection
//! between different execution strategies:
//!
//! - `Subprocess`: V1 — spawns `nika run <workflow>` as a child process.
//! - `Embedded`: V2 — runs the workflow in-process via nika-engine Runner.

use std::sync::atomic::AtomicU32;
use std::sync::Arc;

use crate::config::ServeConfig;

/// Metadata for a single artifact produced by a workflow execution.
#[derive(Debug, Clone, serde::Serialize)]
pub struct ArtifactInfo {
    pub task_id: String,
    pub name: String,
    pub path: String,
    pub size: u64,
    pub format: String,
    pub checksum: Option<String>,
}

/// Result of a workflow execution, including output and any produced artifacts.
pub struct ExecutionResult {
    pub output: String,
    pub artifacts: Vec<ArtifactInfo>,
}

/// Context provided to an executor for a single job execution.
pub struct ExecutionContext {
    pub config: Arc<ServeConfig>,
    pub shutdown_rx: tokio::sync::watch::Receiver<bool>,
    /// For subprocess executor: stores the child PID for cancel (SIGTERM).
    /// Embedded executor ignores this.
    pub child_pid: Arc<AtomicU32>,
    /// Job ID for event forwarding.
    pub job_id: String,
    /// SSE event sender for real-time task-level events (embedded executor only).
    pub event_tx: Option<tokio::sync::broadcast::Sender<crate::events::ServeEvent>>,
    /// Storage handle for checkpoint persistence during execution.
    pub storage: Option<nika_storage::Storage>,
    /// Job ID to resume from (load checkpoints from this source job).
    pub resume_from: Option<String>,
}

/// Execution backend selector.
///
/// Uses an enum instead of `dyn Trait` to avoid async trait object-safety
/// issues while keeping runtime dispatch.
#[derive(Clone)]
pub enum Executor {
    /// V1: spawn `nika run <workflow>` as a child process.
    Subprocess,
    /// V2: run workflow in-process via nika-engine Runner.
    Embedded,
}

impl Executor {
    /// Execute a single workflow and return its output + artifacts on success.
    pub async fn execute(
        &self,
        workflow: &str,
        inputs: Option<&serde_json::Value>,
        ctx: &mut ExecutionContext,
    ) -> Result<ExecutionResult, String> {
        match self {
            Self::Subprocess => {
                let output = crate::worker::run_subprocess(
                    &ctx.config,
                    workflow,
                    inputs,
                    &ctx.child_pid,
                    &mut ctx.shutdown_rx,
                )
                .await?;
                // Subprocess mode: no artifact metadata available
                Ok(ExecutionResult {
                    output,
                    artifacts: vec![],
                })
            }
            Self::Embedded => {
                run_embedded(
                    &ctx.config,
                    workflow,
                    inputs,
                    &mut ctx.shutdown_rx,
                    &ctx.job_id,
                    ctx.event_tx.as_ref(),
                    ctx.storage.as_ref(),
                    ctx.resume_from.as_deref(),
                )
                .await
            }
        }
    }
}

/// Extract artifact metadata from runner event log after execution.
fn extract_artifacts(runner: &nika_engine::runtime::Runner) -> Vec<ArtifactInfo> {
    runner
        .event_log()
        .events()
        .into_iter()
        .filter_map(|event| {
            if let nika_event::EventKind::ArtifactWritten {
                task_id,
                path,
                size,
                format,
                checksum,
            } = event.kind
            {
                let name = std::path::Path::new(&path)
                    .file_name()
                    .map(|n| n.to_string_lossy().to_string())
                    .unwrap_or_else(|| path.clone());
                Some(ArtifactInfo {
                    task_id: task_id.to_string(),
                    name,
                    path,
                    size,
                    format,
                    checksum,
                })
            } else {
                None
            }
        })
        .collect()
}

/// Execute a workflow in-process using nika-engine Runner.
///
/// Parses the workflow YAML, creates a Runner, and executes within the
/// current process. Supports graceful cancellation via the shutdown signal.
/// If `event_tx` is provided, forwards task-level events to SSE clients.
#[allow(clippy::too_many_arguments)]
async fn run_embedded(
    config: &ServeConfig,
    workflow: &str,
    inputs: Option<&serde_json::Value>,
    shutdown_rx: &mut tokio::sync::watch::Receiver<bool>,
    job_id: &str,
    event_tx: Option<&tokio::sync::broadcast::Sender<crate::events::ServeEvent>>,
    storage: Option<&nika_storage::Storage>,
    resume_from: Option<&str>,
) -> Result<ExecutionResult, String> {
    let workflow_path = config.workflows_dir.join(workflow);

    // Read workflow file
    let yaml = tokio::fs::read_to_string(&workflow_path)
        .await
        .map_err(|e| format!("failed to read workflow: {e}"))?;

    let base_path = workflow_path
        .parent()
        .unwrap_or(std::path::Path::new("."))
        .to_path_buf();

    // Parse workflow (Phase 1 + Phase 2)
    let mut analyzed = nika_engine::ast::parse_analyzed_with_includes(&yaml, &base_path)
        .map_err(|e| format!("workflow parse error: {e}"))?;

    // Apply inputs (override workflow defaults)
    if let Some(obj) = inputs.and_then(|v| v.as_object()) {
        for (key, value) in obj {
            analyzed.inputs.insert(key.clone(), value.clone());
        }
    }

    // Create Runner with broadcast-enabled event log for SSE forwarding
    let cancel_token = tokio_util::sync::CancellationToken::new();
    let cancel_clone = cancel_token.clone();

    let (event_log, engine_event_rx) = nika_event::EventLog::new_with_broadcast();

    let mut runner = nika_engine::runtime::Runner::with_event_log(analyzed, event_log)
        .map_err(|e| format!("runner init error: {e}"))?
        .quiet()
        .with_base_path(base_path)
        .with_cancel_token(cancel_token);

    // Wire project root + working_dir mode from nika.toml so exec cwd,
    // nika:read security boundary, and from_example paths resolve correctly.
    if let Some(ref root) = config.project_root {
        runner = runner.with_project_root(root.clone());
    }
    if let Some(ref wd) = config.working_dir_mode {
        runner = runner.with_working_dir_mode(wd.clone());
    }

    // Resume: load checkpoints from source job and inject as pre-computed results.
    // The Runner's DAG automatically skips tasks that already have outputs.
    if let (Some(source_job), Some(storage)) = (resume_from, storage) {
        let checkpoints = storage
            .load_checkpoints(source_job)
            .await
            .map_err(|e| format!("failed to load checkpoints: {e}"))?;
        let count = checkpoints.len();
        for cp in checkpoints {
            if let Ok(value) = serde_json::from_str::<serde_json::Value>(&cp.output) {
                runner = runner.with_initial_context(&cp.task_id, value);
            }
        }
        if count > 0 {
            tracing::info!(
                source_job,
                count,
                "restored checkpoints — skipping completed tasks"
            );
        }
    }

    // Spawn event forwarder task: reads engine events, sends typed SSE events,
    // and saves checkpoints for completed tasks.
    if let Some(tx) = event_tx {
        let tx = tx.clone();
        let jid = job_id.to_string();
        let storage_clone = storage.cloned();
        spawn_event_forwarder(engine_event_rx, tx, jid, storage_clone);
    }

    let timeout_secs = config.job_timeout_secs;
    let timeout = std::time::Duration::from_secs(timeout_secs);

    // BUG-9: Spawn runner in a separate task for panic isolation.
    // With panic="unwind", panics in the workflow are caught at the task
    // boundary and returned as JoinError instead of crashing the server.
    // After execution, extract artifact metadata from the event log.
    let handle = tokio::spawn(async move {
        let run_result = tokio::time::timeout(timeout, runner.run()).await;
        let artifacts = extract_artifacts(&runner);
        (run_result, artifacts)
    });

    tokio::select! {
        result = handle => {
            match result {
                Ok((Ok(Ok(output)), artifacts)) => Ok(ExecutionResult { output, artifacts }),
                Ok((Ok(Err(e)), _)) => Err(format!("workflow failed: {e}")),
                Ok((Err(_), _)) => {
                    cancel_clone.cancel();
                    Err(format!("timeout after {timeout_secs}s"))
                }
                Err(join_err) => {
                    cancel_clone.cancel();
                    if join_err.is_panic() {
                        tracing::error!("workflow panicked — isolated by task boundary");
                        Err("workflow panicked".into())
                    } else {
                        Err(format!("task cancelled: {join_err}"))
                    }
                }
            }
        }
        _ = shutdown_rx.changed() => {
            cancel_clone.cancel();
            Err("server shutting down".into())
        }
    }
}

/// Spawn a background task that forwards engine events to SSE event bus
/// and saves checkpoints for completed tasks.
///
/// Filters for task-level events (start, complete, fail, artifact) and
/// converts them to typed ServeEvents for real-time client streaming.
/// When storage is available, TaskCompleted events are persisted as
/// checkpoints for resume functionality.
fn spawn_event_forwarder(
    mut rx: tokio::sync::broadcast::Receiver<nika_event::Event>,
    tx: tokio::sync::broadcast::Sender<crate::events::ServeEvent>,
    job_id: String,
    storage: Option<nika_storage::Storage>,
) {
    tokio::spawn(async move {
        loop {
            match rx.recv().await {
                Ok(event) => {
                    let serve_event = match &event.kind {
                        nika_event::EventKind::TaskStarted { task_id, verb, .. } => {
                            Some(crate::events::ServeEvent::TaskStart {
                                job_id: job_id.clone(),
                                task_id: task_id.to_string(),
                                verb: verb.to_string(),
                            })
                        }
                        nika_event::EventKind::TaskCompleted {
                            task_id,
                            output,
                            duration_ms,
                        } => {
                            // Save checkpoint for resume
                            if let Some(ref storage) = storage {
                                if let Ok(json) = serde_json::to_string(output.as_ref()) {
                                    let _ = storage.save_checkpoint(&job_id, task_id, &json).await;
                                }
                            }
                            Some(crate::events::ServeEvent::TaskComplete {
                                job_id: job_id.clone(),
                                task_id: task_id.to_string(),
                                duration_ms: *duration_ms,
                            })
                        }
                        nika_event::EventKind::TaskFailed {
                            task_id,
                            error,
                            duration_ms,
                            ..
                        } => Some(crate::events::ServeEvent::TaskFailed {
                            job_id: job_id.clone(),
                            task_id: task_id.to_string(),
                            error: error.clone(),
                            duration_ms: *duration_ms,
                        }),
                        nika_event::EventKind::ArtifactWritten {
                            task_id,
                            path,
                            size,
                            ..
                        } => Some(crate::events::ServeEvent::ArtifactWritten {
                            job_id: job_id.clone(),
                            task_id: task_id.to_string(),
                            path: path.clone(),
                            size: *size,
                        }),
                        _ => None, // Skip non-task events
                    };
                    if let Some(ev) = serve_event {
                        // Best-effort: ignore send failures (no subscribers)
                        let _ = tx.send(ev);
                    }
                }
                Err(tokio::sync::broadcast::error::RecvError::Lagged(n)) => {
                    tracing::debug!(skipped = n, "event forwarder lagged");
                    continue;
                }
                Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::AtomicU32;

    #[test]
    fn executor_is_send_sync() {
        fn assert_send_sync<T: Send + Sync>() {}
        assert_send_sync::<Executor>();
    }

    #[test]
    fn executor_embedded_variant_exists() {
        let _exec = Executor::Embedded;
    }

    #[test]
    fn execution_result_has_artifacts() {
        let result = ExecutionResult {
            output: "hello".into(),
            artifacts: vec![ArtifactInfo {
                task_id: "t1".into(),
                name: "report.md".into(),
                path: "/tmp/report.md".into(),
                size: 1024,
                format: "markdown".into(),
                checksum: Some("abc123".into()),
            }],
        };
        assert_eq!(result.artifacts.len(), 1);
        assert_eq!(result.artifacts[0].name, "report.md");
    }

    #[test]
    fn execution_context_construction() {
        let (_tx, rx) = tokio::sync::watch::channel(false);
        let config = Arc::new(ServeConfig {
            bind: "127.0.0.1:0".parse().unwrap(),
            workflows_dir: std::path::PathBuf::from("/tmp"),
            max_concurrent: 4,
            job_timeout_secs: 60,
            max_output_bytes: 1024,
            db_path: std::path::PathBuf::from(":memory:"),
            auth_token: "test-token-1234567890abcdef1234567".into(),
            cors_origin: None,
            executor_mode: crate::config::ExecutorMode::Embedded,
            rate_per_second: 10,
            rate_burst: 30,
            gc_retention_secs: 7 * 24 * 3600,
            gc_interval_secs: 3600,
            project_root: None,
            working_dir_mode: None,
        });

        let ctx = ExecutionContext {
            config,
            shutdown_rx: rx,
            child_pid: Arc::new(AtomicU32::new(0)),
            job_id: "test-job".into(),
            event_tx: None,
            storage: None,
            resume_from: None,
        };

        assert_eq!(ctx.child_pid.load(std::sync::atomic::Ordering::Relaxed), 0);
    }
}
