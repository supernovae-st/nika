//! YAML parser with span tracking using marked_yaml.
//!
//! This module provides the entry point for parsing YAML workflows
//! into the raw AST with full source position tracking.

use indexmap::IndexMap;
use marked_yaml::{parse_yaml, LoadError, Marker, Node, Span as MarkedSpan};

use super::action::{
    RawAgentAction, RawExecAction, RawFetchAction, RawInferAction, RawInvokeAction, RawTaskAction,
};
use super::mcp::{RawMcpConfig, RawMcpServer};
use super::task::{RawFlow, RawForEach, RawOutputConfig, RawRetryConfig, RawTask, RawUseTarget};
use super::workflow::RawWorkflow;
use crate::source::{ByteOffset, FileId, Span, Spanned};

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

/// Get an optional f64 field from a mapping.
fn get_f64_field(
    file: FileId,
    map: &marked_yaml::types::MarkedMappingNode,
    key: &str,
) -> Result<Option<Spanned<f64>>, ParseError> {
    match map.get_node(key) {
        Some(Node::Scalar(s)) => {
            let span = marked_span_to_span(file, s.span());
            let value: f64 = s.as_str().parse().map_err(|_| ParseError {
                kind: ParseErrorKind::InvalidType,
                span,
                message: format!("'{}' must be a number", key),
            })?;
            Ok(Some(Spanned::new(value, span)))
        }
        Some(node) => Err(ParseError {
            kind: ParseErrorKind::InvalidType,
            span: node_to_span(file, node),
            message: format!("'{}' must be a number", key),
        }),
        None => Ok(None),
    }
}

/// Get an optional u32 field from a mapping.
fn get_u32_field(
    file: FileId,
    map: &marked_yaml::types::MarkedMappingNode,
    key: &str,
) -> Result<Option<Spanned<u32>>, ParseError> {
    match map.get_node(key) {
        Some(Node::Scalar(s)) => {
            let span = marked_span_to_span(file, s.span());
            let value: u32 = s.as_str().parse().map_err(|_| ParseError {
                kind: ParseErrorKind::InvalidType,
                span,
                message: format!("'{}' must be a positive integer", key),
            })?;
            Ok(Some(Spanned::new(value, span)))
        }
        Some(node) => Err(ParseError {
            kind: ParseErrorKind::InvalidType,
            span: node_to_span(file, node),
            message: format!("'{}' must be a positive integer", key),
        }),
        None => Ok(None),
    }
}

/// Get an optional u64 field from a mapping.
fn get_u64_field(
    file: FileId,
    map: &marked_yaml::types::MarkedMappingNode,
    key: &str,
) -> Result<Option<Spanned<u64>>, ParseError> {
    match map.get_node(key) {
        Some(Node::Scalar(s)) => {
            let span = marked_span_to_span(file, s.span());
            let value: u64 = s.as_str().parse().map_err(|_| ParseError {
                kind: ParseErrorKind::InvalidType,
                span,
                message: format!("'{}' must be a positive integer", key),
            })?;
            Ok(Some(Spanned::new(value, span)))
        }
        Some(node) => Err(ParseError {
            kind: ParseErrorKind::InvalidType,
            span: node_to_span(file, node),
            message: format!("'{}' must be a positive integer", key),
        }),
        None => Ok(None),
    }
}

/// Get an optional bool field from a mapping.
fn get_bool_field(
    file: FileId,
    map: &marked_yaml::types::MarkedMappingNode,
    key: &str,
) -> Result<Option<Spanned<bool>>, ParseError> {
    match map.get_node(key) {
        Some(Node::Scalar(s)) => {
            let span = marked_span_to_span(file, s.span());
            let value = match s.as_str().to_lowercase().as_str() {
                "true" | "yes" | "on" | "1" => true,
                "false" | "no" | "off" | "0" => false,
                _ => {
                    return Err(ParseError {
                        kind: ParseErrorKind::InvalidType,
                        span,
                        message: format!("'{}' must be a boolean", key),
                    });
                }
            };
            Ok(Some(Spanned::new(value, span)))
        }
        Some(node) => Err(ParseError {
            kind: ParseErrorKind::InvalidType,
            span: node_to_span(file, node),
            message: format!("'{}' must be a boolean", key),
        }),
        None => Ok(None),
    }
}

/// Parse a string-to-string mapping (for headers, env vars).
#[allow(clippy::type_complexity)]
fn parse_string_map(
    file: FileId,
    parent: &marked_yaml::types::MarkedMappingNode,
    key: &str,
) -> Result<Option<Spanned<IndexMap<Spanned<String>, Spanned<String>>>>, ParseError> {
    match parent.get_node(key) {
        Some(Node::Mapping(m)) => {
            let span = marked_span_to_span(file, m.span());
            let mut result = IndexMap::new();

            for (k, v) in m.iter() {
                let key_span = marked_span_to_span(file, k.span());
                let key_str = Spanned::new(k.as_str().to_string(), key_span);
                let val = extract_string(file, v)?;
                result.insert(key_str, val);
            }

            Ok(Some(Spanned::new(result, span)))
        }
        Some(node) => Err(ParseError {
            kind: ParseErrorKind::InvalidType,
            span: node_to_span(file, node),
            message: format!("'{}' must be a mapping", key),
        }),
        None => Ok(None),
    }
}

/// Parse an array of strings.
fn parse_string_array(
    file: FileId,
    parent: &marked_yaml::types::MarkedMappingNode,
    key: &str,
) -> Result<Option<Spanned<Vec<Spanned<String>>>>, ParseError> {
    match parent.get_node(key) {
        Some(Node::Sequence(seq)) => {
            let span = marked_span_to_span(file, seq.span());
            let items: Result<Vec<_>, _> =
                seq.iter().map(|node| extract_string(file, node)).collect();
            Ok(Some(Spanned::new(items?, span)))
        }
        Some(node) => Err(ParseError {
            kind: ParseErrorKind::InvalidType,
            span: node_to_span(file, node),
            message: format!("'{}' must be an array", key),
        }),
        None => Ok(None),
    }
}

/// Parse a JSON value from a node.
fn parse_json_value(
    file: FileId,
    parent: &marked_yaml::types::MarkedMappingNode,
    key: &str,
) -> Result<Option<Spanned<serde_json::Value>>, ParseError> {
    match parent.get_node(key) {
        Some(node) => {
            let span = node_to_span(file, node);
            let value = node_to_json(node);
            Ok(Some(Spanned::new(value, span)))
        }
        None => Ok(None),
    }
}

/// Convert a YAML node to a JSON value.
fn node_to_json(node: &Node) -> serde_json::Value {
    match node {
        Node::Scalar(s) => {
            let str_val = s.as_str();
            // Try parsing as different types
            if let Ok(n) = str_val.parse::<i64>() {
                serde_json::Value::Number(n.into())
            } else if let Ok(n) = str_val.parse::<f64>() {
                serde_json::Number::from_f64(n)
                    .map(serde_json::Value::Number)
                    .unwrap_or(serde_json::Value::String(str_val.to_string()))
            } else if str_val == "true" {
                serde_json::Value::Bool(true)
            } else if str_val == "false" {
                serde_json::Value::Bool(false)
            } else if str_val == "null" || str_val == "~" {
                serde_json::Value::Null
            } else {
                serde_json::Value::String(str_val.to_string())
            }
        }
        Node::Mapping(m) => {
            let obj: serde_json::Map<String, serde_json::Value> = m
                .iter()
                .map(|(k, v)| (k.as_str().to_string(), node_to_json(v)))
                .collect();
            serde_json::Value::Object(obj)
        }
        Node::Sequence(s) => {
            let arr: Vec<serde_json::Value> = s.iter().map(node_to_json).collect();
            serde_json::Value::Array(arr)
        }
    }
}

// ============================================================================
// Action Parsing (5 Verbs)
// ============================================================================

/// Parse the task action from the mapping (dispatches to verb-specific parsers).
fn parse_action(
    file: FileId,
    map: &marked_yaml::types::MarkedMappingNode,
) -> Result<Option<RawTaskAction>, ParseError> {
    // Check for infer verb
    if let Some(node) = map.get_node("infer") {
        let action = parse_infer_action(file, node)?;
        let span = node_to_span(file, node);
        return Ok(Some(RawTaskAction::Infer(Spanned::new(action, span))));
    }
    // Check for exec verb
    if let Some(node) = map.get_node("exec") {
        let action = parse_exec_action(file, node)?;
        let span = node_to_span(file, node);
        return Ok(Some(RawTaskAction::Exec(Spanned::new(action, span))));
    }
    // Check for fetch verb
    if let Some(node) = map.get_node("fetch") {
        let action = parse_fetch_action(file, node)?;
        let span = node_to_span(file, node);
        return Ok(Some(RawTaskAction::Fetch(Spanned::new(action, span))));
    }
    // Check for invoke verb
    if let Some(node) = map.get_node("invoke") {
        let action = parse_invoke_action(file, node)?;
        let span = node_to_span(file, node);
        return Ok(Some(RawTaskAction::Invoke(Spanned::new(action, span))));
    }
    // Check for agent verb
    if let Some(node) = map.get_node("agent") {
        let action = parse_agent_action(file, node)?;
        let span = node_to_span(file, node);
        return Ok(Some(RawTaskAction::Agent(Spanned::new(action, span))));
    }

    Ok(None)
}

/// Parse infer action - supports both shorthand (string) and full form (mapping).
fn parse_infer_action(file: FileId, node: &Node) -> Result<RawInferAction, ParseError> {
    let span = node_to_span(file, node);

    match node {
        // Shorthand: infer: "prompt string"
        Node::Scalar(s) => Ok(RawInferAction {
            prompt: Spanned::new(s.as_str().to_string(), span),
            system: None,
            temperature: None,
            max_tokens: None,
            stop: None,
            thinking: None,
            thinking_budget: None,
        }),
        // Full form: infer: { prompt: "...", temperature: ... }
        Node::Mapping(m) => {
            let prompt = get_string_field(file, m, "prompt")?.ok_or_else(|| ParseError {
                kind: ParseErrorKind::MissingField,
                span,
                message: "infer action requires 'prompt' field".to_string(),
            })?;

            Ok(RawInferAction {
                prompt,
                system: get_string_field(file, m, "system")?,
                temperature: get_f64_field(file, m, "temperature")?,
                max_tokens: get_u32_field(file, m, "max_tokens")?,
                stop: parse_string_array(file, m, "stop")?,
                thinking: get_bool_field(file, m, "thinking")?.or(get_bool_field(
                    file,
                    m,
                    "extended_thinking",
                )?),
                thinking_budget: get_u32_field(file, m, "thinking_budget")?,
            })
        }
        _ => Err(ParseError {
            kind: ParseErrorKind::InvalidType,
            span,
            message: "infer must be a string or mapping".to_string(),
        }),
    }
}

/// Parse exec action - supports both shorthand (string) and full form (mapping).
fn parse_exec_action(file: FileId, node: &Node) -> Result<RawExecAction, ParseError> {
    let span = node_to_span(file, node);

    match node {
        // Shorthand: exec: "command string"
        Node::Scalar(s) => Ok(RawExecAction {
            command: Spanned::new(s.as_str().to_string(), span),
            shell: None,
            working_dir: None,
            env: None,
            timeout_ms: None,
            capture_stdout: None,
            capture_stderr: None,
        }),
        // Full form
        Node::Mapping(m) => {
            let command = get_string_field(file, m, "command")?.ok_or_else(|| ParseError {
                kind: ParseErrorKind::MissingField,
                span,
                message: "exec action requires 'command' field".to_string(),
            })?;

            Ok(RawExecAction {
                command,
                shell: get_bool_field(file, m, "shell")?,
                working_dir: get_string_field(file, m, "working_dir")?
                    .or(get_string_field(file, m, "cwd")?),
                env: parse_string_map(file, m, "env")?,
                timeout_ms: get_u64_field(file, m, "timeout_ms")?
                    .or(get_u64_field(file, m, "timeout")?),
                capture_stdout: get_bool_field(file, m, "capture_stdout")?,
                capture_stderr: get_bool_field(file, m, "capture_stderr")?,
            })
        }
        _ => Err(ParseError {
            kind: ParseErrorKind::InvalidType,
            span,
            message: "exec must be a string or mapping".to_string(),
        }),
    }
}

/// Parse fetch action - always requires a mapping.
fn parse_fetch_action(file: FileId, node: &Node) -> Result<RawFetchAction, ParseError> {
    let span = node_to_span(file, node);

    let m = match node {
        Node::Mapping(m) => m,
        _ => {
            return Err(ParseError {
                kind: ParseErrorKind::InvalidType,
                span,
                message: "fetch must be a mapping".to_string(),
            });
        }
    };

    let url = get_string_field(file, m, "url")?.ok_or_else(|| ParseError {
        kind: ParseErrorKind::MissingField,
        span,
        message: "fetch action requires 'url' field".to_string(),
    })?;

    Ok(RawFetchAction {
        url,
        method: get_string_field(file, m, "method")?,
        headers: parse_string_map(file, m, "headers")?,
        body: get_string_field(file, m, "body")?,
        json: parse_json_value(file, m, "json")?,
        timeout_ms: get_u64_field(file, m, "timeout_ms")?.or(get_u64_field(file, m, "timeout")?),
        follow_redirects: get_bool_field(file, m, "follow_redirects")?,
    })
}

/// Parse invoke action - always requires a mapping.
fn parse_invoke_action(file: FileId, node: &Node) -> Result<RawInvokeAction, ParseError> {
    let span = node_to_span(file, node);

    let m = match node {
        Node::Mapping(m) => m,
        _ => {
            return Err(ParseError {
                kind: ParseErrorKind::InvalidType,
                span,
                message: "invoke must be a mapping".to_string(),
            });
        }
    };

    let tool = get_string_field(file, m, "tool")?.ok_or_else(|| ParseError {
        kind: ParseErrorKind::MissingField,
        span,
        message: "invoke action requires 'tool' field".to_string(),
    })?;

    Ok(RawInvokeAction {
        tool,
        params: parse_json_value(file, m, "params")?,
        mcp: get_string_field(file, m, "mcp")?.or(get_string_field(file, m, "server")?),
        timeout_ms: get_u64_field(file, m, "timeout_ms")?.or(get_u64_field(file, m, "timeout")?),
    })
}

/// Parse agent action - always requires a mapping.
fn parse_agent_action(file: FileId, node: &Node) -> Result<RawAgentAction, ParseError> {
    let span = node_to_span(file, node);

    let m = match node {
        Node::Mapping(m) => m,
        _ => {
            return Err(ParseError {
                kind: ParseErrorKind::InvalidType,
                span,
                message: "agent must be a mapping".to_string(),
            });
        }
    };

    // Agent can use either 'goal' or 'prompt' for the main instruction
    let goal = get_string_field(file, m, "goal")?
        .or(get_string_field(file, m, "prompt")?)
        .ok_or_else(|| ParseError {
            kind: ParseErrorKind::MissingField,
            span,
            message: "agent action requires 'goal' or 'prompt' field".to_string(),
        })?;

    Ok(RawAgentAction {
        goal,
        tools: parse_string_array(file, m, "tools")?,
        max_iterations: get_u32_field(file, m, "max_iterations")?.or(get_u32_field(
            file,
            m,
            "max_turns",
        )?),
        max_tokens: get_u32_field(file, m, "max_tokens")?,
        from: get_string_field(file, m, "from")?,
        skills: parse_string_array(file, m, "skills")?,
    })
}

// ============================================================================
// use:/flow:/for_each:/retry:/output: Parsing
// ============================================================================

/// Parse use: references.
#[allow(clippy::type_complexity)]
fn parse_use_refs(
    file: FileId,
    map: &marked_yaml::types::MarkedMappingNode,
) -> Result<Option<Spanned<IndexMap<Spanned<String>, RawUseTarget>>>, ParseError> {
    match map.get_node("use") {
        Some(Node::Mapping(m)) => {
            let span = marked_span_to_span(file, m.span());
            let mut refs = IndexMap::new();

            for (key, value) in m.iter() {
                let alias_span = marked_span_to_span(file, key.span());
                let alias = Spanned::new(key.as_str().to_string(), alias_span);
                let target = parse_use_target(file, value)?;
                refs.insert(alias, target);
            }

            Ok(Some(Spanned::new(refs, span)))
        }
        Some(node) => Err(ParseError {
            kind: ParseErrorKind::InvalidType,
            span: node_to_span(file, node),
            message: "use must be a mapping".to_string(),
        }),
        None => Ok(None),
    }
}

/// Parse a use target (simple task_id or extended { task, path }).
fn parse_use_target(file: FileId, node: &Node) -> Result<RawUseTarget, ParseError> {
    let span = node_to_span(file, node);

    match node {
        // Simple: alias: task_id
        Node::Scalar(s) => Ok(RawUseTarget::TaskId(Spanned::new(
            s.as_str().to_string(),
            span,
        ))),
        // Extended: { task: id, path: "..." }
        Node::Mapping(m) => {
            let task = get_string_field(file, m, "task")?.ok_or_else(|| ParseError {
                kind: ParseErrorKind::MissingField,
                span,
                message: "use target requires 'task' field".to_string(),
            })?;
            let path = get_string_field(file, m, "path")?;

            Ok(RawUseTarget::Extended { task, path, span })
        }
        _ => Err(ParseError {
            kind: ParseErrorKind::InvalidType,
            span,
            message: "use target must be string or mapping".to_string(),
        }),
    }
}

/// Parse flow: dependencies.
fn parse_flow(
    file: FileId,
    map: &marked_yaml::types::MarkedMappingNode,
) -> Result<Option<Spanned<RawFlow>>, ParseError> {
    match map.get_node("flow") {
        Some(Node::Scalar(s)) => {
            let span = marked_span_to_span(file, s.span());
            Ok(Some(Spanned::new(
                RawFlow::Single(Spanned::new(s.as_str().to_string(), span)),
                span,
            )))
        }
        Some(Node::Sequence(seq)) => {
            let span = marked_span_to_span(file, seq.span());
            let ids: Result<Vec<_>, _> = seq.iter().map(|n| extract_string(file, n)).collect();
            Ok(Some(Spanned::new(RawFlow::Multiple(ids?), span)))
        }
        Some(node) => Err(ParseError {
            kind: ParseErrorKind::InvalidType,
            span: node_to_span(file, node),
            message: "flow must be string or array".to_string(),
        }),
        None => Ok(None),
    }
}

/// Parse for_each: iteration.
fn parse_for_each(
    file: FileId,
    map: &marked_yaml::types::MarkedMappingNode,
) -> Result<Option<Spanned<RawForEach>>, ParseError> {
    match map.get_node("for_each") {
        Some(Node::Sequence(seq)) => {
            let span = marked_span_to_span(file, seq.span());
            // Array literal - serialize to JSON string for storage
            let arr: Vec<serde_json::Value> = seq.iter().map(node_to_json).collect();
            let items_str = serde_json::to_string(&arr).unwrap_or_default();

            Ok(Some(Spanned::new(
                RawForEach {
                    items: Spanned::new(items_str, span),
                    as_var: get_string_field(file, map, "as")?,
                    parallel: get_u32_field(file, map, "concurrency")?
                        .or(get_u32_field(file, map, "parallel")?),
                },
                span,
            )))
        }
        Some(Node::Scalar(s)) => {
            let span = marked_span_to_span(file, s.span());
            Ok(Some(Spanned::new(
                RawForEach {
                    items: Spanned::new(s.as_str().to_string(), span),
                    as_var: get_string_field(file, map, "as")?,
                    parallel: get_u32_field(file, map, "concurrency")?
                        .or(get_u32_field(file, map, "parallel")?),
                },
                span,
            )))
        }
        Some(node) => Err(ParseError {
            kind: ParseErrorKind::InvalidType,
            span: node_to_span(file, node),
            message: "for_each must be array or string".to_string(),
        }),
        None => Ok(None),
    }
}

/// Parse retry: configuration.
fn parse_retry(
    file: FileId,
    map: &marked_yaml::types::MarkedMappingNode,
) -> Result<Option<Spanned<RawRetryConfig>>, ParseError> {
    match map.get_node("retry") {
        Some(Node::Mapping(m)) => {
            let span = marked_span_to_span(file, m.span());
            Ok(Some(Spanned::new(
                RawRetryConfig {
                    max_attempts: get_u32_field(file, m, "max_attempts")?
                        .or(get_u32_field(file, m, "max")?),
                    delay_ms: get_u64_field(file, m, "delay_ms")?
                        .or(get_u64_field(file, m, "delay")?),
                    backoff: get_f64_field(file, m, "backoff")?.or(get_f64_field(
                        file,
                        m,
                        "backoff_multiplier",
                    )?),
                },
                span,
            )))
        }
        Some(node) => Err(ParseError {
            kind: ParseErrorKind::InvalidType,
            span: node_to_span(file, node),
            message: "retry must be a mapping".to_string(),
        }),
        None => Ok(None),
    }
}

/// Parse output: configuration.
fn parse_output(
    file: FileId,
    map: &marked_yaml::types::MarkedMappingNode,
) -> Result<Option<Spanned<RawOutputConfig>>, ParseError> {
    match map.get_node("output") {
        Some(Node::Mapping(m)) => {
            let span = marked_span_to_span(file, m.span());
            Ok(Some(Spanned::new(
                RawOutputConfig {
                    format: get_string_field(file, m, "format")?,
                    schema: parse_json_value(file, m, "schema")?,
                    schema_ref: get_string_field(file, m, "schema_ref")?
                        .or(get_string_field(file, m, "$ref")?),
                },
                span,
            )))
        }
        Some(node) => Err(ParseError {
            kind: ParseErrorKind::InvalidType,
            span: node_to_span(file, node),
            message: "output must be a mapping".to_string(),
        }),
        None => Ok(None),
    }
}

// ============================================================================
// Main Parser
// ============================================================================

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
    workflow.schema = get_string_field(file_id, map, "schema")?.ok_or_else(|| ParseError {
        kind: ParseErrorKind::MissingField,
        span: workflow.span,
        message: "missing required field 'schema'".to_string(),
    })?;

    // Extract optional fields
    workflow.workflow = get_string_field(file_id, map, "workflow")?;
    workflow.description = get_string_field(file_id, map, "description")?;
    workflow.provider = get_string_field(file_id, map, "provider")?;
    workflow.model = get_string_field(file_id, map, "model")?;

    // Parse MCP server configurations
    workflow.mcp = parse_mcp_config(file_id, map)?;

    // Parse tasks
    workflow.tasks = parse_tasks(file_id, map)?;

    Ok(workflow)
}

/// Parse the MCP configuration block from a workflow mapping.
fn parse_mcp_config(
    file_id: FileId,
    map: &marked_yaml::types::MarkedMappingNode,
) -> Result<Option<Spanned<RawMcpConfig>>, ParseError> {
    // Look for "mcp:" mapping
    let mcp_node = match map.get_node("mcp") {
        Some(node) => node,
        None => return Ok(None),
    };

    let mcp_map = match mcp_node {
        Node::Mapping(m) => m,
        _ => {
            return Err(ParseError {
                kind: ParseErrorKind::InvalidType,
                span: node_to_span(file_id, mcp_node),
                message: "mcp must be a mapping".to_string(),
            });
        }
    };

    let mcp_span = marked_span_to_span(file_id, mcp_map.span());
    let mut config = RawMcpConfig::default();

    // Look for "servers:" mapping inside mcp
    if let Some(servers_node) = mcp_map.get_node("servers") {
        let servers_map = match servers_node {
            Node::Mapping(m) => m,
            _ => {
                return Err(ParseError {
                    kind: ParseErrorKind::InvalidType,
                    span: node_to_span(file_id, servers_node),
                    message: "mcp.servers must be a mapping".to_string(),
                });
            }
        };

        // Parse each server entry
        for (key, value) in servers_map.iter() {
            let server_name = Spanned::new(
                key.as_str().to_string(),
                marked_span_to_span(file_id, key.span()),
            );

            let server = parse_mcp_server(file_id, value)?;
            config.servers.insert(server_name, server);
        }
    }

    Ok(Some(Spanned::new(config, mcp_span)))
}

/// Parse a single MCP server configuration.
fn parse_mcp_server(file_id: FileId, node: &Node) -> Result<Spanned<RawMcpServer>, ParseError> {
    let span = node_to_span(file_id, node);

    let map = match node {
        Node::Mapping(m) => m,
        _ => {
            return Err(ParseError {
                kind: ParseErrorKind::InvalidType,
                span,
                message: "MCP server config must be a mapping".to_string(),
            });
        }
    };

    let server = RawMcpServer {
        command: get_string_field(file_id, map, "command")?,
        args: parse_string_array(file_id, map, "args")?,
        env: parse_string_map(file_id, map, "env")?,
        cwd: get_string_field(file_id, map, "cwd")?,
        url: get_string_field(file_id, map, "url")?,
        transport: get_string_field(file_id, map, "transport")?,
    };

    Ok(Spanned::new(server, span))
}

/// Parse the tasks array from a workflow mapping.
fn parse_tasks(
    file_id: FileId,
    map: &marked_yaml::types::MarkedMappingNode,
) -> Result<Spanned<Vec<Spanned<RawTask>>>, ParseError> {
    match map.get_node("tasks") {
        Some(Node::Sequence(seq)) => {
            let span = marked_span_to_span(file_id, seq.span());
            let tasks = seq
                .iter()
                .map(|task_node| parse_task(file_id, task_node))
                .collect::<Result<Vec<_>, _>>()?;
            Ok(Spanned::new(tasks, span))
        }
        Some(node) => Err(ParseError {
            kind: ParseErrorKind::InvalidType,
            span: node_to_span(file_id, node),
            message: "tasks must be a sequence".to_string(),
        }),
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

    // Extract task id (required)
    let id = get_string_field(file_id, map, "id")?.ok_or_else(|| ParseError {
        kind: ParseErrorKind::MissingField,
        span,
        message: "task missing required field 'id'".to_string(),
    })?;

    // Extract optional fields
    let description = get_string_field(file_id, map, "description")?;
    let provider = get_string_field(file_id, map, "provider")?;
    let model = get_string_field(file_id, map, "model")?;

    // Parse all task fields
    let action = parse_action(file_id, map)?;
    let use_refs = parse_use_refs(file_id, map)?;
    let flow = parse_flow(file_id, map)?;
    let output = parse_output(file_id, map)?;
    let for_each = parse_for_each(file_id, map)?;
    let retry = parse_retry(file_id, map)?;

    let task = RawTask {
        span,
        id,
        description,
        provider,
        model,
        action,
        use_refs,
        flow,
        output,
        for_each,
        retry,
    };

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
        assert_eq!(
            workflow.description.as_ref().unwrap().value,
            "A test workflow"
        );
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
        assert!(schema_span.start.0 <= schema_span.end.0);
    }

    // =========================================================================
    // Verb Parsing Tests (5 Verbs)
    // =========================================================================

    #[test]
    fn test_parse_infer_shorthand() {
        let yaml = r#"
schema: "nika/workflow@0.10"
tasks:
  - id: generate
    infer: "Generate a headline"
"#;
        let workflow = parse(yaml, FileId(0)).unwrap();
        let task = workflow.get_task("generate").unwrap();

        match &task.value.action {
            Some(RawTaskAction::Infer(action)) => {
                assert_eq!(action.value.prompt.value, "Generate a headline");
                assert!(action.value.temperature.is_none());
                assert!(action.value.system.is_none());
            }
            _ => panic!("Expected Infer action"),
        }
    }

    #[test]
    fn test_parse_infer_full_form() {
        let yaml = r#"
schema: "nika/workflow@0.10"
tasks:
  - id: generate
    infer:
      prompt: "Generate content"
      system: "You are a helpful assistant"
      temperature: 0.7
      max_tokens: 1000
      thinking: true
      thinking_budget: 8000
"#;
        let workflow = parse(yaml, FileId(0)).unwrap();
        let task = workflow.get_task("generate").unwrap();

        match &task.value.action {
            Some(RawTaskAction::Infer(action)) => {
                assert_eq!(action.value.prompt.value, "Generate content");
                assert_eq!(
                    action.value.system.as_ref().unwrap().value,
                    "You are a helpful assistant"
                );
                assert!((action.value.temperature.as_ref().unwrap().value - 0.7).abs() < 0.001);
                assert_eq!(action.value.max_tokens.as_ref().unwrap().value, 1000);
                assert!(action.value.thinking.as_ref().unwrap().value);
                assert_eq!(action.value.thinking_budget.as_ref().unwrap().value, 8000);
            }
            _ => panic!("Expected Infer action"),
        }
    }

    #[test]
    fn test_parse_exec_shorthand() {
        let yaml = r#"
schema: "nika/workflow@0.10"
tasks:
  - id: build
    exec: "npm run build"
"#;
        let workflow = parse(yaml, FileId(0)).unwrap();
        let task = workflow.get_task("build").unwrap();

        match &task.value.action {
            Some(RawTaskAction::Exec(action)) => {
                assert_eq!(action.value.command.value, "npm run build");
                assert!(action.value.shell.is_none());
            }
            _ => panic!("Expected Exec action"),
        }
    }

    #[test]
    fn test_parse_exec_full_form() {
        let yaml = r#"
schema: "nika/workflow@0.10"
tasks:
  - id: build
    exec:
      command: "npm run build"
      shell: true
      cwd: "/app"
      timeout: 30000
      env:
        NODE_ENV: production
"#;
        let workflow = parse(yaml, FileId(0)).unwrap();
        let task = workflow.get_task("build").unwrap();

        match &task.value.action {
            Some(RawTaskAction::Exec(action)) => {
                assert_eq!(action.value.command.value, "npm run build");
                assert!(action.value.shell.as_ref().unwrap().value);
                assert_eq!(action.value.working_dir.as_ref().unwrap().value, "/app");
                assert_eq!(action.value.timeout_ms.as_ref().unwrap().value, 30000);
                let env = action.value.env.as_ref().unwrap();
                assert!(env.value.values().any(|v| v.value == "production"));
            }
            _ => panic!("Expected Exec action"),
        }
    }

    #[test]
    fn test_parse_fetch_action() {
        let yaml = r#"
schema: "nika/workflow@0.10"
tasks:
  - id: api_call
    fetch:
      url: "https://api.example.com/data"
      method: POST
      headers:
        Authorization: "Bearer token"
      timeout: 5000
"#;
        let workflow = parse(yaml, FileId(0)).unwrap();
        let task = workflow.get_task("api_call").unwrap();

        match &task.value.action {
            Some(RawTaskAction::Fetch(action)) => {
                assert_eq!(action.value.url.value, "https://api.example.com/data");
                assert_eq!(action.value.method.as_ref().unwrap().value, "POST");
                assert_eq!(action.value.timeout_ms.as_ref().unwrap().value, 5000);
                let headers = action.value.headers.as_ref().unwrap();
                assert!(headers.value.values().any(|v| v.value.contains("Bearer")));
            }
            _ => panic!("Expected Fetch action"),
        }
    }

    #[test]
    fn test_parse_invoke_action() {
        let yaml = r#"
schema: "nika/workflow@0.10"
tasks:
  - id: mcp_call
    invoke:
      tool: novanet_generate
      mcp: novanet
      params:
        entity: "qr-code"
        locale: "fr-FR"
"#;
        let workflow = parse(yaml, FileId(0)).unwrap();
        let task = workflow.get_task("mcp_call").unwrap();

        match &task.value.action {
            Some(RawTaskAction::Invoke(action)) => {
                assert_eq!(action.value.tool.value, "novanet_generate");
                assert_eq!(action.value.mcp.as_ref().unwrap().value, "novanet");
                assert!(action.value.params.is_some());
            }
            _ => panic!("Expected Invoke action"),
        }
    }

    #[test]
    fn test_parse_agent_action() {
        let yaml = r#"
schema: "nika/workflow@0.10"
tasks:
  - id: research
    agent:
      goal: "Research AI trends"
      tools:
        - nika:read
        - nika:write
      max_turns: 10
"#;
        let workflow = parse(yaml, FileId(0)).unwrap();
        let task = workflow.get_task("research").unwrap();

        match &task.value.action {
            Some(RawTaskAction::Agent(action)) => {
                assert_eq!(action.value.goal.value, "Research AI trends");
                let tools = action.value.tools.as_ref().unwrap();
                assert_eq!(tools.value.len(), 2);
                assert_eq!(tools.value[0].value, "nika:read");
                assert_eq!(action.value.max_iterations.as_ref().unwrap().value, 10);
            }
            _ => panic!("Expected Agent action"),
        }
    }

    // =========================================================================
    // Task Configuration Parsing Tests
    // =========================================================================

    #[test]
    fn test_parse_use_refs_simple() {
        let yaml = r#"
schema: "nika/workflow@0.10"
tasks:
  - id: step1
    infer: "Generate"
  - id: step2
    use:
      data: step1
    infer: "Process {{use.data}}"
"#;
        let workflow = parse(yaml, FileId(0)).unwrap();
        let task = workflow.get_task("step2").unwrap();

        let use_refs = task.value.use_refs.as_ref().unwrap();
        assert_eq!(use_refs.value.len(), 1);

        let (alias, target) = use_refs.value.iter().next().unwrap();
        assert_eq!(alias.value, "data");
        assert_eq!(target.task_id(), "step1");
    }

    #[test]
    fn test_parse_use_refs_extended() {
        let yaml = r#"
schema: "nika/workflow@0.10"
tasks:
  - id: step1
    infer: "Generate"
  - id: step2
    use:
      data:
        task: step1
        path: "$.result.value"
    infer: "Process"
"#;
        let workflow = parse(yaml, FileId(0)).unwrap();
        let task = workflow.get_task("step2").unwrap();

        let use_refs = task.value.use_refs.as_ref().unwrap();
        let (_, target) = use_refs.value.iter().next().unwrap();

        match target {
            RawUseTarget::Extended { task, path, .. } => {
                assert_eq!(task.value, "step1");
                assert_eq!(path.as_ref().unwrap().value, "$.result.value");
            }
            _ => panic!("Expected Extended target"),
        }
    }

    #[test]
    fn test_parse_flow_single() {
        let yaml = r#"
schema: "nika/workflow@0.10"
tasks:
  - id: step1
    infer: "Generate"
  - id: step2
    flow: step1
    infer: "Process"
"#;
        let workflow = parse(yaml, FileId(0)).unwrap();
        let task = workflow.get_task("step2").unwrap();

        let flow = task.value.flow.as_ref().unwrap();
        assert_eq!(flow.value.task_ids(), vec!["step1"]);
    }

    #[test]
    fn test_parse_flow_multiple() {
        let yaml = r#"
schema: "nika/workflow@0.10"
tasks:
  - id: step1
    infer: "Step 1"
  - id: step2
    infer: "Step 2"
  - id: step3
    flow: [step1, step2]
    infer: "Process"
"#;
        let workflow = parse(yaml, FileId(0)).unwrap();
        let task = workflow.get_task("step3").unwrap();

        let flow = task.value.flow.as_ref().unwrap();
        assert_eq!(flow.value.task_ids(), vec!["step1", "step2"]);
    }

    #[test]
    fn test_parse_for_each_array() {
        let yaml = r#"
schema: "nika/workflow@0.10"
tasks:
  - id: parallel
    for_each: ["a", "b", "c"]
    as: item
    concurrency: 3
    infer: "Process {{use.item}}"
"#;
        let workflow = parse(yaml, FileId(0)).unwrap();
        let task = workflow.get_task("parallel").unwrap();

        let for_each = task.value.for_each.as_ref().unwrap();
        assert!(for_each.value.items.value.contains("["));
        assert_eq!(for_each.value.as_var.as_ref().unwrap().value, "item");
        assert_eq!(for_each.value.parallel.as_ref().unwrap().value, 3);
    }

    #[test]
    fn test_parse_for_each_binding() {
        let yaml = r#"
schema: "nika/workflow@0.10"
tasks:
  - id: parallel
    for_each: "{{use.items}}"
    infer: "Process"
"#;
        let workflow = parse(yaml, FileId(0)).unwrap();
        let task = workflow.get_task("parallel").unwrap();

        let for_each = task.value.for_each.as_ref().unwrap();
        assert_eq!(for_each.value.items.value, "{{use.items}}");
    }

    #[test]
    fn test_parse_retry_config() {
        let yaml = r#"
schema: "nika/workflow@0.10"
tasks:
  - id: resilient
    retry:
      max_attempts: 3
      delay_ms: 1000
      backoff: 2.0
    infer: "Generate"
"#;
        let workflow = parse(yaml, FileId(0)).unwrap();
        let task = workflow.get_task("resilient").unwrap();

        let retry = task.value.retry.as_ref().unwrap();
        assert_eq!(retry.value.max_attempts.as_ref().unwrap().value, 3);
        assert_eq!(retry.value.delay_ms.as_ref().unwrap().value, 1000);
        assert!((retry.value.backoff.as_ref().unwrap().value - 2.0).abs() < 0.001);
    }

    #[test]
    fn test_parse_output_config() {
        let yaml = r#"
schema: "nika/workflow@0.10"
tasks:
  - id: structured
    output:
      format: json
      schema:
        type: object
        properties:
          name:
            type: string
    infer: "Generate JSON"
"#;
        let workflow = parse(yaml, FileId(0)).unwrap();
        let task = workflow.get_task("structured").unwrap();

        let output = task.value.output.as_ref().unwrap();
        assert_eq!(output.value.format.as_ref().unwrap().value, "json");
        assert!(output.value.schema.is_some());
    }

    // =========================================================================
    // Error Cases
    // =========================================================================

    #[test]
    fn test_parse_infer_missing_prompt() {
        let yaml = r#"
schema: "nika/workflow@0.10"
tasks:
  - id: generate
    infer:
      temperature: 0.7
"#;
        let result = parse(yaml, FileId(0));
        assert!(result.is_err());

        let err = result.unwrap_err();
        assert_eq!(err.kind, ParseErrorKind::MissingField);
        assert!(err.message.contains("prompt"));
    }

    #[test]
    fn test_parse_fetch_missing_url() {
        let yaml = r#"
schema: "nika/workflow@0.10"
tasks:
  - id: api_call
    fetch:
      method: GET
"#;
        let result = parse(yaml, FileId(0));
        assert!(result.is_err());

        let err = result.unwrap_err();
        assert_eq!(err.kind, ParseErrorKind::MissingField);
        assert!(err.message.contains("url"));
    }

    #[test]
    fn test_parse_invoke_missing_tool() {
        let yaml = r#"
schema: "nika/workflow@0.10"
tasks:
  - id: mcp_call
    invoke:
      mcp: novanet
"#;
        let result = parse(yaml, FileId(0));
        assert!(result.is_err());

        let err = result.unwrap_err();
        assert_eq!(err.kind, ParseErrorKind::MissingField);
        assert!(err.message.contains("tool"));
    }

    #[test]
    fn test_parse_agent_missing_goal() {
        let yaml = r#"
schema: "nika/workflow@0.10"
tasks:
  - id: research
    agent:
      tools: [nika:read]
"#;
        let result = parse(yaml, FileId(0));
        assert!(result.is_err());

        let err = result.unwrap_err();
        assert_eq!(err.kind, ParseErrorKind::MissingField);
        assert!(err.message.contains("goal"));
    }

    #[test]
    fn test_parse_invalid_temperature() {
        let yaml = r#"
schema: "nika/workflow@0.10"
tasks:
  - id: generate
    infer:
      prompt: "Test"
      temperature: not_a_number
"#;
        let result = parse(yaml, FileId(0));
        assert!(result.is_err());

        let err = result.unwrap_err();
        assert_eq!(err.kind, ParseErrorKind::InvalidType);
    }
}
