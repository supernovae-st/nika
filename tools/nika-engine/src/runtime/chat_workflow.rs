// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! ChatWorkflow - DAG wrapper for chat conversations
//!
//! Turns every chat message into a traceable DAG node with stable NodeIndex.
//! Foundation for @mention references where `@N` references stay valid after deletion.
//!
//! # Architecture
//!
//! ```text
//! ChatWorkflow {
//!     dag: StableDag<ChatMessage>,
//!     message_counter: u32,
//!     id_to_index: HashMap<String, NodeIndex>,
//! }
//!
//! Sequential Flow:
//! [msg-001] ──► [msg-002] ──► [msg-003] ──► [msg-004]
//!    User        Assistant      User        Assistant
//! ```

use crate::binding::{
    has_parallel_marker, parse_mentions, resolve_mention, strip_parallel_marker, text_to_bindings,
    BindingSpec, Mention, MentionResolutionError, ResolvedMention,
};
use crate::dag::StableDag;
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
    pub dag: StableDag<ChatMessage>,
    /// Counter for generating sequential message IDs
    message_counter: u32,
    /// Map from message ID to NodeIndex for fast lookup
    id_to_index: HashMap<String, NodeIndex>,
}

impl ChatWorkflow {
    /// Create a new empty ChatWorkflow.
    pub fn new() -> Self {
        Self {
            dag: StableDag::new(),
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

    // ═══════════════════════════════════════════════════════════════════════════
    // Task 2.9: @mention Integration
    // ═══════════════════════════════════════════════════════════════════════════

    /// Add a message that is parallel (independent of previous message).
    ///
    /// Unlike `add_message()`, this does NOT create an auto-edge from the
    /// previous message. Used for messages starting with `//` parallel marker.
    pub fn add_message_parallel(&mut self, content: &str, role: Role) -> NodeIndex {
        self.message_counter += 1;
        let id = format!("msg-{:03}", self.message_counter);

        // Strip the parallel marker from content if present
        let clean_content = strip_parallel_marker(content);

        let message = ChatMessage {
            id: id.clone(),
            content: clean_content.to_string(),
            role,
            timestamp: Utc::now(),
        };

        let idx = self.dag.add_node(message);
        self.id_to_index.insert(id, idx);

        // NO auto-edge for parallel messages
        idx
    }

    /// Add a message, automatically detecting if it's parallel or sequential.
    ///
    /// Messages starting with `//` are parallel (no edge from previous).
    /// All other messages get the standard sequential edge.
    pub fn add_message_auto(&mut self, content: &str, role: Role) -> NodeIndex {
        if has_parallel_marker(content) {
            self.add_message_parallel(content, role)
        } else {
            self.add_message(content, role)
        }
    }

    /// Parse @mentions from a message's content.
    ///
    /// Returns all mentions found in the message content.
    pub fn get_mentions(&self, idx: NodeIndex) -> Vec<Mention> {
        match self.get_message_by_index(idx) {
            Some(msg) => parse_mentions(&msg.content),
            None => vec![],
        }
    }

    /// Resolve a mention using this conversation's message count.
    pub fn resolve_mention(
        &self,
        mention: &Mention,
    ) -> Result<ResolvedMention, MentionResolutionError> {
        resolve_mention(mention, self.message_counter)
    }

    /// Get BindingSpec for all mentions in a message.
    ///
    /// Parses mentions from the message content and converts them to
    /// BindingSpec bindings that can be used for DAG edges.
    pub fn get_bindings_for_message(
        &self,
        idx: NodeIndex,
    ) -> Result<BindingSpec, MentionResolutionError> {
        match self.get_message_by_index(idx) {
            Some(msg) => text_to_bindings(&msg.content, self.message_counter),
            None => Ok(BindingSpec::default()),
        }
    }

    /// Check if a message has the parallel marker.
    ///
    /// Returns true if the message content starts with `//`.
    pub fn is_parallel_message(&self, idx: NodeIndex) -> bool {
        match self.get_message_by_index(idx) {
            Some(msg) => has_parallel_marker(&msg.content),
            None => false,
        }
    }

    // ═══════════════════════════════════════════════════════════════════════════
    // Task 2.10: Edge Creation from @mentions
    // ═══════════════════════════════════════════════════════════════════════════

    /// Add edges from mentioned messages to the target message.
    ///
    /// For each @mention in the target message's content, creates an edge
    /// from the mentioned message(s) to the target. This forms the DAG
    /// dependency graph based on explicit references.
    ///
    /// Returns the number of edges added.
    pub fn add_edges_from_mentions(
        &mut self,
        target_idx: NodeIndex,
    ) -> Result<usize, MentionResolutionError> {
        // Get the target message's content
        let content = match self.get_message_by_index(target_idx) {
            Some(msg) => msg.content.clone(),
            None => return Ok(0),
        };

        // Parse mentions from content
        let mentions = parse_mentions(&content);
        let mut edges_added = 0;

        for mention in mentions {
            // Resolve the mention to message indices
            let resolved = resolve_mention(&mention, self.message_counter)?;

            match resolved {
                ResolvedMention::Single(n) => {
                    if let Some(source_idx) = self.get_index_by_number(n) {
                        // Don't add self-loops
                        if source_idx != target_idx {
                            self.dag.add_edge(source_idx, target_idx);
                            edges_added += 1;
                        }
                    }
                }
                ResolvedMention::Multiple(indices) => {
                    for n in indices {
                        if let Some(source_idx) = self.get_index_by_number(n) {
                            // Don't add self-loops
                            if source_idx != target_idx {
                                self.dag.add_edge(source_idx, target_idx);
                                edges_added += 1;
                            }
                        }
                    }
                }
                ResolvedMention::Empty => {}
            }
        }

        Ok(edges_added)
    }

    /// Add a message and create edges from any @mentions in its content.
    ///
    /// This combines `add_message_auto()` with `add_edges_from_mentions()`:
    /// 1. Adds the message (parallel if `//`, sequential otherwise)
    /// 2. Creates edges from all @mentioned messages
    ///
    /// Returns the NodeIndex of the new message.
    pub fn add_message_with_mentions(
        &mut self,
        content: &str,
        role: Role,
    ) -> Result<NodeIndex, MentionResolutionError> {
        let idx = self.add_message_auto(content, role);
        self.add_edges_from_mentions(idx)?;
        Ok(idx)
    }

    /// Get all incoming edges (dependencies) for a message.
    ///
    /// Returns the NodeIndices of messages that this message depends on.
    pub fn get_dependencies(&self, idx: NodeIndex) -> Vec<NodeIndex> {
        self.dag.incoming_neighbors(idx).collect()
    }

    /// Get all outgoing edges (dependents) for a message.
    ///
    /// Returns the NodeIndices of messages that depend on this message.
    pub fn get_dependents(&self, idx: NodeIndex) -> Vec<NodeIndex> {
        self.dag.outgoing_neighbors(idx).collect()
    }

    // ═══════════════════════════════════════════════════════════════════════════
    // Iteration support for TUI sync
    // ═══════════════════════════════════════════════════════════════════════════

    /// Iterate over all messages in the workflow.
    ///
    /// Returns messages in insertion order (by message number).
    pub fn all_messages(&self) -> Vec<(NodeIndex, &ChatMessage)> {
        (1..=self.message_counter)
            .filter_map(|n| {
                let idx = self.get_index_by_number(n)?;
                let msg = self.get_message_by_index(idx)?;
                Some((idx, msg))
            })
            .collect()
    }

    /// Get the total message count (for iteration bounds).
    pub fn total_messages(&self) -> u32 {
        self.message_counter
    }

    // ═══════════════════════════════════════════════════════════════════════════
    // YAML Export for /export yaml command
    // ═══════════════════════════════════════════════════════════════════════════

    /// Export the chat conversation to Nika workflow YAML.
    ///
    /// Converts User messages to `infer:` tasks, preserving @N dependencies
    /// as `depends_on:` entries. Assistant/System/Tool messages are skipped.
    ///
    /// # Example Output
    ///
    /// ```yaml
    /// schema: "nika/workflow@0.12"
    /// provider: claude
    ///
    /// tasks:
    ///   - id: msg-002
    ///     infer: "What is Rust?"
    ///
    ///   - id: msg-004
    ///     depends_on: [msg-002]
    ///     infer: "@2 Tell me more about safety"
    /// ```
    pub fn to_yaml(&self) -> String {
        let mut yaml = String::new();

        // Header
        yaml.push_str("# Auto-generated from Nika Chat session\n");
        yaml.push_str(&format!(
            "# Generated: {}\n",
            Utc::now().format("%Y-%m-%d %H:%M:%S UTC")
        ));
        yaml.push_str("# Use `nika run <file>` to execute\n\n");

        yaml.push_str("schema: \"nika/workflow@0.12\"\n");
        yaml.push_str("provider: claude\n\n");

        // Collect tasks with their dependencies
        // (id, content, deps: Vec<String>)
        let mut tasks: Vec<(&str, &str, Vec<String>)> = Vec::new();

        for (idx, msg) in self.all_messages() {
            match msg.role {
                Role::User => {
                    // Collect @N dependencies (only User→User edges)
                    let mut deps = Vec::new();
                    for dep_idx in self.get_dependencies(idx) {
                        if let Some(dep_msg) = self.get_message_by_index(dep_idx) {
                            if dep_msg.role == Role::User {
                                deps.push(dep_msg.id.clone());
                            }
                        }
                    }
                    tasks.push((&msg.id, &msg.content, deps));
                }
                Role::Assistant | Role::System | Role::Tool => {
                    // Non-user messages are skipped in workflow export
                }
            }
        }

        // Tasks section
        if tasks.is_empty() {
            yaml.push_str("# No user messages to convert to tasks\n");
            yaml.push_str("tasks: []\n");
        } else {
            yaml.push_str("tasks:\n");
            for (id, content, deps) in &tasks {
                let escaped = escape_yaml_string(content);
                yaml.push_str(&format!("  - id: \"{}\"\n", id));
                if !deps.is_empty() {
                    let dep_list: Vec<String> = deps.iter().map(|d| format!("\"{}\"", d)).collect();
                    yaml.push_str(&format!("    depends_on: [{}]\n", dep_list.join(", ")));
                }
                yaml.push_str(&format!("    infer: \"{}\"\n\n", escaped));
            }
        }

        yaml
    }
}

/// Escape a string for YAML output (double-quoted style).
fn escape_yaml_string(s: &str) -> String {
    s.replace('\\', "\\\\")
        .replace('"', "\\\"")
        .replace('\n', "\\n")
        .replace('\r', "\\r")
        .replace('\t', "\\t")
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
        assert_eq!(idx.index(), 0); // First message gets index 0
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

        assert_eq!(
            workflow.get_message_by_id("msg-001").unwrap().role,
            Role::User
        );
        assert_eq!(
            workflow.get_message_by_id("msg-002").unwrap().role,
            Role::Assistant
        );
        assert_eq!(
            workflow.get_message_by_id("msg-003").unwrap().role,
            Role::System
        );
        assert_eq!(
            workflow.get_message_by_id("msg-004").unwrap().role,
            Role::Tool
        );
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
        assert!(
            !workflow.dag.has_edge(idx2, idx1),
            "Should NOT have edge 2 → 1"
        );
        assert!(
            !workflow.dag.has_edge(idx3, idx2),
            "Should NOT have edge 3 → 2"
        );
    }

    #[test]
    fn test_first_message_has_no_incoming_edge() {
        let mut workflow = ChatWorkflow::new();

        let idx1 = workflow.add_message("First message", Role::User);

        // First message should have no incoming edges
        assert_eq!(
            workflow.dag.edge_count(),
            0,
            "First message should have no edges"
        );

        // Add second message - now we should have exactly one edge
        let _idx2 = workflow.add_message("Second message", Role::Assistant);
        assert_eq!(
            workflow.dag.edge_count(),
            1,
            "Should have exactly 1 edge after 2 messages"
        );

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

    // ═══════════════════════════════════════════════════════════════════════════
    // Task 1.6: Thread-Safety with parking_lot::Mutex tests
    // ═══════════════════════════════════════════════════════════════════════════

    #[test]
    fn test_concurrent_access_with_mutex() {
        use parking_lot::Mutex;
        use std::sync::Arc;
        use std::thread;

        let workflow = Arc::new(Mutex::new(ChatWorkflow::new()));
        let mut handles = vec![];

        // Spawn 4 threads, each adding messages
        for i in 0..4 {
            let wf = Arc::clone(&workflow);
            handles.push(thread::spawn(move || {
                for j in 0..5 {
                    let mut guard = wf.lock();
                    let role = if (i + j) % 2 == 0 {
                        Role::User
                    } else {
                        Role::Assistant
                    };
                    guard.add_message(&format!("Thread {} msg {}", i, j), role);
                }
            }));
        }

        // Wait for all threads
        for h in handles {
            h.join().unwrap();
        }

        // Verify total messages: 4 threads × 5 messages = 20
        let guard = workflow.lock();
        assert_eq!(guard.message_count(), 20);
        assert_eq!(guard.current_message_number(), 20);

        // Verify sequential edges exist (linear flow maintained)
        // Due to mutex, messages are added sequentially, so edges are 1→2, 2→3, etc.
        assert_eq!(guard.dag.edge_count(), 19); // 20 nodes, 19 edges
    }

    #[test]
    fn test_send_sync_bounds_with_arc() {
        use std::sync::Arc;

        // ChatWorkflow can be wrapped in Arc for sharing across threads
        fn assert_send_sync<T: Send + Sync>() {}
        assert_send_sync::<Arc<parking_lot::Mutex<ChatWorkflow>>>();

        // Create and share
        let workflow = Arc::new(parking_lot::Mutex::new(ChatWorkflow::new()));
        let _cloned = Arc::clone(&workflow);
    }

    // ═══════════════════════════════════════════════════════════════════════════
    // Task 2.9: @mention Integration Tests
    // ═══════════════════════════════════════════════════════════════════════════

    #[test]
    fn test_add_message_parallel_no_auto_edge() {
        let mut workflow = ChatWorkflow::new();

        workflow.add_message("First", Role::User);
        workflow.add_message_parallel("// Independent task", Role::User);

        // 2 messages, but only 0 edges (parallel message has no edge)
        assert_eq!(workflow.message_count(), 2);
        assert_eq!(workflow.dag.edge_count(), 0);
    }

    #[test]
    fn test_add_message_parallel_strips_marker() {
        let mut workflow = ChatWorkflow::new();

        let idx = workflow.add_message_parallel("// Independent task", Role::User);

        // Content should have marker stripped
        let msg = workflow.get_message_by_index(idx).unwrap();
        assert_eq!(msg.content, "Independent task");
    }

    #[test]
    fn test_add_message_auto_detects_parallel() {
        let mut workflow = ChatWorkflow::new();

        workflow.add_message_auto("First", Role::User);
        workflow.add_message_auto("Second", Role::Assistant);
        workflow.add_message_auto("// Parallel", Role::User);

        // 3 messages: 2 sequential (1 edge), 1 parallel (0 edge)
        assert_eq!(workflow.message_count(), 3);
        assert_eq!(workflow.dag.edge_count(), 1); // Only 1→2 edge
    }

    #[test]
    fn test_add_message_auto_sequential() {
        let mut workflow = ChatWorkflow::new();

        workflow.add_message_auto("First", Role::User);
        workflow.add_message_auto("Second", Role::Assistant);

        // 2 messages with 1 edge (sequential)
        assert_eq!(workflow.message_count(), 2);
        assert_eq!(workflow.dag.edge_count(), 1);
    }

    #[test]
    fn test_get_mentions_parses_content() {
        let mut workflow = ChatWorkflow::new();

        workflow.add_message("First message", Role::User);
        workflow.add_message("Second message", Role::Assistant);
        let idx = workflow.add_message("Based on @1 and @2", Role::User);

        let mentions = workflow.get_mentions(idx);
        assert_eq!(mentions.len(), 2);
        assert_eq!(mentions[0], Mention::Number(1));
        assert_eq!(mentions[1], Mention::Number(2));
    }

    #[test]
    fn test_get_mentions_empty_for_no_mentions() {
        let mut workflow = ChatWorkflow::new();

        let idx = workflow.add_message("No mentions here", Role::User);

        let mentions = workflow.get_mentions(idx);
        assert!(mentions.is_empty());
    }

    #[test]
    fn test_resolve_mention_uses_message_count() {
        let mut workflow = ChatWorkflow::new();

        workflow.add_message("First", Role::User);
        workflow.add_message("Second", Role::Assistant);
        workflow.add_message("Third", Role::User);

        // @last should resolve to 3
        let result = workflow.resolve_mention(&Mention::Last);
        assert_eq!(result, Ok(ResolvedMention::Single(3)));

        // @all should resolve to [1, 2, 3]
        let result = workflow.resolve_mention(&Mention::All);
        assert_eq!(result, Ok(ResolvedMention::Multiple(vec![1, 2, 3])));
    }

    #[test]
    fn test_get_bindings_for_message() {
        let mut workflow = ChatWorkflow::new();

        workflow.add_message("First", Role::User);
        workflow.add_message("Second", Role::Assistant);
        let idx = workflow.add_message("Based on @1", Role::User);

        let spec = workflow.get_bindings_for_message(idx).unwrap();
        assert_eq!(spec.len(), 1);
        assert!(spec.contains_key("ref_1"));
        assert_eq!(spec["ref_1"].path, "msg-001.output");
    }

    #[test]
    fn test_get_bindings_for_message_with_last() {
        let mut workflow = ChatWorkflow::new();

        workflow.add_message("First", Role::User);
        workflow.add_message("Second", Role::Assistant);
        let idx = workflow.add_message("Continue from @last", Role::User);

        // @last resolves to current message count (3) at resolution time
        let spec = workflow.get_bindings_for_message(idx).unwrap();
        assert_eq!(spec.len(), 1);
        assert!(spec.contains_key("ref_3")); // @last = message 3 (current count)
    }

    #[test]
    fn test_is_parallel_message() {
        let mut workflow = ChatWorkflow::new();

        let idx1 = workflow.add_message("Normal", Role::User);
        let idx2 = workflow.add_message("// Parallel", Role::User);

        assert!(!workflow.is_parallel_message(idx1));
        assert!(workflow.is_parallel_message(idx2));
    }

    // ═══════════════════════════════════════════════════════════════════════════
    // Task 2.10: Edge Creation from @mentions Tests
    // ═══════════════════════════════════════════════════════════════════════════

    #[test]
    fn test_add_edges_from_mentions_single() {
        let mut workflow = ChatWorkflow::new();

        workflow.add_message("First", Role::User);
        workflow.add_message("Second", Role::Assistant);
        let idx3 = workflow.add_message("Based on @1", Role::User);

        let edges_added = workflow.add_edges_from_mentions(idx3).unwrap();
        assert_eq!(edges_added, 1);

        // Should have edge from msg-001 to msg-003
        let deps = workflow.get_dependencies(idx3);
        assert_eq!(deps.len(), 2); // 1 from auto-edge + 1 from mention
    }

    #[test]
    fn test_add_edges_from_mentions_multiple() {
        let mut workflow = ChatWorkflow::new();

        workflow.add_message("First", Role::User);
        workflow.add_message("Second", Role::Assistant);
        let idx3 = workflow.add_message("Based on @1 and @2", Role::User);

        let edges_added = workflow.add_edges_from_mentions(idx3).unwrap();
        assert_eq!(edges_added, 2);
    }

    #[test]
    fn test_add_edges_from_mentions_range() {
        let mut workflow = ChatWorkflow::new();

        workflow.add_message("First", Role::User);
        workflow.add_message("Second", Role::Assistant);
        workflow.add_message("Third", Role::User);
        let idx4 = workflow.add_message("Summarize @1..3", Role::User);

        let edges_added = workflow.add_edges_from_mentions(idx4).unwrap();
        assert_eq!(edges_added, 3); // Edges from @1, @2, @3
    }

    #[test]
    fn test_add_edges_from_mentions_no_self_loop() {
        let mut workflow = ChatWorkflow::new();

        workflow.add_message("First", Role::User);
        workflow.add_message("Second", Role::Assistant);
        // @3 references itself (message count is 3 after adding this)
        let idx3 = workflow.add_message("Self reference @3", Role::User);

        // Note: @3 resolves to 3, but idx3 IS message 3, so it would be a self-loop
        // However, the message says "@3" which means it wants to reference message 3
        // At the time of resolution, message_counter is 3, so @3 is valid
        // But we prevent self-loops
        let edges_added = workflow.add_edges_from_mentions(idx3).unwrap();
        assert_eq!(edges_added, 0); // Self-loop prevented
    }

    #[test]
    fn test_add_edges_from_mentions_no_mentions() {
        let mut workflow = ChatWorkflow::new();

        workflow.add_message("First", Role::User);
        let idx2 = workflow.add_message("No mentions here", Role::User);

        let edges_added = workflow.add_edges_from_mentions(idx2).unwrap();
        assert_eq!(edges_added, 0);
    }

    #[test]
    fn test_add_message_with_mentions() {
        let mut workflow = ChatWorkflow::new();

        workflow.add_message("First", Role::User);
        workflow.add_message("Second", Role::Assistant);
        let idx3 = workflow
            .add_message_with_mentions("Based on @1", Role::User)
            .unwrap();

        // Should have auto-edge (2→3) + mention edge (1→3)
        let deps = workflow.get_dependencies(idx3);
        assert_eq!(deps.len(), 2);
    }

    #[test]
    fn test_add_message_with_mentions_parallel() {
        let mut workflow = ChatWorkflow::new();

        workflow.add_message("First", Role::User);
        workflow.add_message("Second", Role::Assistant);
        let idx3 = workflow
            .add_message_with_mentions("// Based on @1", Role::User)
            .unwrap();

        // Parallel message: no auto-edge, only mention edge (1→3)
        let deps = workflow.get_dependencies(idx3);
        assert_eq!(deps.len(), 1);
    }

    #[test]
    fn test_add_message_with_mentions_error() {
        let mut workflow = ChatWorkflow::new();

        workflow.add_message("First", Role::User);

        // @10 doesn't exist
        let result = workflow.add_message_with_mentions("Reference @10", Role::User);
        assert!(result.is_err());
    }

    #[test]
    fn test_get_dependencies() {
        let mut workflow = ChatWorkflow::new();

        let idx1 = workflow.add_message("First", Role::User);
        let idx2 = workflow.add_message("Second", Role::Assistant);
        let idx3 = workflow.add_message("Third", Role::User);

        // idx3 depends on idx2 (auto-edge)
        let deps = workflow.get_dependencies(idx3);
        assert_eq!(deps.len(), 1);
        assert_eq!(deps[0], idx2);

        // idx1 has no dependencies (first message)
        let deps = workflow.get_dependencies(idx1);
        assert!(deps.is_empty());
    }

    #[test]
    fn test_get_dependents() {
        let mut workflow = ChatWorkflow::new();

        let idx1 = workflow.add_message("First", Role::User);
        let idx2 = workflow.add_message("Second", Role::Assistant);
        workflow.add_message("Third", Role::User);

        // idx1 has idx2 as dependent (auto-edge 1→2)
        let dependents = workflow.get_dependents(idx1);
        assert_eq!(dependents.len(), 1);
        assert_eq!(dependents[0], idx2);
    }

    #[test]
    fn test_complex_mention_dag() {
        let mut workflow = ChatWorkflow::new();

        // Build a DAG:
        // @1 USER: What is Rust?
        // @2 ASSISTANT: Rust is a systems programming language...
        // @3 USER: // What is Go?  (parallel, no edge from @2)
        // @4 ASSISTANT: Go is a programming language...
        // @5 USER: Compare @2 and @4  (depends on both @2 and @4)

        workflow
            .add_message_with_mentions("What is Rust?", Role::User)
            .unwrap();
        workflow
            .add_message_with_mentions("Rust is a systems programming language...", Role::Assistant)
            .unwrap();
        workflow
            .add_message_with_mentions("// What is Go?", Role::User)
            .unwrap();
        workflow
            .add_message_with_mentions("Go is a programming language...", Role::Assistant)
            .unwrap();
        let idx5 = workflow
            .add_message_with_mentions("Compare @2 and @4", Role::User)
            .unwrap();

        // Message 5 should depend on messages 2, 4, and 4 (auto-edge)
        let deps = workflow.get_dependencies(idx5);
        assert_eq!(deps.len(), 3); // auto-edge from @4 + mention edges from @2 and @4

        // Total edges: 1→2, 3→4, 4→5, 2→5, 4→5 (duplicate)
        // Actually: 1→2, 3→4, 4→5 (auto), 2→5 (mention), 4→5 (mention but duplicate)
        // StableGraph allows parallel edges, so we might have 5 edges
        assert!(workflow.dag.edge_count() >= 4);
    }

    // ═══════════════════════════════════════════════════════════════════════════
    // Task 2.4: to_yaml() export tests
    // ═══════════════════════════════════════════════════════════════════════════

    #[test]
    fn test_to_yaml_empty_workflow() {
        let workflow = ChatWorkflow::new();
        let yaml = workflow.to_yaml();

        // Header should be present
        assert!(yaml.contains("# Auto-generated from Nika Chat session"));
        assert!(yaml.contains("schema: \"nika/workflow@0.12\""));
        assert!(yaml.contains("provider: claude"));

        // No tasks
        assert!(yaml.contains("tasks: []"));
        // No flows section at all (uses depends_on per-task instead)
        assert!(!yaml.contains("flows:"));
    }

    #[test]
    fn test_to_yaml_single_user_message() {
        let mut workflow = ChatWorkflow::new();
        workflow.add_message("What is Rust?", Role::User);

        let yaml = workflow.to_yaml();

        // Should have one task
        assert!(yaml.contains("tasks:"));
        assert!(yaml.contains("- id: \"msg-001\""));
        assert!(yaml.contains("infer: \"What is Rust?\""));
    }

    #[test]
    fn test_to_yaml_skips_non_user_messages() {
        let mut workflow = ChatWorkflow::new();
        workflow.add_message("What is Rust?", Role::User); // msg-001
        workflow.add_message("Rust is a systems language", Role::Assistant); // msg-002
        workflow.add_message("System initialized", Role::System); // msg-003
        workflow.add_message("Tool output", Role::Tool); // msg-004
        workflow.add_message("Tell me more", Role::User); // msg-005

        let yaml = workflow.to_yaml();

        // Only User messages become tasks
        assert!(yaml.contains("- id: \"msg-001\""));
        assert!(yaml.contains("- id: \"msg-005\""));

        // Assistant/System/Tool should NOT appear as task IDs
        assert!(!yaml.contains("- id: \"msg-002\""));
        assert!(!yaml.contains("- id: \"msg-003\""));
        assert!(!yaml.contains("- id: \"msg-004\""));
    }

    #[test]
    fn test_to_yaml_escapes_quotes() {
        let mut workflow = ChatWorkflow::new();
        workflow.add_message("Generate \"hello world\" program", Role::User);

        let yaml = workflow.to_yaml();

        // Quotes should be escaped
        assert!(yaml.contains(r#"infer: "Generate \"hello world\" program""#));
    }

    #[test]
    fn test_to_yaml_escapes_multiline() {
        let mut workflow = ChatWorkflow::new();
        workflow.add_message("Line 1\nLine 2", Role::User);

        let yaml = workflow.to_yaml();

        // Newlines should be escaped as \n in the output
        assert!(yaml.contains("Line 1\\nLine 2"));
    }

    #[test]
    fn test_to_yaml_with_mention_creates_depends_on() {
        let mut workflow = ChatWorkflow::new();
        workflow
            .add_message_with_mentions("What is Rust?", Role::User)
            .unwrap(); // msg-001
        workflow
            .add_message_with_mentions("Rust is...", Role::Assistant)
            .unwrap(); // msg-002
        workflow
            .add_message_with_mentions("Expand on @1", Role::User)
            .unwrap(); // msg-003

        let yaml = workflow.to_yaml();

        // msg-003 should have depends_on referencing msg-001
        assert!(yaml.contains("depends_on:"));
        assert!(yaml.contains("\"msg-001\""));
        // No separate flows section
        assert!(!yaml.contains("flows:"));
    }

    #[test]
    fn test_to_yaml_multiple_mentions_create_depends_on() {
        let mut workflow = ChatWorkflow::new();
        workflow
            .add_message_with_mentions("First question", Role::User)
            .unwrap(); // msg-001
        workflow
            .add_message_with_mentions("Answer 1", Role::Assistant)
            .unwrap(); // msg-002
        workflow
            .add_message_with_mentions("Second question", Role::User)
            .unwrap(); // msg-003
        workflow
            .add_message_with_mentions("Answer 2", Role::Assistant)
            .unwrap(); // msg-004
        workflow
            .add_message_with_mentions("Compare @1 and @3", Role::User)
            .unwrap(); // msg-005

        let yaml = workflow.to_yaml();

        // msg-005 should have depends_on with both msg-001 and msg-003
        assert!(yaml.contains("depends_on:"));
        assert!(yaml.contains("\"msg-001\""));
        assert!(yaml.contains("\"msg-003\""));
        // No separate flows section
        assert!(!yaml.contains("flows:"));
    }

    #[test]
    fn test_escape_yaml_string_simple() {
        assert_eq!(escape_yaml_string("hello"), "hello");
    }

    #[test]
    fn test_escape_yaml_string_with_quotes() {
        assert_eq!(escape_yaml_string(r#"say "hi""#), r#"say \"hi\""#);
    }

    #[test]
    fn test_escape_yaml_string_with_newline() {
        assert_eq!(escape_yaml_string("line1\nline2"), "line1\\nline2");
    }

    #[test]
    fn test_escape_yaml_string_with_backslash() {
        assert_eq!(escape_yaml_string(r"path\to\file"), r"path\\to\\file");
    }

    #[test]
    fn test_escape_yaml_string_complex() {
        let input = "He said \"hello\"\nThen walked away";
        let expected = r#"He said \"hello\"\nThen walked away"#;
        assert_eq!(escape_yaml_string(input), expected);
    }

    #[test]
    fn test_to_yaml_round_trip_parseable() {
        // Verify the generated YAML is syntactically valid
        let mut workflow = ChatWorkflow::new();
        workflow.add_message("Question 1", Role::User);
        workflow.add_message("Answer 1", Role::Assistant);
        workflow.add_message("Question 2", Role::User);

        let yaml = workflow.to_yaml();

        // Should be parseable as YAML (not panic)
        // Note: Using serde_json::Value as target since serde-saphyr doesn't export Value type
        let parsed: serde_json::Value = crate::serde_yaml::from_str(&yaml).expect("Valid YAML");

        // Verify structure
        assert!(parsed["schema"].as_str().is_some());
        assert!(parsed["tasks"].is_array()); // serde_json uses is_array()
                                             // No flows section — dependencies expressed via depends_on per-task
        assert!(parsed.get("flows").is_none());
    }
}
