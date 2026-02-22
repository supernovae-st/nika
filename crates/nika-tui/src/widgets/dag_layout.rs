//! DAG Layout Algorithm
//!
//! Sugiyama-style hierarchical layout for DAG visualization.
//! Assigns nodes to layers based on dependencies and computes
//! x/y coordinates for rendering.
//!
//! Algorithm steps:
//! 1. Topological sort to assign layers (longest path to source)
//! 2. Order nodes within each layer
//! 3. Compute x/y coordinates based on spacing config

use rustc_hash::FxHashMap;
use std::collections::VecDeque;

/// Position of a node in the DAG layout
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct NodePosition {
    /// Layer (vertical position, 0 = top)
    pub layer: usize,
    /// Order within layer (horizontal position)
    pub order: usize,
    /// X coordinate in characters
    pub x: u16,
    /// Y coordinate in characters
    pub y: u16,
    /// Width of the node box
    pub width: u16,
    /// Height of the node box
    pub height: u16,
}

/// Layout configuration
#[derive(Debug, Clone)]
pub struct LayoutConfig {
    /// Minimum horizontal spacing between nodes
    pub h_spacing: u16,
    /// Minimum vertical spacing between layers
    pub v_spacing: u16,
    /// Maximum width for node boxes
    pub max_node_width: u16,
    /// Expanded mode (larger boxes)
    pub expanded: bool,
}

impl Default for LayoutConfig {
    fn default() -> Self {
        Self {
            h_spacing: 4,
            v_spacing: 3,
            max_node_width: 50,
            expanded: false,
        }
    }
}

/// Node information for layout computation
#[derive(Debug, Clone)]
pub struct LayoutNode<'a> {
    /// Node identifier
    pub id: &'a str,
    /// Dependencies (predecessor node IDs)
    pub dependencies: Vec<&'a str>,
    /// Display width hint (e.g., based on label length)
    pub width_hint: Option<u16>,
}

impl<'a> LayoutNode<'a> {
    /// Create a new layout node
    pub fn new(id: &'a str) -> Self {
        Self {
            id,
            dependencies: Vec::new(),
            width_hint: None,
        }
    }

    /// Add dependencies
    pub fn with_dependencies(mut self, deps: Vec<&'a str>) -> Self {
        self.dependencies = deps;
        self
    }

    /// Set width hint
    pub fn with_width_hint(mut self, width: u16) -> Self {
        self.width_hint = Some(width);
        self
    }
}

/// Computed DAG layout with node positions
#[derive(Debug, Clone)]
pub struct DagLayout {
    /// Node positions indexed by node ID
    positions: FxHashMap<String, NodePosition>,
    /// Nodes organized by layer (for iteration)
    layers: Vec<Vec<String>>,
}

impl DagLayout {
    /// Compute layout for a set of nodes
    ///
    /// # Arguments
    /// * `nodes` - Nodes with their dependencies
    /// * `config` - Layout configuration
    /// * `node_widths` - Optional map of node ID to display width
    ///
    /// # Returns
    /// A `DagLayout` with computed positions for all nodes
    pub fn compute<'a>(
        nodes: &[LayoutNode<'a>],
        config: &LayoutConfig,
        node_widths: Option<&FxHashMap<String, u16>>,
    ) -> Self {
        if nodes.is_empty() {
            return Self {
                positions: FxHashMap::default(),
                layers: Vec::new(),
            };
        }

        // Step 1: Assign layers via topological sort
        let layer_assignments = Self::assign_layers(nodes);

        // Step 2: Order nodes within each layer
        let layers = Self::order_within_layers(nodes, &layer_assignments);

        // Step 3: Compute positions
        let positions = Self::compute_positions(&layers, config, node_widths);

        Self { positions, layers }
    }

    /// Get position for a node by ID
    pub fn get(&self, id: &str) -> Option<&NodePosition> {
        self.positions.get(id)
    }

    /// Get number of layers
    pub fn layer_count(&self) -> usize {
        self.layers.len()
    }

    /// Iterate over layers (each layer is a vec of node IDs)
    pub fn layers(&self) -> impl Iterator<Item = &Vec<String>> {
        self.layers.iter()
    }

    /// Get all positions
    pub fn positions(&self) -> &FxHashMap<String, NodePosition> {
        &self.positions
    }

    /// Assign layers to nodes using reverse topological sort
    ///
    /// Each node is assigned to the layer = max(predecessor layers) + 1
    /// Roots (no dependencies) are assigned to layer 0.
    fn assign_layers(nodes: &[LayoutNode<'_>]) -> FxHashMap<String, usize> {
        let mut layers: FxHashMap<String, usize> = FxHashMap::default();
        let mut in_degree: FxHashMap<&str, usize> = FxHashMap::default();
        let mut successors: FxHashMap<&str, Vec<&str>> = FxHashMap::default();

        // Build successor map and compute in-degrees
        for node in nodes {
            in_degree.entry(node.id).or_insert(0);
            successors.entry(node.id).or_default();

            for dep in &node.dependencies {
                *in_degree.entry(node.id).or_insert(0) += 1;
                successors.entry(dep).or_default().push(node.id);
            }
        }

        // Kahn's algorithm with layer tracking
        let mut queue: VecDeque<&str> = VecDeque::new();

        // Start with nodes that have no dependencies (layer 0)
        for node in nodes {
            if node.dependencies.is_empty() {
                queue.push_back(node.id);
                layers.insert(node.id.to_string(), 0);
            }
        }

        // Process nodes in topological order
        while let Some(current) = queue.pop_front() {
            let current_layer = *layers.get(current).unwrap_or(&0);

            if let Some(succs) = successors.get(current) {
                for &succ in succs {
                    // Update successor's layer to be at least current + 1
                    let succ_layer = layers.entry(succ.to_string()).or_insert(0);
                    *succ_layer = (*succ_layer).max(current_layer + 1);

                    // Decrement in-degree
                    if let Some(deg) = in_degree.get_mut(succ) {
                        *deg = deg.saturating_sub(1);
                        if *deg == 0 {
                            queue.push_back(succ);
                        }
                    }
                }
            }
        }

        // Handle disconnected nodes (no dependencies and no successors)
        for node in nodes {
            layers.entry(node.id.to_string()).or_insert(0);
        }

        layers
    }

    /// Order nodes within each layer using barycenter method
    ///
    /// The barycenter method minimizes edge crossings by ordering nodes
    /// based on the average position of their neighbors in adjacent layers.
    fn order_within_layers(
        nodes: &[LayoutNode<'_>],
        layer_assignments: &FxHashMap<String, usize>,
    ) -> Vec<Vec<String>> {
        // Find max layer
        let max_layer = layer_assignments.values().copied().max().unwrap_or(0);

        // Initialize layers with original order
        let mut layers: Vec<Vec<String>> = vec![Vec::new(); max_layer + 1];

        // Assign nodes to layers preserving original order initially
        for node in nodes {
            if let Some(&layer) = layer_assignments.get(node.id) {
                layers[layer].push(node.id.to_string());
            }
        }

        // Build adjacency maps for barycenter calculation
        // successors: node_id -> [successor_ids] (nodes that depend on this node)
        // predecessors: node_id -> [predecessor_ids] (nodes this node depends on)
        let mut successors: FxHashMap<String, Vec<String>> = FxHashMap::default();
        let mut predecessors: FxHashMap<String, Vec<String>> = FxHashMap::default();

        for node in nodes {
            successors.entry(node.id.to_string()).or_default();
            predecessors.entry(node.id.to_string()).or_default();

            for dep in &node.dependencies {
                successors
                    .entry(dep.to_string())
                    .or_default()
                    .push(node.id.to_string());
                predecessors
                    .entry(node.id.to_string())
                    .or_default()
                    .push(dep.to_string());
            }
        }

        // Apply barycenter method in multiple passes
        const MAX_ITERATIONS: usize = 4;

        for _ in 0..MAX_ITERATIONS {
            // Forward pass: order layers based on predecessor positions
            for layer_idx in 1..layers.len() {
                Self::order_layer_by_barycenter(
                    &mut layers,
                    layer_idx,
                    &predecessors,
                    true, // use predecessors
                );
            }

            // Backward pass: order layers based on successor positions
            for layer_idx in (0..layers.len().saturating_sub(1)).rev() {
                Self::order_layer_by_barycenter(
                    &mut layers,
                    layer_idx,
                    &successors,
                    false, // use successors
                );
            }
        }

        layers
    }

    /// Order a single layer using barycenter method
    ///
    /// Each node gets a barycenter value = average position of its neighbors
    /// in the adjacent layer. Nodes are then sorted by this value.
    fn order_layer_by_barycenter(
        layers: &mut [Vec<String>],
        layer_idx: usize,
        neighbors: &FxHashMap<String, Vec<String>>,
        use_prev_layer: bool,
    ) {
        let adjacent_layer_idx = if use_prev_layer {
            layer_idx.saturating_sub(1)
        } else {
            layer_idx.saturating_add(1)
        };

        // Guard against out of bounds
        if adjacent_layer_idx >= layers.len() || adjacent_layer_idx == layer_idx {
            return;
        }

        // Build position map for adjacent layer
        let adjacent_positions: FxHashMap<&str, usize> = layers[adjacent_layer_idx]
            .iter()
            .enumerate()
            .map(|(pos, id)| (id.as_str(), pos))
            .collect();

        // Calculate barycenter for each node in current layer
        let mut barycenters: Vec<(String, f64)> = layers[layer_idx]
            .iter()
            .map(|node_id| {
                let neighbor_positions: Vec<usize> = neighbors
                    .get(node_id)
                    .map(|n| {
                        n.iter()
                            .filter_map(|neighbor| {
                                adjacent_positions.get(neighbor.as_str()).copied()
                            })
                            .collect()
                    })
                    .unwrap_or_default();

                let barycenter = if neighbor_positions.is_empty() {
                    // No neighbors - keep original relative position (use infinity to sort last)
                    f64::MAX
                } else {
                    // Average position of neighbors
                    let sum: usize = neighbor_positions.iter().sum();
                    (sum as f64) / (neighbor_positions.len() as f64)
                };

                (node_id.clone(), barycenter)
            })
            .collect();

        // Sort by barycenter value (stable sort to preserve order for equal values)
        barycenters.sort_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal));

        // Update layer with new order
        layers[layer_idx] = barycenters.into_iter().map(|(id, _)| id).collect();
    }

    /// Compute x/y positions for all nodes
    fn compute_positions(
        layers: &[Vec<String>],
        config: &LayoutConfig,
        node_widths: Option<&FxHashMap<String, u16>>,
    ) -> FxHashMap<String, NodePosition> {
        let mut positions: FxHashMap<String, NodePosition> = FxHashMap::default();

        let node_height = if config.expanded { 3 } else { 1 };

        for (layer_idx, layer) in layers.iter().enumerate() {
            let y = (layer_idx as u16) * (node_height + config.v_spacing);

            let mut x: u16 = 0;

            for (order, node_id) in layer.iter().enumerate() {
                // Determine node width
                let width = node_widths
                    .and_then(|w| w.get(node_id).copied())
                    .unwrap_or(config.max_node_width.min(20))
                    .min(config.max_node_width);

                positions.insert(
                    node_id.clone(),
                    NodePosition {
                        layer: layer_idx,
                        order,
                        x,
                        y,
                        width,
                        height: node_height,
                    },
                );

                x += width + config.h_spacing;
            }
        }

        positions
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ═══════════════════════════════════════════════════════════════
    // LINEAR DAG TESTS
    // ═══════════════════════════════════════════════════════════════

    #[test]
    fn test_linear_dag() {
        // Simple chain: a → b → c
        let nodes = vec![
            LayoutNode::new("a"),
            LayoutNode::new("b").with_dependencies(vec!["a"]),
            LayoutNode::new("c").with_dependencies(vec!["b"]),
        ];

        let config = LayoutConfig::default();
        let layout = DagLayout::compute(&nodes, &config, None);

        // Verify layer assignments
        assert_eq!(layout.layer_count(), 3);

        let pos_a = layout.get("a").expect("a should have position");
        let pos_b = layout.get("b").expect("b should have position");
        let pos_c = layout.get("c").expect("c should have position");

        // a is layer 0, b is layer 1, c is layer 2
        assert_eq!(pos_a.layer, 0);
        assert_eq!(pos_b.layer, 1);
        assert_eq!(pos_c.layer, 2);

        // Verify y coordinates increase with layer
        assert!(pos_b.y > pos_a.y);
        assert!(pos_c.y > pos_b.y);
    }

    // ═══════════════════════════════════════════════════════════════
    // PARALLEL (DIAMOND) DAG TESTS
    // ═══════════════════════════════════════════════════════════════

    #[test]
    fn test_parallel_dag() {
        // Diamond: start → {a, b} → end
        let nodes = vec![
            LayoutNode::new("start"),
            LayoutNode::new("a").with_dependencies(vec!["start"]),
            LayoutNode::new("b").with_dependencies(vec!["start"]),
            LayoutNode::new("end").with_dependencies(vec!["a", "b"]),
        ];

        let config = LayoutConfig::default();
        let layout = DagLayout::compute(&nodes, &config, None);

        // Verify layer assignments
        assert_eq!(layout.layer_count(), 3);

        let pos_start = layout.get("start").expect("start should have position");
        let pos_a = layout.get("a").expect("a should have position");
        let pos_b = layout.get("b").expect("b should have position");
        let pos_end = layout.get("end").expect("end should have position");

        // start is layer 0, a/b are layer 1, end is layer 2
        assert_eq!(pos_start.layer, 0);
        assert_eq!(pos_a.layer, 1);
        assert_eq!(pos_b.layer, 1);
        assert_eq!(pos_end.layer, 2);

        // a and b should be in the same layer (layer 1)
        let layer_1 = &layout.layers[1];
        assert_eq!(layer_1.len(), 2);
        assert!(layer_1.contains(&"a".to_string()));
        assert!(layer_1.contains(&"b".to_string()));
    }

    // ═══════════════════════════════════════════════════════════════
    // EMPTY DAG TESTS
    // ═══════════════════════════════════════════════════════════════

    #[test]
    fn test_empty_dag() {
        let nodes: Vec<LayoutNode<'_>> = vec![];
        let config = LayoutConfig::default();
        let layout = DagLayout::compute(&nodes, &config, None);

        assert_eq!(layout.layer_count(), 0);
        assert!(layout.positions().is_empty());
    }

    // ═══════════════════════════════════════════════════════════════
    // DISCONNECTED NODES TESTS
    // ═══════════════════════════════════════════════════════════════

    #[test]
    fn test_disconnected_nodes() {
        // Three independent nodes with no dependencies
        let nodes = vec![
            LayoutNode::new("x"),
            LayoutNode::new("y"),
            LayoutNode::new("z"),
        ];

        let config = LayoutConfig::default();
        let layout = DagLayout::compute(&nodes, &config, None);

        // All should be in layer 0
        assert_eq!(layout.layer_count(), 1);

        let pos_x = layout.get("x").expect("x should have position");
        let pos_y = layout.get("y").expect("y should have position");
        let pos_z = layout.get("z").expect("z should have position");

        assert_eq!(pos_x.layer, 0);
        assert_eq!(pos_y.layer, 0);
        assert_eq!(pos_z.layer, 0);

        // They should have different x coordinates (horizontal spread)
        assert_ne!(pos_x.x, pos_y.x);
        assert_ne!(pos_y.x, pos_z.x);
    }

    // ═══════════════════════════════════════════════════════════════
    // LAYOUT CONFIG TESTS
    // ═══════════════════════════════════════════════════════════════

    #[test]
    fn test_layout_config_default() {
        let config = LayoutConfig::default();
        assert_eq!(config.h_spacing, 4);
        assert_eq!(config.v_spacing, 3);
        assert_eq!(config.max_node_width, 50);
        assert!(!config.expanded);
    }

    #[test]
    fn test_expanded_mode_increases_height() {
        let nodes = vec![LayoutNode::new("a")];

        let compact_config = LayoutConfig {
            expanded: false,
            ..Default::default()
        };
        let expanded_config = LayoutConfig {
            expanded: true,
            ..Default::default()
        };

        let compact_layout = DagLayout::compute(&nodes, &compact_config, None);
        let expanded_layout = DagLayout::compute(&nodes, &expanded_config, None);

        let compact_height = compact_layout.get("a").unwrap().height;
        let expanded_height = expanded_layout.get("a").unwrap().height;

        assert!(
            expanded_height > compact_height,
            "Expanded mode should have larger node height"
        );
    }

    // ═══════════════════════════════════════════════════════════════
    // NODE WIDTH TESTS
    // ═══════════════════════════════════════════════════════════════

    #[test]
    fn test_custom_node_widths() {
        let nodes = vec![
            LayoutNode::new("short"),
            LayoutNode::new("very_long_task_name"),
        ];

        let mut widths: FxHashMap<String, u16> = FxHashMap::default();
        widths.insert("short".to_string(), 10);
        widths.insert("very_long_task_name".to_string(), 25);

        let config = LayoutConfig::default();
        let layout = DagLayout::compute(&nodes, &config, Some(&widths));

        let pos_short = layout.get("short").unwrap();
        let pos_long = layout.get("very_long_task_name").unwrap();

        assert_eq!(pos_short.width, 10);
        assert_eq!(pos_long.width, 25);
    }

    #[test]
    fn test_width_clamped_to_max() {
        let nodes = vec![LayoutNode::new("wide")];

        let mut widths: FxHashMap<String, u16> = FxHashMap::default();
        widths.insert("wide".to_string(), 100); // Exceeds max

        let config = LayoutConfig {
            max_node_width: 30,
            ..Default::default()
        };
        let layout = DagLayout::compute(&nodes, &config, Some(&widths));

        let pos = layout.get("wide").unwrap();
        assert_eq!(pos.width, 30, "Width should be clamped to max_node_width");
    }

    // ═══════════════════════════════════════════════════════════════
    // COMPLEX DAG TESTS
    // ═══════════════════════════════════════════════════════════════

    #[test]
    fn test_multi_level_dag() {
        // a → b → d
        // a → c → d
        // (longer chain determines layer for d)
        let nodes = vec![
            LayoutNode::new("a"),
            LayoutNode::new("b").with_dependencies(vec!["a"]),
            LayoutNode::new("c").with_dependencies(vec!["a"]),
            LayoutNode::new("d").with_dependencies(vec!["b", "c"]),
        ];

        let config = LayoutConfig::default();
        let layout = DagLayout::compute(&nodes, &config, None);

        let pos_a = layout.get("a").unwrap();
        let pos_b = layout.get("b").unwrap();
        let pos_c = layout.get("c").unwrap();
        let pos_d = layout.get("d").unwrap();

        // a is layer 0, b/c are layer 1, d is layer 2
        assert_eq!(pos_a.layer, 0);
        assert_eq!(pos_b.layer, 1);
        assert_eq!(pos_c.layer, 1);
        assert_eq!(pos_d.layer, 2);
    }

    #[test]
    fn test_wide_dag() {
        // start → {a, b, c, d, e} (5 parallel tasks)
        let nodes = vec![
            LayoutNode::new("start"),
            LayoutNode::new("a").with_dependencies(vec!["start"]),
            LayoutNode::new("b").with_dependencies(vec!["start"]),
            LayoutNode::new("c").with_dependencies(vec!["start"]),
            LayoutNode::new("d").with_dependencies(vec!["start"]),
            LayoutNode::new("e").with_dependencies(vec!["start"]),
        ];

        let config = LayoutConfig::default();
        let layout = DagLayout::compute(&nodes, &config, None);

        // All parallel tasks should be in layer 1
        let layer_1 = &layout.layers[1];
        assert_eq!(layer_1.len(), 5);

        // X coordinates should be spread out
        let x_coords: Vec<u16> = ["a", "b", "c", "d", "e"]
            .iter()
            .map(|id| layout.get(id).unwrap().x)
            .collect();

        // All x coordinates should be unique
        let mut sorted = x_coords.clone();
        sorted.sort();
        sorted.dedup();
        assert_eq!(sorted.len(), 5, "All nodes should have unique x positions");
    }

    // ═══════════════════════════════════════════════════════════════
    // LAYER ITERATION TESTS
    // ═══════════════════════════════════════════════════════════════

    #[test]
    fn test_layers_iteration() {
        let nodes = vec![
            LayoutNode::new("a"),
            LayoutNode::new("b").with_dependencies(vec!["a"]),
            LayoutNode::new("c").with_dependencies(vec!["b"]),
        ];

        let config = LayoutConfig::default();
        let layout = DagLayout::compute(&nodes, &config, None);

        let layer_vec: Vec<_> = layout.layers().collect();
        assert_eq!(layer_vec.len(), 3);
        assert!(layer_vec[0].contains(&"a".to_string()));
        assert!(layer_vec[1].contains(&"b".to_string()));
        assert!(layer_vec[2].contains(&"c".to_string()));
    }

    // ═══════════════════════════════════════════════════════════════
    // POSITION PROPERTIES TESTS
    // ═══════════════════════════════════════════════════════════════

    #[test]
    fn test_node_position_default() {
        let pos = NodePosition::default();
        assert_eq!(pos.layer, 0);
        assert_eq!(pos.order, 0);
        assert_eq!(pos.x, 0);
        assert_eq!(pos.y, 0);
        assert_eq!(pos.width, 0);
        assert_eq!(pos.height, 0);
    }

    #[test]
    fn test_y_increases_with_layer() {
        let nodes = vec![
            LayoutNode::new("a"),
            LayoutNode::new("b").with_dependencies(vec!["a"]),
            LayoutNode::new("c").with_dependencies(vec!["b"]),
            LayoutNode::new("d").with_dependencies(vec!["c"]),
        ];

        let config = LayoutConfig::default();
        let layout = DagLayout::compute(&nodes, &config, None);

        let y_a = layout.get("a").unwrap().y;
        let y_b = layout.get("b").unwrap().y;
        let y_c = layout.get("c").unwrap().y;
        let y_d = layout.get("d").unwrap().y;

        assert!(y_a < y_b);
        assert!(y_b < y_c);
        assert!(y_c < y_d);
    }

    // ═══════════════════════════════════════════════════════════════
    // MIXED DEPENDENCIES TESTS
    // ═══════════════════════════════════════════════════════════════

    #[test]
    fn test_mixed_disconnected_and_connected() {
        // Connected chain: a → b
        // Disconnected: x, y
        let nodes = vec![
            LayoutNode::new("a"),
            LayoutNode::new("b").with_dependencies(vec!["a"]),
            LayoutNode::new("x"),
            LayoutNode::new("y"),
        ];

        let config = LayoutConfig::default();
        let layout = DagLayout::compute(&nodes, &config, None);

        // a, x, y should be in layer 0
        assert_eq!(layout.get("a").unwrap().layer, 0);
        assert_eq!(layout.get("x").unwrap().layer, 0);
        assert_eq!(layout.get("y").unwrap().layer, 0);

        // b should be in layer 1
        assert_eq!(layout.get("b").unwrap().layer, 1);
    }

    // ═══════════════════════════════════════════════════════════════
    // LAYOUT NODE BUILDER TESTS
    // ═══════════════════════════════════════════════════════════════

    #[test]
    fn test_layout_node_builder() {
        let node = LayoutNode::new("task1")
            .with_dependencies(vec!["dep1", "dep2"])
            .with_width_hint(25);

        assert_eq!(node.id, "task1");
        assert_eq!(node.dependencies, vec!["dep1", "dep2"]);
        assert_eq!(node.width_hint, Some(25));
    }

    #[test]
    fn test_layout_node_default_width_hint() {
        let node = LayoutNode::new("task1");
        assert!(node.width_hint.is_none());
    }

    // ═══════════════════════════════════════════════════════════════
    // BARYCENTER ORDERING TESTS
    // ═══════════════════════════════════════════════════════════════

    #[test]
    fn test_barycenter_reduces_crossings() {
        // Create a graph where barycenter should reorder:
        //   a ─────────────► y
        //   b ─────────────► x
        // Without barycenter: [a,b] -> [x,y] has 1 crossing
        // With barycenter: [a,b] -> [y,x] has 0 crossings
        let nodes = vec![
            LayoutNode::new("a"),
            LayoutNode::new("b"),
            LayoutNode::new("x").with_dependencies(vec!["b"]),
            LayoutNode::new("y").with_dependencies(vec!["a"]),
        ];

        let config = LayoutConfig::default();
        let layout = DagLayout::compute(&nodes, &config, None);

        // Layer 1 should have x and y
        let layer_1 = &layout.layers[1];
        assert_eq!(layer_1.len(), 2);

        // After barycenter, y should come before x (aligned with a before b)
        // because y depends on a (position 0) and x depends on b (position 1)
        let y_pos = layer_1.iter().position(|s| s == "y");
        let x_pos = layer_1.iter().position(|s| s == "x");

        assert!(
            y_pos.is_some() && x_pos.is_some(),
            "Both x and y should be in layer 1"
        );
        assert!(
            y_pos < x_pos,
            "y (depending on a at pos 0) should come before x (depending on b at pos 1)"
        );
    }

    #[test]
    fn test_barycenter_handles_no_dependencies() {
        // Nodes without dependencies should not crash
        let nodes = vec![
            LayoutNode::new("a"),
            LayoutNode::new("b"),
            LayoutNode::new("c"),
        ];

        let config = LayoutConfig::default();
        let layout = DagLayout::compute(&nodes, &config, None);

        // All nodes should be in layer 0
        assert_eq!(layout.layer_count(), 1);
        assert_eq!(layout.layers[0].len(), 3);
    }
}
