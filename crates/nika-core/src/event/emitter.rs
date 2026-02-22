//! EventEmitter Trait - abstraction for event emission (v0.3)
//!
//! Enables dependency injection for testing - real emitters in production,
//! NoopEmitter or MockEmitter in tests.
//!
//! Key types:
//! - `EventEmitter`: Trait for emitting events
//! - `NoopEmitter`: Zero-cost no-op implementation for tests

use super::log::{EventKind, EventLog};

/// Trait for emitting events during workflow execution
///
/// Enables dependency injection: real EventLog in production,
/// NoopEmitter or custom mock in tests.
pub trait EventEmitter: Send + Sync {
    /// Emit an event and return its ID
    fn emit(&self, kind: EventKind) -> u64;
}

/// Implement EventEmitter for EventLog (the real implementation)
impl EventEmitter for EventLog {
    fn emit(&self, kind: EventKind) -> u64 {
        EventLog::emit(self, kind)
    }
}

/// No-op emitter for testing (zero allocation, always returns 0)
#[derive(Debug, Clone, Default)]
pub struct NoopEmitter;

impl NoopEmitter {
    /// Create a new NoopEmitter
    pub fn new() -> Self {
        Self
    }
}

impl EventEmitter for NoopEmitter {
    fn emit(&self, _kind: EventKind) -> u64 {
        0 // Always return 0, do nothing
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::sync::Arc;

    /// Use actual package version in tests to avoid version drift
    const TEST_VERSION: &str = env!("CARGO_PKG_VERSION");

    // ═══════════════════════════════════════════════════════════════
    // EventEmitter trait tests
    // ═══════════════════════════════════════════════════════════════

    #[test]
    fn event_emitter_trait_is_object_safe() {
        // Verify the trait can be used as a trait object
        fn accepts_emitter(_: &dyn EventEmitter) {}

        let log = EventLog::new();
        accepts_emitter(&log);

        let noop = NoopEmitter::new();
        accepts_emitter(&noop);
    }

    #[test]
    fn event_emitter_trait_works_with_arc() {
        // Verify it works with Arc (required for concurrent access)
        let emitter: Arc<dyn EventEmitter> = Arc::new(EventLog::new());
        let id = emitter.emit(EventKind::WorkflowStarted {
            task_count: 1,
            generation_id: "test".to_string(),
            workflow_hash: "hash".to_string(),
            nika_version: TEST_VERSION.to_string(),
        });
        assert_eq!(id, 0); // First event
    }

    // ═══════════════════════════════════════════════════════════════
    // EventLog as EventEmitter tests
    // ═══════════════════════════════════════════════════════════════

    #[test]
    fn eventlog_implements_emitter() {
        let log = EventLog::new();
        let emitter: &dyn EventEmitter = &log;

        let id = emitter.emit(EventKind::TaskStarted {
            task_id: Arc::from("test_task"),
            verb: "infer".into(),
            inputs: json!({}),
        });

        assert_eq!(id, 0);
        assert_eq!(log.len(), 1);
    }

    #[test]
    fn eventlog_emitter_returns_monotonic_ids() {
        let log = EventLog::new();
        let emitter: &dyn EventEmitter = &log;

        let id1 = emitter.emit(EventKind::WorkflowStarted {
            task_count: 2,
            generation_id: "gen1".to_string(),
            workflow_hash: "hash1".to_string(),
            nika_version: TEST_VERSION.to_string(),
        });
        let id2 = emitter.emit(EventKind::TaskStarted {
            task_id: Arc::from("task1"),
            verb: "infer".into(),
            inputs: json!({}),
        });
        let id3 = emitter.emit(EventKind::TaskCompleted {
            task_id: Arc::from("task1"),
            output: Arc::new(json!("done")),
            duration_ms: 100,
        });

        assert_eq!(id1, 0);
        assert_eq!(id2, 1);
        assert_eq!(id3, 2);
    }

    // ═══════════════════════════════════════════════════════════════
    // NoopEmitter tests
    // ═══════════════════════════════════════════════════════════════

    #[test]
    fn noop_emitter_always_returns_zero() {
        let noop = NoopEmitter::new();

        let id1 = noop.emit(EventKind::WorkflowStarted {
            task_count: 5,
            generation_id: "gen".to_string(),
            workflow_hash: "hash".to_string(),
            nika_version: TEST_VERSION.to_string(),
        });
        let id2 = noop.emit(EventKind::TaskStarted {
            task_id: Arc::from("task"),
            verb: "infer".into(),
            inputs: json!({}),
        });
        let id3 = noop.emit(EventKind::WorkflowCompleted {
            final_output: Arc::new(json!("output")),
            total_duration_ms: 1000,
        });

        assert_eq!(id1, 0);
        assert_eq!(id2, 0);
        assert_eq!(id3, 0);
    }

    #[test]
    fn noop_emitter_is_clone() {
        let noop = NoopEmitter::new();
        let _cloned = noop.clone();
    }

    #[test]
    fn noop_emitter_is_default() {
        let noop = NoopEmitter;
        assert_eq!(
            noop.emit(EventKind::WorkflowStarted {
                task_count: 1,
                generation_id: "".to_string(),
                workflow_hash: "".to_string(),
                nika_version: TEST_VERSION.to_string(),
            }),
            0
        );
    }

    #[test]
    fn noop_emitter_is_send_sync() {
        fn assert_send_sync<T: Send + Sync>() {}
        assert_send_sync::<NoopEmitter>();
    }

    // ═══════════════════════════════════════════════════════════════
    // Generic function tests
    // ═══════════════════════════════════════════════════════════════

    fn emit_workflow_started<E: EventEmitter>(emitter: &E, task_count: usize) -> u64 {
        emitter.emit(EventKind::WorkflowStarted {
            task_count,
            generation_id: "test-gen".to_string(),
            workflow_hash: "test-hash".to_string(),
            nika_version: TEST_VERSION.to_string(),
        })
    }

    #[test]
    fn generic_function_works_with_eventlog() {
        let log = EventLog::new();
        let id = emit_workflow_started(&log, 3);
        assert_eq!(id, 0);
        assert_eq!(log.len(), 1);
    }

    #[test]
    fn generic_function_works_with_noop() {
        let noop = NoopEmitter::new();
        let id = emit_workflow_started(&noop, 3);
        assert_eq!(id, 0);
    }
}
