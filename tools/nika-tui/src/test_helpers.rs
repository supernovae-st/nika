// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Shared test utilities for nika-tui
//!
//! Reduces boilerplate across 6+ test files (2148+ tests).
//! Provides constructors for TuiState and common EventKind variants.

use std::sync::Arc;

use nika_engine::event::{EventKind, FinishReason};

/// Standard test state constructor
pub fn test_state() -> crate::state::TuiState {
    crate::state::TuiState::new("test.nika.yaml")
}

/// Arc<str> shorthand for task IDs
pub fn tid(id: &str) -> Arc<str> {
    Arc::from(id)
}

/// Build TaskScheduled event
pub fn task_scheduled(id: &str) -> EventKind {
    EventKind::TaskScheduled {
        task_id: tid(id),
        dependencies: vec![],
    }
}

/// Build TaskStarted event
pub fn task_started(id: &str, verb: &str) -> EventKind {
    EventKind::TaskStarted {
        task_id: tid(id),
        verb: verb.into(),
        inputs: Arc::new(serde_json::json!({})),
    }
}

/// Build WorkflowStarted event
pub fn workflow_started(task_count: usize) -> EventKind {
    EventKind::WorkflowStarted {
        task_count,
        generation_id: "gen-test".to_string(),
        workflow_hash: "hash-test".to_string(),
        nika_version: env!("CARGO_PKG_VERSION").to_string(),
    }
}

/// Build McpInvoke event
pub fn mcp_invoke(task_id: &str, call_id: &str, tool: &str) -> EventKind {
    EventKind::McpInvoke {
        task_id: tid(task_id),
        call_id: call_id.to_string(),
        mcp_server: "test-server".to_string(),
        tool: Some(tool.to_string()),
        resource: None,
        params: None,
    }
}

/// Build ProviderResponded event
pub fn provider_responded(task_id: &str, input: u64, output: u64) -> EventKind {
    EventKind::ProviderResponded {
        task_id: tid(task_id),
        request_id: None,
        input_tokens: input,
        output_tokens: output,
        cache_read_tokens: 0,
        cost_usd: 0.0,
        ttft_ms: None,
        finish_reason: FinishReason::Stop,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_helpers_test_state_creates_valid_state() {
        let state = test_state();
        assert_eq!(state.workflow.task_count, 0);
    }

    #[test]
    fn test_helpers_tid_creates_arc_str() {
        let id = tid("my-task");
        assert_eq!(&*id, "my-task");
    }

    #[test]
    fn test_helpers_event_builders_produce_correct_variants() {
        assert!(matches!(
            task_scheduled("t1"),
            EventKind::TaskScheduled { .. }
        ));
        assert!(matches!(
            task_started("t1", "infer"),
            EventKind::TaskStarted { .. }
        ));
        assert!(matches!(
            workflow_started(3),
            EventKind::WorkflowStarted { .. }
        ));
        assert!(matches!(
            mcp_invoke("t1", "c1", "tool"),
            EventKind::McpInvoke { .. }
        ));
        assert!(matches!(
            provider_responded("t1", 100, 50),
            EventKind::ProviderResponded { .. }
        ));
    }
}
