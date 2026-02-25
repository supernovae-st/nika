//! ChatWorkflow - DAG wrapper for chat conversations (v0.9.1)
//!
//! Turns every chat message into a traceable DAG node with stable NodeIndex.
//! Foundation for @mention references where `@N` references stay valid after deletion.
//!
//! # Architecture
//!
//! ```text
//! ChatWorkflow {
//!     dag: StableFlowGraph<ChatMessage>,
//!     message_counter: u32,
//!     id_to_index: HashMap<String, NodeIndex>,
//! }
//!
//! Sequential Flow:
//! [msg-001] ──► [msg-002] ──► [msg-003] ──► [msg-004]
//!    User        Assistant      User        Assistant
//! ```

use crate::dag::StableFlowGraph;
use chrono::{DateTime, Utc};
use petgraph::stable_graph::NodeIndex;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Role of a chat message participant.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Role {
    User,
    Assistant,
    System,
    Tool,
}

/// A chat message as a DAG node.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatMessage {
    pub id: String,
    pub content: String,
    pub role: Role,
    pub timestamp: DateTime<Utc>,
}

/// DAG wrapper for chat conversation.
///
/// Every message becomes a node in the DAG with a stable NodeIndex.
/// Sequential edges are auto-created for linear conversation flow.
pub struct ChatWorkflow {
    /// The underlying DAG with stable node indices
    pub dag: StableFlowGraph<ChatMessage>,
    /// Counter for generating sequential message IDs
    message_counter: u32,
    /// Map from message ID to NodeIndex for fast lookup
    id_to_index: HashMap<String, NodeIndex>,
}

impl ChatWorkflow {
    /// Create a new empty ChatWorkflow.
    pub fn new() -> Self {
        Self {
            dag: StableFlowGraph::new(),
            message_counter: 0,
            id_to_index: HashMap::new(),
        }
    }

    /// Get the number of messages in the conversation.
    pub fn message_count(&self) -> usize {
        self.dag.node_count()
    }
}

impl Default for ChatWorkflow {
    fn default() -> Self {
        Self::new()
    }
}

// ═══════════════════════════════════════════════════════════════════════════════
// Compile-time assertion: ChatWorkflow must be Send + Sync for async usage
// ═══════════════════════════════════════════════════════════════════════════════
const _: () = {
    const fn assert_send_sync<T: Send + Sync>() {}
    assert_send_sync::<ChatWorkflow>();
};

#[cfg(test)]
mod tests {
    use super::*;

    // ═══════════════════════════════════════════════════════════════════════════
    // Task 1.1: Basic construction tests
    // ═══════════════════════════════════════════════════════════════════════════

    #[test]
    fn test_chat_workflow_new_creates_empty_dag() {
        let workflow = ChatWorkflow::new();

        assert_eq!(workflow.message_count(), 0);
        assert_eq!(workflow.dag.node_count(), 0);
    }

    #[test]
    fn test_chat_workflow_default() {
        let workflow = ChatWorkflow::default();
        assert_eq!(workflow.message_count(), 0);
    }

    #[test]
    fn test_chat_workflow_send_sync() {
        fn assert_send<T: Send>() {}
        fn assert_sync<T: Sync>() {}

        // ChatWorkflow should be Send + Sync for async usage
        assert_send::<ChatWorkflow>();
        assert_sync::<ChatWorkflow>();
    }

    // ═══════════════════════════════════════════════════════════════════════════
    // Role enum tests
    // ═══════════════════════════════════════════════════════════════════════════

    #[test]
    fn test_all_role_variants() {
        let roles = [Role::User, Role::Assistant, Role::System, Role::Tool];
        assert_eq!(roles.len(), 4);
    }

    #[test]
    fn test_role_equality() {
        assert_eq!(Role::User, Role::User);
        assert_ne!(Role::User, Role::Assistant);
    }

    #[test]
    fn test_role_clone() {
        let role = Role::Assistant;
        let cloned = role;
        assert_eq!(role, cloned);
    }

    #[test]
    fn test_role_serialization() {
        let role = Role::User;
        let json = serde_json::to_string(&role).unwrap();
        let restored: Role = serde_json::from_str(&json).unwrap();
        assert_eq!(role, restored);
    }
}
