//! Validation layer implementations
//!
//! Each layer validates a specific aspect of the workflow:
//! - Layer 1: Schema - YAML structure
//! - Layer 2: Nodes - Node definitions
//! - Layer 3: Edges - Edge connections
//! - Layer 4: Paradigms - Paradigm compatibility (THE KEY RULE!)
//! - Layer 5: Graph - Structural integrity (see graph.rs)

use crate::errors::{Severity, ValidationError, ValidationLayer};
use crate::rules::{NodeTypes, ParadigmMatrix};
use crate::workflow::Workflow;
use std::collections::{HashMap, HashSet, VecDeque};

/// Layer 2: Validate node definitions
pub fn validate_nodes(workflow: &Workflow, node_types: &NodeTypes) -> Vec<ValidationError> {
    let mut errors = Vec::new();

    // Check for duplicate node IDs
    let mut seen_ids = HashSet::new();
    for node in &workflow.nodes {
        if !seen_ids.insert(&node.id) {
            errors.push(ValidationError::DuplicateNodeId {
                layer: ValidationLayer::Nodes,
                id: node.id.clone(),
            });
        }
    }

    // Check node ID format (starts with letter, alphanumeric with hyphens/underscores)
    let id_regex = regex::Regex::new(r"^[a-zA-Z][a-zA-Z0-9_-]*$").unwrap();
    for node in &workflow.nodes {
        if !id_regex.is_match(&node.id) {
            errors.push(ValidationError::InvalidNodeIdFormat {
                layer: ValidationLayer::Nodes,
                id: node.id.clone(),
                suggestion: "Node ID must start with a letter and contain only alphanumeric characters, hyphens, or underscores".to_string(),
            });
        }
    }

    // Check node types are valid
    for node in &workflow.nodes {
        if node_types.is_visual_type(&node.node_type) {
            // Visual nodes are allowed but produce a warning
            errors.push(ValidationError::VisualNodeType {
                layer: ValidationLayer::Nodes,
                node_type: node.node_type.clone(),
                severity: Severity::Warning,
            });
        } else if !node_types.is_valid_type(&node.node_type) {
            let suggestions = node_types.find_similar(&node.node_type, 3);
            errors.push(ValidationError::UnknownNodeType {
                layer: ValidationLayer::Nodes,
                node_type: node.node_type.clone(),
                suggestions: suggestions.into_iter().map(String::from).collect(),
            });
        }
    }

    errors
}

/// Layer 3: Validate edge connections
pub fn validate_edges(workflow: &Workflow) -> Vec<ValidationError> {
    let mut errors = Vec::new();
    let node_ids: HashSet<&str> = workflow.node_ids().collect();

    for edge in &workflow.edges {
        // Check source exists
        if !node_ids.contains(edge.source.as_str()) {
            errors.push(ValidationError::EdgeSourceNotFound {
                layer: ValidationLayer::Edges,
                source_node: edge.source.clone(),
                available_nodes: workflow.node_ids().map(String::from).collect(),
            });
        }

        // Check target exists
        if !node_ids.contains(edge.target.as_str()) {
            errors.push(ValidationError::EdgeTargetNotFound {
                layer: ValidationLayer::Edges,
                target_node: edge.target.clone(),
                available_nodes: workflow.node_ids().map(String::from).collect(),
            });
        }

        // Check for self-loops
        if edge.source == edge.target {
            errors.push(ValidationError::SelfLoop {
                layer: ValidationLayer::Edges,
                id: edge.source.clone(),
            });
        }
    }

    errors
}

/// Layer 4: Validate paradigm connections (THE KEY RULE!)
///
/// This is the core Nika validation:
/// - 🤖 → 🧠 ❌ (isolated cannot connect to context)
/// - 🤖 → 🤖 ❌ (isolated cannot connect to isolated)
pub fn validate_paradigms(
    workflow: &Workflow,
    node_types: &NodeTypes,
    paradigm_matrix: &ParadigmMatrix,
) -> Vec<ValidationError> {
    let mut errors = Vec::new();

    for edge in &workflow.edges {
        // Get source and target nodes
        let source_node = match workflow.get_node(&edge.source) {
            Some(n) => n,
            None => continue, // Skip if edge validation will catch this
        };

        let target_node = match workflow.get_node(&edge.target) {
            Some(n) => n,
            None => continue, // Skip if edge validation will catch this
        };

        // Get paradigms for source and target
        let source_paradigm = match node_types.get_paradigm(&source_node.node_type) {
            Some(p) => p,
            None => continue, // Skip if node type validation will catch this
        };

        let target_paradigm = match node_types.get_paradigm(&target_node.node_type) {
            Some(p) => p,
            None => continue, // Skip if node type validation will catch this
        };

        // Check if connection is allowed
        if !paradigm_matrix.is_connection_allowed(source_paradigm, target_paradigm) {
            let source_symbol = paradigm_matrix.get_symbol(source_paradigm).unwrap_or("?");
            let target_symbol = paradigm_matrix.get_symbol(target_paradigm).unwrap_or("?");

            let suggestion = if source_paradigm == "isolated" && target_paradigm == "context" {
                format!(
                    "Use bridge pattern: {} {} → ⚡ [data node] → {} {}",
                    source_symbol, edge.source, target_symbol, edge.target
                )
            } else if source_paradigm == "isolated" && target_paradigm == "isolated" {
                "Isolated agents must be orchestrated by Main Agent, not each other".to_string()
            } else {
                format!(
                    "Connection from {} to {} is not allowed",
                    source_paradigm, target_paradigm
                )
            };

            errors.push(ValidationError::InvalidParadigmConnection {
                layer: ValidationLayer::Paradigms,
                source_id: edge.source.clone(),
                source_type: source_node.node_type.clone(),
                source_paradigm: format!("{} {}", source_symbol, source_paradigm),
                target_id: edge.target.clone(),
                target_type: target_node.node_type.clone(),
                target_paradigm: format!("{} {}", target_symbol, target_paradigm),
                suggestion,
            });
        }
    }

    errors
}

/// Layer 5: Validate graph structure
///
/// Checks for:
/// - Orphan nodes (no connections)
/// - Unreachable nodes (not reachable from entry)
/// - Cycles (warning, may be intentional)
pub fn validate_graph(workflow: &Workflow) -> Vec<ValidationError> {
    let mut warnings = Vec::new();

    if workflow.nodes.is_empty() {
        return warnings;
    }

    // Build adjacency lists
    let mut outgoing: HashMap<&str, Vec<&str>> = HashMap::new();
    let mut incoming: HashMap<&str, Vec<&str>> = HashMap::new();

    for node in &workflow.nodes {
        outgoing.entry(node.id.as_str()).or_default();
        incoming.entry(node.id.as_str()).or_default();
    }

    for edge in &workflow.edges {
        outgoing
            .entry(edge.source.as_str())
            .or_default()
            .push(edge.target.as_str());
        incoming
            .entry(edge.target.as_str())
            .or_default()
            .push(edge.source.as_str());
    }

    // Check for orphan nodes (no in or out edges)
    for node in &workflow.nodes {
        let has_outgoing = outgoing
            .get(node.id.as_str())
            .is_some_and(|v| !v.is_empty());
        let has_incoming = incoming
            .get(node.id.as_str())
            .is_some_and(|v| !v.is_empty());

        if !has_outgoing && !has_incoming {
            warnings.push(ValidationError::OrphanNode {
                layer: ValidationLayer::Graph,
                id: node.id.clone(),
                severity: Severity::Warning,
            });
        }
    }

    // Find entry points (nodes with no incoming edges)
    // Note: startNode is NOT part of the standard - it's a Studio visual marker.
    // Entry points are simply nodes with no incoming edges.
    let entry_points: Vec<&str> = workflow
        .nodes
        .iter()
        .filter(|n| incoming.get(n.id.as_str()).is_none_or(|v| v.is_empty()))
        .map(|n| n.id.as_str())
        .collect();

    // BFS to find reachable nodes
    let mut reachable = HashSet::new();
    let mut queue = VecDeque::new();

    for entry in &entry_points {
        queue.push_back(*entry);
        reachable.insert(*entry);
    }

    while let Some(node_id) = queue.pop_front() {
        if let Some(neighbors) = outgoing.get(node_id) {
            for neighbor in neighbors {
                if reachable.insert(*neighbor) {
                    queue.push_back(*neighbor);
                }
            }
        }
    }

    // Check for unreachable nodes
    for node in &workflow.nodes {
        if !reachable.contains(node.id.as_str()) && !entry_points.contains(&node.id.as_str()) {
            warnings.push(ValidationError::UnreachableNode {
                layer: ValidationLayer::Graph,
                id: node.id.clone(),
                severity: Severity::Warning,
            });
        }
    }

    // Detect cycles using DFS
    let mut visited = HashSet::new();
    let mut rec_stack = HashSet::new();
    let mut cycle_path = Vec::new();

    fn detect_cycle<'a>(
        node: &'a str,
        outgoing: &HashMap<&'a str, Vec<&'a str>>,
        visited: &mut HashSet<&'a str>,
        rec_stack: &mut HashSet<&'a str>,
        path: &mut Vec<&'a str>,
    ) -> Option<String> {
        visited.insert(node);
        rec_stack.insert(node);
        path.push(node);

        if let Some(neighbors) = outgoing.get(node) {
            for neighbor in neighbors {
                if !visited.contains(*neighbor) {
                    if let Some(cycle) = detect_cycle(neighbor, outgoing, visited, rec_stack, path)
                    {
                        return Some(cycle);
                    }
                } else if rec_stack.contains(*neighbor) {
                    // Found cycle - build path string
                    let cycle_start = path.iter().position(|&n| n == *neighbor).unwrap();
                    let cycle_nodes: Vec<&str> = path[cycle_start..].to_vec();
                    return Some(format!("{} → {}", cycle_nodes.join(" → "), neighbor));
                }
            }
        }

        rec_stack.remove(node);
        path.pop();
        None
    }

    for node in &workflow.nodes {
        if !visited.contains(node.id.as_str()) {
            if let Some(cycle) = detect_cycle(
                node.id.as_str(),
                &outgoing,
                &mut visited,
                &mut rec_stack,
                &mut cycle_path,
            ) {
                warnings.push(ValidationError::CycleDetected {
                    layer: ValidationLayer::Graph,
                    cycle_path: cycle,
                    severity: Severity::Warning,
                });
                break; // Only report first cycle found
            }
        }
    }

    warnings
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rules::{NodeTypes, ParadigmMatrix};
    use crate::workflow::Workflow;

    // Test YAML fixtures
    const PARADIGM_MATRIX_YAML: &str = r#"
version: "1.0"
description: "Test matrix"
paradigms:
  context:
    symbol: "🧠"
    description: "Context"
    color: "violet"
    border: "solid"
    sdk_mapping: "query()"
    token_cost: "500+"
  isolated:
    symbol: "🤖"
    description: "Isolated"
    color: "amber"
    border: "dashed"
    sdk_mapping: "agents"
    token_cost: "8000+"
  data:
    symbol: "⚡"
    description: "Data"
    color: "cyan"
    border: "thin"
    sdk_mapping: "@tool"
    token_cost: "0"
connections:
  context:
    context: true
    data: true
    isolated: true
  data:
    context: true
    data: true
    isolated: true
  isolated:
    context: false
    data: true
    isolated: false
"#;

    const NODE_TYPES_YAML: &str = r#"
version: "1.0"
description: "Test types"
lookup:
  context: context
  isolated: isolated
  data: data
  nika/transform: data
  nika/summarize: isolated
"#;

    fn make_matrix() -> ParadigmMatrix {
        ParadigmMatrix::from_yaml(PARADIGM_MATRIX_YAML).unwrap()
    }

    fn make_node_types() -> NodeTypes {
        NodeTypes::from_yaml(NODE_TYPES_YAML).unwrap()
    }

    // ========== Layer 2: Node Validation ==========

    #[test]
    fn test_validate_nodes_valid() {
        let workflow: Workflow = serde_yaml::from_str(
            r#"
mainAgent:
  model: "claude-sonnet-4-5"
  systemPrompt: "Test"
nodes:
  - id: prompt1
    type: context
  - id: transform1
    type: nika/transform
edges: []
"#,
        )
        .unwrap();

        let errors = validate_nodes(&workflow, &make_node_types());
        assert!(errors.is_empty(), "Expected no errors: {:?}", errors);
    }

    #[test]
    fn test_validate_nodes_duplicate_id() {
        let workflow: Workflow = serde_yaml::from_str(
            r#"
mainAgent:
  model: "claude-sonnet-4-5"
  systemPrompt: "Test"
nodes:
  - id: myNode
    type: context
  - id: myNode
    type: nika/transform
edges: []
"#,
        )
        .unwrap();

        let errors = validate_nodes(&workflow, &make_node_types());
        assert_eq!(errors.len(), 1);
        assert!(matches!(
            &errors[0],
            ValidationError::DuplicateNodeId { id, .. } if id == "myNode"
        ));
    }

    #[test]
    fn test_validate_nodes_invalid_id_format() {
        let workflow: Workflow = serde_yaml::from_str(
            r#"
mainAgent:
  model: "claude-sonnet-4-5"
  systemPrompt: "Test"
nodes:
  - id: 123invalid
    type: context
edges: []
"#,
        )
        .unwrap();

        let errors = validate_nodes(&workflow, &make_node_types());
        assert_eq!(errors.len(), 1);
        assert!(matches!(
            &errors[0],
            ValidationError::InvalidNodeIdFormat { id, .. } if id == "123invalid"
        ));
    }

    #[test]
    fn test_validate_nodes_unknown_type() {
        let workflow: Workflow = serde_yaml::from_str(
            r#"
mainAgent:
  model: "claude-sonnet-4-5"
  systemPrompt: "Test"
nodes:
  - id: node1
    type: madeUpNode
edges: []
"#,
        )
        .unwrap();

        let errors = validate_nodes(&workflow, &make_node_types());
        assert_eq!(errors.len(), 1);
        assert!(matches!(
            &errors[0],
            ValidationError::UnknownNodeType { node_type, .. } if node_type == "madeUpNode"
        ));
    }

    // ========== Layer 3: Edge Validation ==========

    #[test]
    fn test_validate_edges_valid() {
        let workflow: Workflow = serde_yaml::from_str(
            r#"
mainAgent:
  model: "claude-sonnet-4-5"
  systemPrompt: "Test"
nodes:
  - id: prompt1
    type: context
  - id: transform1
    type: nika/transform
edges:
  - source: prompt1
    target: transform1
"#,
        )
        .unwrap();

        let errors = validate_edges(&workflow);
        assert!(errors.is_empty());
    }

    #[test]
    fn test_validate_edges_missing_source() {
        let workflow: Workflow = serde_yaml::from_str(
            r#"
mainAgent:
  model: "claude-sonnet-4-5"
  systemPrompt: "Test"
nodes:
  - id: transform1
    type: nika/transform
edges:
  - source: nonexistent
    target: transform1
"#,
        )
        .unwrap();

        let errors = validate_edges(&workflow);
        assert_eq!(errors.len(), 1);
        assert!(matches!(
            &errors[0],
            ValidationError::EdgeSourceNotFound { source_node, .. } if source_node == "nonexistent"
        ));
    }

    #[test]
    fn test_validate_edges_self_loop() {
        let workflow: Workflow = serde_yaml::from_str(
            r#"
mainAgent:
  model: "claude-sonnet-4-5"
  systemPrompt: "Test"
nodes:
  - id: node1
    type: context
edges:
  - source: node1
    target: node1
"#,
        )
        .unwrap();

        let errors = validate_edges(&workflow);
        assert_eq!(errors.len(), 1);
        assert!(matches!(&errors[0], ValidationError::SelfLoop { id, .. } if id == "node1"));
    }

    // ========== Layer 4: Paradigm Validation (THE KEY TESTS!) ==========

    #[test]
    fn test_validate_paradigms_context_to_context() {
        // 🧠 → 🧠 ✅
        let workflow: Workflow = serde_yaml::from_str(
            r#"
mainAgent:
  model: "claude-sonnet-4-5"
  systemPrompt: "Test"
nodes:
  - id: prompt1
    type: context
  - id: prompt2
    type: context
edges:
  - source: prompt1
    target: prompt2
"#,
        )
        .unwrap();

        let errors = validate_paradigms(&workflow, &make_node_types(), &make_matrix());
        assert!(errors.is_empty(), "🧠 → 🧠 should be valid");
    }

    #[test]
    fn test_validate_paradigms_data_to_context() {
        // ⚡ → 🧠 ✅
        let workflow: Workflow = serde_yaml::from_str(
            r#"
mainAgent:
  model: "claude-sonnet-4-5"
  systemPrompt: "Test"
nodes:
  - id: transform1
    type: nika/transform
  - id: prompt1
    type: context
edges:
  - source: transform1
    target: prompt1
"#,
        )
        .unwrap();

        let errors = validate_paradigms(&workflow, &make_node_types(), &make_matrix());
        assert!(errors.is_empty(), "⚡ → 🧠 should be valid");
    }

    #[test]
    fn test_validate_paradigms_isolated_to_data() {
        // 🤖 → ⚡ ✅ (Bridge pattern step 1)
        let workflow: Workflow = serde_yaml::from_str(
            r#"
mainAgent:
  model: "claude-sonnet-4-5"
  systemPrompt: "Test"
nodes:
  - id: expert1
    type: isolated
  - id: transform1
    type: nika/transform
edges:
  - source: expert1
    target: transform1
"#,
        )
        .unwrap();

        let errors = validate_paradigms(&workflow, &make_node_types(), &make_matrix());
        assert!(
            errors.is_empty(),
            "🤖 → ⚡ should be valid (bridge pattern)"
        );
    }

    #[test]
    fn test_validate_paradigms_isolated_to_context_invalid() {
        // 🤖 → 🧠 ❌ (THE KEY RULE!)
        let workflow: Workflow = serde_yaml::from_str(
            r#"
mainAgent:
  model: "claude-sonnet-4-5"
  systemPrompt: "Test"
nodes:
  - id: expert1
    type: isolated
  - id: prompt1
    type: context
edges:
  - source: expert1
    target: prompt1
"#,
        )
        .unwrap();

        let errors = validate_paradigms(&workflow, &make_node_types(), &make_matrix());
        assert_eq!(errors.len(), 1, "🤖 → 🧠 should be INVALID");

        match &errors[0] {
            ValidationError::InvalidParadigmConnection {
                source_type,
                target_type,
                suggestion,
                ..
            } => {
                assert_eq!(source_type, "isolated");
                assert_eq!(target_type, "context");
                assert!(suggestion.contains("bridge pattern"));
            }
            _ => panic!("Expected InvalidParadigmConnection"),
        }
    }

    #[test]
    fn test_validate_paradigms_isolated_to_isolated_invalid() {
        // 🤖 → 🤖 ❌
        let workflow: Workflow = serde_yaml::from_str(
            r#"
mainAgent:
  model: "claude-sonnet-4-5"
  systemPrompt: "Test"
nodes:
  - id: expert1
    type: isolated
  - id: expert2
    type: nika/summarize
edges:
  - source: expert1
    target: expert2
"#,
        )
        .unwrap();

        let errors = validate_paradigms(&workflow, &make_node_types(), &make_matrix());
        assert_eq!(errors.len(), 1, "🤖 → 🤖 should be INVALID");

        match &errors[0] {
            ValidationError::InvalidParadigmConnection { suggestion, .. } => {
                assert!(suggestion.contains("orchestrated by Main Agent"));
            }
            _ => panic!("Expected InvalidParadigmConnection"),
        }
    }

    #[test]
    fn test_validate_paradigms_bridge_pattern_valid() {
        // Full bridge pattern: 🤖 → ⚡ → 🧠 ✅
        let workflow: Workflow = serde_yaml::from_str(
            r#"
mainAgent:
  model: "claude-sonnet-4-5"
  systemPrompt: "Test"
nodes:
  - id: expert1
    type: isolated
  - id: bridge
    type: nika/transform
  - id: prompt1
    type: context
edges:
  - source: expert1
    target: bridge
  - source: bridge
    target: prompt1
"#,
        )
        .unwrap();

        let errors = validate_paradigms(&workflow, &make_node_types(), &make_matrix());
        assert!(
            errors.is_empty(),
            "Bridge pattern 🤖 → ⚡ → 🧠 should be valid"
        );
    }

    // ========== Layer 5: Graph Validation ==========

    #[test]
    fn test_validate_graph_no_warnings() {
        let workflow: Workflow = serde_yaml::from_str(
            r#"
mainAgent:
  model: "claude-sonnet-4-5"
  systemPrompt: "Test"
nodes:
  - id: prompt1
    type: context
  - id: transform1
    type: nika/transform
edges:
  - source: prompt1
    target: transform1
"#,
        )
        .unwrap();

        let warnings = validate_graph(&workflow);
        assert!(warnings.is_empty(), "Expected no warnings: {:?}", warnings);
    }

    #[test]
    fn test_validate_graph_orphan_node() {
        let workflow: Workflow = serde_yaml::from_str(
            r#"
mainAgent:
  model: "claude-sonnet-4-5"
  systemPrompt: "Test"
nodes:
  - id: connected1
    type: context
  - id: connected2
    type: nika/transform
  - id: orphan
    type: data
edges:
  - source: connected1
    target: connected2
"#,
        )
        .unwrap();

        let warnings = validate_graph(&workflow);
        assert_eq!(warnings.len(), 1);
        assert!(matches!(
            &warnings[0],
            ValidationError::OrphanNode { id, .. } if id == "orphan"
        ));
    }

    #[test]
    fn test_validate_graph_cycle_detected() {
        let workflow: Workflow = serde_yaml::from_str(
            r#"
mainAgent:
  model: "claude-sonnet-4-5"
  systemPrompt: "Test"
nodes:
  - id: a
    type: context
  - id: b
    type: nika/transform
  - id: c
    type: data
edges:
  - source: a
    target: b
  - source: b
    target: c
  - source: c
    target: a
"#,
        )
        .unwrap();

        let warnings = validate_graph(&workflow);
        assert!(
            warnings
                .iter()
                .any(|w| matches!(w, ValidationError::CycleDetected { .. })),
            "Should detect cycle"
        );
    }

    #[test]
    fn test_validate_graph_empty_workflow() {
        let workflow: Workflow = serde_yaml::from_str(
            r#"
mainAgent:
  model: "claude-sonnet-4-5"
  systemPrompt: "Test"
nodes: []
edges: []
"#,
        )
        .unwrap();

        let warnings = validate_graph(&workflow);
        assert!(
            warnings.is_empty(),
            "Empty workflow should have no warnings"
        );
    }
}
