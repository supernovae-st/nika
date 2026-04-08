// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! nika_run - Execute nested workflow with timeout and depth protection.
//!
//! # Parameters
//!
//! ```json
//! {
//!   "workflow": "path/to/workflow.nika.yaml",  // Path to workflow file
//!   "context": { ... },                         // Context to pass (optional)
//!   "timeout_secs": 300,                        // Execution timeout (default: 300s)
//!   "max_depth": 3                              // Max recursion depth (default: 3, max: 10)
//! }
//! ```
//!
//! # Returns
//!
//! ```json
//! {
//!   "executed": true,
//!   "workflow": "path/to/workflow.nika.yaml",
//!   "output": { ... },
//!   "duration_ms": 1234
//! }
//! ```
//!
//! # Security
//!
//! - Path canonicalization prevents directory traversal attacks
//! - Depth limiting prevents infinite recursion (max: 10)
//! - Timeout prevents runaway workflows
//! - task_local! depth tracking prevents race conditions between concurrent workflows

use super::BuiltinTool;
use crate::ast::parse_analyzed;
use crate::error::NikaError;
use crate::runtime::Runner;
use nika_core::trust::TrustLevel;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::cell::Cell;
use std::future::Future;
use std::path::{Path, PathBuf};
use std::pin::Pin;
use std::sync::Arc;
use std::time::{Duration, Instant};

tokio::task_local! {
    /// Workflow nesting depth for the current execution context.
    /// Uses task_local! for proper isolation between concurrent workflows.
    /// Replaces global AtomicU32 which had race conditions.
    pub(crate) static WORKFLOW_DEPTH: Cell<u32>;

    /// ID of the task currently dispatching a builtin tool. Set by the
    /// runner before each `BuiltinTool::call` so the called tool can
    /// emit security errors that reference the calling task.
    ///
    /// `None` outside a runner-driven dispatch (e.g. CLI `nika invoke`).
    pub(crate) static CURRENT_TASK_ID: Option<Arc<str>>;

    /// Trust level of the calling task. Set by the runner before each
    /// builtin dispatch. Used by Item 4 (`nika:run` ceiling), Item 3b
    /// (path recon block), and downstream sprints.
    ///
    /// Defaults to `Trusted` outside a runner context.
    pub(crate) static CURRENT_TASK_TRUST: TrustLevel;

    /// Whether the calling task has `trust: elevated`. Set by the runner
    /// before each builtin dispatch.
    pub(crate) static CURRENT_TASK_ELEVATED: bool;

    /// Canonical workflow file paths in the parent chain — for `nika:run`
    /// cycle detection (Item 4). Each `nika:run` push its own canonical
    /// path before scoping the nested invocation.
    pub(crate) static PARENT_CHAIN: Vec<PathBuf>;
}

/// Get current workflow depth, returns 0 if not in a workflow context.
fn current_depth() -> u32 {
    WORKFLOW_DEPTH.try_with(|d| d.get()).unwrap_or(0)
}

/// Get the calling task ID, or `None` outside a runner-driven dispatch.
///
/// Wired into Item 3b/4 in subsequent Sprint 2 commits.
#[allow(dead_code)]
#[inline]
pub(crate) fn current_task_id() -> Option<Arc<str>> {
    CURRENT_TASK_ID.try_with(|id| id.clone()).unwrap_or(None)
}

/// Get the calling task's trust level, or `Trusted` outside a runner-driven
/// dispatch (which only happens in standalone tool invocations like
/// `nika invoke nika:read foo.md`).
///
/// Wired into Item 3b/4 in subsequent Sprint 2 commits.
#[allow(dead_code)]
#[inline]
pub(crate) fn current_task_trust() -> TrustLevel {
    CURRENT_TASK_TRUST
        .try_with(|t| *t)
        .unwrap_or(TrustLevel::Trusted)
}

/// Whether the calling task has `trust: elevated`. Defaults to `false`
/// (conservative — never auto-elevate when context is missing).
///
/// Used by run_infer for the spotlight bypass + Item 3b/4 path-recon.
#[inline]
pub(crate) fn current_task_elevated() -> bool {
    CURRENT_TASK_ELEVATED.try_with(|e| *e).unwrap_or(false)
}

/// Snapshot of the parent workflow chain for `nika:run` cycle detection.
/// Returns an empty vec outside a nested-run context.
///
/// Wired into Item 4 in a subsequent Sprint 2 commit.
#[allow(dead_code)]
#[inline]
pub(crate) fn current_parent_chain() -> Vec<PathBuf> {
    PARENT_CHAIN.try_with(|c| c.clone()).unwrap_or_default()
}

/// Maximum allowed recursion depth for nested workflows.
const MAX_ALLOWED_DEPTH: u32 = 10;

/// Maximum allowed timeout in seconds.
const MAX_TIMEOUT_SECS: u64 = 3600;

/// Parameters for nika_run tool.
/// Includes timeout_secs, max_depth for production safety.
#[derive(Debug, Clone, Deserialize)]
pub struct RunParams {
    /// Path to the workflow file to execute.
    /// Required when yaml_content is not provided.
    #[serde(default)]
    pub workflow: String,
    /// Inline YAML workflow content (alternative to file path).
    /// When provided, `workflow` is used only as a label.
    #[serde(default)]
    pub yaml_content: Option<String>,
    /// Context as JSON string (for OpenAI strict mode).
    #[serde(default)]
    pub context_json: Option<String>,
    /// Context to pass to the nested workflow (optional).
    #[serde(default)]
    pub context: Option<Value>,
    /// Execution timeout in seconds (default: 300).
    #[serde(default = "default_timeout")]
    pub timeout_secs: u64,
    /// Maximum recursion depth (default: 3, max: 10).
    #[serde(default = "default_max_depth")]
    pub max_depth: u32,
}

fn default_timeout() -> u64 {
    300
}

fn default_max_depth() -> u32 {
    3
}

impl RunParams {
    /// Get the context as a JSON Value, parsing from context_json if needed.
    pub fn get_context(&self) -> Result<Option<Value>, NikaError> {
        if let Some(ref json_str) = self.context_json {
            let value =
                serde_json::from_str(json_str).map_err(|e| NikaError::BuiltinInvalidParams {
                    tool: "nika:run".into(),
                    reason: format!("Invalid context_json: {}", e),
                })?;
            Ok(Some(value))
        } else {
            Ok(self.context.clone())
        }
    }
}

/// Response from nika_run tool.
#[derive(Debug, Clone, Serialize)]
pub struct RunResponse {
    /// Whether the workflow was executed.
    pub executed: bool,
    /// Path to the workflow that was executed.
    pub workflow: String,
    /// Output from the nested workflow.
    pub output: Value,
    /// Execution duration in milliseconds.
    pub duration_ms: u64,
    /// Depth at which this workflow was executed.
    pub depth: u32,
}

/// nika_run builtin tool.
///
/// Executes a nested workflow and returns its output.
/// Useful for workflow composition and modular design.
///
/// Features:
/// - Timeout protection (default: 300s)
/// - Depth limiting to prevent infinite recursion (max: 10)
/// - Path canonicalization for security
#[derive(Default)]
pub struct RunTool;

impl BuiltinTool for RunTool {
    fn name(&self) -> &'static str {
        "run"
    }

    fn description(&self) -> &'static str {
        "Execute nested workflow and return its output"
    }

    fn parameters_schema(&self) -> serde_json::Value {
        // Includes timeout_secs and max_depth for production safety
        serde_json::json!({
            "type": "object",
            "properties": {
                "workflow": {
                    "type": "string",
                    "description": "Path to the workflow file to execute (or label when using yaml_content)"
                },
                "yaml_content": {
                    "type": "string",
                    "description": "Inline YAML workflow content (alternative to file path)"
                },
                "context_json": {
                    "type": "string",
                    "description": "Context as JSON string (for OpenAI: '{\"key\": \"value\"}')"
                },
                "timeout_secs": {
                    "type": "integer",
                    "description": "Execution timeout in seconds (default: 300, max: 3600)"
                },
                "max_depth": {
                    "type": "integer",
                    "description": "Maximum recursion depth (default: 3, max: 10)"
                }
            },
            "required": [],
            "additionalProperties": false
        })
    }

    fn call<'a>(
        &'a self,
        args: String,
    ) -> Pin<Box<dyn Future<Output = Result<String, NikaError>> + Send + 'a>> {
        Box::pin(async move {
            let start = Instant::now();

            // Parse parameters
            let params: RunParams =
                serde_json::from_str(&args).map_err(|e| NikaError::BuiltinInvalidParams {
                    tool: "nika_run".into(),
                    reason: format!("Invalid JSON parameters: {}", e),
                })?;

            // Validate: either workflow path or yaml_content must be provided
            let is_inline = params.yaml_content.is_some();
            if !is_inline && params.workflow.is_empty() {
                return Err(NikaError::BuiltinInvalidParams {
                    tool: "nika_run".into(),
                    reason: "Either 'workflow' (file path) or 'yaml_content' (inline YAML) must be provided".into(),
                });
            }

            // Validate workflow path extension (only for file-based runs)
            if !is_inline
                && !params.workflow.ends_with(".nika.yaml")
                && !params.workflow.ends_with(".nika.yml")
            {
                return Err(NikaError::BuiltinInvalidParams {
                    tool: "nika_run".into(),
                    reason: format!(
                        "Workflow path must have .nika.yaml or .nika.yml extension: '{}'",
                        params.workflow
                    ),
                });
            }

            // Clamp max_depth and timeout to allowed ranges (defense-in-depth)
            // Ensure minimum of 1 to prevent zero-value edge cases
            let max_depth = params.max_depth.clamp(1, MAX_ALLOWED_DEPTH);
            let timeout_secs = params.timeout_secs.clamp(1, MAX_TIMEOUT_SECS);

            // Check current depth using task_local! (race-condition-safe)
            let depth = current_depth();
            if depth >= max_depth {
                return Err(NikaError::RunDepthExceeded {
                    depth: depth + 1,
                    max: max_depth,
                });
            }

            // ── Nika Shield Item 4: capability check ──────────────────────
            // Tainted callers cannot launch nested workflows unless they
            // carry `trust: elevated`. Reads the calling task's trust state
            // from the task_local set by execute_task_iteration (Item 0.F).
            let caller_trust = current_task_trust();
            let caller_elevated = current_task_elevated();
            if caller_trust.is_untrusted() && !caller_elevated {
                let caller_id = current_task_id()
                    .map(|id| id.to_string())
                    .unwrap_or_else(|| "<top>".to_string());
                return Err(NikaError::CapabilityDenied {
                    task_id: caller_id,
                    action: "nika:run".to_string(),
                    reason: "parent task has untrusted inputs and is not trust: elevated"
                        .to_string(),
                });
            }

            // ── Nika Shield Item 4: cycle detection ───────────────────────
            // Track canonical workflow paths in PARENT_CHAIN. Re-entering a
            // workflow via nested call (workflow A → B → A) hard-fails with
            // NIKA-387 even when within the depth limit. Pure cycle, not
            // depth-based.
            let canonical_for_cycle: Option<std::path::PathBuf> = if !is_inline {
                let workflow_path = Path::new(&params.workflow);
                workflow_path.canonicalize().ok()
            } else {
                None
            };
            let parent_chain = current_parent_chain();
            if let Some(ref canon) = canonical_for_cycle {
                if parent_chain.iter().any(|p| p == canon) {
                    return Err(NikaError::RunCycleDetected {
                        workflow_path: canon.display().to_string(),
                    });
                }
            }

            let next_depth = depth + 1;
            let timeout_duration = Duration::from_secs(timeout_secs);

            // Get YAML content: either inline or from file
            // B08: track workflow file directory for base_path propagation
            let mut workflow_dir: Option<std::path::PathBuf> = None;
            let yaml_content = if let Some(ref inline_yaml) = params.yaml_content {
                // Inline YAML: validate minimum structure
                if !inline_yaml.contains("schema:") {
                    return Err(NikaError::BuiltinInvalidParams {
                        tool: "nika_run".into(),
                        reason: "Inline YAML must contain 'schema:' field".into(),
                    });
                }
                tracing::debug!(
                    target: "nika_run",
                    label = %params.workflow,
                    len = inline_yaml.len(),
                    "Using inline YAML content"
                );
                inline_yaml.clone()
            } else {
                // File-based: path canonicalization + async read
                let workflow_path = Path::new(&params.workflow);
                let canonical_path =
                    workflow_path
                        .canonicalize()
                        .map_err(|e| NikaError::BuiltinToolError {
                            tool: "nika_run".into(),
                            reason: format!(
                                "Failed to resolve workflow path '{}': {}",
                                params.workflow, e
                            ),
                        })?;

                workflow_dir = canonical_path.parent().map(|p| p.to_path_buf());

                tracing::debug!(
                    target: "nika_run",
                    original = %params.workflow,
                    canonical = %canonical_path.display(),
                    "Resolved workflow path"
                );

                tokio::time::timeout(
                    Duration::from_secs(30),
                    tokio::fs::read_to_string(&canonical_path),
                )
                .await
                .map_err(|_| NikaError::BuiltinToolError {
                    tool: "nika_run".into(),
                    reason: format!(
                        "Timed out reading workflow file '{}' after 30 seconds",
                        params.workflow
                    ),
                })?
                .map_err(|e| NikaError::BuiltinToolError {
                    tool: "nika_run".into(),
                    reason: format!("Failed to read workflow file: {}", e),
                })?
            };

            let workflow =
                parse_analyzed(&yaml_content).map_err(|e| NikaError::BuiltinToolError {
                    tool: "nika_run".into(),
                    reason: format!("Failed to parse workflow YAML: {}", e),
                })?;

            tracing::info!(
                target: "nika_run",
                workflow = %params.workflow,
                depth = next_depth,
                max_depth = max_depth,
                timeout_secs = timeout_secs,
                has_context = params.context.is_some() || params.context_json.is_some(),
                "Executing nested workflow"
            );

            // Create runner and inject context if provided
            let mut runner = Runner::new(workflow)?.quiet();
            // B08 fix: propagate base_path from workflow file location
            if let Some(dir) = workflow_dir {
                runner = runner.with_base_path(dir);
            }

            // ── Nika Shield Item 4: trust ceiling propagation ─────────────
            // The nested workflow inherits the parent's trust ceiling via
            // `InvocationSource::NestedRun { ceiling }`. This means the
            // child workflow's `inputs:` bindings are trusted at MOST as
            // much as the parent task that called nika:run. Propagates
            // through arbitrary nesting because input_trust() returns
            // the ceiling directly.
            runner = runner.with_invocation_source(nika_core::trust::InvocationSource::NestedRun {
                ceiling: caller_trust,
            });

            // Inject parent context into child workflow's datastore
            if let Some(context) = params.get_context()? {
                runner = runner.with_initial_context("__parent_context__", context);
            }

            // Execute with task_local! depth tracking + cycle-detection
            // chain. WORKFLOW_DEPTH.scope() provides automatic cleanup on
            // panic/cancellation; PARENT_CHAIN extends with the current
            // workflow's canonical path so deeper nests can detect cycles.
            let mut new_chain = parent_chain.clone();
            if let Some(canon) = canonical_for_cycle {
                new_chain.push(canon);
            }
            let execution_result = WORKFLOW_DEPTH
                .scope(Cell::new(next_depth), async {
                    PARENT_CHAIN
                        .scope(new_chain, async {
                            tokio::time::timeout(timeout_duration, runner.run()).await
                        })
                        .await
                })
                .await;

            let duration_ms = start.elapsed().as_millis() as u64;

            // Handle timeout or execution result
            let result = match execution_result {
                Ok(Ok(output)) => output,
                Ok(Err(e)) => {
                    return Err(NikaError::BuiltinToolError {
                        tool: "nika_run".into(),
                        reason: format!("Workflow execution failed: {}", e),
                    });
                }
                Err(_) => {
                    return Err(NikaError::BuiltinToolError {
                        tool: "nika_run".into(),
                        reason: format!(
                            "Workflow execution timed out after {} seconds",
                            timeout_secs
                        ),
                    });
                }
            };

            // Build response with workflow output
            let response = RunResponse {
                executed: true,
                workflow: params.workflow,
                output: serde_json::json!({
                    "status": "completed",
                    "result": result
                }),
                duration_ms,
                depth: next_depth,
            };

            serde_json::to_string(&response).map_err(|e| NikaError::BuiltinToolError {
                tool: "nika_run".into(),
                reason: format!("Failed to serialize response: {}", e),
            })
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// P0-5: outside any task_local scope, the helpers must return the
    /// safe defaults (no calling task, Trusted floor, not elevated, empty
    /// chain). This is the contract that lets `nika invoke nika:read foo.md`
    /// work as a top-level CLI command.
    #[tokio::test]
    async fn task_local_helpers_return_safe_defaults_outside_scope() {
        assert!(current_task_id().is_none());
        assert_eq!(current_task_trust(), TrustLevel::Trusted);
        assert!(!current_task_elevated());
        assert!(current_parent_chain().is_empty());
    }

    /// Round-trip the task_local through a `.scope().await` and verify the
    /// helpers see the scoped values from inside, then revert to defaults
    /// outside the scope.
    #[tokio::test]
    async fn task_local_helpers_round_trip_through_scopes() {
        let id: Arc<str> = Arc::from("task_a");
        let id_for_scope = Arc::clone(&id);

        CURRENT_TASK_ID
            .scope(Some(id_for_scope), async {
                CURRENT_TASK_TRUST
                    .scope(TrustLevel::Untrusted, async {
                        CURRENT_TASK_ELEVATED
                            .scope(true, async {
                                PARENT_CHAIN
                                    .scope(
                                        vec![std::path::PathBuf::from("/wf/a.nika.yaml")],
                                        async {
                                            assert_eq!(
                                                current_task_id().as_deref(),
                                                Some("task_a")
                                            );
                                            assert_eq!(current_task_trust(), TrustLevel::Untrusted);
                                            assert!(current_task_elevated());
                                            assert_eq!(current_parent_chain().len(), 1);
                                        },
                                    )
                                    .await
                            })
                            .await
                    })
                    .await
            })
            .await;

        // Outside the scope, defaults are restored.
        assert!(current_task_id().is_none());
        assert_eq!(current_task_trust(), TrustLevel::Trusted);
        assert!(!current_task_elevated());
        assert!(current_parent_chain().is_empty());
    }

    /// Sprint 2 Item 4: tainted callers cannot launch nested workflows
    /// unless they carry `trust: elevated`. Returns NIKA-380.
    #[tokio::test]
    async fn nika_run_blocks_tainted_caller_without_elevation() {
        let tool = RunTool;
        let id: Arc<str> = Arc::from("parent");
        let result = CURRENT_TASK_ID
            .scope(Some(id), async {
                CURRENT_TASK_TRUST
                    .scope(TrustLevel::Untrusted, async {
                        CURRENT_TASK_ELEVATED
                            .scope(false, async {
                                WORKFLOW_DEPTH
                                    .scope(Cell::new(0), async {
                                        tool.call(
                                            r#"{"workflow":"some/path.nika.yaml"}"#.to_string(),
                                        )
                                        .await
                                    })
                                    .await
                            })
                            .await
                    })
                    .await
            })
            .await;
        assert!(result.is_err(), "tainted caller must be blocked");
        let err = result.unwrap_err();
        assert_eq!(err.code(), "NIKA-380");
    }

    /// Sprint 2 Item 4: tainted + elevated callers can launch nested
    /// workflows. They still hit the file-not-found error but at least the
    /// capability check passes.
    #[tokio::test]
    async fn nika_run_allows_tainted_elevated_caller() {
        let tool = RunTool;
        let id: Arc<str> = Arc::from("parent");
        let result = CURRENT_TASK_ID
            .scope(Some(id), async {
                CURRENT_TASK_TRUST
                    .scope(TrustLevel::Untrusted, async {
                        CURRENT_TASK_ELEVATED
                            .scope(true, async {
                                WORKFLOW_DEPTH
                                    .scope(Cell::new(0), async {
                                        tool.call(
                                            r#"{"workflow":"nonexistent.nika.yaml"}"#.to_string(),
                                        )
                                        .await
                                    })
                                    .await
                            })
                            .await
                    })
                    .await
            })
            .await;
        // Should NOT fail with NIKA-380 (capability denied) — it should
        // proceed past the capability check and hit the file resolution
        // failure (NIKA-210 BuiltinToolError) instead.
        let err = result.unwrap_err();
        assert_ne!(
            err.code(),
            "NIKA-380",
            "elevated caller must bypass capability check, got: {err}"
        );
    }

    /// Sprint 2 Item 4: depth limit returns NIKA-386, not the old generic
    /// BuiltinToolError. Hardcoded MAX_ALLOWED_DEPTH = 10 still applies as
    /// the upper bound.
    #[tokio::test]
    async fn nika_run_depth_exceeded_returns_nika_386() {
        let tool = RunTool;
        let result = WORKFLOW_DEPTH
            .scope(Cell::new(MAX_ALLOWED_DEPTH), async {
                tool.call(r#"{"workflow":"some/path.nika.yaml","max_depth":3}"#.to_string())
                    .await
            })
            .await;
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert_eq!(err.code(), "NIKA-386");
    }

    /// Sprint 2 Item 4: a workflow path already in PARENT_CHAIN cannot be
    /// re-entered, even within the depth limit. Returns NIKA-387.
    #[tokio::test]
    async fn nika_run_cycle_detection_blocks_re_entry() {
        use tempfile::TempDir;

        let dir = TempDir::new().expect("tempdir");
        let workflow_path = dir.path().join("loop.nika.yaml");
        std::fs::write(
            &workflow_path,
            "schema: \"nika/workflow@0.12\"\nworkflow: loop\ntasks:\n  - id: t\n    infer: \"x\"\n",
        )
        .expect("write workflow");

        let canonical = workflow_path.canonicalize().expect("canonicalize");
        let chain = vec![canonical.clone()];

        let tool = RunTool;
        let workflow_str = workflow_path.to_string_lossy().to_string();
        let result = WORKFLOW_DEPTH
            .scope(Cell::new(1), async {
                PARENT_CHAIN
                    .scope(chain, async {
                        tool.call(format!(r#"{{"workflow":"{workflow_str}"}}"#))
                            .await
                    })
                    .await
            })
            .await;
        assert!(result.is_err(), "cycle must be blocked");
        let err = result.unwrap_err();
        assert_eq!(err.code(), "NIKA-387");
    }

    #[test]
    fn test_run_tool_name() {
        let tool = RunTool;
        assert_eq!(tool.name(), "run");
    }

    #[test]
    fn test_run_tool_description() {
        let tool = RunTool;
        assert!(tool.description().contains("workflow"));
    }

    #[test]
    fn test_run_tool_schema() {
        let tool = RunTool;
        let schema = tool.parameters_schema();
        assert_eq!(schema["type"], "object");
        assert!(schema["properties"]["workflow"].is_object());
        // context_json for OpenAI compatibility
        assert!(schema["properties"]["context_json"].is_object());
        // timeout_secs and max_depth for production safety
        assert!(schema["properties"]["timeout_secs"].is_object());
        assert!(schema["properties"]["max_depth"].is_object());
        assert_eq!(schema["additionalProperties"], false);
        // No required fields: either workflow or yaml_content can be used
        assert!(schema["properties"]["yaml_content"].is_object());
    }

    #[tokio::test]
    async fn test_run_nonexistent_file_errors() {
        let tool = RunTool;
        let result = tool
            .call(r#"{"workflow": "path/to/workflow.nika.yaml"}"#.to_string())
            .await;

        assert!(result.is_err());
        let err = result.unwrap_err();
        // Path canonicalization gives "Failed to resolve workflow path" error
        assert!(
            err.to_string().contains("resolve workflow path")
                || err.to_string().contains("not found")
        );
    }

    #[tokio::test]
    async fn test_run_executes_real_workflow() {
        use std::io::Write;
        use tempfile::NamedTempFile;

        // Create a minimal workflow file
        let mut temp_file = NamedTempFile::with_suffix(".nika.yaml").unwrap();
        writeln!(
            temp_file,
            r#"schema: nika/workflow@0.12
workflow: test-workflow
tasks:
  - id: hello
    exec: "echo hello""#
        )
        .unwrap();

        let tool = RunTool;
        let result = tool
            .call(format!(
                r#"{{"workflow": "{}"}}"#,
                temp_file.path().display()
            ))
            .await;

        assert!(result.is_ok());
        let response: serde_json::Value = serde_json::from_str(&result.unwrap()).unwrap();
        assert_eq!(response["executed"], true);
    }

    #[tokio::test]
    async fn test_run_returns_workflow_output() {
        use std::io::Write;
        use tempfile::NamedTempFile;

        // Create a minimal workflow file
        let mut temp_file = NamedTempFile::with_suffix(".nika.yaml").unwrap();
        writeln!(
            temp_file,
            r#"schema: nika/workflow@0.12
workflow: test-output
tasks:
  - id: greet
    exec: "echo world""#
        )
        .unwrap();

        let tool = RunTool;
        let result = tool
            .call(format!(
                r#"{{"workflow": "{}"}}"#,
                temp_file.path().display()
            ))
            .await;

        assert!(result.is_ok());
        let response: serde_json::Value = serde_json::from_str(&result.unwrap()).unwrap();
        assert_eq!(response["executed"], true);
        assert!(response["output"].is_object());
    }

    #[tokio::test]
    async fn test_run_empty_path_errors() {
        let tool = RunTool;
        let result = tool.call(r#"{"workflow": ""}"#.to_string()).await;

        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(
            err.to_string().contains("must be provided")
                || err.to_string().contains("cannot be empty")
        );
    }

    #[tokio::test]
    async fn test_run_invalid_extension_errors() {
        let tool = RunTool;
        let result = tool
            .call(r#"{"workflow": "workflow.yaml"}"#.to_string())
            .await;

        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.to_string().contains(".nika.yaml"));
    }

    #[tokio::test]
    async fn test_run_accepts_yml_extension() {
        use std::io::Write;
        use tempfile::NamedTempFile;

        // Create a minimal workflow file with .yml extension
        let mut temp_file = NamedTempFile::with_suffix(".nika.yml").unwrap();
        writeln!(
            temp_file,
            r#"schema: nika/workflow@0.12
workflow: test-yml
tasks:
  - id: test
    exec: "echo yml""#
        )
        .unwrap();

        let tool = RunTool;
        let result = tool
            .call(format!(
                r#"{{"workflow": "{}"}}"#,
                temp_file.path().display()
            ))
            .await;

        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_run_invalid_json() {
        let tool = RunTool;
        let result = tool.call("not json".to_string()).await;

        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.to_string().contains("Invalid JSON parameters"));
    }

    #[tokio::test]
    async fn test_run_missing_workflow_and_yaml() {
        let tool = RunTool;
        let result = tool.call(r#"{"context": {"test": 1}}"#.to_string()).await;

        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(
            err.to_string().contains("must be provided"),
            "Expected 'must be provided' error, got: {}",
            err
        );
    }

    #[tokio::test]
    async fn test_run_params_deserialization() {
        let json = r#"{"workflow": "test.nika.yaml", "context": {"key": "value"}}"#;
        let params: RunParams = serde_json::from_str(json).unwrap();

        assert_eq!(params.workflow, "test.nika.yaml");
        assert!(params.context.is_some());
        assert_eq!(params.context.as_ref().unwrap()["key"], "value");
        // defaults
        assert_eq!(params.timeout_secs, 300);
        assert_eq!(params.max_depth, 3);
    }

    #[tokio::test]
    async fn test_run_params_without_context() {
        let json = r#"{"workflow": "test.nika.yaml"}"#;
        let params: RunParams = serde_json::from_str(json).unwrap();

        assert_eq!(params.workflow, "test.nika.yaml");
        assert!(params.context.is_none());
        // defaults applied
        assert_eq!(params.timeout_secs, 300);
        assert_eq!(params.max_depth, 3);
    }

    #[tokio::test]
    async fn test_run_params_custom_timeout_and_depth() {
        let json = r#"{"workflow": "test.nika.yaml", "timeout_secs": 60, "max_depth": 5}"#;
        let params: RunParams = serde_json::from_str(json).unwrap();

        assert_eq!(params.workflow, "test.nika.yaml");
        assert_eq!(params.timeout_secs, 60);
        assert_eq!(params.max_depth, 5);
    }

    #[tokio::test]
    async fn test_run_response_includes_duration_and_depth() {
        use std::io::Write;
        use tempfile::NamedTempFile;

        // Create a minimal workflow file
        let mut temp_file = NamedTempFile::with_suffix(".nika.yaml").unwrap();
        writeln!(
            temp_file,
            r#"schema: nika/workflow@0.12
workflow: test-response
tasks:
  - id: test
    exec: "echo test""#
        )
        .unwrap();

        let tool = RunTool;
        let result = tool
            .call(format!(
                r#"{{"workflow": "{}"}}"#,
                temp_file.path().display()
            ))
            .await;

        assert!(result.is_ok());
        let response: serde_json::Value = serde_json::from_str(&result.unwrap()).unwrap();
        assert_eq!(response["executed"], true);
        assert!(response["duration_ms"].is_number());
        assert_eq!(response["depth"], 1);
    }

    #[test]
    fn test_max_depth_constant() {
        assert_eq!(MAX_ALLOWED_DEPTH, 10);
    }

    #[test]
    fn test_max_timeout_constant() {
        assert_eq!(MAX_TIMEOUT_SECS, 3600);
    }

    #[test]
    fn test_default_timeout() {
        assert_eq!(default_timeout(), 300);
    }

    #[test]
    fn test_default_max_depth() {
        assert_eq!(default_max_depth(), 3);
    }

    // ═══════════════════════════════════════════════════════════════
    // task_local! DEPTH TRACKING TESTS
    // ═══════════════════════════════════════════════════════════════

    #[tokio::test]
    async fn test_current_depth_returns_zero_outside_scope() {
        // Outside any WORKFLOW_DEPTH scope, should return 0
        let depth = current_depth();
        assert_eq!(depth, 0, "Outside scope should return 0");
    }

    #[tokio::test]
    async fn test_current_depth_returns_value_inside_scope() {
        // Inside scope, should return the set value
        let depth = WORKFLOW_DEPTH
            .scope(Cell::new(5), async { current_depth() })
            .await;
        assert_eq!(depth, 5, "Inside scope should return set value");
    }

    #[tokio::test]
    async fn test_depth_isolation_between_concurrent_tasks() {
        use std::sync::Arc;
        use tokio::sync::Barrier;

        // Two concurrent tasks should have isolated depth values
        let barrier = Arc::new(Barrier::new(2));

        let b1 = Arc::clone(&barrier);
        let task1 = tokio::spawn(async move {
            WORKFLOW_DEPTH
                .scope(Cell::new(1), async {
                    // Wait for both tasks to be inside their scopes
                    b1.wait().await;
                    tokio::time::sleep(Duration::from_millis(10)).await;
                    current_depth()
                })
                .await
        });

        let b2 = Arc::clone(&barrier);
        let task2 = tokio::spawn(async move {
            WORKFLOW_DEPTH
                .scope(Cell::new(99), async {
                    // Wait for both tasks to be inside their scopes
                    b2.wait().await;
                    tokio::time::sleep(Duration::from_millis(10)).await;
                    current_depth()
                })
                .await
        });

        let (result1, result2) = tokio::join!(task1, task2);
        assert_eq!(result1.unwrap(), 1, "Task 1 should have depth 1");
        assert_eq!(result2.unwrap(), 99, "Task 2 should have depth 99");
    }

    #[tokio::test]
    async fn test_depth_nested_scopes() {
        // Nested scopes should work correctly
        let inner_depth = WORKFLOW_DEPTH
            .scope(Cell::new(1), async {
                let outer = current_depth();
                let inner = WORKFLOW_DEPTH
                    .scope(Cell::new(2), async { current_depth() })
                    .await;
                (outer, inner)
            })
            .await;

        assert_eq!(inner_depth.0, 1, "Outer scope should have depth 1");
        assert_eq!(inner_depth.1, 2, "Inner scope should have depth 2");
    }

    #[tokio::test]
    async fn test_depth_cleanup_on_panic() {
        use std::panic::AssertUnwindSafe;

        // Scope should clean up even on panic
        let result = std::panic::catch_unwind(AssertUnwindSafe(|| {
            tokio::runtime::Handle::current().block_on(async {
                let _ = WORKFLOW_DEPTH
                    .scope(Cell::new(42), async {
                        panic!("test panic");
                    })
                    .await;
            });
        }));

        assert!(result.is_err(), "Should have panicked");
        // After panic, depth should be back to 0 (outside scope)
        let depth = current_depth();
        assert_eq!(depth, 0, "Depth should be 0 after panic cleanup");
    }

    // ═══════════════════════════════════════════════════════════════
    // TIMEOUT CLAMPING TESTS
    // ═══════════════════════════════════════════════════════════════

    #[tokio::test]
    async fn test_timeout_clamped_to_max() {
        use std::io::Write;
        use tempfile::NamedTempFile;

        // Create a minimal workflow file
        let mut temp_file = NamedTempFile::with_suffix(".nika.yaml").unwrap();
        writeln!(
            temp_file,
            r#"schema: nika/workflow@0.12
workflow: test-timeout-clamp
tasks:
  - id: test
    exec: "echo done""#
        )
        .unwrap();

        let tool = RunTool;
        // Request timeout > MAX_TIMEOUT_SECS (3600)
        let result = tool
            .call(format!(
                r#"{{"workflow": "{}", "timeout_secs": 99999}}"#,
                temp_file.path().display()
            ))
            .await;

        // Should succeed (clamped, not rejected)
        assert!(result.is_ok(), "Should succeed with clamped timeout");
    }

    // ═══════════════════════════════════════════════════════════════
    // CONTEXT INJECTION TESTS
    // ═══════════════════════════════════════════════════════════════

    #[test]
    fn test_run_params_get_context_from_context_field() {
        let params = RunParams {
            workflow: "test.nika.yaml".to_string(),
            yaml_content: None,
            context_json: None,
            context: Some(serde_json::json!({"key": "value"})),
            timeout_secs: 300,
            max_depth: 3,
        };

        let context = params.get_context().unwrap();
        assert!(context.is_some());
        assert_eq!(context.unwrap()["key"], "value");
    }

    #[test]
    fn test_run_params_get_context_from_context_json() {
        let params = RunParams {
            workflow: "test.nika.yaml".to_string(),
            yaml_content: None,
            context_json: Some(r#"{"from": "json"}"#.to_string()),
            context: None,
            timeout_secs: 300,
            max_depth: 3,
        };

        let context = params.get_context().unwrap();
        assert!(context.is_some());
        assert_eq!(context.unwrap()["from"], "json");
    }

    #[test]
    fn test_run_params_get_context_json_priority() {
        // context_json should take priority over context
        let params = RunParams {
            workflow: "test.nika.yaml".to_string(),
            yaml_content: None,
            context_json: Some(r#"{"priority": "json"}"#.to_string()),
            context: Some(serde_json::json!({"priority": "object"})),
            timeout_secs: 300,
            max_depth: 3,
        };

        let context = params.get_context().unwrap();
        assert!(context.is_some());
        assert_eq!(context.unwrap()["priority"], "json");
    }

    #[test]
    fn test_run_params_get_context_invalid_json_errors() {
        let params = RunParams {
            workflow: "test.nika.yaml".to_string(),
            yaml_content: None,
            context_json: Some("not valid json".to_string()),
            context: None,
            timeout_secs: 300,
            max_depth: 3,
        };

        let result = params.get_context();
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("Invalid context_json"));
    }

    #[test]
    fn test_run_params_get_context_none_when_both_empty() {
        let params = RunParams {
            workflow: "test.nika.yaml".to_string(),
            yaml_content: None,
            context_json: None,
            context: None,
            timeout_secs: 300,
            max_depth: 3,
        };

        let context = params.get_context().unwrap();
        assert!(context.is_none());
    }

    // ═══════════════════════════════════════════
    // D.4: Inline YAML support
    // ═══════════════════════════════════════════

    #[tokio::test]
    async fn test_run_inline_yaml() {
        let tool = RunTool;
        let yaml = r#"schema: nika/workflow@0.12
workflow: inline-test
tasks:
  - id: greet
    exec: "echo inline""#;

        let result = tool
            .call(
                serde_json::json!({
                    "workflow": "inline-test",
                    "yaml_content": yaml
                })
                .to_string(),
            )
            .await;

        assert!(
            result.is_ok(),
            "Inline YAML should work: {:?}",
            result.err()
        );
        let response: serde_json::Value = serde_json::from_str(&result.unwrap()).unwrap();
        assert_eq!(response["executed"], true);
    }

    #[tokio::test]
    async fn test_run_inline_yaml_invalid_schema() {
        let tool = RunTool;
        let result = tool
            .call(
                serde_json::json!({
                    "yaml_content": "this is not YAML with schema"
                })
                .to_string(),
            )
            .await;

        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.to_string().contains("schema"));
    }

    #[tokio::test]
    async fn test_run_inline_yaml_malformed() {
        let tool = RunTool;
        let result = tool
            .call(
                serde_json::json!({
                    "yaml_content": "schema: nika/workflow@0.12\ntasks: not_a_list"
                })
                .to_string(),
            )
            .await;

        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_run_inline_yaml_no_schema_keyword() {
        let tool = RunTool;
        let result = tool
            .call(
                serde_json::json!({
                    "yaml_content": "tasks:\n  - id: foo\n    exec: echo"
                })
                .to_string(),
            )
            .await;

        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.to_string().contains("schema"));
    }

    #[tokio::test]
    async fn test_run_neither_workflow_nor_yaml_errors() {
        let tool = RunTool;
        let result = tool.call(r#"{}"#.to_string()).await;

        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.to_string().contains("must be provided"));
    }

    #[test]
    fn test_params_deserialize_with_yaml_content() {
        let params: RunParams = serde_json::from_str(
            r#"{
            "workflow": "label",
            "yaml_content": "schema: nika/workflow@0.12\ntasks: []"
        }"#,
        )
        .unwrap();
        assert_eq!(params.workflow, "label");
        assert!(params.yaml_content.is_some());
    }

    #[tokio::test]
    async fn test_run_inline_yaml_respects_depth_limit() {
        let tool = RunTool;
        let result = tool
            .call(
                serde_json::json!({
                    "yaml_content": "schema: nika/workflow@0.12\ntasks:\n  - id: x\n    exec: echo",
                    "max_depth": 1
                })
                .to_string(),
            )
            .await;

        // At depth 0, max_depth 1 should allow execution (0 < 1)
        assert!(
            result.is_ok(),
            "Depth 0 < max_depth 1 should work: {:?}",
            result.err()
        );
    }
}
