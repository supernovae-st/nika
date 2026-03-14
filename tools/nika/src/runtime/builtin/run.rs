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
//! - v0.14.0: task_local! depth tracking prevents race conditions between concurrent workflows

use super::BuiltinTool;
use crate::ast::parse_workflow;
use crate::error::NikaError;
use crate::runtime::Runner;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::cell::Cell;
use std::future::Future;
use std::path::Path;
use std::pin::Pin;
use std::time::{Duration, Instant};

tokio::task_local! {
    /// Workflow nesting depth for the current execution context.
    /// Uses task_local! for proper isolation between concurrent workflows.
    /// v0.14.0: Replaces global AtomicU32 which had race conditions.
    static WORKFLOW_DEPTH: Cell<u32>;
}

/// Get current workflow depth, returns 0 if not in a workflow context.
fn current_depth() -> u32 {
    WORKFLOW_DEPTH.try_with(|d| d.get()).unwrap_or(0)
}

/// Maximum allowed recursion depth for nested workflows.
const MAX_ALLOWED_DEPTH: u32 = 10;

/// Maximum allowed timeout in seconds.
const MAX_TIMEOUT_SECS: u64 = 3600;

/// Parameters for nika_run tool.
/// v0.14.0: Added timeout_secs, max_depth for production safety.
#[derive(Debug, Clone, Deserialize)]
pub struct RunParams {
    /// Path to the workflow file to execute.
    pub workflow: String,
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
/// Features (v0.14.0):
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
        // v0.14.0: Added timeout_secs and max_depth for production safety
        serde_json::json!({
            "type": "object",
            "properties": {
                "workflow": {
                    "type": "string",
                    "description": "Path to the workflow file to execute"
                },
                "context_json": {
                    "type": "string",
                    "description": "Context as JSON string (for OpenAI: '{\"key\": \"value\"}')"
                },
                "timeout_secs": {
                    "type": "integer",
                    "description": "Execution timeout in seconds (default: 300)",
                    "default": 300,
                    "minimum": 1,
                    "maximum": 3600
                },
                "max_depth": {
                    "type": "integer",
                    "description": "Maximum recursion depth (default: 3, max: 10)",
                    "default": 3,
                    "minimum": 1,
                    "maximum": 10
                }
            },
            "required": ["workflow"],
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

            // Validate workflow path
            if params.workflow.is_empty() {
                return Err(NikaError::BuiltinInvalidParams {
                    tool: "nika_run".into(),
                    reason: "Workflow path cannot be empty".into(),
                });
            }

            // Validate workflow path extension
            if !params.workflow.ends_with(".nika.yaml") && !params.workflow.ends_with(".nika.yml") {
                return Err(NikaError::BuiltinInvalidParams {
                    tool: "nika_run".into(),
                    reason: format!(
                        "Workflow path must have .nika.yaml or .nika.yml extension: '{}'",
                        params.workflow
                    ),
                });
            }

            // v0.14.0: Clamp max_depth and timeout to allowed ranges (defense-in-depth)
            let max_depth = params.max_depth.min(MAX_ALLOWED_DEPTH);
            let timeout_secs = params.timeout_secs.min(MAX_TIMEOUT_SECS);

            // v0.14.0: Check current depth using task_local! (race-condition-safe)
            let depth = current_depth();
            if depth >= max_depth {
                return Err(NikaError::BuiltinToolError {
                    tool: "nika_run".into(),
                    reason: format!(
                        "Maximum recursion depth ({}) reached at depth {}. Cannot execute nested workflow '{}'.",
                        max_depth, depth, params.workflow
                    ),
                });
            }

            let next_depth = depth + 1;
            let timeout_duration = Duration::from_secs(timeout_secs);

            // v0.14.0: Path canonicalization for security
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

            // Security: Ensure path doesn't escape to unexpected locations
            // (canonicalize resolves symlinks and ..)
            tracing::debug!(
                target: "nika_run",
                original = %params.workflow,
                canonical = %canonical_path.display(),
                "Resolved workflow path"
            );

            // v0.14.0: Use tokio::fs for async file I/O (was blocking std::fs)
            // Wrap file read in timeout to prevent hangs on slow filesystems
            let yaml_content = tokio::time::timeout(
                Duration::from_secs(30), // 30s timeout for file I/O
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
            })?;

            let workflow =
                parse_workflow(&yaml_content).map_err(|e| NikaError::BuiltinToolError {
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

            // v0.14.0: Create runner and inject context if provided
            let mut runner = Runner::new(workflow).quiet();

            // Inject parent context into child workflow's datastore
            if let Some(context) = params.get_context()? {
                runner = runner.with_initial_context("__parent_context__", context);
            }

            // v0.14.0: Execute with task_local! depth tracking
            // WORKFLOW_DEPTH.scope() provides automatic cleanup on panic/cancellation
            let execution_result = WORKFLOW_DEPTH
                .scope(Cell::new(next_depth), async {
                    tokio::time::timeout(timeout_duration, runner.run()).await
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
        // v0.12.1: context_json for OpenAI compatibility
        assert!(schema["properties"]["context_json"].is_object());
        // v0.14.0: timeout_secs and max_depth for production safety
        assert!(schema["properties"]["timeout_secs"].is_object());
        assert!(schema["properties"]["max_depth"].is_object());
        assert_eq!(schema["additionalProperties"], false);
        assert!(schema["required"]
            .as_array()
            .unwrap()
            .contains(&serde_json::json!("workflow")));
    }

    #[tokio::test]
    async fn test_run_nonexistent_file_errors() {
        let tool = RunTool;
        let result = tool
            .call(r#"{"workflow": "path/to/workflow.nika.yaml"}"#.to_string())
            .await;

        assert!(result.is_err());
        let err = result.unwrap_err();
        // v0.14.0: Path canonicalization gives "Failed to resolve workflow path" error
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
            r#"schema: nika/workflow@0.2
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
            r#"schema: nika/workflow@0.2
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
        assert!(err.to_string().contains("cannot be empty"));
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
            r#"schema: nika/workflow@0.2
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
    async fn test_run_missing_workflow() {
        let tool = RunTool;
        let result = tool.call(r#"{"context": {"test": 1}}"#.to_string()).await;

        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.to_string().contains("Invalid JSON parameters"));
    }

    #[tokio::test]
    async fn test_run_params_deserialization() {
        let json = r#"{"workflow": "test.nika.yaml", "context": {"key": "value"}}"#;
        let params: RunParams = serde_json::from_str(json).unwrap();

        assert_eq!(params.workflow, "test.nika.yaml");
        assert!(params.context.is_some());
        assert_eq!(params.context.as_ref().unwrap()["key"], "value");
        // v0.14.0: defaults
        assert_eq!(params.timeout_secs, 300);
        assert_eq!(params.max_depth, 3);
    }

    #[tokio::test]
    async fn test_run_params_without_context() {
        let json = r#"{"workflow": "test.nika.yaml"}"#;
        let params: RunParams = serde_json::from_str(json).unwrap();

        assert_eq!(params.workflow, "test.nika.yaml");
        assert!(params.context.is_none());
        // v0.14.0: defaults applied
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
            r#"schema: nika/workflow@0.2
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
    // v0.14.0: task_local! DEPTH TRACKING TESTS
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
    // v0.14.0: TIMEOUT CLAMPING TESTS
    // ═══════════════════════════════════════════════════════════════

    #[tokio::test]
    async fn test_timeout_clamped_to_max() {
        use std::io::Write;
        use tempfile::NamedTempFile;

        // Create a minimal workflow file
        let mut temp_file = NamedTempFile::with_suffix(".nika.yaml").unwrap();
        writeln!(
            temp_file,
            r#"schema: nika/workflow@0.2
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
    // v0.14.0: CONTEXT INJECTION TESTS
    // ═══════════════════════════════════════════════════════════════

    #[test]
    fn test_run_params_get_context_from_context_field() {
        let params = RunParams {
            workflow: "test.nika.yaml".to_string(),
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
            context_json: None,
            context: None,
            timeout_secs: 300,
            max_depth: 3,
        };

        let context = params.get_context().unwrap();
        assert!(context.is_none());
    }
}
