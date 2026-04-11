// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! `MockMcpPool` — programmable MCP pool for tests.
//!
//! Unlike the production adapter in `nika-engine`, `MockMcpPool` does not
//! spawn MCP servers or talk rmcp. It routes calls by server name to a
//! programmable queue of canned responses. Use the fixture constructors
//! (`happy`, `cancelled`, `error`, `oversized`) for common scenarios.

use std::collections::{HashMap, VecDeque};
use std::sync::Arc;

use async_trait::async_trait;
use nika_core::mcp::ContentBlock;
use parking_lot::Mutex;
use tokio_util::sync::CancellationToken;

use nika_kernel::mcp::{
    McpCallOptions, McpError, McpPool, McpResourceContent, McpToolDescriptor, McpToolResult,
    MAX_MCP_RESULT_SIZE,
};

/// Canned tool-call response — success or error.
pub type ToolCallResponse = Result<McpToolResult, McpError>;

/// Canned resource-read response.
pub type ResourceResponse = Result<McpResourceContent, McpError>;

/// Per-server programmable state.
#[derive(Default)]
struct ServerState {
    /// FIFO queue of tool-call responses, keyed by tool name. When the
    /// queue for a tool is empty, subsequent calls return `ServerNotFound`.
    tool_queues: HashMap<String, VecDeque<ToolCallResponse>>,
    /// Map of URI → resource-read response.
    resources: HashMap<String, ResourceResponse>,
    /// `list_tools` returns this descriptor list verbatim.
    descriptors: Vec<McpToolDescriptor>,
    /// If `true`, `call_tool` also records incoming calls for assertions.
    record_calls: bool,
}

/// Observed tool call (for assertions).
#[derive(Debug, Clone)]
pub struct RecordedCall {
    pub server: String,
    pub tool: String,
    pub args: serde_json::Value,
    pub task_id: Arc<str>,
}

/// Programmable MCP pool for tests.
///
/// Routes `(server, tool)` to a FIFO queue of canned responses. Cancellation
/// is honored: if `opts.cancel.is_cancelled()` is true at entry, the mock
/// returns `McpError::Cancelled` without consuming a queued response.
#[derive(Clone, Default)]
pub struct MockMcpPool {
    servers: Arc<Mutex<HashMap<String, ServerState>>>,
    calls: Arc<Mutex<Vec<RecordedCall>>>,
}

impl MockMcpPool {
    pub fn new() -> Self {
        Self::default()
    }

    /// Register a server with an empty queue. Subsequent `has_server`
    /// returns `true` for this name.
    pub fn register_server(&self, server: impl Into<String>) -> &Self {
        self.servers.lock().entry(server.into()).or_default();
        self
    }

    /// Enqueue a tool-call result for `(server, tool)`. Creates the server
    /// entry if missing.
    pub fn enqueue_tool_result(
        &self,
        server: impl Into<String>,
        tool: impl Into<String>,
        response: ToolCallResponse,
    ) -> &Self {
        let server = server.into();
        let tool = tool.into();
        let mut guard = self.servers.lock();
        let state = guard.entry(server).or_default();
        state.tool_queues.entry(tool).or_default().push_back(response);
        self
    }

    /// Enqueue a successful text-only response for `(server, tool)`.
    pub fn enqueue_text_ok(
        &self,
        server: impl Into<String>,
        tool: impl Into<String>,
        text: impl Into<String>,
    ) -> &Self {
        let result = McpToolResult::new(vec![ContentBlock::text(text)], false, false);
        self.enqueue_tool_result(server, tool, Ok(result))
    }

    /// Register a resource-read response for `(server, uri)`.
    pub fn register_resource(
        &self,
        server: impl Into<String>,
        uri: impl Into<String>,
        response: ResourceResponse,
    ) -> &Self {
        let server = server.into();
        let mut guard = self.servers.lock();
        let state = guard.entry(server).or_default();
        state.resources.insert(uri.into(), response);
        self
    }

    /// Set the descriptor list returned by `list_tools(server)`.
    pub fn set_descriptors(
        &self,
        server: impl Into<String>,
        descriptors: Vec<McpToolDescriptor>,
    ) -> &Self {
        let server = server.into();
        let mut guard = self.servers.lock();
        guard.entry(server).or_default().descriptors = descriptors;
        self
    }

    /// Enable call recording. Recorded calls are retrievable via
    /// [`recorded_calls`].
    pub fn record_calls(&self, server: impl Into<String>) -> &Self {
        let server = server.into();
        let mut guard = self.servers.lock();
        guard.entry(server).or_default().record_calls = true;
        self
    }

    /// Return a snapshot of all recorded calls (empty unless `record_calls`
    /// was enabled on the relevant server).
    pub fn recorded_calls(&self) -> Vec<RecordedCall> {
        self.calls.lock().clone()
    }

    // ── Fixture constructors ───────────────────────────────────────────

    /// Fixture: happy path. Server `mock` registered, `test_tool` returns
    /// `"ok"` text. Useful for the 80% case.
    pub fn happy() -> Self {
        let pool = Self::new();
        pool.enqueue_text_ok("mock", "test_tool", "ok");
        pool
    }

    /// Fixture: error path. Server `mock` registered, `test_tool` returns
    /// `ToolCallFailed`.
    pub fn error() -> Self {
        let pool = Self::new();
        pool.enqueue_tool_result(
            "mock",
            "test_tool",
            Err(McpError::ToolCallFailed {
                server: "mock".to_string(),
                tool: "test_tool".to_string(),
                reason: "mock error fixture".to_string(),
            }),
        );
        pool
    }

    /// Fixture: oversized path. Server `mock` registered, `test_tool`
    /// returns an `McpToolResult` whose `content_size_bytes` exceeds
    /// `MAX_MCP_RESULT_SIZE`. The adapter layer would normally reject this
    /// with `ResultTooLarge` before it reaches the verb — but the mock
    /// does not enforce the cap, so consumers can assert how their own
    /// guards react.
    pub fn oversized() -> Self {
        let pool = Self::new();
        // Build a 51 MB text block (slightly above the 50 MB cap).
        let huge = "x".repeat(MAX_MCP_RESULT_SIZE + 1_048_576);
        let result = McpToolResult::new(vec![ContentBlock::text(huge)], false, false);
        pool.enqueue_tool_result("mock", "test_tool", Ok(result));
        pool
    }
}

#[async_trait]
impl McpPool for MockMcpPool {
    async fn call_tool(
        &self,
        server: &str,
        tool: &str,
        args: serde_json::Value,
        opts: McpCallOptions,
    ) -> Result<McpToolResult, McpError> {
        // Honor cancellation before touching the queue.
        if opts.cancel.is_cancelled() {
            return Err(McpError::Cancelled {
                server: server.to_string(),
                tool: tool.to_string(),
            });
        }

        // Pop a response, optionally recording the call first.
        let mut guard = self.servers.lock();
        let state = guard.get_mut(server).ok_or_else(|| McpError::ServerNotFound {
            server: server.to_string(),
        })?;

        if state.record_calls {
            self.calls.lock().push(RecordedCall {
                server: server.to_string(),
                tool: tool.to_string(),
                args: args.clone(),
                task_id: Arc::clone(&opts.task_id),
            });
        }

        let queue = state
            .tool_queues
            .get_mut(tool)
            .ok_or_else(|| McpError::ToolCallFailed {
                server: server.to_string(),
                tool: tool.to_string(),
                reason: "no canned response for this tool".to_string(),
            })?;

        queue.pop_front().unwrap_or_else(|| {
            Err(McpError::ToolCallFailed {
                server: server.to_string(),
                tool: tool.to_string(),
                reason: "MockMcpPool: response queue exhausted".to_string(),
            })
        })
    }

    async fn read_resource(
        &self,
        server: &str,
        uri: &str,
        cancel: &CancellationToken,
    ) -> Result<McpResourceContent, McpError> {
        if cancel.is_cancelled() {
            return Err(McpError::Cancelled {
                server: server.to_string(),
                tool: format!("resource:{uri}"),
            });
        }

        let guard = self.servers.lock();
        let state = guard.get(server).ok_or_else(|| McpError::ServerNotFound {
            server: server.to_string(),
        })?;

        state
            .resources
            .get(uri)
            .cloned()
            .unwrap_or_else(|| {
                Err(McpError::ResourceFailed {
                    uri: uri.to_string(),
                    reason: "MockMcpPool: no canned resource".to_string(),
                })
            })
    }

    async fn list_tools(&self, server: &str) -> Result<Vec<McpToolDescriptor>, McpError> {
        let guard = self.servers.lock();
        let state = guard.get(server).ok_or_else(|| McpError::ServerNotFound {
            server: server.to_string(),
        })?;
        Ok(state.descriptors.clone())
    }

    fn has_server(&self, server: &str) -> bool {
        self.servers.lock().contains_key(server)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn opts() -> McpCallOptions {
        McpCallOptions::new(Arc::from("test-task"), CancellationToken::new())
    }

    #[tokio::test]
    async fn happy_fixture_returns_text() {
        let pool = MockMcpPool::happy();
        assert!(pool.has_server("mock"));

        let result = pool
            .call_tool("mock", "test_tool", serde_json::json!({}), opts())
            .await
            .unwrap();
        assert!(!result.is_error);
        assert_eq!(result.text(), "ok");
        assert_eq!(result.content_size_bytes, 2);
    }

    #[tokio::test]
    async fn error_fixture_returns_tool_call_failed() {
        let pool = MockMcpPool::error();
        let result = pool
            .call_tool("mock", "test_tool", serde_json::json!({}), opts())
            .await;
        assert!(matches!(
            result,
            Err(McpError::ToolCallFailed { .. })
        ));
    }

    #[tokio::test]
    async fn oversized_fixture_returns_ok_above_cap() {
        let pool = MockMcpPool::oversized();
        let result = pool
            .call_tool("mock", "test_tool", serde_json::json!({}), opts())
            .await
            .unwrap();
        // The mock does NOT enforce the cap — that's the adapter's job.
        assert!(result.content_size_bytes > MAX_MCP_RESULT_SIZE);
    }

    #[tokio::test]
    async fn cancelled_token_returns_cancelled_error() {
        let pool = MockMcpPool::happy();
        let cancel = CancellationToken::new();
        cancel.cancel();
        let opts = McpCallOptions::new(Arc::from("t"), cancel);

        let result = pool
            .call_tool("mock", "test_tool", serde_json::json!({}), opts)
            .await;
        assert!(matches!(
            result,
            Err(McpError::Cancelled { .. })
        ));
    }

    #[tokio::test]
    async fn server_not_found_for_unknown_server() {
        let pool = MockMcpPool::new();
        let result = pool
            .call_tool("nope", "test_tool", serde_json::json!({}), opts())
            .await;
        assert!(matches!(
            result,
            Err(McpError::ServerNotFound { .. })
        ));
        assert!(!pool.has_server("nope"));
    }

    #[tokio::test]
    async fn queue_exhaustion_after_first_response() {
        let pool = MockMcpPool::new();
        pool.enqueue_text_ok("s", "t", "first");

        let r1 = pool.call_tool("s", "t", serde_json::json!({}), opts()).await;
        assert!(r1.is_ok());
        assert_eq!(r1.unwrap().text(), "first");

        let r2 = pool.call_tool("s", "t", serde_json::json!({}), opts()).await;
        assert!(matches!(r2, Err(McpError::ToolCallFailed { .. })));
    }

    #[tokio::test]
    async fn record_calls_captures_args_and_task_id() {
        let pool = MockMcpPool::new();
        pool.record_calls("s");
        pool.enqueue_text_ok("s", "t", "ok");

        let args = serde_json::json!({"key": "value"});
        let custom_opts = McpCallOptions::new(Arc::from("task-42"), CancellationToken::new());
        pool.call_tool("s", "t", args.clone(), custom_opts)
            .await
            .unwrap();

        let calls = pool.recorded_calls();
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].server, "s");
        assert_eq!(calls[0].tool, "t");
        assert_eq!(calls[0].args, args);
        assert_eq!(&*calls[0].task_id, "task-42");
    }

    #[tokio::test]
    async fn read_resource_returns_registered_content() {
        let pool = MockMcpPool::new();
        let rc = McpResourceContent::new("mock://resource/1")
            .with_mime_type("application/json")
            .with_text(r#"{"ok": true}"#);
        pool.register_resource("srv", "mock://resource/1", Ok(rc.clone()));

        let result = pool
            .read_resource("srv", "mock://resource/1", &CancellationToken::new())
            .await
            .unwrap();
        assert_eq!(result.uri, "mock://resource/1");
        assert_eq!(result.text.as_deref(), Some(r#"{"ok": true}"#));
    }

    #[tokio::test]
    async fn list_tools_returns_registered_descriptors() {
        let pool = MockMcpPool::new();
        pool.set_descriptors(
            "s",
            vec![
                McpToolDescriptor::new("search".to_string())
                    .with_description("Search the web".to_string()),
                McpToolDescriptor::new("fetch".to_string()),
            ],
        );

        let tools = pool.list_tools("s").await.unwrap();
        assert_eq!(tools.len(), 2);
        assert_eq!(tools[0].name, "search");
        assert_eq!(tools[0].description.as_deref(), Some("Search the web"));
        assert_eq!(tools[1].name, "fetch");
    }

    #[tokio::test]
    async fn list_tools_empty_for_registered_server() {
        let pool = MockMcpPool::new();
        pool.register_server("s");
        let tools = pool.list_tools("s").await.unwrap();
        assert!(tools.is_empty());
    }

    #[tokio::test]
    async fn list_tools_server_not_found() {
        let pool = MockMcpPool::new();
        let result = pool.list_tools("nope").await;
        assert!(matches!(result, Err(McpError::ServerNotFound { .. })));
    }
}
