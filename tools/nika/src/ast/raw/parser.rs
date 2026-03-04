//! YAML parser with span tracking using marked_yaml.
//!
//! This module provides the entry point for parsing YAML workflows
//! into the raw AST with full source position tracking.

use marked_yaml::{parse_yaml, Node, Marker, Span as MarkedSpan, LoadError};

use crate::source::{FileId, Span, ByteOffset, Spanned};
use super::workflow::RawWorkflow;
use super::task::RawTask;

/// Errors that can occur during parsing.
#[derive(Debug)]
pub struct ParseError {
    /// The error kind
    pub kind: ParseErrorKind,
    /// Location of the error (if available)
    pub span: Span,
    /// Error message
    pub message: String,
}

/// Kinds of parse errors.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ParseErrorKind {
    /// YAML syntax error
    Syntax,
    /// Missing required field
    MissingField,
    /// Invalid field type
    InvalidType,
    /// Unknown field
    UnknownField,
    /// Invalid schema version
    InvalidSchema,
}

impl std::fmt::Display for ParseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.message)
    }
}

impl std::error::Error for ParseError {}

/// Convert a marked_yaml Marker to our ByteOffset.
fn marker_to_offset(marker: &Marker) -> ByteOffset {
    // marked_yaml uses character() for byte offset
    ByteOffset::new(marker.character() as u32)
}

/// Convert a marked_yaml Marker to a point Span.
fn marker_to_span(file: FileId, marker: &Marker) -> Span {
    let offset = marker_to_offset(marker);
    Span {
        file,
        start: offset,
        end: offset,
    }
}

/// Extract span from a LoadError if it carries a Marker.
///
/// Most LoadError variants include position information:
/// - TopLevelMustBeMapping(Marker)
/// - TopLevelMustBeSequence(Marker)
/// - UnexpectedAnchor(Marker)
/// - MappingKeyMustBeScalar(Marker)
/// - UnexpectedTag(Marker)
/// - ScanError(Marker, _)
/// - DuplicateKey(Box<DuplicateKeyInner>)
fn extract_span_from_load_error(file: FileId, error: &LoadError) -> Span {
    match error {
        LoadError::TopLevelMustBeMapping(marker)
        | LoadError::TopLevelMustBeSequence(marker)
        | LoadError::UnexpectedAnchor(marker)
        | LoadError::MappingKeyMustBeScalar(marker)
        | LoadError::UnexpectedTag(marker) => marker_to_span(file, marker),
        LoadError::ScanError(marker, _) => marker_to_span(file, marker),
        LoadError::DuplicateKey(inner) => {
            // DuplicateKeyInner has key: MarkedScalarNode, use its span
            marked_span_to_span(file, inner.key.span())
        }
    }
}

/// Convert a marked_yaml Span to our Span.
fn marked_span_to_span(file: FileId, span: &MarkedSpan) -> Span {
    match (span.start(), span.end()) {
        (Some(start), Some(end)) => Span {
            file,
            start: marker_to_offset(start),
            end: marker_to_offset(end),
        },
        (Some(start), None) => Span {
            file,
            start: marker_to_offset(start),
            end: marker_to_offset(start), // Point span
        },
        _ => Span::dummy(),
    }
}

/// Convert a marked_yaml Node span to our Span.
fn node_to_span(file: FileId, node: &Node) -> Span {
    marked_span_to_span(file, node.span())
}

/// Extract a spanned string from a YAML scalar node.
fn extract_string(file: FileId, node: &Node) -> Result<Spanned<String>, ParseError> {
    let span = node_to_span(file, node);
    match node {
        Node::Scalar(s) => Ok(Spanned::new(s.to_string(), span)),
        _ => Err(ParseError {
            kind: ParseErrorKind::InvalidType,
            span,
            message: "expected string".to_string(),
        }),
    }
}

/// Get an optional string field from a mapping by key.
fn get_string_field(
    file: FileId,
    map: &marked_yaml::types::MarkedMappingNode,
    key: &str,
) -> Result<Option<Spanned<String>>, ParseError> {
    match map.get_node(key) {
        Some(node) => extract_string(file, node).map(Some),
        None => Ok(None),
    }
}

/// Parse a YAML source string into a RawWorkflow.
///
/// This is the main entry point for Phase 1 parsing. The returned
/// RawWorkflow contains full span information for all nodes.
///
/// # Arguments
///
/// * `source` - The YAML source content
/// * `file_id` - The FileId from the SourceRegistry
///
/// # Returns
///
/// A RawWorkflow with span tracking, or a ParseError with location.
///
/// # Example
///
/// ```ignore
/// use nika::ast::raw;
/// use nika::source::SourceRegistry;
///
/// let mut sources = SourceRegistry::new();
/// let file_id = sources.add_file("workflow.yaml", content.clone());
/// let workflow = raw::parse(&content, file_id)?;
/// ```
pub fn parse(source: &str, file_id: FileId) -> Result<RawWorkflow, ParseError> {
    // Parse YAML with marked_yaml
    // The first argument is a source ID (we use file_id.0)
    let node = parse_yaml(file_id.0 as usize, source).map_err(|e| {
        // Extract span from LoadError variants that carry a Marker
        let span = extract_span_from_load_error(file_id, &e);
        ParseError {
            kind: ParseErrorKind::Syntax,
            span,
            message: format!("YAML syntax error: {}", e),
        }
    })?;

    // The root must be a mapping
    let map = match &node {
        Node::Mapping(m) => m,
        _ => {
            return Err(ParseError {
                kind: ParseErrorKind::InvalidType,
                span: node_to_span(file_id, &node),
                message: "workflow must be a YAML mapping".to_string(),
            });
        }
    };

    // Parse workflow fields
    let mut workflow = RawWorkflow::default();
    workflow.span = node_to_span(file_id, &node);

    // Extract schema (required)
    workflow.schema = get_string_field(file_id, map, "schema")?
        .ok_or_else(|| ParseError {
            kind: ParseErrorKind::MissingField,
            span: workflow.span,
            message: "missing required field 'schema'".to_string(),
        })?;

    // Extract optional fields
    workflow.workflow = get_string_field(file_id, map, "workflow")?;
    workflow.description = get_string_field(file_id, map, "description")?;
    workflow.provider = get_string_field(file_id, map, "provider")?;
    workflow.model = get_string_field(file_id, map, "model")?;

    // Parse tasks
    workflow.tasks = parse_tasks(file_id, map)?;

    Ok(workflow)
}

/// Parse the tasks array from a workflow mapping.
fn parse_tasks(
    file_id: FileId,
    map: &marked_yaml::types::MarkedMappingNode,
) -> Result<Spanned<Vec<Spanned<RawTask>>>, ParseError> {
    match map.get_node("tasks") {
        Some(Node::Sequence(seq)) => {
            let span = marked_span_to_span(file_id, seq.span());
            let tasks = seq.iter()
                .map(|task_node| parse_task(file_id, task_node))
                .collect::<Result<Vec<_>, _>>()?;
            Ok(Spanned::new(tasks, span))
        }
        Some(node) => {
            Err(ParseError {
                kind: ParseErrorKind::InvalidType,
                span: node_to_span(file_id, node),
                message: "tasks must be a sequence".to_string(),
            })
        }
        None => {
            // No tasks field - return empty array with dummy span
            Ok(Spanned::dummy(Vec::new()))
        }
    }
}

/// Parse a single task from a YAML node.
fn parse_task(file_id: FileId, node: &Node) -> Result<Spanned<RawTask>, ParseError> {
    let span = node_to_span(file_id, node);

    let map = match node {
        Node::Mapping(m) => m,
        _ => {
            return Err(ParseError {
                kind: ParseErrorKind::InvalidType,
                span,
                message: "task must be a mapping".to_string(),
            });
        }
    };

    let mut task = RawTask::default();
    task.span = span;

    // Extract task id (required)
    task.id = get_string_field(file_id, map, "id")?
        .ok_or_else(|| ParseError {
            kind: ParseErrorKind::MissingField,
            span,
            message: "task missing required field 'id'".to_string(),
        })?;

    // Extract optional fields
    task.description = get_string_field(file_id, map, "description")?;
    task.provider = get_string_field(file_id, map, "provider")?;
    task.model = get_string_field(file_id, map, "model")?;

    // Note: Full implementation would also parse:
    // - action (infer/exec/fetch/invoke/agent)
    // - use:
    // - flow:
    // - output:
    // - for_each:
    // - retry:
    // This is a foundation that can be extended.

    Ok(Spanned::new(task, span))
}

#[cfg(test)]
mod tests {
    use super::*;

    const SIMPLE_WORKFLOW: &str = r#"
schema: "nika/workflow@0.10"
workflow: test-workflow
description: "A test workflow"
provider: claude
model: claude-sonnet-4-6

tasks:
  - id: task1
    description: "First task"

  - id: task2
    description: "Second task"
"#;

    #[test]
    fn test_parse_simple_workflow() {
        let file_id = FileId(0);
        let result = parse(SIMPLE_WORKFLOW, file_id);

        assert!(result.is_ok(), "Parse failed: {:?}", result.err());
        let workflow = result.unwrap();

        assert_eq!(workflow.schema.value, "nika/workflow@0.10");
        assert_eq!(workflow.name(), "test-workflow");
        assert_eq!(workflow.description.as_ref().unwrap().value, "A test workflow");
        assert_eq!(workflow.provider.as_ref().unwrap().value, "claude");
        assert_eq!(workflow.model.as_ref().unwrap().value, "claude-sonnet-4-6");
        assert_eq!(workflow.task_count(), 2);

        // Check spans are not dummy
        assert!(!workflow.schema.span.is_dummy());
        assert!(!workflow.tasks.span.is_dummy());
    }

    #[test]
    fn test_parse_task_ids() {
        let file_id = FileId(0);
        let workflow = parse(SIMPLE_WORKFLOW, file_id).unwrap();

        let task1 = workflow.get_task("task1");
        assert!(task1.is_some());
        assert_eq!(task1.unwrap().value.id.value, "task1");

        let task2 = workflow.get_task("task2");
        assert!(task2.is_some());
        assert_eq!(task2.unwrap().value.id.value, "task2");
    }

    #[test]
    fn test_parse_missing_schema() {
        let yaml = r#"
workflow: test
tasks: []
"#;
        let result = parse(yaml, FileId(0));
        assert!(result.is_err());

        let err = result.unwrap_err();
        assert_eq!(err.kind, ParseErrorKind::MissingField);
        assert!(err.message.contains("schema"));
    }

    #[test]
    fn test_parse_invalid_yaml() {
        let yaml = "invalid: yaml: syntax: [";
        let result = parse(yaml, FileId(0));
        assert!(result.is_err());

        let err = result.unwrap_err();
        assert_eq!(err.kind, ParseErrorKind::Syntax);
    }

    #[test]
    fn test_span_tracking() {
        let yaml = r#"schema: "nika/workflow@0.10"
workflow: my-workflow
tasks:
  - id: hello
"#;
        let file_id = FileId(0);
        let workflow = parse(yaml, file_id).unwrap();

        // Check that schema has correct span
        let schema_span = workflow.schema.span;
        assert!(!schema_span.is_dummy());

        // Check span bounds are reasonable
        assert!(schema_span.start.0 < schema_span.end.0 || schema_span.start.0 == schema_span.end.0);
    }
}
