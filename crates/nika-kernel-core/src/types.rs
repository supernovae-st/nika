// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Shared cross-boundary types used across kernel subsystems.

// TaskId descended to nika-error/id.rs (Phase 0).
pub use nika_error::id::TaskId;

/// Metadata about a workflow (name, version, description).
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct WorkflowMeta {
    /// Workflow name.
    pub name: String,
    /// Optional version string.
    pub version: Option<String>,
    /// Optional human-readable description.
    pub description: Option<String>,
}

impl WorkflowMeta {
    /// Create a new workflow metadata with required name.
    #[must_use]
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            version: None,
            description: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn task_id_display() {
        let id = TaskId::new("research_step");
        assert_eq!(id.to_string(), "research_step");
    }

    #[test]
    fn task_id_equality() {
        let a = TaskId::new("t1");
        let b = TaskId::new("t1");
        assert_eq!(a, b);
    }

    #[test]
    fn task_id_serde_roundtrip() {
        let id = TaskId::new("abc-123");
        let json = serde_json::to_string(&id).expect("serialize");
        let back: TaskId = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(id, back);
    }

    #[test]
    fn workflow_meta_new_defaults() {
        let meta = WorkflowMeta::new("my-wf");
        assert_eq!(meta.name, "my-wf");
        assert!(meta.version.is_none());
        assert!(meta.description.is_none());
    }

    fn _assert_send_sync<T: Send + Sync>() {}

    #[test]
    fn types_are_send_sync() {
        _assert_send_sync::<TaskId>();
        _assert_send_sync::<WorkflowMeta>();
    }
}
