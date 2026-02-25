//! Rig ToolDyn adapter for builtin tools (v0.9.3)
//!
//! Wraps BuiltinTool implementations as rig-core ToolDyn for use in RigAgentLoop.
//!
//! # Architecture
//!
//! ```text
//! RigAgentLoop
//!   └── tools: Vec<Box<dyn ToolDyn>>
//!         ├── NikaMcpTool (MCP tools)
//!         ├── SpawnAgentTool
//!         └── NikaBuiltinToolAdapter (builtin tools) ← NEW
//!               └── Arc<dyn BuiltinTool>
//! ```

use std::sync::Arc;

use futures::future::BoxFuture;
use rig::completion::ToolDefinition;
use rig::tool::{ToolDyn, ToolError};

use super::BuiltinTool;

/// Adapter that wraps a BuiltinTool for use with rig-core's agent system.
///
/// This allows builtin tools (nika:sleep, nika:log, etc.) to be called
/// by the LLM during agentic execution.
pub struct NikaBuiltinToolAdapter {
    /// The wrapped builtin tool
    tool: Arc<dyn BuiltinTool>,
    /// Full tool name with nika: prefix
    full_name: String,
}

impl NikaBuiltinToolAdapter {
    /// Create a new adapter wrapping a builtin tool.
    ///
    /// # Arguments
    /// * `tool` - The builtin tool to wrap
    pub fn new(tool: Arc<dyn BuiltinTool>) -> Self {
        let full_name = format!("nika:{}", tool.name());
        Self { tool, full_name }
    }
}

impl std::fmt::Debug for NikaBuiltinToolAdapter {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("NikaBuiltinToolAdapter")
            .field("name", &self.full_name)
            .finish()
    }
}

impl ToolDyn for NikaBuiltinToolAdapter {
    fn name(&self) -> String {
        self.full_name.clone()
    }

    fn definition(&self, _prompt: String) -> BoxFuture<'_, ToolDefinition> {
        let def = ToolDefinition {
            name: self.full_name.clone(),
            description: self.tool.description().to_string(),
            parameters: self.tool.parameters_schema(),
        };
        Box::pin(async move { def })
    }

    fn call(&self, args: String) -> BoxFuture<'_, Result<String, ToolError>> {
        Box::pin(async move {
            self.tool
                .call(args)
                .await
                .map_err(|e| ToolError::ToolCallError(Box::new(BuiltinToolError(e.to_string()))))
        })
    }
}

/// Error type for builtin tool failures, compatible with ToolError.
#[derive(Debug)]
struct BuiltinToolError(String);

impl std::fmt::Display for BuiltinToolError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl std::error::Error for BuiltinToolError {}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::error::NikaError;

    struct TestTool;

    impl BuiltinTool for TestTool {
        fn name(&self) -> &'static str {
            "test"
        }

        fn description(&self) -> &'static str {
            "A test tool for unit tests"
        }

        fn parameters_schema(&self) -> serde_json::Value {
            serde_json::json!({
                "type": "object",
                "properties": {
                    "value": {"type": "string"}
                },
                "required": ["value"]
            })
        }

        fn call<'a>(
            &'a self,
            args: String,
        ) -> std::pin::Pin<
            Box<dyn std::future::Future<Output = Result<String, NikaError>> + Send + 'a>,
        > {
            Box::pin(async move {
                let params: serde_json::Value = serde_json::from_str(&args)
                    .map_err(|e| NikaError::BuiltinToolError {
                        tool: "test".into(),
                        reason: format!("Invalid JSON: {}", e),
                    })?;
                let value = params["value"].as_str().unwrap_or("");
                Ok(format!(r#"{{"received":"{}"}}"#, value))
            })
        }
    }

    #[test]
    fn test_adapter_name() {
        let tool = Arc::new(TestTool);
        let adapter = NikaBuiltinToolAdapter::new(tool);
        assert_eq!(adapter.name(), "nika:test");
    }

    #[tokio::test]
    async fn test_adapter_definition() {
        let tool = Arc::new(TestTool);
        let adapter = NikaBuiltinToolAdapter::new(tool);
        let def = adapter.definition("test".to_string()).await;

        assert_eq!(def.name, "nika:test");
        assert_eq!(def.description, "A test tool for unit tests");
        assert_eq!(
            def.parameters,
            serde_json::json!({
                "type": "object",
                "properties": {
                    "value": {"type": "string"}
                },
                "required": ["value"]
            })
        );
    }

    #[tokio::test]
    async fn test_adapter_call_success() {
        let tool = Arc::new(TestTool);
        let adapter = NikaBuiltinToolAdapter::new(tool);

        let result = adapter.call(r#"{"value": "hello"}"#.to_string()).await;

        assert!(result.is_ok());
        assert_eq!(result.unwrap(), r#"{"received":"hello"}"#);
    }

    #[tokio::test]
    async fn test_adapter_call_invalid_json() {
        let tool = Arc::new(TestTool);
        let adapter = NikaBuiltinToolAdapter::new(tool);

        let result = adapter.call("not json".to_string()).await;

        assert!(result.is_err());
    }

    #[test]
    fn test_adapter_debug() {
        let tool = Arc::new(TestTool);
        let adapter = NikaBuiltinToolAdapter::new(tool);
        let debug_str = format!("{:?}", adapter);
        assert!(debug_str.contains("nika:test"));
    }

    // Test that the adapter implements Send + Sync (required by rig-core)
    #[test]
    fn test_adapter_send_sync() {
        fn assert_send_sync<T: Send + Sync>() {}
        assert_send_sync::<NikaBuiltinToolAdapter>();
    }
}
