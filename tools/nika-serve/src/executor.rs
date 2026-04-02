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

/// Context provided to an executor for a single job execution.
pub struct ExecutionContext {
    pub config: Arc<ServeConfig>,
    pub shutdown_rx: tokio::sync::watch::Receiver<bool>,
    /// For subprocess executor: stores the child PID for cancel (SIGTERM).
    /// Embedded executor ignores this.
    pub child_pid: Arc<AtomicU32>,
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
    /// Execute a single workflow and return its output on success.
    pub async fn execute(
        &self,
        workflow: &str,
        inputs: Option<&serde_json::Value>,
        ctx: &mut ExecutionContext,
    ) -> Result<String, String> {
        match self {
            Self::Subprocess => {
                crate::worker::run_subprocess(
                    &ctx.config,
                    workflow,
                    inputs,
                    &ctx.child_pid,
                    &mut ctx.shutdown_rx,
                )
                .await
            }
            Self::Embedded => {
                run_embedded(&ctx.config, workflow, inputs, &mut ctx.shutdown_rx).await
            }
        }
    }
}

/// Execute a workflow in-process using nika-engine Runner.
///
/// Parses the workflow YAML, creates a Runner, and executes within the
/// current process. Supports graceful cancellation via the shutdown signal.
async fn run_embedded(
    config: &ServeConfig,
    workflow: &str,
    inputs: Option<&serde_json::Value>,
    shutdown_rx: &mut tokio::sync::watch::Receiver<bool>,
) -> Result<String, String> {
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

    // Create Runner
    let cancel_token = tokio_util::sync::CancellationToken::new();
    let cancel_clone = cancel_token.clone();

    let mut runner = nika_engine::runtime::Runner::new(analyzed)
        .map_err(|e| format!("runner init error: {e}"))?
        .quiet()
        .with_base_path(base_path)
        .with_cancel_token(cancel_token);

    let timeout_secs = config.job_timeout_secs;
    let timeout = std::time::Duration::from_secs(timeout_secs);

    // BUG-9: Spawn runner in a separate task for panic isolation.
    // With panic="unwind", panics in the workflow are caught at the task
    // boundary and returned as JoinError instead of crashing the server.
    let handle = tokio::spawn(async move {
        tokio::time::timeout(timeout, runner.run()).await
    });

    tokio::select! {
        result = handle => {
            match result {
                Ok(Ok(Ok(output))) => Ok(output),
                Ok(Ok(Err(e))) => Err(format!("workflow failed: {e}")),
                Ok(Err(_)) => {
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
        });

        let ctx = ExecutionContext {
            config,
            shutdown_rx: rx,
            child_pid: Arc::new(AtomicU32::new(0)),
        };

        assert_eq!(ctx.child_pid.load(std::sync::atomic::Ordering::Relaxed), 0);
    }
}
