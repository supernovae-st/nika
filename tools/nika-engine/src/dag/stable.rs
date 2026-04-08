// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! StableDag - petgraph::StableGraph wrapper for stable NodeIndex
//!
//! Provides stable NodeIndex values that persist after node deletion.
//! Critical for @mention references where deleted messages should not
//! invalidate existing references.
//!
//! # Why StableGraph?
//!
//! ```text
//! DiGraph (before):
//! ├── Node 0: "msg-001" (index=0)
//! ├── Node 1: "msg-002" (index=1)  ← DELETE THIS
//! └── Node 2: "msg-003" (index=2)  ← BECOMES index=1 ⚠️
//!
//! StableGraph:
//! ├── Node 0: "msg-001" (index=0)
//! ├── Node 1: DELETED
//! └── Node 2: "msg-003" (index=2)  ← STAYS index=2 ✅
//! ```

use petgraph::stable_graph::{EdgeIndex, NodeIndex, StableGraph};
use petgraph::Directed;
use serde::{Deserialize, Serialize};

/// Edge weight for flow dependencies.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct DagEdge {
    /// Optional edge label for debugging
    pub label: Option<String>,
}

/// Wrapper around petgraph::StableGraph for DAG operations.
/// NodeIndex values remain stable after node deletion.
///
/// # Serialization Warning
///
/// While this struct derives Serialize/Deserialize via petgraph's `serde-1` feature,
/// **NodeIndex values serialize as raw u32 integers**. These are NOT stable across:
/// - Graph modifications (add/remove nodes)
/// - Serialization roundtrips after deletion
///
/// For persistence, use stable identifiers (e.g., message IDs) instead of NodeIndex.
/// The `index_map: HashMap<MessageId, NodeIndex>` pattern is recommended.
#[derive(Debug, Clone)]
pub struct StableDag<N> {
    inner: StableGraph<N, DagEdge, Directed>,
}

// Manual Serialize impl to avoid lifetime issues with derive
impl<N: Serialize> Serialize for StableDag<N> {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        self.inner.serialize(serializer)
    }
}

// Manual Deserialize impl
impl<'de, N: Deserialize<'de>> Deserialize<'de> for StableDag<N> {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let inner = StableGraph::<N, DagEdge, Directed>::deserialize(deserializer)?;
        Ok(Self { inner })
    }
}

impl<N> StableDag<N> {
    /// Create an empty StableDag.
    pub fn new() -> Self {
        Self {
            inner: StableGraph::new(),
        }
    }

    /// Create with pre-allocated capacity for nodes and edges.
    pub fn with_capacity(nodes: usize, edges: usize) -> Self {
        Self {
            inner: StableGraph::with_capacity(nodes, edges),
        }
    }

    /// Get the number of nodes in the graph.
    #[inline]
    pub fn node_count(&self) -> usize {
        self.inner.node_count()
    }

    /// Get the number of edges in the graph.
    #[inline]
    pub fn edge_count(&self) -> usize {
        self.inner.edge_count()
    }

    // ═══════════════════════════════════════════════════════════════════════
    // Node operations (Task 0.2)
    // ═══════════════════════════════════════════════════════════════════════

    /// Add a node and return its stable index.
    /// The index remains valid even after other nodes are removed.
    pub fn add_node(&mut self, weight: N) -> NodeIndex {
        self.inner.add_node(weight)
    }

    /// Get node weight by index.
    pub fn node_weight(&self, idx: NodeIndex) -> Option<&N> {
        self.inner.node_weight(idx)
    }

    /// Get mutable node weight by index.
    pub fn node_weight_mut(&mut self, idx: NodeIndex) -> Option<&mut N> {
        self.inner.node_weight_mut(idx)
    }

    // ═══════════════════════════════════════════════════════════════════════
    // Node removal (Task 0.3) — KEY: Other indices remain stable
    // ═══════════════════════════════════════════════════════════════════════

    /// Remove a node by index.
    /// Returns the node weight if it existed.
    /// **Other node indices remain stable** — this is the key invariant.
    pub fn remove_node(&mut self, idx: NodeIndex) -> Option<N> {
        self.inner.remove_node(idx)
    }

    /// Check if a node exists.
    pub fn contains_node(&self, idx: NodeIndex) -> bool {
        self.inner.contains_node(idx)
    }

    // ═══════════════════════════════════════════════════════════════════════
    // Edge operations (Task 0.4)
    // ═══════════════════════════════════════════════════════════════════════

    /// Add a directed edge from `a` to `b`.
    /// Returns None if either node doesn't exist.
    pub fn add_edge(&mut self, a: NodeIndex, b: NodeIndex) -> Option<EdgeIndex> {
        if !self.contains_node(a) || !self.contains_node(b) {
            return None;
        }
        Some(self.inner.add_edge(a, b, DagEdge::default()))
    }

    /// Add a directed edge with a label.
    pub fn add_edge_with_label(
        &mut self,
        a: NodeIndex,
        b: NodeIndex,
        label: impl Into<String>,
    ) -> Option<EdgeIndex> {
        if !self.contains_node(a) || !self.contains_node(b) {
            return None;
        }
        Some(self.inner.add_edge(
            a,
            b,
            DagEdge {
                label: Some(label.into()),
            },
        ))
    }

    /// Check if an edge exists from `a` to `b`.
    pub fn has_edge(&self, a: NodeIndex, b: NodeIndex) -> bool {
        self.inner.find_edge(a, b).is_some()
    }

    /// Remove an edge between nodes.
    /// Returns true if an edge was removed.
    pub fn remove_edge(&mut self, a: NodeIndex, b: NodeIndex) -> bool {
        if let Some(edge) = self.inner.find_edge(a, b) {
            self.inner.remove_edge(edge);
            true
        } else {
            false
        }
    }

    // ═══════════════════════════════════════════════════════════════════════
    // Task 2.10: Neighbor traversal for mention edges
    // ═══════════════════════════════════════════════════════════════════════

    /// Get neighbors in a specific direction.
    ///
    /// - `Incoming`: nodes that have edges TO this node (dependencies)
    /// - `Outgoing`: nodes that have edges FROM this node (dependents)
    pub fn neighbors_directed(
        &self,
        idx: NodeIndex,
        direction: petgraph::Direction,
    ) -> impl Iterator<Item = NodeIndex> + '_ {
        self.inner.neighbors_directed(idx, direction)
    }

    /// Get incoming neighbors (dependencies).
    pub fn incoming_neighbors(&self, idx: NodeIndex) -> impl Iterator<Item = NodeIndex> + '_ {
        self.neighbors_directed(idx, petgraph::Direction::Incoming)
    }

    /// Get outgoing neighbors (dependents).
    pub fn outgoing_neighbors(&self, idx: NodeIndex) -> impl Iterator<Item = NodeIndex> + '_ {
        self.neighbors_directed(idx, petgraph::Direction::Outgoing)
    }
}

impl<N> Default for StableDag<N> {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_stable_graph_new_creates_empty_graph() {
        let graph: StableDag<String> = StableDag::new();
        assert_eq!(graph.node_count(), 0);
        assert_eq!(graph.edge_count(), 0);
    }

    #[test]
    fn test_stable_graph_with_capacity() {
        let graph: StableDag<String> = StableDag::with_capacity(10, 20);
        assert_eq!(graph.node_count(), 0);
        // Capacity is internal optimization, not testable directly
    }

    #[test]
    fn test_stable_graph_default_is_empty() {
        let graph: StableDag<String> = StableDag::default();
        assert_eq!(graph.node_count(), 0);
        assert_eq!(graph.edge_count(), 0);
    }

    // ═══════════════════════════════════════════════════════════════════════
    // Task 0.2: add_node() tests
    // ═══════════════════════════════════════════════════════════════════════

    #[test]
    fn test_add_node_returns_stable_index() {
        let mut graph: StableDag<String> = StableDag::new();

        let idx1 = graph.add_node("msg-001".to_string());
        let idx2 = graph.add_node("msg-002".to_string());

        assert_eq!(idx1.index(), 0);
        assert_eq!(idx2.index(), 1);
        assert_eq!(graph.node_count(), 2);
    }

    #[test]
    fn test_add_node_increments_count() {
        let mut graph: StableDag<String> = StableDag::new();

        assert_eq!(graph.node_count(), 0);
        graph.add_node("a".to_string());
        assert_eq!(graph.node_count(), 1);
        graph.add_node("b".to_string());
        assert_eq!(graph.node_count(), 2);
    }

    #[test]
    fn test_node_weight_retrieves_value() {
        let mut graph: StableDag<String> = StableDag::new();
        let idx = graph.add_node("test-value".to_string());

        assert_eq!(graph.node_weight(idx), Some(&"test-value".to_string()));
    }

    #[test]
    fn test_node_weight_mut_allows_modification() {
        let mut graph: StableDag<String> = StableDag::new();
        let idx = graph.add_node("original".to_string());

        if let Some(weight) = graph.node_weight_mut(idx) {
            *weight = "modified".to_string();
        }

        assert_eq!(graph.node_weight(idx), Some(&"modified".to_string()));
    }

    // ═══════════════════════════════════════════════════════════════════════
    // Task 0.3: remove_node() tests — INDEX STABILITY IS KEY
    // ═══════════════════════════════════════════════════════════════════════

    #[test]
    fn test_remove_node_preserves_other_indices() {
        let mut graph: StableDag<String> = StableDag::new();

        let idx0 = graph.add_node("msg-001".to_string());
        let idx1 = graph.add_node("msg-002".to_string());
        let idx2 = graph.add_node("msg-003".to_string());

        // Remove middle node
        graph.remove_node(idx1);

        // idx0 and idx2 should still be valid with same values
        assert_eq!(graph.node_weight(idx0), Some(&"msg-001".to_string()));
        assert_eq!(graph.node_weight(idx1), None); // Removed
        assert_eq!(graph.node_weight(idx2), Some(&"msg-003".to_string()));

        // Indices should NOT have shifted — THIS IS THE KEY INVARIANT
        assert_eq!(idx0.index(), 0);
        assert_eq!(idx2.index(), 2); // Still 2, not 1!
    }

    #[test]
    fn test_remove_node_decrements_count() {
        let mut graph: StableDag<String> = StableDag::new();

        let idx = graph.add_node("test".to_string());
        assert_eq!(graph.node_count(), 1);

        graph.remove_node(idx);
        assert_eq!(graph.node_count(), 0);
    }

    #[test]
    fn test_remove_node_returns_weight() {
        let mut graph: StableDag<String> = StableDag::new();
        let idx = graph.add_node("value".to_string());

        let removed = graph.remove_node(idx);
        assert_eq!(removed, Some("value".to_string()));
    }

    #[test]
    fn test_remove_node_missing_returns_none() {
        let mut graph: StableDag<String> = StableDag::new();
        let idx = graph.add_node("value".to_string());
        graph.remove_node(idx);

        // Second removal returns None
        let removed = graph.remove_node(idx);
        assert_eq!(removed, None);
    }

    #[test]
    fn test_contains_node_after_removal() {
        let mut graph: StableDag<String> = StableDag::new();
        let idx = graph.add_node("test".to_string());

        assert!(graph.contains_node(idx));
        graph.remove_node(idx);
        assert!(!graph.contains_node(idx));
    }

    // ═══════════════════════════════════════════════════════════════════════
    // Task 0.4: Edge operations tests
    // ═══════════════════════════════════════════════════════════════════════

    #[test]
    fn test_add_edge_creates_directed_edge() {
        let mut graph: StableDag<String> = StableDag::new();

        let a = graph.add_node("a".to_string());
        let b = graph.add_node("b".to_string());

        let edge = graph.add_edge(a, b);
        assert!(edge.is_some());
        assert_eq!(graph.edge_count(), 1);
    }

    #[test]
    fn test_add_edge_invalid_node_returns_none() {
        use petgraph::stable_graph::NodeIndex;

        let mut graph: StableDag<String> = StableDag::new();

        let a = graph.add_node("a".to_string());
        let invalid = NodeIndex::new(999);

        let edge = graph.add_edge(a, invalid);
        assert!(edge.is_none());
        assert_eq!(graph.edge_count(), 0);
    }

    #[test]
    fn test_has_edge_checks_existence() {
        let mut graph: StableDag<String> = StableDag::new();

        let a = graph.add_node("a".to_string());
        let b = graph.add_node("b".to_string());
        let c = graph.add_node("c".to_string());

        graph.add_edge(a, b);

        assert!(graph.has_edge(a, b));
        assert!(!graph.has_edge(b, a)); // Directed - no reverse
        assert!(!graph.has_edge(a, c)); // No edge exists
    }

    #[test]
    fn test_remove_edge_works() {
        let mut graph: StableDag<String> = StableDag::new();

        let a = graph.add_node("a".to_string());
        let b = graph.add_node("b".to_string());

        graph.add_edge(a, b);
        assert!(graph.has_edge(a, b));
        assert_eq!(graph.edge_count(), 1);

        let removed = graph.remove_edge(a, b);
        assert!(removed);
        assert!(!graph.has_edge(a, b));
        assert_eq!(graph.edge_count(), 0);
    }

    #[test]
    fn test_remove_edge_nonexistent_returns_false() {
        let mut graph: StableDag<String> = StableDag::new();

        let a = graph.add_node("a".to_string());
        let b = graph.add_node("b".to_string());

        let removed = graph.remove_edge(a, b);
        assert!(!removed);
    }
}
