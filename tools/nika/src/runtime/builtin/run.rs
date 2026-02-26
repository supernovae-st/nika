//! nika:run - Execute nested workflow.
//!
//! # Parameters
//!
//! ```json
//! {
//!   "workflow": "path/to/workflow.nika.yaml",  // Path to workflow file
//!   "context": { ... }                          // Context to pass (optional)
//! }
//! ```
//!
//! # Returns
//!
//! ```json
//! {
//!   "executed": true,
//!   "workflow": "path/to/workflow.nika.yaml",
//!   "output": { ... }
//! }
//! ```
//!
//! # Note
//!
//! This tool requires runtime integration with the workflow executor.
//! Full execution support will be added in Task 3.9 (Integrate Router with Executor).

use super::BuiltinTool;
use crate::ast::Workflow;
use crate::error::NikaError;
use crate::runtime::Runner;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::future::Future;
use std::path::Path;
use std::pin::Pin;

/// Parameters for nika:run tool.
/// v0.12.1: Supports both `context` (JSON) and `context_json` (string) for OpenAI compatibility.
#[derive(Debug, Clone, Deserialize)]
pub struct RunParams {
    /// Path to the workflow file to execute.
    pub workflow: String,
    /// Context as JSON string (for OpenAI strict mode).
    #[serde(default)]
    pub context_json: Option<String>,
    /// Context to pass to the nested workflow (optional).
    /// Deprecated: Use context_json for OpenAI compatibility.
    #[serde(default)]
    pub context: Option<Value>,
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

/// Response from nika:run tool.
#[derive(Debug, Clone, Serialize)]
pub struct RunResponse {
    /// Whether the workflow was executed.
    pub executed: bool,
    /// Path to the workflow that was executed.
    pub workflow: String,
    /// Output from the nested workflow.
    pub output: Value,
}

/// nika:run builtin tool.
///
/// Executes a nested workflow and returns its output.
/// Useful for workflow composition and modular design.
///
/// Full execution support requires runtime integration with the
/// workflow executor, which will be added in Task 3.9.
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
        // v0.12.1: OpenAI-compatible schema with additionalProperties: false
        // context_json is used for OpenAI strict mode (JSON string)
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
            // Parse parameters
            let params: RunParams =
                serde_json::from_str(&args).map_err(|e| NikaError::BuiltinInvalidParams {
                    tool: "nika:run".into(),
                    reason: format!("Invalid JSON parameters: {}", e),
                })?;

            // Validate workflow path
            if params.workflow.is_empty() {
                return Err(NikaError::BuiltinInvalidParams {
                    tool: "nika:run".into(),
                    reason: "Workflow path cannot be empty".into(),
                });
            }

            // Validate workflow path extension
            if !params.workflow.ends_with(".nika.yaml") && !params.workflow.ends_with(".nika.yml") {
                return Err(NikaError::BuiltinInvalidParams {
                    tool: "nika:run".into(),
                    reason: format!(
                        "Workflow path must have .nika.yaml or .nika.yml extension: '{}'",
                        params.workflow
                    ),
                });
            }

            // v0.10.0: Full workflow execution via Runner
            let workflow_path = Path::new(&params.workflow);

            // Check if file exists
            if !workflow_path.exists() {
                return Err(NikaError::BuiltinToolError {
                    tool: "nika:run".into(),
                    reason: format!("Workflow file not found: '{}'", params.workflow),
                });
            }

            // Read and parse workflow YAML
            let yaml_content = std::fs::read_to_string(workflow_path).map_err(|e| {
                NikaError::BuiltinToolError {
                    tool: "nika:run".into(),
                    reason: format!("Failed to read workflow file: {}", e),
                }
            })?;

            let workflow: Workflow =
                serde_yaml::from_str(&yaml_content).map_err(|e| NikaError::BuiltinToolError {
                    tool: "nika:run".into(),
                    reason: format!("Failed to parse workflow YAML: {}", e),
                })?;

            tracing::info!(
                target: "nika:run",
                workflow = %params.workflow,
                has_context = params.context.is_some(),
                "Executing nested workflow"
            );

            // Create Runner and execute workflow
            let runner = Runner::new(workflow).quiet();
            let result = runner
                .run()
                .await
                .map_err(|e| NikaError::BuiltinToolError {
                    tool: "nika:run".into(),
                    reason: format!("Workflow execution failed: {}", e),
                })?;

            // Build response with workflow output
            // Runner::run() returns a String (final task output or summary)
            let response = RunResponse {
                executed: true,
                workflow: params.workflow,
                output: serde_json::json!({
                    "status": "completed",
                    "result": result
                }),
            };

            serde_json::to_string(&response).map_err(|e| NikaError::BuiltinToolError {
                tool: "nika:run".into(),
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
        assert!(err.to_string().contains("not found"));
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
    }

    #[tokio::test]
    async fn test_run_params_without_context() {
        let json = r#"{"workflow": "test.nika.yaml"}"#;
        let params: RunParams = serde_json::from_str(json).unwrap();

        assert_eq!(params.workflow, "test.nika.yaml");
        assert!(params.context.is_none());
    }
}
