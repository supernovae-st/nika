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

    // ═══════════════════════════════════════════════════════════════════════════
    // Task 1.2: add_message() → Node Creation
    // ═══════════════════════════════════════════════════════════════════════════

    /// Add a message to the conversation DAG.
    /// Returns the stable NodeIndex of the new message.
    ///
    /// Automatically creates a sequential edge from the previous message
    /// to maintain linear conversation flow.
    pub fn add_message(&mut self, content: &str, role: Role) -> NodeIndex {
        self.message_counter += 1;
        let id = format!("msg-{:03}", self.message_counter);

        let message = ChatMessage {
            id: id.clone(),
            content: content.to_string(),
            role,
            timestamp: Utc::now(),
        };

        let idx = self.dag.add_node(message);
        self.id_to_index.insert(id, idx);

        // Auto-create edge from previous message (sequential flow)
        // First message (counter=1) has no previous, so skip
        if self.message_counter > 1 {
            if let Some(prev_idx) = self.get_index_by_number(self.message_counter - 1) {
                self.dag.add_edge(prev_idx, idx);
            }
        }

        idx
    }

    /// Get a message by its ID (e.g., "msg-001").
    pub fn get_message_by_id(&self, id: &str) -> Option<&ChatMessage> {
        self.id_to_index
            .get(id)
            .and_then(|idx| self.dag.node_weight(*idx))
    }

    // ═══════════════════════════════════════════════════════════════════════════
    // Task 1.3: get_message_by_index() and get_message_by_number()
    // ═══════════════════════════════════════════════════════════════════════════

    /// Get a message by its NodeIndex.
    pub fn get_message_by_index(&self, idx: NodeIndex) -> Option<&ChatMessage> {
        self.dag.node_weight(idx)
    }

    /// Get a message by its number (for @N references).
    /// @1 returns the first message (msg-001).
    pub fn get_message_by_number(&self, n: u32) -> Option<&ChatMessage> {
        let id = format!("msg-{:03}", n);
        self.get_message_by_id(&id)
    }

    /// Get the NodeIndex by message number.
    pub fn get_index_by_number(&self, n: u32) -> Option<NodeIndex> {
        let id = format!("msg-{:03}", n);
        self.id_to_index.get(&id).copied()
    }

    // ═══════════════════════════════════════════════════════════════════════════
    // Task 1.5: Message Counter for @N References
    // ═══════════════════════════════════════════════════════════════════════════

    /// Get the current message number (for @N references).
    /// Returns 0 if no messages have been added.
    pub fn current_message_number(&self) -> u32 {
        self.message_counter
    }

    /// Get the most recent message (last added).
    pub fn last_message(&self) -> Option<&ChatMessage> {
        if self.message_counter == 0 {
            return None;
        }
        self.get_message_by_number(self.message_counter)
    }

    /// Get the NodeIndex of the most recent message.
    pub fn last_message_index(&self) -> Option<NodeIndex> {
        if self.message_counter == 0 {
            return None;
        }
        self.get_index_by_number(self.message_counter)
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

    // ═══════════════════════════════════════════════════════════════════════════
    // Task 1.2: add_message() tests
    // ═══════════════════════════════════════════════════════════════════════════

    #[test]
    fn test_add_message_creates_node() {
        let mut workflow = ChatWorkflow::new();

        let idx = workflow.add_message("Hello!", Role::User);

        assert_eq!(workflow.message_count(), 1);
        assert!(idx.index() >= 0);
    }

    #[test]
    fn test_add_message_generates_sequential_ids() {
        let mut workflow = ChatWorkflow::new();

        workflow.add_message("First", Role::User);
        workflow.add_message("Second", Role::Assistant);
        workflow.add_message("Third", Role::User);

        let msg1 = workflow.get_message_by_id("msg-001");
        let msg2 = workflow.get_message_by_id("msg-002");
        let msg3 = workflow.get_message_by_id("msg-003");

        assert!(msg1.is_some());
        assert!(msg2.is_some());
        assert!(msg3.is_some());

        assert_eq!(msg1.unwrap().content, "First");
        assert_eq!(msg2.unwrap().content, "Second");
        assert_eq!(msg3.unwrap().content, "Third");
    }

    #[test]
    fn test_add_message_assigns_correct_role() {
        let mut workflow = ChatWorkflow::new();

        workflow.add_message("User msg", Role::User);
        workflow.add_message("Assistant msg", Role::Assistant);
        workflow.add_message("System msg", Role::System);
        workflow.add_message("Tool msg", Role::Tool);

        assert_eq!(workflow.get_message_by_id("msg-001").unwrap().role, Role::User);
        assert_eq!(workflow.get_message_by_id("msg-002").unwrap().role, Role::Assistant);
        assert_eq!(workflow.get_message_by_id("msg-003").unwrap().role, Role::System);
        assert_eq!(workflow.get_message_by_id("msg-004").unwrap().role, Role::Tool);
    }

    #[test]
    fn test_get_message_by_id_nonexistent() {
        let workflow = ChatWorkflow::new();
        assert!(workflow.get_message_by_id("msg-999").is_none());
    }

    // ═══════════════════════════════════════════════════════════════════════════
    // Task 1.3: get_message_by_index() and get_message_by_number() tests
    // ═══════════════════════════════════════════════════════════════════════════

    #[test]
    fn test_get_message_by_index() {
        let mut workflow = ChatWorkflow::new();

        let idx = workflow.add_message("Test message", Role::User);
        let msg = workflow.get_message_by_index(idx);

        assert!(msg.is_some());
        assert_eq!(msg.unwrap().content, "Test message");
    }

    #[test]
    fn test_get_message_by_number() {
        let mut workflow = ChatWorkflow::new();

        workflow.add_message("First", Role::User);
        workflow.add_message("Second", Role::Assistant);

        // @1 → msg-001, @2 → msg-002
        let msg1 = workflow.get_message_by_number(1);
        let msg2 = workflow.get_message_by_number(2);
        let msg3 = workflow.get_message_by_number(3);

        assert_eq!(msg1.unwrap().content, "First");
        assert_eq!(msg2.unwrap().content, "Second");
        assert!(msg3.is_none());
    }

    #[test]
    fn test_get_index_by_number() {
        let mut workflow = ChatWorkflow::new();

        let idx1 = workflow.add_message("First", Role::User);
        let idx2 = workflow.add_message("Second", Role::Assistant);

        assert_eq!(workflow.get_index_by_number(1), Some(idx1));
        assert_eq!(workflow.get_index_by_number(2), Some(idx2));
        assert_eq!(workflow.get_index_by_number(3), None);
    }

    #[test]
    fn test_get_message_by_invalid_index() {
        let workflow = ChatWorkflow::new();
        let invalid_idx = NodeIndex::new(999);
        assert!(workflow.get_message_by_index(invalid_idx).is_none());
    }

    // ═══════════════════════════════════════════════════════════════════════════
    // Task 1.4: Auto-Edge Creation tests
    // ═══════════════════════════════════════════════════════════════════════════

    #[test]
    fn test_auto_edge_sequential_messages() {
        let mut workflow = ChatWorkflow::new();

        let idx1 = workflow.add_message("First", Role::User);
        let idx2 = workflow.add_message("Second", Role::Assistant);
        let idx3 = workflow.add_message("Third", Role::User);

        // Sequential edges: 1 → 2 → 3
        assert!(workflow.dag.has_edge(idx1, idx2), "Should have edge 1 → 2");
        assert!(workflow.dag.has_edge(idx2, idx3), "Should have edge 2 → 3");

        // No reverse edges
        assert!(!workflow.dag.has_edge(idx2, idx1), "Should NOT have edge 2 → 1");
        assert!(!workflow.dag.has_edge(idx3, idx2), "Should NOT have edge 3 → 2");
    }

    #[test]
    fn test_first_message_has_no_incoming_edge() {
        let mut workflow = ChatWorkflow::new();

        let idx1 = workflow.add_message("First message", Role::User);

        // First message should have no incoming edges
        assert_eq!(workflow.dag.edge_count(), 0, "First message should have no edges");

        // Add second message - now we should have exactly one edge
        let _idx2 = workflow.add_message("Second message", Role::Assistant);
        assert_eq!(workflow.dag.edge_count(), 1, "Should have exactly 1 edge after 2 messages");

        // The first message still has no incoming edge (it's the source)
        assert!(workflow.dag.node_weight(idx1).is_some());
    }

    // ═══════════════════════════════════════════════════════════════════════════
    // Task 1.5: Message Counter for @N References tests
    // ═══════════════════════════════════════════════════════════════════════════

    #[test]
    fn test_current_message_number_empty() {
        let workflow = ChatWorkflow::new();
        assert_eq!(workflow.current_message_number(), 0);
    }

    #[test]
    fn test_current_message_number_increments() {
        let mut workflow = ChatWorkflow::new();

        workflow.add_message("First", Role::User);
        assert_eq!(workflow.current_message_number(), 1);

        workflow.add_message("Second", Role::Assistant);
        assert_eq!(workflow.current_message_number(), 2);

        workflow.add_message("Third", Role::User);
        assert_eq!(workflow.current_message_number(), 3);
    }

    #[test]
    fn test_last_message_empty() {
        let workflow = ChatWorkflow::new();
        assert!(workflow.last_message().is_none());
    }

    #[test]
    fn test_last_message_returns_most_recent() {
        let mut workflow = ChatWorkflow::new();

        workflow.add_message("First", Role::User);
        assert_eq!(workflow.last_message().unwrap().content, "First");

        workflow.add_message("Second", Role::Assistant);
        assert_eq!(workflow.last_message().unwrap().content, "Second");

        workflow.add_message("Third", Role::User);
        assert_eq!(workflow.last_message().unwrap().content, "Third");
    }

    #[test]
    fn test_last_message_index() {
        let mut workflow = ChatWorkflow::new();

        assert!(workflow.last_message_index().is_none());

        let idx1 = workflow.add_message("First", Role::User);
        assert_eq!(workflow.last_message_index(), Some(idx1));

        let idx2 = workflow.add_message("Second", Role::Assistant);
        assert_eq!(workflow.last_message_index(), Some(idx2));
    }
}
