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
use super::workflow::{RawContextConfig, RawIncludeSpec, RawPkgConfig, RawWorkflow};
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
    ///
    /// Parse-phase errors use NIKA-160..164 to avoid collision with
    /// the top-level NikaError workflow codes (NIKA-001..005).
    pub fn code(&self) -> &'static str {
        match self {
            Self::Syntax => "NIKA-160",
            Self::MissingField => "NIKA-161",
            Self::InvalidType => "NIKA-162",
            Self::UnknownField => "NIKA-163",
            Self::InvalidSchema => "NIKA-164",
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
            if !value.is_finite() {
                return Err(ParseError {
                    kind: ParseErrorKind::InvalidType,
                    span,
                    message: format!("'{}' must be a finite number (got {})", key, s.as_str()),
                });
            }
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
    // Reject tasks with multiple verbs (must have exactly 0 or 1).
    // `agent: <string>` (scalar) is a preset reference, NOT a verb —
    // only `agent: { ... }` (mapping) counts as the agent verb.
    let verb_keys = ["infer", "exec", "fetch", "invoke", "agent"];
    let found: Vec<&str> = verb_keys
        .iter()
        .filter(|k| {
            let Some(node) = map.get_node(k) else {
                return false;
            };
            // agent: scalar is a preset ref, not a verb
            if **k == "agent" && matches!(node, Node::Scalar(_)) {
                return false;
            }
            true
        })
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
        let mut action = parse_infer_action(file, node)?;
        let span = node_to_span(file, node);

        // For shorthand `infer: "prompt"`, merge task-level LLM fields that
        // would otherwise be silently ignored (they are siblings in the task
        // mapping, not children of the infer node).
        if matches!(node, Node::Scalar(_)) {
            if action.max_tokens.is_none() {
                action.max_tokens = get_u32_field(file, map, "max_tokens")?;
            }
            if action.temperature.is_none() {
                action.temperature = get_f64_field(file, map, "temperature")?;
            }
            if action.system.is_none() {
                action.system = get_string_field(file, map, "system")?;
            }
            if action.extended_thinking.is_none() {
                action.extended_thinking = get_bool_field(file, map, "extended_thinking")?;
            }
            if action.thinking_budget.is_none() {
                action.thinking_budget = get_u32_field(file, map, "thinking_budget")?;
            }
            if action.response_format.is_none() {
                action.response_format = get_string_field(file, map, "response_format")?;
            }
        }

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
        return Ok(Some(RawTaskAction::Fetch(Box::new(Spanned::new(action, span)))));
    }
    // Check for invoke verb
    if let Some(node) = map.get_node("invoke") {
        let action = parse_invoke_action(file, node)?;
        let span = node_to_span(file, node);
        return Ok(Some(RawTaskAction::Invoke(Spanned::new(action, span))));
    }
    // Check for agent verb (mapping only — scalar is a preset reference, handled by caller)
    if let Some(node) = map.get_node("agent") {
        if !matches!(node, Node::Scalar(_)) {
            let action = parse_agent_action(file, node)?;
            let span = node_to_span(file, node);
            return Ok(Some(RawTaskAction::Agent(Box::new(Spanned::new(
                action, span,
            )))));
        }
    }

    // No verb found — check for common misspellings before returning None.
    // Known non-verb task keys that are legitimate without a verb (e.g. decompose tasks).
    let known_non_verb_keys: &[&str] = &[
        "id",
        "description",
        "provider",
        "model",
        "with",
        "depends_on",
        "output",
        "for_each",
        "retry",
        "decompose",
        "structured",
        "artifact",
        "log",
        "concurrency",
        "fail_fast",
        "timeout",
    ];

    let task_keys: Vec<String> = map.iter().map(|(k, _)| k.as_str().to_string()).collect();
    let unrecognized: Vec<&str> = task_keys
        .iter()
        .map(|s| s.as_str())
        .filter(|k| !verb_keys.contains(k) && !known_non_verb_keys.contains(k))
        .collect();

    if !unrecognized.is_empty() {
        // Check if any unrecognized key looks like a misspelled verb
        let misspellings: Vec<(&str, &str)> = unrecognized
            .iter()
            .filter_map(|key| {
                verb_keys.iter().find_map(|verb| {
                    if is_likely_misspelling(key, verb) {
                        Some((*key, *verb))
                    } else {
                        None
                    }
                })
            })
            .collect();

        if !misspellings.is_empty() {
            let suggestions: Vec<String> = misspellings
                .iter()
                .map(|(key, verb)| format!("'{}' (did you mean '{}'?)", key, verb))
                .collect();
            let span = marked_span_to_span(file, map.span());
            return Err(ParseError {
                kind: ParseErrorKind::MissingField,
                span,
                message: format!(
                    "no valid verb found. Expected one of: {}. Possible misspelling: {}",
                    verb_keys.join(", "),
                    suggestions.join(", ")
                ),
            });
        }
    }

    Ok(None)
}

/// Check if `input` is a likely misspelling of `target` using edit distance.
/// Returns true if the strings are within edit distance 2 and share a common prefix.
fn is_likely_misspelling(input: &str, target: &str) -> bool {
    if input.len() > 256 || target.len() > 256 {
        return false;
    }
    if input == target {
        return false;
    }
    let len_diff = (input.len() as isize - target.len() as isize).unsigned_abs();
    if len_diff > 2 {
        return false;
    }
    // Simple Levenshtein distance check (bounded to 2)
    levenshtein_bounded(input, target, 2) <= 2
}

/// Bounded Levenshtein distance. Returns distance or `bound + 1` if exceeded.
fn levenshtein_bounded(a: &str, b: &str, bound: usize) -> usize {
    let a_bytes = a.as_bytes();
    let b_bytes = b.as_bytes();
    let m = a_bytes.len();
    let n = b_bytes.len();

    if m.abs_diff(n) > bound {
        return bound + 1;
    }

    let mut prev: Vec<usize> = (0..=n).collect();
    let mut curr = vec![0; n + 1];

    for i in 1..=m {
        curr[0] = i;
        let mut min_in_row = curr[0];
        for j in 1..=n {
            let cost = if a_bytes[i - 1] == b_bytes[j - 1] {
                0
            } else {
                1
            };
            curr[j] = (prev[j] + 1).min(curr[j - 1] + 1).min(prev[j - 1] + cost);
            min_in_row = min_in_row.min(curr[j]);
        }
        if min_in_row > bound {
            return bound + 1;
        }
        std::mem::swap(&mut prev, &mut curr);
    }
    prev[n]
}

/// Parse infer action - supports both shorthand (string) and full form (mapping).
///
/// # Content / Prompt Rules
///
/// - `infer: "prompt"` — shorthand, text-only
/// - `infer: { prompt: "..." }` — full form, text-only
/// - `infer: { content: [...] }` — vision mode, prompt optional
/// - `infer: { prompt: "...", content: [...] }` — prompt prepended as first Text part
/// - Error if neither `prompt` nor `content` is present
fn parse_infer_action(file: FileId, node: &Node) -> Result<RawInferAction, ParseError> {
    let span = node_to_span(file, node);

    match node {
        // Shorthand: infer: "prompt string"
        Node::Scalar(s) => Ok(RawInferAction {
            prompt: Spanned::new(s.as_str().to_string(), span),
            system: None,
            temperature: None,
            max_tokens: None,
            extended_thinking: None,
            thinking_budget: None,
            content: None,
            response_format: None,
            guardrails: Vec::new(),
        }),
        // Full form: infer: { prompt: "...", temperature: ..., content: [...] }
        Node::Mapping(m) => {
            let prompt = get_string_field(file, m, "prompt")?;
            let content = parse_content_field(file, m)?;

            // Require at least one of prompt or content
            if prompt.is_none() && content.is_none() {
                return Err(ParseError {
                    kind: ParseErrorKind::MissingField,
                    span,
                    message: "infer action requires 'prompt' or 'content' field".to_string(),
                });
            }

            // If content is present but no prompt, use empty prompt (validated later)
            let prompt = prompt.unwrap_or_else(|| Spanned::new(String::new(), span));

            let guardrails = parse_guardrails_field(file, m)?;

            // Detect commonly misplaced keys inside infer: block
            let misplaced_keys: Vec<&str> = ["provider", "model", "base_url"]
                .iter()
                .filter(|k| m.get_node(k).is_some())
                .copied()
                .collect();
            if !misplaced_keys.is_empty() {
                return Err(ParseError {
                    kind: ParseErrorKind::UnknownField,
                    span,
                    message: format!(
                        "{} must be set at task level, not inside the infer: block",
                        misplaced_keys.join(", ")
                    ),
                });
            }

            Ok(RawInferAction {
                prompt,
                system: get_string_field(file, m, "system")?,
                temperature: get_f64_field(file, m, "temperature")?,
                max_tokens: get_u32_field(file, m, "max_tokens")?,
                extended_thinking: get_bool_field(file, m, "extended_thinking")?,
                thinking_budget: get_u32_field(file, m, "thinking_budget")?,
                content,
                response_format: get_string_field(file, m, "response_format")?,
                guardrails,
            })
        }
        _ => Err(ParseError {
            kind: ParseErrorKind::InvalidType,
            span,
            message: "infer must be a string or mapping".to_string(),
        }),
    }
}

/// Parse the `content:` field from an infer mapping.
///
/// Returns `None` if the field is absent. Parses a YAML sequence where each
/// element is a mapping with `type`, and type-specific fields.
fn parse_content_field(
    file: FileId,
    map: &marked_yaml::types::MarkedMappingNode,
) -> Result<Option<Spanned<Vec<crate::ast::content::RawContentPart>>>, ParseError> {
    use crate::ast::content::RawContentPart;

    let node = match map.get_node("content") {
        Some(n) => n,
        None => return Ok(None),
    };

    let span = node_to_span(file, node);

    let seq = match node {
        Node::Sequence(s) => s,
        _ => {
            return Err(ParseError {
                kind: ParseErrorKind::InvalidType,
                span,
                message: "content must be a sequence".to_string(),
            });
        }
    };

    if seq.is_empty() {
        return Err(ParseError {
            kind: ParseErrorKind::InvalidType,
            span,
            message: "content must not be empty".to_string(),
        });
    }

    let mut parts = Vec::with_capacity(seq.len());

    for item in seq.iter() {
        let item_span = node_to_span(file, item);
        let m = match item {
            Node::Mapping(m) => m,
            _ => {
                return Err(ParseError {
                    kind: ParseErrorKind::InvalidType,
                    span: item_span,
                    message: "each content part must be a mapping with 'type' field".to_string(),
                });
            }
        };

        let type_field = get_string_field(file, m, "type")?.ok_or_else(|| ParseError {
            kind: ParseErrorKind::MissingField,
            span: item_span,
            message: "content part requires 'type' field".to_string(),
        })?;

        let part = match type_field.value.as_str() {
            "text" => {
                let text = get_string_field(file, m, "text")?.ok_or_else(|| ParseError {
                    kind: ParseErrorKind::MissingField,
                    span: item_span,
                    message: "text content part requires 'text' field".to_string(),
                })?;
                RawContentPart::Text { text }
            }
            "image" => {
                let source = get_string_field(file, m, "source")?.ok_or_else(|| ParseError {
                    kind: ParseErrorKind::MissingField,
                    span: item_span,
                    message: "image content part requires 'source' field".to_string(),
                })?;
                let detail = get_string_field(file, m, "detail")?;
                RawContentPart::Image { source, detail }
            }
            "image_url" => {
                let url = get_string_field(file, m, "url")?.ok_or_else(|| ParseError {
                    kind: ParseErrorKind::MissingField,
                    span: item_span,
                    message: "image_url content part requires 'url' field".to_string(),
                })?;
                let detail = get_string_field(file, m, "detail")?;
                RawContentPart::ImageUrl { url, detail }
            }
            other => {
                return Err(ParseError {
                    kind: ParseErrorKind::InvalidType,
                    span: type_field.span,
                    message: format!(
                        "unknown content part type '{}', expected: text, image, image_url",
                        other
                    ),
                });
            }
        };

        parts.push(part);
    }

    Ok(Some(Spanned::new(parts, span)))
}

/// Parse exec action - supports both shorthand (string) and full form (mapping).
fn parse_exec_action(file: FileId, node: &Node) -> Result<RawExecAction, ParseError> {
    let span = node_to_span(file, node);

    match node {
        // Shorthand: exec: "command string"
        Node::Scalar(s) => Ok(RawExecAction {
            command: Spanned::new(s.as_str().to_string(), span),
            shell: None,
            cwd: None,
            env: None,
            timeout_ms: None,
            max_stdout: None,
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
                cwd: get_string_field(file, m, "cwd")?,
                env: parse_string_map(file, m, "env")?,
                // timeout_ms is the primary field (milliseconds).
                // timeout is the schema alias (seconds) — convert to ms.
                timeout_ms: match get_u64_field(file, m, "timeout_ms")? {
                    Some(v) => Some(v),
                    None => get_u64_field(file, m, "timeout")?
                        .map(|s| Spanned::new(s.value.saturating_mul(1000), s.span)),
                },
                max_stdout: get_u64_field(file, m, "max_stdout")?,
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

    let extract = get_string_field(file, m, "extract")?;
    let selector = get_string_field(file, m, "selector")?;

    Ok(RawFetchAction {
        url,
        method: get_string_field(file, m, "method")?,
        headers: parse_string_map(file, m, "headers")?,
        body: get_string_field(file, m, "body")?,
        json: parse_json_value(file, m, "json")?,
        timeout_ms: match get_u64_field(file, m, "timeout_ms")? {
            Some(v) => Some(v),
            None => get_u64_field(file, m, "timeout")?
                .map(|s| Spanned::new(s.value.saturating_mul(1000), s.span)),
        },
        follow_redirects: get_bool_field(file, m, "follow_redirects")?,
        response: get_string_field(file, m, "response")?,
        extract,
        selector,
        session: get_bool_field(file, m, "session")?,
        cache: get_bool_field(file, m, "cache")?,
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

    let tool = get_string_field(file, m, "tool")?;
    let resource = get_string_field(file, m, "resource")?;

    if tool.is_none() && resource.is_none() {
        return Err(ParseError {
            kind: ParseErrorKind::MissingField,
            span,
            message: "invoke action requires 'tool' or 'resource' field".to_string(),
        });
    }

    Ok(RawInvokeAction {
        tool,
        resource,
        params: parse_json_value(file, m, "params")?,
        mcp: get_string_field(file, m, "mcp")?.or(get_string_field(file, m, "server")?),
        timeout_ms: match get_u64_field(file, m, "timeout_ms")? {
            Some(v) => Some(v),
            None => get_u64_field(file, m, "timeout")?
                .map(|s| Spanned::new(s.value.saturating_mul(1000), s.span)),
        },
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

    let prompt = get_string_field(file, m, "prompt")?.ok_or_else(|| ParseError {
        kind: ParseErrorKind::MissingField,
        span,
        message: "agent action requires 'prompt' field".to_string(),
    })?;

    Ok(RawAgentAction {
        prompt,
        tools: parse_string_array(file, m, "tools")?,
        max_turns: get_u32_field(file, m, "max_turns")?,
        max_tokens: get_u32_field(file, m, "max_tokens")?,
        from: get_string_field(file, m, "from")?,
        skills: parse_string_array(file, m, "skills")?,
        provider: get_string_field(file, m, "provider")?,
        model: get_string_field(file, m, "model")?,
        mcp: parse_string_array(file, m, "mcp")?,
        system: get_string_field(file, m, "system")?,
        temperature: get_f64_field(file, m, "temperature")?,
        token_budget: get_u32_field(file, m, "token_budget")?,
        extended_thinking: get_bool_field(file, m, "extended_thinking")?,
        thinking_budget: get_u32_field(file, m, "thinking_budget")?,
        depth_limit: get_u32_field(file, m, "depth_limit")?,
        tool_choice: get_string_field(file, m, "tool_choice")?,
        stop_sequences: parse_string_array(file, m, "stop_sequences")?,
        scope: get_string_field(file, m, "scope")?,
        guardrails: parse_guardrails_field(file, m)?,
        completion: parse_optional_serde_field(file, m, "completion")?,
        limits: parse_optional_serde_field(file, m, "limits")?,
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
            message: "depends_on/flow must be a string or array of strings".to_string(),
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
            let items_str = serde_json::to_string(&arr).map_err(|e| ParseError {
                kind: ParseErrorKind::InvalidType,
                span,
                message: format!("failed to serialize for_each items: {}", e),
            })?;

            Ok(Some(Spanned::new(
                RawForEach {
                    items: Spanned::new(items_str, span),
                    as_var: get_string_field(file, map, "as")?,
                    concurrency: get_u32_field(file, map, "concurrency")?,
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
                    concurrency: get_u32_field(file, map, "concurrency")?,
                    fail_fast: get_bool_field(file, map, "fail_fast")?,
                },
                span,
            )))
        }
        Some(Node::Mapping(m)) => {
            // Object form: for_each: { items: ..., as: ..., concurrency: ..., fail_fast: ... }
            let span = marked_span_to_span(file, m.span());
            let items = match m.get_node("items") {
                Some(Node::Sequence(seq)) => {
                    let arr: Vec<serde_json::Value> = seq.iter().map(node_to_json).collect();
                    let items_str = serde_json::to_string(&arr).map_err(|e| ParseError {
                        kind: ParseErrorKind::InvalidType,
                        span,
                        message: format!("failed to serialize for_each items: {}", e),
                    })?;
                    Spanned::new(items_str, marked_span_to_span(file, seq.span()))
                }
                Some(Node::Scalar(s)) => {
                    Spanned::new(s.as_str().to_string(), marked_span_to_span(file, s.span()))
                }
                _ => {
                    return Err(ParseError {
                        kind: ParseErrorKind::MissingField,
                        span,
                        message: "for_each object form requires 'items' field".to_string(),
                    });
                }
            };
            // Inner mapping fields take precedence, fall back to task-level
            let as_var = get_string_field(file, m, "as")?.or(get_string_field(file, map, "as")?);
            let concurrency =
                get_u32_field(file, m, "concurrency")?.or(get_u32_field(file, map, "concurrency")?);
            let fail_fast =
                get_bool_field(file, m, "fail_fast")?.or(get_bool_field(file, map, "fail_fast")?);

            Ok(Some(Spanned::new(
                RawForEach {
                    items,
                    as_var,
                    concurrency,
                    fail_fast,
                },
                span,
            )))
        }
        // All Node variants (Sequence, Scalar, Mapping) are covered above.
        // This branch is kept for forward-compatibility if Node gains new variants.
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
                    max_attempts: get_u32_field(file, m, "max_attempts")?.or(get_u32_field(
                        file,
                        m,
                        "max_retries",
                    )?),
                    delay_ms: match get_u64_field(file, m, "delay_ms")? {
                        Some(v) => Some(v),
                        None => get_u64_field(file, m, "delay")?
                            .map(|s| Spanned::new(s.value.saturating_mul(1000), s.span)),
                    },
                    backoff: get_f64_field(file, m, "backoff")?,
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
                    schema_ref: get_string_field(file, m, "schema_ref")?,
                    max_retries: get_u32_field(file, m, "max_retries")?,
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

/// Parse the `guardrails:` field from an infer or agent mapping.
///
/// Guardrails are a YAML sequence of objects, each with a `type` field.
/// Uses serde deserialization via `GuardrailConfig` which is `#[serde(tag = "type")]`.
///
/// Returns an empty Vec if the field is absent.
fn parse_guardrails_field(
    file: FileId,
    map: &marked_yaml::types::MarkedMappingNode,
) -> Result<Vec<crate::ast::guardrails::GuardrailConfig>, ParseError> {
    match map.get_node("guardrails") {
        Some(node) => {
            let span = node_to_span(file, node);
            let json_value = node_to_json(node);
            serde_json::from_value(json_value).map_err(|e| ParseError {
                kind: ParseErrorKind::InvalidType,
                span,
                message: format!("invalid guardrails config: {e}"),
            })
        }
        None => Ok(Vec::new()),
    }
}

fn parse_optional_serde_field<T: serde::de::DeserializeOwned>(
    file: FileId,
    map: &marked_yaml::types::MarkedMappingNode,
    field_name: &str,
) -> Result<Option<T>, ParseError> {
    match map.get_node(field_name) {
        Some(node) => {
            let span = node_to_span(file, node);
            let json_value = node_to_json(node);
            let parsed = serde_json::from_value(json_value).map_err(|e| ParseError {
                kind: ParseErrorKind::InvalidType,
                span,
                message: format!("invalid {field_name} config: {e}"),
            })?;
            Ok(Some(parsed))
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
        let message = if matches!(&e, LoadError::UnexpectedAnchor(_)) {
            "YAML anchors (&/*) are not supported. \
             Use `include:` with `prefix:` for shared task definitions. \
             See: https://github.com/SuperNovae-studio/nika/blob/main/docs/adr/0001-yaml-anchors-not-supported.md"
                .to_string()
        } else {
            format!("YAML syntax error: {}", e)
        };
        ParseError {
            kind: ParseErrorKind::Syntax,
            span,
            message,
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
    workflow.goal = get_string_field(file_id, map, "goal")?;
    workflow.provider = get_string_field(file_id, map, "provider")?;
    workflow.model = get_string_field(file_id, map, "model")?;
    workflow.base_url = get_string_field(file_id, map, "base_url")?;

    // Parse MCP server configurations
    workflow.mcp = parse_mcp_config(file_id, map)?;

    // Parse pkg configuration
    workflow.pkg = parse_pkg_config(file_id, map)?;

    // Parse context configuration
    workflow.context = parse_context_config(file_id, map)?;

    // Parse include
    workflow.include = parse_include(file_id, map)?;

    // Parse inputs
    workflow.inputs = parse_inputs(file_id, map)?;

    // Parse artifacts config
    workflow.artifacts = match map.get_node("artifacts") {
        Some(node) => {
            let span = node_to_span(file_id, node);
            Some(Spanned::new(node_to_json(node), span))
        }
        None => None,
    };

    // Parse log config
    workflow.log = match map.get_node("log") {
        Some(node) => {
            let span = node_to_span(file_id, node);
            Some(Spanned::new(node_to_json(node), span))
        }
        None => None,
    };

    // Parse agents config
    workflow.agents = match map.get_node("agents") {
        Some(node) => {
            let span = node_to_span(file_id, node);
            Some(Spanned::new(node_to_json(node), span))
        }
        None => None,
    };

    // Parse workflow-level skills mapping (alias -> path)
    workflow.skills = parse_string_map(file_id, map, "skills")?;

    // Parse orchestrate configuration (goal-driven agent loop)
    workflow.orchestrate = match map.get_node("orchestrate") {
        Some(node) => {
            let span = node_to_span(file_id, node);
            Some(Spanned::new(node_to_json(node), span))
        }
        None => None,
    };

    // Parse routing configuration (fallback chains, smart routing)
    workflow.routing = match map.get_node("routing") {
        Some(node) => {
            let span = node_to_span(file_id, node);
            Some(Spanned::new(node_to_json(node), span))
        }
        None => None,
    };

    // Parse global workflow timeout
    workflow.max_duration_secs = get_u64_field(file_id, map, "max_duration_secs")?;

    // Parse tasks
    workflow.tasks = parse_tasks(file_id, map)?;

    // ── Validate no unknown workflow keys (NIKA-163) ─────────────────
    let known_workflow_keys: &[&str] = &[
        "schema",
        "workflow",
        "description",
        "goal",
        "provider",
        "model",
        "base_url",
        "mcp",
        "pkg",
        "context",
        "include",
        "inputs",
        "artifacts",
        "log",
        "agents",
        "skills",
        "orchestrate",
        "routing",
        "max_duration_secs",
        "tasks",
    ];
    for (key, _) in map.iter() {
        let key_str = key.as_str();
        if !known_workflow_keys.contains(&key_str) {
            let span = marked_span_to_span(file_id, key.span());
            // Guard against pathologically long keys before Levenshtein
            if key_str.len() > 256 {
                return Err(ParseError {
                    kind: ParseErrorKind::UnknownField,
                    span,
                    message: format!(
                        "unknown workflow field '{}...' (key too long)",
                        &key_str[..32]
                    ),
                });
            }
            let suggestion = known_workflow_keys
                .iter()
                .find(|k| is_likely_misspelling(key_str, k))
                .map(|k| format!("did you mean '{}'?", k));
            return Err(ParseError {
                kind: ParseErrorKind::UnknownField,
                span,
                message: match suggestion {
                    Some(ref s) => format!("unknown workflow field '{}'. {}", key_str, s),
                    None => format!(
                        "unknown workflow field '{}'. Known fields: {}",
                        key_str,
                        known_workflow_keys.join(", ")
                    ),
                },
            });
        }
    }

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
        from: get_string_field(file_id, map, "from")?,
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

/// Parse include: specification.
///
/// ```yaml
/// include:
///   - path: ./partials/setup.nika.yaml
///     prefix: setup_
///   - path: pkg:@nika/core@1.0/seo.nika.yaml
/// ```
fn parse_include(
    file_id: FileId,
    map: &marked_yaml::types::MarkedMappingNode,
) -> Result<Option<Spanned<Vec<Spanned<RawIncludeSpec>>>>, ParseError> {
    let include_node = match map.get_node("include") {
        Some(node) => node,
        None => return Ok(None),
    };

    let seq = match include_node {
        Node::Sequence(s) => s,
        _ => {
            return Err(ParseError {
                kind: ParseErrorKind::InvalidType,
                span: node_to_span(file_id, include_node),
                message: "include must be a sequence".to_string(),
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
                    message: "include entry must be a mapping with 'path' field".to_string(),
                });
            }
        };

        let path = get_string_field(file_id, item_map, "path")?.ok_or_else(|| ParseError {
            kind: ParseErrorKind::MissingField,
            span: item_span,
            message: "include entry requires 'path' field".to_string(),
        })?;

        let prefix = get_string_field(file_id, item_map, "prefix")?;

        specs.push(Spanned::new(
            RawIncludeSpec {
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
            let mut seen_ids = std::collections::HashSet::new();
            let mut tasks = Vec::with_capacity(seq.len());
            for task_node in seq.iter() {
                let task = parse_task(file_id, task_node)?;
                let task_id = &task.value.id.value;
                if !seen_ids.insert(task_id.clone()) {
                    return Err(ParseError {
                        kind: ParseErrorKind::InvalidType,
                        span: task.value.id.span,
                        message: format!("duplicate task id '{}'", task_id),
                    });
                }
                tasks.push(task);
            }
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

/// Validate that a task mapping contains only known keys.
///
/// Called after `parse_action()` succeeds (verb is present) to catch typos like
/// `dependson:` that would otherwise be silently ignored.
fn validate_task_keys(
    file_id: FileId,
    map: &marked_yaml::types::MarkedMappingNode,
) -> Result<(), ParseError> {
    const KNOWN_TASK_KEYS: &[&str] = &[
        "id",
        "description",
        "provider",
        "model",
        "base_url",
        "preset",
        "with",
        "depends_on",
        "output",
        "for_each",
        "as",
        "retry",
        "decompose",
        "structured",
        "artifact",
        "routing",
        "record",
        "context_budget",
        "log",
        "concurrency",
        "fail_fast",
        "timeout",
        "when",
        // 5 verb keys
        "infer",
        "exec",
        "fetch",
        "invoke",
        "agent",
        // Infer shorthand siblings
        "max_tokens",
        "temperature",
        "system",
        "extended_thinking",
        "thinking_budget",
        "response_format",
    ];

    for (key, _) in map.iter() {
        let key_str = key.as_str();
        if !KNOWN_TASK_KEYS.contains(&key_str) {
            let span = marked_span_to_span(file_id, key.span());
            // Guard against pathologically long keys
            if key_str.len() > 256 {
                return Err(ParseError {
                    kind: ParseErrorKind::UnknownField,
                    span,
                    message: format!("unknown task field '{}...' (key too long)", &key_str[..32]),
                });
            }
            // Common mistakes that are too far for Levenshtein to catch
            let explicit_suggestion = match key_str {
                "use" => Some("did you mean 'with'?"),
                "max_retries" => {
                    Some("did you mean 'retry: { max_attempts: N }'? (max_retries is only valid inside structured:)")
                }
                _ => None,
            };
            let suggestion = explicit_suggestion
                .map(|s| format!(" ({})", s))
                .or_else(|| {
                    KNOWN_TASK_KEYS
                        .iter()
                        .find(|k| is_likely_misspelling(key_str, k))
                        .map(|k| format!(" (did you mean '{}'?)", k))
                });
            return Err(ParseError {
                kind: ParseErrorKind::UnknownField,
                span,
                message: format!(
                    "unknown task field '{}'{}",
                    key_str,
                    suggestion.unwrap_or_default()
                ),
            });
        }
    }

    Ok(())
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
    let model = get_string_field(file_id, map, "model")?;
    let base_url = get_string_field(file_id, map, "base_url")?;
    let preset = get_string_field(file_id, map, "preset")?;

    // Parse provider: string or array (fallback chain).
    // `provider: anthropic` → single provider
    // `provider: [groq, anthropic]` → first is primary, full list becomes routing.fallback
    let (provider, provider_chain) = match map.get_node("provider") {
        Some(Node::Scalar(_)) => {
            let p = extract_string(file_id, map.get_node("provider").unwrap())?;
            (Some(p), None)
        }
        Some(Node::Sequence(seq)) => {
            let span = marked_span_to_span(file_id, seq.span());
            let items: Result<Vec<_>, _> = seq
                .iter()
                .map(|node| extract_string(file_id, node))
                .collect();
            let items = items?;
            if items.is_empty() {
                return Err(ParseError {
                    kind: ParseErrorKind::InvalidType,
                    span,
                    message: "provider array must have at least one entry".to_string(),
                });
            }
            let primary = items[0].clone();
            let chain: Vec<String> = items.into_iter().map(|s| s.into_inner()).collect();
            (Some(primary), Some(chain))
        }
        Some(_) => {
            let node = map.get_node("provider").unwrap();
            return Err(ParseError {
                kind: ParseErrorKind::InvalidType,
                span: node_to_span(file_id, node),
                message: "provider must be a string or array of strings".to_string(),
            });
        }
        None => (None, None),
    };

    // Parse all task fields
    let action = parse_action(file_id, map)?;

    // `agent: <string>` (scalar) is a preset reference — merge into preset field.
    // This is ergonomic sugar: `agent: think` ≡ `preset: think`.
    // If both `agent: think` and `preset: other` are set, `agent:` wins.
    let preset = match map.get_node("agent") {
        Some(Node::Scalar(_)) => {
            let agent_str = extract_string(file_id, map.get_node("agent").unwrap())?;
            Some(agent_str)
        }
        _ => preset,
    };

    // ── Validate no unknown task keys (NIKA-163) when a verb IS present ──
    if action.is_some() {
        validate_task_keys(file_id, map)?;
    }

    let with_refs = parse_with_refs(file_id, map)?;
    let depends_on = parse_depends_on(file_id, map)?;
    let output = parse_output(file_id, map)?;
    let for_each = parse_for_each(file_id, map)?;
    let retry = parse_retry(file_id, map)?;
    let decompose = parse_decompose(file_id, map)?;
    let structured = parse_structured(file_id, map)?;

    // Parse artifact: config (task-level artifact output)
    let artifact = match map.get_node("artifact") {
        Some(node) => {
            let span = node_to_span(file_id, node);
            let value = node_to_json(node);
            Some(Spanned::new(value, span))
        }
        None => None,
    };

    // Parse routing: config (task-level routing override).
    // If provider was an array, auto-populate routing.fallback from the chain.
    let routing = match (map.get_node("routing"), &provider_chain) {
        (Some(node), _) => {
            // Explicit routing: block takes priority
            let span = node_to_span(file_id, node);
            let value = node_to_json(node);
            Some(Spanned::new(value, span))
        }
        (None, Some(chain)) => {
            // Auto-generate routing from provider array
            let fallback_arr: Vec<serde_json::Value> = chain
                .iter()
                .map(|s| serde_json::Value::String(s.clone()))
                .collect();
            let value = serde_json::json!({ "fallback": fallback_arr });
            Some(Spanned::new(value, span))
        }
        (None, None) => None,
    };

    // Parse log: config (task-level log override)
    let log = match map.get_node("log") {
        Some(node) => {
            let span = node_to_span(file_id, node);
            let value = node_to_json(node);
            Some(Spanned::new(value, span))
        }
        None => None,
    };

    // Parse record: config (task-level output compression)
    let record = match map.get_node("record") {
        Some(node) => {
            let span = node_to_span(file_id, node);
            let value = node_to_json(node);
            Some(Spanned::new(value, span))
        }
        None => None,
    };

    // Parse context_budget: token limit for binding truncation
    let context_budget = get_u32_field(file_id, map, "context_budget")?;
    let when = get_string_field(file_id, map, "when")?;

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
        base_url,
        preset,
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
        routing,
        artifact,
        log,
        record,
        context_budget,
        when,
    };

    Ok(Spanned::new(task, span))
}

#[cfg(test)]
mod tests {
    use super::*;

    const SIMPLE_WORKFLOW: &str = r#"
schema: "nika/workflow@0.12"
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

        assert_eq!(workflow.schema.value, "nika/workflow@0.12");
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
        let yaml = r#"schema: "nika/workflow@0.12"
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
schema: "nika/workflow@0.12"
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
    fn test_parse_infer_shorthand_with_task_level_max_tokens_and_temperature() {
        let yaml = r#"
schema: "nika/workflow@0.12"
tasks:
  - id: test
    infer: "Say hello"
    max_tokens: 20
    temperature: 0.5
"#;
        let workflow = parse(yaml, FileId(0)).unwrap();
        let task = workflow.get_task("test").unwrap();

        match &task.value.action {
            Some(RawTaskAction::Infer(action)) => {
                assert_eq!(action.value.prompt.value, "Say hello");
                assert_eq!(action.value.max_tokens.as_ref().unwrap().value, 20);
                assert!((action.value.temperature.as_ref().unwrap().value - 0.5).abs() < 0.001);
            }
            _ => panic!("Expected Infer action"),
        }
    }

    #[test]
    fn test_parse_infer_shorthand_with_task_level_system() {
        let yaml = r#"
schema: "nika/workflow@0.12"
tasks:
  - id: test
    infer: "Translate this"
    system: "You are a translator"
    temperature: 0.3
"#;
        let workflow = parse(yaml, FileId(0)).unwrap();
        let task = workflow.get_task("test").unwrap();

        match &task.value.action {
            Some(RawTaskAction::Infer(action)) => {
                assert_eq!(action.value.prompt.value, "Translate this");
                assert_eq!(
                    action.value.system.as_ref().unwrap().value,
                    "You are a translator"
                );
                assert!((action.value.temperature.as_ref().unwrap().value - 0.3).abs() < 0.001);
            }
            _ => panic!("Expected Infer action"),
        }
    }

    #[test]
    fn test_parse_infer_shorthand_with_all_task_level_fields() {
        let yaml = r#"
schema: "nika/workflow@0.12"
tasks:
  - id: test
    infer: "Think deeply"
    system: "Be thorough"
    max_tokens: 4096
    temperature: 0.9
    extended_thinking: true
    thinking_budget: 8000
    response_format: json
"#;
        let workflow = parse(yaml, FileId(0)).unwrap();
        let task = workflow.get_task("test").unwrap();

        match &task.value.action {
            Some(RawTaskAction::Infer(action)) => {
                assert_eq!(action.value.prompt.value, "Think deeply");
                assert_eq!(action.value.system.as_ref().unwrap().value, "Be thorough");
                assert_eq!(action.value.max_tokens.as_ref().unwrap().value, 4096);
                assert!((action.value.temperature.as_ref().unwrap().value - 0.9).abs() < 0.001);
                assert!(action.value.extended_thinking.as_ref().unwrap().value);
                assert_eq!(action.value.thinking_budget.as_ref().unwrap().value, 8000);
                assert_eq!(action.value.response_format.as_ref().unwrap().value, "json");
            }
            _ => panic!("Expected Infer action"),
        }
    }

    #[test]
    fn test_parse_infer_full_form() {
        let yaml = r#"
schema: "nika/workflow@0.12"
tasks:
  - id: generate
    infer:
      prompt: "Generate content"
      system: "You are a helpful assistant"
      temperature: 0.7
      max_tokens: 1000
      extended_thinking: true
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
                assert!(action.value.extended_thinking.as_ref().unwrap().value);
                assert_eq!(action.value.thinking_budget.as_ref().unwrap().value, 8000);
            }
            _ => panic!("Expected Infer action"),
        }
    }

    #[test]
    fn test_parse_infer_rejects_misplaced_provider() {
        let yaml = r#"
schema: "nika/workflow@0.12"
tasks:
  - id: test
    infer:
      prompt: "Hello"
      provider: "groq"
"#;
        let err = parse(yaml, FileId(0)).unwrap_err();
        assert!(
            err.message.contains("task level"),
            "Error should hint about task level: {}",
            err.message
        );
    }

    #[test]
    fn test_parse_infer_rejects_misplaced_model() {
        let yaml = r#"
schema: "nika/workflow@0.12"
tasks:
  - id: test
    infer:
      prompt: "Hello"
      model: "gpt-4o"
"#;
        let err = parse(yaml, FileId(0)).unwrap_err();
        assert!(err.message.contains("task level"), "{}", err.message);
    }

    #[test]
    fn test_parse_exec_shorthand() {
        let yaml = r#"
schema: "nika/workflow@0.12"
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
schema: "nika/workflow@0.12"
tasks:
  - id: build
    exec:
      command: "npm run build"
      shell: true
      cwd: "/app"
      timeout: 30
      env:
        NODE_ENV: production
"#;
        let workflow = parse(yaml, FileId(0)).unwrap();
        let task = workflow.get_task("build").unwrap();

        match &task.value.action {
            Some(RawTaskAction::Exec(action)) => {
                assert_eq!(action.value.command.value, "npm run build");
                assert!(action.value.shell.as_ref().unwrap().value);
                assert_eq!(action.value.cwd.as_ref().unwrap().value, "/app");
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
schema: "nika/workflow@0.12"
tasks:
  - id: api_call
    fetch:
      url: "https://api.example.com/data"
      method: POST
      headers:
        Authorization: "Bearer token"
      timeout: 5
"#;
        let workflow = parse(yaml, FileId(0)).unwrap();
        let task = workflow.get_task("api_call").unwrap();

        match &task.value.action {
            Some(RawTaskAction::Fetch(action)) => {
                assert_eq!(action.value.url.value, "https://api.example.com/data");
                assert_eq!(action.value.method.as_ref().unwrap().value, "POST");
                assert_eq!(action.value.timeout_ms.as_ref().unwrap().value, 5000); // 5 seconds * 1000
                let headers = action.value.headers.as_ref().unwrap();
                assert!(headers.value.values().any(|v| v.value.contains("Bearer")));
            }
            _ => panic!("Expected Fetch action"),
        }
    }

    #[test]
    fn test_parse_invoke_action() {
        let yaml = r#"
schema: "nika/workflow@0.12"
tasks:
  - id: mcp_call
    invoke:
      tool: novanet_context
      mcp: novanet
      params:
        entity: "qr-code"
        locale: "fr-FR"
"#;
        let workflow = parse(yaml, FileId(0)).unwrap();
        let task = workflow.get_task("mcp_call").unwrap();

        match &task.value.action {
            Some(RawTaskAction::Invoke(action)) => {
                assert_eq!(action.value.tool.as_ref().unwrap().value, "novanet_context");
                assert_eq!(action.value.mcp.as_ref().unwrap().value, "novanet");
                assert!(action.value.params.is_some());
            }
            _ => panic!("Expected Invoke action"),
        }
    }

    #[test]
    fn test_parse_agent_action() {
        let yaml = r#"
schema: "nika/workflow@0.12"
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
                assert_eq!(action.value.max_turns.as_ref().unwrap().value, 10);
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
    fn test_parse_agent_goal_removed() {
        let yaml = r#"
schema: "nika/workflow@0.12"
tasks:
  - id: research
    agent:
      goal: "Legacy goal syntax"
      max_turns: 5
"#;
        let result = parse(yaml, FileId(0));
        assert!(result.is_err(), "goal alias should be rejected");
        let err = result.unwrap_err();
        assert_eq!(err.kind, ParseErrorKind::MissingField);
        assert!(err.message.contains("prompt"));
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
    fn test_parse_include() {
        let yaml = r#"
schema: "nika/workflow@0.12"
include:
  - path: ./partials/setup.nika.yaml
    prefix: setup_
  - path: "pkg:@nika/core@1.0/seo.nika.yaml"
tasks:
  - id: main_task
    infer: "Main logic"
"#;
        let workflow = parse(yaml, FileId(0)).unwrap();

        let include = workflow.include.as_ref().unwrap();
        assert_eq!(include.value.len(), 2);

        assert_eq!(
            include.value[0].value.path.value,
            "./partials/setup.nika.yaml"
        );
        assert_eq!(
            include.value[0].value.prefix.as_ref().unwrap().value,
            "setup_"
        );

        assert_eq!(
            include.value[1].value.path.value,
            "pkg:@nika/core@1.0/seo.nika.yaml"
        );
        assert!(include.value[1].value.prefix.is_none());
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
schema: "nika/workflow@0.12"
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
        assert_eq!(for_each.value.concurrency.as_ref().unwrap().value, 3);
    }

    #[test]
    fn test_parse_for_each_binding() {
        let yaml = r#"
schema: "nika/workflow@0.12"
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
    fn test_parse_for_each_object_form_with_template() {
        let yaml = r#"
schema: "nika/workflow@0.12"
tasks:
  - id: batch
    for_each:
      items: "{{with.data}}"
      as: item
      concurrency: 5
      fail_fast: false
    infer: "Process {{with.item}}"
"#;
        let workflow = parse(yaml, FileId(0)).unwrap();
        let task = workflow.get_task("batch").unwrap();

        let for_each = task.value.for_each.as_ref().unwrap();
        assert_eq!(for_each.value.items.value, "{{with.data}}");
        assert_eq!(for_each.value.as_var.as_ref().unwrap().value, "item");
        assert_eq!(for_each.value.concurrency.as_ref().unwrap().value, 5);
        assert!(!for_each.value.fail_fast.as_ref().unwrap().value);
    }

    #[test]
    fn test_parse_for_each_object_form_with_array() {
        let yaml = r#"
schema: "nika/workflow@0.12"
tasks:
  - id: batch
    for_each:
      items: ["a", "b", "c"]
      as: x
      concurrency: 3
    infer: "Process {{with.x}}"
"#;
        let workflow = parse(yaml, FileId(0)).unwrap();
        let task = workflow.get_task("batch").unwrap();

        let for_each = task.value.for_each.as_ref().unwrap();
        assert!(for_each.value.items.value.contains("["));
        assert_eq!(for_each.value.as_var.as_ref().unwrap().value, "x");
        assert_eq!(for_each.value.concurrency.as_ref().unwrap().value, 3);
    }

    #[test]
    fn test_parse_for_each_object_form_missing_items() {
        let yaml = r#"
schema: "nika/workflow@0.12"
tasks:
  - id: batch
    for_each:
      as: item
    infer: "No items"
"#;
        let result = parse(yaml, FileId(0));
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(
            err.message.contains("items"),
            "error should mention 'items': {}",
            err.message
        );
    }

    #[test]
    fn test_parse_retry_config() {
        let yaml = r#"
schema: "nika/workflow@0.12"
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
schema: "nika/workflow@0.12"
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
schema: "nika/workflow@0.12"
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
schema: "nika/workflow@0.12"
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
schema: "nika/workflow@0.12"
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
schema: "nika/workflow@0.12"
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
        assert!(
            result.is_err(),
            "task with multiple verbs should be rejected"
        );
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
schema: "nika/workflow@0.12"
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

    #[test]
    fn parse_rejects_yaml_nan_temperature() {
        // YAML .nan is rendered as ".nan" by marked_yaml, rejected by Rust's parse::<f64>()
        let yaml = r#"
schema: "nika/workflow@0.12"
tasks:
  - id: test
    infer:
      prompt: "hello"
      temperature: .nan
"#;
        let result = parse(yaml, FileId(0));
        assert!(result.is_err(), "YAML .nan temperature should be rejected");
    }

    #[test]
    fn parse_rejects_nan_string_temperature() {
        // "NaN" parses as f64::NaN successfully — caught by is_finite() check
        let yaml = r#"
schema: "nika/workflow@0.12"
tasks:
  - id: test
    infer:
      prompt: "hello"
      temperature: NaN
"#;
        let result = parse(yaml, FileId(0));
        assert!(result.is_err(), "NaN temperature should be rejected");
        let err = result.unwrap_err();
        assert!(
            err.message.contains("finite") || err.message.contains("number"),
            "Error should mention finite or number: {}",
            err.message
        );
    }

    #[test]
    fn parse_rejects_infinity_temperature() {
        let yaml = r#"
schema: "nika/workflow@0.12"
tasks:
  - id: test
    infer:
      prompt: "hello"
      temperature: .inf
"#;
        let result = parse(yaml, FileId(0));
        assert!(result.is_err(), "Infinity temperature should be rejected");
    }

    #[test]
    fn parse_rejects_inf_string_temperature() {
        // "inf" parses as f64::INFINITY — caught by is_finite() check
        let yaml = "schema: \"nika/workflow@0.12\"\ntasks:\n  - id: test\n    infer:\n      prompt: \"hello\"\n      temperature: inf\n";
        let result = parse(yaml, FileId(0));
        assert!(result.is_err(), "inf temperature should be rejected");
    }

    #[test]
    fn parse_rejects_negative_infinity_temperature() {
        let yaml = r#"
schema: "nika/workflow@0.12"
tasks:
  - id: test
    infer:
      prompt: "hello"
      temperature: -.inf
"#;
        let result = parse(yaml, FileId(0));
        assert!(result.is_err(), "Negative infinity should be rejected");
    }

    // --- Edge cases: empty/malformed input ---

    #[test]
    fn parse_empty_string_errors() {
        let result = parse("", FileId(0));
        assert!(result.is_err(), "empty string should fail to parse");
    }

    #[test]
    fn parse_yaml_array_instead_of_map() {
        let result = parse("- item1\n- item2", FileId(0));
        assert!(result.is_err(), "YAML array root should be rejected");
    }

    #[test]
    fn parse_temperature_zero_is_valid() {
        let yaml = r#"
schema: "nika/workflow@0.12"
tasks:
  - id: t
    infer:
      prompt: "hi"
      temperature: 0.0
"#;
        let result = parse(yaml, FileId(0));
        assert!(result.is_ok(), "temperature 0.0 should be valid");
    }

    #[test]
    fn parse_temperature_one_is_valid() {
        let yaml = r#"
schema: "nika/workflow@0.12"
tasks:
  - id: t
    infer:
      prompt: "hi"
      temperature: 1.0
"#;
        let result = parse(yaml, FileId(0));
        assert!(result.is_ok(), "temperature 1.0 should be valid");
    }

    #[test]
    fn parse_whitespace_only_errors() {
        let result = parse("   \n\n  \t  ", FileId(0));
        assert!(result.is_err(), "whitespace-only input should fail");
    }

    // =========================================================================
    // Vision / Content Parsing Tests
    // =========================================================================

    #[test]
    fn parse_infer_with_content_text_and_image() {
        let yaml = r#"
schema: "nika/workflow@0.12"
workflow: vision-test
provider: claude
model: claude-sonnet-4-6
tasks:
  - id: describe
    infer:
      content:
        - type: text
          text: "Describe this image"
        - type: image
          source: "blake3:abc123"
          detail: high
"#;
        let result = parse(yaml, FileId(0));
        assert!(
            result.is_ok(),
            "vision content should parse: {:?}",
            result.err()
        );
        let wf = result.unwrap();
        let task = &wf.tasks.value[0];
        match &task.value.action {
            Some(RawTaskAction::Infer(s)) => {
                let content = s.value.content.as_ref().expect("content should be Some");
                assert_eq!(content.value.len(), 2);
            }
            other => panic!("expected Some(Infer), got {:?}", other),
        }
    }

    #[test]
    fn parse_infer_content_only_no_prompt() {
        let yaml = r#"
schema: "nika/workflow@0.12"
workflow: vision-no-prompt
provider: claude
model: claude-sonnet-4-6
tasks:
  - id: t
    infer:
      content:
        - type: text
          text: "What is this?"
"#;
        let result = parse(yaml, FileId(0));
        assert!(
            result.is_ok(),
            "content without prompt should parse: {:?}",
            result.err()
        );
        let wf = result.unwrap();
        let task = &wf.tasks.value[0];
        match &task.value.action {
            Some(RawTaskAction::Infer(s)) => {
                assert!(
                    s.value.prompt.value.is_empty(),
                    "prompt should be empty string"
                );
                assert!(s.value.content.is_some(), "content should be present");
            }
            other => panic!("expected Some(Infer), got {:?}", other),
        }
    }

    #[test]
    fn parse_infer_prompt_and_content() {
        let yaml = r#"
schema: "nika/workflow@0.12"
workflow: both
provider: claude
model: claude-sonnet-4-6
tasks:
  - id: t
    infer:
      prompt: "Analyze carefully"
      content:
        - type: image
          source: "blake3:xyz"
"#;
        let result = parse(yaml, FileId(0));
        assert!(
            result.is_ok(),
            "prompt+content should parse: {:?}",
            result.err()
        );
        let wf = result.unwrap();
        let task = &wf.tasks.value[0];
        match &task.value.action {
            Some(RawTaskAction::Infer(s)) => {
                assert_eq!(s.value.prompt.value, "Analyze carefully");
                assert!(s.value.content.is_some());
            }
            other => panic!("expected Some(Infer), got {:?}", other),
        }
    }

    #[test]
    fn parse_infer_shorthand_still_works() {
        let yaml = r#"
schema: "nika/workflow@0.12"
workflow: shorthand
provider: claude
model: claude-sonnet-4-6
tasks:
  - id: t
    infer: "Just a simple prompt"
"#;
        let result = parse(yaml, FileId(0));
        assert!(result.is_ok());
        let wf = result.unwrap();
        match &wf.tasks.value[0].value.action {
            Some(RawTaskAction::Infer(s)) => {
                assert_eq!(s.value.prompt.value, "Just a simple prompt");
                assert!(s.value.content.is_none());
            }
            other => panic!("expected Some(Infer), got {:?}", other),
        }
    }

    #[test]
    fn parse_infer_neither_prompt_nor_content_errors() {
        let yaml = r#"
schema: "nika/workflow@0.12"
workflow: err
provider: claude
model: claude-sonnet-4-6
tasks:
  - id: t
    infer:
      temperature: 0.5
"#;
        let result = parse(yaml, FileId(0));
        assert!(result.is_err(), "neither prompt nor content should fail");
        let err = result.unwrap_err();
        assert!(err.message.contains("prompt") || err.message.contains("content"));
    }

    #[test]
    fn parse_infer_content_invalid_type_errors() {
        let yaml = r#"
schema: "nika/workflow@0.12"
workflow: err
provider: claude
model: claude-sonnet-4-6
tasks:
  - id: t
    infer:
      content:
        - type: video
          url: "https://example.com"
"#;
        let result = parse(yaml, FileId(0));
        assert!(result.is_err(), "unknown content type should fail");
        let err = result.unwrap_err();
        assert!(err.message.contains("unknown content part type"));
    }

    #[test]
    fn parse_infer_content_empty_sequence_errors() {
        let yaml = r#"
schema: "nika/workflow@0.12"
workflow: err
provider: claude
model: claude-sonnet-4-6
tasks:
  - id: t
    infer:
      content: []
"#;
        let result = parse(yaml, FileId(0));
        assert!(result.is_err(), "empty content should fail");
    }

    #[test]
    fn parse_infer_content_image_url_part() {
        let yaml = r#"
schema: "nika/workflow@0.12"
workflow: url
provider: openai
model: gpt-4o
tasks:
  - id: t
    infer:
      content:
        - type: image_url
          url: "https://example.com/photo.jpg"
          detail: low
        - type: text
          text: "What is in this photo?"
"#;
        let result = parse(yaml, FileId(0));
        assert!(result.is_ok(), "image_url should parse: {:?}", result.err());
    }

    #[test]
    fn test_parse_infer_with_guardrails() {
        let yaml = r#"
schema: "nika/workflow@0.12"
tasks:
  - id: summarize
    infer:
      prompt: "Summarize this article"
      guardrails:
        - type: length
          min_words: 50
          max_words: 200
        - type: regex
          pattern: "^Summary:"
          message: "Output must start with 'Summary:'"
"#;
        let workflow = parse(yaml, FileId(0)).unwrap();
        let task = workflow.get_task("summarize").unwrap();

        match &task.value.action {
            Some(RawTaskAction::Infer(action)) => {
                assert_eq!(action.value.prompt.value, "Summarize this article");
                assert_eq!(action.value.guardrails.len(), 2);
                assert_eq!(action.value.guardrails[0].guardrail_type(), "length");
                assert_eq!(action.value.guardrails[1].guardrail_type(), "regex");
            }
            _ => panic!("Expected Infer action"),
        }
    }

    #[test]
    fn test_parse_infer_shorthand_no_guardrails() {
        let yaml = r#"
schema: "nika/workflow@0.12"
tasks:
  - id: quick
    infer: "Generate a headline"
"#;
        let workflow = parse(yaml, FileId(0)).unwrap();
        let task = workflow.get_task("quick").unwrap();

        match &task.value.action {
            Some(RawTaskAction::Infer(action)) => {
                assert!(
                    action.value.guardrails.is_empty(),
                    "Shorthand infer should have no guardrails"
                );
            }
            _ => panic!("Expected Infer action"),
        }
    }

    #[test]
    fn test_parse_infer_guardrails_on_failure_fail() {
        let yaml = r#"
schema: "nika/workflow@0.12"
tasks:
  - id: strict
    infer:
      prompt: "Generate strict output"
      guardrails:
        - type: length
          min_words: 10
          on_failure: fail
"#;
        let workflow = parse(yaml, FileId(0)).unwrap();
        let task = workflow.get_task("strict").unwrap();

        match &task.value.action {
            Some(RawTaskAction::Infer(action)) => {
                assert_eq!(action.value.guardrails.len(), 1);
                assert_eq!(
                    action.value.guardrails[0].on_failure(),
                    crate::ast::guardrails::OnFailure::Fail
                );
            }
            _ => panic!("Expected Infer action"),
        }
    }

    #[test]
    fn test_unknown_workflow_key_detected() {
        let yaml = r#"
schema: "nika/workflow@0.12"
tsks:
  - id: a
    exec: "echo hi"
"#;
        let result = parse(yaml, FileId(0));
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert_eq!(err.kind, ParseErrorKind::UnknownField);
        assert!(err.message.contains("tsks"));
        assert!(err.message.contains("did you mean 'tasks'"));
    }

    #[test]
    fn test_unknown_workflow_key_no_suggestion() {
        let yaml = r#"
schema: "nika/workflow@0.12"
foobar: xyz
tasks:
  - id: a
    exec: "echo hi"
"#;
        let result = parse(yaml, FileId(0));
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert_eq!(err.kind, ParseErrorKind::UnknownField);
        assert!(err.message.contains("foobar"));
        assert!(err.message.contains("Known fields"));
    }

    #[test]
    fn test_valid_workflow_all_known_keys() {
        let yaml = r#"
schema: "nika/workflow@0.12"
workflow: test
description: "Test"
provider: mock
model: test
tasks:
  - id: a
    exec: "echo hi"
"#;
        let result = parse(yaml, FileId(0));
        assert!(result.is_ok());
    }

    #[test]
    fn test_unknown_task_key_with_verb_present() {
        let yaml = r#"
schema: "nika/workflow@0.12"
tasks:
  - id: a
    exec: "echo hi"
    dependson:
      - b
"#;
        let result = parse(yaml, FileId(0));
        assert!(
            result.is_err(),
            "unknown task key should be rejected even when verb is present"
        );
        let err = result.unwrap_err();
        assert_eq!(err.kind, ParseErrorKind::UnknownField);
        assert!(
            err.message.contains("dependson"),
            "error should mention the unknown key, got: {}",
            err.message
        );
        assert!(
            err.message.contains("did you mean 'depends_on'"),
            "error should suggest depends_on, got: {}",
            err.message
        );
    }

    #[test]
    fn test_known_task_keys_with_verb_no_error() {
        let yaml = r#"
schema: "nika/workflow@0.12"
agents:
  helper:
    system: "You are helpful"
    provider: mock
    model: mock-fast
tasks:
  - id: a
    exec: "echo hi"
    depends_on: [b]
    with:
      data: $b
    retry:
      max_attempts: 3
      delay_ms: 1000
    preset: helper
  - id: b
    infer: "hello"
"#;
        let result = parse(yaml, FileId(0));
        assert!(
            result.is_ok(),
            "all known task keys should parse fine: {:?}",
            result.err()
        );
    }

    #[test]
    fn test_parse_task_with_preset() {
        let yaml = r#"
schema: "nika/workflow@0.12"
agents:
  assistant:
    system: "You are helpful"
    provider: mock
    model: mock-fast
tasks:
  - id: a
    preset: assistant
    infer: "hello"
"#;
        let result = parse(yaml, FileId(0));
        assert!(
            result.is_ok(),
            "preset: field should parse correctly: {:?}",
            result.err()
        );
        let wf = result.unwrap();
        let task = &wf.tasks.value[0].value;
        assert_eq!(
            task.preset.as_ref().map(|s| s.value.as_str()),
            Some("assistant")
        );
    }

    #[test]
    fn test_parse_task_without_preset_is_none() {
        let yaml = r#"
schema: "nika/workflow@0.12"
tasks:
  - id: a
    infer: "hello"
"#;
        let result = parse(yaml, FileId(0));
        assert!(result.is_ok(), "parse failed: {:?}", result.err());
        let wf = result.unwrap();
        let task = &wf.tasks.value[0].value;
        assert!(
            task.preset.is_none(),
            "task without preset: should have None"
        );
    }

    // ─────────────────────────────────────────────────────────────
    // Provider array syntax (fallback chains)
    // ─────────────────────────────────────────────────────────────

    #[test]
    fn provider_string_parsed_as_single() {
        let yaml = r#"
schema: "nika/workflow@0.12"
tasks:
  - id: gen
    provider: anthropic
    infer: "hello"
"#;
        let result = parse(yaml, FileId(0));
        assert!(result.is_ok(), "parse failed: {:?}", result.err());
        let wf = result.unwrap();
        let task = &wf.tasks.value[0].value;
        assert_eq!(task.provider.as_ref().unwrap().value, "anthropic");
        // No routing generated for single provider
        assert!(task.routing.is_none());
    }

    #[test]
    fn provider_array_parsed_as_fallback_chain() {
        let yaml = r#"
schema: "nika/workflow@0.12"
tasks:
  - id: gen
    provider: [groq, anthropic]
    infer: "hello"
"#;
        let result = parse(yaml, FileId(0));
        assert!(result.is_ok(), "parse failed: {:?}", result.err());
        let wf = result.unwrap();
        let task = &wf.tasks.value[0].value;
        // Primary provider is first in chain
        assert_eq!(task.provider.as_ref().unwrap().value, "groq");
        // Routing fallback is auto-populated from array
        assert!(task.routing.is_some());
        let routing_json = task.routing.as_ref().unwrap();
        let fallback = routing_json.value["fallback"].as_array().unwrap();
        assert_eq!(fallback.len(), 2);
        assert_eq!(fallback[0].as_str().unwrap(), "groq");
        assert_eq!(fallback[1].as_str().unwrap(), "anthropic");
    }

    #[test]
    fn provider_single_element_array_no_routing() {
        let yaml = r#"
schema: "nika/workflow@0.12"
tasks:
  - id: gen
    provider: [anthropic]
    infer: "hello"
"#;
        let result = parse(yaml, FileId(0));
        assert!(result.is_ok(), "parse failed: {:?}", result.err());
        let wf = result.unwrap();
        let task = &wf.tasks.value[0].value;
        assert_eq!(task.provider.as_ref().unwrap().value, "anthropic");
        // Single-element array: routing generated but only 1 entry
        assert!(task.routing.is_some());
    }

    #[test]
    fn provider_empty_array_rejected() {
        let yaml = r#"
schema: "nika/workflow@0.12"
tasks:
  - id: gen
    provider: []
    infer: "hello"
"#;
        let result = parse(yaml, FileId(0));
        assert!(result.is_err(), "empty provider array should be rejected");
    }

    #[test]
    fn explicit_routing_overrides_provider_array() {
        let yaml = r#"
schema: "nika/workflow@0.12"
tasks:
  - id: gen
    provider: [groq, anthropic]
    routing:
      fallback: [openai, mistral]
    infer: "hello"
"#;
        let result = parse(yaml, FileId(0));
        assert!(result.is_ok(), "parse failed: {:?}", result.err());
        let wf = result.unwrap();
        let task = &wf.tasks.value[0].value;
        // Provider is first from array
        assert_eq!(task.provider.as_ref().unwrap().value, "groq");
        // Explicit routing: block wins over auto-generated
        let routing_json = task.routing.as_ref().unwrap();
        let fallback = routing_json.value["fallback"].as_array().unwrap();
        assert_eq!(fallback[0].as_str().unwrap(), "openai");
    }

    // ═══════════════════════════════════════════════════════════════
    // AGENT PRESET DISAMBIGUATION TESTS
    // ═══════════════════════════════════════════════════════════════

    #[test]
    fn test_agent_scalar_is_preset_not_verb() {
        let yaml = r#"
schema: "nika/workflow@0.12"
tasks:
  - id: plan
    agent: think
    infer: "Plan the architecture"
"#;
        let wf = parse(yaml, FileId(0)).unwrap();
        let task = &wf.tasks.value[0].value;

        // agent: think (scalar) → preset, NOT verb
        assert_eq!(
            task.preset.as_ref().unwrap().value,
            "think",
            "agent: <string> should be stored as preset"
        );
        // infer: should be the actual verb
        assert!(
            matches!(&task.action, Some(RawTaskAction::Infer(_))),
            "infer: should be the verb when agent: is scalar"
        );
    }

    #[test]
    fn test_agent_mapping_is_verb() {
        // Regression: agent: { prompt: "..." } should still be parsed as agent verb
        let yaml = r#"
schema: "nika/workflow@0.12"
tasks:
  - id: research
    agent:
      prompt: "Research AI trends"
      max_turns: 5
"#;
        let wf = parse(yaml, FileId(0)).unwrap();
        let task = &wf.tasks.value[0].value;

        assert!(
            matches!(&task.action, Some(RawTaskAction::Agent(_))),
            "agent: {{ ... }} (mapping) should be parsed as agent verb"
        );
        assert!(
            task.preset.is_none(),
            "preset should NOT be set when agent: is a mapping verb"
        );
    }

    #[test]
    fn test_agent_scalar_overrides_explicit_preset() {
        let yaml = r#"
schema: "nika/workflow@0.12"
tasks:
  - id: plan
    preset: old_preset
    agent: think
    infer: "Plan it"
"#;
        let wf = parse(yaml, FileId(0)).unwrap();
        let task = &wf.tasks.value[0].value;

        // agent: think overrides preset: old_preset
        assert_eq!(
            task.preset.as_ref().unwrap().value,
            "think",
            "agent: <string> should override preset:"
        );
    }

    #[test]
    fn test_agent_scalar_standalone_no_verb() {
        // agent: think with no other verb — task has no action
        let yaml = r#"
schema: "nika/workflow@0.12"
tasks:
  - id: noop
    agent: think
"#;
        let wf = parse(yaml, FileId(0)).unwrap();
        let task = &wf.tasks.value[0].value;

        assert!(task.action.is_none(), "No verb should be parsed");
        assert_eq!(task.preset.as_ref().unwrap().value, "think");
    }

    #[test]
    fn test_parse_context_budget() {
        let yaml = r#"
schema: "nika/workflow@0.12"
tasks:
  - id: constrained
    context_budget: 4000
    infer: "Summarize data"
"#;
        let wf = parse(yaml, FileId(0)).unwrap();
        let task = wf.get_task("constrained").unwrap();
        assert_eq!(task.value.context_budget.as_ref().unwrap().value, 4000);
    }

    #[test]
    fn test_parse_context_budget_missing() {
        let yaml = r#"
schema: "nika/workflow@0.12"
tasks:
  - id: no_budget
    infer: "No budget"
"#;
        let wf = parse(yaml, FileId(0)).unwrap();
        let task = wf.get_task("no_budget").unwrap();
        assert!(task.value.context_budget.is_none());
    }

    #[test]
    fn test_parse_context_budget_unknown_key_still_works() {
        let yaml = r#"
schema: "nika/workflow@0.12"
tasks:
  - id: bad
    context_budgetx: 4000
    infer: "test"
"#;
        let result = parse(yaml, FileId(0));
        assert!(result.is_err(), "Misspelled key should fail");
        let err = result.unwrap_err();
        assert!(
            err.message.contains("unknown task field"),
            "Error should mention unknown field: {}",
            err.message
        );
    }

    #[test]
    fn test_reject_use_keyword_with_helpful_hint() {
        let yaml = r#"
schema: "nika/workflow@0.12"
tasks:
  - id: bad
    use: { data: $step1 }
    infer: "test"
"#;
        let err = parse(yaml, FileId(0)).unwrap_err();
        assert!(
            err.message.contains("did you mean 'with'"),
            "Should suggest 'with:' for 'use:': {}",
            err.message
        );
    }

    #[test]
    fn test_reject_max_retries_at_task_level() {
        let yaml = r#"
schema: "nika/workflow@0.12"
tasks:
  - id: bad
    max_retries: 3
    infer: "test"
"#;
        let err = parse(yaml, FileId(0)).unwrap_err();
        assert!(
            err.message.contains("retry:"),
            "Should suggest 'retry:' for 'max_retries:': {}",
            err.message
        );
    }

    // =========================================================================
    // D.1: goal: field for P-ORCHESTRATE
    // =========================================================================

    #[test]
    fn test_parse_goal_field() {
        let yaml = r#"
schema: "nika/workflow@0.12"
goal: "Research and write a comprehensive report on quantum computing"
tasks:
  - id: research
    infer: "Research quantum computing"
"#;
        let result = parse(yaml, FileId(0));
        assert!(result.is_ok(), "Parse failed: {:?}", result.err());
        let workflow = result.unwrap();
        assert_eq!(
            workflow.goal.as_ref().unwrap().value,
            "Research and write a comprehensive report on quantum computing"
        );
    }

    #[test]
    fn test_parse_workflow_without_goal() {
        let yaml = r#"
schema: "nika/workflow@0.12"
tasks:
  - id: step1
    exec: "echo hello"
"#;
        let result = parse(yaml, FileId(0));
        assert!(result.is_ok(), "Parse failed: {:?}", result.err());
        let workflow = result.unwrap();
        assert!(workflow.goal.is_none());
    }

    #[test]
    fn test_parse_orchestrate_config() {
        let yaml = r#"
schema: "nika/workflow@0.12"
goal: "Research AI"
orchestrate:
  max_rounds: 20
  confidence_target: 0.95
  agent: researcher
  max_cost_usd: 5.0
tasks:
  - id: step1
    infer: "test"
"#;
        let result = parse(yaml, FileId(0));
        assert!(result.is_ok(), "Parse failed: {:?}", result.err());
        let workflow = result.unwrap();
        assert!(workflow.orchestrate.is_some());
    }

    #[test]
    fn test_parse_orchestrate_empty_config() {
        let yaml = r#"
schema: "nika/workflow@0.12"
goal: "Build something"
orchestrate: {}
tasks:
  - id: step1
    exec: "echo hello"
"#;
        let result = parse(yaml, FileId(0));
        assert!(result.is_ok(), "Parse failed: {:?}", result.err());
        let workflow = result.unwrap();
        assert!(workflow.orchestrate.is_some());
    }

    #[test]
    fn test_parse_goal_with_tasks() {
        let yaml = r#"
schema: "nika/workflow@0.12"
goal: "Build a podcast episode"
description: "Full podcast production pipeline"
provider: anthropic
tasks:
  - id: outline
    infer: "Create outline"
  - id: script
    depends_on: [outline]
    infer: "Write script"
"#;
        let result = parse(yaml, FileId(0));
        assert!(result.is_ok(), "Parse failed: {:?}", result.err());
        let workflow = result.unwrap();
        assert_eq!(
            workflow.goal.as_ref().unwrap().value,
            "Build a podcast episode"
        );
        assert_eq!(workflow.task_count(), 2);
    }

    #[test]
    fn parse_yaml_anchor_gives_actionable_error() {
        let yaml = r#"
schema: "nika/workflow@0.12"
tasks:
  - id: &shared_task base
    infer: "hello"
"#;
        let result = parse(yaml, FileId(0));
        assert!(result.is_err(), "YAML anchors should be rejected");
        let err = result.unwrap_err();
        assert_eq!(err.kind, ParseErrorKind::Syntax);
        assert!(
            err.message.contains("anchors"),
            "Error should mention anchors: {}",
            err.message,
        );
        assert!(
            err.message.contains("include:"),
            "Error should suggest include: as alternative: {}",
            err.message,
        );
    }
}
