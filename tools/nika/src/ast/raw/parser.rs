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
use super::task::{RawForEach, RawOutputConfig, RawRetryConfig, RawTask};
use super::workflow::{RawContextConfig, RawImportSpec, RawPkgConfig, RawWorkflow};
use crate::ast::decompose::{DecomposeSpec, DecomposeStrategy};
use crate::ast::structured::StructuredOutputSpec;
use crate::source::{ByteOffset, FileId, Span, Spanned};

/// Errors that can occur during parsing.
#[derive(Debug, Clone)]
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

impl ParseErrorKind {
    /// Get the error code for this kind.
    pub fn code(&self) -> &'static str {
        match self {
            Self::Syntax => "NIKA-001",
            Self::MissingField => "NIKA-002",
            Self::InvalidType => "NIKA-003",
            Self::UnknownField => "NIKA-004",
            Self::InvalidSchema => "NIKA-005",
        }
    }
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
    // Reject tasks with multiple verbs (must have exactly 0 or 1)
    let verb_keys = ["infer", "exec", "fetch", "invoke", "agent"];
    let found: Vec<&str> = verb_keys
        .iter()
        .filter(|k| map.get_node(k).is_some())
        .copied()
        .collect();
    if found.len() > 1 {
        let span = marked_span_to_span(file, map.span());
        return Err(ParseError {
            kind: ParseErrorKind::InvalidType,
            span,
            message: format!(
                "task has multiple verbs ({}); each task must have exactly one",
                found.join(", ")
            ),
        });
    }

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

    // Agent uses 'prompt' as primary field, 'goal' as legacy fallback
    let prompt = get_string_field(file, m, "prompt")?
        .or(get_string_field(file, m, "goal")?)
        .ok_or_else(|| ParseError {
            kind: ParseErrorKind::MissingField,
            span,
            message: "agent action requires 'prompt' field (or legacy 'goal')".to_string(),
        })?;

    Ok(RawAgentAction {
        prompt,
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
// with:/depends_on:/for_each:/retry:/output: Parsing
// ============================================================================

/// Parse with: bindings.
///
/// Values are raw strings parsed by `parse_with_entry()` in Phase 2 (analyzer).
/// Examples:
///   - `data: step1` — simple task reference
///   - `temp: step1.data.temp ?? 20` — path + default
///   - `cfg: $env.API_KEY` — environment binding
///   - `val: step1.output | upper | trim` — with transforms
#[allow(clippy::type_complexity)]
fn parse_with_refs(
    file: FileId,
    map: &marked_yaml::types::MarkedMappingNode,
) -> Result<Option<Spanned<IndexMap<Spanned<String>, Spanned<String>>>>, ParseError> {
    parse_string_map(file, map, "with")
}

/// Parse depends_on: ordering dependencies.
///
/// Pure ordering edges — no data flows through them.
/// Data dependencies are expressed via `with:` bindings.
fn parse_depends_on(
    file: FileId,
    map: &marked_yaml::types::MarkedMappingNode,
) -> Result<Option<Spanned<Vec<Spanned<String>>>>, ParseError> {
    // depends_on: accepts both single string and array
    match map.get_node("depends_on") {
        Some(Node::Scalar(s)) => {
            let span = marked_span_to_span(file, s.span());
            Ok(Some(Spanned::new(
                vec![Spanned::new(s.as_str().to_string(), span)],
                span,
            )))
        }
        Some(Node::Sequence(seq)) => {
            let span = marked_span_to_span(file, seq.span());
            let ids: Result<Vec<_>, _> = seq.iter().map(|n| extract_string(file, n)).collect();
            Ok(Some(Spanned::new(ids?, span)))
        }
        Some(node) => Err(ParseError {
            kind: ParseErrorKind::InvalidType,
            span: node_to_span(file, node),
            message: "depends_on must be a string or array of strings".to_string(),
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
                    fail_fast: get_bool_field(file, map, "fail_fast")?,
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
                    fail_fast: get_bool_field(file, map, "fail_fast")?,
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

/// Parse decompose: configuration.
fn parse_decompose(
    file: FileId,
    map: &marked_yaml::types::MarkedMappingNode,
) -> Result<Option<Spanned<DecomposeSpec>>, ParseError> {
    match map.get_node("decompose") {
        Some(Node::Mapping(m)) => {
            let span = marked_span_to_span(file, m.span());

            let traverse = get_string_field(file, m, "traverse")?
                .ok_or_else(|| ParseError {
                    kind: ParseErrorKind::MissingField,
                    span,
                    message: "decompose missing required field 'traverse'".to_string(),
                })?
                .value;

            let source = get_string_field(file, m, "source")?
                .ok_or_else(|| ParseError {
                    kind: ParseErrorKind::MissingField,
                    span,
                    message: "decompose missing required field 'source'".to_string(),
                })?
                .value;

            let strategy = match get_string_field(file, m, "strategy")? {
                Some(s) => match s.value.as_str() {
                    "semantic" => DecomposeStrategy::Semantic,
                    "static" => DecomposeStrategy::Static,
                    "nested" => DecomposeStrategy::Nested,
                    other => {
                        return Err(ParseError {
                            kind: ParseErrorKind::InvalidType,
                            span: s.span,
                            message: format!(
                                "invalid decompose strategy '{}': expected semantic, static, or nested",
                                other
                            ),
                        });
                    }
                },
                None => DecomposeStrategy::default(),
            };

            let mcp_server = get_string_field(file, m, "mcp_server")?.map(|s| s.value);

            let max_items = get_u32_field(file, m, "max_items")?.map(|s| s.value as usize);

            let max_depth = get_u32_field(file, m, "max_depth")?.map(|s| s.value as usize);

            Ok(Some(Spanned::new(
                DecomposeSpec {
                    strategy,
                    traverse,
                    source,
                    mcp_server,
                    max_items,
                    max_depth,
                },
                span,
            )))
        }
        Some(node) => Err(ParseError {
            kind: ParseErrorKind::InvalidType,
            span: node_to_span(file, node),
            message: "decompose must be a mapping".to_string(),
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

/// Parse structured: configuration (StructuredOutputSpec).
///
/// Supports shorthand (string path) or full mapping form:
/// ```yaml
/// structured: ./schemas/user.json          # shorthand
/// structured:                               # full form
///   schema: ./schemas/user.json
///   max_retries: 3
/// ```
fn parse_structured(
    file: FileId,
    map: &marked_yaml::types::MarkedMappingNode,
) -> Result<Option<StructuredOutputSpec>, ParseError> {
    match map.get_node("structured") {
        Some(node) => {
            let span = node_to_span(file, node);
            // Convert YAML node to JSON value, then deserialize via serde_json.
            // StructuredOutputSpec's custom Deserialize handles both string and map forms.
            let json_value = node_to_json(node);
            let spec: StructuredOutputSpec =
                serde_json::from_value(json_value).map_err(|e| ParseError {
                    kind: ParseErrorKind::InvalidType,
                    span,
                    message: format!("invalid structured output config: {e}"),
                })?;
            Ok(Some(spec))
        }
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

    // Parse pkg configuration
    workflow.pkg = parse_pkg_config(file_id, map)?;

    // Parse context configuration
    workflow.context = parse_context_config(file_id, map)?;

    // Parse imports
    workflow.imports = parse_imports(file_id, map)?;

    // Parse inputs
    workflow.inputs = parse_inputs(file_id, map)?;

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

    // Support both formats:
    //   Nested: mcp: { servers: { novanet: { command: ... } } }
    //   Flat:   mcp: { novanet: { command: ... } }
    let servers_map = if let Some(servers_node) = mcp_map.get_node("servers") {
        match servers_node {
            Node::Mapping(m) => m,
            _ => {
                return Err(ParseError {
                    kind: ParseErrorKind::InvalidType,
                    span: node_to_span(file_id, servers_node),
                    message: "mcp.servers must be a mapping".to_string(),
                });
            }
        }
    } else {
        // Flat format: entries directly under mcp:
        mcp_map
    };

    for (key, value) in servers_map.iter() {
        let server_name = Spanned::new(
            key.as_str().to_string(),
            marked_span_to_span(file_id, key.span()),
        );

        let server = parse_mcp_server(file_id, value)?;
        config.servers.insert(server_name, server);
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

/// Parse pkg: configuration.
fn parse_pkg_config(
    file_id: FileId,
    map: &marked_yaml::types::MarkedMappingNode,
) -> Result<Option<Spanned<RawPkgConfig>>, ParseError> {
    let pkg_node = match map.get_node("pkg") {
        Some(node) => node,
        None => return Ok(None),
    };

    let pkg_map = match pkg_node {
        Node::Mapping(m) => m,
        _ => {
            return Err(ParseError {
                kind: ParseErrorKind::InvalidType,
                span: node_to_span(file_id, pkg_node),
                message: "pkg must be a mapping".to_string(),
            });
        }
    };

    let span = marked_span_to_span(file_id, pkg_map.span());
    let include = match parse_string_array(file_id, pkg_map, "include")? {
        Some(arr) => arr.value,
        None => Vec::new(),
    };

    Ok(Some(Spanned::new(RawPkgConfig { include }, span)))
}

/// Parse context: configuration.
fn parse_context_config(
    file_id: FileId,
    map: &marked_yaml::types::MarkedMappingNode,
) -> Result<Option<Spanned<RawContextConfig>>, ParseError> {
    let ctx_node = match map.get_node("context") {
        Some(node) => node,
        None => return Ok(None),
    };

    let ctx_map = match ctx_node {
        Node::Mapping(m) => m,
        _ => {
            return Err(ParseError {
                kind: ParseErrorKind::InvalidType,
                span: node_to_span(file_id, ctx_node),
                message: "context must be a mapping".to_string(),
            });
        }
    };

    let span = marked_span_to_span(file_id, ctx_map.span());
    let files = parse_string_map(file_id, ctx_map, "files")?.map(|s| s.value);

    Ok(Some(Spanned::new(RawContextConfig { files }, span)))
}

/// Parse imports: specification.
///
/// ```yaml
/// imports:
///   - path: ./partials/setup.nika.yaml
///     prefix: setup_
///   - path: pkg:@spn/core@1.0/seo.nika.yaml
/// ```
fn parse_imports(
    file_id: FileId,
    map: &marked_yaml::types::MarkedMappingNode,
) -> Result<Option<Spanned<Vec<Spanned<RawImportSpec>>>>, ParseError> {
    // Accept both "imports:" (canonical) and "include:" (legacy alias)
    let imports_node = match map.get_node("imports").or_else(|| map.get_node("include")) {
        Some(node) => node,
        None => return Ok(None),
    };

    let seq = match imports_node {
        Node::Sequence(s) => s,
        _ => {
            return Err(ParseError {
                kind: ParseErrorKind::InvalidType,
                span: node_to_span(file_id, imports_node),
                message: "imports must be a sequence".to_string(),
            });
        }
    };

    let outer_span = marked_span_to_span(file_id, seq.span());
    let mut specs = Vec::new();

    for item_node in seq.iter() {
        let item_span = node_to_span(file_id, item_node);

        let item_map = match item_node {
            Node::Mapping(m) => m,
            _ => {
                return Err(ParseError {
                    kind: ParseErrorKind::InvalidType,
                    span: item_span,
                    message: "import entry must be a mapping with 'path' field".to_string(),
                });
            }
        };

        let path = get_string_field(file_id, item_map, "path")?.ok_or_else(|| ParseError {
            kind: ParseErrorKind::MissingField,
            span: item_span,
            message: "import entry requires 'path' field".to_string(),
        })?;

        let prefix = get_string_field(file_id, item_map, "prefix")?;

        specs.push(Spanned::new(
            RawImportSpec {
                path,
                prefix,
                span: item_span,
            },
            item_span,
        ));
    }

    Ok(Some(Spanned::new(specs, outer_span)))
}

/// Parse inputs: parameters with defaults.
///
/// ```yaml
/// inputs:
///   locale: "fr-FR"
///   max_items: 10
/// ```
#[allow(clippy::type_complexity)]
fn parse_inputs(
    file_id: FileId,
    map: &marked_yaml::types::MarkedMappingNode,
) -> Result<Option<Spanned<IndexMap<Spanned<String>, Spanned<serde_json::Value>>>>, ParseError> {
    let inputs_node = match map.get_node("inputs") {
        Some(node) => node,
        None => return Ok(None),
    };

    let inputs_map = match inputs_node {
        Node::Mapping(m) => m,
        _ => {
            return Err(ParseError {
                kind: ParseErrorKind::InvalidType,
                span: node_to_span(file_id, inputs_node),
                message: "inputs must be a mapping".to_string(),
            });
        }
    };

    let span = marked_span_to_span(file_id, inputs_map.span());
    let mut result = IndexMap::new();

    for (k, v) in inputs_map.iter() {
        let key_span = marked_span_to_span(file_id, k.span());
        let key = Spanned::new(k.as_str().to_string(), key_span);
        let val_span = node_to_span(file_id, v);
        let val = Spanned::new(node_to_json(v), val_span);
        result.insert(key, val);
    }

    Ok(Some(Spanned::new(result, span)))
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
    let with_refs = parse_with_refs(file_id, map)?;
    let depends_on = parse_depends_on(file_id, map)?;
    let output = parse_output(file_id, map)?;
    let for_each = parse_for_each(file_id, map)?;
    let retry = parse_retry(file_id, map)?;
    let decompose = parse_decompose(file_id, map)?;
    let structured = parse_structured(file_id, map)?;

    // Parse standalone concurrency/fail_fast (used with decompose when no for_each)
    let standalone_concurrency = if for_each.is_none() {
        get_u32_field(file_id, map, "concurrency")?
    } else {
        None
    };
    let standalone_fail_fast = if for_each.is_none() {
        get_bool_field(file_id, map, "fail_fast")?
    } else {
        None
    };

    let task = RawTask {
        span,
        id,
        description,
        provider,
        model,
        action,
        with_refs,
        depends_on,
        output,
        for_each,
        retry,
        decompose,
        concurrency: standalone_concurrency,
        fail_fast: standalone_fail_fast,
        structured,
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
      prompt: "Research AI trends"
      tools:
        - nika:read
        - nika:write
      max_turns: 10
"#;
        let workflow = parse(yaml, FileId(0)).unwrap();
        let task = workflow.get_task("research").unwrap();

        match &task.value.action {
            Some(RawTaskAction::Agent(action)) => {
                assert_eq!(action.value.prompt.value, "Research AI trends");
                let tools = action.value.tools.as_ref().unwrap();
                assert_eq!(tools.value.len(), 2);
                assert_eq!(tools.value[0].value, "nika:read");
                assert_eq!(action.value.max_iterations.as_ref().unwrap().value, 10);
            }
            _ => panic!("Expected Agent action"),
        }
    }

    #[test]
    fn test_parse_agent_prompt_is_primary_field() {
        let yaml = r#"
schema: "nika/workflow@0.12"
tasks:
  - id: research
    agent:
      prompt: "Research AI trends"
      max_turns: 10
"#;
        let workflow = parse(yaml, FileId(0)).unwrap();
        let task = workflow.get_task("research").unwrap();

        match &task.value.action {
            Some(RawTaskAction::Agent(action)) => {
                // Field must be named `prompt`, not `goal`
                assert_eq!(action.value.prompt.value, "Research AI trends");
            }
            _ => panic!("Expected Agent action"),
        }
    }

    #[test]
    fn test_parse_agent_goal_legacy_fallback() {
        let yaml = r#"
schema: "nika/workflow@0.12"
tasks:
  - id: research
    agent:
      goal: "Legacy goal syntax"
      max_turns: 5
"#;
        let workflow = parse(yaml, FileId(0)).unwrap();
        let task = workflow.get_task("research").unwrap();

        match &task.value.action {
            Some(RawTaskAction::Agent(action)) => {
                // Legacy `goal:` should still work as fallback
                assert_eq!(action.value.prompt.value, "Legacy goal syntax");
            }
            _ => panic!("Expected Agent action"),
        }
    }

    // =========================================================================
    // Task Configuration Parsing Tests
    // =========================================================================

    #[test]
    fn test_parse_with_refs_simple() {
        let yaml = r#"
schema: "nika/workflow@0.12"
tasks:
  - id: step1
    infer: "Generate"
  - id: step2
    with:
      data: step1
    infer: "Process {{with.data}}"
"#;
        let workflow = parse(yaml, FileId(0)).unwrap();
        let task = workflow.get_task("step2").unwrap();

        let with_refs = task.value.with_refs.as_ref().unwrap();
        assert_eq!(with_refs.value.len(), 1);

        let (alias, value) = with_refs.value.iter().next().unwrap();
        assert_eq!(alias.value, "data");
        assert_eq!(value.value, "step1");
    }

    #[test]
    fn test_parse_with_refs_binding_expr() {
        let yaml = r#"
schema: "nika/workflow@0.12"
tasks:
  - id: step1
    infer: "Generate"
  - id: step2
    with:
      data: "step1"
      temp: "step1.data.temp ?? 20"
      cfg: "$env.API_KEY"
      val: "step1.output | upper | trim"
    infer: "Process"
"#;
        let workflow = parse(yaml, FileId(0)).unwrap();
        let task = workflow.get_task("step2").unwrap();

        let with_refs = task.value.with_refs.as_ref().unwrap();
        assert_eq!(with_refs.value.len(), 4);

        let vals: Vec<&str> = with_refs.value.values().map(|v| v.value.as_str()).collect();
        assert_eq!(vals[0], "step1");
        assert_eq!(vals[1], "step1.data.temp ?? 20");
        assert_eq!(vals[2], "$env.API_KEY");
        assert_eq!(vals[3], "step1.output | upper | trim");
    }

    #[test]
    fn test_parse_depends_on_single() {
        let yaml = r#"
schema: "nika/workflow@0.12"
tasks:
  - id: step1
    infer: "Generate"
  - id: step2
    depends_on: step1
    infer: "Process"
"#;
        let workflow = parse(yaml, FileId(0)).unwrap();
        let task = workflow.get_task("step2").unwrap();

        let deps = task.value.depends_on.as_ref().unwrap();
        assert_eq!(deps.value.len(), 1);
        assert_eq!(deps.value[0].value, "step1");
    }

    #[test]
    fn test_parse_depends_on_multiple() {
        let yaml = r#"
schema: "nika/workflow@0.12"
tasks:
  - id: step1
    infer: "Step 1"
  - id: step2
    infer: "Step 2"
  - id: step3
    depends_on: [step1, step2]
    infer: "Process"
"#;
        let workflow = parse(yaml, FileId(0)).unwrap();
        let task = workflow.get_task("step3").unwrap();

        let deps = task.value.depends_on.as_ref().unwrap();
        assert_eq!(deps.value.len(), 2);
        assert_eq!(deps.value[0].value, "step1");
        assert_eq!(deps.value[1].value, "step2");
    }

    #[test]
    fn test_parse_imports() {
        let yaml = r#"
schema: "nika/workflow@0.12"
imports:
  - path: ./partials/setup.nika.yaml
    prefix: setup_
  - path: "pkg:@spn/core@1.0/seo.nika.yaml"
tasks:
  - id: main_task
    infer: "Main logic"
"#;
        let workflow = parse(yaml, FileId(0)).unwrap();

        let imports = workflow.imports.as_ref().unwrap();
        assert_eq!(imports.value.len(), 2);

        assert_eq!(
            imports.value[0].value.path.value,
            "./partials/setup.nika.yaml"
        );
        assert_eq!(
            imports.value[0].value.prefix.as_ref().unwrap().value,
            "setup_"
        );

        assert_eq!(
            imports.value[1].value.path.value,
            "pkg:@spn/core@1.0/seo.nika.yaml"
        );
        assert!(imports.value[1].value.prefix.is_none());
    }

    #[test]
    fn test_parse_inputs() {
        let yaml = r#"
schema: "nika/workflow@0.12"
inputs:
  locale: "fr-FR"
  max_items: 10
  debug: true
tasks:
  - id: main_task
    infer: "Main"
"#;
        let workflow = parse(yaml, FileId(0)).unwrap();

        let inputs = workflow.inputs.as_ref().unwrap();
        assert_eq!(inputs.value.len(), 3);

        let keys: Vec<&str> = inputs.value.keys().map(|k| k.value.as_str()).collect();
        assert_eq!(keys, vec!["locale", "max_items", "debug"]);

        assert_eq!(
            inputs.value.values().next().unwrap().value,
            serde_json::Value::String("fr-FR".to_string())
        );
    }

    #[test]
    fn test_parse_context_config() {
        let yaml = r#"
schema: "nika/workflow@0.12"
context:
  files:
    brand: ./context/brand.md
    data: ./context/data.json
tasks:
  - id: main
    infer: "Use brand: {{context.files.brand}}"
"#;
        let workflow = parse(yaml, FileId(0)).unwrap();

        let ctx = workflow.context.as_ref().unwrap();
        let files = ctx.value.files.as_ref().unwrap();
        assert_eq!(files.len(), 2);
        assert!(files.values().any(|v| v.value == "./context/brand.md"));
    }

    #[test]
    fn test_parse_pkg_config() {
        let yaml = r#"
schema: "nika/workflow@0.12"
pkg:
  include:
    - "github:user/repo"
    - "local:./path"
tasks:
  - id: main
    infer: "Main"
"#;
        let workflow = parse(yaml, FileId(0)).unwrap();

        let pkg = workflow.pkg.as_ref().unwrap();
        assert_eq!(pkg.value.include.len(), 2);
        assert_eq!(pkg.value.include[0].value, "github:user/repo");
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
    infer: "Process {{with.item}}"
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
    for_each: "{{with.items}}"
    infer: "Process"
"#;
        let workflow = parse(yaml, FileId(0)).unwrap();
        let task = workflow.get_task("parallel").unwrap();

        let for_each = task.value.for_each.as_ref().unwrap();
        assert_eq!(for_each.value.items.value, "{{with.items}}");
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
    fn test_parse_agent_missing_prompt() {
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
        assert!(err.message.contains("prompt"));
    }

    #[test]
    fn test_parse_rejects_multiple_verbs_in_task() {
        let yaml = r#"
schema: "nika/workflow@0.12"
tasks:
  - id: ambiguous
    infer: "Generate something"
    exec: "echo hello"
"#;
        let result = parse(yaml, FileId(0));
        assert!(result.is_err(), "task with multiple verbs should be rejected");
        let err = result.unwrap_err();
        assert_eq!(err.kind, ParseErrorKind::InvalidType);
        assert!(
            err.message.contains("multiple verbs"),
            "error should mention multiple verbs, got: {}",
            err.message
        );
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
