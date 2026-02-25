//! BuiltinToolRouter for nika:* tool dispatch.

use super::{AssertTool, BuiltinTool, EmitTool, LogTool, PromptTool, RunTool, SleepTool};
use crate::error::NikaError;
use std::collections::HashMap;
use std::sync::Arc;

/// Router for builtin nika:* tools.
///
/// Dispatches tool calls to appropriate builtin implementations based on
/// the nika: prefix.
///
/// # Example
///
/// ```ignore
/// let router = BuiltinToolRouter::new();
///
/// // Check if tool is builtin
/// if BuiltinToolRouter::is_builtin("nika:sleep") {
///     let result = router.dispatch("nika:sleep", r#"{"duration":"1s"}"#).await?;
/// }
/// ```
pub struct BuiltinToolRouter {
    tools: HashMap<&'static str, Arc<dyn BuiltinTool>>,
}

impl BuiltinToolRouter {
    /// Create a new router with all 6 builtin tools registered.
    pub fn new() -> Self {
        let mut tools: HashMap<&'static str, Arc<dyn BuiltinTool>> = HashMap::new();

        // Register all 6 builtin tools
        tools.insert("sleep", Arc::new(SleepTool));
        tools.insert("log", Arc::new(LogTool));
        tools.insert("emit", Arc::new(EmitTool));
        tools.insert("assert", Arc::new(AssertTool));
        tools.insert("prompt", Arc::new(PromptTool::default()));
        tools.insert("run", Arc::new(RunTool));

        Self { tools }
    }

    /// Check if a tool name is a builtin (has nika: prefix).
    ///
    /// # Example
    /// ```ignore
    /// assert!(BuiltinToolRouter::is_builtin("nika:sleep"));
    /// assert!(!BuiltinToolRouter::is_builtin("novanet:describe"));
    /// ```
    #[inline]
    pub fn is_builtin(tool_name: &str) -> bool {
        tool_name.starts_with("nika:")
    }

    /// Extract the tool name from a nika: prefixed string.
    ///
    /// Returns None if the string doesn't start with "nika:".
    ///
    /// # Example
    /// ```ignore
    /// assert_eq!(BuiltinToolRouter::extract_name("nika:sleep"), Some("sleep"));
    /// assert_eq!(BuiltinToolRouter::extract_name("novanet:x"), None);
    /// ```
    #[inline]
    pub fn extract_name(tool_name: &str) -> Option<&str> {
        tool_name.strip_prefix("nika:")
    }

    /// Check if the router has a specific tool registered.
    pub fn has_tool(&self, name: &str) -> bool {
        self.tools.contains_key(name)
    }

    /// Get all registered tool names.
    pub fn tool_names(&self) -> Vec<&'static str> {
        self.tools.keys().copied().collect()
    }

    /// Register a builtin tool.
    pub fn register<T: BuiltinTool + 'static>(&mut self, tool: T) {
        self.tools.insert(tool.name(), Arc::new(tool));
    }

    /// Dispatch a tool call to the appropriate builtin tool.
    ///
    /// # Arguments
    /// * `tool_name` - Full tool name with nika: prefix (e.g., "nika:sleep")
    /// * `args` - JSON-encoded arguments
    ///
    /// # Returns
    /// * `Ok(String)` - JSON-encoded result from the tool
    /// * `Err(NikaError)` - If tool not found or execution fails
    pub async fn dispatch(&self, tool_name: &str, args: String) -> Result<String, NikaError> {
        let name = Self::extract_name(tool_name).ok_or_else(|| NikaError::BuiltinToolError {
            tool: tool_name.into(),
            reason: "Not a builtin tool (missing nika: prefix)".into(),
        })?;

        let tool = self
            .tools
            .get(name)
            .ok_or_else(|| NikaError::BuiltinToolError {
                tool: tool_name.into(),
                reason: format!("Unknown builtin tool: {}", name),
            })?;

        tool.call(args).await
    }
}

impl Default for BuiltinToolRouter {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_router_is_builtin() {
        assert!(BuiltinToolRouter::is_builtin("nika:sleep"));
        assert!(BuiltinToolRouter::is_builtin("nika:log"));
        assert!(BuiltinToolRouter::is_builtin("nika:emit"));
        assert!(BuiltinToolRouter::is_builtin("nika:assert"));
        assert!(BuiltinToolRouter::is_builtin("nika:prompt"));
        assert!(BuiltinToolRouter::is_builtin("nika:run"));
        assert!(!BuiltinToolRouter::is_builtin("novanet:describe"));
        assert!(!BuiltinToolRouter::is_builtin("sleep"));
        assert!(!BuiltinToolRouter::is_builtin(""));
    }

    #[test]
    fn test_router_extract_name() {
        assert_eq!(BuiltinToolRouter::extract_name("nika:sleep"), Some("sleep"));
        assert_eq!(BuiltinToolRouter::extract_name("nika:log"), Some("log"));
        assert_eq!(BuiltinToolRouter::extract_name("nika:emit"), Some("emit"));
        assert_eq!(
            BuiltinToolRouter::extract_name("nika:assert"),
            Some("assert")
        );
        assert_eq!(
            BuiltinToolRouter::extract_name("nika:prompt"),
            Some("prompt")
        );
        assert_eq!(BuiltinToolRouter::extract_name("nika:run"), Some("run"));
        assert_eq!(BuiltinToolRouter::extract_name("novanet:x"), None);
        assert_eq!(BuiltinToolRouter::extract_name("sleep"), None);
        assert_eq!(BuiltinToolRouter::extract_name(""), None);
    }

    #[test]
    fn test_router_new_has_all_tools() {
        let router = BuiltinToolRouter::new();
        assert!(router.has_tool("sleep"));
        assert!(router.has_tool("log"));
        assert!(router.has_tool("emit"));
        assert!(router.has_tool("assert"));
        assert!(router.has_tool("prompt"));
        assert!(router.has_tool("run"));
        assert_eq!(router.tool_names().len(), 6);
    }

    #[test]
    fn test_router_register_tool() {
        struct TestTool;

        impl BuiltinTool for TestTool {
            fn name(&self) -> &'static str {
                "test"
            }

            fn call<'a>(
                &'a self,
                _args: String,
            ) -> std::pin::Pin<
                Box<dyn std::future::Future<Output = Result<String, NikaError>> + Send + 'a>,
            > {
                Box::pin(async { Ok("test result".to_string()) })
            }
        }

        let mut router = BuiltinToolRouter::new();
        router.register(TestTool);

        assert!(router.has_tool("test"));
        assert!(!router.has_tool("unknown"));
    }

    #[tokio::test]
    async fn test_router_dispatch_registered_tool() {
        struct TestTool;

        impl BuiltinTool for TestTool {
            fn name(&self) -> &'static str {
                "test"
            }

            fn call<'a>(
                &'a self,
                args: String,
            ) -> std::pin::Pin<
                Box<dyn std::future::Future<Output = Result<String, NikaError>> + Send + 'a>,
            > {
                Box::pin(async move { Ok(format!("received: {}", args)) })
            }
        }

        let mut router = BuiltinToolRouter::new();
        router.register(TestTool);

        let result = router
            .dispatch("nika:test", r#"{"hello":"world"}"#.to_string())
            .await;

        assert!(result.is_ok());
        assert_eq!(result.unwrap(), r#"received: {"hello":"world"}"#);
    }

    #[tokio::test]
    async fn test_router_dispatch_unknown_tool() {
        let router = BuiltinToolRouter::new();

        let result = router.dispatch("nika:unknown", "{}".to_string()).await;

        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.to_string().contains("Unknown builtin tool"));
    }

    #[tokio::test]
    async fn test_router_dispatch_not_builtin() {
        let router = BuiltinToolRouter::new();

        let result = router.dispatch("novanet:describe", "{}".to_string()).await;

        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.to_string().contains("Not a builtin tool"));
    }

    #[test]
    fn test_router_default() {
        let router = BuiltinToolRouter::default();
        // Default router has all 6 tools
        assert_eq!(router.tool_names().len(), 6);
    }

    #[tokio::test]
    async fn test_router_dispatch_sleep() {
        let router = BuiltinToolRouter::new();
        let result = router
            .dispatch("nika:sleep", r#"{"duration":"1ms"}"#.to_string())
            .await;

        assert!(result.is_ok());
        let response: serde_json::Value = serde_json::from_str(&result.unwrap()).unwrap();
        assert_eq!(response["slept_for_ms"], 1);
    }

    #[tokio::test]
    async fn test_router_dispatch_log() {
        let router = BuiltinToolRouter::new();
        let result = router
            .dispatch(
                "nika:log",
                r#"{"level":"info","message":"test"}"#.to_string(),
            )
            .await;

        assert!(result.is_ok());
        let response: serde_json::Value = serde_json::from_str(&result.unwrap()).unwrap();
        assert_eq!(response["logged"], true);
    }

    #[tokio::test]
    async fn test_router_dispatch_emit() {
        let router = BuiltinToolRouter::new();
        let result = router
            .dispatch(
                "nika:emit",
                r#"{"name":"test_event","payload":{}}"#.to_string(),
            )
            .await;

        assert!(result.is_ok());
        let response: serde_json::Value = serde_json::from_str(&result.unwrap()).unwrap();
        assert_eq!(response["emitted"], true);
    }

    #[tokio::test]
    async fn test_router_dispatch_assert_true() {
        let router = BuiltinToolRouter::new();
        let result = router
            .dispatch("nika:assert", r#"{"condition":true}"#.to_string())
            .await;

        assert!(result.is_ok());
        let response: serde_json::Value = serde_json::from_str(&result.unwrap()).unwrap();
        assert_eq!(response["passed"], true);
    }

    #[tokio::test]
    async fn test_router_dispatch_assert_false() {
        let router = BuiltinToolRouter::new();
        let result = router
            .dispatch("nika:assert", r#"{"condition":false}"#.to_string())
            .await;

        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.to_string().contains("Assertion failed"));
    }

    #[tokio::test]
    async fn test_router_dispatch_prompt_headless() {
        let router = BuiltinToolRouter::new();
        // In headless mode with default, should use default
        let result = router
            .dispatch(
                "nika:prompt",
                r#"{"message":"Test?","default":"yes"}"#.to_string(),
            )
            .await;

        assert!(result.is_ok());
        let response: serde_json::Value = serde_json::from_str(&result.unwrap()).unwrap();
        assert_eq!(response["response"], "yes");
        assert_eq!(response["default_used"], true);
    }

    #[tokio::test]
    async fn test_router_dispatch_run_placeholder() {
        let router = BuiltinToolRouter::new();
        let result = router
            .dispatch("nika:run", r#"{"workflow":"test.nika.yaml"}"#.to_string())
            .await;

        assert!(result.is_ok());
        let response: serde_json::Value = serde_json::from_str(&result.unwrap()).unwrap();
        assert_eq!(response["workflow"], "test.nika.yaml");
        // Placeholder returns executed: false until integrated
        assert_eq!(response["executed"], false);
    }
}
