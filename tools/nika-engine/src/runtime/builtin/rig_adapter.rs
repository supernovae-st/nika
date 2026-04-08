// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Rig ToolDyn adapter for builtin tools
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
//!         └── NikaBuiltinToolAdapter (builtin tools)
//!               ├── Arc<dyn BuiltinTool>
//!               └── EventLog
//! ```

use std::sync::Arc;

use futures::future::BoxFuture;
use rig::completion::ToolDefinition;
use rig::tool::{ToolDyn, ToolError};

use super::BuiltinTool;
use crate::event::{EventKind, EventLog};

/// Adapter that wraps a BuiltinTool for use with rig-core's agent system.
///
/// This allows builtin tools (nika_sleep, nika_log, etc.) to be called
/// by the LLM during agentic execution.
///
/// Supports EventLog emission for `nika_log` and `nika_emit` tools.
/// Uses `nika_` prefix instead of `nika:` for Anthropic API compatibility.
pub struct NikaBuiltinToolAdapter {
    /// The wrapped builtin tool
    tool: Arc<dyn BuiltinTool>,
    /// Full tool name with nika_ prefix
    full_name: String,
    /// EventLog for emitting events
    event_log: Option<Arc<EventLog>>,
    /// Task ID for event context
    task_id: Option<Arc<str>>,
}

impl NikaBuiltinToolAdapter {
    /// Create a new adapter wrapping a builtin tool.
    ///
    /// # Arguments
    /// * `tool` - The builtin tool to wrap
    pub fn new(tool: Arc<dyn BuiltinTool>) -> Self {
        // Use underscore instead of colon for Anthropic API compatibility
        // Pattern: ^[a-zA-Z0-9_-]{1,128}$ - colon is NOT allowed
        let full_name = format!("nika_{}", tool.name());
        Self {
            tool,
            full_name,
            event_log: None,
            task_id: None,
        }
    }

    /// Set EventLog and task_id for event emission
    ///
    /// When set, `nika:log` will emit `EventKind::Log` and
    /// `nika:emit` will emit `EventKind::Custom` to the EventLog.
    pub fn with_event_log(mut self, event_log: Arc<EventLog>, task_id: Arc<str>) -> Self {
        self.event_log = Some(event_log);
        self.task_id = Some(task_id);
        self
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
        let args_clone = args.clone();
        Box::pin(async move {
            let call_start = std::time::Instant::now();

            // Call the underlying tool
            let result =
                self.tool.call(args_clone.clone()).await.map_err(|e| {
                    ToolError::ToolCallError(Box::new(BuiltinToolError(e.to_string())))
                });

            let success = result.is_ok();
            let duration_ms = call_start.elapsed().as_millis() as u64;

            // EMIT: BuiltinToolInvoked for ALL tools
            if let Some(ref event_log) = self.event_log {
                let tool_name = format!("nika:{}", self.tool.name());
                event_log.emit(EventKind::BuiltinToolInvoked {
                    task_id: self.task_id.clone().unwrap_or_else(|| Arc::from("unknown")),
                    tool_name,
                    duration_ms,
                    success,
                });
            }

            // Emit additional events for specific tools
            if let Some(ref event_log) = self.event_log {
                let is_orchestrator = self
                    .task_id
                    .as_ref()
                    .map(|id| id.as_ref() == crate::runtime::orchestrate::ORCHESTRATOR_TASK_ID)
                    .unwrap_or(false);

                match self.tool.name() {
                    "log" => {
                        if let Ok(ref result_str) = result {
                            if let Ok(response) =
                                serde_json::from_str::<serde_json::Value>(result_str)
                            {
                                let level =
                                    response["level"].as_str().unwrap_or("info").to_string();
                                let message =
                                    response["message"].as_str().unwrap_or("").to_string();
                                event_log.emit(EventKind::Log {
                                    level,
                                    message,
                                    task_id: self.task_id.clone(),
                                });
                            }
                        }
                    }
                    "emit" => {
                        if let Ok(ref result_str) = result {
                            if let Ok(response) =
                                serde_json::from_str::<serde_json::Value>(result_str)
                            {
                                let name =
                                    response["name"].as_str().unwrap_or("unknown").to_string();
                                let payload = response["payload"].clone();
                                event_log.emit(EventKind::Custom {
                                    name,
                                    payload,
                                    task_id: self.task_id.clone(),
                                });
                            }
                        }
                    }
                    // Orchestrator events: only emit when called from __orchestrator__ task
                    "run" if is_orchestrator => {
                        let round = event_log
                            .events()
                            .iter()
                            .filter(|e| {
                                matches!(&e.kind, EventKind::OrchestratorSubWorkflow { .. })
                            })
                            .count() as u32
                            + 1;
                        match &result {
                            Ok(ref result_str) => {
                                let task_count =
                                    serde_json::from_str::<serde_json::Value>(result_str)
                                        .ok()
                                        .and_then(|v| {
                                            v["output"]["result"].as_str().map(|s| s.len())
                                        })
                                        .unwrap_or(0);
                                event_log.emit(EventKind::OrchestratorSubWorkflow {
                                    round,
                                    yaml_hash: format!("round-{}", round),
                                    task_count,
                                });
                            }
                            Err(ref e) => {
                                event_log.emit(EventKind::OrchestratorFailed {
                                    round,
                                    reason: e.to_string(),
                                });
                            }
                        }
                    }
                    // Emit OrchestratorRound when orchestrator calls nika:complete
                    "complete" if is_orchestrator => {
                        let round = event_log
                            .events()
                            .iter()
                            .filter(|e| matches!(&e.kind, EventKind::OrchestratorRound { .. }))
                            .count() as u32
                            + 1;
                        let cost_usd: f64 = event_log
                            .events()
                            .iter()
                            .filter_map(|e| match &e.kind {
                                EventKind::ProviderResponded { cost_usd, .. } => Some(cost_usd),
                                _ => None,
                            })
                            .sum();
                        let records_count = event_log
                            .events()
                            .iter()
                            .filter(|e| matches!(&e.kind, EventKind::TaskCompleted { .. }))
                            .count();
                        event_log.emit(EventKind::OrchestratorRound {
                            round,
                            records_count,
                            cost_usd,
                        });
                    }
                    _ => {}
                }
            }

            result
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
                let params: serde_json::Value =
                    serde_json::from_str(&args).map_err(|e| NikaError::BuiltinToolError {
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
        assert_eq!(adapter.name(), "nika_test");
    }

    #[tokio::test]
    async fn test_adapter_definition() {
        let tool = Arc::new(TestTool);
        let adapter = NikaBuiltinToolAdapter::new(tool);
        let def = adapter.definition("test".to_string()).await;

        assert_eq!(def.name, "nika_test");
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
        assert!(debug_str.contains("nika_test"));
    }

    // Test that the adapter implements Send + Sync (required by rig-core)
    #[test]
    fn test_adapter_send_sync() {
        fn assert_send_sync<T: Send + Sync>() {}
        assert_send_sync::<NikaBuiltinToolAdapter>();
    }

    // =========================================================================
    // Event Emission Tests
    // =========================================================================

    #[tokio::test]
    async fn test_log_tool_emits_event() {
        use super::super::LogTool;
        use rig::tool::ToolDyn;

        let event_log = Arc::new(EventLog::new());
        let task_id: Arc<str> = "test-task-1".into();

        let adapter = NikaBuiltinToolAdapter::new(Arc::new(LogTool))
            .with_event_log(Arc::clone(&event_log), Arc::clone(&task_id));

        // Call the log tool
        let result = adapter
            .call(r#"{"level": "info", "message": "Test log message"}"#.to_string())
            .await;

        assert!(result.is_ok());

        // Verify events were emitted (BuiltinToolInvoked + Log)
        let events = event_log.events();
        assert_eq!(events.len(), 2);

        // First event: BuiltinToolInvoked
        assert!(
            matches!(&events[0].kind, EventKind::BuiltinToolInvoked { tool_name, success, .. } if tool_name == "nika:log" && *success)
        );

        // Second event: Log
        if let EventKind::Log {
            level,
            message,
            task_id: tid,
        } = &events[1].kind
        {
            assert_eq!(level, "info");
            assert_eq!(message, "Test log message");
            assert_eq!(tid.as_ref().map(|t| t.as_ref()), Some("test-task-1"));
        } else {
            panic!("Expected EventKind::Log, got {:?}", events[1].kind);
        }
    }

    #[tokio::test]
    async fn test_emit_tool_emits_custom_event() {
        use super::super::EmitTool;
        use rig::tool::ToolDyn;

        let event_log = Arc::new(EventLog::new());
        let task_id: Arc<str> = "test-task-2".into();

        let adapter = NikaBuiltinToolAdapter::new(Arc::new(EmitTool))
            .with_event_log(Arc::clone(&event_log), Arc::clone(&task_id));

        // Call the emit tool
        let result = adapter
            .call(r#"{"name": "user_action", "payload": {"action": "click"}}"#.to_string())
            .await;

        assert!(result.is_ok());

        // Verify events were emitted (BuiltinToolInvoked + Custom)
        let events = event_log.events();
        assert_eq!(events.len(), 2);

        // First event: BuiltinToolInvoked
        assert!(
            matches!(&events[0].kind, EventKind::BuiltinToolInvoked { tool_name, success, .. } if tool_name == "nika:emit" && *success)
        );

        // Second event: Custom
        if let EventKind::Custom {
            name,
            payload,
            task_id: tid,
        } = &events[1].kind
        {
            assert_eq!(name, "user_action");
            assert_eq!(payload["action"], "click");
            assert_eq!(tid.as_ref().map(|t| t.as_ref()), Some("test-task-2"));
        } else {
            panic!("Expected EventKind::Custom, got {:?}", events[1].kind);
        }
    }

    #[tokio::test]
    async fn test_adapter_without_event_log_does_not_emit() {
        use super::super::LogTool;
        use rig::tool::ToolDyn;

        // Create adapter WITHOUT event_log
        let adapter = NikaBuiltinToolAdapter::new(Arc::new(LogTool));

        // Call should succeed but no events emitted
        let result = adapter
            .call(r#"{"level": "info", "message": "No event expected"}"#.to_string())
            .await;

        assert!(result.is_ok());
        // No event_log, so no events to check - just verify it doesn't panic
    }

    #[test]
    fn test_with_event_log_builder() {
        let event_log = Arc::new(EventLog::new());
        let task_id: Arc<str> = "test-task".into();

        let adapter = NikaBuiltinToolAdapter::new(Arc::new(TestTool))
            .with_event_log(Arc::clone(&event_log), Arc::clone(&task_id));

        // Verify the builder pattern works
        assert_eq!(adapter.name(), "nika_test");
    }
}
