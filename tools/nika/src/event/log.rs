//! EventLog - Event sourcing implementation
//!
//! Provides full audit trail with replay capability.
//! - Event: envelope with id + timestamp + kind
//! - EventKind: 34 variants across 11 categories (workflow/task/fine-grained/MCP/context/agent/guardrails/builtin/artifact/structured-output/limits)
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

// ═══════════════════════════════════════════════════════════════
// Helper structs for ContextAssembled event
// ═══════════════════════════════════════════════════════════════

/// A source included in the assembled context
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ContextSource {
    /// Node/source identifier
    pub node: String,
    /// Token count for this source
    pub tokens: u32,
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
    pub input_tokens: u32,

    /// Output tokens generated for this turn
    pub output_tokens: u32,

    /// Cache read tokens (Anthropic prompt caching)
    #[serde(default)]
    pub cache_read_tokens: u32,

    /// Stop reason: "end_turn", "tool_use", "max_tokens", "stop_sequence"
    pub stop_reason: String,
}

impl AgentTurnMetadata {
    /// Create metadata for a simple text response (no thinking)
    pub fn text_only(response: impl Into<String>, stop_reason: impl Into<String>) -> Self {
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
        input_tokens: u32,
        output_tokens: u32,
        stop_reason: impl Into<String>,
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
    pub fn total_tokens(&self) -> u32 {
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
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
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
    TaskScheduled {
        task_id: Arc<str>,
        dependencies: Vec<Arc<str>>,
    },
    /// Task execution begins with resolved inputs from with: block
    TaskStarted {
        task_id: Arc<str>,
        /// Verb type (infer, exec, fetch, invoke, agent)
        verb: Arc<str>,
        /// Resolved inputs from ResolvedBindings (what the task receives)
        inputs: Value,
    },
    TaskCompleted {
        task_id: Arc<str>,
        output: Arc<Value>,
        duration_ms: u64,
    },
    TaskFailed {
        task_id: Arc<str>,
        error: String,
        duration_ms: u64,
    },

    // ═══════════════════════════════════════════
    // FINE-GRAINED (template/provider)
    // ═══════════════════════════════════════════
    TemplateResolved {
        task_id: Arc<str>,
        template: String,
        result: String,
    },
    ProviderCalled {
        task_id: Arc<str>,
        provider: String,
        model: String,
        prompt_len: usize,
    },
    ProviderResponded {
        task_id: Arc<str>,
        /// API request ID (for debugging with provider)
        request_id: Option<String>,
        /// Input tokens
        input_tokens: u32,
        /// Output tokens
        output_tokens: u32,
        /// Cache read tokens (if any)
        cache_read_tokens: u32,
        /// Time to first token (ms), if known
        ttft_ms: Option<u64>,
        /// Finish reason
        finish_reason: String,
        /// Estimated cost in USD
        cost_usd: f64,
    },

    // ═══════════════════════════════════════════
    // CONTEXT ASSEMBLY
    // ═══════════════════════════════════════════
    /// Context assembly event for observability
    ContextAssembled {
        task_id: Arc<str>,
        /// Sources included in context
        sources: Vec<ContextSource>,
        /// Items excluded (with reasons)
        excluded: Vec<ExcludedItem>,
        /// Total tokens in assembled context
        total_tokens: u32,
        /// Budget utilization percentage
        budget_used_pct: f32,
        /// Was context truncated?
        truncated: bool,
    },

    // ═══════════════════════════════════════════
    // MCP EVENTS
    // ═══════════════════════════════════════════
    /// MCP tool call or resource read initiated
    McpInvoke {
        task_id: Arc<str>,
        /// Unique call ID for correlating with McpResponse
        call_id: String,
        mcp_server: String,
        tool: Option<String>,
        resource: Option<String>,
        /// Full params passed to MCP tool (for TUI display)
        #[serde(skip_serializing_if = "Option::is_none")]
        params: Option<Value>,
    },
    /// MCP operation completed
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
        response: Option<Value>,
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
    /// Agent loop started
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
    AgentTurn {
        task_id: Arc<str>,
        turn_index: u32,
        /// Event kind: "started", "continue", "natural_completion", "explicit_completion"
        kind: String,
        /// Turn metadata including response text, tokens, thinking
        #[serde(skip_serializing_if = "Option::is_none")]
        metadata: Option<AgentTurnMetadata>,
    },
    /// Agent loop completed (reached stop condition or max turns)
    AgentComplete {
        task_id: Arc<str>,
        turns: u32,
        stop_reason: String,
    },

    // ═══════════════════════════════════════════
    // NESTED AGENT EVENTS
    // ═══════════════════════════════════════════
    /// A sub-agent was spawned by a parent agent
    AgentSpawned {
        /// ID of the parent task that spawned the child
        parent_task_id: Arc<str>,
        /// ID of the newly spawned child task
        child_task_id: Arc<str>,
        /// Current depth level (1 = root agent spawning first child)
        depth: u32,
    },

    // ═══════════════════════════════════════════
    // GUARDRAIL EVENTS
    // ═══════════════════════════════════════════
    /// Guardrail check passed
    GuardrailPassed {
        /// Task ID for correlation
        task_id: Arc<str>,
        /// Guardrail type: "length", "schema", "regex"
        guardrail_type: String,
        /// Human-readable description of the guardrail
        description: String,
    },
    /// Guardrail check failed
    GuardrailFailed {
        /// Task ID for correlation
        task_id: Arc<str>,
        /// Guardrail type: "length", "schema", "regex", "llm"
        guardrail_type: String,
        /// Human-readable description of the guardrail
        description: String,
        /// Error message explaining why it failed
        message: String,
    },
    /// Guardrail failure requires escalation
    ///
    /// Emitted when a guardrail with `on_failure: escalate` fails.
    /// This signals that human intervention or special handling is needed.
    GuardrailEscalation {
        /// Task ID for correlation
        task_id: Arc<str>,
        /// Guardrail type: "length", "schema", "regex", "llm"
        guardrail_type: String,
        /// Guardrail ID for identification
        guardrail_id: String,
        /// Error message explaining why it failed
        message: String,
        /// Severity level: "low", "medium", "high", "critical"
        severity: String,
        /// Suggested action (optional)
        #[serde(skip_serializing_if = "Option::is_none")]
        suggested_action: Option<String>,
    },

    // ═══════════════════════════════════════════
    // BUILTIN TOOL EVENTS
    // ═══════════════════════════════════════════
    /// Log event emitted by nika:log builtin tool
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
    ArtifactWritten {
        /// Task that produced this artifact
        task_id: Arc<str>,
        /// Final resolved path
        path: String,
        /// Size in bytes
        size: u64,
        /// Output format (text, json)
        format: String,
    },
    /// Artifact write failed
    ArtifactFailed {
        /// Task that produced this artifact
        task_id: Arc<str>,
        /// Intended path
        path: String,
        /// Error reason
        reason: String,
    },

    // ═══════════════════════════════════════════
    // STRUCTURED OUTPUT EVENTS
    // ═══════════════════════════════════════════
    /// Structured output extraction attempt at a specific layer
    ///
    /// Emitted for each layer/retry attempt in the 4-layer defense system:
    /// - Layer 1: rig Extractor (Rust types with JsonSchema)
    /// - Layer 2: Extract + Validate (post-processing)
    /// - Layer 3: Retry with Feedback
    /// - Layer 4: LLM Repair
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

    // ═══════════════════════════════════════════════════════════════
    // LIMIT EVENTS
    // ═══════════════════════════════════════════════════════════════
    /// Limit reached during agent execution
    ///
    /// Emitted when an agent execution limit is hit (turns, tokens, cost, or duration).
    /// The action field indicates what was done in response.
    LimitReached {
        /// Task ID for correlation
        task_id: Arc<str>,
        /// Limit type: "turns", "tokens", "cost", "duration"
        limit_type: String,
        /// Current value when limit was reached
        value: f64,
        /// Configured threshold
        threshold: f64,
        /// Action taken: "complete_partial", "fail", "escalate"
        action: String,
    },
    /// Agent completed with partial results due to limit
    ///
    /// Emitted when an agent gracefully completes with partial results
    /// after hitting a limit with action: complete_partial.
    PartialCompletion {
        /// Task ID for correlation
        task_id: Arc<str>,
        /// Progress percentage (0.0-1.0)
        progress: f64,
        /// Preview of partial result (truncated if long)
        result_preview: String,
    },
}

impl EventKind {
    /// Extract task_id if event is task-related
    pub fn task_id(&self) -> Option<&str> {
        match self {
            Self::TaskScheduled { task_id, .. }
            | Self::TaskStarted { task_id, .. }
            | Self::TaskCompleted { task_id, .. }
            | Self::TaskFailed { task_id, .. }
            | Self::TemplateResolved { task_id, .. }
            | Self::ProviderCalled { task_id, .. }
            | Self::ProviderResponded { task_id, .. }
            | Self::ContextAssembled { task_id, .. }
            | Self::McpInvoke { task_id, .. }
            | Self::McpResponse { task_id, .. }
            | Self::McpRetry { task_id, .. }  // P2 Fix: Added McpRetry
            | Self::AgentStart { task_id, .. }
            | Self::AgentTurn { task_id, .. }
            | Self::AgentComplete { task_id, .. }
            | Self::ArtifactWritten { task_id, .. }
            | Self::ArtifactFailed { task_id, .. }
            | Self::StructuredOutputAttempt { task_id, .. }
            | Self::StructuredOutputSuccess { task_id, .. }
            | Self::GuardrailPassed { task_id, .. }
            | Self::GuardrailFailed { task_id, .. }
            | Self::GuardrailEscalation { task_id, .. }
            | Self::LimitReached { task_id, .. }
            | Self::PartialCompletion { task_id, .. } => Some(task_id),
            // AgentSpawned uses parent_task_id as the primary task reference
            Self::AgentSpawned { parent_task_id, .. } => Some(parent_task_id),
            // Log and Custom may optionally have task_id
            Self::Log { task_id, .. } | Self::Custom { task_id, .. } => {
                task_id.as_ref().map(|s| s.as_ref())
            }
            Self::WorkflowStarted { .. }
            | Self::WorkflowCompleted { .. }
            | Self::WorkflowFailed { .. }
            | Self::WorkflowAborted { .. }
            | Self::WorkflowPaused
            | Self::WorkflowResumed
            | Self::McpConnected { .. }
            | Self::McpError { .. } => None,
        }
    }

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
        )
    }
}

/// Thread-safe, append-only event log
///
/// Optionally supports real-time event broadcasting for TUI integration.
/// Use `new_with_broadcast()` to create an EventLog that sends events
/// to subscribers via tokio broadcast channel.
#[derive(Clone)]
pub struct EventLog {
    events: Arc<RwLock<Vec<Event>>>,
    start_time: Instant,
    next_id: Arc<AtomicU64>,
    /// Optional broadcast sender for TUI real-time updates
    broadcast_tx: Option<broadcast::Sender<Event>>,
}

impl EventLog {
    /// Create a new event log (call at workflow start)
    pub fn new() -> Self {
        Self {
            events: Arc::new(RwLock::new(Vec::new())),
            start_time: Instant::now(),
            next_id: Arc::new(AtomicU64::new(0)),
            broadcast_tx: None,
        }
    }

    /// Create a new event log with broadcast channel for TUI
    ///
    /// Returns (EventLog, Receiver) tuple. Pass the receiver to TUI App.
    /// P1 Fix: Increase channel capacity from 256 to 512 events (buffer for TUI lag + fast providers).
    pub fn new_with_broadcast() -> (Self, broadcast::Receiver<Event>) {
        let (tx, rx) = broadcast::channel(512);
        let event_log = Self {
            events: Arc::new(RwLock::new(Vec::new())),
            start_time: Instant::now(),
            next_id: Arc::new(AtomicU64::new(0)),
            broadcast_tx: Some(tx),
        };
        (event_log, rx)
    }

    /// Subscribe to event broadcasts (for additional TUI observers)
    ///
    /// Returns None if this EventLog was not created with `new_with_broadcast()`.
    pub fn subscribe(&self) -> Option<broadcast::Receiver<Event>> {
        self.broadcast_tx.as_ref().map(|tx| tx.subscribe())
    }

    /// Emit an event (thread-safe, returns event ID)
    ///
    /// If broadcast channel is configured, also sends to subscribers.
    pub fn emit(&self, kind: EventKind) -> u64 {
        let id = self.next_id.fetch_add(1, Ordering::SeqCst);
        let event = Event {
            id,
            timestamp_ms: self.start_time.elapsed().as_millis() as u64,
            kind,
        };

        self.events.write().push(event.clone()); // parking_lot: no unwrap needed

        // Broadcast to TUI if configured (ignore errors - TUI might be closed)
        if let Some(ref tx) = self.broadcast_tx {
            let _ = tx.send(event);
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
    fn provider_responded(task_id: &str, input_tokens: u32, output_tokens: u32) -> EventKind {
        EventKind::ProviderResponded {
            task_id: Arc::from(task_id),
            request_id: Some("req-456".to_string()),
            input_tokens,
            output_tokens,
            cache_read_tokens: 0,
            ttft_ms: Some(150),
            finish_reason: "stop".to_string(),
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
            inputs: json!({}),
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
            inputs: json!({}),
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
                inputs: json!({"weather": "sunny"}),
            }
        );
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
            inputs: json!({}),
        });
        let id3 = log.emit(EventKind::TaskStarted {
            verb: "infer".into(),
            task_id: "t2".into(),
            inputs: json!({}),
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
            inputs: json!({}),
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
            inputs: json!({}),
        });
        log.emit(EventKind::TaskStarted {
            verb: "infer".into(),
            task_id: "beta".into(),
            inputs: json!({}),
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
            inputs: json!({}),
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
            inputs: json!({}),
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
            inputs: json!({}),
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
                        inputs: json!({}),
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
            inputs: json!({}),
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
            inputs: inputs.clone(),
        });

        let events = log.filter_task("analyze");
        assert_eq!(events.len(), 1);

        if let EventKind::TaskStarted {
            inputs: captured, ..
        } = &events[0].kind
        {
            assert_eq!(captured, &inputs);
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
            finish_reason: "stop".to_string(),
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
            assert_eq!(finish_reason, "stop");
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
        assert_eq!(metadata.stop_reason, "end_turn");
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
        assert_eq!(metadata.stop_reason, "tool_use");
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
            stop_reason: "end_turn".to_string(),
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
            stop_reason: "end_turn".to_string(),
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
            kind: "end_turn".to_string(), // Canonical snake_case
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
            kind: "started".to_string(),
            metadata: None,
        });

        let events = log.filter_task("agent_task");
        assert_eq!(events.len(), 1);

        if let EventKind::AgentTurn { metadata, kind, .. } = &events[0].kind {
            assert!(metadata.is_none());
            assert_eq!(kind, "started");
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
            guardrail_type: "length".to_string(),
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
            assert_eq!(guardrail_type, "length");
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
            guardrail_type: "regex".to_string(),
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
            assert_eq!(guardrail_type, "regex");
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
            guardrail_type: "schema".to_string(),
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
            guardrail_type: "length".to_string(),
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
            guardrail_type: "length".to_string(),
            description: "min_words: 5".to_string(),
        };
        assert_eq!(passed.task_id(), Some("guard1"));

        let failed = EventKind::GuardrailFailed {
            task_id: "guard2".into(),
            guardrail_type: "regex".to_string(),
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
            guardrail_type: "length".to_string(),
            description: "min_words: 10, max_words: 100".to_string(),
        });

        // Schema guardrail fails
        log.emit(EventKind::GuardrailFailed {
            task_id: "validate_output".into(),
            guardrail_type: "schema".to_string(),
            description: "json_schema".to_string(),
            message: "Missing required field: 'title'".to_string(),
        });

        // Regex guardrail passes
        log.emit(EventKind::GuardrailPassed {
            task_id: "validate_output".into(),
            guardrail_type: "regex".to_string(),
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
            guardrail_type: "llm".to_string(),
            guardrail_id: "content_safety".to_string(),
            message: "Content may be inappropriate for the target audience".to_string(),
            severity: "high".to_string(),
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
            assert_eq!(guardrail_type, "llm");
            assert_eq!(guardrail_id, "content_safety");
            assert!(message.contains("inappropriate"));
            assert_eq!(severity, "high");
            assert!(suggested_action.is_some());
        } else {
            panic!("Expected GuardrailEscalation event");
        }
    }

    #[test]
    fn guardrail_escalation_serializes() {
        let event = EventKind::GuardrailEscalation {
            task_id: "task1".into(),
            guardrail_type: "llm".to_string(),
            guardrail_id: "safety_check".to_string(),
            message: "Safety violation detected".to_string(),
            severity: "critical".to_string(),
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
            guardrail_type: "llm".to_string(),
            guardrail_id: "quality".to_string(),
            message: "Quality below threshold".to_string(),
            severity: "medium".to_string(),
            suggested_action: None,
        };
        assert_eq!(escalation.task_id(), Some("esc1"));
    }

    // ═══════════════════════════════════════════════════════════════
    // Limit events tests
    // ═══════════════════════════════════════════════════════════════

    #[test]
    fn limit_reached_event() {
        let log = EventLog::new();

        log.emit(EventKind::LimitReached {
            task_id: "agent_task".into(),
            limit_type: "turns".to_string(),
            value: 20.0,
            threshold: 20.0,
            action: "complete_partial".to_string(),
        });

        let events = log.filter_task("agent_task");
        assert_eq!(events.len(), 1);

        if let EventKind::LimitReached {
            limit_type,
            value,
            threshold,
            action,
            ..
        } = &events[0].kind
        {
            assert_eq!(limit_type, "turns");
            assert!((value - 20.0).abs() < f64::EPSILON);
            assert!((threshold - 20.0).abs() < f64::EPSILON);
            assert_eq!(action, "complete_partial");
        } else {
            panic!("Expected LimitReached event");
        }
    }

    #[test]
    fn limit_reached_cost_event() {
        let log = EventLog::new();

        log.emit(EventKind::LimitReached {
            task_id: "expensive_task".into(),
            limit_type: "cost".to_string(),
            value: 2.50,
            threshold: 2.00,
            action: "fail".to_string(),
        });

        let events = log.filter_task("expensive_task");
        assert_eq!(events.len(), 1);

        if let EventKind::LimitReached {
            limit_type,
            value,
            threshold,
            action,
            ..
        } = &events[0].kind
        {
            assert_eq!(limit_type, "cost");
            assert!((value - 2.50).abs() < f64::EPSILON);
            assert!((threshold - 2.00).abs() < f64::EPSILON);
            assert_eq!(action, "fail");
        } else {
            panic!("Expected LimitReached event");
        }
    }

    #[test]
    fn partial_completion_event() {
        let log = EventLog::new();

        log.emit(EventKind::PartialCompletion {
            task_id: "agent_task".into(),
            progress: 0.75,
            result_preview: "Generated 15 of 20 items...".to_string(),
        });

        let events = log.filter_task("agent_task");
        assert_eq!(events.len(), 1);

        if let EventKind::PartialCompletion {
            progress,
            result_preview,
            ..
        } = &events[0].kind
        {
            assert!((progress - 0.75).abs() < f64::EPSILON);
            assert_eq!(result_preview, "Generated 15 of 20 items...");
        } else {
            panic!("Expected PartialCompletion event");
        }
    }

    #[test]
    fn limit_reached_serializes() {
        let event = EventKind::LimitReached {
            task_id: "task1".into(),
            limit_type: "tokens".to_string(),
            value: 50000.0,
            threshold: 50000.0,
            action: "complete_partial".to_string(),
        };

        let json = serde_json::to_value(&event).unwrap();
        assert_eq!(json["type"], "limit_reached");
        assert_eq!(json["task_id"], "task1");
        assert_eq!(json["limit_type"], "tokens");
        assert_eq!(json["value"], 50000.0);
        assert_eq!(json["threshold"], 50000.0);
        assert_eq!(json["action"], "complete_partial");
    }

    #[test]
    fn partial_completion_serializes() {
        let event = EventKind::PartialCompletion {
            task_id: "task1".into(),
            progress: 0.5,
            result_preview: "Partial results...".to_string(),
        };

        let json = serde_json::to_value(&event).unwrap();
        assert_eq!(json["type"], "partial_completion");
        assert_eq!(json["task_id"], "task1");
        assert_eq!(json["progress"], 0.5);
        assert_eq!(json["result_preview"], "Partial results...");
    }

    #[test]
    fn limit_events_task_id_extraction() {
        let limit = EventKind::LimitReached {
            task_id: "limit1".into(),
            limit_type: "duration".to_string(),
            value: 300.0,
            threshold: 300.0,
            action: "escalate".to_string(),
        };
        assert_eq!(limit.task_id(), Some("limit1"));

        let partial = EventKind::PartialCompletion {
            task_id: "partial1".into(),
            progress: 0.8,
            result_preview: "Almost done".to_string(),
        };
        assert_eq!(partial.task_id(), Some("partial1"));
    }

    #[test]
    fn limit_events_full_workflow() {
        // Simulates an agent hitting limit and completing partially
        let log = EventLog::new();

        // Agent starts
        log.emit(EventKind::AgentStart {
            task_id: "research_agent".into(),
            max_turns: 20,
            mcp_servers: vec!["novanet".to_string()],
        });

        // Agent runs for a while...
        log.emit(EventKind::AgentTurn {
            task_id: "research_agent".into(),
            turn_index: 15,
            kind: "continue".to_string(),
            metadata: None,
        });

        // Limit reached
        log.emit(EventKind::LimitReached {
            task_id: "research_agent".into(),
            limit_type: "turns".to_string(),
            value: 20.0,
            threshold: 20.0,
            action: "complete_partial".to_string(),
        });

        // Partial completion
        log.emit(EventKind::PartialCompletion {
            task_id: "research_agent".into(),
            progress: 0.8,
            result_preview: "Found 16 of 20 expected results...".to_string(),
        });

        // Agent completes
        log.emit(EventKind::AgentComplete {
            task_id: "research_agent".into(),
            turns: 20,
            stop_reason: "limit_reached".to_string(),
        });

        let events = log.filter_task("research_agent");
        assert_eq!(events.len(), 5);

        // Check sequence
        assert!(matches!(&events[0].kind, EventKind::AgentStart { .. }));
        assert!(matches!(&events[1].kind, EventKind::AgentTurn { .. }));
        assert!(matches!(&events[2].kind, EventKind::LimitReached { .. }));
        assert!(matches!(
            &events[3].kind,
            EventKind::PartialCompletion { .. }
        ));
        assert!(matches!(&events[4].kind, EventKind::AgentComplete { .. }));
    }

    // ═══════════════════════════════════════════════════════════════
    // WAVE 2 TESTS: NDJSON event system audit
    // ═══════════════════════════════════════════════════════════════

    /// Helper: build one instance of every EventKind variant (all 34)
    fn all_34_variants() -> Vec<EventKind> {
        vec![
            // Workflow (6)
            EventKind::WorkflowStarted {
                task_count: 3,
                generation_id: "gen-1".into(),
                workflow_hash: "abc123".into(),
                nika_version: "0.27.0".into(),
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
                inputs: serde_json::json!({}),
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
            },
            EventKind::ProviderResponded {
                task_id: "t1".into(),
                request_id: Some("req-abc".into()),
                input_tokens: 100,
                output_tokens: 50,
                cache_read_tokens: 10,
                ttft_ms: Some(200),
                finish_reason: "end_turn".into(),
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
                params: Some(serde_json::json!({"query": "test"})),
            },
            EventKind::McpResponse {
                task_id: "t1".into(),
                call_id: "call-1".into(),
                output_len: 256,
                duration_ms: 80,
                cached: false,
                is_error: false,
                response: Some(serde_json::json!({"found": 3})),
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
                kind: "started".into(),
                metadata: Some(AgentTurnMetadata {
                    thinking: Some("Let me think...".into()),
                    response_text: "Here is my response".into(),
                    input_tokens: 100,
                    output_tokens: 50,
                    cache_read_tokens: 0,
                    stop_reason: "end_turn".into(),
                }),
            },
            EventKind::AgentComplete {
                task_id: "agent1".into(),
                turns: 3,
                stop_reason: "natural_completion".into(),
            },
            EventKind::AgentSpawned {
                parent_task_id: "agent1".into(),
                child_task_id: "sub-agent1".into(),
                depth: 1,
            },
            // Guardrails (3)
            EventKind::GuardrailPassed {
                task_id: "t1".into(),
                guardrail_type: "length".into(),
                description: "max 1000 chars".into(),
            },
            EventKind::GuardrailFailed {
                task_id: "t1".into(),
                guardrail_type: "schema".into(),
                description: "JSON schema".into(),
                message: "missing field 'title'".into(),
            },
            EventKind::GuardrailEscalation {
                task_id: "t1".into(),
                guardrail_type: "llm".into(),
                guardrail_id: "safety-check".into(),
                message: "content flagged".into(),
                severity: "high".into(),
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
            },
            EventKind::ArtifactFailed {
                task_id: "t1".into(),
                path: "/tmp/output.json".into(),
                reason: "permission denied".into(),
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
            // Limits (2)
            EventKind::LimitReached {
                task_id: "t1".into(),
                limit_type: "turns".into(),
                value: 10.0,
                threshold: 10.0,
                action: "complete_partial".into(),
            },
            EventKind::PartialCompletion {
                task_id: "t1".into(),
                progress: 0.8,
                result_preview: "partial result...".into(),
            },
        ]
    }

    #[test]
    fn wave2_variant_count_is_34() {
        let variants = all_34_variants();
        assert_eq!(
            variants.len(),
            34,
            "EventKind should have exactly 34 variants"
        );
    }

    #[test]
    fn wave2_all_variants_serialize_deserialize_roundtrip() {
        for (i, variant) in all_34_variants().into_iter().enumerate() {
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
        for (i, variant) in all_34_variants().into_iter().enumerate() {
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
            nika_version: "0.27.0".into(),
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
        // EventLog broadcast capacity is 512
        // new_with_broadcast returns (EventLog, Receiver)
        let (log, mut rx) = EventLog::new_with_broadcast();

        // Emit 600 events (exceeds 512 capacity)
        for _ in 0..600 {
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
        // Should have experienced lag because 600 > 512
        assert!(
            lagged,
            "Expected broadcast lag when emitting 600 events into 512 capacity channel"
        );
        // But all 600 events should be in the log (log is unbounded)
        assert_eq!(log.events().len(), 600);
    }

    // Dead variant tests: these 3 variants are defined but never emitted in runtime code
    #[test]
    fn wave2_guardrail_escalation_is_dead_variant() {
        // GuardrailEscalation is defined but never emitted in runtime.
        // Only appears in tests. This test documents that fact.
        let variant = EventKind::GuardrailEscalation {
            task_id: "t1".into(),
            guardrail_type: "llm".into(),
            guardrail_id: "check-1".into(),
            message: "flagged".into(),
            severity: "high".into(),
            suggested_action: None,
        };
        // It serializes fine -- it's just never emitted
        let json = serde_json::to_string(&variant).unwrap();
        assert!(json.contains("guardrail_escalation"));
    }

    #[test]
    fn wave2_limit_reached_is_dead_variant() {
        // LimitReached is defined but never emitted in runtime code
        let variant = EventKind::LimitReached {
            task_id: "t1".into(),
            limit_type: "turns".into(),
            value: 10.0,
            threshold: 10.0,
            action: "fail".into(),
        };
        let json = serde_json::to_string(&variant).unwrap();
        assert!(json.contains("limit_reached"));
    }

    #[test]
    fn wave2_partial_completion_is_dead_variant() {
        // PartialCompletion is defined but never emitted in runtime code
        let variant = EventKind::PartialCompletion {
            task_id: "t1".into(),
            progress: 0.5,
            result_preview: "half done".into(),
        };
        let json = serde_json::to_string(&variant).unwrap();
        assert!(json.contains("partial_completion"));
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
            finish_reason: "end_turn".into(),
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
            guardrail_type: "llm".into(),
            guardrail_id: "check".into(),
            message: "flagged".into(),
            severity: "high".into(),
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
            finish_reason: "end_turn".into(),
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
        let variants = all_34_variants();
        // Variants WITH task_id (26 of them)
        let with_task_id: Vec<_> = variants.iter().filter(|v| v.task_id().is_some()).collect();
        // Variants WITHOUT task_id (8): 6 workflow + McpConnected + McpError
        let without_task_id: Vec<_> = variants.iter().filter(|v| v.task_id().is_none()).collect();

        // Custom has task_id: None in our test data, so it goes in without
        // Log has task_id: Some("t1"), so it goes in with
        assert_eq!(
            with_task_id.len(),
            25,
            "25 variants should have task_id (including Log with Some)"
        );
        assert_eq!(
            without_task_id.len(),
            9,
            "9 variants should lack task_id (workflow-level + McpConnected + McpError + Custom with None)"
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
            inputs: serde_json::json!({}),
        };
        assert!(!non_wf.is_workflow_event());
    }
}
