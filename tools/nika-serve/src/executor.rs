//! Workflow execution abstraction.
//!
//! Defines the execution backend for `nika serve`, allowing runtime selection
//! between different execution strategies:
//!
//! - [`SubprocessExecutor`]: V1 — spawns `nika run <workflow>` as a child process.
//! - `EmbeddedExecutor` (v0.57): V2 — runs the workflow in-process via nika-engine Runner.

use std::sync::atomic::AtomicU32;
use std::sync::Arc;

use crate::config::ServeConfig;

/// Context provided to an executor for a single job execution.
pub struct ExecutionContext {
    pub config: Arc<ServeConfig>,
    pub shutdown_rx: tokio::sync::watch::Receiver<bool>,
    /// For subprocess executor: stores the child PID for cancel (SIGTERM).
    /// Embedded executor can ignore this (set to 0).
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
    fn execution_context_construction() {
        let (_tx, rx) = tokio::sync::watch::channel(false);
        let config = Arc::new(ServeConfig {
            bind: "127.0.0.1:0".parse().unwrap(),
            workflows_dir: std::path::PathBuf::from("/tmp"),
            max_concurrent: 4,
            job_timeout_secs: 60,
            max_output_bytes: 1024,
            db_path: std::path::PathBuf::from(":memory:"),
            auth_token: "test-token-1234567".into(),
            cors_origin: None,
            executor_mode: crate::config::ExecutorMode::Subprocess,
        });

        let ctx = ExecutionContext {
            config,
            shutdown_rx: rx,
            child_pid: Arc::new(AtomicU32::new(0)),
        };

        assert_eq!(ctx.child_pid.load(std::sync::atomic::Ordering::Relaxed), 0);
    }
}
