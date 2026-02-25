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
use crate::error::NikaError;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::future::Future;
use std::pin::Pin;

/// Parameters for nika:run tool.
#[derive(Debug, Clone, Deserialize)]
pub struct RunParams {
    /// Path to the workflow file to execute.
    pub workflow: String,
    /// Context to pass to the nested workflow (optional).
    #[serde(default)]
    pub context: Option<Value>,
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
        serde_json::json!({
            "type": "object",
            "properties": {
                "workflow": {
                    "type": "string",
                    "description": "Path to the workflow file to execute"
                },
                "context": {
                    "type": "object",
                    "description": "Context to pass to the nested workflow"
                }
            },
            "required": ["workflow"]
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

            // TODO: Full workflow execution integration (Task 3.9)
            // For now, return a placeholder response indicating the tool was called
            // The actual Runner integration will be done when the Router is connected
            // to the Executor.

            tracing::info!(
                target: "nika:run",
                workflow = %params.workflow,
                has_context = params.context.is_some(),
                "Workflow execution requested (integration pending)"
            );

            // Return placeholder response
            // In production, this will execute the workflow and return its output
            let response = RunResponse {
                executed: false, // Will be true when integration is complete
                workflow: params.workflow,
                output: serde_json::json!({
                    "status": "pending_integration",
                    "message": "Full workflow execution will be available after Task 3.9"
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
        let tool = RunTool::default();
        assert_eq!(tool.name(), "run");
    }

    #[test]
    fn test_run_tool_description() {
        let tool = RunTool::default();
        assert!(tool.description().contains("workflow"));
    }

    #[test]
    fn test_run_tool_schema() {
        let tool = RunTool::default();
        let schema = tool.parameters_schema();
        assert_eq!(schema["type"], "object");
        assert!(schema["properties"]["workflow"].is_object());
        assert!(schema["properties"]["context"].is_object());
        assert!(schema["required"]
            .as_array()
            .unwrap()
            .contains(&serde_json::json!("workflow")));
    }

    #[tokio::test]
    async fn test_run_valid_workflow_path() {
        let tool = RunTool::default();
        let result = tool
            .call(r#"{"workflow": "path/to/workflow.nika.yaml"}"#.to_string())
            .await;

        assert!(result.is_ok());
        let response: serde_json::Value = serde_json::from_str(&result.unwrap()).unwrap();
        assert_eq!(response["workflow"], "path/to/workflow.nika.yaml");
        // Currently returns executed: false (pending integration)
        assert_eq!(response["executed"], false);
    }

    #[tokio::test]
    async fn test_run_with_context() {
        let tool = RunTool::default();
        let result = tool
            .call(
                r#"{"workflow": "test.nika.yaml", "context": {"key": "value", "count": 42}}"#
                    .to_string(),
            )
            .await;

        assert!(result.is_ok());
        let response: serde_json::Value = serde_json::from_str(&result.unwrap()).unwrap();
        assert_eq!(response["workflow"], "test.nika.yaml");
    }

    #[tokio::test]
    async fn test_run_empty_path_errors() {
        let tool = RunTool::default();
        let result = tool
            .call(r#"{"workflow": ""}"#.to_string())
            .await;

        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.to_string().contains("cannot be empty"));
    }

    #[tokio::test]
    async fn test_run_invalid_extension_errors() {
        let tool = RunTool::default();
        let result = tool
            .call(r#"{"workflow": "workflow.yaml"}"#.to_string())
            .await;

        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.to_string().contains(".nika.yaml"));
    }

    #[tokio::test]
    async fn test_run_accepts_yml_extension() {
        let tool = RunTool::default();
        let result = tool
            .call(r#"{"workflow": "workflow.nika.yml"}"#.to_string())
            .await;

        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_run_invalid_json() {
        let tool = RunTool::default();
        let result = tool.call("not json".to_string()).await;

        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.to_string().contains("Invalid JSON parameters"));
    }

    #[tokio::test]
    async fn test_run_missing_workflow() {
        let tool = RunTool::default();
        let result = tool
            .call(r#"{"context": {"test": 1}}"#.to_string())
            .await;

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
