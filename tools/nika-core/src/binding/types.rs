// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Core Binding Types
//!
//! Foundational types for the binding system redesign:
//! - `BindingPath`: Parsed path like `$step1.data.items[0].name`
//! - `BindingSource`: Where data comes from (Task, Context, Input, Env, LoopVar)
//! - `PathSegment`: Individual path segment (field name or array index)
//! - `BindingType`: Type constraint on binding values (7 variants)
//!
//! # Path Syntax
//!
//! All binding paths start with `$`:
//!
//! | Pattern | Source | Example |
//! |---------|--------|---------|
//! | `$task_id` | Task output | `$step1`, `$step1.data.name` |
//! | `$context.*` | Context file/session | `$context.files.brand` |
//! | `$inputs.*` | Workflow inputs | `$inputs.locale` |
//! | `$env.*` | Environment variable | `$env.API_URL` |
//! | `$item` | Loop variable (for_each) | `$item`, `$item.name` |
//!
//! Array indexing uses brackets: `$data.items[0].name`
//!
//! # Design Notes
//!
//! - `Arc<str>` for identifiers: cheap clones, no lifetime issues
//! - `BindingPath` is `Hash + Eq` for use as map keys
//! - `BindingType` has serde support for YAML deserialization
//! - Parse errors use NIKA-150 error code

use std::fmt;
use std::sync::Arc;

use serde::Deserialize;

/// Source path for a binding -- parsed from "$task_id.field.path"
///
/// Combines a source (where data comes from) with optional property
/// access segments for nested data traversal.
///
/// # Examples
///
/// ```text
/// $step1           → Task("step1"), segments: []
/// $step1.name      → Task("step1"), segments: [Field("name")]
/// $data[0].name    → Task("data"), segments: [Index(0), Field("name")]
/// $context.files.x → Context("files.x"), segments: []
/// $inputs.locale   → Input("locale"), segments: []
/// $env.API_URL     → Env("API_URL"), segments: []
/// ```
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct BindingPath {
    /// Where the data comes from
    pub source: BindingSource,
    /// Property access segments after the source
    pub segments: Vec<PathSegment>,
}

/// Where data originates
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum BindingSource {
    /// Task output: $task_id
    Task(Arc<str>),
    /// Context file/session: $context.files.brand or $context.session
    Context(Arc<str>),
    /// Workflow input: $inputs.locale
    Input(Arc<str>),
    /// Environment variable: $env.API_URL
    Env(Arc<str>),
    /// Vault credential: $vault.SERVICE.FIELD
    Vault {
        /// Service name (e.g., "stripe", "anthropic")
        service: Arc<str>,
        /// Field name (e.g., "api_key", "secret")
        field: Arc<str>,
    },
    /// Loop variable: $item (from for_each as:)
    LoopVar(Arc<str>),
}

/// Single segment in a property path
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum PathSegment {
    /// Named field: .name
    Field(Arc<str>),
    /// Array index: \[0\]
    Index(usize),
}

/// Type constraint on a binding value (optional, defaults to Any)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum BindingType {
    /// No type constraint (default)
    #[default]
    Any,
    /// JSON string
    String,
    /// JSON number (integer or float)
    Number,
    /// JSON integer only
    Integer,
    /// JSON boolean
    Boolean,
    /// JSON array
    Array,
    /// JSON object
    Object,
}

/// Error type for binding path parsing (NIKA-150)
#[derive(Debug, Clone, PartialEq)]
pub struct BindingPathError {
    pub input: String,
    pub reason: String,
}

impl fmt::Display for BindingPathError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "[NIKA-150] Invalid binding path '{}': {}",
            self.input, self.reason
        )
    }
}

impl std::error::Error for BindingPathError {}

// ═══════════════════════════════════════════════════════════════
// Reserved namespace prefixes
// ═══════════════════════════════════════════════════════════════

const RESERVED_CONTEXT: &str = "context";
const RESERVED_INPUTS: &str = "inputs";
const RESERVED_ENV: &str = "env";
const RESERVED_VAULT: &str = "vault";

impl BindingPath {
    /// Parse a binding path string like `$task_id.field[0].name`
    ///
    /// Reserved namespaces: context, inputs, env
    /// Everything else is a task reference or loop variable.
    ///
    /// Loop variables (from `for_each as: item`) are resolved by the caller
    /// providing a `loop_vars` hint set. Without that hint, ambiguous single-segment
    /// paths like `$item` are treated as Task references by default. Callers can
    /// use `parse_with_loop_vars` for disambiguation.
    pub fn parse(input: &str) -> Result<Self, BindingPathError> {
        Self::parse_inner(input, None)
    }

    /// Parse with loop variable hints for disambiguation.
    ///
    /// Any top-level identifier matching `loop_vars` becomes `BindingSource::LoopVar`.
    pub fn parse_with_loop_vars(input: &str, loop_vars: &[&str]) -> Result<Self, BindingPathError> {
        Self::parse_inner(input, Some(loop_vars))
    }

    fn parse_inner(input: &str, loop_vars: Option<&[&str]>) -> Result<Self, BindingPathError> {
        let trimmed = input.trim();

        // Must start with $
        let rest = trimmed.strip_prefix('$').ok_or_else(|| {
            // E10: detect {{...}} template syntax and give a helpful suggestion
            let reason = if trimmed.starts_with("{{") || trimmed.contains("{{") {
                "Template syntax {{...}} is not valid in with: bindings. \
                 Use $task_id | transform syntax instead. \
                 Example: data: $my_task | upper"
                    .to_string()
            } else {
                "with: binding paths must start with '$'. \
                 Example: data: $task_id | transform"
                    .to_string()
            };
            BindingPathError {
                input: trimmed.to_string(),
                reason,
            }
        })?;

        if rest.is_empty() {
            return Err(BindingPathError {
                input: trimmed.to_string(),
                reason: "empty path after '$'".to_string(),
            });
        }

        // Tokenize: split on '.' and '[', preserving bracket content
        let tokens = tokenize_path(rest).map_err(|reason| BindingPathError {
            input: trimmed.to_string(),
            reason,
        })?;

        if tokens.is_empty() {
            return Err(BindingPathError {
                input: trimmed.to_string(),
                reason: "empty path after '$'".to_string(),
            });
        }

        // First token is the root identifier
        let root = match &tokens[0] {
            PathToken::Field(name) => name.as_str(),
            PathToken::Index(_) => {
                return Err(BindingPathError {
                    input: trimmed.to_string(),
                    reason: "path cannot start with an array index".to_string(),
                });
            }
        };

        // Check reserved namespaces
        match root {
            RESERVED_CONTEXT => {
                // $context.files.brand → Context("files.brand")
                // Everything after "context." becomes the context sub-path
                if tokens.len() < 2 {
                    return Err(BindingPathError {
                        input: trimmed.to_string(),
                        reason: "'$context' requires a sub-path (e.g., '$context.files.brand')"
                            .to_string(),
                    });
                }
                let sub_path = tokens_to_dotted_string(&tokens[1..]);
                Ok(BindingPath {
                    source: BindingSource::Context(Arc::from(sub_path.as_str())),
                    segments: vec![],
                })
            }
            RESERVED_INPUTS => {
                // $inputs.locale → Input("locale")
                if tokens.len() < 2 {
                    return Err(BindingPathError {
                        input: trimmed.to_string(),
                        reason: "'$inputs' requires a sub-path (e.g., '$inputs.locale')"
                            .to_string(),
                    });
                }
                let sub_path = tokens_to_dotted_string(&tokens[1..]);
                Ok(BindingPath {
                    source: BindingSource::Input(Arc::from(sub_path.as_str())),
                    segments: vec![],
                })
            }
            RESERVED_ENV => {
                // $env.API_URL → Env("API_URL")
                if tokens.len() < 2 {
                    return Err(BindingPathError {
                        input: trimmed.to_string(),
                        reason: "'$env' requires a variable name (e.g., '$env.API_URL')"
                            .to_string(),
                    });
                }
                let var_name = tokens_to_dotted_string(&tokens[1..]);
                Ok(BindingPath {
                    source: BindingSource::Env(Arc::from(var_name.as_str())),
                    segments: vec![],
                })
            }
            RESERVED_VAULT => {
                // $vault.stripe.secret → Vault { service: "stripe", field: "secret" }
                if tokens.len() < 3 {
                    return Err(BindingPathError {
                        input: trimmed.to_string(),
                        reason:
                            "'$vault' requires service and field (e.g., '$vault.stripe.secret')"
                                .to_string(),
                    });
                }
                let service = match &tokens[1] {
                    PathToken::Field(name) => name.clone(),
                    PathToken::Index(_) => {
                        return Err(BindingPathError {
                            input: trimmed.to_string(),
                            reason: "vault service name cannot be an array index".to_string(),
                        });
                    }
                };
                let field = match &tokens[2] {
                    PathToken::Field(name) => name.clone(),
                    PathToken::Index(_) => {
                        return Err(BindingPathError {
                            input: trimmed.to_string(),
                            reason: "vault field name cannot be an array index".to_string(),
                        });
                    }
                };
                if tokens.len() > 3 {
                    return Err(BindingPathError {
                        input: trimmed.to_string(),
                        reason: "'$vault' paths have exactly two segments: $vault.SERVICE.FIELD"
                            .to_string(),
                    });
                }
                Ok(BindingPath {
                    source: BindingSource::Vault {
                        service: Arc::from(service.as_str()),
                        field: Arc::from(field.as_str()),
                    },
                    segments: vec![],
                })
            }
            _ => {
                // Check if this is a loop variable
                let is_loop_var = loop_vars.map(|vars| vars.contains(&root)).unwrap_or(false);

                let source = if is_loop_var {
                    BindingSource::LoopVar(Arc::from(root))
                } else {
                    BindingSource::Task(Arc::from(root))
                };

                // Remaining tokens become path segments
                let segments = tokens[1..]
                    .iter()
                    .map(|t| match t {
                        PathToken::Field(name) => PathSegment::Field(Arc::from(name.as_str())),
                        PathToken::Index(idx) => PathSegment::Index(*idx),
                    })
                    .collect();

                Ok(BindingPath { source, segments })
            }
        }
    }

    /// Extract the task ID if this is a Task source
    pub fn task_id(&self) -> Option<&Arc<str>> {
        match &self.source {
            BindingSource::Task(id) => Some(id),
            _ => None,
        }
    }

    /// Returns true if this binding references a task output
    pub fn is_task_ref(&self) -> bool {
        matches!(self.source, BindingSource::Task(_))
    }
}

// ═══════════════════════════════════════════════════════════════
// Internal tokenizer
// ═══════════════════════════════════════════════════════════════

#[derive(Debug, Clone)]
enum PathToken {
    Field(String),
    Index(usize),
}

/// Tokenize a path string (after stripping $) into fields and indices.
///
/// "step1.data.items[0].name" → [Field("step1"), Field("data"), Field("items"), Index(0), Field("name")]
fn tokenize_path(input: &str) -> Result<Vec<PathToken>, String> {
    let mut tokens = Vec::new();
    let mut current = String::new();
    let mut chars = input.chars().peekable();

    while let Some(&ch) = chars.peek() {
        match ch {
            '.' => {
                chars.next();
                if !current.is_empty() {
                    tokens.push(PathToken::Field(std::mem::take(&mut current)));
                }
                // Leading dot or double dot check
                if tokens.is_empty() && current.is_empty() {
                    return Err("path cannot start with '.'".to_string());
                }
            }
            '[' => {
                chars.next();
                if !current.is_empty() {
                    tokens.push(PathToken::Field(std::mem::take(&mut current)));
                }
                // Parse index content until ']'
                let mut idx_str = String::new();
                loop {
                    match chars.next() {
                        Some(']') => break,
                        Some(c) => idx_str.push(c),
                        None => return Err("unclosed bracket in path".to_string()),
                    }
                }
                let idx: usize = idx_str
                    .parse()
                    .map_err(|_| format!("invalid array index: '{}'", idx_str))?;
                tokens.push(PathToken::Index(idx));
            }
            ']' => {
                return Err("unexpected ']' without matching '['".to_string());
            }
            _ => {
                chars.next();
                current.push(ch);
            }
        }
    }

    if !current.is_empty() {
        tokens.push(PathToken::Field(current));
    }

    Ok(tokens)
}

/// Convert tokens back to a dotted string (for Context/Input/Env sub-paths)
fn tokens_to_dotted_string(tokens: &[PathToken]) -> String {
    tokens
        .iter()
        .map(|t| match t {
            PathToken::Field(name) => name.clone(),
            PathToken::Index(idx) => idx.to_string(),
        })
        .collect::<Vec<_>>()
        .join(".")
}

// ═══════════════════════════════════════════════════════════════
// Display implementations
// ═══════════════════════════════════════════════════════════════

impl fmt::Display for BindingPath {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "${}", self.source)?;
        for seg in &self.segments {
            write!(f, "{}", seg)?;
        }
        Ok(())
    }
}

impl fmt::Display for BindingSource {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            BindingSource::Task(id) => write!(f, "{}", id),
            BindingSource::Context(path) => write!(f, "context.{}", path),
            BindingSource::Input(path) => write!(f, "inputs.{}", path),
            BindingSource::Env(var) => write!(f, "env.{}", var),
            BindingSource::Vault { service, field } => {
                write!(f, "vault.{}.{}", service, field)
            }
            BindingSource::LoopVar(name) => write!(f, "{}", name),
        }
    }
}

impl fmt::Display for PathSegment {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            PathSegment::Field(name) => write!(f, ".{}", name),
            PathSegment::Index(idx) => write!(f, "[{}]", idx),
        }
    }
}

impl fmt::Display for BindingType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            BindingType::Any => write!(f, "any"),
            BindingType::String => write!(f, "string"),
            BindingType::Number => write!(f, "number"),
            BindingType::Integer => write!(f, "integer"),
            BindingType::Boolean => write!(f, "boolean"),
            BindingType::Array => write!(f, "array"),
            BindingType::Object => write!(f, "object"),
        }
    }
}

impl BindingSource {
    /// Returns true if this is a task reference
    pub fn is_task(&self) -> bool {
        matches!(self, BindingSource::Task(_))
    }

    /// Returns true if this is a vault credential reference
    pub fn is_vault(&self) -> bool {
        matches!(self, BindingSource::Vault { .. })
    }
}

// ═══════════════════════════════════════════════════════════════
// Tests
// ═══════════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;

    // ─────────────────────────────────────────────────────────────
    // BindingPath::parse — Task references
    // ─────────────────────────────────────────────────────────────

    #[test]
    fn parse_simple_task_ref() {
        let bp = BindingPath::parse("$step1").unwrap();
        assert_eq!(bp.source, BindingSource::Task(Arc::from("step1")));
        assert!(bp.segments.is_empty());
    }

    #[test]
    fn parse_task_with_field() {
        let bp = BindingPath::parse("$step1.output").unwrap();
        assert_eq!(bp.source, BindingSource::Task(Arc::from("step1")));
        assert_eq!(bp.segments, vec![PathSegment::Field(Arc::from("output"))]);
    }

    #[test]
    fn parse_task_deep_path() {
        let bp = BindingPath::parse("$step1.data.items[0].name").unwrap();
        assert_eq!(bp.source, BindingSource::Task(Arc::from("step1")));
        assert_eq!(
            bp.segments,
            vec![
                PathSegment::Field(Arc::from("data")),
                PathSegment::Field(Arc::from("items")),
                PathSegment::Index(0),
                PathSegment::Field(Arc::from("name")),
            ]
        );
    }

    // ─────────────────────────────────────────────────────────────
    // BindingPath::parse — Reserved namespaces
    // ─────────────────────────────────────────────────────────────

    #[test]
    fn parse_context_file() {
        let bp = BindingPath::parse("$context.files.brand").unwrap();
        assert_eq!(bp.source, BindingSource::Context(Arc::from("files.brand")));
        assert!(bp.segments.is_empty());
    }

    #[test]
    fn parse_context_session() {
        let bp = BindingPath::parse("$context.session").unwrap();
        assert_eq!(bp.source, BindingSource::Context(Arc::from("session")));
        assert!(bp.segments.is_empty());
    }

    #[test]
    fn parse_input() {
        let bp = BindingPath::parse("$inputs.locale").unwrap();
        assert_eq!(bp.source, BindingSource::Input(Arc::from("locale")));
        assert!(bp.segments.is_empty());
    }

    #[test]
    fn parse_input_nested() {
        let bp = BindingPath::parse("$inputs.config.theme").unwrap();
        assert_eq!(bp.source, BindingSource::Input(Arc::from("config.theme")));
        assert!(bp.segments.is_empty());
    }

    #[test]
    fn parse_env() {
        let bp = BindingPath::parse("$env.API_URL").unwrap();
        assert_eq!(bp.source, BindingSource::Env(Arc::from("API_URL")));
        assert!(bp.segments.is_empty());
    }

    // ─────────────────────────────────────────────────────────────
    // BindingPath::parse — Loop variables
    // ─────────────────────────────────────────────────────────────

    #[test]
    fn parse_loop_var() {
        let bp = BindingPath::parse_with_loop_vars("$item", &["item"]).unwrap();
        assert_eq!(bp.source, BindingSource::LoopVar(Arc::from("item")));
        assert!(bp.segments.is_empty());
    }

    #[test]
    fn parse_loop_var_with_field() {
        let bp = BindingPath::parse_with_loop_vars("$item.name", &["item"]).unwrap();
        assert_eq!(bp.source, BindingSource::LoopVar(Arc::from("item")));
        assert_eq!(bp.segments, vec![PathSegment::Field(Arc::from("name"))]);
    }

    #[test]
    fn parse_without_loop_hint_is_task() {
        // Without loop_vars hint, $item is treated as a Task reference
        let bp = BindingPath::parse("$item").unwrap();
        assert_eq!(bp.source, BindingSource::Task(Arc::from("item")));
    }

    // ─────────────────────────────────────────────────────────────
    // BindingPath::parse — Array indexing
    // ─────────────────────────────────────────────────────────────

    #[test]
    fn parse_index_segment() {
        let bp = BindingPath::parse("$data[0]").unwrap();
        assert_eq!(bp.source, BindingSource::Task(Arc::from("data")));
        assert_eq!(bp.segments, vec![PathSegment::Index(0)]);
    }

    #[test]
    fn parse_multiple_indexes() {
        let bp = BindingPath::parse("$data[0].items[1]").unwrap();
        assert_eq!(bp.source, BindingSource::Task(Arc::from("data")));
        assert_eq!(
            bp.segments,
            vec![
                PathSegment::Index(0),
                PathSegment::Field(Arc::from("items")),
                PathSegment::Index(1),
            ]
        );
    }

    #[test]
    fn parse_consecutive_indexes() {
        let bp = BindingPath::parse("$matrix[0][1]").unwrap();
        assert_eq!(bp.source, BindingSource::Task(Arc::from("matrix")));
        assert_eq!(
            bp.segments,
            vec![PathSegment::Index(0), PathSegment::Index(1)]
        );
    }

    // ─────────────────────────────────────────────────────────────
    // BindingPath::parse — Error cases
    // ─────────────────────────────────────────────────────────────

    #[test]
    fn parse_missing_dollar() {
        let err = BindingPath::parse("step1").unwrap_err();
        assert!(err.reason.contains("must start with '$'"));
    }

    #[test]
    fn parse_empty() {
        let err = BindingPath::parse("$").unwrap_err();
        assert!(err.reason.contains("empty path"));
    }

    #[test]
    fn parse_empty_string() {
        let err = BindingPath::parse("").unwrap_err();
        assert!(err.reason.contains("must start with '$'"));
    }

    #[test]
    fn parse_unclosed_bracket() {
        let err = BindingPath::parse("$data[0").unwrap_err();
        assert!(err.reason.contains("unclosed bracket"));
    }

    #[test]
    fn parse_invalid_index() {
        let err = BindingPath::parse("$data[abc]").unwrap_err();
        assert!(err.reason.contains("invalid array index"));
    }

    #[test]
    fn parse_context_without_subpath() {
        let err = BindingPath::parse("$context").unwrap_err();
        assert!(err.reason.contains("requires a sub-path"));
    }

    #[test]
    fn parse_inputs_without_subpath() {
        let err = BindingPath::parse("$inputs").unwrap_err();
        assert!(err.reason.contains("requires a sub-path"));
    }

    #[test]
    fn parse_env_without_var() {
        let err = BindingPath::parse("$env").unwrap_err();
        assert!(err.reason.contains("requires a variable name"));
    }

    #[test]
    fn parse_unexpected_close_bracket() {
        let err = BindingPath::parse("$data]0[").unwrap_err();
        assert!(err.reason.contains("unexpected ']'"));
    }

    // ─────────────────────────────────────────────────────────────
    // BindingPath::parse — Whitespace handling
    // ─────────────────────────────────────────────────────────────

    #[test]
    fn parse_with_leading_whitespace() {
        let bp = BindingPath::parse("  $step1.name  ").unwrap();
        assert_eq!(bp.source, BindingSource::Task(Arc::from("step1")));
        assert_eq!(bp.segments, vec![PathSegment::Field(Arc::from("name"))]);
    }

    // ─────────────────────────────────────────────────────────────
    // Display roundtrip
    // ─────────────────────────────────────────────────────────────

    #[test]
    fn display_roundtrip_task() {
        let original = "$step1.data.items[0].name";
        let bp = BindingPath::parse(original).unwrap();
        let displayed = bp.to_string();
        let reparsed = BindingPath::parse(&displayed).unwrap();
        assert_eq!(bp, reparsed);
    }

    #[test]
    fn display_roundtrip_context() {
        let original = "$context.files.brand";
        let bp = BindingPath::parse(original).unwrap();
        let displayed = bp.to_string();
        let reparsed = BindingPath::parse(&displayed).unwrap();
        assert_eq!(bp, reparsed);
    }

    #[test]
    fn display_roundtrip_inputs() {
        let original = "$inputs.config.theme";
        let bp = BindingPath::parse(original).unwrap();
        let displayed = bp.to_string();
        let reparsed = BindingPath::parse(&displayed).unwrap();
        assert_eq!(bp, reparsed);
    }

    #[test]
    fn display_roundtrip_env() {
        let original = "$env.API_URL";
        let bp = BindingPath::parse(original).unwrap();
        let displayed = bp.to_string();
        let reparsed = BindingPath::parse(&displayed).unwrap();
        assert_eq!(bp, reparsed);
    }

    #[test]
    fn display_simple_task() {
        let bp = BindingPath::parse("$step1").unwrap();
        assert_eq!(bp.to_string(), "$step1");
    }

    // ─────────────────────────────────────────────────────────────
    // Helper methods
    // ─────────────────────────────────────────────────────────────

    #[test]
    fn task_id_extraction() {
        let bp = BindingPath::parse("$step1.field").unwrap();
        assert_eq!(bp.task_id().map(|s| s.as_ref()), Some("step1"));
    }

    #[test]
    fn task_id_none_for_context() {
        let bp = BindingPath::parse("$context.files.brand").unwrap();
        assert!(bp.task_id().is_none());
    }

    #[test]
    fn is_task_ref_true() {
        let bp = BindingPath::parse("$step1").unwrap();
        assert!(bp.is_task_ref());
    }

    #[test]
    fn is_task_ref_false_for_context() {
        let bp = BindingPath::parse("$context.files.brand").unwrap();
        assert!(!bp.is_task_ref());
    }

    #[test]
    fn is_task_ref_false_for_input() {
        let bp = BindingPath::parse("$inputs.locale").unwrap();
        assert!(!bp.is_task_ref());
    }

    #[test]
    fn is_task_ref_false_for_env() {
        let bp = BindingPath::parse("$env.HOME").unwrap();
        assert!(!bp.is_task_ref());
    }

    #[test]
    fn source_is_task() {
        assert!(BindingSource::Task(Arc::from("x")).is_task());
        assert!(!BindingSource::Context(Arc::from("x")).is_task());
        assert!(!BindingSource::Input(Arc::from("x")).is_task());
        assert!(!BindingSource::Env(Arc::from("x")).is_task());
        assert!(!BindingSource::Vault {
            service: Arc::from("s"),
            field: Arc::from("f")
        }
        .is_task());
        assert!(!BindingSource::LoopVar(Arc::from("x")).is_task());
    }

    // ─────────────────────────────────────────────────────────────
    // BindingType
    // ─────────────────────────────────────────────────────────────

    #[test]
    fn binding_type_default() {
        assert_eq!(BindingType::default(), BindingType::Any);
    }

    #[test]
    fn binding_type_deserialize() {
        let t: BindingType = serde_json::from_str(r#""string""#).unwrap();
        assert_eq!(t, BindingType::String);
    }

    #[test]
    fn binding_type_all_variants() {
        let cases = [
            (r#""any""#, BindingType::Any),
            (r#""string""#, BindingType::String),
            (r#""number""#, BindingType::Number),
            (r#""integer""#, BindingType::Integer),
            (r#""boolean""#, BindingType::Boolean),
            (r#""array""#, BindingType::Array),
            (r#""object""#, BindingType::Object),
        ];
        for (json, expected) in cases {
            let t: BindingType = serde_json::from_str(json).unwrap();
            assert_eq!(t, expected, "Failed for JSON: {}", json);
        }
    }

    #[test]
    fn binding_type_display() {
        assert_eq!(BindingType::Any.to_string(), "any");
        assert_eq!(BindingType::String.to_string(), "string");
        assert_eq!(BindingType::Number.to_string(), "number");
        assert_eq!(BindingType::Integer.to_string(), "integer");
        assert_eq!(BindingType::Boolean.to_string(), "boolean");
        assert_eq!(BindingType::Array.to_string(), "array");
        assert_eq!(BindingType::Object.to_string(), "object");
    }

    #[test]
    fn binding_type_invalid_deserialize() {
        let result = serde_json::from_str::<BindingType>(r#""unknown""#);
        assert!(result.is_err());
    }

    // ─────────────────────────────────────────────────────────────
    // BindingPath::parse — Vault references ($vault.SERVICE.FIELD)
    // ─────────────────────────────────────────────────────────────

    #[test]
    fn parse_vault_binding() {
        let bp = BindingPath::parse("$vault.stripe.secret").unwrap();
        assert_eq!(
            bp.source,
            BindingSource::Vault {
                service: Arc::from("stripe"),
                field: Arc::from("secret"),
            }
        );
        assert!(bp.segments.is_empty());
    }

    #[test]
    fn parse_vault_binding_with_underscore() {
        let bp = BindingPath::parse("$vault.my_service.api_key").unwrap();
        assert_eq!(
            bp.source,
            BindingSource::Vault {
                service: Arc::from("my_service"),
                field: Arc::from("api_key"),
            }
        );
    }

    #[test]
    fn parse_vault_without_field_errors() {
        let err = BindingPath::parse("$vault.stripe").unwrap_err();
        assert!(err.reason.contains("requires service and field"));
    }

    #[test]
    fn parse_vault_without_subpath_errors() {
        let err = BindingPath::parse("$vault").unwrap_err();
        assert!(err.reason.contains("requires service and field"));
    }

    #[test]
    fn parse_vault_too_many_segments_errors() {
        let err = BindingPath::parse("$vault.stripe.secret.extra").unwrap_err();
        assert!(err.reason.contains("exactly two segments"));
    }

    #[test]
    fn is_task_ref_false_for_vault() {
        let bp = BindingPath::parse("$vault.stripe.key").unwrap();
        assert!(!bp.is_task_ref());
    }

    #[test]
    fn display_roundtrip_vault() {
        let original = "$vault.stripe.secret";
        let bp = BindingPath::parse(original).unwrap();
        let displayed = bp.to_string();
        assert_eq!(displayed, original);
        let reparsed = BindingPath::parse(&displayed).unwrap();
        assert_eq!(bp, reparsed);
    }

    #[test]
    fn vault_source_is_vault() {
        assert!(BindingSource::Vault {
            service: Arc::from("s"),
            field: Arc::from("f")
        }
        .is_vault());
        assert!(!BindingSource::Task(Arc::from("x")).is_vault());
        assert!(!BindingSource::Env(Arc::from("x")).is_vault());
    }

    // ─────────────────────────────────────────────────────────────
    // Edge cases
    // ─────────────────────────────────────────────────────────────

    #[test]
    fn parse_underscore_task_id() {
        let bp = BindingPath::parse("$my_step_1.result").unwrap();
        assert_eq!(bp.source, BindingSource::Task(Arc::from("my_step_1")));
    }

    #[test]
    fn parse_context_deep_path() {
        let bp = BindingPath::parse("$context.files.config.nested.deep").unwrap();
        assert_eq!(
            bp.source,
            BindingSource::Context(Arc::from("files.config.nested.deep"))
        );
    }

    #[test]
    fn parse_env_with_numbers() {
        let bp = BindingPath::parse("$env.AWS_REGION_1").unwrap();
        assert_eq!(bp.source, BindingSource::Env(Arc::from("AWS_REGION_1")));
    }

    #[test]
    fn binding_path_error_display() {
        let err = BindingPathError {
            input: "step1".to_string(),
            reason: "must start with '$'".to_string(),
        };
        assert!(err.to_string().contains("NIKA-150"));
        assert!(err.to_string().contains("step1"));
    }

    #[test]
    fn template_syntax_in_binding_gives_helpful_error() {
        // E10: {{...}} in with: blocks should suggest $ syntax
        let err = BindingPath::parse("{{inputs.name | upper}}").unwrap_err();
        assert!(
            err.reason.to_lowercase().contains("template syntax"),
            "error should mention template syntax: {}",
            err.reason
        );
        assert!(
            err.reason.contains("$task_id"),
            "error should suggest $ syntax: {}",
            err.reason
        );
    }

    #[test]
    fn clone_and_eq() {
        let bp1 = BindingPath::parse("$step1.name").unwrap();
        let bp2 = bp1.clone();
        assert_eq!(bp1, bp2);
    }

    #[test]
    fn hash_consistency() {
        use std::collections::HashSet;
        let bp1 = BindingPath::parse("$step1.name").unwrap();
        let bp2 = BindingPath::parse("$step1.name").unwrap();
        let mut set = HashSet::new();
        set.insert(bp1);
        set.insert(bp2);
        assert_eq!(set.len(), 1); // Same path = same hash
    }
}
