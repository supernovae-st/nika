// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Tool executor traits — agent-v2 kernel hook.
//!
//! ISP decomposition: `ToolExecute`, `ToolBatch`.
//! Super-trait: `ToolExecutor` (blanket for both).
//!
//! These hooks enable agent-v2 to call tools through a unified interface
//! (builtin + MCP tools behind a single trait). Default `execute_batch`
//! is sequential; agent-v2 overrides for parallel execution.

use serde::{Deserialize, Serialize};

/// Opaque tool call identifier.
///
/// Inner field is private (Audit-1 P0-2, 2026-04-16) so future validation
/// (e.g. UUID format check, length cap) can land in `new()` without
/// breaking callers. Use `new()` to construct, `as_str()` to read.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[non_exhaustive]
pub struct ToolCallId {
    value: String,
}

impl ToolCallId {
    /// Create a new tool call identifier.
    #[must_use]
    pub fn new(id: impl Into<String>) -> Self {
        Self { value: id.into() }
    }

    /// Borrow the inner string slice.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.value
    }
}

impl std::fmt::Display for ToolCallId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.value)
    }
}

/// A tool invocation request.
#[derive(Debug, Clone)]
#[non_exhaustive]
pub struct ToolCall {
    /// Unique call identifier.
    pub id: ToolCallId,
    /// Tool name (e.g., `"nika:read"`, `"server::tool"`).
    pub name: String,
    /// Tool input parameters as JSON.
    pub input: serde_json::Value,
}

impl ToolCall {
    /// Create a new tool call.
    #[must_use]
    pub fn new(id: impl Into<String>, name: impl Into<String>, input: serde_json::Value) -> Self {
        Self {
            id: ToolCallId::new(id),
            name: name.into(),
            input,
        }
    }
}

/// Result of a tool execution.
///
/// Mirrors the MCP tool-result split of `content` (the model-facing TEXT
/// view) + `structuredContent` (the typed value): `content` is what the
/// agent loop / LLM reads (always a String), `structured` carries the
/// tool's real typed value when it has one. The `invoke` verb's task
/// dataflow (`tasks.X.output` → CEL / `for_each`) reads `structured` so a
/// `nika:glob` array survives the seam as an array; a String-only MCP tool
/// leaves `structured: None` and stays a String downstream.
#[derive(Debug, Clone)]
#[non_exhaustive]
pub struct ToolResult {
    /// The tool call ID this result corresponds to.
    pub tool_use_id: ToolCallId,
    /// Tool output content — the model-facing TEXT view (the LLM reads
    /// this; never the structured value).
    pub content: String,
    /// The tool's typed value when it produced one (MCP `structuredContent`
    /// analogue). `None` for text-only tools. ONLY the `invoke` verb's
    /// task-output dataflow reads this — the agent loop always feeds the
    /// model `content`.
    pub structured: Option<serde_json::Value>,
    /// Whether the tool execution resulted in an error.
    pub is_error: bool,
}

impl ToolResult {
    /// Create a successful tool result (text-only · no structured value).
    #[must_use]
    pub fn success(tool_use_id: impl Into<String>, content: impl Into<String>) -> Self {
        Self {
            tool_use_id: ToolCallId::new(tool_use_id),
            content: content.into(),
            structured: None,
            is_error: false,
        }
    }

    /// Attach the tool's typed value (the MCP `structuredContent` plane).
    /// Builder over [`success`](Self::success): the `content` String stays
    /// the model-facing view, `structured` carries the real value for the
    /// `invoke` verb's task dataflow.
    #[must_use]
    pub fn with_structured(mut self, value: serde_json::Value) -> Self {
        self.structured = Some(value);
        self
    }

    /// Create an error tool result.
    #[must_use]
    pub fn error(tool_use_id: impl Into<String>, content: impl Into<String>) -> Self {
        Self {
            tool_use_id: ToolCallId::new(tool_use_id),
            content: content.into(),
            structured: None,
            is_error: true,
        }
    }
}

/// Tool execution errors.
#[derive(Debug, thiserror::Error, miette::Diagnostic)]
#[non_exhaustive]
pub enum ToolExecError {
    /// Tool not found.
    #[error("tool not found: {name}")]
    NotFound {
        /// Tool name that was not found.
        name: String,
    },

    /// Tool execution timed out.
    #[error("tool timed out: {name} after {duration_ms}ms")]
    Timeout {
        /// Tool name.
        name: String,
        /// Timeout duration in milliseconds.
        duration_ms: u64,
    },

    /// Tool execution failed.
    #[error("tool execution failed: {name}: {reason}")]
    ExecutionFailed {
        /// Tool name.
        name: String,
        /// Failure reason.
        reason: String,
    },

    /// Tool system not available.
    #[error("tool not available: {reason}")]
    NotAvailable {
        /// Why the tool system is unavailable.
        reason: String,
    },
}

/// Execute a single tool call.
#[trait_variant::make(ToolExecuteDyn: Send)]
pub trait ToolExecute: Send + Sync {
    /// Execute a tool call and return its result.
    ///
    /// CANCEL SAFETY: depends on the tool. Impls MUST document per-tool
    /// whether cancellation can leave side effects (shell commands with
    /// `kill_on_drop` = cancel-safe; HTTP POST = idempotency-keyed;
    /// filesystem writes = atomic-rename recommended).
    async fn execute(&self, call: ToolCall) -> Result<ToolResult, ToolExecError>;
}

/// Execute tool calls in batch.
#[trait_variant::make(ToolBatchDyn: Send)]
pub trait ToolBatch: Send + Sync {
    /// Execute multiple tool calls.
    ///
    /// Default: sequential. Agent-v2 overrides for parallel execution.
    ///
    /// CANCEL SAFETY: cancel-safe at batch boundary. Dropping mid-batch
    /// leaves any already-executed calls complete and the remaining ones
    /// unstarted. Per-call safety delegates to `ToolExecute::execute`.
    async fn execute_batch(&self, calls: Vec<ToolCall>) -> Vec<Result<ToolResult, ToolExecError>>;
}

/// Full tool executor — blanket super-trait.
pub trait ToolExecutor: ToolExecute + ToolBatch {}
impl<T: ToolExecute + ToolBatch> ToolExecutor for T {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tool_call_id_display() {
        let id = ToolCallId::new("tc_001");
        assert_eq!(id.to_string(), "tc_001");
    }

    #[test]
    fn tool_call_id_equality() {
        let a = ToolCallId::new("x");
        let b = ToolCallId::new("x");
        assert_eq!(a, b);
    }

    #[test]
    fn tool_call_new() {
        let call = ToolCall::new("tc_1", "nika:read", serde_json::json!({"path": "/tmp"}));
        assert_eq!(call.id.as_str(), "tc_1");
        assert_eq!(call.name, "nika:read");
    }

    #[test]
    fn tool_result_success() {
        let result = ToolResult::success("tc_1", "file contents");
        assert!(!result.is_error);
        assert_eq!(result.content, "file contents");
        // success() is text-only — the structured plane stays empty until a
        // tool opts in via with_structured (the invoke dataflow's signal).
        assert!(result.structured.is_none());
    }

    #[test]
    fn tool_result_with_structured_keeps_content_and_carries_value() {
        // The MCP split: content (TEXT view) is unchanged, structured carries
        // the typed value the invoke verb's task output reads.
        let value = serde_json::json!(["a.md", "b.md"]);
        let result =
            ToolResult::success("tc_1", "[\"a.md\",\"b.md\"]").with_structured(value.clone());
        assert_eq!(result.content, "[\"a.md\",\"b.md\"]");
        assert_eq!(result.structured, Some(value));
        assert!(!result.is_error);
    }

    #[test]
    fn tool_result_error() {
        let result = ToolResult::error("tc_1", "file not found");
        assert!(result.is_error);
        assert!(result.structured.is_none());
    }

    #[test]
    fn tool_exec_error_not_found_display() {
        let err = ToolExecError::NotFound {
            name: "nika:missing".into(),
        };
        assert_eq!(err.to_string(), "tool not found: nika:missing");
    }

    #[test]
    fn tool_exec_error_timeout_display() {
        let err = ToolExecError::Timeout {
            name: "nika:read".into(),
            duration_ms: 5000,
        };
        assert!(err.to_string().contains("timed out"));
    }

    #[test]
    fn tool_exec_error_not_available_display() {
        let err = ToolExecError::NotAvailable {
            reason: "no executor configured".into(),
        };
        assert!(err.to_string().contains("not available"));
    }

    fn _assert_send_sync<T: Send + Sync>() {}

    #[test]
    fn tool_types_send_sync() {
        _assert_send_sync::<ToolCallId>();
        _assert_send_sync::<ToolCall>();
        _assert_send_sync::<ToolResult>();
    }
}
