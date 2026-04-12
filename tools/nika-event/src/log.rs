// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! EventLog - Event sourcing implementation
//!
//! Provides full audit trail with replay capability.
//! - Event: envelope with id + timestamp + kind
//! - EventKind: 100 variants across 20 categories (workflow/task/fine-grained/MCP/context/agent/nested-agent/record/guardrails/builtin/artifact/media/structured-output/vision/http/exec/fetch/routing/policy/boot/security)
//! - EventLog: thread-safe, append-only log
//!
//! `AgentTurnMetadata` provides reasoning capture (thinking, tokens, stop_reason).
//! `AgentTurn` variant includes optional metadata.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Instant;

use parking_lot::RwLock; // 2-3x faster than std::sync::RwLock
use tokio::sync::broadcast;

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::trace::TraceWriter;

// ═══════════════════════════════════════════════════════════════
// Helper structs for ContextAssembled event
// ═══════════════════════════════════════════════════════════════

/// A source included in the assembled context
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ContextSource {
    /// Node/source identifier
    pub node: String,
    /// Token count for this source
    pub tokens: u64,
}

/// An item excluded from context assembly
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ExcludedItem {
    /// Node/source identifier
    pub node: String,
    /// Reason for exclusion
    pub reason: String,
}

// ═══════════════════════════════════════════════════════════════
// AgentTurnMetadata for Reasoning Capture
// ═══════════════════════════════════════════════════════════════

/// Agent turn response metadata for observability
///
/// Captures detailed information about each agent turn, including:
/// - Thinking content (if Claude extended thinking is enabled)
/// - Response text
/// - Token usage
/// - Stop reason
///
/// ## Note on Thinking Capture
/// Full thinking block capture requires using rig's streaming API or
/// direct completion requests. When using `agent.prompt()`, thinking
/// is not available (will be `None`).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AgentTurnMetadata {
    /// Thinking content from Claude's extended thinking (if enabled)
    ///
    /// This field is populated when using streaming completion or
    /// direct completion API. It's `None` when using Agent::prompt().
    #[serde(skip_serializing_if = "Option::is_none")]
    pub thinking: Option<String>,

    /// Main response text from the agent
    pub response_text: String,

    /// Input tokens used for this turn
    pub input_tokens: u64,

    /// Output tokens generated for this turn
    pub output_tokens: u64,

    /// Cache read tokens (Anthropic prompt caching)
    #[serde(default)]
    pub cache_read_tokens: u64,

    /// Stop reason
    pub stop_reason: crate::types::AgentStopReason,
}

impl AgentTurnMetadata {
    /// Create metadata for a simple text response (no thinking)
    pub fn text_only(
        response: impl Into<String>,
        stop_reason: impl Into<crate::types::AgentStopReason>,
    ) -> Self {
        Self {
            thinking: None,
            response_text: response.into(),
            input_tokens: 0,
            output_tokens: 0,
            cache_read_tokens: 0,
            stop_reason: stop_reason.into(),
        }
    }

    /// Create metadata with token usage
    pub fn with_usage(
        response: impl Into<String>,
        input_tokens: u64,
        output_tokens: u64,
        stop_reason: impl Into<crate::types::AgentStopReason>,
    ) -> Self {
        Self {
            thinking: None,
            response_text: response.into(),
            input_tokens,
            output_tokens,
            cache_read_tokens: 0,
            stop_reason: stop_reason.into(),
        }
    }

    /// Total tokens (input + output)
    pub fn total_tokens(&self) -> u64 {
        self.input_tokens + self.output_tokens
    }

    /// Check if thinking was captured
    pub fn has_thinking(&self) -> bool {
        self.thinking.is_some()
    }
}

/// Single event in the workflow execution log
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Event {
    /// Monotonic sequence ID (for ordering)
    pub id: u64,
    /// Time since workflow start (ms)
    pub timestamp_ms: u64,
    /// Event type and data
    pub kind: EventKind,
}

/// All possible event types (3 levels)
///
/// Uses `Arc<str>` for task_id fields to enable zero-cost cloning.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, nika_macros::EventTaskId)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum EventKind {
    // ═══════════════════════════════════════════
    // WORKFLOW LEVEL
    // ═══════════════════════════════════════════
    WorkflowStarted {
        task_count: usize,
        /// Unique generation ID for this execution
        generation_id: String,
        /// Hash of workflow file for cache invalidation
        workflow_hash: String,
        /// Nika version
        nika_version: String,
    },
    WorkflowCompleted {
        final_output: Arc<Value>,
        total_duration_ms: u64,
    },
    WorkflowFailed {
        error: String,
        failed_task: Option<Arc<str>>,
    },
    /// Workflow was cancelled by user
    WorkflowAborted {
        /// Reason for abort (e.g., "User cancelled", "Timeout")
        reason: String,
        /// Duration before abort (ms)
        duration_ms: u64,
        /// Tasks that were still running when aborted
        running_tasks: Vec<Arc<str>>,
    },
    /// Workflow execution paused
    WorkflowPaused,
    /// Workflow execution resumed
    WorkflowResumed,

    // ═══════════════════════════════════════════
    // TASK LEVEL
    // ═══════════════════════════════════════════
    #[has_task_id]
    TaskScheduled {
        task_id: Arc<str>,
        dependencies: Vec<Arc<str>>,
    },
    /// Task execution begins with resolved inputs from with: block
    #[has_task_id]
    TaskStarted {
        task_id: Arc<str>,
        /// Verb type (infer, exec, fetch, invoke, agent)
        verb: Arc<str>,
        /// Resolved inputs from ResolvedBindings (what the task receives)
        inputs: Arc<Value>,
    },
    #[has_task_id]
    TaskCompleted {
        task_id: Arc<str>,
        output: Arc<Value>,
        duration_ms: u64,
    },
    #[has_task_id]
    TaskFailed {
        task_id: Arc<str>,
        error: String,
        duration_ms: u64,
        /// Structured error code (e.g. "NIKA-044") for programmatic extraction
        #[serde(skip_serializing_if = "Option::is_none")]
        error_code: Option<String>,
    },
    /// A task was skipped because a dependency failed.
    #[has_task_id]
    TaskSkipped {
        task_id: Arc<str>,
        /// The dependency that failed causing the skip
        reason: String,
    },

    // ═══════════════════════════════════════════
    // FINE-GRAINED (template/provider)
    // ═══════════════════════════════════════════
    #[has_task_id]
    TemplateResolved {
        task_id: Arc<str>,
        template: String,
        result: String,
    },
    #[has_task_id]
    ProviderCalled {
        task_id: Arc<str>,
        provider: String,
        model: String,
        prompt_len: usize,
        /// Custom endpoint URL (if using OpenAI-compatible server)
        #[serde(skip_serializing_if = "Option::is_none")]
        endpoint_url: Option<String>,
    },
    #[has_task_id]
    ProviderResponded {
        task_id: Arc<str>,
        /// API request ID (for debugging with provider)
        request_id: Option<String>,
        /// Input tokens
        input_tokens: u64,
        /// Output tokens
        output_tokens: u64,
        /// Cache read tokens (if any)
        cache_read_tokens: u64,
        /// Time to first token (ms), if known
        ttft_ms: Option<u64>,
        /// Finish reason
        finish_reason: crate::types::FinishReason,
        /// Estimated cost in USD
        cost_usd: f64,
    },

    /// Provider fallback: one provider failed, trying the next in the chain
    #[has_task_id]
    ProviderFallback {
        task_id: Arc<str>,
        /// Provider that failed
        from: String,
        /// Provider being tried next
        to: String,
        /// Reason for fallback (error message)
        reason: String,
    },

    // ═══════════════════════════════════════════
    // CONTEXT ASSEMBLY
    // ═══════════════════════════════════════════
    /// Context assembly event for observability
    #[has_task_id]
    ContextAssembled {
        task_id: Arc<str>,
        /// Sources included in context
        sources: Vec<ContextSource>,
        /// Items excluded (with reasons)
        excluded: Vec<ExcludedItem>,
        /// Total tokens in assembled context
        total_tokens: u64,
        /// Budget utilization percentage
        budget_used_pct: f32,
        /// Was context truncated?
        truncated: bool,
    },

    // ═══════════════════════════════════════════
    // MCP EVENTS
    // ═══════════════════════════════════════════
    /// MCP tool call or resource read initiated
    #[has_task_id]
    McpInvoke {
        task_id: Arc<str>,
        /// Unique call ID for correlating with McpResponse
        call_id: String,
        mcp_server: String,
        tool: Option<String>,
        resource: Option<String>,
        /// Full params passed to MCP tool (for TUI display)
        #[serde(skip_serializing_if = "Option::is_none")]
        params: Option<Arc<Value>>,
    },
    /// MCP operation completed
    #[has_task_id]
    McpResponse {
        task_id: Arc<str>,
        /// Correlates with McpInvoke.call_id
        call_id: String,
        output_len: usize,
        /// Duration of MCP call in milliseconds
        duration_ms: u64,
        /// Whether response came from cache
        cached: bool,
        /// Whether MCP tool returned an error
        is_error: bool,
        /// Full response JSON (for TUI display)
        #[serde(skip_serializing_if = "Option::is_none")]
        response: Option<Arc<Value>>,
    },
    /// MCP server connection established
    McpConnected {
        /// Name of the connected MCP server
        server_name: String,
    },
    /// MCP server connection failed
    McpError {
        /// Name of the MCP server
        server_name: String,
        /// Error description
        error: String,
    },
    /// MCP operation retry attempt
    ///
    /// Emitted when MCP tool calls fail with connection errors and are retried.
    /// Use `McpClient::call_tool_with_retry_events()` for observable retry tracking.
    /// TUI handlers display this event with attempt count and error details.
    #[has_task_id]
    McpRetry {
        /// Task ID initiating the retry
        task_id: Arc<str>,
        /// Name of the MCP server
        server_name: String,
        /// Tool or resource being retried
        operation: String,
        /// Current attempt number (1-based)
        attempt: u32,
        /// Max attempts configured
        max_attempts: u32,
        /// Error that triggered the retry
        error: String,
    },

    // ═══════════════════════════════════════════
    // AGENT EVENTS
    // ═══════════════════════════════════════════
    /// Agent preset was applied to a task (via `agent: <name>` or `preset: <name>`)
    #[has_task_id]
    PresetApplied {
        task_id: Arc<str>,
        preset_name: String,
        provider: Option<String>,
        model: Option<String>,
    },
    /// Agent loop started
    #[has_task_id]
    AgentStart {
        task_id: Arc<str>,
        max_turns: u32,
        mcp_servers: Vec<String>,
    },
    /// Agent turn event with optional metadata
    ///
    /// When `metadata` is present, it contains:
    /// - Response text
    /// - Token usage (input/output/cache)
    /// - Stop reason
    /// - Thinking content (if using streaming API)
    #[has_task_id]
    AgentTurn {
        task_id: Arc<str>,
        turn_index: u32,
        /// Event kind
        kind: crate::types::AgentTurnKind,
        /// Turn metadata including response text, tokens, thinking
        #[serde(skip_serializing_if = "Option::is_none")]
        metadata: Option<AgentTurnMetadata>,
    },
    /// Agent loop completed (reached stop condition or max turns)
    #[has_task_id]
    AgentComplete {
        task_id: Arc<str>,
        turns: u32,
        stop_reason: crate::types::AgentStopReason,
    },

    // ═══════════════════════════════════════════
    // NESTED AGENT EVENTS
    // ═══════════════════════════════════════════
    /// A sub-agent was spawned by a parent agent
    #[has_task_id(field = "parent_task_id")]
    AgentSpawned {
        /// ID of the parent task that spawned the child
        parent_task_id: Arc<str>,
        /// ID of the newly spawned child task
        child_task_id: Arc<str>,
        /// Current depth level (1 = root agent spawning first child)
        depth: u32,
    },

    // ═══════════════════════════════════════════
    // RECORD EVENTS
    // ═══════════════════════════════════════════
    /// Task output was compressed into a Record
    #[has_task_id]
    RecordCreated {
        task_id: Arc<str>,
        /// Token count of compressed summary
        summary_tokens: u64,
        /// Confidence score from compression LLM
        confidence: f64,
        /// Cost of the compression call
        compression_cost_usd: f64,
        /// Compression ratio (compressed/original)
        compression_ratio: f64,
        /// Model used for compression
        model: String,
    },
    /// Record compression was skipped (fallback to truncation or disabled)
    #[has_task_id]
    RecordSkipped {
        task_id: Arc<str>,
        /// Reason compression was skipped
        reason: String,
    },

    // ═══════════════════════════════════════════
    // GUARDRAIL EVENTS
    // ═══════════════════════════════════════════
    /// Guardrail check passed
    #[has_task_id]
    GuardrailPassed {
        /// Task ID for correlation
        task_id: Arc<str>,
        /// Guardrail type
        guardrail_type: crate::types::GuardrailType,
        /// Human-readable description of the guardrail
        description: String,
    },
    /// Guardrail check failed
    #[has_task_id]
    GuardrailFailed {
        /// Task ID for correlation
        task_id: Arc<str>,
        /// Guardrail type
        guardrail_type: crate::types::GuardrailType,
        /// Human-readable description of the guardrail
        description: String,
        /// Error message explaining why it failed
        message: String,
    },
    /// Guardrail failure requires escalation
    ///
    /// Emitted when a guardrail with `on_failure: escalate` fails.
    /// This signals that human intervention or special handling is needed.
    #[has_task_id]
    GuardrailEscalation {
        /// Task ID for correlation
        task_id: Arc<str>,
        /// Guardrail type
        guardrail_type: crate::types::GuardrailType,
        /// Guardrail ID for identification
        guardrail_id: String,
        /// Error message explaining why it failed
        message: String,
        /// Severity level
        severity: crate::types::Severity,
        /// Suggested action (optional)
        #[serde(skip_serializing_if = "Option::is_none")]
        suggested_action: Option<String>,
    },

    // ═══════════════════════════════════════════
    // BUILTIN TOOL EVENTS
    // ═══════════════════════════════════════════
    /// Log event emitted by nika:log builtin tool
    #[has_task_id(optional)]
    Log {
        /// Log level: trace, debug, info, warn, error
        level: String,
        /// Log message
        message: String,
        /// Optional task context
        #[serde(skip_serializing_if = "Option::is_none")]
        task_id: Option<Arc<str>>,
    },

    /// Custom event emitted by nika:emit builtin tool
    #[has_task_id(optional)]
    Custom {
        /// Event name/type
        name: String,
        /// Event payload (arbitrary JSON)
        payload: Value,
        /// Optional task context
        #[serde(skip_serializing_if = "Option::is_none")]
        task_id: Option<Arc<str>>,
    },

    // ═══════════════════════════════════════════
    // ARTIFACT EVENTS
    // ═══════════════════════════════════════════
    /// Artifact successfully written to disk
    #[has_task_id]
    ArtifactWritten {
        /// Task that produced this artifact
        task_id: Arc<str>,
        /// Final resolved path
        path: String,
        /// Size in bytes
        size: u64,
        /// Output format (text, json, binary)
        format: String,
        /// Blake3 checksum from CAS (binary artifacts only)
        #[serde(skip_serializing_if = "Option::is_none")]
        checksum: Option<String>,
    },
    /// Artifact write failed
    #[has_task_id]
    ArtifactFailed {
        /// Task that produced this artifact
        task_id: Arc<str>,
        /// Intended path
        path: String,
        /// Error reason
        reason: String,
    },

    // ═══════════════════════════════════════════
    // MEDIA EVENTS
    // ═══════════════════════════════════════════
    /// Media content blocks extracted from MCP tool result
    #[has_task_id]
    MediaExtracted {
        task_id: Arc<str>,
        /// Number of non-text content blocks found
        block_count: u32,
        /// Content types found (e.g., ["image", "audio"])
        content_types: Vec<String>,
    },

    /// Single media block processed (decoded + detected)
    #[has_task_id]
    MediaProcessed {
        task_id: Arc<str>,
        /// blake3 hash of the decoded content (with "blake3:" prefix)
        hash: String,
        /// Detected MIME type
        mime_type: String,
        /// File size in bytes (decoded)
        size_bytes: u64,
    },

    /// Media file stored in CAS
    #[has_task_id]
    MediaStored {
        task_id: Arc<str>,
        /// blake3 hash (with "blake3:" prefix)
        hash: String,
        /// File path in CAS store
        path: String,
        /// File size in bytes
        size_bytes: u64,
        /// Whether read-back verification passed (or skipped for small files)
        verified: bool,
        /// Whether this was a dedup hit
        deduplicated: bool,
        /// Pipeline latency in milliseconds (decode -> store)
        pipeline_ms: u64,
    },

    /// Media storage failed
    #[has_task_id]
    MediaStoreFailed {
        task_id: Arc<str>,
        /// blake3 hash (if available, empty string if pre-hash failure)
        hash: String,
        /// Error description
        reason: String,
    },

    /// Media integrity check completed (emitted after all tasks finish)
    MediaIntegrityCheck {
        /// Number of media refs checked
        checked: u64,
        /// Number of integrity warnings (missing files, size mismatches)
        warnings: u64,
    },

    // ═══════════════════════════════════════════
    // STRUCTURED OUTPUT EVENTS
    // ═══════════════════════════════════════════
    /// Structured output extraction attempt at a specific layer
    ///
    /// Emitted for each layer/retry attempt in the 4-layer defense system:
    /// - Layer 0: Tool Injection (DynamicSubmitTool, provider-side)
    /// - Layer 2: Extract + Validate (post-processing)
    /// - Layer 3: Retry with Feedback
    /// - Layer 4: LLM Repair
    #[has_task_id]
    StructuredOutputAttempt {
        /// Task ID for correlation
        task_id: Arc<str>,
        /// Layer number (0-4)
        layer: u8,
        /// Human-readable layer name (e.g., "tool_injection", "extract_validate")
        layer_name: String,
        /// Attempt number within this layer (1-based)
        attempt: u32,
        /// Whether this attempt succeeded
        success: bool,
        /// Error message if failed
        #[serde(skip_serializing_if = "Option::is_none")]
        error: Option<String>,
    },
    /// Structured output successfully extracted
    ///
    /// Emitted when any layer successfully produces valid output
    #[has_task_id]
    StructuredOutputSuccess {
        /// Task ID for correlation
        task_id: Arc<str>,
        /// Layer that succeeded (1-4)
        layer: u8,
        /// Human-readable layer name
        layer_name: String,
        /// Total attempts across all layers before success
        total_attempts: u32,
    },
    /// Structured output validation timed out
    ///
    /// Emitted when the aggregate timeout (600s) is exceeded across all layers.
    #[has_task_id]
    StructuredOutputTimeout {
        task_id: Arc<str>,
        timeout_secs: u64,
    },

    // ═══════════════════════════════════════════
    // VISION EVENTS
    // ═══════════════════════════════════════════
    /// Vision content parts resolved for multimodal inference.
    ///
    /// Emitted when CAS image references in `content:` are resolved
    /// to base64 data before sending to a vision-capable LLM.
    #[has_task_id]
    VisionContentResolved {
        /// Task that triggered vision resolution
        task_id: Arc<str>,
        /// Number of image parts resolved
        image_count: u32,
        /// Total bytes of image data resolved from CAS
        total_bytes: u64,
        /// Time to resolve all images (ms)
        resolve_ms: u64,
    },

    // ═══════════════════════════════════════════
    // HTTP TELEMETRY EVENTS
    // ═══════════════════════════════════════════
    /// HTTP request initiated by fetch: verb
    #[has_task_id]
    HttpRequest {
        task_id: Arc<str>,
        method: String,
        url: String,
        has_body: bool,
    },
    /// HTTP response received by fetch: verb
    #[has_task_id]
    HttpResponse {
        task_id: Arc<str>,
        status_code: u16,
        content_type: Option<String>,
        content_length: Option<u64>,
        elapsed_ms: u64,
    },

    // ═══════════════════════════════════════════
    // MEDIA CLEANUP EVENTS
    // ═══════════════════════════════════════════
    /// Media store cleanup (GC) operation completed
    MediaCleanup {
        /// Number of files removed
        removed: u64,
        /// Total bytes freed
        bytes_freed: u64,
        /// Whether this was a dry-run (no files actually deleted)
        dry_run: bool,
    },

    // ═══════════════════════════════════════════
    // EXEC EVENTS
    // ═══════════════════════════════════════════
    /// Shell command execution completed with exit details
    #[has_task_id]
    ExecCompleted {
        task_id: Arc<str>,
        /// Process exit code (0 = success)
        exit_code: i32,
        /// Length of stdout in bytes
        stdout_len: usize,
        /// Length of stderr in bytes
        stderr_len: usize,
        /// Execution duration in milliseconds
        duration_ms: u64,
    },

    // ═══════════════════════════════════════════
    // FETCH EVENTS
    // ═══════════════════════════════════════════
    /// Fetch retry attempt (mirrors McpRetry pattern)
    #[has_task_id]
    FetchRetry {
        task_id: Arc<str>,
        /// URL being fetched
        url: String,
        /// Current attempt number (1-based)
        attempt: u32,
        /// Max attempts configured
        max_attempts: u32,
        /// HTTP status code that triggered retry (if any)
        #[serde(skip_serializing_if = "Option::is_none")]
        status_code: Option<u16>,
        /// Backoff delay before this attempt in ms
        backoff_ms: u64,
    },

    /// Fetch retries exhausted — all attempts failed
    #[has_task_id]
    FetchExhausted {
        task_id: Arc<str>,
        /// URL that was being fetched
        url: String,
        /// Total attempts made
        attempts: u32,
        /// Last HTTP status code (None for network errors)
        #[serde(skip_serializing_if = "Option::is_none")]
        last_status: Option<u16>,
        /// Human-readable reason
        reason: String,
    },

    /// Task-level retry attempt (all verbs except fetch, which has FetchRetry)
    #[has_task_id]
    TaskRetry {
        task_id: Arc<str>,
        /// Current attempt number (1-based)
        attempt: u32,
        /// Max attempts configured
        max_attempts: u32,
        /// Backoff delay before this attempt in ms
        backoff_ms: u64,
        /// Error that triggered the retry
        #[serde(skip_serializing_if = "Option::is_none")]
        error: Option<String>,
    },

    /// Provider-level auto-retry for transient HTTP errors (500, 502, 503, 429, timeout).
    ///
    /// Built-in 4-attempt safety net in the infer executor, independent of task-level
    /// `retry:` config. Emitted on each retry for observability and cost tracking.
    #[has_task_id]
    ProviderAutoRetried {
        task_id: Arc<str>,
        /// Current attempt number (1-based, attempt 1 = first retry)
        attempt: u32,
        /// Max provider retry attempts (currently 4)
        max_attempts: u32,
        /// Backoff delay before this attempt in ms
        delay_ms: u64,
        /// Error that triggered the retry
        error: String,
    },

    // ═══════════════════════════════════════════
    // ROUTING EVENTS
    // ═══════════════════════════════════════════
    /// A fallback was triggered — switching from one provider to another.
    #[has_task_id]
    FallbackTriggered {
        task_id: Arc<str>,
        /// Provider that failed.
        from_provider: String,
        /// Provider being tried next.
        to_provider: String,
        /// Reason for the fallback (e.g. "error", "timeout", "structured_failure").
        reason: String,
        /// Which attempt in the fallback chain (0-indexed).
        attempt: u32,
    },

    // ═══════════════════════════════════════════
    // POLICY EVENTS
    // ═══════════════════════════════════════════
    /// Security policy blocked an operation
    #[has_task_id]
    PolicyBlocked {
        task_id: Arc<str>,
        /// Verb that was blocked (exec, fetch, invoke)
        verb: String,
        /// Policy type that triggered block (command_blocklist, host_blocklist, ssrf, etc.)
        policy_type: String,
        /// Human-readable reason for the block
        reason: String,
    },

    // ═══════════════════════════════════════════
    // BOOT EVENTS
    // ═══════════════════════════════════════════
    /// Boot phase completed (one per phase during startup)
    BootPhaseCompleted {
        /// Phase name: "config_discovery", "config_validation", "memory_loading",
        /// "secrets_loading", "mcp_startup", "provider_validation", "ready"
        phase: String,
        /// Whether the phase succeeded
        success: bool,
        /// Phase duration in milliseconds
        duration_ms: u64,
        /// Warnings produced during this phase
        #[serde(skip_serializing_if = "Vec::is_empty")]
        warnings: Vec<String>,
    },

    /// Native (local) model loaded successfully
    NativeModelLoaded {
        /// Model identifier (file path for GGUF, HF ID for HuggingFace)
        model: String,
        /// Model kind: "gguf" or "huggingface"
        kind: String,
        /// Model file size in bytes (0 for HuggingFace downloads)
        size_bytes: u64,
        /// Load duration in milliseconds
        duration_ms: u64,
        /// Whether model has vision capabilities
        is_vision: bool,
    },

    // ═══════════════════════════════════════════
    // CONTEXT BUDGET EVENTS
    // ═══════════════════════════════════════════
    /// Context budget check passed — bindings fit within budget
    #[has_task_id]
    BudgetOk {
        task_id: Arc<str>,
        /// Configured budget in tokens
        budget: u32,
        /// Actual estimated tokens
        actual: u32,
    },
    /// Context budget exceeded — bindings were truncated to fit
    #[has_task_id]
    BudgetExceeded {
        task_id: Arc<str>,
        /// Configured budget in tokens
        budget: u32,
        /// Original estimated tokens before truncation
        actual: u32,
        /// Aliases that were truncated
        truncated_fields: Vec<String>,
    },

    // ═══════════════════════════════════════════
    // BINDING EVENTS
    // ═══════════════════════════════════════════
    /// Binding default value applied (via ?? operator)
    #[has_task_id]
    BindingDefaultApplied {
        task_id: Arc<str>,
        /// Alias name in with: block
        alias: String,
        /// Original binding path that was null/missing
        path: String,
        /// Default value that was used
        default_value: Value,
    },

    /// Binding transform chain applied (e.g., |upper|trim|sort)
    #[has_task_id]
    BindingTransformApplied {
        task_id: Arc<str>,
        /// Alias name
        alias: String,
        /// Transform expression (e.g., "upper | trim")
        transform_chain: String,
    },

    /// Environment variable resolved via $env.VAR_NAME binding
    #[has_task_id]
    BindingEnvResolved {
        task_id: Arc<str>,
        /// Environment variable name
        var_name: String,
        /// Whether the env var was found
        found: bool,
    },

    /// Vault credential resolved via $vault.SERVICE.FIELD binding
    #[has_task_id]
    BindingVaultResolved {
        task_id: Arc<str>,
        /// Service name
        service: String,
        /// Field name
        field: String,
        /// Whether the credential field was found
        found: bool,
    },

    // ═══════════════════════════════════════════
    // DAG ORCHESTRATION EVENTS
    // ═══════════════════════════════════════════
    /// Decompose modifier expansion started
    #[has_task_id]
    DecomposeStarted {
        task_id: Arc<str>,
        /// Strategy: "semantic", "static", "nested"
        strategy: String,
    },

    /// Decompose modifier expansion completed
    #[has_task_id]
    DecomposeCompleted {
        task_id: Arc<str>,
        /// Strategy used
        strategy: String,
        /// Number of items produced
        item_count: usize,
        /// Expansion duration in milliseconds
        duration_ms: u64,
    },

    /// for_each iteration batch started
    #[has_task_id]
    ForEachStarted {
        task_id: Arc<str>,
        /// Number of items to iterate over
        item_count: usize,
        /// Concurrency level (1 = sequential)
        concurrency: usize,
        /// Whether fail_fast is enabled
        fail_fast: bool,
    },

    /// for_each iteration batch completed with aggregated results
    #[has_task_id]
    ForEachCompleted {
        task_id: Arc<str>,
        /// Total iterations attempted
        total: u32,
        /// Successful iterations
        succeeded: u32,
        /// Failed iterations (errors)
        failed: u32,
        /// Skipped iterations (cancelled by fail_fast)
        skipped: u32,
        /// Total duration across all iterations (ms)
        duration_ms: u64,
    },

    // ═══════════════════════════════════════════
    // PROVIDER LIFECYCLE EVENTS
    // ═══════════════════════════════════════════
    /// Provider initialized (first use, cache miss)
    ProviderInitialized {
        /// Provider name (anthropic, openai, mistral, etc.)
        provider: String,
        /// Default model for this provider
        model: String,
        /// Whether this was served from cache
        cached: bool,
    },

    /// Builtin tool invoked by agent (nika:read, nika:write, etc.)
    #[has_task_id]
    BuiltinToolInvoked {
        task_id: Arc<str>,
        /// Tool name: "nika:read", "nika:write", etc.
        tool_name: String,
        /// Call duration in milliseconds
        duration_ms: u64,
        /// Whether the call succeeded
        success: bool,
    },

    // ═══════════════════════════════════════════
    // STREAMING EVENTS
    // ═══════════════════════════════════════════
    /// Streaming token delta — emitted during LLM streaming for live progress.
    ///
    /// This is the highest-frequency event and should be lightweight.
    /// LiveRenderer uses it to show live "out:1.2k" on task bars,
    /// turning 30s inference from "frozen" to "clearly alive".
    #[has_task_id]
    StreamingDelta {
        task_id: Arc<str>,
        /// Tokens received in this chunk.
        delta_tokens: u64,
        /// Total tokens received so far for this task.
        total_tokens: u64,
    },

    // ═══════════════════════════════════════════
    // ORCHESTRATOR EVENTS
    // ═══════════════════════════════════════════
    /// Orchestrator started pursuing a goal.
    OrchestratorStarted {
        goal: String,
        max_rounds: u32,
        #[serde(skip_serializing_if = "Option::is_none")]
        agent: Option<String>,
    },

    /// Orchestrator completed a round.
    OrchestratorRound {
        round: u32,
        records_count: usize,
        cost_usd: f64,
    },

    /// Orchestrator launched a sub-workflow.
    OrchestratorSubWorkflow {
        round: u32,
        yaml_hash: String,
        task_count: usize,
    },

    /// Orchestrator completed successfully.
    OrchestratorCompleted {
        rounds: u32,
        total_cost_usd: f64,
        confidence: f64,
    },

    /// Orchestrator failed.
    OrchestratorFailed { round: u32, reason: String },

    // ═══════════════════════════════════════════
    // FETCH EXTRACT EVENTS
    // ═══════════════════════════════════════════
    /// Extraction mode applied to fetch response.
    #[has_task_id]
    ExtractApplied {
        task_id: Arc<str>,
        /// Extract mode: "css", "jq", "text", "markdown", "llm_txt"
        mode: String,
        /// CSS/jq selector used (if any)
        #[serde(skip_serializing_if = "Option::is_none")]
        selector: Option<String>,
        /// Input body length (bytes)
        input_len: usize,
        /// Output length after extraction (bytes)
        output_len: usize,
    },

    // ═══════════════════════════════════════════
    // SECURITY EVENTS
    // ═══════════════════════════════════════════
    /// LLM output security scan finding (output_scanner).
    #[has_task_id]
    SecurityScanFinding {
        task_id: Arc<str>,
        /// Pattern category: "invisible_unicode", "exfiltration", "role_hijack", etc.
        pattern: String,
        /// Human-readable description of the finding.
        description: String,
    },

    // ═══════════════════════════════════════════
    // FOR_EACH ITEM EVENTS
    // ═══════════════════════════════════════════
    /// A single for_each iteration started.
    #[has_task_id]
    ForEachItemStarted {
        task_id: Arc<str>,
        /// 0-based index of this iteration
        index: usize,
        /// Total items in the for_each array
        total: usize,
    },
    /// A single for_each iteration completed successfully.
    #[has_task_id]
    ForEachItemCompleted {
        task_id: Arc<str>,
        index: usize,
        duration_ms: u64,
    },
    /// A single for_each iteration failed.
    #[has_task_id]
    ForEachItemFailed {
        task_id: Arc<str>,
        index: usize,
        error: String,
    },

    // ═══════════════════════════════════════════
    // CANCELLATION
    // ═══════════════════════════════════════════
    /// Task was cancelled (not failed) due to workflow abort or fail_fast.
    #[has_task_id]
    TaskCancelled { task_id: Arc<str>, reason: String },

    // ═══════════════════════════════════════════
    // FALLBACK
    // ═══════════════════════════════════════════
    /// All providers in the fallback chain failed.
    #[has_task_id]
    FallbackChainExhausted {
        task_id: Arc<str>,
        /// Providers that were tried
        providers_tried: Vec<String>,
        /// Error from the last provider
        last_error: String,
    },

    // ═══════════════════════════════════════════

    // ═══════════════════════════════════════════
    // ON_ERROR FALLBACK
    // ═══════════════════════════════════════════
    /// Task-level on_error: fallback was triggered after primary execution + retries failed.
    #[has_task_id]
    TaskFallbackTriggered {
        task_id: Arc<str>,
        /// Which on_error action fired: "ignore", "retry_with_provider", or "fallback"
        action: String,
        /// For retry_with_provider: the provider name. For fallback: the fallback task id.
        target: String,
        /// Whether the fallback itself succeeded.
        success: bool,
    },

    // DIAGNOSTICS (S6 telemetry)
    // ═══════════════════════════════════════════
    /// Template binding resolution failed before task execution.
    #[has_task_id]
    TemplateResolutionFailed {
        task_id: Arc<str>,
        template: String,
        error: String,
    },
    /// Domain rate limiter delayed request.
    #[has_task_id]
    RateLimitDelay {
        task_id: Arc<str>,
        domain: String,
        delay_ms: u64,
    },
    /// Schema file could not be loaded or parsed.
    #[has_task_id]
    SchemaLoadFailed {
        task_id: Arc<str>,
        schema_path: String,
        error: String,
    },
    /// Vision content resolution failed (CAS read, unsupported format, etc.).
    #[has_task_id]
    VisionContentFailed {
        task_id: Arc<str>,
        source: String,
        stage: String,
        error: String,
    },

    // ═══════════════════════════════════════════
    // NIKA SHIELD SECURITY EVENTS
    // ═══════════════════════════════════════════
    /// Taint analysis completed at check time.
    TaintAnalysisComplete {
        /// Number of tasks at each trust level.
        trusted_count: usize,
        model_generated_count: usize,
        model_tainted_count: usize,
        untrusted_count: usize,
        /// Number of warnings generated.
        warning_count: usize,
    },
    /// Trust level assigned to a task after execution.
    #[has_task_id]
    TrustLevelAssigned {
        task_id: Arc<str>,
        trust_level: String,
    },
    /// Task uses `trust: elevated` — bypasses spotlight/capability restrictions.
    #[has_task_id]
    TrustElevationUsed { task_id: Arc<str> },
    /// Spotlight fence applied to untrusted binding in prompt.
    #[has_task_id]
    SpotlightApplied {
        task_id: Arc<str>,
        binding_alias: String,
        trust_level: String,
    },
    /// Spotlight skipped (disabled globally or elevated).
    #[has_task_id]
    SpotlightSkipped { task_id: Arc<str>, reason: String },
    /// Agent tool restricted due to trust chain.
    #[has_task_id]
    AgentToolRestricted {
        task_id: Arc<str>,
        removed_tool: String,
        reason: String,
    },
    /// Canary token injected into system prompt.
    #[has_task_id]
    CanaryInjected { task_id: Arc<str> },
    /// Canary token detected in LLM output — ALERT!
    #[has_task_id]
    CanaryDetected {
        task_id: Arc<str>,
        match_type: String,
    },
    /// Output scanner finding (pattern detection).
    #[has_task_id]
    ScanFindingDetected {
        task_id: Arc<str>,
        category: String,
        severity: String,
    },
    /// Skill file integrity verified (blake3 hash match).
    SkillIntegrityVerified { path: String, hash: String },
    /// Skill file integrity failed (hash mismatch) — ALERT!
    SkillIntegrityFailed {
        path: String,
        expected: String,
        actual: String,
    },
    /// Capability denied at runtime.
    #[has_task_id]
    CapabilityDenied {
        task_id: Arc<str>,
        action: String,
        reason: String,
    },
    /// ML detection run (behind shield-ml feature flag).
    #[has_task_id]
    MlDetectionRun {
        task_id: Arc<str>,
        score: f64,
        latency_ms: u64,
    },
    /// ML detection blocked task (score above threshold).
    #[has_task_id]
    MlDetectionBlocked { task_id: Arc<str>, score: f64 },
}

impl EventKind {


    /// Check if this is a workflow-level event
    pub fn is_workflow_event(&self) -> bool {
        matches!(
            self,
            Self::WorkflowStarted { .. }
                | Self::WorkflowCompleted { .. }
                | Self::WorkflowFailed { .. }
                | Self::WorkflowAborted { .. }
                | Self::WorkflowPaused
                | Self::WorkflowResumed
                | Self::OrchestratorStarted { .. }
                | Self::OrchestratorRound { .. }
                | Self::OrchestratorSubWorkflow { .. }
                | Self::OrchestratorCompleted { .. }
                | Self::OrchestratorFailed { .. }
        )
    }
}

/// Thread-safe, append-only event log
///
/// Optionally supports real-time event broadcasting for TUI integration.
/// Use `new_with_broadcast()` to create an EventLog that sends events
/// to subscribers via tokio broadcast channel.
///
/// The event log has a capacity of [`MAX_EVENTS`]. When exceeded, the oldest
/// half is dropped to amortize eviction cost (H2 discovery fix).
///
/// Maximum events to keep in memory (~2-5 MB at 10,000 events).
const MAX_EVENTS: usize = 10_000;

#[derive(Clone)]
pub struct EventLog {
    events: Arc<RwLock<Vec<Event>>>,
    start_time: Instant,
    next_id: Arc<AtomicU64>,
    /// Optional broadcast sender for TUI real-time updates
    broadcast_tx: Option<broadcast::Sender<Event>>,
    /// Optional trace writer for incremental crash-resilient trace output.
    /// When attached, every emitted event is immediately flushed to the NDJSON
    /// trace file so that partial data survives a process crash.
    trace_writer: Option<Arc<TraceWriter>>,
}

impl EventLog {
    /// Create a new event log (call at workflow start)
    pub fn new() -> Self {
        Self {
            events: Arc::new(RwLock::new(Vec::with_capacity(256))),
            start_time: Instant::now(),
            next_id: Arc::new(AtomicU64::new(0)),
            broadcast_tx: None,
            trace_writer: None,
        }
    }

    /// Create a new event log with broadcast channel for TUI
    ///
    /// Returns (EventLog, Receiver) tuple. Pass the receiver to TUI App.
    /// Broadcast capacity: 4096 events (accommodate heavy for_each + fast providers).
    pub fn new_with_broadcast() -> (Self, broadcast::Receiver<Event>) {
        let (tx, rx) = broadcast::channel(4096);
        let event_log = Self {
            events: Arc::new(RwLock::new(Vec::with_capacity(256))),
            start_time: Instant::now(),
            next_id: Arc::new(AtomicU64::new(0)),
            broadcast_tx: Some(tx),
            trace_writer: None,
        };
        (event_log, rx)
    }

    /// Subscribe to event broadcasts (for additional TUI observers)
    ///
    /// Returns None if this EventLog was not created with `new_with_broadcast()`.
    pub fn subscribe(&self) -> Option<broadcast::Receiver<Event>> {
        self.broadcast_tx.as_ref().map(|tx| tx.subscribe())
    }

    /// Attach a trace writer for incremental crash-resilient output.
    ///
    /// Once attached, every event emitted via `emit()` is immediately
    /// flushed to the trace file. If the process crashes, the partial
    /// trace survives on disk.
    pub fn attach_trace_writer(&mut self, writer: TraceWriter) {
        self.trace_writer = Some(Arc::new(writer));
    }

    /// Emit an event (thread-safe, returns event ID)
    ///
    /// If broadcast channel is configured, also sends to subscribers.
    /// If a trace writer is attached, the event is immediately flushed
    /// to the NDJSON trace file for crash resilience.
    pub fn emit(&self, kind: EventKind) -> u64 {
        // Relaxed is sufficient: fetch_add guarantees unique monotonic IDs regardless
        // of ordering, and the subsequent RwLock::write() provides the memory fence.
        let id = self.next_id.fetch_add(1, Ordering::Relaxed);
        let event = Event {
            id,
            timestamp_ms: self.start_time.elapsed().as_millis() as u64,
            kind,
        };

        // PERF(M3): Clone only when broadcast or trace needs a copy.
        // In CLI mode (no broadcast, no trace), the event moves directly
        // into the Vec with zero clones.
        let needs_broadcast = self.broadcast_tx.is_some();
        let needs_trace = self.trace_writer.is_some();

        if needs_broadcast || needs_trace {
            // Clone for Vec storage; original moves to broadcast/trace
            let for_vec = event.clone();

            if let Some(ref writer) = self.trace_writer {
                if let Err(e) = writer.append_event(&event) {
                    tracing::warn!(error = %e, "Failed to write event to trace file");
                }
            }

            if let Some(ref tx) = self.broadcast_tx {
                if let Err(e) = tx.send(event) {
                    tracing::debug!(error = %e, "Broadcast send failed (no receivers)");
                }
            } else {
                drop(event); // consumed by trace above, explicit drop for clarity
            }

            let mut events = self.events.write();
            if events.len() >= MAX_EVENTS {
                // Drop oldest half to amortize eviction cost
                let drain_to = events.len() / 2;
                tracing::warn!(
                    evicted = drain_to,
                    limit = MAX_EVENTS,
                    "EventLog full — evicting oldest {drain_to} events (limit: {MAX_EVENTS})"
                );
                events.drain(..drain_to);
            }
            events.push(for_vec);
        } else {
            // Fast path: no clone needed
            let mut events = self.events.write();
            if events.len() >= MAX_EVENTS {
                let drain_to = events.len() / 2;
                tracing::warn!(
                    evicted = drain_to,
                    limit = MAX_EVENTS,
                    "EventLog full — evicting oldest {drain_to} events (limit: {MAX_EVENTS})"
                );
                events.drain(..drain_to);
            }
            events.push(event);
        }

        id
    }

    /// Get all events (cloned - use `with_events` for zero-copy access)
    pub fn events(&self) -> Vec<Event> {
        self.events.read().clone()
    }

    /// Zero-copy access to events via callback
    ///
    /// Holds read lock for duration of callback - keep it short.
    /// Use this instead of `events()` when you don't need ownership.
    pub fn with_events<T>(&self, f: impl FnOnce(&[Event]) -> T) -> T {
        f(&self.events.read())
    }

    /// Filter events by task ID (zero-copy filtering)
    pub fn filter_task(&self, task_id: &str) -> Vec<Event> {
        self.with_events(|events| {
            events
                .iter()
                .filter(|e| e.kind.task_id() == Some(task_id))
                .cloned()
                .collect()
        })
    }

    /// Filter workflow-level events only (zero-copy filtering)
    pub fn workflow_events(&self) -> Vec<Event> {
        self.with_events(|events| {
            events
                .iter()
                .filter(|e| e.kind.is_workflow_event())
                .cloned()
                .collect()
        })
    }

    /// Count events for a specific task (no allocation)
    pub fn count_task(&self, task_id: &str) -> usize {
        self.with_events(|events| {
            events
                .iter()
                .filter(|e| e.kind.task_id() == Some(task_id))
                .count()
        })
    }

    /// Serialize to JSON for persistence/debugging
    pub fn to_json(&self) -> Value {
        self.with_events(|events| serde_json::to_value(events).unwrap_or(Value::Null))
    }

    pub fn events_since(&self, since_id: Option<u64>) -> Vec<Event> {
        self.with_events(|events| {
            events
                .iter()
                .filter(|e| since_id.is_none_or(|last| e.id > last))
                .cloned()
                .collect()
        })
    }

    /// Iterate over events since `since_id` without cloning.
    ///
    /// The closure receives a slice of events that have not yet been rendered.
    /// Uses `partition_point` for O(log n) lookup since event IDs are monotonically
    /// increasing.
    pub fn with_events_since<R>(&self, since_id: Option<u64>, f: impl FnOnce(&[Event]) -> R) -> R {
        self.with_events(|events| {
            if let Some(last_id) = since_id {
                let start = events.partition_point(|e| e.id <= last_id);
                f(&events[start..])
            } else {
                f(events)
            }
        })
    }

    /// Number of events
    pub fn len(&self) -> usize {
        self.events.read().len()
    }

    /// Check if empty
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

impl Default for EventLog {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Debug for EventLog {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("EventLog")
            .field("len", &self.len())
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    /// Use actual package version in tests to avoid version drift
    const TEST_VERSION: &str = env!("CARGO_PKG_VERSION");

    // ═══════════════════════════════════════════════════════════════
    // Test helpers
    // ═══════════════════════════════════════════════════════════════

    /// Create a WorkflowStarted event with test defaults
    fn workflow_started(task_count: usize) -> EventKind {
        EventKind::WorkflowStarted {
            task_count,
            generation_id: "test-gen-123".to_string(),
            workflow_hash: "abc123".to_string(),
            nika_version: TEST_VERSION.to_string(),
        }
    }

    /// Create a ProviderResponded event with test defaults
    fn provider_responded(task_id: &str, input_tokens: u64, output_tokens: u64) -> EventKind {
        EventKind::ProviderResponded {
            task_id: Arc::from(task_id),
            request_id: Some("req-456".to_string()),
            input_tokens,
            output_tokens,
            cache_read_tokens: 0,
            ttft_ms: Some(150),
            finish_reason: crate::types::FinishReason::Stop,
            cost_usd: 0.001,
        }
    }

    // ═══════════════════════════════════════════════════════════════
    // Event + EventKind tests
    // ═══════════════════════════════════════════════════════════════

    #[test]
    fn eventkind_task_id_extraction() {
        let started = EventKind::TaskStarted {
            verb: "infer".into(),
            task_id: "task1".into(),
            inputs: Arc::new(json!({})),
        };
        assert_eq!(started.task_id(), Some("task1"));

        let workflow = workflow_started(5);
        assert_eq!(workflow.task_id(), None);
    }

    #[test]
    fn eventkind_is_workflow_event() {
        assert!(workflow_started(3).is_workflow_event());
        assert!(EventKind::WorkflowCompleted {
            final_output: Arc::new(json!("done")),
            total_duration_ms: 1000,
        }
        .is_workflow_event());
        assert!(!EventKind::TaskStarted {
            verb: "infer".into(),
            task_id: "t1".into(),
            inputs: Arc::new(json!({})),
        }
        .is_workflow_event());
    }

    #[test]
    fn eventkind_serializes_with_type_tag() {
        let kind = EventKind::TaskCompleted {
            task_id: "greet".into(),
            output: Arc::new(json!({"message": "Hello"})),
            duration_ms: 150,
        };

        let json = serde_json::to_value(&kind).unwrap();
        assert_eq!(json["type"], "task_completed");
        assert_eq!(json["task_id"], "greet");
        assert_eq!(json["output"]["message"], "Hello");
    }

    #[test]
    fn eventkind_deserializes_from_tagged_json() {
        let json = json!({
            "type": "task_started",
            "task_id": "analyze",
            "verb": "infer",
            "inputs": {"weather": "sunny"}
        });

        let kind: EventKind = serde_json::from_value(json).unwrap();
        assert_eq!(
            kind,
            EventKind::TaskStarted {
                task_id: "analyze".into(),
                verb: "infer".into(),
                inputs: Arc::new(json!({"weather": "sunny"})),
            }
        );
    }

    // ═══════════════════════════════════════════════════════════════
    // S6 telemetry variant serde roundtrip tests
    // ═══════════════════════════════════════════════════════════════

    #[test]
    fn template_resolution_failed_serde_roundtrip() {
        let event = EventKind::TemplateResolutionFailed {
            task_id: Arc::from("my_task"),
            template: "{{with.missing}}".to_string(),
            error: "unknown alias 'missing'".to_string(),
        };
        let json = serde_json::to_string(&event).unwrap();
        assert!(json.contains(r#""type":"template_resolution_failed""#));
        let rt: EventKind = serde_json::from_str(&json).unwrap();
        assert_eq!(event, rt);
        assert_eq!(rt.task_id(), Some("my_task"));
    }

    #[test]
    fn rate_limit_delay_serde_roundtrip() {
        let event = EventKind::RateLimitDelay {
            task_id: Arc::from("fetch_step"),
            domain: "api.example.com".to_string(),
            delay_ms: 1500,
        };
        let json = serde_json::to_string(&event).unwrap();
        assert!(json.contains(r#""type":"rate_limit_delay""#));
        let rt: EventKind = serde_json::from_str(&json).unwrap();
        assert_eq!(event, rt);
        assert_eq!(rt.task_id(), Some("fetch_step"));
    }

    #[test]
    fn schema_load_failed_serde_roundtrip() {
        let event = EventKind::SchemaLoadFailed {
            task_id: Arc::from("extract"),
            schema_path: "./schema.json".to_string(),
            error: "No such file".to_string(),
        };
        let json = serde_json::to_string(&event).unwrap();
        assert!(json.contains(r#""type":"schema_load_failed""#));
        let rt: EventKind = serde_json::from_str(&json).unwrap();
        assert_eq!(event, rt);
        assert_eq!(rt.task_id(), Some("extract"));
    }

    #[test]
    fn vision_content_failed_serde_roundtrip() {
        let event = EventKind::VisionContentFailed {
            task_id: Arc::from("analyze_image"),
            source: "blake3:abc123".to_string(),
            stage: "cas_read".to_string(),
            error: "blob not found".to_string(),
        };
        let json = serde_json::to_string(&event).unwrap();
        assert!(json.contains(r#""type":"vision_content_failed""#));
        let rt: EventKind = serde_json::from_str(&json).unwrap();
        assert_eq!(event, rt);
        assert_eq!(rt.task_id(), Some("analyze_image"));
    }

    // ═══════════════════════════════════════════════════════════════
    // EventLog tests
    // ═══════════════════════════════════════════════════════════════

    #[test]
    fn eventlog_new_starts_empty() {
        let log = EventLog::new();
        assert!(log.is_empty());
        assert_eq!(log.len(), 0);
    }

    #[test]
    fn eventlog_emit_returns_monotonic_ids() {
        let log = EventLog::new();

        let id1 = log.emit(workflow_started(3));
        let id2 = log.emit(EventKind::TaskStarted {
            verb: "infer".into(),
            task_id: "t1".into(),
            inputs: Arc::new(json!({})),
        });
        let id3 = log.emit(EventKind::TaskStarted {
            verb: "infer".into(),
            task_id: "t2".into(),
            inputs: Arc::new(json!({})),
        });

        assert_eq!(id1, 0);
        assert_eq!(id2, 1);
        assert_eq!(id3, 2);
        assert_eq!(log.len(), 3);
    }

    #[test]
    fn eventlog_events_returns_all() {
        let log = EventLog::new();
        log.emit(workflow_started(2));
        log.emit(EventKind::TaskStarted {
            verb: "infer".into(),
            task_id: "t1".into(),
            inputs: Arc::new(json!({})),
        });

        let events = log.events();
        assert_eq!(events.len(), 2);
        assert_eq!(events[0].id, 0);
        assert_eq!(events[1].id, 1);
    }

    #[test]
    fn eventlog_filter_task_returns_only_matching() {
        let log = EventLog::new();
        log.emit(workflow_started(2));
        log.emit(EventKind::TaskStarted {
            verb: "infer".into(),
            task_id: "alpha".into(),
            inputs: Arc::new(json!({})),
        });
        log.emit(EventKind::TaskStarted {
            verb: "infer".into(),
            task_id: "beta".into(),
            inputs: Arc::new(json!({})),
        });
        log.emit(EventKind::TaskCompleted {
            task_id: "alpha".into(),
            output: Arc::new(json!("result")),
            duration_ms: 100,
        });

        let alpha_events = log.filter_task("alpha");
        assert_eq!(alpha_events.len(), 2); // Started + Completed
        assert!(alpha_events
            .iter()
            .all(|e| e.kind.task_id() == Some("alpha")));

        let beta_events = log.filter_task("beta");
        assert_eq!(beta_events.len(), 1);
    }

    #[test]
    fn eventlog_workflow_events_returns_only_workflow() {
        let log = EventLog::new();
        log.emit(workflow_started(1));
        log.emit(EventKind::TaskStarted {
            verb: "infer".into(),
            task_id: "t1".into(),
            inputs: Arc::new(json!({})),
        });
        log.emit(EventKind::WorkflowCompleted {
            final_output: Arc::new(json!("done")),
            total_duration_ms: 500,
        });

        let wf_events = log.workflow_events();
        assert_eq!(wf_events.len(), 2);
        assert!(wf_events.iter().all(|e| e.kind.is_workflow_event()));
    }

    #[test]
    fn eventlog_to_json() {
        let log = EventLog::new();
        log.emit(EventKind::TaskStarted {
            verb: "infer".into(),
            task_id: "task1".into(),
            inputs: Arc::new(json!({})),
        });

        let json = log.to_json();
        assert!(json.is_array());
        assert_eq!(json.as_array().unwrap().len(), 1);
        assert_eq!(json[0]["kind"]["type"], "task_started");
    }

    #[test]
    fn eventlog_is_clone() {
        let log = EventLog::new();
        log.emit(workflow_started(1));

        let cloned = log.clone();
        assert_eq!(cloned.len(), 1);

        // Cloned shares the same underlying data (Arc)
        log.emit(EventKind::TaskStarted {
            verb: "infer".into(),
            task_id: "t1".into(),
            inputs: Arc::new(json!({})),
        });
        assert_eq!(cloned.len(), 2);
    }

    #[test]
    fn eventlog_thread_safe_concurrent_emits() {
        use std::thread;

        let log = EventLog::new();

        let handles: Vec<_> = (0..10)
            .map(|i| {
                let log = log.clone();
                thread::spawn(move || {
                    log.emit(EventKind::TaskStarted {
                        verb: "infer".into(),
                        task_id: Arc::from(format!("task{}", i)),
                        inputs: Arc::new(json!({})),
                    })
                })
            })
            .collect();

        for h in handles {
            h.join().unwrap();
        }

        assert_eq!(log.len(), 10);

        // All IDs should be unique
        let events = log.events();
        let mut ids: Vec<u64> = events.iter().map(|e| e.id).collect();
        ids.sort();
        ids.dedup();
        assert_eq!(ids.len(), 10);
    }

    #[test]
    fn event_timestamp_is_relative() {
        let log = EventLog::new();

        // First event should have small timestamp
        log.emit(workflow_started(1));

        std::thread::sleep(std::time::Duration::from_millis(10));

        log.emit(EventKind::TaskStarted {
            verb: "infer".into(),
            task_id: "t1".into(),
            inputs: Arc::new(json!({})),
        });

        let events = log.events();
        assert!(events[1].timestamp_ms >= events[0].timestamp_ms);
    }

    // ═══════════════════════════════════════════════════════════════
    // TaskStarted captures resolved inputs
    // ═══════════════════════════════════════════════════════════════

    #[test]
    fn task_started_captures_full_context() {
        let log = EventLog::new();

        let inputs = json!({
            "weather": "sunny",
            "temperature": 25,
            "nested": {"key": "value"}
        });

        log.emit(EventKind::TaskStarted {
            verb: "infer".into(),
            task_id: "analyze".into(),
            inputs: Arc::new(inputs.clone()),
        });

        let events = log.filter_task("analyze");
        assert_eq!(events.len(), 1);

        if let EventKind::TaskStarted {
            inputs: captured, ..
        } = &events[0].kind
        {
            assert_eq!(captured.as_ref(), &inputs);
            assert_eq!(captured["weather"], "sunny");
            assert_eq!(captured["nested"]["key"], "value");
        } else {
            panic!("Expected TaskStarted event");
        }
    }

    // ═══════════════════════════════════════════════════════════════
    // Enhanced event tests
    // ═══════════════════════════════════════════════════════════════

    #[test]
    fn workflow_started_includes_generation_id() {
        let log = EventLog::new();
        log.emit(EventKind::WorkflowStarted {
            task_count: 3,
            generation_id: "gen-abc-123".to_string(),
            workflow_hash: "sha256:deadbeef".to_string(),
            nika_version: TEST_VERSION.to_string(),
        });

        let events = log.events();
        if let EventKind::WorkflowStarted {
            generation_id,
            workflow_hash,
            nika_version,
            ..
        } = &events[0].kind
        {
            assert_eq!(generation_id, "gen-abc-123");
            assert_eq!(workflow_hash, "sha256:deadbeef");
            assert_eq!(nika_version, TEST_VERSION);
        } else {
            panic!("Expected WorkflowStarted event");
        }
    }

    #[test]
    fn provider_responded_tracks_detailed_tokens() {
        let log = EventLog::new();
        log.emit(EventKind::ProviderResponded {
            task_id: "infer_task".into(),
            request_id: Some("req-xyz-789".to_string()),
            input_tokens: 500,
            output_tokens: 150,
            cache_read_tokens: 200,
            ttft_ms: Some(85),
            finish_reason: crate::types::FinishReason::Stop,
            cost_usd: 0.0025,
        });

        let events = log.filter_task("infer_task");
        assert_eq!(events.len(), 1);

        if let EventKind::ProviderResponded {
            request_id,
            input_tokens,
            output_tokens,
            cache_read_tokens,
            ttft_ms,
            finish_reason,
            cost_usd,
            ..
        } = &events[0].kind
        {
            assert_eq!(request_id, &Some("req-xyz-789".to_string()));
            assert_eq!(*input_tokens, 500);
            assert_eq!(*output_tokens, 150);
            assert_eq!(*cache_read_tokens, 200);
            assert_eq!(*ttft_ms, Some(85));
            assert_eq!(*finish_reason, crate::types::FinishReason::Stop);
            assert!((*cost_usd - 0.0025).abs() < f64::EPSILON);
        } else {
            panic!("Expected ProviderResponded event");
        }
    }

    #[test]
    fn context_assembled_tracks_sources() {
        let log = EventLog::new();

        let sources = vec![
            ContextSource {
                node: "system_prompt".to_string(),
                tokens: 200,
            },
            ContextSource {
                node: "user_input".to_string(),
                tokens: 50,
            },
            ContextSource {
                node: "examples".to_string(),
                tokens: 300,
            },
        ];

        let excluded = vec![ExcludedItem {
            node: "large_history".to_string(),
            reason: "exceeded budget".to_string(),
        }];

        log.emit(EventKind::ContextAssembled {
            task_id: "assemble_task".into(),
            sources: sources.clone(),
            excluded: excluded.clone(),
            total_tokens: 550,
            budget_used_pct: 55.0,
            truncated: false,
        });

        let events = log.filter_task("assemble_task");
        assert_eq!(events.len(), 1);

        if let EventKind::ContextAssembled {
            sources: s,
            excluded: e,
            total_tokens,
            budget_used_pct,
            truncated,
            ..
        } = &events[0].kind
        {
            assert_eq!(s.len(), 3);
            assert_eq!(s[0].node, "system_prompt");
            assert_eq!(s[0].tokens, 200);
            assert_eq!(e.len(), 1);
            assert_eq!(e[0].reason, "exceeded budget");
            assert_eq!(*total_tokens, 550);
            assert!((*budget_used_pct - 55.0).abs() < f32::EPSILON);
            assert!(!*truncated);
        } else {
            panic!("Expected ContextAssembled event");
        }
    }

    #[test]
    fn context_source_and_excluded_item_serialize() {
        let source = ContextSource {
            node: "test_node".to_string(),
            tokens: 100,
        };
        let json = serde_json::to_value(&source).unwrap();
        assert_eq!(json["node"], "test_node");
        assert_eq!(json["tokens"], 100);

        let excluded = ExcludedItem {
            node: "big_file".to_string(),
            reason: "too large".to_string(),
        };
        let json = serde_json::to_value(&excluded).unwrap();
        assert_eq!(json["node"], "big_file");
        assert_eq!(json["reason"], "too large");
    }

    #[test]
    fn provider_responded_helper_creates_valid_event() {
        let event = provider_responded("test_task", 100, 50);
        assert_eq!(event.task_id(), Some("test_task"));

        if let EventKind::ProviderResponded {
            input_tokens,
            output_tokens,
            ..
        } = event
        {
            assert_eq!(input_tokens, 100);
            assert_eq!(output_tokens, 50);
        } else {
            panic!("Expected ProviderResponded event");
        }
    }

    // ═══════════════════════════════════════════════════════════════
    // AgentTurnMetadata tests
    // ═══════════════════════════════════════════════════════════════

    #[test]
    fn agent_turn_metadata_text_only() {
        let metadata = AgentTurnMetadata::text_only("Hello world", "end_turn");

        assert_eq!(metadata.response_text, "Hello world");
        assert_eq!(metadata.stop_reason, crate::types::AgentStopReason::EndTurn);
        assert_eq!(metadata.input_tokens, 0);
        assert_eq!(metadata.output_tokens, 0);
        assert_eq!(metadata.cache_read_tokens, 0);
        assert!(!metadata.has_thinking());
        assert_eq!(metadata.total_tokens(), 0);
    }

    #[test]
    fn agent_turn_metadata_with_usage() {
        let metadata = AgentTurnMetadata::with_usage("Response", 100, 50, "tool_use");

        assert_eq!(metadata.response_text, "Response");
        assert_eq!(metadata.stop_reason, crate::types::AgentStopReason::ToolUse);
        assert_eq!(metadata.input_tokens, 100);
        assert_eq!(metadata.output_tokens, 50);
        assert_eq!(metadata.total_tokens(), 150);
        assert!(!metadata.has_thinking());
    }

    #[test]
    fn agent_turn_metadata_with_thinking() {
        let metadata = AgentTurnMetadata {
            thinking: Some("Let me think about this...".to_string()),
            response_text: "Here's my answer".to_string(),
            input_tokens: 200,
            output_tokens: 100,
            cache_read_tokens: 50,
            stop_reason: crate::types::AgentStopReason::EndTurn,
        };

        assert!(metadata.has_thinking());
        assert_eq!(
            metadata.thinking.as_ref().unwrap(),
            "Let me think about this..."
        );
        assert_eq!(metadata.total_tokens(), 300);
    }

    #[test]
    fn agent_turn_metadata_serializes() {
        let metadata = AgentTurnMetadata::with_usage("Test response", 100, 50, "end_turn");
        let json = serde_json::to_value(&metadata).unwrap();

        assert_eq!(json["response_text"], "Test response");
        assert_eq!(json["input_tokens"], 100);
        assert_eq!(json["output_tokens"], 50);
        assert_eq!(json["stop_reason"], "end_turn");
        // thinking should be skipped when None
        assert!(json.get("thinking").is_none());
    }

    #[test]
    fn agent_turn_metadata_with_thinking_serializes() {
        let metadata = AgentTurnMetadata {
            thinking: Some("My thoughts".to_string()),
            response_text: "My response".to_string(),
            input_tokens: 50,
            output_tokens: 25,
            cache_read_tokens: 0,
            stop_reason: crate::types::AgentStopReason::EndTurn,
        };
        let json = serde_json::to_value(&metadata).unwrap();

        assert_eq!(json["thinking"], "My thoughts");
        assert_eq!(json["response_text"], "My response");
    }

    #[test]
    fn agent_turn_with_metadata_serializes() {
        let log = EventLog::new();

        let metadata = AgentTurnMetadata::with_usage("Agent response", 100, 50, "end_turn");

        log.emit(EventKind::AgentTurn {
            task_id: "agent_task".into(),
            turn_index: 1,
            kind: crate::types::AgentTurnKind::from("end_turn"), // Canonical snake_case
            metadata: Some(metadata),
        });

        let events = log.filter_task("agent_task");
        assert_eq!(events.len(), 1);

        if let EventKind::AgentTurn {
            metadata: Some(m), ..
        } = &events[0].kind
        {
            assert_eq!(m.response_text, "Agent response");
            assert_eq!(m.total_tokens(), 150);
        } else {
            panic!("Expected AgentTurn with metadata");
        }
    }

    #[test]
    fn agent_turn_without_metadata_serializes() {
        let log = EventLog::new();

        log.emit(EventKind::AgentTurn {
            task_id: "agent_task".into(),
            turn_index: 1,
            kind: crate::types::AgentTurnKind::Started,
            metadata: None,
        });

        let events = log.filter_task("agent_task");
        assert_eq!(events.len(), 1);

        if let EventKind::AgentTurn { metadata, kind, .. } = &events[0].kind {
            assert!(metadata.is_none());
            assert_eq!(*kind, crate::types::AgentTurnKind::Started);
        } else {
            panic!("Expected AgentTurn without metadata");
        }
    }

    // ═══════════════════════════════════════════════════════════════
    // Structured Output events tests
    // ═══════════════════════════════════════════════════════════════

    #[test]
    fn structured_output_attempt_success() {
        let log = EventLog::new();

        log.emit(EventKind::StructuredOutputAttempt {
            task_id: "extract_task".into(),
            layer: 1,
            layer_name: "rig_extractor".to_string(),
            attempt: 1,
            success: true,
            error: None,
        });

        let events = log.filter_task("extract_task");
        assert_eq!(events.len(), 1);

        if let EventKind::StructuredOutputAttempt {
            layer,
            layer_name,
            attempt,
            success,
            error,
            ..
        } = &events[0].kind
        {
            assert_eq!(*layer, 1);
            assert_eq!(layer_name, "rig_extractor");
            assert_eq!(*attempt, 1);
            assert!(*success);
            assert!(error.is_none());
        } else {
            panic!("Expected StructuredOutputAttempt event");
        }
    }

    #[test]
    fn structured_output_attempt_failure() {
        let log = EventLog::new();

        log.emit(EventKind::StructuredOutputAttempt {
            task_id: "extract_task".into(),
            layer: 2,
            layer_name: "extract_validate".to_string(),
            attempt: 2,
            success: false,
            error: Some("Missing required field 'name'".to_string()),
        });

        let events = log.filter_task("extract_task");
        assert_eq!(events.len(), 1);

        if let EventKind::StructuredOutputAttempt {
            layer,
            layer_name,
            attempt,
            success,
            error,
            ..
        } = &events[0].kind
        {
            assert_eq!(*layer, 2);
            assert_eq!(layer_name, "extract_validate");
            assert_eq!(*attempt, 2);
            assert!(!*success);
            assert_eq!(error.as_ref().unwrap(), "Missing required field 'name'");
        } else {
            panic!("Expected StructuredOutputAttempt event");
        }
    }

    #[test]
    fn structured_output_success_event() {
        let log = EventLog::new();

        log.emit(EventKind::StructuredOutputSuccess {
            task_id: "extract_task".into(),
            layer: 3,
            layer_name: "retry_with_feedback".to_string(),
            total_attempts: 4,
        });

        let events = log.filter_task("extract_task");
        assert_eq!(events.len(), 1);

        if let EventKind::StructuredOutputSuccess {
            layer,
            layer_name,
            total_attempts,
            ..
        } = &events[0].kind
        {
            assert_eq!(*layer, 3);
            assert_eq!(layer_name, "retry_with_feedback");
            assert_eq!(*total_attempts, 4);
        } else {
            panic!("Expected StructuredOutputSuccess event");
        }
    }

    #[test]
    fn structured_output_attempt_serializes() {
        let event = EventKind::StructuredOutputAttempt {
            task_id: "task1".into(),
            layer: 1,
            layer_name: "rig_extractor".to_string(),
            attempt: 1,
            success: true,
            error: None,
        };

        let json = serde_json::to_value(&event).unwrap();
        assert_eq!(json["type"], "structured_output_attempt");
        assert_eq!(json["task_id"], "task1");
        assert_eq!(json["layer"], 1);
        assert_eq!(json["layer_name"], "rig_extractor");
        assert_eq!(json["attempt"], 1);
        assert_eq!(json["success"], true);
        // error should be skipped when None
        assert!(json.get("error").is_none());
    }

    #[test]
    fn structured_output_attempt_with_error_serializes() {
        let event = EventKind::StructuredOutputAttempt {
            task_id: "task1".into(),
            layer: 4,
            layer_name: "llm_repair".to_string(),
            attempt: 1,
            success: false,
            error: Some("Repair failed".to_string()),
        };

        let json = serde_json::to_value(&event).unwrap();
        assert_eq!(json["type"], "structured_output_attempt");
        assert_eq!(json["layer"], 4);
        assert_eq!(json["layer_name"], "llm_repair");
        assert_eq!(json["success"], false);
        assert_eq!(json["error"], "Repair failed");
    }

    #[test]
    fn structured_output_success_serializes() {
        let event = EventKind::StructuredOutputSuccess {
            task_id: "task1".into(),
            layer: 2,
            layer_name: "extract_validate".to_string(),
            total_attempts: 3,
        };

        let json = serde_json::to_value(&event).unwrap();
        assert_eq!(json["type"], "structured_output_success");
        assert_eq!(json["task_id"], "task1");
        assert_eq!(json["layer"], 2);
        assert_eq!(json["layer_name"], "extract_validate");
        assert_eq!(json["total_attempts"], 3);
    }

    #[test]
    fn structured_output_events_task_id_extraction() {
        let attempt = EventKind::StructuredOutputAttempt {
            task_id: "extract1".into(),
            layer: 1,
            layer_name: "rig_extractor".to_string(),
            attempt: 1,
            success: true,
            error: None,
        };
        assert_eq!(attempt.task_id(), Some("extract1"));

        let success = EventKind::StructuredOutputSuccess {
            task_id: "extract2".into(),
            layer: 2,
            layer_name: "extract_validate".to_string(),
            total_attempts: 1,
        };
        assert_eq!(success.task_id(), Some("extract2"));
    }

    #[test]
    fn structured_output_full_workflow() {
        // Simulates a full structured output workflow with multiple attempts
        let log = EventLog::new();

        // Layer 1 attempt 1: fails
        log.emit(EventKind::StructuredOutputAttempt {
            task_id: "parse_json".into(),
            layer: 1,
            layer_name: "rig_extractor".to_string(),
            attempt: 1,
            success: false,
            error: Some("JSON parse error".to_string()),
        });

        // Layer 2 attempt 1: fails
        log.emit(EventKind::StructuredOutputAttempt {
            task_id: "parse_json".into(),
            layer: 2,
            layer_name: "extract_validate".to_string(),
            attempt: 1,
            success: false,
            error: Some("Schema validation failed".to_string()),
        });

        // Layer 3 attempt 1: fails
        log.emit(EventKind::StructuredOutputAttempt {
            task_id: "parse_json".into(),
            layer: 3,
            layer_name: "retry_with_feedback".to_string(),
            attempt: 1,
            success: false,
            error: Some("Still invalid".to_string()),
        });

        // Layer 3 attempt 2: succeeds
        log.emit(EventKind::StructuredOutputAttempt {
            task_id: "parse_json".into(),
            layer: 3,
            layer_name: "retry_with_feedback".to_string(),
            attempt: 2,
            success: true,
            error: None,
        });

        // Final success
        log.emit(EventKind::StructuredOutputSuccess {
            task_id: "parse_json".into(),
            layer: 3,
            layer_name: "retry_with_feedback".to_string(),
            total_attempts: 4,
        });

        let events = log.filter_task("parse_json");
        assert_eq!(events.len(), 5);

        // Verify attempt sequence
        let attempts: Vec<_> = events
            .iter()
            .filter_map(|e| {
                if let EventKind::StructuredOutputAttempt {
                    layer,
                    attempt,
                    success,
                    ..
                } = &e.kind
                {
                    Some((*layer, *attempt, *success))
                } else {
                    None
                }
            })
            .collect();

        assert_eq!(
            attempts,
            vec![
                (1, 1, false), // Layer 1, attempt 1, failed
                (2, 1, false), // Layer 2, attempt 1, failed
                (3, 1, false), // Layer 3, attempt 1, failed
                (3, 2, true),  // Layer 3, attempt 2, success
            ]
        );

        // Verify final success
        if let EventKind::StructuredOutputSuccess {
            layer,
            total_attempts,
            ..
        } = &events[4].kind
        {
            assert_eq!(*layer, 3);
            assert_eq!(*total_attempts, 4);
        } else {
            panic!("Expected StructuredOutputSuccess as last event");
        }
    }

    // ═══════════════════════════════════════════════════════════════
    // Guardrail events tests
    // ═══════════════════════════════════════════════════════════════

    #[test]
    fn guardrail_passed_event() {
        let log = EventLog::new();

        log.emit(EventKind::GuardrailPassed {
            task_id: "agent_task".into(),
            guardrail_type: crate::types::GuardrailType::Length,
            description: "min_words: 10".to_string(),
        });

        let events = log.filter_task("agent_task");
        assert_eq!(events.len(), 1);

        if let EventKind::GuardrailPassed {
            guardrail_type,
            description,
            ..
        } = &events[0].kind
        {
            assert_eq!(*guardrail_type, crate::types::GuardrailType::Length);
            assert_eq!(description, "min_words: 10");
        } else {
            panic!("Expected GuardrailPassed event");
        }
    }

    #[test]
    fn guardrail_failed_event() {
        let log = EventLog::new();

        log.emit(EventKind::GuardrailFailed {
            task_id: "agent_task".into(),
            guardrail_type: crate::types::GuardrailType::Regex,
            description: "must_contain_email".to_string(),
            message: "Output does not match pattern: [a-z]+@[a-z]+\\.[a-z]+".to_string(),
        });

        let events = log.filter_task("agent_task");
        assert_eq!(events.len(), 1);

        if let EventKind::GuardrailFailed {
            guardrail_type,
            description,
            message,
            ..
        } = &events[0].kind
        {
            assert_eq!(*guardrail_type, crate::types::GuardrailType::Regex);
            assert_eq!(description, "must_contain_email");
            assert!(message.contains("does not match pattern"));
        } else {
            panic!("Expected GuardrailFailed event");
        }
    }

    #[test]
    fn guardrail_passed_serializes() {
        let event = EventKind::GuardrailPassed {
            task_id: "task1".into(),
            guardrail_type: crate::types::GuardrailType::Schema,
            description: "output_schema".to_string(),
        };

        let json = serde_json::to_value(&event).unwrap();
        assert_eq!(json["type"], "guardrail_passed");
        assert_eq!(json["task_id"], "task1");
        assert_eq!(json["guardrail_type"], "schema");
        assert_eq!(json["description"], "output_schema");
    }

    #[test]
    fn guardrail_failed_serializes() {
        let event = EventKind::GuardrailFailed {
            task_id: "task1".into(),
            guardrail_type: crate::types::GuardrailType::Length,
            description: "max_chars: 100".to_string(),
            message: "Output has 150 chars, max is 100".to_string(),
        };

        let json = serde_json::to_value(&event).unwrap();
        assert_eq!(json["type"], "guardrail_failed");
        assert_eq!(json["task_id"], "task1");
        assert_eq!(json["guardrail_type"], "length");
        assert_eq!(json["description"], "max_chars: 100");
        assert_eq!(json["message"], "Output has 150 chars, max is 100");
    }

    #[test]
    fn guardrail_events_task_id_extraction() {
        let passed = EventKind::GuardrailPassed {
            task_id: "guard1".into(),
            guardrail_type: crate::types::GuardrailType::Length,
            description: "min_words: 5".to_string(),
        };
        assert_eq!(passed.task_id(), Some("guard1"));

        let failed = EventKind::GuardrailFailed {
            task_id: "guard2".into(),
            guardrail_type: crate::types::GuardrailType::Regex,
            description: "pattern".to_string(),
            message: "No match".to_string(),
        };
        assert_eq!(failed.task_id(), Some("guard2"));
    }

    #[test]
    fn guardrail_events_full_workflow() {
        // Simulates a guardrail check workflow with mixed pass/fail
        let log = EventLog::new();

        // Length guardrail passes
        log.emit(EventKind::GuardrailPassed {
            task_id: "validate_output".into(),
            guardrail_type: crate::types::GuardrailType::Length,
            description: "min_words: 10, max_words: 100".to_string(),
        });

        // Schema guardrail fails
        log.emit(EventKind::GuardrailFailed {
            task_id: "validate_output".into(),
            guardrail_type: crate::types::GuardrailType::Schema,
            description: "json_schema".to_string(),
            message: "Missing required field: 'title'".to_string(),
        });

        // Regex guardrail passes
        log.emit(EventKind::GuardrailPassed {
            task_id: "validate_output".into(),
            guardrail_type: crate::types::GuardrailType::Regex,
            description: "contains_email".to_string(),
        });

        let events = log.filter_task("validate_output");
        assert_eq!(events.len(), 3);

        // Count passes and fails
        let passed_count = events
            .iter()
            .filter(|e| matches!(&e.kind, EventKind::GuardrailPassed { .. }))
            .count();
        let failed_count = events
            .iter()
            .filter(|e| matches!(&e.kind, EventKind::GuardrailFailed { .. }))
            .count();

        assert_eq!(passed_count, 2);
        assert_eq!(failed_count, 1);
    }

    #[test]
    fn guardrail_escalation_event() {
        let log = EventLog::new();

        log.emit(EventKind::GuardrailEscalation {
            task_id: "agent_task".into(),
            guardrail_type: crate::types::GuardrailType::Llm,
            guardrail_id: "content_safety".to_string(),
            message: "Content may be inappropriate for the target audience".to_string(),
            severity: crate::types::Severity::High,
            suggested_action: Some("Review output before publishing".to_string()),
        });

        let events = log.filter_task("agent_task");
        assert_eq!(events.len(), 1);

        if let EventKind::GuardrailEscalation {
            guardrail_type,
            guardrail_id,
            message,
            severity,
            suggested_action,
            ..
        } = &events[0].kind
        {
            assert_eq!(*guardrail_type, crate::types::GuardrailType::Llm);
            assert_eq!(guardrail_id, "content_safety");
            assert!(message.contains("inappropriate"));
            assert_eq!(*severity, crate::types::Severity::High);
            assert!(suggested_action.is_some());
        } else {
            panic!("Expected GuardrailEscalation event");
        }
    }

    #[test]
    fn guardrail_escalation_serializes() {
        let event = EventKind::GuardrailEscalation {
            task_id: "task1".into(),
            guardrail_type: crate::types::GuardrailType::Llm,
            guardrail_id: "safety_check".to_string(),
            message: "Safety violation detected".to_string(),
            severity: crate::types::Severity::Critical,
            suggested_action: None,
        };

        let json = serde_json::to_value(&event).unwrap();
        assert_eq!(json["type"], "guardrail_escalation");
        assert_eq!(json["task_id"], "task1");
        assert_eq!(json["guardrail_type"], "llm");
        assert_eq!(json["guardrail_id"], "safety_check");
        assert_eq!(json["severity"], "critical");
    }

    #[test]
    fn guardrail_escalation_task_id_extraction() {
        let escalation = EventKind::GuardrailEscalation {
            task_id: "esc1".into(),
            guardrail_type: crate::types::GuardrailType::Llm,
            guardrail_id: "quality".to_string(),
            message: "Quality below threshold".to_string(),
            severity: crate::types::Severity::Medium,
            suggested_action: None,
        };
        assert_eq!(escalation.task_id(), Some("esc1"));
    }

    // ═══════════════════════════════════════════════════════════════
    // WAVE 2 TESTS: NDJSON event system audit
    // ═══════════════════════════════════════════════════════════════

    /// Helper: build one instance of every EventKind variant (all 37)
    fn all_38_variants() -> Vec<EventKind> {
        vec![
            // Workflow (6)
            EventKind::WorkflowStarted {
                task_count: 3,
                generation_id: "gen-1".into(),
                workflow_hash: "abc123".into(),
                nika_version: env!("CARGO_PKG_VERSION").into(),
            },
            EventKind::WorkflowCompleted {
                final_output: Arc::new(serde_json::json!({"result": "ok"})),
                total_duration_ms: 1234,
            },
            EventKind::WorkflowFailed {
                error: "boom".into(),
                failed_task: Some("task1".into()),
            },
            EventKind::WorkflowAborted {
                reason: "timeout".into(),
                duration_ms: 500,
                running_tasks: vec!["t1".into(), "t2".into()],
            },
            EventKind::WorkflowPaused,
            EventKind::WorkflowResumed,
            // Task (4)
            EventKind::TaskScheduled {
                task_id: "t1".into(),
                dependencies: vec!["t0".into()],
            },
            EventKind::TaskStarted {
                task_id: "t1".into(),
                verb: "infer".into(),
                inputs: Arc::new(serde_json::json!({})),
            },
            EventKind::TaskCompleted {
                task_id: "t1".into(),
                output: Arc::new(serde_json::json!("done")),
                duration_ms: 100,
            },
            EventKind::TaskFailed {
                task_id: "t1".into(),
                error: "fail".into(),
                duration_ms: 50,
                error_code: None,
            },
            // Fine-grained (3)
            EventKind::TemplateResolved {
                task_id: "t1".into(),
                template: "{{with.x}}".into(),
                result: "hello".into(),
            },
            EventKind::ProviderCalled {
                task_id: "t1".into(),
                provider: "anthropic".into(),
                model: "claude-3-haiku".into(),
                prompt_len: 42,
                endpoint_url: None,
            },
            EventKind::ProviderResponded {
                task_id: "t1".into(),
                request_id: Some("req-abc".into()),
                input_tokens: 100,
                output_tokens: 50,
                cache_read_tokens: 10,
                ttft_ms: Some(200),
                finish_reason: crate::types::FinishReason::EndTurn,
                cost_usd: 0.001,
            },
            // Context (1)
            EventKind::ContextAssembled {
                task_id: "t1".into(),
                sources: vec![ContextSource {
                    node: "entity-1".into(),
                    tokens: 500,
                }],
                excluded: vec![ExcludedItem {
                    node: "entity-2".into(),
                    reason: "over budget".into(),
                }],
                total_tokens: 500,
                budget_used_pct: 0.75,
                truncated: false,
            },
            // MCP (5)
            EventKind::McpInvoke {
                task_id: "t1".into(),
                call_id: "call-1".into(),
                mcp_server: "novanet".into(),
                tool: Some("novanet_search".into()),
                resource: None,
                params: Some(Arc::new(serde_json::json!({"query": "test"}))),
            },
            EventKind::McpResponse {
                task_id: "t1".into(),
                call_id: "call-1".into(),
                output_len: 256,
                duration_ms: 80,
                cached: false,
                is_error: false,
                response: Some(Arc::new(serde_json::json!({"found": 3}))),
            },
            EventKind::McpConnected {
                server_name: "novanet".into(),
            },
            EventKind::McpError {
                server_name: "novanet".into(),
                error: "connection refused".into(),
            },
            EventKind::McpRetry {
                task_id: "t1".into(),
                server_name: "novanet".into(),
                operation: "novanet_search".into(),
                attempt: 2,
                max_attempts: 3,
                error: "timeout".into(),
            },
            // Agent (4)
            EventKind::AgentStart {
                task_id: "agent1".into(),
                max_turns: 10,
                mcp_servers: vec!["novanet".into()],
            },
            EventKind::AgentTurn {
                task_id: "agent1".into(),
                turn_index: 1,
                kind: crate::types::AgentTurnKind::Started,
                metadata: Some(AgentTurnMetadata {
                    thinking: Some("Let me think...".into()),
                    response_text: "Here is my response".into(),
                    input_tokens: 100,
                    output_tokens: 50,
                    cache_read_tokens: 0,
                    stop_reason: crate::types::AgentStopReason::EndTurn,
                }),
            },
            EventKind::AgentComplete {
                task_id: "agent1".into(),
                turns: 3,
                stop_reason: crate::types::AgentStopReason::NaturalCompletion,
            },
            EventKind::AgentSpawned {
                parent_task_id: "agent1".into(),
                child_task_id: "sub-agent1".into(),
                depth: 1,
            },
            // Records (2)
            EventKind::RecordCreated {
                task_id: "t1".into(),
                summary_tokens: 150,
                confidence: 0.85,
                compression_cost_usd: 0.0002,
                compression_ratio: 0.03,
                model: "claude-haiku-4-5".to_string(),
            },
            EventKind::RecordSkipped {
                task_id: "t1".into(),
                reason: "compression failed".to_string(),
            },
            // Guardrails (3)
            EventKind::GuardrailPassed {
                task_id: "t1".into(),
                guardrail_type: crate::types::GuardrailType::Length,
                description: "max 1000 chars".into(),
            },
            EventKind::GuardrailFailed {
                task_id: "t1".into(),
                guardrail_type: crate::types::GuardrailType::Schema,
                description: "JSON schema".into(),
                message: "missing field 'title'".into(),
            },
            EventKind::GuardrailEscalation {
                task_id: "t1".into(),
                guardrail_type: crate::types::GuardrailType::Llm,
                guardrail_id: "safety-check".into(),
                message: "content flagged".into(),
                severity: crate::types::Severity::High,
                suggested_action: Some("human review".into()),
            },
            // Builtin (2)
            EventKind::Log {
                level: "info".into(),
                message: "step completed".into(),
                task_id: Some("t1".into()),
            },
            EventKind::Custom {
                name: "my_event".into(),
                payload: serde_json::json!({"key": "val"}),
                task_id: None,
            },
            // Artifact (2)
            EventKind::ArtifactWritten {
                task_id: "t1".into(),
                path: "/tmp/output.json".into(),
                size: 1024,
                format: "json".into(),
                checksum: None,
            },
            EventKind::ArtifactFailed {
                task_id: "t1".into(),
                path: "/tmp/output.json".into(),
                reason: "permission denied".into(),
            },
            // Media (4)
            EventKind::MediaExtracted {
                task_id: "gen_img".into(),
                block_count: 2,
                content_types: vec!["image".into(), "audio".into()],
            },
            EventKind::MediaProcessed {
                task_id: "gen_img".into(),
                hash: "blake3:a1b2c3d4".into(),
                mime_type: "image/png".into(),
                size_bytes: 4096,
            },
            EventKind::MediaStored {
                task_id: "gen_img".into(),
                hash: "blake3:a1b2c3d4".into(),
                path: ".nika/media/store/a1/b2c3d4".into(),
                size_bytes: 4096,
                verified: true,
                deduplicated: false,
                pipeline_ms: 42,
            },
            EventKind::MediaStoreFailed {
                task_id: "gen_img".into(),
                hash: "blake3:a1b2c3d4".into(),
                reason: "disk full".into(),
            },
            // Structured Output (2)
            EventKind::StructuredOutputAttempt {
                task_id: "t1".into(),
                layer: 1,
                layer_name: "rig_extractor".into(),
                attempt: 1,
                success: true,
                error: None,
            },
            EventKind::StructuredOutputSuccess {
                task_id: "t1".into(),
                layer: 1,
                layer_name: "rig_extractor".into(),
                total_attempts: 1,
            },
            // Media Cleanup (1)
            EventKind::MediaCleanup {
                removed: 5,
                bytes_freed: 10240,
                dry_run: false,
            },
            // Media Integrity Check (1)
            EventKind::MediaIntegrityCheck {
                checked: 10,
                warnings: 0,
            },
        ]
    }

    #[test]
    fn wave2_variant_count_is_40() {
        let variants = all_38_variants();
        assert_eq!(
            variants.len(),
            40,
            "EventKind should have exactly 40 variants (38 + RecordCreated + RecordSkipped)"
        );
    }

    #[test]
    fn wave2_all_variants_serialize_deserialize_roundtrip() {
        for (i, variant) in all_38_variants().into_iter().enumerate() {
            let json = serde_json::to_string(&variant)
                .unwrap_or_else(|e| panic!("variant {i} failed to serialize: {e}"));
            let back: EventKind = serde_json::from_str(&json)
                .unwrap_or_else(|e| panic!("variant {i} failed to deserialize: {e}\nJSON: {json}"));
            assert_eq!(variant, back, "variant {i} round-trip mismatch");
        }
    }

    #[test]
    fn wave2_ndjson_no_embedded_newlines() {
        // NDJSON = one JSON object per line, no embedded newlines
        for (i, variant) in all_38_variants().into_iter().enumerate() {
            let json = serde_json::to_string(&variant).unwrap();
            assert!(
                !json.contains('\n'),
                "variant {i} contains embedded newline in JSON: {json}"
            );
        }
    }

    #[test]
    fn wave2_full_event_envelope_ndjson_valid() {
        let log = EventLog::new();
        log.emit(EventKind::WorkflowStarted {
            task_count: 1,
            generation_id: "g1".into(),
            workflow_hash: "h1".into(),
            nika_version: env!("CARGO_PKG_VERSION").into(),
        });
        let events = log.events();
        let event = &events[0];
        let json = serde_json::to_string(event).unwrap();
        assert!(!json.contains('\n'));
        // Verify envelope fields
        let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert!(parsed.get("id").is_some());
        assert!(parsed.get("timestamp_ms").is_some());
        assert!(parsed.get("kind").is_some());
    }

    #[test]
    fn wave2_event_ids_monotonic_under_contention() {
        use std::thread;
        let log = EventLog::new();
        let threads: Vec<_> = (0..20)
            .map(|_| {
                let log = log.clone();
                thread::spawn(move || {
                    for _ in 0..50 {
                        log.emit(EventKind::WorkflowPaused);
                    }
                })
            })
            .collect();
        for t in threads {
            t.join().unwrap();
        }
        let events = log.events();
        assert_eq!(events.len(), 1000);
        // All IDs must be unique and form a contiguous range 0..1000
        let mut ids: Vec<u64> = events.iter().map(|e| e.id).collect();
        ids.sort();
        ids.dedup();
        assert_eq!(ids.len(), 1000, "IDs must be unique");
        assert_eq!(ids[0], 0);
        assert_eq!(ids[999], 999);
    }

    #[test]
    fn wave2_timestamps_monotonically_nondecreasing() {
        let log = EventLog::new();
        for _ in 0..100 {
            log.emit(EventKind::WorkflowPaused);
        }
        let events = log.events();
        for window in events.windows(2) {
            assert!(
                window[1].timestamp_ms >= window[0].timestamp_ms,
                "Timestamps must be monotonically non-decreasing"
            );
        }
    }

    #[test]
    fn wave2_timestamps_are_relative_not_epoch() {
        let log = EventLog::new();
        log.emit(EventKind::WorkflowPaused);
        let events = log.events();
        // Relative timestamps should be small (< 1 second)
        assert!(
            events[0].timestamp_ms < 1000,
            "First event timestamp {} should be < 1s (relative to start)",
            events[0].timestamp_ms
        );
    }

    #[test]
    fn wave2_broadcast_channel_lagged_on_overflow() {
        // EventLog broadcast capacity is 4096
        // new_with_broadcast returns (EventLog, Receiver)
        let (log, mut rx) = EventLog::new_with_broadcast();

        // Emit 4200 events (exceeds 4096 capacity)
        for _ in 0..4200 {
            log.emit(EventKind::WorkflowPaused);
        }

        // The receiver should experience lag (missed events)
        let mut lagged = false;
        loop {
            match rx.try_recv() {
                Ok(_) => {}
                Err(broadcast::error::TryRecvError::Lagged(_n)) => {
                    lagged = true;
                    // After lag, drain remaining
                    while rx.try_recv().is_ok() {}
                    break;
                }
                Err(broadcast::error::TryRecvError::Empty) => break,
                Err(broadcast::error::TryRecvError::Closed) => break,
            }
        }
        // Should have experienced lag because 4200 > 4096
        assert!(
            lagged,
            "Expected broadcast lag when emitting 4200 events into 4096 capacity channel"
        );
        // But all events should be in the log (log is unbounded)
        assert_eq!(log.events().len(), 4200);
    }

    // Dead variant tests: LimitReached and PartialCompletion are defined but never emitted in runtime code.
    // GuardrailEscalation IS emitted at runtime/rig_agent_loop/thinking.rs:99.
    #[test]
    fn wave2_guardrail_escalation_serialization() {
        // GuardrailEscalation IS emitted in runtime (thinking.rs:99 on guardrail escalation).
        // This test verifies serialization.
        let variant = EventKind::GuardrailEscalation {
            task_id: "t1".into(),
            guardrail_type: crate::types::GuardrailType::Llm,
            guardrail_id: "check-1".into(),
            message: "flagged".into(),
            severity: crate::types::Severity::High,
            suggested_action: None,
        };
        // It serializes fine -- it's just never emitted
        let json = serde_json::to_string(&variant).unwrap();
        assert!(json.contains("guardrail_escalation"));
    }

    #[test]
    fn wave2_optional_fields_serialized_as_null_when_none() {
        // ProviderResponded's request_id and ttft_ms are Option but WITHOUT
        // skip_serializing_if, so they serialize as null (not omitted).
        let variant = EventKind::ProviderResponded {
            task_id: "t1".into(),
            request_id: None,
            input_tokens: 100,
            output_tokens: 50,
            cache_read_tokens: 0,
            ttft_ms: None,
            finish_reason: crate::types::FinishReason::EndTurn,
            cost_usd: 0.0,
        };
        let json = serde_json::to_string(&variant).unwrap();
        // These are present as null (not omitted) since no skip_serializing_if
        assert!(
            json.contains("\"request_id\":null"),
            "None should serialize as null: {json}"
        );
        assert!(
            json.contains("\"ttft_ms\":null"),
            "None should serialize as null: {json}"
        );
    }

    #[test]
    fn wave2_skip_serializing_if_omits_none_fields() {
        // Fields WITH skip_serializing_if should be truly absent when None
        let variant = EventKind::GuardrailEscalation {
            task_id: "t1".into(),
            guardrail_type: crate::types::GuardrailType::Llm,
            guardrail_id: "check".into(),
            message: "flagged".into(),
            severity: crate::types::Severity::High,
            suggested_action: None, // has skip_serializing_if
        };
        let json = serde_json::to_string(&variant).unwrap();
        assert!(
            !json.contains("suggested_action"),
            "skip_serializing_if should omit None: {json}"
        );
    }

    #[test]
    fn wave2_optional_fields_present_when_some() {
        let variant = EventKind::ProviderResponded {
            task_id: "t1".into(),
            request_id: Some("req-1".into()),
            input_tokens: 100,
            output_tokens: 50,
            cache_read_tokens: 0,
            ttft_ms: Some(150),
            finish_reason: crate::types::FinishReason::EndTurn,
            cost_usd: 0.001,
        };
        let json = serde_json::to_string(&variant).unwrap();
        assert!(
            json.contains("\"request_id\":\"req-1\""),
            "Some fields should contain value: {json}"
        );
        assert!(
            json.contains("\"ttft_ms\":150"),
            "Some fields should contain value: {json}"
        );
    }

    #[test]
    fn wave2_task_id_extraction_all_variants() {
        let variants = all_38_variants();
        // Variants WITH task_id (27 of them)
        let with_task_id: Vec<_> = variants.iter().filter(|v| v.task_id().is_some()).collect();
        // Variants WITHOUT task_id (11): 6 workflow + McpConnected + McpError + Custom(None) + MediaCleanup + MediaIntegrityCheck
        let without_task_id: Vec<_> = variants.iter().filter(|v| v.task_id().is_none()).collect();

        // Custom has task_id: None in our test data, so it goes in without
        // Log has task_id: Some("t1"), so it goes in with
        assert_eq!(
            with_task_id.len(),
            29,
            "29 variants should have task_id (including Log with Some, 4 media events, 2 record events)"
        );
        assert_eq!(
            without_task_id.len(),
            11,
            "11 variants should lack task_id (workflow-level + McpConnected + McpError + Custom with None + MediaCleanup + MediaIntegrityCheck)"
        );
    }

    #[test]
    fn wave2_workflow_completed_wraps_json_as_string() {
        // WorkflowCompleted.final_output is Arc<Value>.
        // When the output IS a JSON object, it gets double-serialized
        // if the runner wraps it as a string first.
        let inner = serde_json::json!({"key": "value"});
        let variant = EventKind::WorkflowCompleted {
            final_output: Arc::new(inner.clone()),
            total_duration_ms: 100,
        };
        let json = serde_json::to_string(&variant).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();
        let output = &parsed["final_output"];
        // The output should be a proper JSON object, not a string containing JSON
        assert!(
            output.is_object(),
            "final_output should be a JSON object, not a string: {output}"
        );
    }

    #[test]
    fn wave2_cloned_eventlog_shares_events() {
        let log1 = EventLog::new();
        let log2 = log1.clone();
        log1.emit(EventKind::WorkflowPaused);
        log2.emit(EventKind::WorkflowResumed);
        // Both clones should see 2 events (shared Arc)
        assert_eq!(log1.events().len(), 2);
        assert_eq!(log2.events().len(), 2);
    }

    #[test]
    fn wave2_serde_tag_is_snake_case() {
        // Verify serde(rename_all = "snake_case") works correctly
        let variant = EventKind::WorkflowStarted {
            task_count: 1,
            generation_id: "g".into(),
            workflow_hash: "h".into(),
            nika_version: "v".into(),
        };
        let json = serde_json::to_string(&variant).unwrap();
        assert!(
            json.contains("\"type\":\"workflow_started\""),
            "Tag should be snake_case: {json}"
        );

        let variant2 = EventKind::McpRetry {
            task_id: "t".into(),
            server_name: "s".into(),
            operation: "op".into(),
            attempt: 1,
            max_attempts: 3,
            error: "e".into(),
        };
        let json2 = serde_json::to_string(&variant2).unwrap();
        assert!(
            json2.contains("\"type\":\"mcp_retry\""),
            "Tag should be snake_case: {json2}"
        );
    }

    #[test]
    fn wave2_is_workflow_event_correct() {
        let workflow_events = vec![
            EventKind::WorkflowStarted {
                task_count: 1,
                generation_id: "g".into(),
                workflow_hash: "h".into(),
                nika_version: "v".into(),
            },
            EventKind::WorkflowCompleted {
                final_output: Arc::new(serde_json::json!(null)),
                total_duration_ms: 0,
            },
            EventKind::WorkflowFailed {
                error: "e".into(),
                failed_task: None,
            },
            EventKind::WorkflowAborted {
                reason: "r".into(),
                duration_ms: 0,
                running_tasks: vec![],
            },
            EventKind::WorkflowPaused,
            EventKind::WorkflowResumed,
        ];
        for wf in &workflow_events {
            assert!(wf.is_workflow_event(), "{:?} should be workflow event", wf);
        }
        // Non-workflow events should return false
        let non_wf = EventKind::TaskStarted {
            task_id: "t".into(),
            verb: "infer".into(),
            inputs: Arc::new(serde_json::json!({})),
        };
        assert!(!non_wf.is_workflow_event());
    }

    // ═══════════════════════════════════════════════════════════════
    // Media event emission pipeline tests
    // ═══════════════════════════════════════════════════════════════

    // ----- Helpers for media tests -----

    /// Realistic blake3 hash (64 hex chars after prefix)
    const BLAKE3_PNG: &str =
        "blake3:a7ffc6f8bf1ed76651c14756a061d662f580ff4de43b49fa82d80a4b80f8434a";
    const BLAKE3_WAV: &str =
        "blake3:e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855";
    const BLAKE3_PDF: &str =
        "blake3:6b86b273ff34fce19d6b804eff5a3f5747ada4eaa22f1d49c01e52ddb7875b4b";

    fn media_extracted(task_id: &str, block_count: u32, types: &[&str]) -> EventKind {
        EventKind::MediaExtracted {
            task_id: Arc::from(task_id),
            block_count,
            content_types: types.iter().map(|s| s.to_string()).collect(),
        }
    }

    fn media_processed(task_id: &str, hash: &str, mime: &str, size: u64) -> EventKind {
        EventKind::MediaProcessed {
            task_id: Arc::from(task_id),
            hash: hash.to_string(),
            mime_type: mime.to_string(),
            size_bytes: size,
        }
    }

    fn media_stored(
        task_id: &str,
        hash: &str,
        path: &str,
        size: u64,
        verified: bool,
        deduplicated: bool,
        pipeline_ms: u64,
    ) -> EventKind {
        EventKind::MediaStored {
            task_id: Arc::from(task_id),
            hash: hash.to_string(),
            path: path.to_string(),
            size_bytes: size,
            verified,
            deduplicated,
            pipeline_ms,
        }
    }

    fn media_store_failed(task_id: &str, hash: &str, reason: &str) -> EventKind {
        EventKind::MediaStoreFailed {
            task_id: Arc::from(task_id),
            hash: hash.to_string(),
            reason: reason.to_string(),
        }
    }

    // ═══════════════════════════════════════════════════════════════
    // 1. Serde roundtrip for ALL 4 media variants
    // ═══════════════════════════════════════════════════════════════

    #[test]
    fn media_extracted_serde_roundtrip_single_type() {
        let event = media_extracted("gen_logo", 1, &["image"]);
        let json = serde_json::to_string(&event).unwrap();
        let back: EventKind = serde_json::from_str(&json).unwrap();
        assert_eq!(event, back);
    }

    #[test]
    fn media_extracted_serde_roundtrip_multiple_types() {
        let event = media_extracted("gen_multi", 4, &["image", "audio", "video", "application"]);
        let json = serde_json::to_string(&event).unwrap();
        let back: EventKind = serde_json::from_str(&json).unwrap();
        assert_eq!(event, back);

        // Verify field values survived roundtrip
        if let EventKind::MediaExtracted {
            block_count,
            content_types,
            ..
        } = &back
        {
            assert_eq!(*block_count, 4);
            assert_eq!(content_types.len(), 4);
            assert_eq!(content_types[0], "image");
            assert_eq!(content_types[3], "application");
        } else {
            panic!("Expected MediaExtracted");
        }
    }

    #[test]
    fn media_extracted_serde_roundtrip_empty_types() {
        // Edge case: block_count=0 with no content_types
        let event = media_extracted("empty_task", 0, &[]);
        let json = serde_json::to_string(&event).unwrap();
        let back: EventKind = serde_json::from_str(&json).unwrap();
        assert_eq!(event, back);
    }

    #[test]
    fn media_processed_serde_roundtrip_png() {
        let event = media_processed("gen_img", BLAKE3_PNG, "image/png", 65536);
        let json = serde_json::to_string(&event).unwrap();
        let back: EventKind = serde_json::from_str(&json).unwrap();
        assert_eq!(event, back);

        if let EventKind::MediaProcessed {
            hash,
            mime_type,
            size_bytes,
            ..
        } = &back
        {
            assert!(hash.starts_with("blake3:"));
            assert_eq!(hash.len(), "blake3:".len() + 64); // blake3 = 64 hex chars
            assert_eq!(mime_type, "image/png");
            assert_eq!(*size_bytes, 65536);
        } else {
            panic!("Expected MediaProcessed");
        }
    }

    #[test]
    fn media_processed_serde_roundtrip_wav() {
        let event = media_processed("gen_audio", BLAKE3_WAV, "audio/wav", 1_048_576);
        let json = serde_json::to_string(&event).unwrap();
        let back: EventKind = serde_json::from_str(&json).unwrap();
        assert_eq!(event, back);
    }

    #[test]
    fn media_processed_serde_roundtrip_pdf() {
        let event = media_processed("gen_doc", BLAKE3_PDF, "application/pdf", 204800);
        let json = serde_json::to_string(&event).unwrap();
        let back: EventKind = serde_json::from_str(&json).unwrap();
        assert_eq!(event, back);
    }

    #[test]
    fn media_stored_serde_roundtrip_all_fields() {
        let event = media_stored(
            "gen_img",
            BLAKE3_PNG,
            ".nika/media/store/a7/ffc6f8bf1ed76651c14756a061d662f580ff4de43b49fa82d80a4b80f8434a",
            65536,
            true,
            false,
            42,
        );
        let json = serde_json::to_string(&event).unwrap();
        let back: EventKind = serde_json::from_str(&json).unwrap();
        assert_eq!(event, back);

        if let EventKind::MediaStored {
            hash,
            path,
            size_bytes,
            verified,
            deduplicated,
            pipeline_ms,
            ..
        } = &back
        {
            assert_eq!(hash, BLAKE3_PNG);
            assert!(path.starts_with(".nika/media/store/"));
            assert_eq!(*size_bytes, 65536);
            assert!(*verified);
            assert!(!*deduplicated);
            assert_eq!(*pipeline_ms, 42);
        } else {
            panic!("Expected MediaStored");
        }
    }

    #[test]
    fn media_stored_serde_roundtrip_deduplicated() {
        let event = media_stored(
            "gen_img_dup",
            BLAKE3_PNG,
            ".nika/media/store/a7/ffc6f8bf1ed76651c14756a061d662f580ff4de43b49fa82d80a4b80f8434a",
            65536,
            false, // verified=false for dedup hits
            true,  // deduplicated=true
            1,     // fast path
        );
        let json = serde_json::to_string(&event).unwrap();
        let back: EventKind = serde_json::from_str(&json).unwrap();
        assert_eq!(event, back);
    }

    #[test]
    fn media_store_failed_serde_roundtrip_with_hash() {
        let event = media_store_failed("gen_img", BLAKE3_PNG, "disk full");
        let json = serde_json::to_string(&event).unwrap();
        let back: EventKind = serde_json::from_str(&json).unwrap();
        assert_eq!(event, back);

        if let EventKind::MediaStoreFailed { hash, reason, .. } = &back {
            assert_eq!(hash, BLAKE3_PNG);
            assert_eq!(reason, "disk full");
        } else {
            panic!("Expected MediaStoreFailed");
        }
    }

    #[test]
    fn media_store_failed_serde_roundtrip_empty_hash() {
        // Pre-hash failure: hash is empty string
        let event = media_store_failed("gen_img", "", "base64 decode failed");
        let json = serde_json::to_string(&event).unwrap();
        let back: EventKind = serde_json::from_str(&json).unwrap();
        assert_eq!(event, back);

        if let EventKind::MediaStoreFailed { hash, reason, .. } = &back {
            assert!(hash.is_empty(), "Pre-hash failure should have empty hash");
            assert_eq!(reason, "base64 decode failed");
        } else {
            panic!("Expected MediaStoreFailed");
        }
    }

    // ═══════════════════════════════════════════════════════════════
    // 2. Media event task_id extraction
    // ═══════════════════════════════════════════════════════════════

    #[test]
    fn media_extracted_returns_task_id() {
        let event = media_extracted("extract_images", 3, &["image", "audio", "video"]);
        assert_eq!(event.task_id(), Some("extract_images"));
    }

    #[test]
    fn media_processed_returns_task_id() {
        let event = media_processed("process_png", BLAKE3_PNG, "image/png", 1024);
        assert_eq!(event.task_id(), Some("process_png"));
    }

    #[test]
    fn media_stored_returns_task_id() {
        let event = media_stored(
            "store_to_cas",
            BLAKE3_PNG,
            ".nika/media/store/a7/ffc6",
            1024,
            true,
            false,
            10,
        );
        assert_eq!(event.task_id(), Some("store_to_cas"));
    }

    #[test]
    fn media_store_failed_returns_task_id() {
        let event = media_store_failed("fail_store", BLAKE3_PNG, "permission denied");
        assert_eq!(event.task_id(), Some("fail_store"));
    }

    #[test]
    fn all_4_media_variants_have_task_id() {
        let variants: Vec<(&str, EventKind)> = vec![
            (
                "extracted",
                media_extracted("t_extract", 2, &["image", "audio"]),
            ),
            (
                "processed",
                media_processed("t_process", BLAKE3_WAV, "audio/wav", 2048),
            ),
            (
                "stored",
                media_stored(
                    "t_store",
                    BLAKE3_PDF,
                    ".nika/media/store/6b/86b2",
                    4096,
                    true,
                    false,
                    5,
                ),
            ),
            (
                "failed",
                media_store_failed("t_fail", "", "budget exceeded"),
            ),
        ];

        for (name, event) in &variants {
            assert!(
                event.task_id().is_some(),
                "Media{name} must return Some task_id"
            );
        }
    }

    // ═══════════════════════════════════════════════════════════════
    // 3. Media events NDJSON compliance
    // ═══════════════════════════════════════════════════════════════

    #[test]
    fn media_extracted_ndjson_no_newlines() {
        let event = media_extracted("ndjson_test", 5, &["image", "audio", "video"]);
        let json = serde_json::to_string(&event).unwrap();
        assert!(
            !json.contains('\n'),
            "MediaExtracted JSON must not contain newlines: {json}"
        );
    }

    #[test]
    fn media_processed_ndjson_no_newlines() {
        let event = media_processed("ndjson_test", BLAKE3_PNG, "image/png", 999999);
        let json = serde_json::to_string(&event).unwrap();
        assert!(
            !json.contains('\n'),
            "MediaProcessed JSON must not contain newlines: {json}"
        );
    }

    #[test]
    fn media_stored_ndjson_no_newlines() {
        let event = media_stored(
            "ndjson_test",
            BLAKE3_WAV,
            ".nika/media/store/e3/b0c44298fc1c149afbf4c8996fb924",
            512000,
            true,
            true,
            100,
        );
        let json = serde_json::to_string(&event).unwrap();
        assert!(
            !json.contains('\n'),
            "MediaStored JSON must not contain newlines: {json}"
        );
    }

    #[test]
    fn media_store_failed_ndjson_no_newlines() {
        let event = media_store_failed("ndjson_test", "", "write error: No space left on device");
        let json = serde_json::to_string(&event).unwrap();
        assert!(
            !json.contains('\n'),
            "MediaStoreFailed JSON must not contain newlines: {json}"
        );
    }

    #[test]
    fn all_4_media_variants_ndjson_roundtrip() {
        // Full NDJSON contract: serialize to single line, deserialize back
        let variants = vec![
            media_extracted("rt_task", 2, &["image", "audio"]),
            media_processed("rt_task", BLAKE3_PNG, "image/png", 8192),
            media_stored(
                "rt_task",
                BLAKE3_PNG,
                ".nika/media/store/a7/ffc6f8bf1ed76651",
                8192,
                true,
                false,
                25,
            ),
            media_store_failed("rt_task", BLAKE3_PNG, "verification checksum mismatch"),
        ];

        for (i, variant) in variants.into_iter().enumerate() {
            let json = serde_json::to_string(&variant).unwrap();
            assert!(
                !json.contains('\n'),
                "Media variant {i} has embedded newline"
            );
            let back: EventKind = serde_json::from_str(&json).unwrap_or_else(|e| {
                panic!("Media variant {i} failed to deserialize: {e}\nJSON: {json}")
            });
            assert_eq!(variant, back, "Media variant {i} roundtrip mismatch");
        }
    }

    #[test]
    fn media_events_ndjson_full_envelope() {
        // Verify the full Event envelope (id + timestamp_ms + kind) serializes to single-line
        let log = EventLog::new();
        log.emit(media_extracted("envelope_test", 1, &["image"]));
        log.emit(media_processed(
            "envelope_test",
            BLAKE3_PNG,
            "image/png",
            4096,
        ));
        log.emit(media_stored(
            "envelope_test",
            BLAKE3_PNG,
            ".nika/media/store/a7/ffc6",
            4096,
            true,
            false,
            15,
        ));
        log.emit(media_store_failed("envelope_test", "", "boom"));

        for event in log.events() {
            let json = serde_json::to_string(&event).unwrap();
            assert!(
                !json.contains('\n'),
                "Full Event envelope must be single-line NDJSON: {json}"
            );
            // Verify envelope structure
            let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();
            assert!(parsed.get("id").is_some(), "Missing 'id' in envelope");
            assert!(
                parsed.get("timestamp_ms").is_some(),
                "Missing 'timestamp_ms' in envelope"
            );
            assert!(parsed.get("kind").is_some(), "Missing 'kind' in envelope");
        }
    }

    // ═══════════════════════════════════════════════════════════════
    // 4. Media events in EventLog
    // ═══════════════════════════════════════════════════════════════

    #[test]
    fn media_extracted_appears_in_eventlog_events() {
        let log = EventLog::new();
        let id = log.emit(media_extracted(
            "media_task",
            3,
            &["image", "audio", "video"],
        ));

        let events = log.events();
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].id, id);

        if let EventKind::MediaExtracted {
            task_id,
            block_count,
            content_types,
        } = &events[0].kind
        {
            assert_eq!(task_id.as_ref(), "media_task");
            assert_eq!(*block_count, 3);
            assert_eq!(content_types, &["image", "audio", "video"]);
        } else {
            panic!("Expected MediaExtracted in events()");
        }
    }

    #[test]
    fn media_stored_broadcast_reaches_subscriber() {
        let (log, mut rx) = EventLog::new_with_broadcast();

        log.emit(media_stored(
            "broadcast_test",
            BLAKE3_PNG,
            ".nika/media/store/a7/ffc6",
            4096,
            true,
            false,
            10,
        ));

        // Subscriber should receive the event
        let received = rx
            .try_recv()
            .expect("Subscriber should receive MediaStored event");
        assert_eq!(received.id, 0);

        if let EventKind::MediaStored {
            task_id,
            hash,
            verified,
            ..
        } = &received.kind
        {
            assert_eq!(task_id.as_ref(), "broadcast_test");
            assert_eq!(hash, BLAKE3_PNG);
            assert!(*verified);
        } else {
            panic!("Expected MediaStored via broadcast");
        }
    }

    #[test]
    fn media_events_broadcast_multiple_subscribers() {
        let (log, mut rx1) = EventLog::new_with_broadcast();
        let mut rx2 = log.subscribe().expect("Should be able to subscribe");

        log.emit(media_processed("multi_sub", BLAKE3_WAV, "audio/wav", 2048));

        let e1 = rx1.try_recv().expect("rx1 should receive");
        let e2 = rx2.try_recv().expect("rx2 should receive");
        assert_eq!(e1.id, e2.id);
        assert_eq!(e1.kind, e2.kind);
    }

    #[test]
    fn filter_task_returns_media_events() {
        let log = EventLog::new();

        // Mix of media and non-media events for the same task
        log.emit(EventKind::TaskStarted {
            task_id: "gen_image".into(),
            verb: "invoke".into(),
            inputs: Arc::new(json!({})),
        });
        log.emit(media_extracted("gen_image", 1, &["image"]));
        log.emit(media_processed("gen_image", BLAKE3_PNG, "image/png", 4096));
        log.emit(media_stored(
            "gen_image",
            BLAKE3_PNG,
            ".nika/media/store/a7/ffc6",
            4096,
            true,
            false,
            20,
        ));

        // Different task
        log.emit(media_extracted("other_task", 2, &["audio", "video"]));

        let gen_events = log.filter_task("gen_image");
        assert_eq!(
            gen_events.len(),
            4,
            "gen_image should have 4 events (1 task + 3 media)"
        );

        // Verify media events are in order
        assert!(matches!(&gen_events[0].kind, EventKind::TaskStarted { .. }));
        assert!(matches!(
            &gen_events[1].kind,
            EventKind::MediaExtracted { .. }
        ));
        assert!(matches!(
            &gen_events[2].kind,
            EventKind::MediaProcessed { .. }
        ));
        assert!(matches!(&gen_events[3].kind, EventKind::MediaStored { .. }));

        let other_events = log.filter_task("other_task");
        assert_eq!(other_events.len(), 1, "other_task should only have 1 event");
    }

    #[test]
    fn filter_task_with_media_failure() {
        let log = EventLog::new();

        log.emit(media_extracted("fail_task", 1, &["image"]));
        log.emit(media_processed("fail_task", BLAKE3_PNG, "image/png", 4096));
        log.emit(media_store_failed("fail_task", BLAKE3_PNG, "disk full"));

        let events = log.filter_task("fail_task");
        assert_eq!(events.len(), 3);
        assert!(matches!(
            &events[2].kind,
            EventKind::MediaStoreFailed { .. }
        ));
    }

    #[test]
    fn count_task_includes_media_events() {
        let log = EventLog::new();

        log.emit(media_extracted("count_me", 2, &["image", "audio"]));
        log.emit(media_processed("count_me", BLAKE3_PNG, "image/png", 4096));
        log.emit(media_processed("count_me", BLAKE3_WAV, "audio/wav", 8192));
        log.emit(media_stored(
            "count_me",
            BLAKE3_PNG,
            ".nika/media/store/a7/ffc6",
            4096,
            true,
            false,
            15,
        ));
        log.emit(media_stored(
            "count_me",
            BLAKE3_WAV,
            ".nika/media/store/e3/b0c4",
            8192,
            true,
            false,
            22,
        ));

        assert_eq!(log.count_task("count_me"), 5);
        assert_eq!(log.count_task("no_such_task"), 0);
    }

    #[test]
    fn media_events_not_workflow_events() {
        let log = EventLog::new();

        log.emit(workflow_started(1));
        log.emit(media_extracted("t1", 1, &["image"]));
        log.emit(media_processed("t1", BLAKE3_PNG, "image/png", 4096));
        log.emit(media_stored(
            "t1",
            BLAKE3_PNG,
            ".nika/media/store/a7/ffc6",
            4096,
            true,
            false,
            10,
        ));
        log.emit(media_store_failed("t1", "", "boom"));

        let wf_events = log.workflow_events();
        assert_eq!(
            wf_events.len(),
            1,
            "Media events must NOT appear in workflow_events()"
        );
    }

    // ═══════════════════════════════════════════════════════════════
    // 5. MediaStored field verification
    // ═══════════════════════════════════════════════════════════════

    #[test]
    fn media_stored_pipeline_ms_reasonable_values() {
        // Normal processing: sub-second
        let event = media_stored(
            "fast_store",
            BLAKE3_PNG,
            ".nika/media/store/a7/ffc6",
            4096,
            true,
            false,
            42,
        );
        if let EventKind::MediaStored { pipeline_ms, .. } = &event {
            assert!(
                *pipeline_ms < 10000,
                "pipeline_ms={pipeline_ms} should be < 10000ms"
            );
        }

        // Zero is valid (dedup fast path)
        let event_zero = media_stored(
            "dedup_store",
            BLAKE3_PNG,
            ".nika/media/store/a7/ffc6",
            4096,
            false,
            true,
            0,
        );
        if let EventKind::MediaStored { pipeline_ms, .. } = &event_zero {
            assert_eq!(*pipeline_ms, 0, "Dedup fast path can have 0ms pipeline");
        }

        // Edge: just under threshold
        let event_edge = media_stored(
            "slow_store",
            BLAKE3_PNG,
            ".nika/media/store/a7/ffc6",
            4096,
            true,
            false,
            9999,
        );
        if let EventKind::MediaStored { pipeline_ms, .. } = &event_edge {
            assert!(*pipeline_ms < 10000);
        }
    }

    #[test]
    fn media_stored_verified_and_deduplicated_independent() {
        // All four combinations of (verified, deduplicated) are valid
        let combos: Vec<(bool, bool, &str)> = vec![
            (true, false, "fresh write, verified"),
            (false, false, "fresh write, unverified (small file)"),
            (false, true, "dedup hit, not re-verified"),
            (true, true, "dedup hit, re-verified"),
        ];

        for (verified, deduplicated, desc) in combos {
            let event = media_stored(
                "combo_test",
                BLAKE3_PNG,
                ".nika/media/store/a7/ffc6",
                4096,
                verified,
                deduplicated,
                10,
            );
            if let EventKind::MediaStored {
                verified: v,
                deduplicated: d,
                ..
            } = &event
            {
                assert_eq!(*v, verified, "verified mismatch for: {desc}");
                assert_eq!(*d, deduplicated, "deduplicated mismatch for: {desc}");
            }

            // Roundtrip preserves both booleans
            let json = serde_json::to_string(&event).unwrap();
            let back: EventKind = serde_json::from_str(&json).unwrap();
            assert_eq!(event, back, "Roundtrip failed for: {desc}");
        }
    }

    #[test]
    fn media_stored_path_cas_format() {
        // CAS paths follow the pattern: .nika/media/store/{first2}/{rest}
        let cas_paths = vec![
            ".nika/media/store/a7/ffc6f8bf1ed76651c14756a061d662f580ff4de43b49fa82d80a4b80f8434a",
            ".nika/media/store/e3/b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855",
            ".nika/media/store/6b/86b273ff34fce19d6b804eff5a3f5747ada4eaa22f1d49c01e52ddb7875b4b",
        ];

        for path in cas_paths {
            let event = media_stored("path_test", BLAKE3_PNG, path, 4096, true, false, 10);
            if let EventKind::MediaStored { path: p, .. } = &event {
                assert!(
                    p.starts_with(".nika/media/store/"),
                    "CAS path must start with .nika/media/store/: {p}"
                );
                // Path after prefix should be {2chars}/{rest}
                let suffix = p.strip_prefix(".nika/media/store/").unwrap();
                let parts: Vec<&str> = suffix.splitn(2, '/').collect();
                assert_eq!(parts.len(), 2, "CAS path suffix must be dir/file: {suffix}");
                assert_eq!(
                    parts[0].len(),
                    2,
                    "CAS directory prefix must be 2 chars: {}",
                    parts[0]
                );
                assert!(!parts[1].is_empty(), "CAS filename must not be empty");
            }
        }
    }

    // ═══════════════════════════════════════════════════════════════
    // Bonus: Media event serde tag verification
    // ═══════════════════════════════════════════════════════════════

    #[test]
    fn media_events_serde_tags_are_snake_case() {
        let variants: Vec<(&str, EventKind)> = vec![
            ("media_extracted", media_extracted("t", 1, &["image"])),
            (
                "media_processed",
                media_processed("t", BLAKE3_PNG, "image/png", 100),
            ),
            (
                "media_stored",
                media_stored(
                    "t",
                    BLAKE3_PNG,
                    ".nika/media/store/a7/ffc6",
                    100,
                    true,
                    false,
                    5,
                ),
            ),
            ("media_store_failed", media_store_failed("t", "", "err")),
        ];

        for (expected_tag, event) in variants {
            let json = serde_json::to_string(&event).unwrap();
            let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();
            assert_eq!(
                parsed["type"].as_str().unwrap(),
                expected_tag,
                "Serde tag mismatch for {expected_tag}"
            );
        }
    }

    #[test]
    fn media_events_deserialize_from_json_objects() {
        // Verify we can construct media events from raw JSON (as a consumer would)
        let json_extracted = json!({
            "type": "media_extracted",
            "task_id": "from_json",
            "block_count": 2,
            "content_types": ["image", "audio"]
        });
        let extracted: EventKind = serde_json::from_value(json_extracted).unwrap();
        assert_eq!(extracted.task_id(), Some("from_json"));
        if let EventKind::MediaExtracted {
            block_count,
            content_types,
            ..
        } = &extracted
        {
            assert_eq!(*block_count, 2);
            assert_eq!(content_types, &["image", "audio"]);
        } else {
            panic!("Expected MediaExtracted from JSON");
        }

        let json_stored = json!({
            "type": "media_stored",
            "task_id": "from_json",
            "hash": BLAKE3_PNG,
            "path": ".nika/media/store/a7/ffc6",
            "size_bytes": 8192,
            "verified": true,
            "deduplicated": false,
            "pipeline_ms": 33
        });
        let stored: EventKind = serde_json::from_value(json_stored).unwrap();
        if let EventKind::MediaStored {
            pipeline_ms,
            verified,
            deduplicated,
            ..
        } = &stored
        {
            assert_eq!(*pipeline_ms, 33);
            assert!(*verified);
            assert!(!*deduplicated);
        } else {
            panic!("Expected MediaStored from JSON");
        }
    }

    #[test]
    fn media_full_pipeline_lifecycle_in_eventlog() {
        // Simulates the complete media pipeline lifecycle for a single image
        let log = EventLog::new();
        let task = "generate_screenshot";

        // Step 1: MCP tool returns content blocks with images
        log.emit(EventKind::TaskStarted {
            task_id: task.into(),
            verb: "invoke".into(),
            inputs: Arc::new(json!({"tool": "screenshot"})),
        });

        // Step 2: Media blocks extracted
        log.emit(media_extracted(task, 2, &["image", "image"]));

        // Step 3: Each block processed
        log.emit(media_processed(task, BLAKE3_PNG, "image/png", 65536));
        log.emit(media_processed(task, BLAKE3_WAV, "image/jpeg", 32768));

        // Step 4: Both stored (one fresh, one dedup)
        log.emit(media_stored(
            task,
            BLAKE3_PNG,
            ".nika/media/store/a7/ffc6f8bf1ed76651",
            65536,
            true,
            false,
            35,
        ));
        log.emit(media_stored(
            task,
            BLAKE3_WAV,
            ".nika/media/store/e3/b0c44298fc1c1",
            32768,
            false,
            true,
            2,
        ));

        // Step 5: Task completes
        log.emit(EventKind::TaskCompleted {
            task_id: task.into(),
            output: Arc::new(json!({"images": 2})),
            duration_ms: 500,
        });

        let events = log.filter_task(task);
        assert_eq!(
            events.len(),
            7,
            "Full lifecycle: 1 started + 1 extracted + 2 processed + 2 stored + 1 completed"
        );

        // Verify ordering
        assert!(matches!(&events[0].kind, EventKind::TaskStarted { .. }));
        assert!(matches!(&events[1].kind, EventKind::MediaExtracted { .. }));
        assert!(matches!(&events[2].kind, EventKind::MediaProcessed { .. }));
        assert!(matches!(&events[3].kind, EventKind::MediaProcessed { .. }));
        assert!(matches!(&events[4].kind, EventKind::MediaStored { .. }));
        assert!(matches!(&events[5].kind, EventKind::MediaStored { .. }));
        assert!(matches!(&events[6].kind, EventKind::TaskCompleted { .. }));

        // Verify the dedup store was the second one
        if let EventKind::MediaStored {
            deduplicated,
            pipeline_ms,
            ..
        } = &events[5].kind
        {
            assert!(*deduplicated, "Second store should be dedup hit");
            assert!(*pipeline_ms < 10, "Dedup fast path should be < 10ms");
        }
    }

    #[test]
    fn vision_content_resolved_serde_round_trip() {
        let event = EventKind::VisionContentResolved {
            task_id: Arc::from("describe_image"),
            image_count: 3,
            total_bytes: 1_048_576,
            resolve_ms: 42,
        };
        let json = serde_json::to_string(&event).unwrap();
        assert!(json.contains("vision_content_resolved"));
        assert!(json.contains("describe_image"));
        assert!(json.contains("1048576"));
        let parsed: EventKind = serde_json::from_str(&json).unwrap();
        assert_eq!(event, parsed);
    }

    #[test]
    fn vision_content_resolved_has_task_id() {
        let event = EventKind::VisionContentResolved {
            task_id: Arc::from("my_task"),
            image_count: 1,
            total_bytes: 512,
            resolve_ms: 5,
        };
        assert_eq!(event.task_id(), Some("my_task"));
    }

    // ═══════════════════════════════════════════════════════════════
    // HTTP Telemetry Events
    // ═══════════════════════════════════════════════════════════════

    #[test]
    fn http_request_serde_round_trip() {
        let event = EventKind::HttpRequest {
            task_id: Arc::from("fetch_task"),
            method: "POST".to_string(),
            url: "https://api.example.com/data".to_string(),
            has_body: true,
        };
        let json = serde_json::to_string(&event).unwrap();
        assert!(json.contains("http_request"));
        assert!(json.contains("POST"));
        assert!(json.contains("api.example.com"));
        let parsed: EventKind = serde_json::from_str(&json).unwrap();
        assert_eq!(event, parsed);
    }

    #[test]
    fn http_request_has_task_id() {
        let event = EventKind::HttpRequest {
            task_id: Arc::from("t1"),
            method: "GET".to_string(),
            url: "https://example.com".to_string(),
            has_body: false,
        };
        assert_eq!(event.task_id(), Some("t1"));
    }

    #[test]
    fn http_response_serde_round_trip() {
        let event = EventKind::HttpResponse {
            task_id: Arc::from("fetch_task"),
            status_code: 200,
            content_type: Some("application/json".to_string()),
            content_length: Some(1234),
            elapsed_ms: 150,
        };
        let json = serde_json::to_string(&event).unwrap();
        assert!(json.contains("http_response"));
        assert!(json.contains("200"));
        assert!(json.contains("application/json"));
        let parsed: EventKind = serde_json::from_str(&json).unwrap();
        assert_eq!(event, parsed);
    }

    #[test]
    fn http_response_without_content_type() {
        let event = EventKind::HttpResponse {
            task_id: Arc::from("t2"),
            status_code: 404,
            content_type: None,
            content_length: None,
            elapsed_ms: 50,
        };
        let json = serde_json::to_string(&event).unwrap();
        let parsed: EventKind = serde_json::from_str(&json).unwrap();
        assert_eq!(event, parsed);
    }

    #[test]
    fn http_response_has_task_id() {
        let event = EventKind::HttpResponse {
            task_id: Arc::from("t3"),
            status_code: 500,
            content_type: None,
            content_length: None,
            elapsed_ms: 0,
        };
        assert_eq!(event.task_id(), Some("t3"));
    }

    #[test]
    fn exec_completed_serializes() {
        let event = EventKind::ExecCompleted {
            task_id: "exec_task".into(),
            exit_code: 0,
            stdout_len: 1024,
            stderr_len: 0,
            duration_ms: 150,
        };
        let json = serde_json::to_string(&event).unwrap();
        assert!(json.contains("exec_completed"));
        assert!(json.contains("\"exit_code\":0"));
        let round: EventKind = serde_json::from_str(&json).unwrap();
        assert_eq!(round.task_id(), Some("exec_task"));
    }

    #[test]
    fn fetch_retry_serializes() {
        let event = EventKind::FetchRetry {
            task_id: "fetch_task".into(),
            url: "https://api.example.com/data".to_string(),
            attempt: 2,
            max_attempts: 3,
            status_code: Some(503),
            backoff_ms: 2000,
        };
        let json = serde_json::to_string(&event).unwrap();
        assert!(json.contains("fetch_retry"));
        assert!(json.contains("\"attempt\":2"));
        let round: EventKind = serde_json::from_str(&json).unwrap();
        assert_eq!(round.task_id(), Some("fetch_task"));
    }

    #[test]
    fn policy_blocked_serializes() {
        let event = EventKind::PolicyBlocked {
            task_id: "exec_task".into(),
            verb: "exec".to_string(),
            policy_type: "command_blocklist".to_string(),
            reason: "Command 'sudo' is blocked".to_string(),
        };
        let json = serde_json::to_string(&event).unwrap();
        assert!(json.contains("policy_blocked"));
        assert!(json.contains("command_blocklist"));
        let round: EventKind = serde_json::from_str(&json).unwrap();
        assert_eq!(round.task_id(), Some("exec_task"));
    }

    #[test]
    fn boot_phase_completed_serializes() {
        let event = EventKind::BootPhaseCompleted {
            phase: "mcp_startup".to_string(),
            success: true,
            duration_ms: 1234,
            warnings: vec!["daemon not running".to_string()],
        };
        let json = serde_json::to_string(&event).unwrap();
        assert!(json.contains("boot_phase_completed"));
        assert!(json.contains("mcp_startup"));
        let round: EventKind = serde_json::from_str(&json).unwrap();
        assert_eq!(round.task_id(), None);
    }

    #[test]
    fn native_model_loaded_serializes() {
        let event = EventKind::NativeModelLoaded {
            model: "mistral-7b-instruct.gguf".to_string(),
            kind: "gguf".to_string(),
            size_bytes: 4_000_000_000,
            duration_ms: 3500,
            is_vision: false,
        };
        let json = serde_json::to_string(&event).unwrap();
        assert!(json.contains("native_model_loaded"));
        let round: EventKind = serde_json::from_str(&json).unwrap();
        assert_eq!(round.task_id(), None);
    }

    #[test]
    fn test_budget_ok_event() {
        let event = EventKind::BudgetOk {
            task_id: Arc::from("constrained"),
            budget: 4000,
            actual: 2500,
        };
        assert_eq!(event.task_id(), Some("constrained"));
        let json = serde_json::to_string(&event).unwrap();
        assert!(json.contains("budget_ok"), "Should serialize: {json}");
        let round: EventKind = serde_json::from_str(&json).unwrap();
        if let EventKind::BudgetOk { budget, actual, .. } = round {
            assert_eq!(budget, 4000);
            assert_eq!(actual, 2500);
        } else {
            panic!("Roundtrip failed");
        }
    }

    #[test]
    fn test_budget_exceeded_event() {
        let event = EventKind::BudgetExceeded {
            task_id: Arc::from("oversize"),
            budget: 2000,
            actual: 8000,
            truncated_fields: vec!["data".to_string(), "context".to_string()],
        };
        assert_eq!(event.task_id(), Some("oversize"));
        let json = serde_json::to_string(&event).unwrap();
        assert!(json.contains("budget_exceeded"), "Should serialize: {json}");
        assert!(json.contains("truncated_fields"));
        let round: EventKind = serde_json::from_str(&json).unwrap();
        if let EventKind::BudgetExceeded {
            budget,
            actual,
            truncated_fields,
            ..
        } = round
        {
            assert_eq!(budget, 2000);
            assert_eq!(actual, 8000);
            assert_eq!(truncated_fields, vec!["data", "context"]);
        } else {
            panic!("Roundtrip failed");
        }
    }

    // ═══════════════════════════════════════════
    // D.6: Orchestrator event tests
    // ═══════════════════════════════════════════

    #[test]
    fn orchestrator_started_serializes() {
        let event = EventKind::OrchestratorStarted {
            goal: "Research quantum computing".to_string(),
            max_rounds: 10,
            agent: Some("researcher".to_string()),
        };
        let json = serde_json::to_string(&event).unwrap();
        assert!(json.contains("orchestrator_started"));
        assert!(json.contains("quantum"));
        let round: EventKind = serde_json::from_str(&json).unwrap();
        assert_eq!(round.task_id(), None);
        assert!(round.is_workflow_event());
    }

    #[test]
    fn orchestrator_round_serializes() {
        let event = EventKind::OrchestratorRound {
            round: 3,
            records_count: 7,
            cost_usd: 1.23,
        };
        let json = serde_json::to_string(&event).unwrap();
        assert!(json.contains("orchestrator_round"));
        let round: EventKind = serde_json::from_str(&json).unwrap();
        assert_eq!(round.task_id(), None);
    }

    #[test]
    fn orchestrator_completed_serializes() {
        let event = EventKind::OrchestratorCompleted {
            rounds: 5,
            total_cost_usd: 2.50,
            confidence: 0.92,
        };
        let json = serde_json::to_string(&event).unwrap();
        assert!(json.contains("orchestrator_completed"));
        let round: EventKind = serde_json::from_str(&json).unwrap();
        assert!(round.is_workflow_event());
    }

    #[test]
    fn orchestrator_failed_serializes() {
        let event = EventKind::OrchestratorFailed {
            round: 7,
            reason: "Budget exceeded".to_string(),
        };
        let json = serde_json::to_string(&event).unwrap();
        assert!(json.contains("orchestrator_failed"));
        assert!(json.contains("Budget exceeded"));
    }

    #[test]
    fn eventlog_eviction_preserves_recent_events() {
        // SCALE-001: verify eviction keeps the most recent half
        let log = EventLog::new();
        // Fill beyond MAX_EVENTS
        for i in 0..(MAX_EVENTS + 100) {
            log.emit(EventKind::Log {
                task_id: Some(Arc::from(format!("task_{i}").as_str())),
                message: format!("event {i}"),
                level: "info".to_string(),
            });
        }
        let events = log.events();
        // After eviction of oldest half, we should have roughly MAX_EVENTS/2 + 100
        assert!(
            events.len() < MAX_EVENTS,
            "Events should have been evicted, got {}",
            events.len()
        );
        // Most recent events should be preserved
        let last = events.last().unwrap();
        if let EventKind::Log { message, .. } = &last.kind {
            let expected = format!("event {}", MAX_EVENTS + 99);
            assert_eq!(message, &expected);
        } else {
            panic!("Expected Log event");
        }
    }
}
