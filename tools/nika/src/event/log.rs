//! EventLog - Event sourcing implementation (v0.2)
//!
//! Provides full audit trail with replay capability.
//! - Event: envelope with id + timestamp + kind
//! - EventKind: 13 variants across 5 levels (workflow/task/fine-grained/MCP/context)
//! - EventLog: thread-safe, append-only log

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Instant;

use parking_lot::RwLock; // 2-3x faster than std::sync::RwLock

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
/// Uses Arc<str> for task_id fields to enable zero-cost cloning.
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

    // ═══════════════════════════════════════════
    // TASK LEVEL
    // ═══════════════════════════════════════════
    TaskScheduled {
        task_id: Arc<str>,
        dependencies: Vec<Arc<str>>,
    },
    /// Task execution begins with resolved inputs from use: block
    TaskStarted {
        task_id: Arc<str>,
        /// Resolved inputs from UseBindings (what the task receives)
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
    // CONTEXT ASSEMBLY (v0.2)
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
    // MCP EVENTS (v0.2)
    // ═══════════════════════════════════════════
    /// MCP tool call or resource read initiated
    McpInvoke {
        task_id: Arc<str>,
        mcp_server: String,
        tool: Option<String>,
        resource: Option<String>,
    },
    /// MCP operation completed
    McpResponse {
        task_id: Arc<str>,
        output_len: usize,
    },

    // ═══════════════════════════════════════════
    // AGENT EVENTS (v0.2)
    // ═══════════════════════════════════════════
    /// Agent loop started
    AgentStart {
        task_id: Arc<str>,
        max_turns: u32,
        mcp_servers: Vec<String>,
    },
    /// Agent turn event (started, completed, stop_condition_met, etc.)
    AgentTurn {
        task_id: Arc<str>,
        turn_index: u32,
        /// Event kind: "started", "continue", "natural_completion", "stop_condition_met"
        kind: String,
        /// Cumulative token count (optional)
        tokens: Option<u32>,
    },
    /// Agent loop completed (reached stop condition or max turns)
    AgentComplete {
        task_id: Arc<str>,
        turns: u32,
        stop_reason: String,
    },
}

impl EventKind {
    /// Extract task_id if event is task-related
    #[allow(dead_code)] // Used in tests and future replay
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
            | Self::AgentStart { task_id, .. }
            | Self::AgentTurn { task_id, .. }
            | Self::AgentComplete { task_id, .. } => Some(task_id),
            Self::WorkflowStarted { .. }
            | Self::WorkflowCompleted { .. }
            | Self::WorkflowFailed { .. } => None,
        }
    }

    /// Check if this is a workflow-level event
    #[allow(dead_code)] // Used in tests and future replay
    pub fn is_workflow_event(&self) -> bool {
        matches!(
            self,
            Self::WorkflowStarted { .. }
                | Self::WorkflowCompleted { .. }
                | Self::WorkflowFailed { .. }
        )
    }
}

/// Thread-safe, append-only event log
#[derive(Clone)]
pub struct EventLog {
    events: Arc<RwLock<Vec<Event>>>,
    start_time: Instant,
    next_id: Arc<AtomicU64>,
}

impl EventLog {
    /// Create a new event log (call at workflow start)
    pub fn new() -> Self {
        Self {
            events: Arc::new(RwLock::new(Vec::new())),
            start_time: Instant::now(),
            next_id: Arc::new(AtomicU64::new(0)),
        }
    }

    /// Emit an event (thread-safe, returns event ID)
    pub fn emit(&self, kind: EventKind) -> u64 {
        let id = self.next_id.fetch_add(1, Ordering::SeqCst);
        let event = Event {
            id,
            timestamp_ms: self.start_time.elapsed().as_millis() as u64,
            kind,
        };

        self.events.write().push(event); // parking_lot: no unwrap needed
        id
    }

    /// Get all events (cloned - use `with_events` for zero-copy access)
    #[allow(dead_code)] // Used in tests and future export
    pub fn events(&self) -> Vec<Event> {
        self.events.read().clone()
    }

    /// Zero-copy access to events via callback
    ///
    /// Holds read lock for duration of callback - keep it short.
    /// Use this instead of `events()` when you don't need ownership.
    #[allow(dead_code)] // Used in optimized filter methods
    pub fn with_events<T>(&self, f: impl FnOnce(&[Event]) -> T) -> T {
        f(&self.events.read())
    }

    /// Filter events by task ID (zero-copy filtering)
    #[allow(dead_code)] // Used in tests and future debugging
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
    #[allow(dead_code)] // Used in tests and future export
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
    #[allow(dead_code)] // Used in tests and future metrics
    pub fn count_task(&self, task_id: &str) -> usize {
        self.with_events(|events| {
            events
                .iter()
                .filter(|e| e.kind.task_id() == Some(task_id))
                .count()
        })
    }

    /// Serialize to JSON for persistence/debugging
    #[allow(dead_code)] // Used in tests and future export
    pub fn to_json(&self) -> Value {
        self.with_events(|events| serde_json::to_value(events).unwrap_or(Value::Null))
    }

    /// Number of events
    #[allow(dead_code)] // Used in tests
    pub fn len(&self) -> usize {
        self.events.read().len()
    }

    /// Check if empty
    #[allow(dead_code)] // Used in tests
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

    // ═══════════════════════════════════════════════════════════════
    // Test helpers
    // ═══════════════════════════════════════════════════════════════

    /// Create a WorkflowStarted event with test defaults
    fn workflow_started(task_count: usize) -> EventKind {
        EventKind::WorkflowStarted {
            task_count,
            generation_id: "test-gen-123".to_string(),
            workflow_hash: "abc123".to_string(),
            nika_version: "0.2.0".to_string(),
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
            "inputs": {"weather": "sunny"}
        });

        let kind: EventKind = serde_json::from_value(json).unwrap();
        assert_eq!(
            kind,
            EventKind::TaskStarted {
                task_id: "analyze".into(),
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
            task_id: "t1".into(),
            inputs: json!({}),
        });
        let id3 = log.emit(EventKind::TaskStarted {
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
            task_id: "alpha".into(),
            inputs: json!({}),
        });
        log.emit(EventKind::TaskStarted {
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
    // Enhanced event tests (v0.2 - generation_id, token tracking)
    // ═══════════════════════════════════════════════════════════════

    #[test]
    fn workflow_started_includes_generation_id() {
        let log = EventLog::new();
        log.emit(EventKind::WorkflowStarted {
            task_count: 3,
            generation_id: "gen-abc-123".to_string(),
            workflow_hash: "sha256:deadbeef".to_string(),
            nika_version: "0.2.1".to_string(),
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
            assert_eq!(nika_version, "0.2.1");
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
}
