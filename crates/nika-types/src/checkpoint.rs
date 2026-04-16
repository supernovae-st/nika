// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Agent checkpoint types — agent-v2 state persistence.
//!
//! Checkpoints capture the full state of an agent loop so it can be
//! resumed, inspected, or replayed. Used by agent-v2 for resume and
//! cost tracking.

use alloc::string::String;
use alloc::vec::Vec;

use serde::{Deserialize, Serialize};

use crate::cost::Cost;
use crate::id::TaskId;
use crate::role::Role;

/// A serializable snapshot of an agent loop's state.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[non_exhaustive]
pub struct AgentCheckpoint {
    /// Checkpoint format version.
    pub version: u32,
    /// Session identifier.
    pub session_id: String,
    /// Associated task (if any).
    pub task_id: Option<TaskId>,
    /// Conversation history.
    pub messages: Vec<CheckpointMessage>,
    /// Tool call records.
    pub tool_records: Vec<ToolCallRecord>,
    /// Number of completed turns.
    pub turn_count: u32,
    /// Total tokens consumed.
    pub total_tokens: u64,
    /// Accumulated cost in USD (f64).
    ///
    /// Prefer [`Self::cost`] (exact nano-USD) for billing; scheduled for
    /// removal in v0.85.
    #[deprecated(
        since = "0.81.0",
        note = "use `cost: Option<Cost>` instead; `cost_usd` will be removed in v0.85"
    )]
    pub cost_usd: Option<f64>,
    /// Accumulated cost as nano-USD `Cost`. Preferred over `cost_usd` for
    /// billing aggregation and ledger reconciliation.
    pub cost: Option<Cost>,
    /// Provider name.
    pub provider: String,
    /// Model identifier.
    pub model: String,
    /// Arbitrary context (for plugin state).
    pub context: serde_json::Value,
}

impl AgentCheckpoint {
    /// Create a new checkpoint.
    #[must_use]
    #[allow(deprecated)] // cost_usd initialized for bw-compat; removal in v0.85
    pub fn new(
        session_id: impl Into<String>,
        provider: impl Into<String>,
        model: impl Into<String>,
    ) -> Self {
        Self {
            version: 1,
            session_id: session_id.into(),
            task_id: None,
            messages: Vec::new(),
            tool_records: Vec::new(),
            turn_count: 0,
            total_tokens: 0,
            cost_usd: None,
            cost: None,
            provider: provider.into(),
            model: model.into(),
            context: serde_json::Value::Null,
        }
    }
}

/// A message in a checkpoint conversation history.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[non_exhaustive]
pub struct CheckpointMessage {
    /// Message role.
    pub role: Role,
    /// Message content (flattened to text for checkpoint storage).
    pub content: String,
}

impl CheckpointMessage {
    /// Create a new checkpoint message.
    #[must_use]
    pub fn new(role: Role, content: impl Into<String>) -> Self {
        Self {
            role,
            content: content.into(),
        }
    }
}

/// Record of a tool call within an agent loop.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[non_exhaustive]
pub struct ToolCallRecord {
    /// Tool name.
    pub name: String,
    /// Tool input (JSON).
    pub input: serde_json::Value,
    /// Tool output.
    pub output: String,
    /// Whether the call resulted in an error.
    pub is_error: bool,
    /// Execution duration in milliseconds.
    pub duration_ms: u64,
}

impl ToolCallRecord {
    /// Create a new tool call record.
    #[must_use]
    pub fn new(
        name: impl Into<String>,
        input: serde_json::Value,
        output: impl Into<String>,
        is_error: bool,
        duration_ms: u64,
    ) -> Self {
        Self {
            name: name.into(),
            input,
            output: output.into(),
            is_error,
            duration_ms,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[allow(deprecated)] // reads cost_usd to assert None before v0.85 removal
    fn agent_checkpoint_new_defaults() {
        let cp = AgentCheckpoint::new("sess-1", "anthropic", "claude-sonnet");
        assert_eq!(cp.version, 1);
        assert_eq!(cp.session_id, "sess-1");
        assert!(cp.task_id.is_none());
        assert!(cp.messages.is_empty());
        assert!(cp.tool_records.is_empty());
        assert_eq!(cp.turn_count, 0);
        assert_eq!(cp.total_tokens, 0);
        assert!(cp.cost_usd.is_none());
        assert!(cp.cost.is_none());
    }

    #[test]
    fn checkpoint_serde_roundtrip() {
        let cp = AgentCheckpoint::new("s1", "openai", "gpt-4o");
        let json = serde_json::to_string(&cp).expect("serialize");
        let back: AgentCheckpoint = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(back.session_id, "s1");
        assert_eq!(back.provider, "openai");
    }

    #[test]
    fn checkpoint_message_new() {
        let msg = CheckpointMessage::new(Role::User, "hello");
        assert_eq!(msg.role, Role::User);
        assert_eq!(msg.content, "hello");
    }

    #[test]
    fn tool_call_record_new() {
        let record = ToolCallRecord::new(
            "nika:read",
            serde_json::json!({"path": "/tmp/x"}),
            "file contents",
            false,
            42,
        );
        assert_eq!(record.name, "nika:read");
        assert!(!record.is_error);
        assert_eq!(record.duration_ms, 42);
    }

    #[test]
    fn tool_call_record_serde_roundtrip() {
        let record = ToolCallRecord::new("t", serde_json::json!(null), "out", true, 100);
        let json = serde_json::to_string(&record).expect("serialize");
        let back: ToolCallRecord = serde_json::from_str(&json).expect("deserialize");
        assert!(back.is_error);
        assert_eq!(back.duration_ms, 100);
    }

    fn _assert_send_sync<T: Send + Sync>() {}

    #[test]
    fn checkpoint_types_send_sync() {
        _assert_send_sync::<AgentCheckpoint>();
        _assert_send_sync::<CheckpointMessage>();
        _assert_send_sync::<ToolCallRecord>();
    }
}
