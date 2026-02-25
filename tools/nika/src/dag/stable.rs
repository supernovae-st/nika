//! StableFlowGraph - petgraph::StableGraph wrapper for stable NodeIndex (v0.9.0)
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
//! StableGraph (v0.9.0+):
//! ├── Node 0: "msg-001" (index=0)
//! ├── Node 1: DELETED
//! └── Node 2: "msg-003" (index=2)  ← STAYS index=2 ✅
//! ```

use petgraph::stable_graph::StableGraph;
use petgraph::Directed;
use serde::{Deserialize, Serialize};

/// Edge weight for flow dependencies.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct FlowEdge {
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
pub struct StableFlowGraph<N> {
    inner: StableGraph<N, FlowEdge, Directed>,
}

// Manual Serialize impl to avoid lifetime issues with derive
impl<N: Serialize> Serialize for StableFlowGraph<N> {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        self.inner.serialize(serializer)
    }
}

// Manual Deserialize impl
impl<'de, N: Deserialize<'de>> Deserialize<'de> for StableFlowGraph<N> {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let inner = StableGraph::<N, FlowEdge, Directed>::deserialize(deserializer)?;
        Ok(Self { inner })
    }
}

impl<N> StableFlowGraph<N> {
    /// Create an empty StableFlowGraph.
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
}

impl<N> Default for StableFlowGraph<N> {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_stable_graph_new_creates_empty_graph() {
        let graph: StableFlowGraph<String> = StableFlowGraph::new();
        assert_eq!(graph.node_count(), 0);
        assert_eq!(graph.edge_count(), 0);
    }

    #[test]
    fn test_stable_graph_with_capacity() {
        let graph: StableFlowGraph<String> = StableFlowGraph::with_capacity(10, 20);
        assert_eq!(graph.node_count(), 0);
        // Capacity is internal optimization, not testable directly
    }

    #[test]
    fn test_stable_graph_default_is_empty() {
        let graph: StableFlowGraph<String> = StableFlowGraph::default();
        assert_eq!(graph.node_count(), 0);
        assert_eq!(graph.edge_count(), 0);
    }
}
