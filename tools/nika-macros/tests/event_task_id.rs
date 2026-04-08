// SPDX-License-Identifier: AGPL-3.0-or-later

//! Tests for #[derive(EventTaskId)]

use nika_macros::EventTaskId;
use std::sync::Arc;

#[derive(EventTaskId)]
enum TestEvent {
    #[has_task_id]
    TaskStarted { task_id: Arc<str>, verb: String },

    #[has_task_id]
    TaskCompleted { task_id: Arc<str>, output: u64 },

    // No #[has_task_id] → returns None
    WorkflowStarted { task_count: usize },

    // Unit variant → returns None
    WorkflowPaused,

    // Custom field name
    #[has_task_id(field = "parent_task_id")]
    AgentSpawned {
        parent_task_id: Arc<str>,
        child_task_id: Arc<str>,
        depth: u32,
    },

    // Optional task_id
    #[has_task_id(optional)]
    Log {
        level: String,
        message: String,
        task_id: Option<Arc<str>>,
    },
}

#[test]
fn task_id_some_for_tagged_variant() {
    let e = TestEvent::TaskStarted {
        task_id: Arc::from("t1"),
        verb: "infer".into(),
    };
    assert_eq!(e.task_id(), Some("t1"));
}

#[test]
fn task_id_some_for_completed() {
    let e = TestEvent::TaskCompleted {
        task_id: Arc::from("t2"),
        output: 42,
    };
    assert_eq!(e.task_id(), Some("t2"));
}

#[test]
fn task_id_none_for_workflow_event() {
    let e = TestEvent::WorkflowStarted { task_count: 5 };
    assert_eq!(e.task_id(), None);
}

#[test]
fn task_id_none_for_unit_variant() {
    let e = TestEvent::WorkflowPaused;
    assert_eq!(e.task_id(), None);
}

#[test]
fn task_id_custom_field_name() {
    let e = TestEvent::AgentSpawned {
        parent_task_id: Arc::from("parent"),
        child_task_id: Arc::from("child"),
        depth: 1,
    };
    assert_eq!(e.task_id(), Some("parent"));
}

#[test]
fn task_id_optional_some() {
    let e = TestEvent::Log {
        level: "info".into(),
        message: "hello".into(),
        task_id: Some(Arc::from("t3")),
    };
    assert_eq!(e.task_id(), Some("t3"));
}

#[test]
fn task_id_optional_none() {
    let e = TestEvent::Log {
        level: "info".into(),
        message: "hello".into(),
        task_id: None,
    };
    assert_eq!(e.task_id(), None);
}
