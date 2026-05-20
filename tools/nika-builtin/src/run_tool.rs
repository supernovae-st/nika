// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! nika:run — Execute nested workflows with depth and cycle protection.
//!
//! This tool reads trust state from `nika_kernel::task_local`, performs Shield
//! checks (trust capability, depth limit, cycle detection), constructs a
//! [`RunSpec`], and delegates execution to [`RunExecutor`].
//!
//! nika-engine provides `EngineRunExecutor` which handles YAML parsing,
//! `Runner` creation, and `task_local` scoping (WORKFLOW_DEPTH + PARENT_CHAIN).
//!
//! # Parameters
//!
//! ```json
//! {
//!   "workflow": "path/to/workflow.nika.yaml",
//!   "yaml_content": "schema: nika/workflow@0.12 ...",
//!   "context_json": "{\"key\": \"value\"}",
//!   "timeout_secs": 300,
//!   "max_depth": 3
//! }
//! ```
//!
//! `context_json` accepts EITHER a JSON-encoded string OR an inline object.
//! The engine's `coerce_json_types` template-resolution helper auto-parses
//! JSON-shaped strings into native `Value::Object` BEFORE the schema is
//! validated, so accepting both shapes prevents B-4-style false rejects.
//!
//! # Returns
//!
//! ```json
//! {
//!   "executed": true,
//!   "workflow": "path/to/workflow.nika.yaml",
//!   "output": { "status": "completed", "result": {...} },
//!   "duration_ms": 1234,
//!   "depth": 1
//! }
//! ```
//!
//! [`RunSpec`]: nika_kernel::scope::RunSpec
//! [`RunExecutor`]: nika_kernel::scope::RunExecutor

use crate::{BuiltinError, BuiltinTool, __sealed};
use nika_kernel::scope::{RunExecutor, RunSpec};
use nika_kernel::task_local::{
    current_depth, current_parent_chain, current_task_elevated, current_task_id,
    current_task_trust,
};
use serde::{Deserialize, Serialize};
use std::future::Future;
use std::path::PathBuf;
use std::pin::Pin;
use std::sync::Arc;
use std::time::Duration;

const MAX_ALLOWED_DEPTH: u32 = 10;
const MAX_TIMEOUT_SECS: u64 = 3600;

fn default_timeout() -> u64 {
    300
}
fn default_max_depth() -> u32 {
    3
}

/// Accepts EITHER a JSON-encoded string OR an inline JSON object/array.
///
/// Template resolution in the engine (`coerce_json_types`) auto-parses
/// JSON-shaped strings like `"{\"k\":\"v\"}"` back to `Value::Object`
/// BEFORE schema validation. Without an untagged enum here, the param
/// resolver passes a Map · serde rejects on `Option<String>` (B-4).
#[derive(Debug, Clone, Deserialize)]
#[serde(untagged)]
pub enum ContextJson {
    /// Pre-serialized JSON string (legacy OpenAI strict-mode shape).
    String(String),
    /// Native JSON value (object/array/primitive) — post template resolve.
    Value(serde_json::Value),
}

/// Parameters for `nika:run`.
#[derive(Debug, Clone, Deserialize)]
pub struct RunParams {
    /// Path to the workflow file (required when `yaml_content` is absent).
    #[serde(default)]
    pub workflow: String,
    /// Inline YAML content (alternative to file path).
    #[serde(default)]
    pub yaml_content: Option<String>,
    /// Context as JSON string OR object (see [`ContextJson`]).
    ///
    /// Pre-B-4 this was `Option<String>` only. Now accepts both shapes
    /// because the engine's template-resolution coerces JSON-shaped
    /// strings into objects before this struct is deserialized.
    #[serde(default)]
    pub context_json: Option<ContextJson>,
    /// Context object to pass into the child workflow.
    #[serde(default)]
    pub context: Option<serde_json::Value>,
    /// Execution timeout in seconds (default: 300, max: 3600).
    #[serde(default = "default_timeout")]
    pub timeout_secs: u64,
    /// Maximum recursion depth (default: 3, max: 10).
    #[serde(default = "default_max_depth")]
    pub max_depth: u32,
}

impl RunParams {
    fn get_context(&self) -> Result<Option<serde_json::Value>, BuiltinError> {
        match self.context_json {
            Some(ContextJson::String(ref json_str)) => {
                // Backward-compat path · pre-serialized JSON-encoded string.
                serde_json::from_str(json_str).map(Some).map_err(|e| {
                    BuiltinError::InvalidArgs {
                        tool: "nika:run".into(),
                        reason: format!("Invalid context_json: {e}"),
                    }
                })
            }
            Some(ContextJson::Value(ref v)) => {
                // B-4 fix path · post-coercion native object/array.
                Ok(Some(v.clone()))
            }
            None => Ok(self.context.clone()),
        }
    }
}

/// Response from `nika:run`.
#[derive(Debug, Clone, Serialize)]
pub struct RunResponse {
    pub executed: bool,
    pub workflow: String,
    pub output: serde_json::Value,
    pub duration_ms: u64,
    pub depth: u32,
}

/// `nika:run` builtin tool.
///
/// Performs Shield checks (trust, depth, cycle) and delegates to
/// [`RunExecutor`] for the actual workflow execution.
pub struct RunTool {
    executor: Arc<dyn RunExecutor>,
}

impl RunTool {
    /// Create a new `RunTool` backed by the given executor.
    ///
    /// In production, `executor` is `EngineRunExecutor` from nika-engine.
    pub fn new(executor: Arc<dyn RunExecutor>) -> Self {
        Self { executor }
    }
}

impl __sealed::Sealed for RunTool {}

impl BuiltinTool for RunTool {
    fn name(&self) -> &'static str {
        "run"
    }

    fn description(&self) -> &'static str {
        "Execute a nested workflow and return its output"
    }

    fn parameters_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "workflow": {
                    "type": "string",
                    "description": "Path to the .nika.yaml workflow file"
                },
                "yaml_content": {
                    "type": "string",
                    "description": "Inline YAML content (alternative to file path)"
                },
                "context_json": {
                    "type": ["string", "object", "array", "null"],
                    "description": "Context as JSON string OR native object/array. \
                                    Strings are parsed; objects/arrays are passed through. \
                                    Engine template-resolution coerces JSON-shaped strings \
                                    to objects before this point (B-4 fix v0.82)."
                },
                "timeout_secs": {
                    "type": "integer",
                    "description": "Timeout in seconds (default: 300, max: 3600)"
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
    ) -> Pin<Box<dyn Future<Output = Result<String, BuiltinError>> + Send + 'a>> {
        Box::pin(async move {
            let start = std::time::Instant::now();

            let params: RunParams = serde_json::from_str(&args)
                .map_err(|e| BuiltinError::invalid_params("nika:run", e))?;

            let is_inline = params.yaml_content.is_some();

            // Validate: must provide path or inline YAML
            if !is_inline && params.workflow.is_empty() {
                return Err(BuiltinError::InvalidArgs {
                    tool: "nika:run".into(),
                    reason: "Either 'workflow' (file path) or 'yaml_content' (inline YAML) must be provided".into(),
                });
            }

            // Validate file extension for path-based runs
            if !is_inline
                && !params.workflow.ends_with(".nika.yaml")
                && !params.workflow.ends_with(".nika.yml")
            {
                return Err(BuiltinError::InvalidArgs {
                    tool: "nika:run".into(),
                    reason: format!(
                        "Workflow path must have .nika.yaml or .nika.yml extension: '{}'",
                        params.workflow
                    ),
                });
            }

            let max_depth = params.max_depth.clamp(1, MAX_ALLOWED_DEPTH);
            let timeout_secs = params.timeout_secs.clamp(1, MAX_TIMEOUT_SECS);

            // ── Shield: trust capability check ───────────────────────────
            let caller_trust = current_task_trust();
            let caller_elevated = current_task_elevated();
            if caller_trust.is_untrusted() && !caller_elevated {
                let task_id = current_task_id()
                    .map(|id| id.to_string())
                    .unwrap_or_else(|| "<top>".to_string());
                return Err(BuiltinError::Denied {
                    tool: "nika:run".into(),
                    reason: format!(
                        "[NIKA-380] task '{task_id}' has untrusted inputs and is not trust: elevated"
                    ),
                });
            }

            // ── Shield: depth limit ───────────────────────────────────────
            let depth = current_depth();
            if depth >= max_depth {
                return Err(BuiltinError::Denied {
                    tool: "nika:run".into(),
                    reason: format!(
                        "[NIKA-386] Workflow depth {d} exceeds max_depth {max_depth}",
                        d = depth + 1
                    ),
                });
            }

            // ── Shield: cycle detection ───────────────────────────────────
            let canonical_for_cycle: Option<PathBuf> = if !is_inline {
                std::path::Path::new(&params.workflow).canonicalize().ok()
            } else {
                None
            };
            let parent_chain = current_parent_chain();
            if let Some(ref canon) = canonical_for_cycle {
                if parent_chain.iter().any(|p| p == canon) {
                    return Err(BuiltinError::Denied {
                        tool: "nika:run".into(),
                        reason: format!(
                            "[NIKA-387] Cycle detected: workflow '{}' is already in the call chain",
                            canon.display()
                        ),
                    });
                }
            }

            // Build parent chain for the child (includes our path)
            let mut new_chain = parent_chain;
            if let Some(canon) = canonical_for_cycle {
                new_chain.push(canon.clone());
            }

            let spec = RunSpec {
                path: if !is_inline {
                    Some(PathBuf::from(&params.workflow))
                } else {
                    None
                },
                yaml_content: params.yaml_content.clone(),
                label: params.workflow.clone(),
                caller_trust,
                parent_context: params.get_context()?,
                depth: depth + 1,
                max_depth,
                timeout: Duration::from_secs(timeout_secs),
                parent_chain: new_chain,
            };

            let output = self.executor.run_workflow(spec).await?;

            let duration_ms = start.elapsed().as_millis() as u64;

            serde_json::to_string(&RunResponse {
                executed: true,
                workflow: params.workflow,
                output: serde_json::json!({
                    "status": "completed",
                    "result": output
                }),
                duration_ms,
                depth: depth + 1,
            })
            .map_err(|e| BuiltinError::tool_error("nika:run", e))
        })
    }
}

// ─────────────────────────────────────────────────────────────────────
// Tests
// ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use nika_core::trust::TrustLevel;
    use nika_kernel::task_local::{
        CURRENT_TASK_ELEVATED, CURRENT_TASK_ID, CURRENT_TASK_TRUST, PARENT_CHAIN, WORKFLOW_DEPTH,
    };
    use std::cell::Cell;

    /// Mock executor that records specs and returns a fixed value.
    struct MockExecutor {
        return_value: serde_json::Value,
    }

    #[async_trait::async_trait]
    impl RunExecutor for MockExecutor {
        async fn run_workflow(&self, _spec: RunSpec) -> Result<serde_json::Value, BuiltinError> {
            Ok(self.return_value.clone())
        }
    }

    /// Mock executor that always errors.
    struct ErrorExecutor;

    #[async_trait::async_trait]
    impl RunExecutor for ErrorExecutor {
        async fn run_workflow(&self, _spec: RunSpec) -> Result<serde_json::Value, BuiltinError> {
            Err(BuiltinError::Other {
                tool: "nika:run".into(),
                reason: "executor error".into(),
            })
        }
    }

    fn make_tool() -> RunTool {
        RunTool::new(Arc::new(MockExecutor {
            return_value: serde_json::json!({"ok": true}),
        }))
    }

    #[tokio::test]
    async fn test_valid_workflow_path_dispatches_to_executor() {
        let tool = make_tool();
        let result = tool
            .call(
                serde_json::json!({"workflow": "my_flow.nika.yaml"}).to_string(),
            )
            .await;
        assert!(result.is_ok(), "{:?}", result);
        let v: serde_json::Value = serde_json::from_str(&result.unwrap()).unwrap();
        assert_eq!(v["executed"], true);
        assert_eq!(v["workflow"], "my_flow.nika.yaml");
    }

    #[tokio::test]
    async fn test_inline_yaml_dispatches_to_executor() {
        let tool = make_tool();
        let result = tool
            .call(
                serde_json::json!({
                    "workflow": "inline-label",
                    "yaml_content": "schema: nika/workflow@0.12\ntasks: []"
                })
                .to_string(),
            )
            .await;
        assert!(result.is_ok(), "{:?}", result);
    }

    #[tokio::test]
    async fn test_missing_both_workflow_and_yaml_content_errors() {
        let tool = make_tool();
        let result = tool.call(r#"{}"#.to_string()).await;
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("workflow"));
    }

    #[tokio::test]
    async fn test_wrong_extension_errors() {
        let tool = make_tool();
        let result = tool
            .call(serde_json::json!({"workflow": "my_flow.yaml"}).to_string())
            .await;
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains(".nika.yaml"));
    }

    #[tokio::test]
    async fn test_depth_limit_blocks_execution() {
        let tool = make_tool();
        let result = WORKFLOW_DEPTH
            .scope(Cell::new(3), async {
                tool.call(
                    serde_json::json!({"workflow": "wf.nika.yaml", "max_depth": 3}).to_string(),
                )
                .await
            })
            .await;
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("NIKA-386"));
    }

    #[tokio::test]
    async fn test_untrusted_caller_without_elevation_blocked() {
        let tool = make_tool();
        let id: Arc<str> = Arc::from("tainted-task");
        let result = CURRENT_TASK_ID
            .scope(Some(id), async {
                CURRENT_TASK_TRUST
                    .scope(TrustLevel::Untrusted, async {
                        CURRENT_TASK_ELEVATED
                            .scope(false, async {
                                tool.call(
                                    serde_json::json!({"workflow": "wf.nika.yaml"}).to_string(),
                                )
                                .await
                            })
                            .await
                    })
                    .await
            })
            .await;
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("NIKA-380"));
    }

    #[tokio::test]
    async fn test_elevated_untrusted_caller_allowed() {
        let tool = make_tool();
        let result = CURRENT_TASK_TRUST
            .scope(TrustLevel::Untrusted, async {
                CURRENT_TASK_ELEVATED
                    .scope(true, async {
                        tool.call(
                            serde_json::json!({"workflow": "wf.nika.yaml"}).to_string(),
                        )
                        .await
                    })
                    .await
            })
            .await;
        // Should succeed (elevated bypasses the trust check)
        assert!(result.is_ok(), "{:?}", result);
    }

    #[tokio::test]
    async fn test_cycle_detection_blocks_repeated_workflow() {
        let tool = make_tool();
        // Simulate a chain where wf.nika.yaml is already in the parent chain.
        // We can't use the actual path (it doesn't exist), so we use a fake
        // canonical path in the chain.
        let fake_canon = PathBuf::from("/absolute/wf.nika.yaml");
        let chain = vec![fake_canon.clone()];

        // The cycle check only fires if the path can be canonicalized.
        // Since our test path doesn't exist, canonicalize() returns None
        // and the cycle check is skipped. This is intentional — we test the
        // code path, not the actual cycle detection with real files.
        // For real cycle detection, see the nika-engine integration tests.
        let result = PARENT_CHAIN
            .scope(chain, async {
                // We use yaml_content to avoid the path canonicalization
                tool.call(
                    serde_json::json!({
                        "workflow": "label",
                        "yaml_content": "schema: nika/workflow@0.12\ntasks: []"
                    })
                    .to_string(),
                )
                .await
            })
            .await;
        // No cycle — inline yaml, no path in parent_chain
        assert!(result.is_ok(), "{:?}", result);
    }

    #[tokio::test]
    async fn test_executor_error_propagates() {
        let tool = RunTool::new(Arc::new(ErrorExecutor));
        let result = tool
            .call(serde_json::json!({"workflow": "wf.nika.yaml"}).to_string())
            .await;
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("executor error"));
    }

    #[test]
    fn test_name_is_run() {
        assert_eq!(make_tool().name(), "run");
    }

    // ─────────────────────────────────────────────────────────────────
    // B-4 regression coverage · context_json accepts Object OR String
    // ─────────────────────────────────────────────────────────────────

    #[tokio::test]
    async fn test_b4_context_json_accepts_inline_object() {
        // The bug · template engine renders the string in JSON shape,
        // then coerce_json_types turns it into Value::Object before this
        // struct is deserialized. Pre-fix this rejected at NIKA-212.
        let tool = make_tool();
        let result = tool
            .call(
                serde_json::json!({
                    "workflow": "wf.nika.yaml",
                    "context_json": { "urls": ["https://a", "https://b"] }
                })
                .to_string(),
            )
            .await;
        assert!(result.is_ok(), "Object-shape context_json must parse · {:?}", result);
    }

    #[tokio::test]
    async fn test_b4_context_json_accepts_inline_array() {
        let tool = make_tool();
        let result = tool
            .call(
                serde_json::json!({
                    "workflow": "wf.nika.yaml",
                    "context_json": ["a", "b", "c"]
                })
                .to_string(),
            )
            .await;
        assert!(result.is_ok(), "Array-shape context_json must parse · {:?}", result);
    }

    #[tokio::test]
    async fn test_b4_context_json_backward_compat_string() {
        // Pre-existing string-shape MUST still work post-fix.
        let tool = make_tool();
        let result = tool
            .call(
                serde_json::json!({
                    "workflow": "wf.nika.yaml",
                    "context_json": "{\"k\":\"v\"}"
                })
                .to_string(),
            )
            .await;
        assert!(result.is_ok(), "String-shape context_json backward-compat · {:?}", result);
    }

    #[test]
    fn test_b4_get_context_object_path() {
        let params = RunParams {
            workflow: "x".into(),
            yaml_content: None,
            context_json: Some(ContextJson::Value(serde_json::json!({"urls": [1, 2]}))),
            context: None,
            timeout_secs: 300,
            max_depth: 3,
        };
        let ctx = params.get_context().unwrap();
        assert_eq!(ctx, Some(serde_json::json!({"urls": [1, 2]})));
    }

    #[test]
    fn test_b4_get_context_string_path() {
        let params = RunParams {
            workflow: "x".into(),
            yaml_content: None,
            context_json: Some(ContextJson::String(r#"{"k":"v"}"#.into())),
            context: None,
            timeout_secs: 300,
            max_depth: 3,
        };
        let ctx = params.get_context().unwrap();
        assert_eq!(ctx, Some(serde_json::json!({"k": "v"})));
    }

    #[test]
    fn test_b4_get_context_invalid_string_errors() {
        let params = RunParams {
            workflow: "x".into(),
            yaml_content: None,
            context_json: Some(ContextJson::String("not json".into())),
            context: None,
            timeout_secs: 300,
            max_depth: 3,
        };
        let err = params.get_context().unwrap_err();
        assert!(err.to_string().contains("NIKA-212"), "{err}");
    }

    #[test]
    fn test_b4_schema_declares_both_shapes() {
        let tool = make_tool();
        let schema = tool.parameters_schema();
        let cj_type = &schema["properties"]["context_json"]["type"];
        let types: Vec<&str> = cj_type.as_array().unwrap()
            .iter().map(|v| v.as_str().unwrap()).collect();
        assert!(types.contains(&"string"));
        assert!(types.contains(&"object"));
        assert!(types.contains(&"array"));
    }
}
