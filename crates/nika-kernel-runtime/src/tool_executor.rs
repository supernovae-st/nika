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

/// The immutable execution-bound instant supplied to tool evaluators.
///
/// The runtime mints this once, beside its opening trace frame. Tool
/// executors that expose pure projections of run-start time receive the same
/// nanosecond value on each [`ToolCall`]. The value is call-owned so shared
/// dispatchers never carry mutable ambient execution state.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub struct ToolRunStart {
    unix_ns: i64,
}

impl ToolRunStart {
    /// Construct the context from the opening frame's Unix nanoseconds.
    #[must_use]
    pub const fn new(unix_ns: i64) -> Self {
        Self { unix_ns }
    }

    /// Return the exact Unix nanoseconds carried by the opening frame.
    #[must_use]
    pub const fn unix_ns(self) -> i64 {
        self.unix_ns
    }
}

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
    /// Immutable execution context. Private so callers cannot construct a
    /// partially-initialized context; use [`Self::with_run_start`].
    run_start: Option<ToolRunStart>,
}

impl ToolCall {
    /// Create a new tool call.
    #[must_use]
    pub fn new(id: impl Into<String>, name: impl Into<String>, input: serde_json::Value) -> Self {
        Self {
            id: ToolCallId::new(id),
            name: name.into(),
            input,
            run_start: None,
        }
    }

    /// Attach the execution-bound opening instant to this call.
    #[must_use]
    pub fn with_run_start(mut self, run_start: ToolRunStart) -> Self {
        self.run_start = Some(run_start);
        self
    }

    /// Return the execution-bound opening instant, when a runtime supplied it.
    #[must_use]
    pub const fn run_start(&self) -> Option<ToolRunStart> {
        self.run_start
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
    /// On an error (`is_error == true`), the tool's OWN failure metadata —
    /// the user-facing spec code (`NIKA-BUILTIN-FETCH-001`) + retry class.
    /// `None` when the tool reported only text (an MCP `isError` with no
    /// structured failure, or a success). The `invoke` verb reads it so a
    /// genuinely-transient tool failure (HTTP 503/429 · DNS · connection)
    /// stays retryable and the author's `on_codes:` filters on the tool's
    /// own code — both were dropped when only `content` survived the seam
    /// (BUG-D · the metadata-slot gap the `BuiltinFailure` doc foretold).
    pub error_meta: Option<ToolErrorMeta>,
}

/// A failed [`ToolResult`]'s machine-readable error metadata — the
/// spec-code + retry class a tool carries alongside the text `content`.
///
/// Additive companion to `is_error` (which stays the boolean an agent loop
/// reads): the builtin/MCP failure plane sets this so the `invoke` verb can
/// classify transience and surface the tool's own user-facing code, instead
/// of flattening every tool error to a non-retryable `NIKA-451`.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct ToolErrorMeta {
    /// The tool's user-facing spec code (`NIKA-BUILTIN-FETCH-001` ·
    /// `NIKA-<NS>-<NNN>`), the identifier an author writes in `on_codes:`.
    /// `None` when the tool surfaced no structured code.
    pub spec_code: Option<String>,
    /// Whether retrying the call may succeed (HTTP 503/429 · DNS · connection
    /// reset are transient; 4xx-other · refusals are not).
    pub transient: bool,
}

impl ToolErrorMeta {
    /// Construct the error metadata (INV-019 · `new()` on every
    /// `#[non_exhaustive]` struct).
    #[must_use]
    pub fn new(spec_code: Option<String>, transient: bool) -> Self {
        Self {
            spec_code,
            transient,
        }
    }
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
            error_meta: None,
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

    /// Create an error tool result (text-only · no structured failure
    /// metadata — the `invoke` verb falls back to its engine `NIKA-451`
    /// code, non-transient).
    #[must_use]
    pub fn error(tool_use_id: impl Into<String>, content: impl Into<String>) -> Self {
        Self {
            tool_use_id: ToolCallId::new(tool_use_id),
            content: content.into(),
            structured: None,
            is_error: true,
            error_meta: None,
        }
    }

    /// Attach the tool's own failure metadata (spec code + retry class) to
    /// an error result. Builder over [`error`](Self::error): the `content`
    /// String stays the model-facing view, `error_meta` carries the
    /// classification the `invoke` verb reads (BUG-D).
    #[must_use]
    pub fn with_error_meta(mut self, meta: ToolErrorMeta) -> Self {
        self.error_meta = Some(meta);
        self
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
        assert_eq!(call.run_start(), None);
    }

    #[test]
    fn tool_call_carries_immutable_run_start() {
        let start = ToolRunStart::new(42);
        let call = ToolCall::new("tc_1", "nika:jq", serde_json::json!({})).with_run_start(start);
        assert_eq!(call.run_start(), Some(start));
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
