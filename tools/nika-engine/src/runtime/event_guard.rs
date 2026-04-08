// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! RAII guard that guarantees TaskStarted/TaskFailed/TaskCompleted events.
//!
//! If dropped without calling `.complete()` or `.fail()`, emits TaskFailed
//! automatically. This makes silent event drops structurally impossible.

use std::sync::Arc;
use std::time::{Duration, Instant};

use nika_event::{EventKind, EventLog};

/// RAII guard that guarantees event emission for task lifecycle.
///
/// On creation, emits `TaskStarted`. Must be consumed via `.complete()` or
/// `.fail()`. If dropped without either (panic, early return), emits
/// `TaskFailed` automatically with NIKA-098.
pub struct TaskEventGuard {
    task_id: Arc<str>,
    event_log: EventLog,
    start: Instant,
    completed: bool,
}

impl TaskEventGuard {
    /// Create a guard and emit TaskStarted.
    pub fn start(
        event_log: EventLog,
        task_id: Arc<str>,
        verb: &str,
        inputs: serde_json::Value,
    ) -> Self {
        event_log.emit(EventKind::TaskStarted {
            task_id: Arc::clone(&task_id),
            verb: Arc::from(verb),
            inputs: Arc::new(inputs),
        });
        Self {
            task_id,
            event_log,
            start: Instant::now(),
            completed: false,
        }
    }

    /// Emit TaskCompleted and consume the guard (no Drop emission).
    pub fn complete(mut self, output: Arc<serde_json::Value>) {
        self.completed = true;
        self.event_log.emit(EventKind::TaskCompleted {
            task_id: Arc::clone(&self.task_id),
            output,
            duration_ms: self.start.elapsed().as_millis() as u64,
        });
    }

    /// Emit TaskFailed and consume the guard (no Drop emission).
    pub fn fail(mut self, error: &str, error_code: Option<String>) {
        self.completed = true;
        self.event_log.emit(EventKind::TaskFailed {
            task_id: Arc::clone(&self.task_id),
            error: error.to_string(),
            duration_ms: self.start.elapsed().as_millis() as u64,
            error_code,
        });
    }

    /// Elapsed time since guard creation.
    pub fn elapsed(&self) -> Duration {
        self.start.elapsed()
    }
}

impl Drop for TaskEventGuard {
    fn drop(&mut self) {
        if !self.completed {
            tracing::error!(
                task_id = %self.task_id,
                "TaskEventGuard dropped without completion — emitting TaskFailed"
            );
            self.event_log.emit(EventKind::TaskFailed {
                task_id: Arc::clone(&self.task_id),
                error: "internal: task event guard dropped without completion (likely panic or early return)".to_string(),
                duration_ms: self.start.elapsed().as_millis() as u64,
                error_code: Some("NIKA-098".to_string()),
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn guard_dropped_without_completion_emits_task_failed() {
        let log = EventLog::new();
        {
            let _guard = TaskEventGuard::start(
                log.clone(),
                Arc::from("test_task"),
                "infer",
                serde_json::json!({}),
            );
            // Guard is dropped here without calling .complete() or .fail()
        }
        let events = log.events();
        assert_eq!(
            events.len(),
            2,
            "Expected TaskStarted + TaskFailed from drop"
        );
        assert!(matches!(
            &events[0].kind,
            EventKind::TaskStarted { task_id, .. } if task_id.as_ref() == "test_task"
        ));
        assert!(matches!(
            &events[1].kind,
            EventKind::TaskFailed { task_id, error, error_code, .. }
            if task_id.as_ref() == "test_task"
            && error.contains("guard dropped without completion")
            && error_code.as_deref() == Some("NIKA-098")
        ));
    }

    #[test]
    fn guard_complete_emits_task_completed_no_drop_event() {
        let log = EventLog::new();
        let guard = TaskEventGuard::start(
            log.clone(),
            Arc::from("test_task"),
            "infer",
            serde_json::json!({}),
        );
        guard.complete(Arc::new(serde_json::json!("result")));
        let events = log.events();
        assert_eq!(events.len(), 2, "Expected TaskStarted + TaskCompleted");
        assert!(matches!(&events[0].kind, EventKind::TaskStarted { .. }));
        assert!(matches!(
            &events[1].kind,
            EventKind::TaskCompleted { task_id, .. }
            if task_id.as_ref() == "test_task"
        ));
    }

    #[test]
    fn guard_fail_emits_task_failed_no_drop_event() {
        let log = EventLog::new();
        let guard = TaskEventGuard::start(
            log.clone(),
            Arc::from("test_task"),
            "exec",
            serde_json::json!({}),
        );
        guard.fail("something broke", Some("NIKA-050".to_string()));
        let events = log.events();
        assert_eq!(events.len(), 2, "Expected TaskStarted + TaskFailed");
        assert!(matches!(&events[0].kind, EventKind::TaskStarted { .. }));
        assert!(matches!(
            &events[1].kind,
            EventKind::TaskFailed { error, error_code, .. }
            if error == "something broke"
            && error_code.as_deref() == Some("NIKA-050")
        ));
    }

    #[test]
    fn guard_elapsed_tracks_time() {
        let log = EventLog::new();
        let guard = TaskEventGuard::start(
            log.clone(),
            Arc::from("test_task"),
            "fetch",
            serde_json::json!({}),
        );
        std::thread::sleep(std::time::Duration::from_millis(10));
        assert!(guard.elapsed().as_millis() >= 10);
        guard.complete(Arc::new(serde_json::json!(null)));
    }
}
