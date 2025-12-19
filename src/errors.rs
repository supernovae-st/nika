//! Validation error types
//!
//! Structured errors for each validation layer, designed for
//! helpful error messages with suggestions.

use thiserror::Error;

/// Validation layer (1-5)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ValidationLayer {
    Schema = 1,
    Nodes = 2,
    Edges = 3,
    Paradigms = 4,
    Graph = 5,
}

impl std::fmt::Display for ValidationLayer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ValidationLayer::Schema => write!(f, "Schema"),
            ValidationLayer::Nodes => write!(f, "Nodes"),
            ValidationLayer::Edges => write!(f, "Edges"),
            ValidationLayer::Paradigms => write!(f, "Paradigms"),
            ValidationLayer::Graph => write!(f, "Graph"),
        }
    }
}

/// Severity of validation issue
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Severity {
    Error,
    Warning,
}

/// A single validation error with context
#[derive(Debug, Error)]
pub enum ValidationError {
    // Layer 1: Schema errors
    #[error("Missing required field: {field}")]
    MissingField {
        layer: ValidationLayer,
        field: String,
        suggestion: String,
    },

    #[error("Invalid field type for '{field}': expected {expected}")]
    InvalidFieldType {
        layer: ValidationLayer,
        field: String,
        expected: String,
    },

    // Layer 2: Node errors
    #[error("Duplicate node ID: '{id}'")]
    DuplicateNodeId { layer: ValidationLayer, id: String },

    #[error("Invalid node ID format: '{id}'")]
    InvalidNodeIdFormat {
        layer: ValidationLayer,
        id: String,
        suggestion: String,
    },

    #[error("Unknown node type: '{node_type}'")]
    UnknownNodeType {
        layer: ValidationLayer,
        node_type: String,
        suggestions: Vec<String>,
    },

    #[error(
        "Visual-only node type: '{node_type}' (Studio use only, not part of execution standard)"
    )]
    VisualNodeType {
        layer: ValidationLayer,
        node_type: String,
        severity: Severity,
    },

    // Layer 3: Edge errors
    #[error("Edge source '{source_node}' does not exist")]
    EdgeSourceNotFound {
        layer: ValidationLayer,
        source_node: String,
        available_nodes: Vec<String>,
    },

    #[error("Edge target '{target_node}' does not exist")]
    EdgeTargetNotFound {
        layer: ValidationLayer,
        target_node: String,
        available_nodes: Vec<String>,
    },

    #[error("Self-loop detected: node '{id}' connects to itself")]
    SelfLoop { layer: ValidationLayer, id: String },

    // Layer 4: Paradigm errors (THE KEY ERRORS!)
    #[error(
        "Invalid connection: {source_type} ({source_paradigm}) → {target_type} ({target_paradigm})"
    )]
    InvalidParadigmConnection {
        layer: ValidationLayer,
        source_id: String,
        source_type: String,
        source_paradigm: String,
        target_id: String,
        target_type: String,
        target_paradigm: String,
        suggestion: String,
    },

    // Layer 5: Graph warnings
    #[error("Orphan node '{id}' has no connections")]
    OrphanNode {
        layer: ValidationLayer,
        id: String,
        severity: Severity,
    },

    #[error("Node '{id}' is not reachable from workflow entry")]
    UnreachableNode {
        layer: ValidationLayer,
        id: String,
        severity: Severity,
    },

    #[error("Cycle detected: {cycle_path}")]
    CycleDetected {
        layer: ValidationLayer,
        cycle_path: String,
        severity: Severity,
    },

    // Custom node validation errors
    #[error("Custom node '{name}' has invalid 'extends': '{extends}'. Must be CORE primitive (context, isolated, data)")]
    InvalidCustomNodeExtends {
        layer: ValidationLayer,
        name: String,
        extends: String,
    },
}

impl ValidationError {
    /// Get the validation layer for this error
    pub fn layer(&self) -> ValidationLayer {
        match self {
            ValidationError::MissingField { layer, .. } => *layer,
            ValidationError::InvalidFieldType { layer, .. } => *layer,
            ValidationError::DuplicateNodeId { layer, .. } => *layer,
            ValidationError::InvalidNodeIdFormat { layer, .. } => *layer,
            ValidationError::UnknownNodeType { layer, .. } => *layer,
            ValidationError::VisualNodeType { layer, .. } => *layer,
            ValidationError::EdgeSourceNotFound { layer, .. } => *layer,
            ValidationError::EdgeTargetNotFound { layer, .. } => *layer,
            ValidationError::SelfLoop { layer, .. } => *layer,
            ValidationError::InvalidParadigmConnection { layer, .. } => *layer,
            ValidationError::OrphanNode { layer, .. } => *layer,
            ValidationError::UnreachableNode { layer, .. } => *layer,
            ValidationError::CycleDetected { layer, .. } => *layer,
            ValidationError::InvalidCustomNodeExtends { layer, .. } => *layer,
        }
    }

    /// Get severity (error vs warning)
    pub fn severity(&self) -> Severity {
        match self {
            ValidationError::OrphanNode { severity, .. } => *severity,
            ValidationError::UnreachableNode { severity, .. } => *severity,
            ValidationError::CycleDetected { severity, .. } => *severity,
            ValidationError::VisualNodeType { severity, .. } => *severity,
            _ => Severity::Error,
        }
    }

    /// Get suggestion for fixing this error
    pub fn suggestion(&self) -> Option<String> {
        match self {
            ValidationError::MissingField { suggestion, .. } => Some(suggestion.clone()),
            ValidationError::InvalidNodeIdFormat { suggestion, .. } => Some(suggestion.clone()),
            ValidationError::InvalidParadigmConnection { suggestion, .. } => Some(suggestion.clone()),
            ValidationError::UnknownNodeType { suggestions, .. } => {
                if suggestions.is_empty() {
                    None
                } else {
                    Some(format!("Did you mean: {}?", suggestions.join(", ")))
                }
            }
            ValidationError::EdgeSourceNotFound { available_nodes, .. } => {
                if available_nodes.is_empty() {
                    Some("No nodes available in workflow".to_string())
                } else if available_nodes.len() <= 5 {
                    Some(format!("Available nodes: {}", available_nodes.join(", ")))
                } else {
                    Some(format!("Available nodes: {} (and {} more)",
                        available_nodes[..3].join(", "),
                        available_nodes.len() - 3))
                }
            }
            ValidationError::EdgeTargetNotFound { available_nodes, .. } => {
                if available_nodes.is_empty() {
                    Some("No nodes available in workflow".to_string())
                } else if available_nodes.len() <= 5 {
                    Some(format!("Available nodes: {}", available_nodes.join(", ")))
                } else {
                    Some(format!("Available nodes: {} (and {} more)",
                        available_nodes[..3].join(", "),
                        available_nodes.len() - 3))
                }
            }
            _ => None,
        }
    }
}

/// Result of validating a workflow file
#[derive(Debug)]
pub struct ValidationResult {
    pub file_path: String,
    pub node_count: usize,
    pub edge_count: usize,
    pub errors: Vec<ValidationError>,
    pub warnings: Vec<ValidationError>,
}

impl ValidationResult {
    pub fn new(file_path: impl Into<String>) -> Self {
        Self {
            file_path: file_path.into(),
            node_count: 0,
            edge_count: 0,
            errors: Vec::new(),
            warnings: Vec::new(),
        }
    }

    pub fn is_valid(&self) -> bool {
        self.errors.is_empty()
    }

    pub fn has_warnings(&self) -> bool {
        !self.warnings.is_empty()
    }

    pub fn add_error(&mut self, error: ValidationError) {
        if error.severity() == Severity::Warning {
            self.warnings.push(error);
        } else {
            self.errors.push(error);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_validation_layer_display() {
        assert_eq!(format!("{}", ValidationLayer::Schema), "Schema");
        assert_eq!(format!("{}", ValidationLayer::Paradigms), "Paradigms");
    }

    #[test]
    fn test_error_layer() {
        let error = ValidationError::MissingField {
            layer: ValidationLayer::Schema,
            field: "mainAgent".to_string(),
            suggestion: "Add mainAgent section".to_string(),
        };
        assert_eq!(error.layer(), ValidationLayer::Schema);
    }

    #[test]
    fn test_error_severity() {
        let error = ValidationError::DuplicateNodeId {
            layer: ValidationLayer::Nodes,
            id: "test".to_string(),
        };
        assert_eq!(error.severity(), Severity::Error);

        let warning = ValidationError::OrphanNode {
            layer: ValidationLayer::Graph,
            id: "orphan".to_string(),
            severity: Severity::Warning,
        };
        assert_eq!(warning.severity(), Severity::Warning);
    }

    #[test]
    fn test_paradigm_error_message() {
        let error = ValidationError::InvalidParadigmConnection {
            layer: ValidationLayer::Paradigms,
            source_id: "expert1".to_string(),
            source_type: "isolated".to_string(),
            source_paradigm: "🤖 isolated".to_string(),
            target_id: "prompt1".to_string(),
            target_type: "context".to_string(),
            target_paradigm: "🧠 context".to_string(),
            suggestion: "Use bridge pattern: 🤖 → ⚡ → 🧠".to_string(),
        };

        let msg = format!("{}", error);
        assert!(msg.contains("isolated"));
        assert!(msg.contains("context"));
    }

    #[test]
    fn test_validation_result() {
        let mut result = ValidationResult::new("test.nika.yaml");
        assert!(result.is_valid());
        assert!(!result.has_warnings());

        result.add_error(ValidationError::DuplicateNodeId {
            layer: ValidationLayer::Nodes,
            id: "dup".to_string(),
        });
        assert!(!result.is_valid());

        result.add_error(ValidationError::OrphanNode {
            layer: ValidationLayer::Graph,
            id: "orphan".to_string(),
            severity: Severity::Warning,
        });
        assert!(result.has_warnings());
        assert_eq!(result.errors.len(), 1);
        assert_eq!(result.warnings.len(), 1);
    }

    #[test]
    fn test_error_suggestion() {
        let error = ValidationError::InvalidParadigmConnection {
            layer: ValidationLayer::Paradigms,
            source_id: "a".to_string(),
            source_type: "isolated".to_string(),
            source_paradigm: "isolated".to_string(),
            target_id: "b".to_string(),
            target_type: "context".to_string(),
            target_paradigm: "context".to_string(),
            suggestion: "Use bridge pattern: 🤖 → ⚡ → 🧠".to_string(),
        };

        assert_eq!(error.suggestion(), Some("Use bridge pattern: 🤖 → ⚡ → 🧠".to_string()));
    }

    #[test]
    fn test_unknown_type_suggestion() {
        let error = ValidationError::UnknownNodeType {
            layer: ValidationLayer::Nodes,
            node_type: "promtNode".to_string(),
            suggestions: vec!["promptNode".to_string(), "context".to_string()],
        };

        assert_eq!(error.suggestion(), Some("Did you mean: promptNode, context?".to_string()));
    }

    #[test]
    fn test_edge_not_found_suggestion() {
        let error = ValidationError::EdgeSourceNotFound {
            layer: ValidationLayer::Edges,
            source_node: "missing".to_string(),
            available_nodes: vec!["node1".to_string(), "node2".to_string()],
        };

        assert_eq!(error.suggestion(), Some("Available nodes: node1, node2".to_string()));
    }

    #[test]
    fn test_invalid_custom_node_extends() {
        let error = ValidationError::InvalidCustomNodeExtends {
            layer: ValidationLayer::Nodes,
            name: "slackNode".to_string(),
            extends: "routerNode".to_string(),
        };

        assert_eq!(error.layer(), ValidationLayer::Nodes);
        assert_eq!(error.severity(), Severity::Error);
        let msg = format!("{}", error);
        assert!(msg.contains("slackNode"));
        assert!(msg.contains("routerNode"));
        assert!(msg.contains("CORE primitive"));
    }
}
