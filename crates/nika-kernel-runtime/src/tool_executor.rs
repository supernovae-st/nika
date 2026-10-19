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
    /// The task's `timeout:` (spec 03), private for the same reason; use
    /// [`Self::with_deadline`]. A builtin whose transport has its own
    /// deadline (`nika:fetch`) honors it.
    deadline: Option<std::time::Duration>,
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
            deadline: None,
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

    /// Attach the task's `timeout:` as this call's deadline.
    #[must_use]
    pub fn with_deadline(mut self, deadline: std::time::Duration) -> Self {
        self.deadline = Some(deadline);
        self
    }

    /// The task's `timeout:`, when the runtime supplied one.
    #[must_use]
    pub const fn deadline(&self) -> Option<std::time::Duration> {
        self.deadline
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
    /// A non-fatal diagnostic the tool raised BESIDE a success — what its
    /// value does not carry (a `nika:glob` pattern that also matched
    /// directories the walk never returns). Never folded into `content`
    /// or `structured`: a consumer's value is unchanged; the `invoke` verb
    /// carries it to the task's terminal frame as the OBS-E `warning`
    /// field, so the run line and the trace say it. `None` = nothing to
    /// say — the common case, and every pre-existing tool.
    pub warning: Option<String>,
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
    /// The failed call's typed facts (`nika:fetch` · the status it received
    /// and the set it accepted). `None` when the tool surfaced none: every
    /// text-only tool, every transport failure.
    pub details: Option<ToolErrorDetails>,
}

impl ToolErrorMeta {
    /// Construct the error metadata (INV-019 · `new()` on every
    /// `#[non_exhaustive]` struct).
    #[must_use]
    pub fn new(spec_code: Option<String>, transient: bool) -> Self {
        Self {
            spec_code,
            transient,
            details: None,
        }
    }

    /// Attach the failed call's typed facts. Empty details stay absent, so a
    /// failure that observed nothing keeps the shape it always had.
    #[must_use]
    pub fn with_details(mut self, details: ToolErrorDetails) -> Self {
        self.details = (!details.is_empty()).then_some(details);
        self
    }
}

/// The spec code a call the run stopped waiting for fails with (spec 05 ·
/// `NIKA-CANCEL-001`, « task cancelled », never transient).
pub const STOPPED_CALL_CODE: &str = "NIKA-CANCEL-001";

/// What became of a call the run stopped waiting for, as far as its caller
/// can know.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum StopOutcome {
    /// The call was sent: the server may have acted on it before it was
    /// asked to cancel.
    Unknown,
    /// The call was never sent: no server received it.
    NotSent,
}

impl StopOutcome {
    /// The wire word (`unknown` · `not_sent`).
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Unknown => "unknown",
            Self::NotSent => "not_sent",
        }
    }
}

/// A call the run stopped waiting for (the operator's Stop, a task
/// `timeout:` that dropped the attempt): the tool, the call's id, and what
/// became of the call.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct StoppedCall {
    /// The tool id (`mcp:<server>/<tool>`).
    pub tool: String,
    /// The call's id.
    pub call_id: String,
    /// What became of the call.
    pub outcome: StopOutcome,
}

impl StoppedCall {
    /// Name a stopped call (INV-019 · `new()` on every `#[non_exhaustive]`
    /// struct).
    #[must_use]
    pub fn new(tool: impl Into<String>, call_id: impl Into<String>, outcome: StopOutcome) -> Self {
        Self {
            tool: tool.into(),
            call_id: call_id.into(),
            outcome,
        }
    }

    /// The `stopped_call` object: `tool`, `call_id` and `outcome`.
    #[must_use]
    pub fn to_value(&self) -> serde_json::Value {
        serde_json::json!({
            "tool": self.tool,
            "call_id": self.call_id,
            "outcome": self.outcome.as_str(),
        })
    }
}

/// What a failed tool call observed beside its failure, projected as the
/// spec error object's `details` (spec 05 §error structure · for
/// `nika:fetch`, stdlib §Status as data · for a call the run stopped
/// waiting for, the stopped call). Closed on purpose: only the facts named
/// here cross the tool seam, never a tool's free-form data or a provider's
/// text.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
#[non_exhaustive]
pub struct ToolErrorDetails {
    /// The final HTTP status the call received. A transport failure received
    /// none and never fabricates one.
    pub status_code: Option<u16>,
    /// The exact status set the call declared acceptable (`response.accept`).
    pub accepted: Option<Vec<u16>>,
    /// The call the run stopped waiting for, when that is why it failed.
    pub stopped_call: Option<Box<StoppedCall>>,
}

impl ToolErrorDetails {
    /// No facts yet (INV-019 · `new()` on every `#[non_exhaustive]` struct).
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// The final HTTP status the failed call received.
    #[must_use]
    pub fn with_status_code(mut self, status: u16) -> Self {
        self.status_code = Some(status);
        self
    }

    /// The exact accepted-status set the failed call declared.
    #[must_use]
    pub fn with_accepted(mut self, accepted: Vec<u16>) -> Self {
        self.accepted = Some(accepted);
        self
    }

    /// The call the run stopped waiting for.
    #[must_use]
    pub fn with_stopped_call(mut self, stopped: StoppedCall) -> Self {
        self.stopped_call = Some(Box::new(stopped));
        self
    }

    /// Whether the call observed no fact at all.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.status_code.is_none() && self.accepted.is_none() && self.stopped_call.is_none()
    }

    /// The `details` object of the error: only the facts present, keyed as
    /// the spec names them (`status_code`, `accepted`, `stopped_call`).
    #[must_use]
    pub fn to_value(&self) -> serde_json::Value {
        let mut details = serde_json::Map::new();
        if let Some(status) = self.status_code {
            details.insert("status_code".to_owned(), status.into());
        }
        if let Some(accepted) = &self.accepted {
            details.insert("accepted".to_owned(), accepted.clone().into());
        }
        if let Some(stopped) = &self.stopped_call {
            details.insert("stopped_call".to_owned(), stopped.to_value());
        }
        serde_json::Value::Object(details)
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
            warning: None,
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

    /// Attach a non-fatal diagnostic to a success (the OBS-E `warning`
    /// lane · never the value): the tool ran, returned its value whole,
    /// and has one thing to say about what that value leaves out.
    #[must_use]
    pub fn with_warning(mut self, warning: impl Into<String>) -> Self {
        self.warning = Some(warning.into());
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
            warning: None,
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

    /// `call`, which the run stopped waiting for, as its own failure:
    /// [`STOPPED_CALL_CODE`], never transient (no `retry:` reads a stop as a
    /// tool fault), with the stopped call in the error details.
    #[must_use]
    pub fn stopped(call: &ToolCall, outcome: StopOutcome) -> Self {
        let content = match outcome {
            StopOutcome::NotSent => format!(
                "the run stopped waiting for `{}` before it was sent: no server received it",
                call.name
            ),
            StopOutcome::Unknown => format!(
                "the run stopped waiting for `{}` after it was sent: whether the server acted on \
                 it is unknown",
                call.name
            ),
        };
        let stopped = StoppedCall::new(call.name.clone(), call.id.as_str(), outcome);
        Self::error(call.id.as_str(), content).with_error_meta(
            ToolErrorMeta::new(Some(STOPPED_CALL_CODE.to_owned()), false)
                .with_details(ToolErrorDetails::new().with_stopped_call(stopped)),
        )
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
    fn tool_call_carries_the_task_deadline() {
        let call = ToolCall::new("tc_1", "nika:fetch", serde_json::json!({}));
        assert_eq!(call.deadline(), None);
        let call = call.with_deadline(std::time::Duration::from_secs(180));
        assert_eq!(call.deadline(), Some(std::time::Duration::from_secs(180)));
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

    #[test]
    fn error_meta_details_are_absent_unless_a_fact_was_observed() {
        let plain = ToolErrorMeta::new(Some("NIKA-BUILTIN-FETCH-001".into()), true);
        assert_eq!(plain.details, None);
        // Empty details are no details: the shape of a failure that observed
        // nothing never changes.
        assert_eq!(plain.clone().with_details(ToolErrorDetails::new()), plain);
        let observed = plain.with_details(ToolErrorDetails::new().with_status_code(503));
        assert_eq!(
            observed.details,
            Some(ToolErrorDetails::new().with_status_code(503))
        );
    }

    #[test]
    fn error_details_project_only_the_facts_present() {
        assert_eq!(
            ToolErrorDetails::new().with_status_code(404).to_value(),
            serde_json::json!({"status_code": 404})
        );
        assert_eq!(
            ToolErrorDetails::new()
                .with_status_code(503)
                .with_accepted(vec![200, 404])
                .to_value(),
            serde_json::json!({"status_code": 503, "accepted": [200, 404]})
        );
        assert!(ToolErrorDetails::new().is_empty());
        assert_eq!(ToolErrorDetails::new().to_value(), serde_json::json!({}));
        let stopped = ToolErrorDetails::new().with_stopped_call(StoppedCall::new(
            "mcp:s/t",
            "c1",
            StopOutcome::NotSent,
        ));
        assert!(!stopped.is_empty());
        assert_eq!(
            stopped.to_value(),
            serde_json::json!({"stopped_call": {"tool": "mcp:s/t", "call_id": "c1", "outcome": "not_sent"}})
        );
    }

    #[test]
    fn a_stopped_call_fails_with_the_cancel_code_and_names_the_call() {
        let call = ToolCall::new("c7", "mcp:s/slow", serde_json::json!({}));
        for (outcome, word, said) in [
            (StopOutcome::Unknown, "unknown", "after it was sent"),
            (StopOutcome::NotSent, "not_sent", "before it was sent"),
        ] {
            let result = ToolResult::stopped(&call, outcome);
            assert!(result.is_error);
            assert_eq!(result.tool_use_id.as_str(), "c7");
            assert!(
                result.content.contains("`mcp:s/slow`") && result.content.contains(said),
                "{}",
                result.content
            );
            let meta = result.error_meta.expect("the stop's metadata");
            assert_eq!(meta.spec_code.as_deref(), Some("NIKA-CANCEL-001"));
            assert!(!meta.transient, "a stop is never retried");
            let details = meta.details.expect("the stopped call");
            let stopped = details.stopped_call.as_ref().expect("the typed stop");
            assert_eq!(stopped.tool, "mcp:s/slow");
            assert_eq!(stopped.call_id, "c7");
            assert_eq!(stopped.outcome, outcome);
            assert_eq!(
                details.to_value(),
                serde_json::json!({
                    "stopped_call": {"tool": "mcp:s/slow", "call_id": "c7", "outcome": word}
                }),
                "the error details keep the exact wire object for either stop outcome"
            );
        }
    }

    fn _assert_send_sync<T: Send + Sync>() {}

    #[test]
    fn tool_types_send_sync() {
        _assert_send_sync::<ToolCallId>();
        _assert_send_sync::<ToolCall>();
        _assert_send_sync::<ToolResult>();
        _assert_send_sync::<ToolErrorDetails>();
        _assert_send_sync::<StoppedCall>();
    }
}
