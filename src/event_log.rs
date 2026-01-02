//! Event Sourcing for workflow execution (v0.1)
//!
//! Provides full audit trail with replay capability.
//! - Event: envelope with id + timestamp + kind
//! - EventKind: 10 variants across 3 levels (workflow/task/fine-grained)
//! - EventLog: thread-safe, append-only log

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Instant;

use parking_lot::RwLock; // 2-3x faster than std::sync::RwLock

use serde::{Deserialize, Serialize};
use serde_json::Value;

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
    },
    WorkflowCompleted {
        final_output: Value,
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
        /// Resolved inputs from TaskContext (what the task receives)
        inputs: Value,
    },
    TaskCompleted {
        task_id: Arc<str>,
        output: Value,
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
        output_len: usize,
        tokens_used: Option<u32>,
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
            | Self::ProviderResponded { task_id, .. } => Some(task_id),
            Self::WorkflowStarted { .. }
            | Self::WorkflowCompleted { .. }
            | Self::WorkflowFailed { .. } => None,
        }
    }

    /// Check if this is a workflow-level event
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

    /// Get all events (cloned)
    pub fn events(&self) -> Vec<Event> {
        self.events.read().clone()
    }

    /// Filter events by task ID
    pub fn filter_task(&self, task_id: &str) -> Vec<Event> {
        self.events()
            .into_iter()
            .filter(|e| e.kind.task_id() == Some(task_id))
            .collect()
    }

    /// Filter workflow-level events only
    pub fn workflow_events(&self) -> Vec<Event> {
        self.events()
            .into_iter()
            .filter(|e| e.kind.is_workflow_event())
            .collect()
    }

    /// Serialize to JSON for persistence/debugging
    pub fn to_json(&self) -> Value {
        serde_json::to_value(self.events()).unwrap_or(Value::Null)
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

        let workflow = EventKind::WorkflowStarted { task_count: 5 };
        assert_eq!(workflow.task_id(), None);
    }

    #[test]
    fn eventkind_is_workflow_event() {
        assert!(EventKind::WorkflowStarted { task_count: 3 }.is_workflow_event());
        assert!(EventKind::WorkflowCompleted {
            final_output: json!("done"),
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
            output: json!({"message": "Hello"}),
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

        let id1 = log.emit(EventKind::WorkflowStarted { task_count: 3 });
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
        log.emit(EventKind::WorkflowStarted { task_count: 2 });
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
        log.emit(EventKind::WorkflowStarted { task_count: 2 });
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
            output: json!("result"),
            duration_ms: 100,
        });

        let alpha_events = log.filter_task("alpha");
        assert_eq!(alpha_events.len(), 2); // Started + Completed
        assert!(alpha_events.iter().all(|e| e.kind.task_id() == Some("alpha")));

        let beta_events = log.filter_task("beta");
        assert_eq!(beta_events.len(), 1);
    }

    #[test]
    fn eventlog_workflow_events_returns_only_workflow() {
        let log = EventLog::new();
        log.emit(EventKind::WorkflowStarted { task_count: 1 });
        log.emit(EventKind::TaskStarted {
            task_id: "t1".into(),
            inputs: json!({}),
        });
        log.emit(EventKind::WorkflowCompleted {
            final_output: json!("done"),
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
        log.emit(EventKind::WorkflowStarted { task_count: 1 });

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
        log.emit(EventKind::WorkflowStarted { task_count: 1 });

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
}
