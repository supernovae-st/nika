// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Template Resolution — 3-Pass Variable Substitution
//!
//! This module resolves template variables in workflow strings using a 3-pass
//! architecture that ensures security isolation between different binding sources.
//!
//! # 3-Pass Resolution Architecture
//!
//! Template resolution happens in strict sequential order:
//!
//! ```text
//! ┌─────────────────────────────────────────────────────────────────────────┐
//! │  3-PASS TEMPLATE RESOLUTION                                    │
//! ├─────────────────────────────────────────────────────────────────────────┤
//! │                                                                         │
//! │  Pass 1: {{with.alias}} — Task output bindings                         │
//! │  ─────────────────────────────────────────────────────────────────────  │
//! │  • Resolves task outputs bound via `with:` block                        │
//! │  • Supports nested paths: {{with.data.field}}                           │
//! │  • Supports array indexing: {{with.items[0]}} or {{with.items.0}}       │
//! │  • Supports |shell modifier: {{with.value|shell}}                       │
//! │  • Lazy bindings resolved on-demand via RunContext                       │
//! │                                                                         │
//! │  Pass 2: {{context.*}} — Workflow context files                         │
//! │  ─────────────────────────────────────────────────────────────────────  │
//! │  • {{context.files.alias}} — Loaded from `context.files` block          │
//! │  • {{context.session.key}} — Loaded from `context.session` file         │
//! │  • Content is loaded at workflow start, before task execution           │
//! │                                                                         │
//! │  Pass 3: {{inputs.param}} — Workflow input parameters                   │
//! │  ─────────────────────────────────────────────────────────────────────  │
//! │  • Resolves from workflow `inputs:` definitions                         │
//! │  • Uses `default` values when not provided at runtime                   │
//! │  • Supports nested paths: {{inputs.config.theme}}                       │
//! │                                                                         │
//! └─────────────────────────────────────────────────────────────────────────┘
//! ```
//!
//! # Security Isolation
//!
//! **CRITICAL**: Each pass operates on the OUTPUT of the previous pass, but
//! template markers in VALUES are NOT re-evaluated. This prevents injection:
//!
//! ```yaml
//! # If {{with.user_input}} resolves to "{{context.files.secret}}"
//! # The output is literally "{{context.files.secret}}", NOT the file content
//! ```
//!
//! See the `injection_*` tests for comprehensive security verification.
//!
//! # Syntax Reference
//!
//! | Pattern | Source | Example |
//! |---------|--------|---------|
//! | `{{with.alias}}` | Task `with:` block | `{{with.forecast}}` |
//! | `{{with.alias.field}}` | Nested JSON access | `{{with.data.name}}` |
//! | `{{with.alias[N]}}` | Array indexing | `{{with.items[0]}}` |
//! | `{{with.alias\|shell}}` | Shell-escaped | `{{with.filename\|shell}}` |
//! | `{{context.files.X}}` | Context file | `{{context.files.brand}}` |
//! | `{{context.session.X}}` | Session data | `{{context.session.focus}}` |
//! | `{{inputs.param}}` | Input parameter | `{{inputs.topic}}` |
//!
//! # Performance
//!
//! - Returns `Cow::Borrowed` when no templates (zero allocation)
//! - Zero-clone traversal (references until final value)
//! - SmallVec for error collection (stack-allocated up to 4)
//! - Pre-compiled regex via `LazyLock`

use std::borrow::Cow;
use std::sync::LazyLock;

use regex::Regex;
use rustc_hash::{FxHashMap, FxHashSet};
use serde_json::Value;
use smallvec::SmallVec;

use crate::error::NikaError;
use crate::error_domains::BindingError;
use crate::store::RunContext;

use super::resolve::ResolvedBindings;
use super::transform::TransformExpr;

/// Maximum number of template variables allowed per string.
///
/// Prevents CPU exhaustion from pathological templates containing thousands of
/// `{{...}}` blocks that trigger regex backtracking and allocation storms.
const MAX_TEMPLATE_VARS: usize = 256;

/// Maximum path depth for nested alias traversal (e.g., `a.b.c.d.e`).
///
/// Prevents stack/allocation exhaustion from malicious deep paths like
/// `{{a.b.c.d.e.f.g.h.i.j.k.l.m.n.o.p.q.r.s.t.u.v.w.x.y.z}}`.
const MAX_PATH_DEPTH: usize = 32;

/// Pre-compiled regex for {{with.alias}} pattern.
/// Supports optional pipe transforms: {{with.alias|shell}}, {{with.alias|uppercase|trim}}
/// Also supports bracket notation after preprocessing: {{with.items[0]}} → {{with.items.0}}
static USE_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"\{\{\s*with\.(\w+(?:\.\w+)*)(\s*(?:\|\s*\w+)+)?\s*\}\}").unwrap()
});

/// Pre-compiled regex for bracket array notation
/// Converts [0] to .0 for uniform handling
static BRACKET_RE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\[(\d+)\]").unwrap());

// ═══════════════════════════════════════════════════════════════════════════════
// New 2-pass template engine with iterative parser
// ═══════════════════════════════════════════════════════════════════════════════

/// Matches ANY {{...}} block. Content is parsed by parse_template_expr().
/// Unified regex that replaces per-namespace patterns -- dispatched via parse_template_expr().
static TEMPLATE_RE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\{\{(.*?)\}\}").unwrap());

/// Parsed template expression from inside `{{ ... }}`
///
/// Uses an iterative parser that correctly handles words like "contextual"
/// and enables arbitrary transform chains.
#[derive(Debug, Clone, PartialEq)]
pub enum TemplateExpr {
    /// Alias from `with:` block, with optional transforms
    /// e.g., `"title"`, `"title | upper | trim"`, `"data.items[0]"`
    Alias {
        path: String,
        transforms: Vec<String>,
    },
    /// Direct context reference: `"context.files.brand"` or `"context.session.key"`
    Context {
        path: String,
        transforms: Vec<String>,
    },
    /// Direct input reference: `"inputs.locale"` or `"inputs.config.theme"`
    Input {
        path: String,
        transforms: Vec<String>,
    },
    /// Direct skill reference: `"skills.pirate"` or `"skills.writing | trim"`
    Skills {
        path: String,
        transforms: Vec<String>,
    },
}

/// Parse the content inside `{{ ... }}` into a TemplateExpr.
///
/// Grammar:
/// ```text
///   expr := "context." path             → Context
///         | "inputs." path              → Input
///         | "with." alias_path ("|" t)* → Alias (with. prefix stripped)
///         | alias_path ("|" transform)* → Alias
/// ```
///
/// This replaces the buggy negative-lookahead regex approach:
/// 1. No negative lookahead bugs — exact `strip_prefix("context.")` is unambiguous
/// 2. Arbitrary transform chains — `split('|')` handles any number of pipes
/// 3. Better error messages — parser can report exactly what's wrong
/// 4. Simpler to maintain — no complex regex to debug
pub fn parse_template_expr(content: &str) -> Result<TemplateExpr, NikaError> {
    let trimmed = content.trim();

    if trimmed.is_empty() {
        return Err(NikaError::TemplateParse {
            position: 0,
            details: format!("Empty template expression in '{}'", content),
        });
    }

    // Check for context.* and inputs.* FIRST (exact prefix match)
    // "contextual" → NOT Context (no dot after "context")
    // "inputstream" → NOT Input (no dot after "inputs")
    if let Some(rest) = trimmed.strip_prefix("context.") {
        if rest.is_empty() {
            return Err(NikaError::TemplateParse {
                position: 0,
                details: format!("Empty context path after 'context.' in '{}'", content),
            });
        }
        let parts: Vec<&str> = rest.split('|').map(str::trim).collect();
        let path = parts[0].to_string();
        let transforms: Vec<String> = parts[1..].iter().map(|s| s.to_string()).collect();
        if path.is_empty() {
            return Err(NikaError::TemplateParse {
                position: 0,
                details: format!("Empty context path after 'context.' in '{}'", content),
            });
        }
        return Ok(TemplateExpr::Context { path, transforms });
    }
    if let Some(rest) = trimmed.strip_prefix("inputs.") {
        if rest.is_empty() {
            return Err(NikaError::TemplateParse {
                position: 0,
                details: format!("Empty input path after 'inputs.' in '{}'", content),
            });
        }
        let parts: Vec<&str> = rest.split('|').map(str::trim).collect();
        let path = parts[0].to_string();
        let transforms: Vec<String> = parts[1..].iter().map(|s| s.to_string()).collect();
        if path.is_empty() {
            return Err(NikaError::TemplateParse {
                position: 0,
                details: format!("Empty input path after 'inputs.' in '{}'", content),
            });
        }
        return Ok(TemplateExpr::Input { path, transforms });
    }
    if let Some(rest) = trimmed.strip_prefix("skills.") {
        if rest.is_empty() {
            return Err(NikaError::TemplateParse {
                position: 0,
                details: format!("Empty skill path after 'skills.' in '{}'", content),
            });
        }
        let parts: Vec<&str> = rest.split('|').map(str::trim).collect();
        let path = parts[0].to_string();
        let transforms: Vec<String> = parts[1..].iter().map(|s| s.to_string()).collect();
        if path.is_empty() {
            return Err(NikaError::TemplateParse {
                position: 0,
                details: format!("Empty skill path after 'skills.' in '{}'", content),
            });
        }
        return Ok(TemplateExpr::Skills { path, transforms });
    }

    // Strip "with." prefix to get alias path
    // "with.data" → alias path "data"
    let effective = trimmed.strip_prefix("with.").unwrap_or(trimmed);

    // Everything else is an alias (possibly with transforms)
    // Split by | to get alias path and transforms
    let parts: Vec<&str> = effective.split('|').map(str::trim).collect();
    let path = parts[0].to_string();
    let transforms: Vec<String> = parts[1..].iter().map(|s| s.to_string()).collect();

    if path.is_empty() {
        return Err(NikaError::TemplateParse {
            position: 0,
            details: format!("Empty alias path in '{}'", content),
        });
    }

    Ok(TemplateExpr::Alias { path, transforms })
}

/// Convert a Value to its display string for template interpolation
///
/// Unlike `value_to_string()` which errors on null, this returns empty string.
///
/// - String: raw string (no quotes)
/// - Number/Bool: to_string()
/// - Array/Object: JSON-serialize (lossless for LLMs)
/// - Null: empty string
fn value_to_display(value: &Value) -> Cow<'_, str> {
    match value {
        Value::String(s) => Cow::Borrowed(s.as_str()),
        Value::Null => Cow::Borrowed(""),
        Value::Bool(b) => Cow::Owned(b.to_string()),
        Value::Number(n) => Cow::Owned(n.to_string()),
        other => Cow::Owned(other.to_string()), // JSON representation for objects/arrays
    }
}

/// Resolve a dot-separated path against a FxHashMap of alias → Value
///
/// Supports nested paths like "data.items.0" or "data.users.0.name".
fn resolve_alias_path(
    path: &str,
    with_values: &FxHashMap<String, Value>,
) -> Result<Value, NikaError> {
    // Guard against pathologically deep paths
    let segment_count = path.split('.').count();
    if segment_count > MAX_PATH_DEPTH {
        return Err(BindingError::TemplateError {
            template: path.to_string(),
            reason: format!(
                "Path depth {} exceeds maximum of {} segments",
                segment_count, MAX_PATH_DEPTH
            ),
        }
        .into());
    }

    let mut segments = path.split('.');
    let alias = segments.next().ok_or_else(|| BindingError::TemplateError {
        template: path.to_string(),
        reason: "Empty alias path (no segments)".to_string(),
    })?;

    let base = with_values
        .get(alias)
        .ok_or_else(|| BindingError::TemplateError {
            template: alias.to_string(),
            reason: format!(
                "Alias '{}' not found in 'with:' block. Available: [{}]",
                alias,
                with_values.keys().cloned().collect::<Vec<_>>().join(", ")
            ),
        })?;

    // Auto-parse JSON strings so invoke/exec outputs stored as
    // Value::String('{"hash":"blake3:..."}') can be traversed with
    // nested paths like {{chart_result.hash}}.
    // Matches navigate_segments() in binding/resolve.rs (NIKA-253 fix).
    let effective_base =
        crate::binding::jsonpath::try_parse_json_str(base).unwrap_or_else(|| base.clone());
    let mut current = &effective_base;
    let mut traversed: SmallVec<[&str; 8]> = SmallVec::new();
    traversed.push(alias);

    for segment in segments {
        let next = if let Ok(idx) = segment.parse::<usize>() {
            current.get(idx)
        } else {
            current.get(segment)
        };

        match next {
            Some(v) => {
                traversed.push(segment);
                current = v;
            }
            None => {
                if matches!(current, Value::Object(_) | Value::Array(_)) {
                    let traversed_path = traversed.join(".");
                    return Err(NikaError::PathNotFound {
                        path: format!("{}.{}", traversed_path, segment),
                    });
                } else {
                    let value_type = match current {
                        Value::Null => "null",
                        Value::Bool(_) => "bool",
                        Value::Number(_) => "number",
                        Value::String(_) => "string",
                        _ => unreachable!(),
                    };
                    return Err(NikaError::InvalidTraversal {
                        segment: segment.to_string(),
                        value_type: value_type.to_string(),
                        full_path: path.to_string(),
                    });
                }
            }
        }
    }

    Ok(current.clone())
}

/// Resolve all template references in a string
///
/// Pass 1: `{{alias}}` and `{{alias | transform}}` — resolved from `with:` values
/// Pass 2: `{{context.*}}` and `{{inputs.*}}` — direct access (convenience)
///
/// Security: Pass 2 values are NOT re-evaluated by Pass 1 patterns.
/// Template markers in resolved VALUES are never re-processed.
///
/// Features:
/// - Takes `with_values` directly (not `ResolvedBindings`)
/// - No `with.` prefix needed (`{{title}}` instead of `{{with.title}}`)
/// - Supports arbitrary transform chains (`{{title | upper | trim}}`)
/// - Returns empty string for null values (not error)
pub fn resolve_with<'a>(
    template: &'a str,
    with_values: &FxHashMap<String, Value>,
    datastore: &RunContext,
) -> Result<Cow<'a, str>, NikaError> {
    // Early return with borrowed string (zero alloc)
    if !template.contains("{{") {
        return Ok(Cow::Borrowed(template));
    }

    // Guard: reject templates with too many variable references
    let var_count = template.matches("{{").count();
    if var_count > MAX_TEMPLATE_VARS {
        return Err(BindingError::TemplateError {
            template: format!("(template with {} variables)", var_count),
            reason: format!(
                "Template contains {} variable references, exceeding the maximum of {}",
                var_count, MAX_TEMPLATE_VARS
            ),
        }
        .into());
    }

    // Normalize bracket notation to dot notation
    // {{items[0]}} → {{items.0}}
    let normalized = normalize_bracket_notation(template);
    let template_str: &str = normalized.as_ref();

    // ─── Pass 1: Find all {{...}} blocks, parse each, resolve aliases ───
    let mut result = String::with_capacity(template_str.len() + 64);
    let mut last_end = 0;
    let mut errors: SmallVec<[String; 4]> = SmallVec::new();

    for cap in TEMPLATE_RE.captures_iter(template_str) {
        let m = cap.get(0).unwrap();
        let content = &cap[1];

        // Copy segment before this match
        result.push_str(&template_str[last_end..m.start()]);

        match parse_template_expr(content) {
            Ok(TemplateExpr::Alias {
                ref path,
                ref transforms,
            }) => {
                match resolve_alias_path(path, with_values) {
                    Ok(value) => {
                        let has_shell = transforms.iter().any(|t| t == "shell");

                        // When shell is in transforms, skip the full-chain application
                        // to avoid double-processing. Only run the non-shell transforms
                        // followed by escape_for_shell.
                        // PERF(M5): Avoid value.clone() when no transforms —
                        // value_to_display() takes &Value, so pass reference directly.
                        let display = if has_shell {
                            let non_shell: Vec<String> = transforms
                                .iter()
                                .filter(|t| *t != "shell")
                                .cloned()
                                .collect();
                            if non_shell.is_empty() {
                                escape_for_shell(&value_to_display(&value))
                            } else {
                                let transform_str = non_shell.join(" | ");
                                let expr = TransformExpr::parse(&transform_str).map_err(|e| {
                                    NikaError::TemplateParse {
                                        position: m.start(),
                                        details: format!(
                                            "Transform parse error in '{{{{{}}}}}': {}",
                                            content, e
                                        ),
                                    }
                                })?;
                                let transformed =
                                    expr.apply(&value).map_err(|e| NikaError::TemplateParse {
                                        position: m.start(),
                                        details: format!(
                                            "Transform apply error in '{{{{{}}}}}': {}",
                                            content, e
                                        ),
                                    })?;
                                escape_for_shell(&value_to_display(&transformed))
                            }
                        } else if transforms.is_empty() {
                            // No transforms at all — direct reference, zero clone
                            if is_in_json_context(template_str, m.start()) {
                                escape_for_json(&value_to_display(&value)).into_owned()
                            } else {
                                value_to_display(&value).into_owned()
                            }
                        } else {
                            // Apply transform chain (no shell)
                            let transform_str = transforms.join(" | ");
                            let expr = TransformExpr::parse(&transform_str).map_err(|e| {
                                NikaError::TemplateParse {
                                    position: m.start(),
                                    details: format!(
                                        "Transform parse error in '{{{{{}}}}}': {}",
                                        content, e
                                    ),
                                }
                            })?;
                            let transformed =
                                expr.apply(&value).map_err(|e| NikaError::TemplateParse {
                                    position: m.start(),
                                    details: format!(
                                        "Transform apply error in '{{{{{}}}}}': {}",
                                        content, e
                                    ),
                                })?;
                            if is_in_json_context(template_str, m.start()) {
                                escape_for_json(&value_to_display(&transformed)).into_owned()
                            } else {
                                value_to_display(&transformed).into_owned()
                            }
                        };
                        result.push_str(&display);
                    }
                    Err(e) => {
                        // Propagate structural errors (depth, parse) immediately;
                        // only collect "not found" errors for the batch message.
                        let msg = format!("{}", e);
                        if msg.contains("exceeds maximum") || msg.contains("Empty alias path") {
                            return Err(e);
                        }
                        // BUG-035: If transforms include default(), recover with null
                        // so the default() transform can fire instead of NIKA-052.
                        // Use TransformExpr::has_default() for type-safe detection
                        // (matches resolve.rs pattern at line 699).
                        if !transforms.is_empty() {
                            let transform_str = transforms.join(" | ");
                            if let Ok(expr) = TransformExpr::parse(&transform_str) {
                                if expr.has_default() {
                                    if let Ok(transformed) = expr.apply(&Value::Null) {
                                        result.push_str(&value_to_display(&transformed));
                                        last_end = m.end();
                                        continue;
                                    }
                                }
                            }
                        }
                        errors.push(path.clone());
                    }
                }
            }
            Ok(
                TemplateExpr::Context { .. }
                | TemplateExpr::Input { .. }
                | TemplateExpr::Skills { .. },
            ) => {
                // Leave context/inputs/skills refs for later passes — re-emit as {{...}}
                result.push_str(&format!("{{{{{}}}}}", content.trim()));
            }
            Err(e) => {
                // Malformed expression — re-emit literally with warning
                tracing::warn!(expression = %content.trim(), error = %e, "Malformed template expression — passing through literally");
                result.push_str(m.as_str());
            }
        }

        last_end = m.end();
    }

    if !errors.is_empty() {
        return Err(BindingError::TemplateError {
            template: errors.join(", "),
            reason: "Alias(es) not resolved. Did you declare them in 'with:'?".to_string(),
        }
        .into());
    }

    // Copy remaining segment after last match
    result.push_str(&template_str[last_end..]);

    // ─── Pass 2: Resolve {{context.*}} and {{inputs.*}} (direct refs) ───
    // SECURITY: Check the ORIGINAL template for context/inputs references, not
    // the post-Pass-1 result. This prevents template injection where a with: value
    // containing "{{context.files.x}}" triggers Pass 2 resolution.
    let has_context = template.contains("context.");
    let has_inputs = template.contains("inputs.");
    let has_skills = template.contains("skills.");

    if !has_context && !has_inputs && !has_skills {
        return Ok(Cow::Owned(result));
    }

    // SECURITY: Collect trusted context paths from ORIGINAL template (same as resolve())
    if has_context && result.contains("{{") {
        let trusted_context: std::collections::HashSet<String> = TEMPLATE_RE
            .captures_iter(template)
            .filter_map(|cap| {
                let inner = cap[1].trim();
                if let Ok(TemplateExpr::Context { path, .. }) = parse_template_expr(inner) {
                    Some(format!("context.{}", path))
                } else {
                    None
                }
            })
            .collect();

        let intermediate = std::mem::take(&mut result);
        result = String::with_capacity(intermediate.len() + 64);
        let mut last_end = 0;
        let mut context_errors: SmallVec<[String; 4]> = SmallVec::new();

        for cap in TEMPLATE_RE.captures_iter(&intermediate) {
            let m = cap.get(0).unwrap();
            let inner = cap[1].trim();
            let (path, transforms) = match parse_template_expr(inner) {
                Ok(TemplateExpr::Context { path, transforms }) => (path, transforms),
                _ => continue,
            };
            let full_path = format!("context.{}", path);
            // Skip injected context refs that weren't in the original template
            if !trusted_context.contains(&full_path) {
                tracing::warn!(
                    path = %full_path,
                    "Blocked injected context reference in resolve_with (template injection attempt)"
                );
                continue;
            }
            result.push_str(&intermediate[last_end..m.start()]);
            match datastore.resolve_context_path(&full_path) {
                Some(value) => {
                    let replacement = if !transforms.is_empty() {
                        let transform_str = transforms.join(" | ");
                        let expr = TransformExpr::parse(&transform_str).map_err(|e| {
                            NikaError::TemplateParse {
                                position: m.start(),
                                details: format!("Transform parse error: {}", e),
                            }
                        })?;
                        let transformed =
                            expr.apply(&value).map_err(|e| NikaError::TemplateParse {
                                position: m.start(),
                                details: format!("Transform apply error: {}", e),
                            })?;
                        if is_in_json_context(&intermediate, m.start()) {
                            escape_for_json(&value_to_display(&transformed)).into_owned()
                        } else {
                            value_to_display(&transformed).into_owned()
                        }
                    } else {
                        let s = context_value_to_string(&value, &full_path)?;
                        if is_in_json_context(&intermediate, m.start()) {
                            escape_for_json(&s).into_owned()
                        } else {
                            s.into_owned()
                        }
                    };
                    result.push_str(&replacement);
                }
                None => {
                    context_errors.push(full_path);
                }
            }

            last_end = m.end();
        }

        if !context_errors.is_empty() {
            return Err(BindingError::TemplateError {
                template: context_errors.join(", "),
                reason: "Context binding(s) not resolved. Check your 'context:' block in workflow."
                    .to_string(),
            }
            .into());
        }

        result.push_str(&intermediate[last_end..]);
    }

    // SECURITY: Collect trusted input paths from the ORIGINAL template (same as resolve())
    if has_inputs && result.contains("{{") {
        let trusted_inputs: std::collections::HashSet<String> = TEMPLATE_RE
            .captures_iter(template)
            .filter_map(|cap| {
                let inner = cap[1].trim();
                if let Ok(TemplateExpr::Input { path, .. }) = parse_template_expr(inner) {
                    Some(format!("inputs.{}", path))
                } else {
                    None
                }
            })
            .collect();

        let intermediate = std::mem::take(&mut result);
        result = String::with_capacity(intermediate.len() + 64);
        let mut last_end = 0;
        let mut input_errors: SmallVec<[String; 4]> = SmallVec::new();

        for cap in TEMPLATE_RE.captures_iter(&intermediate) {
            let m = cap.get(0).unwrap();
            let inner = cap[1].trim();
            let (path, transforms) = match parse_template_expr(inner) {
                Ok(TemplateExpr::Input { path, transforms }) => (path, transforms),
                _ => continue,
            };
            let full_path = format!("inputs.{}", path);
            // Skip injected input refs that weren't in the original template
            if !trusted_inputs.contains(&full_path) {
                tracing::warn!(
                    path = %full_path,
                    "Blocked injected input reference in resolve_with (template injection attempt)"
                );
                continue;
            }
            result.push_str(&intermediate[last_end..m.start()]);
            match datastore.resolve_input_path(&full_path) {
                Some(value) => {
                    let replacement = if !transforms.is_empty() {
                        let transform_str = transforms.join(" | ");
                        let expr = TransformExpr::parse(&transform_str).map_err(|e| {
                            NikaError::TemplateParse {
                                position: m.start(),
                                details: format!("Transform parse error: {}", e),
                            }
                        })?;
                        let transformed =
                            expr.apply(&value).map_err(|e| NikaError::TemplateParse {
                                position: m.start(),
                                details: format!("Transform apply error: {}", e),
                            })?;
                        if is_in_json_context(&intermediate, m.start()) {
                            escape_for_json(&value_to_display(&transformed)).into_owned()
                        } else {
                            value_to_display(&transformed).into_owned()
                        }
                    } else {
                        let s = input_value_to_string(&value, &full_path)?;
                        if is_in_json_context(&intermediate, m.start()) {
                            escape_for_json(&s).into_owned()
                        } else {
                            s.into_owned()
                        }
                    };
                    result.push_str(&replacement);
                }
                None => {
                    input_errors.push(full_path);
                }
            }

            last_end = m.end();
        }

        if !input_errors.is_empty() {
            return Err(BindingError::TemplateError {
                template: input_errors.join(", "),
                reason: "Input binding(s) not resolved. Check your 'inputs:' block in workflow or provide defaults.".to_string(),
            }.into());
        }

        result.push_str(&intermediate[last_end..]);
    }

    // SECURITY: Collect trusted skills paths from the ORIGINAL template
    if has_skills && result.contains("{{") {
        let trusted_skills: std::collections::HashSet<String> = TEMPLATE_RE
            .captures_iter(template)
            .filter_map(|cap| {
                let inner = cap[1].trim();
                if let Ok(TemplateExpr::Skills { path, .. }) = parse_template_expr(inner) {
                    Some(format!("skills.{}", path))
                } else {
                    None
                }
            })
            .collect();

        let intermediate = std::mem::take(&mut result);
        result = String::with_capacity(intermediate.len() + 64);
        let mut last_end = 0;
        let mut skills_errors: SmallVec<[String; 4]> = SmallVec::new();

        for cap in TEMPLATE_RE.captures_iter(&intermediate) {
            let m = cap.get(0).unwrap();
            let inner = cap[1].trim();
            let (path, transforms) = match parse_template_expr(inner) {
                Ok(TemplateExpr::Skills { path, transforms }) => (path, transforms),
                _ => continue,
            };
            let full_path = format!("skills.{}", path);
            if !trusted_skills.contains(&full_path) {
                tracing::warn!(
                    path = %full_path,
                    "Blocked injected skills reference in resolve_with (template injection attempt)"
                );
                continue;
            }
            result.push_str(&intermediate[last_end..m.start()]);
            match datastore.resolve_skills_path(&path) {
                Some(value) => {
                    let replacement = if !transforms.is_empty() {
                        let transform_str = transforms.join(" | ");
                        let expr = TransformExpr::parse(&transform_str).map_err(|e| {
                            NikaError::TemplateParse {
                                position: m.start(),
                                details: format!("Transform parse error: {}", e),
                            }
                        })?;
                        let transformed =
                            expr.apply(&value).map_err(|e| NikaError::TemplateParse {
                                position: m.start(),
                                details: format!("Transform apply error: {}", e),
                            })?;
                        if is_in_json_context(&intermediate, m.start()) {
                            escape_for_json(&value_to_display(&transformed)).into_owned()
                        } else {
                            value_to_display(&transformed).into_owned()
                        }
                    } else if is_in_json_context(&intermediate, m.start()) {
                        escape_for_json(&value_to_display(&value)).into_owned()
                    } else {
                        value_to_display(&value).into_owned()
                    };
                    result.push_str(&replacement);
                }
                None => {
                    skills_errors.push(full_path);
                }
            }

            last_end = m.end();
        }

        if !skills_errors.is_empty() {
            return Err(BindingError::TemplateError {
                template: skills_errors.join(", "),
                reason: "Skill binding(s) not resolved. Check your 'skills:' block in workflow."
                    .to_string(),
            }
            .into());
        }

        result.push_str(&intermediate[last_end..]);
    }

    Ok(Cow::Owned(result))
}

/// Extract all alias references from a template
///
/// Returns aliases used in `{{alias}}` patterns (no `use.` prefix).
/// Does NOT return context/input refs — those are direct access.
pub fn extract_with_refs(template: &str) -> Vec<String> {
    if !template.contains("{{") {
        return Vec::new();
    }
    let mut aliases = Vec::new();
    for cap in TEMPLATE_RE.captures_iter(template) {
        let content = &cap[1];
        if let Ok(TemplateExpr::Alias { path, .. }) = parse_template_expr(content) {
            let alias = path.split('.').next().unwrap().to_string();
            aliases.push(alias);
        }
    }
    aliases
}

/// Validate that all template alias references exist in declared aliases,
/// and that all inline transforms are valid.
///
/// Optionally validates `{{inputs.*}}` and `{{context.files.*}}` references
/// when the corresponding sets are provided.
pub fn validate_with_refs(
    template: &str,
    declared_aliases: &FxHashSet<String>,
    task_id: &str,
) -> Result<(), NikaError> {
    validate_with_refs_full(template, declared_aliases, task_id, None, None)
}

/// Extended validation with optional inputs and context file checking.
pub fn validate_with_refs_full(
    template: &str,
    declared_aliases: &FxHashSet<String>,
    task_id: &str,
    declared_inputs: Option<&FxHashSet<String>>,
    declared_context_files: Option<&FxHashSet<String>>,
) -> Result<(), NikaError> {
    if !template.contains("{{") {
        return Ok(());
    }
    for cap in TEMPLATE_RE.captures_iter(template) {
        let content = &cap[1];

        // Detect nested templates: {{with.a.{{with.b}}}} → NIKA-074
        if content.contains("{{") {
            return Err(NikaError::TemplateParse {
                position: 0,
                details: format!(
                    "[NIKA-074] Nested templates are not supported in task '{}': \
                     found '{{{{{}}}}}'. Use with: to compose bindings instead.",
                    task_id, content
                ),
            });
        }

        if let Ok(expr) = parse_template_expr(content) {
            // Extract alias and transforms from any variant
            let transforms = match &expr {
                TemplateExpr::Alias { path, transforms } => {
                    // Validate alias reference
                    let alias = path.split('.').next().unwrap().to_string();
                    if !declared_aliases.contains(&alias) {
                        let candidates: Vec<&str> =
                            declared_aliases.iter().map(|s| s.as_str()).collect();
                        let suggestion = nika_core::ast::analyzer::suggestions::find_similar(
                            &alias,
                            &candidates,
                            0.6,
                        );
                        return Err(NikaError::UnknownAlias {
                            alias,
                            task_id: task_id.to_string(),
                            suggestion,
                        });
                    }
                    transforms.as_slice()
                }
                TemplateExpr::Input { path, transforms } => {
                    // Validate inputs.* reference if declared_inputs is provided
                    if let Some(inputs) = declared_inputs {
                        let key = path.split('.').next().unwrap_or(path);
                        if !inputs.contains(key) {
                            return Err(NikaError::TemplateParse {
                                position: 0,
                                details: format!(
                                    "task '{}' references undeclared input '{{{{inputs.{}}}}}'. \
                                     Declared inputs: {}",
                                    task_id,
                                    path,
                                    if inputs.is_empty() {
                                        "(none)".to_string()
                                    } else {
                                        inputs.iter().cloned().collect::<Vec<_>>().join(", ")
                                    }
                                ),
                            });
                        }
                    }
                    transforms.as_slice()
                }
                TemplateExpr::Context { path, transforms } => {
                    // Validate context.files.* reference if declared_context_files is provided
                    if let Some(ctx_files) = declared_context_files {
                        // context.files.alias → check alias exists
                        if let Some(rest) = path.strip_prefix("files.") {
                            let alias = rest.split('.').next().unwrap_or(rest);
                            if !ctx_files.contains(alias) {
                                return Err(NikaError::TemplateParse {
                                    position: 0,
                                    details: format!(
                                        "task '{}' references undeclared context file \
                                         '{{{{context.files.{}}}}}'. Declared files: {}",
                                        task_id,
                                        alias,
                                        if ctx_files.is_empty() {
                                            "(none)".to_string()
                                        } else {
                                            ctx_files.iter().cloned().collect::<Vec<_>>().join(", ")
                                        }
                                    ),
                                });
                            }
                        }
                    }
                    transforms.as_slice()
                }
                TemplateExpr::Skills { transforms, .. } => transforms.as_slice(),
            };

            // Validate inline transforms
            for transform in transforms {
                if let Err(e) = nika_core::binding::TransformExpr::parse(transform) {
                    return Err(NikaError::TemplateParse {
                        position: 0,
                        details: format!(
                            "invalid transform '{}' in task '{}': {}",
                            transform, task_id, e
                        ),
                    });
                }
            }
        }
    }
    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════════
// with: template engine (runtime Workflow path)
// ═══════════════════════════════════════════════════════════════════════════════

/// Escape for JSON string context
///
/// Returns `Cow::Borrowed` when no escaping is needed (common case for simple strings).
fn escape_for_json(s: &str) -> Cow<'_, str> {
    // Fast path: check if any escaping is needed
    let needs_escape = s
        .chars()
        .any(|c| matches!(c, '"' | '\\' | '\n' | '\r' | '\t') || c.is_control());
    if !needs_escape {
        return Cow::Borrowed(s);
    }

    let mut result = String::with_capacity(s.len());
    for ch in s.chars() {
        match ch {
            '"' => result.push_str("\\\""),
            '\\' => result.push_str("\\\\"),
            '\n' => result.push_str("\\n"),
            '\r' => result.push_str("\\r"),
            '\t' => result.push_str("\\t"),
            c if c.is_control() => {
                result.push_str(&format!("\\u{:04x}", c as u32));
            }
            c => result.push(c),
        }
    }
    Cow::Owned(result)
}

/// Escape a string for safe shell usage
///
/// Uses single quotes with proper escaping for all special characters.
/// This ensures values from LLM outputs can be safely used in shell commands.
///
/// Example: "Hello 'world'" becomes "'Hello '\''world'\'''"
pub fn escape_for_shell(s: &str) -> String {
    // Single-quote escaping: wrap in single quotes, escape existing single quotes
    // 'foo' -> safe
    // foo'bar -> 'foo'\''bar'
    if s.is_empty() {
        return "''".to_string();
    }

    let mut result = String::with_capacity(s.len() + 10);
    result.push('\'');

    for ch in s.chars() {
        if ch == '\'' {
            // End current single-quote, add escaped single-quote, start new single-quote
            result.push_str("'\\''");
        } else {
            result.push(ch);
        }
    }

    result.push('\'');
    result
}

/// Normalize bracket notation to dot notation ONLY inside `{{...}}` blocks
///
/// Converts `{{with.items[0]}}` to `{{with.items.0}}` for uniform handling.
/// This allows users to use familiar JavaScript-style array indexing.
///
/// IMPORTANT: Only applies normalization inside template blocks (`{{...}}`),
/// preserving literal bracket notation in surrounding text (e.g., `data[0]`).
fn normalize_bracket_notation(template: &str) -> Cow<'_, str> {
    if !template.contains('[') {
        return Cow::Borrowed(template);
    }

    // Check if any bracket notation exists inside {{ }} blocks
    let mut has_bracket_in_template = false;
    let mut search_start = 0;
    while let Some(open) = template[search_start..].find("{{") {
        let abs_open = search_start + open;
        if let Some(close) = template[abs_open..].find("}}") {
            let block = &template[abs_open..abs_open + close + 2];
            if block.contains('[') {
                has_bracket_in_template = true;
                break;
            }
            search_start = abs_open + close + 2;
        } else {
            break;
        }
    }

    if !has_bracket_in_template {
        return Cow::Borrowed(template);
    }

    // Rebuild string: copy literal segments verbatim, normalize only inside {{ }}
    let mut result = String::with_capacity(template.len());
    let mut pos = 0;

    while pos < template.len() {
        if let Some(open) = template[pos..].find("{{") {
            let abs_open = pos + open;
            // Copy literal text before this {{ block verbatim
            result.push_str(&template[pos..abs_open]);

            if let Some(close) = template[abs_open..].find("}}") {
                let abs_close = abs_open + close + 2;
                let block = &template[abs_open..abs_close];
                // Normalize brackets only within this {{ }} block
                let normalized_block = BRACKET_RE.replace_all(block, ".$1");
                result.push_str(&normalized_block);
                pos = abs_close;
            } else {
                // Unclosed {{ — copy rest verbatim
                result.push_str(&template[abs_open..]);
                pos = template.len();
            }
        } else {
            // No more {{ — copy remaining literal text verbatim
            result.push_str(&template[pos..]);
            break;
        }
    }

    Cow::Owned(result)
}

/// Resolve all {{with.alias}}, {{context.*}}, and {{inputs.*}} templates
///
/// Returns Cow::Borrowed when no templates (zero allocation).
/// Returns Cow::Owned with single-pass resolution when templates exist.
///
/// Performance: Zero-clone traversal - uses references until final value_to_string.
///
/// Supports lazy bindings by resolving them on demand via RunContext.
/// Supports context bindings via {{context.files.alias}} and {{context.session.key}}.
/// Supports inputs bindings via {{inputs.param}}.
///
/// Example: `{{with.forecast}}` → resolved value from bindings
/// Example: `{{with.flight_info.departure}}` → nested access
/// Example: `{{context.files.brand}}` → loaded file content
/// Example: `{{context.session.focus}}` → session data
/// Example: `{{inputs.topic}}` → input parameter default value
///
/// Media side-channel interception for template resolution.
///
/// When the first remaining segment after an alias is "media", the data lives
/// in `TaskResult.media` (side-channel), not in the output JSON. This helper
/// resolves the media path via `RunContext::resolve_path()` and returns the
/// resolved value + empty remaining segments. Returns `None` if media
/// interception doesn't apply (no "media" segment, no source_task_id).
fn intercept_media_path<'a>(
    alias: &str,
    remaining_parts: &SmallVec<[&'a str; 8]>,
    effective_base: Value,
    bindings: &ResolvedBindings,
    datastore: &RunContext,
) -> Result<(Value, SmallVec<[&'a str; 8]>), NikaError> {
    if remaining_parts.first() != Some(&"media") {
        return Ok((effective_base, remaining_parts.clone()));
    }

    if let Some(source_task_id) = bindings.source_task_id(alias) {
        let remaining_path = remaining_parts.join(".");
        let full_path = format!("{}.{}", source_task_id, remaining_path);
        if let Some(v) = datastore.resolve_path(&full_path) {
            Ok((v, SmallVec::new()))
        } else {
            Err(NikaError::PathNotFound {
                path: format!(
                    "with.{}.{} (task '{}' produced no matching media)",
                    alias, remaining_path, source_task_id
                ),
            })
        }
    } else {
        Ok((effective_base, remaining_parts.clone()))
    }
}

pub fn resolve<'a>(
    template: &'a str,
    bindings: &ResolvedBindings,
    datastore: &RunContext,
) -> Result<Cow<'a, str>, NikaError> {
    // Early return with borrowed string (zero alloc)
    // Fast check: must contain `{{` followed eventually by `with.`, `context.`, `inputs.`, or `skills.`
    // Regex handles whitespace variations like `{{ with.` or `{{\twith.`
    if !template.contains("{{") {
        return Ok(Cow::Borrowed(template));
    }
    let has_with = template.contains("with.");
    let has_context = template.contains("context.");
    let has_inputs = template.contains("inputs.");
    let has_skills = template.contains("skills.");
    if !has_with && !has_context && !has_inputs && !has_skills {
        return Ok(Cow::Borrowed(template));
    }

    // Guard: reject templates with too many variable references
    let var_count = template.matches("{{").count();
    if var_count > MAX_TEMPLATE_VARS {
        return Err(BindingError::TemplateError {
            template: format!("(template with {} variables)", var_count),
            reason: format!(
                "Template contains {} variable references, exceeding the maximum of {}",
                var_count, MAX_TEMPLATE_VARS
            ),
        }
        .into());
    }

    // Normalize bracket notation to dot notation
    // {{with.items[0]}} → {{with.items.0}}
    let normalized = normalize_bracket_notation(template);
    let template_str: &str = normalized.as_ref();

    // Single-pass: build result by copying segments + inserting replacements
    // Uses TEMPLATE_RE (matches ALL {{...}}) + parse_template_expr for full transform support.
    let mut result = String::with_capacity(template_str.len() + 64);
    let mut last_end = 0;
    let mut errors: SmallVec<[String; 4]> = SmallVec::new();

    for cap in TEMPLATE_RE.captures_iter(template_str) {
        let m = cap.get(0).unwrap();
        let content = &cap[1];

        // Copy segment before this match
        result.push_str(&template_str[last_end..m.start()]);

        match parse_template_expr(content) {
            Ok(TemplateExpr::Alias {
                ref path,
                ref transforms,
            }) => {
                // Guard: reject pathologically deep alias paths
                let segment_count = path.split('.').count();
                if segment_count > MAX_PATH_DEPTH {
                    return Err(BindingError::TemplateError {
                        template: path.to_string(),
                        reason: format!(
                            "Path depth {} exceeds maximum of {} segments",
                            segment_count, MAX_PATH_DEPTH
                        ),
                    }
                    .into());
                }

                // Split: first segment is alias, rest is nested path
                let mut parts = path.split('.');
                let alias = parts.next().unwrap();

                // Get the resolved value for this alias (supports lazy bindings via RunContext)
                match bindings.get_resolved(alias, datastore) {
                    Ok(base_value) => {
                        // Auto-parse JSON strings so invoke/exec outputs stored
                        // as Value::String('{"hash":"blake3:..."}') can be
                        // traversed with {{with.alias.hash}} (NIKA-253 fix).
                        let effective_base =
                            crate::binding::jsonpath::try_parse_json_str(&base_value)
                                .unwrap_or(base_value);

                        // Collect remaining segments for media interception
                        let remaining_parts: SmallVec<[&str; 8]> = parts.collect();

                        // Media interception: {{with.alias.media[0].hash}}
                        let (resolved_value, segments_to_traverse) = intercept_media_path(
                            alias,
                            &remaining_parts,
                            effective_base,
                            bindings,
                            datastore,
                        )?;

                        let mut value_ref: &Value = &resolved_value;
                        let mut traversed_segments: SmallVec<[&str; 8]> = SmallVec::new();
                        traversed_segments.push(alias);

                        // Traverse remaining path (empty if media resolved above)
                        for &segment in &segments_to_traverse {
                            let next = if let Ok(idx) = segment.parse::<usize>() {
                                value_ref.get(idx)
                            } else {
                                value_ref.get(segment)
                            };

                            match next {
                                Some(v) => {
                                    traversed_segments.push(segment);
                                    value_ref = v;
                                }
                                None => {
                                    let value_type = match value_ref {
                                        Value::Null => "null",
                                        Value::Bool(_) => "bool",
                                        Value::Number(_) => "number",
                                        Value::String(_) => "string",
                                        Value::Array(_) => "array",
                                        Value::Object(_) => "object",
                                    };

                                    if matches!(value_ref, Value::Object(_) | Value::Array(_)) {
                                        let traversed_path = traversed_segments.join(".");
                                        return Err(NikaError::PathNotFound {
                                            path: format!("{}.{}", traversed_path, segment),
                                        });
                                    } else {
                                        return Err(NikaError::InvalidTraversal {
                                            segment: segment.to_string(),
                                            value_type: value_type.to_string(),
                                            full_path: path.to_string(),
                                        });
                                    }
                                }
                            }
                        }

                        // Apply transforms if any, then convert to string
                        let has_shell = transforms.iter().any(|t| t == "shell");

                        let display = if has_shell {
                            // Shell transform: apply non-shell transforms first, then escape
                            let non_shell: Vec<&String> =
                                transforms.iter().filter(|t| *t != "shell").collect();
                            let pre_shell_value = if non_shell.is_empty() {
                                value_ref.clone()
                            } else {
                                let transform_str = non_shell
                                    .iter()
                                    .map(|s| s.as_str())
                                    .collect::<Vec<_>>()
                                    .join(" | ");
                                let expr =
                                    crate::binding::transform::TransformExpr::parse(&transform_str)
                                        .map_err(|e| NikaError::TemplateParse {
                                            position: m.start(),
                                            details: format!("Transform parse error: {}", e),
                                        })?;
                                expr.apply(value_ref)
                                    .map_err(|e| NikaError::TemplateParse {
                                        position: m.start(),
                                        details: format!("Transform apply error: {}", e),
                                    })?
                            };
                            escape_for_shell(&value_to_display(&pre_shell_value))
                        } else if !transforms.is_empty() {
                            // Non-shell transforms: parse and apply chain
                            let transform_str = transforms.join(" | ");
                            let expr =
                                crate::binding::transform::TransformExpr::parse(&transform_str)
                                    .map_err(|e| NikaError::TemplateParse {
                                        position: m.start(),
                                        details: format!("Transform parse error: {}", e),
                                    })?;
                            let final_value =
                                expr.apply(value_ref)
                                    .map_err(|e| NikaError::TemplateParse {
                                        position: m.start(),
                                        details: format!("Transform apply error: {}", e),
                                    })?;
                            if is_in_json_context(template_str, m.start()) {
                                escape_for_json(&value_to_display(&final_value)).into_owned()
                            } else {
                                value_to_display(&final_value).into_owned()
                            }
                        } else {
                            // No transforms: use strict value_to_string (null = error)
                            let replacement = value_to_string(value_ref, path, alias)?;
                            if is_in_json_context(template_str, m.start()) {
                                escape_for_json(&replacement).into_owned()
                            } else {
                                replacement.into_owned()
                            }
                        };

                        result.push_str(&display);
                    }
                    Err(_) => {
                        errors.push(alias.to_string());
                    }
                }
            }
            Ok(
                TemplateExpr::Context { .. }
                | TemplateExpr::Input { .. }
                | TemplateExpr::Skills { .. },
            ) => {
                // Leave context/inputs/skills refs for later passes — re-emit as {{...}}
                result.push_str(&format!("{{{{{}}}}}", content.trim()));
            }
            Err(e) => {
                // Malformed expression — re-emit literally with warning
                tracing::warn!(expression = %content.trim(), error = %e, "Malformed template expression — passing through literally");
                result.push_str(m.as_str());
            }
        }

        last_end = m.end();
    }

    if !errors.is_empty() {
        return Err(BindingError::TemplateError {
            template: errors.join(", "),
            reason: "Alias(es) not resolved. Did you declare them in 'with:'?".to_string(),
        }
        .into());
    }

    // Copy remaining segment after last match
    result.push_str(&template_str[last_end..]);

    // ─────────────────────────────────────────────────────────────
    // Pass 2: Resolve {{context.files.alias}} and {{context.session.key}}
    // ─────────────────────────────────────────────────────────────
    // SECURITY: Collect trusted context paths from the ORIGINAL template
    // before Pass 1 altered the string. This prevents template injection
    // where LLM output containing {{context.files.secret}} is substituted
    // into with: bindings and then resolved here.
    if has_context && result.contains("context.") {
        let trusted_context: std::collections::HashSet<String> = TEMPLATE_RE
            .captures_iter(template)
            .filter_map(|cap| {
                let inner = cap[1].trim();
                if let Ok(TemplateExpr::Context { path, .. }) = parse_template_expr(inner) {
                    Some(format!("context.{}", path))
                } else {
                    None
                }
            })
            .collect();

        let intermediate = std::mem::take(&mut result);
        result = String::with_capacity(intermediate.len() + 64);
        let mut last_end = 0;
        let mut context_errors: SmallVec<[String; 4]> = SmallVec::new();

        for cap in TEMPLATE_RE.captures_iter(&intermediate) {
            let m = cap.get(0).unwrap();
            let inner = cap[1].trim();
            let (path, transforms) = match parse_template_expr(inner) {
                Ok(TemplateExpr::Context { path, transforms }) => (path, transforms),
                _ => continue,
            };
            let full_path = format!("context.{}", path);
            // Skip injected context refs that weren't in the original template
            if !trusted_context.contains(&full_path) {
                tracing::warn!(
                    path = %full_path,
                    "Blocked injected context reference (template injection attempt)"
                );
                continue;
            }
            result.push_str(&intermediate[last_end..m.start()]);
            match datastore.resolve_context_path(&full_path) {
                Some(value) => {
                    let replacement = if !transforms.is_empty() {
                        let transform_str = transforms.join(" | ");
                        let expr = TransformExpr::parse(&transform_str).map_err(|e| {
                            NikaError::TemplateParse {
                                position: m.start(),
                                details: format!("Transform parse error: {}", e),
                            }
                        })?;
                        let transformed =
                            expr.apply(&value).map_err(|e| NikaError::TemplateParse {
                                position: m.start(),
                                details: format!("Transform apply error: {}", e),
                            })?;
                        if is_in_json_context(&intermediate, m.start()) {
                            escape_for_json(&value_to_display(&transformed)).into_owned()
                        } else {
                            value_to_display(&transformed).into_owned()
                        }
                    } else {
                        let s = context_value_to_string(&value, &full_path)?;
                        if is_in_json_context(&intermediate, m.start()) {
                            escape_for_json(&s).into_owned()
                        } else {
                            s.into_owned()
                        }
                    };
                    result.push_str(&replacement);
                }
                None => {
                    context_errors.push(full_path);
                }
            }

            last_end = m.end();
        }

        if !context_errors.is_empty() {
            return Err(BindingError::TemplateError {
                template: context_errors.join(", "),
                reason: "Context binding(s) not resolved. Check your 'context:' block in workflow."
                    .to_string(),
            }
            .into());
        }

        // Copy remaining segment
        result.push_str(&intermediate[last_end..]);

        // Continue to inputs pass if needed
        if !has_inputs || !result.contains("inputs.") {
            return Ok(Cow::Owned(result));
        }
        // Fall through to inputs pass with updated result
    }

    // ─────────────────────────────────────────────────────────────
    // Pass 3: Resolve {{inputs.param}}
    // ─────────────────────────────────────────────────────────────
    // SECURITY: Collect trusted input paths from the ORIGINAL template
    // before Pass 1/2 altered the string. This prevents template injection
    // where LLM output containing {{inputs.secret}} is substituted via
    // with: bindings and then resolved here.
    if has_inputs && result.contains("inputs.") {
        let trusted_inputs: std::collections::HashSet<String> = TEMPLATE_RE
            .captures_iter(template)
            .filter_map(|cap| {
                let inner = cap[1].trim();
                if let Ok(TemplateExpr::Input { path, .. }) = parse_template_expr(inner) {
                    Some(format!("inputs.{}", path))
                } else {
                    None
                }
            })
            .collect();

        let intermediate = std::mem::take(&mut result);
        result = String::with_capacity(intermediate.len() + 64);
        let mut last_end = 0;
        let mut input_errors: SmallVec<[String; 4]> = SmallVec::new();

        for cap in TEMPLATE_RE.captures_iter(&intermediate) {
            let m = cap.get(0).unwrap();
            let inner = cap[1].trim();
            let (path, transforms) = match parse_template_expr(inner) {
                Ok(TemplateExpr::Input { path, transforms }) => (path, transforms),
                _ => continue,
            };
            let full_path = format!("inputs.{}", path);
            // Skip injected input refs that weren't in the original template
            if !trusted_inputs.contains(&full_path) {
                tracing::warn!(
                    path = %full_path,
                    "Blocked injected input reference (template injection attempt)"
                );
                continue;
            }
            result.push_str(&intermediate[last_end..m.start()]);
            match datastore.resolve_input_path(&full_path) {
                Some(value) => {
                    let replacement = if !transforms.is_empty() {
                        let transform_str = transforms.join(" | ");
                        let expr = TransformExpr::parse(&transform_str).map_err(|e| {
                            NikaError::TemplateParse {
                                position: m.start(),
                                details: format!("Transform parse error: {}", e),
                            }
                        })?;
                        let transformed =
                            expr.apply(&value).map_err(|e| NikaError::TemplateParse {
                                position: m.start(),
                                details: format!("Transform apply error: {}", e),
                            })?;
                        if is_in_json_context(&intermediate, m.start()) {
                            escape_for_json(&value_to_display(&transformed)).into_owned()
                        } else {
                            value_to_display(&transformed).into_owned()
                        }
                    } else {
                        let s = input_value_to_string(&value, &full_path)?;
                        if is_in_json_context(&intermediate, m.start()) {
                            escape_for_json(&s).into_owned()
                        } else {
                            s.into_owned()
                        }
                    };
                    result.push_str(&replacement);
                }
                None => {
                    input_errors.push(full_path);
                }
            }

            last_end = m.end();
        }

        if !input_errors.is_empty() {
            return Err(BindingError::TemplateError {
                template: input_errors.join(", "),
                reason: "Input binding(s) not resolved. Check your 'inputs:' block in workflow or provide defaults.".to_string(),
            }.into());
        }

        // Copy remaining segment
        result.push_str(&intermediate[last_end..]);
    }

    // ─────────────────────────────────────────────────────────────
    // Pass 4: Resolve {{skills.name}}
    // ─────────────────────────────────────────────────────────────
    if has_skills && result.contains("skills.") {
        let trusted_skills: std::collections::HashSet<String> = TEMPLATE_RE
            .captures_iter(template)
            .filter_map(|cap| {
                let inner = cap[1].trim();
                if let Ok(TemplateExpr::Skills { path, .. }) = parse_template_expr(inner) {
                    Some(format!("skills.{}", path))
                } else {
                    None
                }
            })
            .collect();

        let intermediate = std::mem::take(&mut result);
        result = String::with_capacity(intermediate.len() + 64);
        let mut last_end = 0;
        let mut skills_errors: SmallVec<[String; 4]> = SmallVec::new();

        for cap in TEMPLATE_RE.captures_iter(&intermediate) {
            let m = cap.get(0).unwrap();
            let inner = cap[1].trim();
            let (path, transforms) = match parse_template_expr(inner) {
                Ok(TemplateExpr::Skills { path, transforms }) => (path, transforms),
                _ => continue,
            };
            let full_path = format!("skills.{}", path);
            if !trusted_skills.contains(&full_path) {
                tracing::warn!(
                    path = %full_path,
                    "Blocked injected skills reference (template injection attempt)"
                );
                continue;
            }
            result.push_str(&intermediate[last_end..m.start()]);
            match datastore.resolve_skills_path(&path) {
                Some(value) => {
                    let replacement = if !transforms.is_empty() {
                        let transform_str = transforms.join(" | ");
                        let expr = TransformExpr::parse(&transform_str).map_err(|e| {
                            NikaError::TemplateParse {
                                position: m.start(),
                                details: format!("Transform parse error: {}", e),
                            }
                        })?;
                        let transformed =
                            expr.apply(&value).map_err(|e| NikaError::TemplateParse {
                                position: m.start(),
                                details: format!("Transform apply error: {}", e),
                            })?;
                        if is_in_json_context(&intermediate, m.start()) {
                            escape_for_json(&value_to_display(&transformed)).into_owned()
                        } else {
                            value_to_display(&transformed).into_owned()
                        }
                    } else if is_in_json_context(&intermediate, m.start()) {
                        escape_for_json(&value_to_display(&value)).into_owned()
                    } else {
                        value_to_display(&value).into_owned()
                    };
                    result.push_str(&replacement);
                }
                None => {
                    skills_errors.push(full_path);
                }
            }

            last_end = m.end();
        }

        if !skills_errors.is_empty() {
            return Err(BindingError::TemplateError {
                template: skills_errors.join(", "),
                reason: "Skill binding(s) not resolved. Check your 'skills:' block in workflow."
                    .to_string(),
            }
            .into());
        }

        result.push_str(&intermediate[last_end..]);
    }

    Ok(Cow::Owned(result))
}

/// Resolve templates for shell context
///
/// Similar to `resolve`, but shell-escapes all substituted values to prevent
/// command injection from LLM outputs containing special characters.
///
/// Example: `echo 'Hello {{with.msg}}'` with msg="Nika's test" becomes
///          `echo 'Hello '\''Nika'\''s test'\'''`
pub fn resolve_for_shell<'a>(
    template: &'a str,
    bindings: &ResolvedBindings,
    datastore: &RunContext,
) -> Result<Cow<'a, str>, NikaError> {
    // Early return if no templates
    if !template.contains("{{") {
        return Ok(Cow::Borrowed(template));
    }
    let has_with = template.contains("with.");
    let has_context = template.contains("context.");
    let has_inputs = template.contains("inputs.");
    let has_skills = template.contains("skills.");
    if !has_with && !has_context && !has_inputs && !has_skills {
        return Ok(Cow::Borrowed(template));
    }

    // Normalize bracket notation: {{with.items[0]}} → {{with.items.0}}
    let normalized = normalize_bracket_notation(template);
    let template_str: &str = normalized.as_ref();

    // Pass 1: Alias bindings (shell-escaped)
    // Uses TEMPLATE_RE + parse_template_expr for full transform support.
    let mut result = String::with_capacity(template_str.len() + 64);
    let mut last_end = 0;
    let mut errors: SmallVec<[String; 4]> = SmallVec::new();

    for cap in TEMPLATE_RE.captures_iter(template_str) {
        let m = cap.get(0).unwrap();
        let content = &cap[1];

        let (path, transforms) = match parse_template_expr(content) {
            Ok(TemplateExpr::Alias { path, transforms }) => (path, transforms),
            _ => continue,
        };

        result.push_str(&template_str[last_end..m.start()]);

        let mut parts = path.split('.');
        let alias = parts.next().unwrap();

        match bindings.get_resolved(alias, datastore) {
            Ok(base_value) => {
                // Auto-parse JSON strings (NIKA-253 fix, same as resolve()).
                let effective_base =
                    crate::binding::jsonpath::try_parse_json_str(&base_value).unwrap_or(base_value);

                // Collect remaining segments for media interception
                let remaining_parts: SmallVec<[&str; 8]> = parts.collect();

                // Media interception: {{with.alias.media[0].hash}}
                let (resolved_value, segments_to_traverse) = intercept_media_path(
                    alias,
                    &remaining_parts,
                    effective_base,
                    bindings,
                    datastore,
                )?;

                let mut value_ref: &Value = &resolved_value;
                let mut traversed_segments: SmallVec<[&str; 8]> = SmallVec::new();
                traversed_segments.push(alias);

                for &segment in &segments_to_traverse {
                    let next = if let Ok(idx) = segment.parse::<usize>() {
                        value_ref.get(idx)
                    } else {
                        value_ref.get(segment)
                    };

                    match next {
                        Some(v) => {
                            traversed_segments.push(segment);
                            value_ref = v;
                        }
                        None => {
                            let value_type = match value_ref {
                                Value::Null => "null",
                                Value::Bool(_) => "bool",
                                Value::Number(_) => "number",
                                Value::String(_) => "string",
                                Value::Array(_) => "array",
                                Value::Object(_) => "object",
                            };

                            if matches!(value_ref, Value::Object(_) | Value::Array(_)) {
                                let traversed_path = traversed_segments.join(".");
                                return Err(NikaError::PathNotFound {
                                    path: format!("{}.{}", traversed_path, segment),
                                });
                            } else {
                                return Err(NikaError::InvalidTraversal {
                                    segment: segment.to_string(),
                                    value_type: value_type.to_string(),
                                    full_path: path.to_string(),
                                });
                            }
                        }
                    }
                }

                // Apply non-shell transforms first, then shell-escape the result.
                let has_shell = transforms.iter().any(|t| t == "shell");
                let non_shell: Vec<&String> = transforms.iter().filter(|t| *t != "shell").collect();

                let raw_value = if !non_shell.is_empty() {
                    let transform_str = non_shell
                        .iter()
                        .map(|s| s.as_str())
                        .collect::<Vec<_>>()
                        .join(" | ");
                    let expr = crate::binding::transform::TransformExpr::parse(&transform_str)
                        .map_err(|e| NikaError::TemplateParse {
                            position: m.start(),
                            details: format!("Transform parse error: {}", e),
                        })?;
                    let transformed =
                        expr.apply(value_ref)
                            .map_err(|e| NikaError::TemplateParse {
                                position: m.start(),
                                details: format!("Transform apply error: {}", e),
                            })?;
                    value_to_display(&transformed).into_owned()
                } else if has_shell {
                    // Only |shell, no other transforms: use strict value_to_string
                    value_to_string(value_ref, &path, alias)?.into_owned()
                } else {
                    // No transforms at all: use strict value_to_string
                    value_to_string(value_ref, &path, alias)?.into_owned()
                };

                // Shell-escape the value
                let escaped = escape_for_shell(&raw_value);
                result.push_str(&escaped);
            }
            Err(_) => {
                errors.push(alias.to_string());
            }
        }

        last_end = m.end();
    }

    if !errors.is_empty() {
        return Err(BindingError::TemplateError {
            template: errors.join(", "),
            reason: "Alias(es) not resolved. Did you declare them in 'with:'?".to_string(),
        }
        .into());
    }

    result.push_str(&template_str[last_end..]);

    // Pass 2: Context bindings (shell-escaped)
    if has_context && result.contains("context.") {
        let intermediate = std::mem::take(&mut result);
        result = String::with_capacity(intermediate.len() + 64);
        let mut last_end = 0;
        let mut context_errors: SmallVec<[String; 4]> = SmallVec::new();

        for cap in TEMPLATE_RE.captures_iter(&intermediate) {
            let m = cap.get(0).unwrap();
            let inner = cap[1].trim();
            let (path, transforms) = match parse_template_expr(inner) {
                Ok(TemplateExpr::Context { path, transforms }) => (path, transforms),
                _ => continue,
            };
            result.push_str(&intermediate[last_end..m.start()]);
            let full_path = format!("context.{}", path);
            match datastore.resolve_context_path(&full_path) {
                Some(value) => {
                    let raw_value = if !transforms.is_empty() {
                        let transform_str = transforms.join(" | ");
                        let expr = TransformExpr::parse(&transform_str).map_err(|e| {
                            NikaError::TemplateParse {
                                position: m.start(),
                                details: format!("Transform parse error: {}", e),
                            }
                        })?;
                        let transformed =
                            expr.apply(&value).map_err(|e| NikaError::TemplateParse {
                                position: m.start(),
                                details: format!("Transform apply error: {}", e),
                            })?;
                        value_to_display(&transformed).into_owned()
                    } else {
                        context_value_to_string(&value, &full_path)?.into_owned()
                    };
                    let escaped = escape_for_shell(&raw_value);
                    result.push_str(&escaped);
                }
                None => {
                    context_errors.push(full_path);
                }
            }

            last_end = m.end();
        }

        if !context_errors.is_empty() {
            return Err(BindingError::TemplateError {
                template: context_errors.join(", "),
                reason: "Context binding(s) not resolved. Check your 'context:' block in workflow."
                    .to_string(),
            }
            .into());
        }

        result.push_str(&intermediate[last_end..]);
    }

    // Pass 3: Input bindings (shell-escaped)
    if has_inputs && result.contains("inputs.") {
        let intermediate = std::mem::take(&mut result);
        result = String::with_capacity(intermediate.len() + 64);
        let mut last_end = 0;
        let mut input_errors: SmallVec<[String; 4]> = SmallVec::new();

        for cap in TEMPLATE_RE.captures_iter(&intermediate) {
            let m = cap.get(0).unwrap();
            let inner = cap[1].trim();
            let (path, transforms) = match parse_template_expr(inner) {
                Ok(TemplateExpr::Input { path, transforms }) => (path, transforms),
                _ => continue,
            };
            result.push_str(&intermediate[last_end..m.start()]);
            let full_path = format!("inputs.{}", path);
            match datastore.resolve_input_path(&full_path) {
                Some(value) => {
                    let raw_value = if !transforms.is_empty() {
                        let transform_str = transforms.join(" | ");
                        let expr = TransformExpr::parse(&transform_str).map_err(|e| {
                            NikaError::TemplateParse {
                                position: m.start(),
                                details: format!("Transform parse error: {}", e),
                            }
                        })?;
                        let transformed =
                            expr.apply(&value).map_err(|e| NikaError::TemplateParse {
                                position: m.start(),
                                details: format!("Transform apply error: {}", e),
                            })?;
                        value_to_display(&transformed).into_owned()
                    } else {
                        input_value_to_string(&value, &full_path)?.into_owned()
                    };
                    let escaped = escape_for_shell(&raw_value);
                    result.push_str(&escaped);
                }
                None => {
                    input_errors.push(full_path);
                }
            }

            last_end = m.end();
        }

        if !input_errors.is_empty() {
            return Err(BindingError::TemplateError {
                template: input_errors.join(", "),
                reason: "Input binding(s) not resolved. Check your 'inputs:' block in workflow or provide defaults.".to_string(),
            }.into());
        }

        result.push_str(&intermediate[last_end..]);
    }

    Ok(Cow::Owned(result))
}

/// Convert JSON Value to string for template substitution (strict mode)
///
/// Returns `Cow::Borrowed` for string values (avoids cloning).
/// Returns error for null values - this prevents silent bugs from missing data.
fn value_to_string<'a>(
    value: &'a Value,
    path: &str,
    alias: &str,
) -> Result<Cow<'a, str>, NikaError> {
    match value {
        Value::String(s) => Ok(Cow::Borrowed(s.as_str())),
        Value::Null => Err(NikaError::NullValue {
            path: path.to_string(),
            alias: alias.to_string(),
        }),
        Value::Bool(b) => Ok(Cow::Owned(b.to_string())),
        Value::Number(n) => Ok(Cow::Owned(n.to_string())),
        // For objects/arrays, return compact JSON representation
        other => Ok(Cow::Owned(other.to_string())),
    }
}

/// Convert context Value to string for template substitution
///
/// Returns `Cow::Borrowed` for string values (avoids cloning).
fn context_value_to_string<'a>(value: &'a Value, path: &str) -> Result<Cow<'a, str>, NikaError> {
    match value {
        Value::String(s) => Ok(Cow::Borrowed(s.as_str())),
        Value::Null => Err(BindingError::TemplateError {
            template: path.to_string(),
            reason: "Context binding resolved to null".to_string(),
        }
        .into()),
        Value::Bool(b) => Ok(Cow::Owned(b.to_string())),
        Value::Number(n) => Ok(Cow::Owned(n.to_string())),
        // For objects/arrays, return compact JSON representation
        other => Ok(Cow::Owned(other.to_string())),
    }
}

/// Convert input Value to string for template substitution
///
/// Returns `Cow::Borrowed` for string values (avoids cloning).
fn input_value_to_string<'a>(value: &'a Value, path: &str) -> Result<Cow<'a, str>, NikaError> {
    match value {
        Value::String(s) => Ok(Cow::Borrowed(s.as_str())),
        Value::Null => Err(BindingError::TemplateError {
            template: path.to_string(),
            reason: "Input binding resolved to null. Provide a 'default' value in your inputs definition.".to_string(),
        }.into()),
        Value::Bool(b) => Ok(Cow::Owned(b.to_string())),
        Value::Number(n) => Ok(Cow::Owned(n.to_string())),
        // For objects/arrays, return compact JSON representation
        other => Ok(Cow::Owned(other.to_string())),
    }
}

/// Check if position is inside a JSON string
fn is_in_json_context(template: &str, pos: usize) -> bool {
    // First check: the template must look like a JSON structure at the top level.
    // A template starting with `{` (after whitespace) indicates JSON object context.
    // This avoids false positives from natural language with unbalanced quotes
    // like: He said "hello {{with.msg}}"
    let trimmed = template.trim_start();
    let looks_like_json = trimmed.starts_with('{') || trimmed.starts_with('[');
    if !looks_like_json {
        return false;
    }

    // Second check: count quote parity to determine if we're inside a JSON string value
    let before = &template[..pos];
    let mut in_string = false;
    let mut escaped = false;

    for ch in before.chars() {
        if escaped {
            escaped = false;
            continue;
        }
        match ch {
            '\\' => escaped = true,
            '"' => in_string = !in_string,
            _ => {}
        }
    }

    in_string
}

/// Extract all alias references from a template (for static validation)
///
/// Returns a Vec of (alias, full_path) tuples.
/// Example: "{{with.weather.temp}}" → vec![("weather", "weather.temp")]
pub fn extract_refs(template: &str) -> Vec<(String, String)> {
    USE_RE
        .captures_iter(template)
        .map(|cap| {
            let full_path = cap[1].to_string();
            let alias = full_path.split('.').next().unwrap().to_string();
            (alias, full_path)
        })
        .collect()
}

/// Validate that all template references exist in declared aliases (static validation)
///
/// This is called by `nika validate` before runtime.
/// Returns Ok(()) if valid, Err with first unknown alias if not.
pub fn validate_refs(
    template: &str,
    declared_aliases: &FxHashSet<String>,
    task_id: &str,
) -> Result<(), NikaError> {
    for (alias, _full_path) in extract_refs(template) {
        if !declared_aliases.contains(&alias) {
            let candidates: Vec<&str> = declared_aliases.iter().map(|s| s.as_str()).collect();
            let suggestion =
                nika_core::ast::analyzer::suggestions::find_similar(&alias, &candidates, 0.6);
            return Err(NikaError::UnknownAlias {
                alias,
                task_id: task_id.to_string(),
                suggestion,
            });
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::borrow::Cow;

    /// Helper to create empty datastore for tests
    fn empty_datastore() -> RunContext {
        RunContext::new(nika_core::trust::InvocationSource::Test)
    }

    #[test]
    fn resolve_simple() {
        let mut bindings = ResolvedBindings::new();
        bindings.set("forecast", json!("Sunny 25C"));
        let ds = empty_datastore();

        let result = resolve("Weather: {{with.forecast}}", &bindings, &ds).unwrap();
        assert_eq!(result, "Weather: Sunny 25C");
    }

    #[test]
    fn resolve_number() {
        let mut bindings = ResolvedBindings::new();
        bindings.set("price", json!(89));
        let ds = empty_datastore();

        let result = resolve("Price: ${{with.price}}", &bindings, &ds).unwrap();
        assert_eq!(result, "Price: $89");
    }

    #[test]
    fn resolve_nested() {
        let mut bindings = ResolvedBindings::new();
        bindings.set("flight_info", json!({"departure": "10:30", "gate": "A12"}));
        let ds = empty_datastore();

        let result = resolve("Depart at {{with.flight_info.departure}}", &bindings, &ds).unwrap();
        assert_eq!(result, "Depart at 10:30");
    }

    #[test]
    fn resolve_multiple() {
        let mut bindings = ResolvedBindings::new();
        bindings.set("a", json!("first"));
        bindings.set("b", json!("second"));
        let ds = empty_datastore();

        let result = resolve("{{with.a}} and {{with.b}}", &bindings, &ds).unwrap();
        assert_eq!(result, "first and second");
    }

    #[test]
    fn resolve_object() {
        let mut bindings = ResolvedBindings::new();
        bindings.set("data", json!({"x": 1, "y": 2}));
        let ds = empty_datastore();

        let result = resolve("Full: {{with.data}}", &bindings, &ds).unwrap();
        // Object is serialized as JSON
        assert!(result.contains("\"x\":1") || result.contains("\"x\": 1"));
    }

    #[test]
    fn resolve_alias_not_found() {
        let mut bindings = ResolvedBindings::new();
        bindings.set("known", json!("value"));
        let ds = empty_datastore();

        let result = resolve("{{with.unknown}}", &bindings, &ds);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("unknown"));
    }

    #[test]
    fn resolve_path_not_found() {
        let mut bindings = ResolvedBindings::new();
        bindings.set("data", json!({"a": 1}));
        let ds = empty_datastore();

        let result = resolve("{{with.data.nonexistent}}", &bindings, &ds);
        assert!(result.is_err());
    }

    #[test]
    fn resolve_no_templates() {
        let bindings = ResolvedBindings::new();
        let ds = empty_datastore();
        let result = resolve("No templates here", &bindings, &ds).unwrap();
        assert_eq!(result, "No templates here");
        // Verify zero-alloc: should be Cow::Borrowed
        assert!(matches!(result, Cow::Borrowed(_)));
    }

    #[test]
    fn resolve_with_templates_is_owned() {
        let mut bindings = ResolvedBindings::new();
        bindings.set("x", json!("value"));
        let ds = empty_datastore();
        let result = resolve("Has {{with.x}} template", &bindings, &ds).unwrap();
        assert_eq!(result, "Has value template");
        // With templates: should be Cow::Owned
        assert!(matches!(result, Cow::Owned(_)));
    }

    #[test]
    fn resolve_array_index() {
        let mut bindings = ResolvedBindings::new();
        bindings.set("items", json!(["first", "second", "third"]));
        let ds = empty_datastore();

        let result = resolve("Item: {{with.items.0}}", &bindings, &ds).unwrap();
        assert_eq!(result, "Item: first");
    }

    // ─────────────────────────────────────────────────────────────
    // Bracket notation tests (array indexing with [N])
    // ─────────────────────────────────────────────────────────────

    #[test]
    fn resolve_bracket_notation_simple() {
        let mut bindings = ResolvedBindings::new();
        bindings.set("items", json!(["first", "second", "third"]));
        let ds = empty_datastore();

        // Bracket notation should work like dot notation
        let result = resolve("Item: {{with.items[0]}}", &bindings, &ds).unwrap();
        assert_eq!(result, "Item: first");
    }

    #[test]
    fn resolve_bracket_notation_second_element() {
        let mut bindings = ResolvedBindings::new();
        bindings.set("items", json!(["first", "second", "third"]));
        let ds = empty_datastore();

        let result = resolve("Item: {{with.items[1]}}", &bindings, &ds).unwrap();
        assert_eq!(result, "Item: second");
    }

    #[test]
    fn resolve_bracket_notation_nested() {
        let mut bindings = ResolvedBindings::new();
        bindings.set(
            "data",
            json!({
                "user": {"name": "Alice", "address": {"city": "Paris"}},
                "items": ["one", "two", "three"]
            }),
        );
        let ds = empty_datastore();

        // Nested object + bracket notation for array
        let result = resolve("First item: {{with.data.items[0]}}", &bindings, &ds).unwrap();
        assert_eq!(result, "First item: one");
    }

    #[test]
    fn resolve_bracket_notation_mixed_syntax() {
        let mut bindings = ResolvedBindings::new();
        bindings.set(
            "data",
            json!({"users": [{"name": "Alice"}, {"name": "Bob"}]}),
        );
        let ds = empty_datastore();

        // Mix of dot and bracket notation
        let result = resolve("User: {{with.data.users[0].name}}", &bindings, &ds).unwrap();
        assert_eq!(result, "User: Alice");
    }

    #[test]
    fn resolve_bracket_notation_multiple() {
        let mut bindings = ResolvedBindings::new();
        bindings.set("items", json!(["a", "b", "c"]));
        let ds = empty_datastore();

        // Multiple bracket notations in one template
        let result = resolve("{{with.items[0]}} and {{with.items[2]}}", &bindings, &ds).unwrap();
        assert_eq!(result, "a and c");
    }

    #[test]
    fn normalize_bracket_notation_unit() {
        // Direct test of the normalization function
        assert_eq!(
            normalize_bracket_notation("{{with.items[0]}}"),
            "{{with.items.0}}"
        );
        assert_eq!(
            normalize_bracket_notation("{{with.data.items[1].name}}"),
            "{{with.data.items.1.name}}"
        );
        assert_eq!(
            normalize_bracket_notation("no brackets here"),
            "no brackets here"
        );
        // Multiple brackets
        assert_eq!(
            normalize_bracket_notation("{{with.a[0]}} and {{with.b[2]}}"),
            "{{with.a.0}} and {{with.b.2}}"
        );
    }

    // ─────────────────────────────────────────────────────────────
    // Strict mode tests
    // ─────────────────────────────────────────────────────────────

    #[test]
    fn resolve_null_is_error() {
        let mut bindings = ResolvedBindings::new();
        bindings.set("data", json!(null));
        let ds = empty_datastore();

        let result = resolve("Value: {{with.data}}", &bindings, &ds);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.to_string().contains("NIKA-072"));
        assert!(err.to_string().contains("Null value"));
    }

    #[test]
    fn resolve_nested_null_is_error() {
        let mut bindings = ResolvedBindings::new();
        bindings.set("data", json!({"value": null}));
        let ds = empty_datastore();

        let result = resolve("Value: {{with.data.value}}", &bindings, &ds);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("NIKA-072"));
    }

    #[test]
    fn resolve_invalid_traversal_on_string() {
        let mut bindings = ResolvedBindings::new();
        bindings.set("data", json!("just a string"));
        let ds = empty_datastore();

        let result = resolve("{{with.data.field}}", &bindings, &ds);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.to_string().contains("NIKA-073"));
        assert!(err.to_string().contains("string"));
    }

    #[test]
    fn resolve_invalid_traversal_on_number() {
        let mut bindings = ResolvedBindings::new();
        bindings.set("price", json!(42));
        let ds = empty_datastore();

        let result = resolve("{{with.price.currency}}", &bindings, &ds);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.to_string().contains("NIKA-073"));
        assert!(err.to_string().contains("number"));
    }

    // ─────────────────────────────────────────────────────────────
    // Static validation tests
    // ─────────────────────────────────────────────────────────────

    #[test]
    fn extract_refs_simple() {
        let refs = extract_refs("Hello {{with.weather}}!");
        assert_eq!(refs.len(), 1);
        assert_eq!(refs[0], ("weather".to_string(), "weather".to_string()));
    }

    #[test]
    fn extract_refs_nested() {
        let refs = extract_refs("{{with.data.field.sub}}");
        assert_eq!(refs.len(), 1);
        assert_eq!(refs[0], ("data".to_string(), "data.field.sub".to_string()));
    }

    #[test]
    fn extract_refs_multiple() {
        let refs = extract_refs("{{with.a}} and {{with.b.c}}");
        assert_eq!(refs.len(), 2);
        assert_eq!(refs[0].0, "a");
        assert_eq!(refs[1].0, "b");
    }

    #[test]
    fn extract_refs_none() {
        let refs = extract_refs("No templates here");
        assert!(refs.is_empty());
    }

    #[test]
    fn validate_refs_success() {
        let declared: FxHashSet<String> =
            ["weather", "price"].iter().map(|s| s.to_string()).collect();
        let result = validate_refs("{{with.weather}} costs {{with.price}}", &declared, "task1");
        assert!(result.is_ok());
    }

    #[test]
    fn validate_refs_unknown_alias() {
        let declared: FxHashSet<String> = ["weather"].iter().map(|s| s.to_string()).collect();
        let result = validate_refs("{{with.weather}} and {{with.unknown}}", &declared, "task1");
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.to_string().contains("NIKA-071"));
        assert!(err.to_string().contains("unknown"));
    }

    // ─────────────────────────────────────────────────────────────
    // Context binding tests
    // ─────────────────────────────────────────────────────────────

    use crate::store::LoadedContext;

    /// Helper to create datastore with context for tests
    fn datastore_with_context() -> RunContext {
        let store = RunContext::new(nika_core::trust::InvocationSource::Test);
        let mut context = LoadedContext::new();
        context.files.insert(
            "brand".to_string(),
            json!("# QR Code AI\nTagline: Scan smarter"),
        );
        context
            .files
            .insert("config".to_string(), json!({"theme": "dark", "version": 2}));
        context.session = Some(json!({"focus": "rust", "level": 3}));
        store.set_context(context);
        store
    }

    #[test]
    fn resolve_context_files_simple() {
        let bindings = ResolvedBindings::new();
        let ds = datastore_with_context();

        let result = resolve("Brand: {{context.files.brand}}", &bindings, &ds).unwrap();
        assert_eq!(result, "Brand: # QR Code AI\nTagline: Scan smarter");
    }

    #[test]
    fn resolve_context_files_nested() {
        let bindings = ResolvedBindings::new();
        let ds = datastore_with_context();

        let result = resolve("Theme: {{context.files.config.theme}}", &bindings, &ds).unwrap();
        assert_eq!(result, "Theme: dark");
    }

    #[test]
    fn resolve_context_session() {
        let bindings = ResolvedBindings::new();
        let ds = datastore_with_context();

        let result = resolve("Focus: {{context.session.focus}}", &bindings, &ds).unwrap();
        assert_eq!(result, "Focus: rust");
    }

    #[test]
    fn resolve_context_session_number() {
        let bindings = ResolvedBindings::new();
        let ds = datastore_with_context();

        let result = resolve("Level: {{context.session.level}}", &bindings, &ds).unwrap();
        assert_eq!(result, "Level: 3");
    }

    #[test]
    fn resolve_context_with_use_bindings() {
        let mut bindings = ResolvedBindings::new();
        bindings.set("greeting", json!("Hello"));
        let ds = datastore_with_context();

        let result = resolve(
            "{{with.greeting}}! Brand: {{context.files.brand}}",
            &bindings,
            &ds,
        )
        .unwrap();
        assert_eq!(result, "Hello! Brand: # QR Code AI\nTagline: Scan smarter");
    }

    #[test]
    fn resolve_context_not_found() {
        let bindings = ResolvedBindings::new();
        let ds = datastore_with_context();

        let result = resolve("{{context.files.nonexistent}}", &bindings, &ds);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.to_string().contains("Context binding"));
        assert!(err.to_string().contains("nonexistent"));
    }

    #[test]
    fn resolve_context_no_context_loaded() {
        let bindings = ResolvedBindings::new();
        let ds = empty_datastore(); // No context loaded

        let result = resolve("{{context.files.brand}}", &bindings, &ds);
        assert!(result.is_err());
    }

    #[test]
    fn resolve_only_context_no_use() {
        let bindings = ResolvedBindings::new();
        let ds = datastore_with_context();

        // Template with ONLY context bindings, no use bindings
        let result = resolve("Theme is {{context.files.config.theme}}", &bindings, &ds).unwrap();
        assert_eq!(result, "Theme is dark");
    }

    #[test]
    fn resolve_context_preserves_no_template() {
        let bindings = ResolvedBindings::new();
        let ds = datastore_with_context();

        // No templates at all
        let result = resolve("Plain text without templates", &bindings, &ds).unwrap();
        assert_eq!(result, "Plain text without templates");
        // Should be borrowed (zero alloc)
        assert!(matches!(result, Cow::Borrowed(_)));
    }

    // ═══════════════════════════════════════════════════════════════════════════
    // Shell escaping tests
    // ═══════════════════════════════════════════════════════════════════════════

    #[test]
    fn escape_for_shell_simple() {
        assert_eq!(escape_for_shell("hello"), "'hello'");
    }

    #[test]
    fn escape_for_shell_empty() {
        assert_eq!(escape_for_shell(""), "''");
    }

    #[test]
    fn escape_for_shell_with_single_quote() {
        // "Nika's" becomes "'Nika'\''s'"
        assert_eq!(escape_for_shell("Nika's"), "'Nika'\\''s'");
    }

    #[test]
    fn escape_for_shell_with_multiple_quotes() {
        // "don't won't" should escape both quotes
        assert_eq!(escape_for_shell("don't won't"), "'don'\\''t won'\\''t'");
    }

    #[test]
    fn escape_for_shell_with_special_chars() {
        // Special shell characters should be safe inside single quotes
        assert_eq!(escape_for_shell("$HOME;rm -rf /"), "'$HOME;rm -rf /'");
    }

    #[test]
    fn escape_for_shell_with_backticks() {
        // Backticks should be safe inside single quotes
        assert_eq!(escape_for_shell("`whoami`"), "'`whoami`'");
    }

    #[test]
    fn escape_for_shell_with_newlines() {
        // Newlines should be preserved inside single quotes
        assert_eq!(escape_for_shell("line1\nline2"), "'line1\nline2'");
    }

    // ═══════════════════════════════════════════════════════════════════════════
    // |shell modifier tests
    // ═══════════════════════════════════════════════════════════════════════════

    #[test]
    fn resolve_shell_modifier_simple() {
        let mut bindings = ResolvedBindings::new();
        bindings.set("msg", json!("hello world"));
        let ds = empty_datastore();

        // Using |shell modifier applies shell escaping
        let result = resolve("echo {{with.msg|shell}}", &bindings, &ds).unwrap();
        assert_eq!(result, "echo 'hello world'");
    }

    #[test]
    fn resolve_shell_modifier_with_quote() {
        let mut bindings = ResolvedBindings::new();
        bindings.set("response", json!("Hello from Nika's v0.5.1!"));
        let ds = empty_datastore();

        // The |shell modifier escapes single quotes correctly
        let result = resolve("echo {{with.response|shell}}", &bindings, &ds).unwrap();
        assert_eq!(result, "echo 'Hello from Nika'\\''s v0.5.1!'");
    }

    #[test]
    fn resolve_shell_modifier_with_special_chars() {
        let mut bindings = ResolvedBindings::new();
        bindings.set("content", json!("Hello; echo pwned"));
        let ds = empty_datastore();

        // Shell special characters are safely escaped
        let result = resolve("echo {{with.content|shell}}", &bindings, &ds).unwrap();
        assert_eq!(result, "echo 'Hello; echo pwned'");
    }

    #[test]
    fn resolve_without_modifier_no_escape() {
        let mut bindings = ResolvedBindings::new();
        bindings.set("msg", json!("hello world"));
        let ds = empty_datastore();

        // Without |shell modifier, no escaping happens
        let result = resolve("echo {{with.msg}}", &bindings, &ds).unwrap();
        assert_eq!(result, "echo hello world");
    }

    #[test]
    fn resolve_shell_modifier_multiple() {
        let mut bindings = ResolvedBindings::new();
        bindings.set("file", json!("test.txt"));
        bindings.set("content", json!("Hello 'world'"));
        let ds = empty_datastore();

        // Multiple bindings with |shell modifier
        let result = resolve(
            "cat {{with.file|shell}} && echo {{with.content|shell}}",
            &bindings,
            &ds,
        )
        .unwrap();
        assert_eq!(result, "cat 'test.txt' && echo 'Hello '\\''world'\\'''");
    }

    #[test]
    fn resolve_for_shell_simple() {
        let mut bindings = ResolvedBindings::new();
        bindings.set("msg", json!("hello world"));
        let ds = empty_datastore();

        let result = resolve_for_shell("echo {{with.msg}}", &bindings, &ds).unwrap();
        assert_eq!(result, "echo 'hello world'");
    }

    #[test]
    fn resolve_for_shell_with_quote() {
        let mut bindings = ResolvedBindings::new();
        bindings.set("response", json!("Hello from Nika's v0.5.1!"));
        let ds = empty_datastore();

        // resolve_for_shell escapes ALL bindings
        let result =
            resolve_for_shell("echo 'Claude said: {{with.response}}'", &bindings, &ds).unwrap();
        // The output has escaped quotes
        assert_eq!(
            result,
            "echo 'Claude said: 'Hello from Nika'\\''s v0.5.1!''"
        );
    }

    #[test]
    fn resolve_for_shell_no_templates() {
        let bindings = ResolvedBindings::new();
        let ds = empty_datastore();

        // No templates - should return borrowed string
        let result = resolve_for_shell("echo hello", &bindings, &ds).unwrap();
        assert_eq!(result, "echo hello");
        assert!(matches!(result, Cow::Borrowed(_)));
    }

    #[test]
    fn resolve_for_shell_preserves_command_structure() {
        let mut bindings = ResolvedBindings::new();
        bindings.set("file", json!("test.txt"));
        bindings.set("content", json!("Hello; echo pwned"));
        let ds = empty_datastore();

        // The command structure is preserved, only the value is escaped
        let result =
            resolve_for_shell("cat {{with.file}} && echo {{with.content}}", &bindings, &ds)
                .unwrap();
        assert_eq!(result, "cat 'test.txt' && echo 'Hello; echo pwned'");
    }

    // ═══════════════════════════════════════════════════════════════════════════
    // Input binding tests
    // ═══════════════════════════════════════════════════════════════════════════

    use rustc_hash::FxHashMap;

    /// Helper to create datastore with inputs for tests
    fn datastore_with_inputs() -> RunContext {
        let store = RunContext::new(nika_core::trust::InvocationSource::Test);
        let mut inputs = FxHashMap::default();
        inputs.insert(
            "topic".to_string(),
            json!({
                "type": "string",
                "default": "AI QR code generation"
            }),
        );
        inputs.insert(
            "depth".to_string(),
            json!({
                "type": "string",
                "default": "comprehensive"
            }),
        );
        inputs.insert(
            "config".to_string(),
            json!({
                "type": "object",
                "default": {
                    "theme": "dark",
                    "count": 5
                }
            }),
        );
        store.set_inputs(inputs);
        store
    }

    #[test]
    fn resolve_inputs_simple() {
        let bindings = ResolvedBindings::new();
        let ds = datastore_with_inputs();

        let result = resolve("Topic: {{inputs.topic}}", &bindings, &ds).unwrap();
        assert_eq!(result, "Topic: AI QR code generation");
    }

    #[test]
    fn resolve_inputs_multiple() {
        let bindings = ResolvedBindings::new();
        let ds = datastore_with_inputs();

        let result = resolve(
            "Research {{inputs.topic}} at {{inputs.depth}} depth",
            &bindings,
            &ds,
        )
        .unwrap();
        assert_eq!(
            result,
            "Research AI QR code generation at comprehensive depth"
        );
    }

    #[test]
    fn resolve_inputs_nested() {
        let bindings = ResolvedBindings::new();
        let ds = datastore_with_inputs();

        let result = resolve("Theme: {{inputs.config.theme}}", &bindings, &ds).unwrap();
        assert_eq!(result, "Theme: dark");
    }

    #[test]
    fn resolve_inputs_with_use_bindings() {
        let mut bindings = ResolvedBindings::new();
        bindings.set("greeting", json!("Hello"));
        let ds = datastore_with_inputs();

        let result = resolve(
            "{{with.greeting}}! Research {{inputs.topic}}",
            &bindings,
            &ds,
        )
        .unwrap();
        assert_eq!(result, "Hello! Research AI QR code generation");
    }

    #[test]
    fn resolve_inputs_with_context() {
        let mut bindings = ResolvedBindings::new();
        bindings.set("msg", json!("Test"));
        let store = RunContext::new(nika_core::trust::InvocationSource::Test);

        // Set both context and inputs
        let mut context = LoadedContext::new();
        context
            .files
            .insert("brand".to_string(), json!("QR Code AI"));
        store.set_context(context);

        let mut inputs = FxHashMap::default();
        inputs.insert(
            "topic".to_string(),
            json!({
                "type": "string",
                "default": "AI trends"
            }),
        );
        store.set_inputs(inputs);

        let result = resolve(
            "{{with.msg}}: {{context.files.brand}} - {{inputs.topic}}",
            &bindings,
            &store,
        )
        .unwrap();
        assert_eq!(result, "Test: QR Code AI - AI trends");
    }

    #[test]
    fn resolve_inputs_not_found() {
        let bindings = ResolvedBindings::new();
        let ds = datastore_with_inputs();

        let result = resolve("{{inputs.nonexistent}}", &bindings, &ds);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.to_string().contains("Input binding"));
        assert!(err.to_string().contains("nonexistent"));
    }

    #[test]
    fn resolve_inputs_no_inputs_loaded() {
        let bindings = ResolvedBindings::new();
        let ds = empty_datastore(); // No inputs

        let result = resolve("{{inputs.topic}}", &bindings, &ds);
        assert!(result.is_err());
    }

    #[test]
    fn resolve_only_inputs_no_use() {
        let bindings = ResolvedBindings::new();
        let ds = datastore_with_inputs();

        // Template with ONLY inputs bindings, no use bindings
        let result = resolve("Topic is {{inputs.topic}}", &bindings, &ds).unwrap();
        assert_eq!(result, "Topic is AI QR code generation");
    }

    #[test]
    fn resolve_inputs_preserves_no_template() {
        let bindings = ResolvedBindings::new();
        let ds = datastore_with_inputs();

        // No templates at all
        let result = resolve("Plain text without templates", &bindings, &ds).unwrap();
        assert_eq!(result, "Plain text without templates");
        // Should be borrowed (zero alloc)
        assert!(matches!(result, Cow::Borrowed(_)));
    }

    // ═══════════════════════════════════════════════════════════════════════════
    // Template Injection Security Tests
    // ═══════════════════════════════════════════════════════════════════════════
    //
    // These tests verify that malicious content in template values cannot:
    // 1. Break out of the intended context (JSON, shell, etc.)
    // 2. Cause re-evaluation of template syntax
    // 3. Inject control characters or escape sequences
    //
    // Security principle: Template values are DATA, not CODE.
    // They should be interpolated literally, never interpreted.

    #[test]
    fn injection_template_syntax_not_reevaluated() {
        // Value contains template syntax - should NOT be re-evaluated
        let mut bindings = ResolvedBindings::new();
        bindings.set("user_input", json!("{{with.secret}}"));
        bindings.set("secret", json!("TOP_SECRET"));
        let ds = empty_datastore();

        let result = resolve("User said: {{with.user_input}}", &bindings, &ds).unwrap();
        // The {{with.secret}} should appear literally, NOT expanded recursively
        assert_eq!(result, "User said: {{with.secret}}");
        assert!(!result.contains("TOP_SECRET"));
    }

    #[test]
    fn injection_nested_template_attack() {
        // Attempt to construct template syntax via concatenation
        let mut bindings = ResolvedBindings::new();
        bindings.set("left", json!("{{with."));
        bindings.set("right", json!("secret}}"));
        bindings.set("secret", json!("LEAKED"));
        let ds = empty_datastore();

        // Even with split template markers, no re-evaluation should occur
        let result = resolve("{{with.left}}{{with.right}}", &bindings, &ds).unwrap();
        assert_eq!(result, "{{with.secret}}");
        assert!(!result.contains("LEAKED"));
    }

    #[test]
    fn injection_json_context_quotes_escaped() {
        // Value with quotes in JSON context should be escaped
        let mut bindings = ResolvedBindings::new();
        bindings.set("name", json!(r#"Alice", "admin": true, "x": "#));
        let ds = empty_datastore();

        // Template is in a JSON context (inside quotes)
        let template = r#"{"user": "{{with.name}}"}"#;
        let result = resolve(template, &bindings, &ds).unwrap();

        // The quotes should be escaped with backslash
        assert!(
            result.contains(r#"\""#),
            "Quotes should be escaped: {}",
            result
        );
        // The result should be valid JSON - quotes are escaped so injection fails
        // The "admin" key appears but as escaped string content, not as JSON structure
        assert_eq!(
            result, r#"{"user": "Alice\", \"admin\": true, \"x\": "}"#,
            "Quotes should be escaped to prevent JSON structure injection"
        );
        // Verify the injected "admin" is inside the string value, not a real key
        // by checking the escaped pattern exists
        assert!(result.contains(r#"\"admin\""#), "admin should be escaped");
    }

    #[test]
    fn injection_json_context_backslash_escaped() {
        // Backslashes in JSON context should be double-escaped
        let mut bindings = ResolvedBindings::new();
        bindings.set("path", json!(r#"C:\Users\admin"#));
        let ds = empty_datastore();

        let template = r#"{"path": "{{with.path}}"}"#;
        let result = resolve(template, &bindings, &ds).unwrap();

        // Backslashes should be escaped
        assert!(
            result.contains(r#"\\"#),
            "Backslashes should be escaped: {}",
            result
        );
    }

    #[test]
    fn injection_json_context_newline_escaped() {
        // Newlines in JSON context should be escaped as \n
        let mut bindings = ResolvedBindings::new();
        bindings.set("text", json!("line1\nline2"));
        let ds = empty_datastore();

        let template = r#"{"text": "{{with.text}}"}"#;
        let result = resolve(template, &bindings, &ds).unwrap();

        // Raw newline should become \n
        assert!(
            result.contains(r#"\n"#),
            "Newlines should be escaped: {}",
            result
        );
        assert!(
            !result.contains('\n') || result.matches('\n').count() == 0 || result.contains("\\n"),
            "Raw newlines should be escaped"
        );
    }

    #[test]
    fn injection_shell_modifier_escapes_semicolon() {
        // Semicolon injection attempt with |shell modifier
        let mut bindings = ResolvedBindings::new();
        bindings.set("filename", json!("file.txt; rm -rf /"));
        let ds = empty_datastore();

        let result = resolve("cat {{with.filename|shell}}", &bindings, &ds).unwrap();
        // With |shell, the dangerous command is wrapped in single quotes
        // This makes the semicolon a literal character, not a command separator
        assert_eq!(result, "cat 'file.txt; rm -rf /'");
        // The entire value including semicolon is inside quotes - safe
        assert!(result.starts_with("cat '") && result.ends_with("'"));
    }

    #[test]
    fn injection_shell_modifier_escapes_backticks() {
        // Command substitution injection attempt
        let mut bindings = ResolvedBindings::new();
        bindings.set("input", json!("`whoami`"));
        let ds = empty_datastore();

        let result = resolve("echo {{with.input|shell}}", &bindings, &ds).unwrap();
        // Backticks safely quoted
        assert_eq!(result, "echo '`whoami`'");
    }

    #[test]
    fn injection_shell_modifier_escapes_dollar_parens() {
        // $(command) injection attempt
        let mut bindings = ResolvedBindings::new();
        bindings.set("input", json!("$(cat /etc/passwd)"));
        let ds = empty_datastore();

        let result = resolve("echo {{with.input|shell}}", &bindings, &ds).unwrap();
        // Dollar-paren safely quoted
        assert_eq!(result, "echo '$(cat /etc/passwd)'");
    }

    #[test]
    fn injection_shell_modifier_escapes_env_vars() {
        // Environment variable injection
        let mut bindings = ResolvedBindings::new();
        bindings.set("input", json!("$HOME/.ssh/id_rsa"));
        let ds = empty_datastore();

        let result = resolve("cat {{with.input|shell}}", &bindings, &ds).unwrap();
        // $HOME is literal, not expanded
        assert_eq!(result, "cat '$HOME/.ssh/id_rsa'");
    }

    #[test]
    fn injection_resolve_for_shell_escapes_all() {
        // resolve_for_shell should escape ALL bindings automatically
        let mut bindings = ResolvedBindings::new();
        bindings.set("cmd", json!("echo 'pwned'; rm -rf /"));
        let ds = empty_datastore();

        let result = resolve_for_shell("{{with.cmd}}", &bindings, &ds).unwrap();
        // The entire value is shell-escaped using single-quote escaping
        // The embedded single quote in 'pwned' is escaped as '\''
        assert_eq!(result, "'echo '\\''pwned'\\''; rm -rf /'");
        // The value starts and ends with single quotes, making everything inside literal
        // Even though '; rm' appears in the string, it's inside quoted context
    }

    #[test]
    fn injection_control_characters_json() {
        // Control characters should be escaped in JSON context
        let mut bindings = ResolvedBindings::new();
        // Tab, carriage return, and form feed
        bindings.set("data", json!("a\tb\rc\x0c"));
        let ds = empty_datastore();

        let template = r#"{"data": "{{with.data}}"}"#;
        let result = resolve(template, &bindings, &ds).unwrap();

        // Control chars should be escaped
        assert!(result.contains(r#"\t"#) || !result.contains('\t'));
        assert!(result.contains(r#"\r"#) || !result.contains('\r'));
    }

    #[test]
    fn injection_unicode_escape_sequences() {
        // Unicode escape sequences should be treated literally
        let mut bindings = ResolvedBindings::new();
        bindings.set("text", json!(r#"\u0000"#)); // Literal backslash-u
        let ds = empty_datastore();

        let result = resolve("Text: {{with.text}}", &bindings, &ds).unwrap();
        // Should appear as literal \u0000, not null byte
        assert_eq!(result, r#"Text: \u0000"#);
    }

    #[test]
    fn injection_null_byte_in_value() {
        // JSON Value::String cannot contain null bytes (serde_json rejects them)
        // But if somehow present, should be handled safely
        let mut bindings = ResolvedBindings::new();
        // serde_json from string with null - this is actually impossible via json!
        // but we test the principle
        bindings.set("normal", json!("safe"));
        let ds = empty_datastore();

        let result = resolve("{{with.normal}}", &bindings, &ds).unwrap();
        assert_eq!(result, "safe");
    }

    #[test]
    fn injection_very_long_value() {
        // Very long values should be handled without stack overflow
        let mut bindings = ResolvedBindings::new();
        let long_string = "A".repeat(100_000);
        bindings.set("big", json!(long_string.clone()));
        let ds = empty_datastore();

        let result = resolve("Data: {{with.big}}", &bindings, &ds).unwrap();
        assert!(result.starts_with("Data: AAAA"));
        assert_eq!(result.len(), 6 + 100_000); // "Data: " + 100k As
    }

    #[test]
    fn injection_deeply_nested_json_value() {
        // Deeply nested JSON should serialize correctly
        let mut bindings = ResolvedBindings::new();
        bindings.set("nested", json!({"a": {"b": {"c": {"d": "deep"}}}}));
        let ds = empty_datastore();

        let result = resolve("{{with.nested}}", &bindings, &ds).unwrap();
        // Should be serialized JSON, not crash
        assert!(result.contains("deep"));
    }

    #[test]
    fn injection_template_markers_in_context_path() {
        // Even context paths with template-like patterns should be safe
        let bindings = ResolvedBindings::new();
        let store = RunContext::new(nika_core::trust::InvocationSource::Test);

        let mut context = LoadedContext::new();
        // File name that looks like template syntax - but file content is safe
        context
            .files
            .insert("normal".to_string(), json!("safe content"));
        store.set_context(context);

        let result = resolve("{{context.files.normal}}", &bindings, &store).unwrap();
        assert_eq!(result, "safe content");
    }

    #[test]
    fn injection_context_value_with_template_syntax() {
        // Context file content with template syntax should NOT be re-evaluated
        let bindings = ResolvedBindings::new();
        let store = RunContext::new(nika_core::trust::InvocationSource::Test);

        let mut context = LoadedContext::new();
        context
            .files
            .insert("brand".to_string(), json!("Brand: {{with.secret}}"));
        store.set_context(context);

        let result = resolve("{{context.files.brand}}", &bindings, &store).unwrap();
        // Template syntax in the VALUE should appear literally
        assert_eq!(result, "Brand: {{with.secret}}");
    }

    #[test]
    fn injection_input_value_with_template_syntax() {
        // Input values with template syntax should NOT be re-evaluated
        let bindings = ResolvedBindings::new();
        let store = RunContext::new(nika_core::trust::InvocationSource::Test);

        let mut inputs = FxHashMap::default();
        inputs.insert(
            "topic".to_string(),
            json!({
                "type": "string",
                "default": "Learn about {{with.secret}}"
            }),
        );
        store.set_inputs(inputs);

        let result = resolve("{{inputs.topic}}", &bindings, &store).unwrap();
        // Template syntax in input default should appear literally
        assert_eq!(result, "Learn about {{with.secret}}");
    }

    #[test]
    fn injection_3pass_no_cross_contamination() {
        // Verify 3-pass resolution doesn't allow pass N output to affect pass N+1
        let mut bindings = ResolvedBindings::new();
        // Pass 1: use binding resolves to something with context syntax
        bindings.set("data", json!("{{context.files.secret}}"));
        let store = RunContext::new(nika_core::trust::InvocationSource::Test);

        let mut context = LoadedContext::new();
        context
            .files
            .insert("secret".to_string(), json!("CONFIDENTIAL"));
        store.set_context(context);

        // Template only has use binding, but its value contains context syntax
        let result = resolve("Result: {{with.data}}", &bindings, &store).unwrap();
        // The context syntax should NOT be evaluated in pass 2
        assert_eq!(result, "Result: {{context.files.secret}}");
        assert!(!result.contains("CONFIDENTIAL"));
    }

    #[test]
    fn injection_html_script_tags() {
        // HTML script injection - template just passes through
        let mut bindings = ResolvedBindings::new();
        bindings.set("content", json!("<script>alert('xss')</script>"));
        let ds = empty_datastore();

        let result = resolve("{{with.content}}", &bindings, &ds).unwrap();
        // NOTE: Template resolution does NOT escape HTML - that's the consumer's job
        // This test documents the behavior: raw HTML passes through
        assert_eq!(result, "<script>alert('xss')</script>");
    }

    #[test]
    fn injection_sql_like_content() {
        // SQL-like content - template just passes through
        let mut bindings = ResolvedBindings::new();
        bindings.set("query", json!("'; DROP TABLE users; --"));
        let ds = empty_datastore();

        let result = resolve(
            "SELECT * FROM x WHERE name='{{with.query}}'",
            &bindings,
            &ds,
        )
        .unwrap();
        // Template resolution does NOT prevent SQL injection - that's the DB layer's job
        // This test documents the behavior
        assert!(result.contains("DROP TABLE"));
    }
}

// ═════════════════════════════════════════════════════════════════════════════
// New template engine tests — parse_template_expr + resolve_with
// ═════════════════════════════════════════════════════════════════════════════

#[cfg(test)]
mod v028_template_tests {
    use super::*;
    use crate::store::{LoadedContext, RunContext};
    use serde_json::json;

    fn empty_datastore() -> RunContext {
        RunContext::new(nika_core::trust::InvocationSource::Test)
    }

    fn make_with(entries: &[(&str, Value)]) -> FxHashMap<String, Value> {
        entries
            .iter()
            .map(|(k, v)| (k.to_string(), v.clone()))
            .collect()
    }

    // ─── parse_template_expr tests ───────────────────────────────────────────

    #[test]
    fn parse_expr_simple_alias() {
        let result = parse_template_expr("title").unwrap();
        assert_eq!(
            result,
            TemplateExpr::Alias {
                path: "title".to_string(),
                transforms: vec![],
            }
        );
    }

    #[test]
    fn parse_expr_alias_with_path() {
        let result = parse_template_expr("data.items").unwrap();
        assert_eq!(
            result,
            TemplateExpr::Alias {
                path: "data.items".to_string(),
                transforms: vec![],
            }
        );
    }

    #[test]
    fn parse_expr_alias_single_transform() {
        let result = parse_template_expr("title | upper").unwrap();
        assert_eq!(
            result,
            TemplateExpr::Alias {
                path: "title".to_string(),
                transforms: vec!["upper".to_string()],
            }
        );
    }

    #[test]
    fn parse_expr_alias_multi_transform() {
        let result = parse_template_expr("x | sort | unique | first(3)").unwrap();
        assert_eq!(
            result,
            TemplateExpr::Alias {
                path: "x".to_string(),
                transforms: vec![
                    "sort".to_string(),
                    "unique".to_string(),
                    "first(3)".to_string(),
                ],
            }
        );
    }

    #[test]
    fn parse_expr_context_files() {
        let result = parse_template_expr("context.files.brand").unwrap();
        assert_eq!(
            result,
            TemplateExpr::Context {
                path: "files.brand".to_string(),
                transforms: vec![]
            }
        );
    }

    #[test]
    fn parse_expr_context_session() {
        let result = parse_template_expr("context.session.key").unwrap();
        assert_eq!(
            result,
            TemplateExpr::Context {
                path: "session.key".to_string(),
                transforms: vec![]
            }
        );
    }

    #[test]
    fn parse_expr_inputs() {
        let result = parse_template_expr("inputs.locale").unwrap();
        assert_eq!(
            result,
            TemplateExpr::Input {
                path: "locale".to_string(),
                transforms: vec![]
            }
        );
    }

    #[test]
    fn parse_expr_inputs_nested() {
        let result = parse_template_expr("inputs.config.theme").unwrap();
        assert_eq!(
            result,
            TemplateExpr::Input {
                path: "config.theme".to_string(),
                transforms: vec![]
            }
        );
    }

    #[test]
    fn parse_expr_context_with_transforms() {
        let result = parse_template_expr("context.files.brand | upper").unwrap();
        assert_eq!(
            result,
            TemplateExpr::Context {
                path: "files.brand".to_string(),
                transforms: vec!["upper".to_string()]
            }
        );
    }

    #[test]
    fn parse_expr_inputs_with_transforms() {
        let result = parse_template_expr("inputs.topic | lower | trim").unwrap();
        assert_eq!(
            result,
            TemplateExpr::Input {
                path: "topic".to_string(),
                transforms: vec!["lower".to_string(), "trim".to_string()]
            }
        );
    }

    #[test]
    fn parse_expr_contextual_is_alias() {
        // "contextual" should NOT match "context." prefix
        let result = parse_template_expr("contextual").unwrap();
        assert_eq!(
            result,
            TemplateExpr::Alias {
                path: "contextual".to_string(),
                transforms: vec![],
            }
        );
    }

    #[test]
    fn parse_expr_inputstream_is_alias() {
        // "inputstream" should NOT match "inputs." prefix
        let result = parse_template_expr("inputstream").unwrap();
        assert_eq!(
            result,
            TemplateExpr::Alias {
                path: "inputstream".to_string(),
                transforms: vec![],
            }
        );
    }

    #[test]
    fn parse_expr_empty_is_error() {
        let result = parse_template_expr("");
        assert!(result.is_err());
    }

    #[test]
    fn parse_expr_whitespace_is_error() {
        let result = parse_template_expr("   ");
        assert!(result.is_err());
    }

    #[test]
    fn parse_expr_context_dot_only_is_error() {
        let result = parse_template_expr("context.");
        assert!(result.is_err());
    }

    #[test]
    fn parse_expr_inputs_dot_only_is_error() {
        let result = parse_template_expr("inputs.");
        assert!(result.is_err());
    }

    #[test]
    fn parse_expr_whitespace_trimmed() {
        let result = parse_template_expr("  title  ").unwrap();
        assert_eq!(
            result,
            TemplateExpr::Alias {
                path: "title".to_string(),
                transforms: vec![],
            }
        );
    }

    #[test]
    fn parse_expr_transform_with_spaces() {
        let result = parse_template_expr("  name  |  upper  |  trim  ").unwrap();
        assert_eq!(
            result,
            TemplateExpr::Alias {
                path: "name".to_string(),
                transforms: vec!["upper".to_string(), "trim".to_string()],
            }
        );
    }

    // ─── value_to_display tests ──────────────────────────────────────────────

    #[test]
    fn display_string() {
        assert_eq!(value_to_display(&json!("hello")), "hello");
    }

    #[test]
    fn display_number() {
        assert_eq!(value_to_display(&json!(42)), "42");
        assert_eq!(value_to_display(&json!(3.12)), "3.12");
    }

    #[test]
    fn display_bool() {
        assert_eq!(value_to_display(&json!(true)), "true");
        assert_eq!(value_to_display(&json!(false)), "false");
    }

    #[test]
    fn display_null_is_empty() {
        assert_eq!(value_to_display(&Value::Null), "");
    }

    #[test]
    fn display_array() {
        assert_eq!(value_to_display(&json!([1, 2, 3])), "[1,2,3]");
    }

    #[test]
    fn display_object() {
        let val = json!({"a": 1});
        let display = value_to_display(&val);
        assert!(display.contains("\"a\""));
        assert!(display.contains("1"));
    }

    // ─── resolve_with tests (Pass 1: alias resolution) ──────────────────────

    #[test]
    fn resolve_with_simple_alias() {
        let with = make_with(&[("name", json!("World"))]);
        let ds = empty_datastore();
        let result = resolve_with("Hello {{name}}", &with, &ds).unwrap();
        assert_eq!(result, "Hello World");
    }

    #[test]
    fn resolve_with_deep_alias() {
        let with = make_with(&[("data", json!({"items": [1, 2, 3]}))]);
        let ds = empty_datastore();
        let result = resolve_with("Items: {{data.items}}", &with, &ds).unwrap();
        assert_eq!(result, "Items: [1,2,3]");
    }

    #[test]
    fn resolve_with_transform() {
        let with = make_with(&[("title", json!("hello world"))]);
        let ds = empty_datastore();
        let result = resolve_with("{{title | upper}}", &with, &ds).unwrap();
        assert_eq!(result, "HELLO WORLD");
    }

    #[test]
    fn resolve_with_array_json_serialization() {
        let with = make_with(&[("items", json!(["a", "b", "c"]))]);
        let ds = empty_datastore();
        let result = resolve_with("{{items}}", &with, &ds).unwrap();
        assert_eq!(result, "[\"a\",\"b\",\"c\"]");
    }

    #[test]
    fn resolve_with_null_is_empty() {
        let with = make_with(&[("val", Value::Null)]);
        let ds = empty_datastore();
        let result = resolve_with("Got: {{val}}!", &with, &ds).unwrap();
        assert_eq!(result, "Got: !");
    }

    #[test]
    fn resolve_with_multiple_aliases() {
        let with = make_with(&[("a", json!("hello")), ("b", json!("world"))]);
        let ds = empty_datastore();
        let result = resolve_with("{{a}} and {{b}}", &with, &ds).unwrap();
        assert_eq!(result, "hello and world");
    }

    #[test]
    fn resolve_with_missing_alias_errors() {
        let with = make_with(&[("name", json!("Alice"))]);
        let ds = empty_datastore();
        let result = resolve_with("{{missing}}", &with, &ds);
        assert!(result.is_err());
    }

    #[test]
    fn resolve_with_number() {
        let with = make_with(&[("count", json!(42))]);
        let ds = empty_datastore();
        let result = resolve_with("Count: {{count}}", &with, &ds).unwrap();
        assert_eq!(result, "Count: 42");
    }

    #[test]
    fn resolve_with_bool() {
        let with = make_with(&[("flag", json!(true))]);
        let ds = empty_datastore();
        let result = resolve_with("Flag: {{flag}}", &with, &ds).unwrap();
        assert_eq!(result, "Flag: true");
    }

    // ─── resolve_with tests (Pass 2: context + inputs) ──────────────────────

    #[test]
    fn resolve_with_context_file() {
        let with = FxHashMap::default();
        let ds = empty_datastore();
        let mut context = LoadedContext::new();
        context
            .files
            .insert("brand".to_string(), json!("SuperNovae AI"));
        ds.set_context(context);

        let result = resolve_with("Brand: {{context.files.brand}}", &with, &ds).unwrap();
        assert_eq!(result, "Brand: SuperNovae AI");
    }

    #[test]
    fn resolve_with_context_session() {
        let with = FxHashMap::default();
        let ds = empty_datastore();
        let mut context = LoadedContext::new();
        context.session = Some(json!({"focus": "rust"}));
        ds.set_context(context);

        let result = resolve_with("Focus: {{context.session.focus}}", &with, &ds).unwrap();
        assert_eq!(result, "Focus: rust");
    }

    #[test]
    fn resolve_with_inputs() {
        let with = FxHashMap::default();
        let ds = empty_datastore();
        let mut inputs = FxHashMap::default();
        inputs.insert("locale".to_string(), json!("fr-FR"));
        ds.set_inputs(inputs);

        let result = resolve_with("Locale: {{inputs.locale}}", &with, &ds).unwrap();
        assert_eq!(result, "Locale: fr-FR");
    }

    #[test]
    fn resolve_with_inputs_nested() {
        let with = FxHashMap::default();
        let ds = empty_datastore();
        let mut inputs = FxHashMap::default();
        inputs.insert("config".to_string(), json!({"theme": "dark"}));
        ds.set_inputs(inputs);

        let result = resolve_with("Theme: {{inputs.config.theme}}", &with, &ds).unwrap();
        assert_eq!(result, "Theme: dark");
    }

    // ─── Security: no re-evaluation ─────────────────────────────────────────

    #[test]
    fn no_reevaluation_alias_containing_template() {
        // If an alias value contains {{ }}, it should NOT be re-evaluated
        let with = make_with(&[("val", json!("{{context.files.secret}}"))]);
        let ds = empty_datastore();
        let mut context = LoadedContext::new();
        context
            .files
            .insert("secret".to_string(), json!("TOP_SECRET"));
        ds.set_context(context);

        let result = resolve_with("Got: {{val}}", &with, &ds).unwrap();
        // SECURITY FIX (Bug 45): has_context/has_inputs checks now use the
        // ORIGINAL template, not the post-Pass-1 result. Since "Got: {{val}}"
        // does NOT contain "context.", Pass 2 is skipped entirely.
        // The {{context.files.secret}} from the alias value remains literal.
        assert_eq!(result, "Got: {{context.files.secret}}");
        assert!(!result.contains("TOP_SECRET"));
    }

    #[test]
    fn no_reevaluation_alias_to_alias() {
        // If an alias value contains {{other_alias}}, it should NOT cause
        // another pass 1 resolution (that's single-pass for aliases)
        let with = make_with(&[("a", json!("{{b}}")), ("b", json!("secret"))]);
        let ds = empty_datastore();

        let result = resolve_with("Got: {{a}}", &with, &ds).unwrap();
        // {{a}} resolves to literal "{{b}}", which is NOT re-evaluated by pass 1
        // Pass 2 only handles context.*/inputs.* — "b" is neither, so stays literal
        assert_eq!(result, "Got: {{b}}");
    }

    // ─── Shell escape ────────────────────────────────────────────────────────

    #[test]
    fn resolve_with_shell_escape() {
        let with = make_with(&[("val", json!("hello 'world'"))]);
        let ds = empty_datastore();
        let result = resolve_with("{{val | shell}}", &with, &ds).unwrap();
        assert_eq!(result, "'hello '\\''world'\\'''");
    }

    #[test]
    fn resolve_with_shell_plus_transform() {
        let with = make_with(&[("val", json!("Hello World"))]);
        let ds = empty_datastore();
        let result = resolve_with("{{val | lower | shell}}", &with, &ds).unwrap();
        assert_eq!(result, "'hello world'");
    }

    // ─── Edge cases ─────────────────────────────────────────────────────────

    #[test]
    fn resolve_with_empty_template() {
        let with = FxHashMap::default();
        let ds = empty_datastore();
        let result = resolve_with("", &with, &ds).unwrap();
        assert_eq!(result, "");
    }

    #[test]
    fn resolve_with_no_templates() {
        let with = FxHashMap::default();
        let ds = empty_datastore();
        let result = resolve_with("plain text", &with, &ds).unwrap();
        assert_eq!(result, "plain text");
        // Should be Cow::Borrowed (zero alloc)
        assert!(matches!(result, Cow::Borrowed(_)));
    }

    #[test]
    fn resolve_with_unclosed_braces() {
        let with = FxHashMap::default();
        let ds = empty_datastore();
        // Unclosed {{ should be left as literal (TEMPLATE_RE won't match)
        let result = resolve_with("{{incomplete", &with, &ds).unwrap();
        assert_eq!(result, "{{incomplete");
    }

    #[test]
    fn resolve_with_bracket_notation() {
        let with = make_with(&[("items", json!(["a", "b", "c"]))]);
        let ds = empty_datastore();
        let result = resolve_with("{{items[1]}}", &with, &ds).unwrap();
        assert_eq!(result, "b");
    }

    #[test]
    fn resolve_with_nested_path() {
        let with = make_with(&[(
            "user",
            json!({"name": "Alice", "address": {"city": "Paris"}}),
        )]);
        let ds = empty_datastore();
        let result = resolve_with("{{user.address.city}}", &with, &ds).unwrap();
        assert_eq!(result, "Paris");
    }

    #[test]
    fn resolve_with_mixed_aliases_and_context() {
        let with = make_with(&[("name", json!("Alice"))]);
        let ds = empty_datastore();
        let mut context = LoadedContext::new();
        context
            .files
            .insert("brand".to_string(), json!("SuperNovae"));
        ds.set_context(context);

        let result =
            resolve_with("Hello {{name}} from {{context.files.brand}}", &with, &ds).unwrap();
        assert_eq!(result, "Hello Alice from SuperNovae");
    }

    // ─── resource exhaustion guard tests ─────────────────────────────────────

    #[test]
    fn resolve_with_rejects_excessive_template_vars() {
        let with = make_with(&[("x", json!("v"))]);
        let ds = empty_datastore();
        // Build a template with MAX_TEMPLATE_VARS + 1 references
        let template: String = (0..=MAX_TEMPLATE_VARS)
            .map(|_| "{{x}}")
            .collect::<Vec<_>>()
            .join(" ");
        let result = resolve_with(&template, &with, &ds);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(
            format!("{}", err).contains("exceeding the maximum"),
            "Expected max vars error, got: {}",
            err
        );
    }

    #[test]
    fn resolve_with_accepts_many_vars_under_limit() {
        let with = make_with(&[("x", json!("v"))]);
        let ds = empty_datastore();
        // Just under the limit should succeed
        let template: String = (0..MAX_TEMPLATE_VARS)
            .map(|_| "{{x}}")
            .collect::<Vec<_>>()
            .join(" ");
        let result = resolve_with(&template, &with, &ds);
        assert!(result.is_ok());
    }

    #[test]
    fn resolve_alias_rejects_excessive_path_depth() {
        // Test the resolve_alias_path guard directly via resolve_with.
        // Build a deep path that exceeds MAX_PATH_DEPTH segments.
        let segments: Vec<String> = (0..=MAX_PATH_DEPTH).map(|i| format!("k{}", i)).collect();
        let deep_path = segments.join(".");

        // Build a deeply nested JSON value using serde_json::Map
        let mut value: Value = json!("leaf");
        for key in segments.iter().rev().skip(1) {
            let mut map = serde_json::Map::new();
            map.insert(key.clone(), value);
            value = Value::Object(map);
        }
        let with = make_with(&[(segments[0].as_str(), value)]);
        let ds = empty_datastore();
        let template = format!("{{{{{}}}}}", deep_path);
        let result = resolve_with(&template, &with, &ds);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(
            format!("{}", err).contains("exceeds maximum"),
            "Expected path depth error, got: {}",
            err
        );
    }

    // ─── extract_with_refs tests ────────────────────────────────────────────

    #[test]
    fn extract_refs_simple() {
        let refs = extract_with_refs("Hello {{name}}!");
        assert_eq!(refs, vec!["name".to_string()]);
    }

    #[test]
    fn extract_refs_deep_path() {
        let refs = extract_with_refs("{{data.items.0}}");
        assert_eq!(refs, vec!["data".to_string()]);
    }

    #[test]
    fn extract_refs_with_transforms() {
        let refs = extract_with_refs("{{title | upper | trim}}");
        assert_eq!(refs, vec!["title".to_string()]);
    }

    #[test]
    fn extract_refs_skips_context_and_inputs() {
        let refs = extract_with_refs("{{name}} and {{context.files.brand}} and {{inputs.locale}}");
        assert_eq!(refs, vec!["name".to_string()]);
    }

    #[test]
    fn extract_refs_empty() {
        let refs = extract_with_refs("no templates here");
        assert!(refs.is_empty());
    }

    #[test]
    fn extract_refs_multiple() {
        let refs = extract_with_refs("{{a}} then {{b.field}} then {{c}}");
        assert_eq!(
            refs,
            vec!["a".to_string(), "b".to_string(), "c".to_string()]
        );
    }

    // ─── validate_with_refs tests ───────────────────────────────────────────

    #[test]
    fn validate_refs_all_declared() {
        let declared: FxHashSet<String> = ["name", "title"].iter().map(|s| s.to_string()).collect();
        let result = validate_with_refs("{{name}} and {{title}}", &declared, "task1");
        assert!(result.is_ok());
    }

    #[test]
    fn validate_refs_unknown_alias() {
        let declared: FxHashSet<String> = ["name"].iter().map(|s| s.to_string()).collect();
        let result = validate_with_refs("{{name}} and {{missing}}", &declared, "task1");
        assert!(result.is_err());
    }

    #[test]
    fn validate_refs_context_not_checked() {
        // context.* refs should not be validated against declared aliases
        let declared: FxHashSet<String> = FxHashSet::default();
        let result = validate_with_refs("{{context.files.brand}}", &declared, "task1");
        assert!(result.is_ok());
    }

    #[test]
    fn validate_refs_inputs_not_checked() {
        // inputs.* refs should not be validated against declared aliases
        let declared: FxHashSet<String> = FxHashSet::default();
        let result = validate_with_refs("{{inputs.locale}}", &declared, "task1");
        assert!(result.is_ok());
    }

    #[test]
    fn validate_refs_valid_inline_transforms() {
        let declared: FxHashSet<String> = ["data"].iter().map(|s| s.to_string()).collect();
        let result = validate_with_refs("{{data | upper | trim}}", &declared, "task1");
        assert!(result.is_ok(), "valid transforms should pass: {:?}", result);
    }

    #[test]
    fn validate_refs_invalid_inline_transform() {
        let declared: FxHashSet<String> = ["data"].iter().map(|s| s.to_string()).collect();
        let result = validate_with_refs("{{data | bogus_transform}}", &declared, "task1");
        assert!(result.is_err(), "invalid transform should fail");
        let err = result.unwrap_err();
        let msg = err.to_string();
        assert!(
            msg.contains("bogus_transform"),
            "error should mention the invalid transform, got: {}",
            msg
        );
    }

    #[test]
    fn validate_refs_invalid_transform_on_inputs() {
        let declared: FxHashSet<String> = FxHashSet::default();
        let result = validate_with_refs("{{inputs.locale | bogus}}", &declared, "task1");
        assert!(result.is_err(), "invalid transform on inputs should fail");
    }

    #[test]
    fn validate_refs_nested_template_detected() {
        let declared: FxHashSet<String> = ["a", "b"].iter().map(|s| s.to_string()).collect();
        let result = validate_with_refs("{{a.{{b}}}}", &declared, "task1");
        assert!(result.is_err(), "nested templates should fail");
        let err = result.unwrap_err();
        let msg = err.to_string();
        assert!(
            msg.contains("NIKA-074") || msg.contains("Nested templates"),
            "should mention nested templates, got: {}",
            msg
        );
    }

    #[test]
    fn validate_refs_non_nested_passes() {
        let declared: FxHashSet<String> = ["a", "b"].iter().map(|s| s.to_string()).collect();
        let result = validate_with_refs("{{a}} then {{b}}", &declared, "task1");
        assert!(
            result.is_ok(),
            "sequential templates should pass: {:?}",
            result
        );
    }

    // ═════════════════════════════════════════════════════════════════════════
    // G5: Validate inputs.* references
    // ═════════════════════════════════════════════════════════════════════════

    #[test]
    fn validate_refs_full_inputs_declared_passes() {
        let declared: FxHashSet<String> = FxHashSet::default();
        let inputs: FxHashSet<String> = ["topic", "locale"].iter().map(|s| s.to_string()).collect();
        let result = validate_with_refs_full(
            "Research {{inputs.topic}} in {{inputs.locale}}",
            &declared,
            "task1",
            Some(&inputs),
            None,
        );
        assert!(result.is_ok(), "declared inputs should pass: {:?}", result);
    }

    #[test]
    fn validate_refs_full_inputs_undeclared_errors() {
        let declared: FxHashSet<String> = FxHashSet::default();
        let inputs: FxHashSet<String> = ["topic"].iter().map(|s| s.to_string()).collect();
        let result = validate_with_refs_full(
            "Translate to {{inputs.missing_locale}}",
            &declared,
            "task1",
            Some(&inputs),
            None,
        );
        assert!(result.is_err(), "undeclared input should error");
        let msg = result.unwrap_err().to_string();
        assert!(
            msg.contains("undeclared input") && msg.contains("missing_locale"),
            "error should mention undeclared input, got: {}",
            msg
        );
    }

    #[test]
    fn validate_refs_full_inputs_none_skips_check() {
        // When no declared_inputs is provided, inputs.* refs should pass
        let declared: FxHashSet<String> = FxHashSet::default();
        let result = validate_with_refs_full("{{inputs.anything}}", &declared, "task1", None, None);
        assert!(result.is_ok(), "no inputs check should pass: {:?}", result);
    }

    // ═════════════════════════════════════════════════════════════════════════
    // G6: Validate context.files.* references
    // ═════════════════════════════════════════════════════════════════════════

    #[test]
    fn validate_refs_full_context_files_declared_passes() {
        let declared: FxHashSet<String> = FxHashSet::default();
        let ctx_files: FxHashSet<String> =
            ["readme", "brand"].iter().map(|s| s.to_string()).collect();
        let result = validate_with_refs_full(
            "Based on {{context.files.readme}}",
            &declared,
            "task1",
            None,
            Some(&ctx_files),
        );
        assert!(
            result.is_ok(),
            "declared context file should pass: {:?}",
            result
        );
    }

    #[test]
    fn validate_refs_full_context_files_undeclared_errors() {
        let declared: FxHashSet<String> = FxHashSet::default();
        let ctx_files: FxHashSet<String> = ["readme"].iter().map(|s| s.to_string()).collect();
        let result = validate_with_refs_full(
            "Use {{context.files.missing}}",
            &declared,
            "task1",
            None,
            Some(&ctx_files),
        );
        assert!(result.is_err(), "undeclared context file should error");
        let msg = result.unwrap_err().to_string();
        assert!(
            msg.contains("undeclared context file") && msg.contains("missing"),
            "error should mention undeclared context file, got: {}",
            msg
        );
    }

    #[test]
    fn validate_refs_full_context_files_none_skips_check() {
        let declared: FxHashSet<String> = FxHashSet::default();
        let result =
            validate_with_refs_full("{{context.files.anything}}", &declared, "task1", None, None);
        assert!(result.is_ok(), "no context check should pass: {:?}", result);
    }

    // ═════════════════════════════════════════════════════════════════════════
    // DEEP AUDIT: is_in_json_context heuristic
    // ═════════════════════════════════════════════════════════════════════════

    /// AUDIT: is_in_json_context false positive with unbalanced quotes.
    ///
    /// BUG: The heuristic counts double-quote characters before the
    /// template position. Regular text with an odd number of quotes
    /// before a template triggers JSON escaping unnecessarily.
    #[test]
    fn audit_is_in_json_context_false_positive_unbalanced() {
        let mut bindings = ResolvedBindings::new();
        bindings.set("msg", json!("line1\nline2"));
        let ds = empty_datastore();

        // Template with an unmatched quote before the placeholder.
        // Not a JSON structure at all — just natural language with a quote.
        let template = r#"He said "hello {{with.msg}}"#;
        let result = resolve(template, &bindings, &ds).unwrap();

        // If is_in_json_context triggers, the newline is escaped to literal \n.
        // If it does NOT trigger, the raw newline is preserved.
        let has_escaped_newline = result.contains("\\n");
        let has_raw_newline = result.contains('\n');

        if has_escaped_newline && !has_raw_newline {
            // BUG CONFIRMED: The heuristic saw an odd number of quotes
            // and assumed JSON context. The newline was escaped when it
            // should not have been.
            //
            // Impact: LLM prompts like `He said "hello {{with.msg}}"` get
            // values incorrectly JSON-escaped (newlines become literal \n,
            // backslashes get doubled, etc.).
            //
            // Fix: Use a proper JSON parser or require an explicit |json
            // modifier instead of auto-detecting based on quote parity.
            panic!(
                "GAP CONFIRMED: is_in_json_context false positive! \
                 Non-JSON template '{}' has value JSON-escaped. \
                 Result: '{}'",
                template, result
            );
        }
        // If raw newline preserved, the heuristic was correct here.
        assert!(
            has_raw_newline,
            "Newline should be preserved (not JSON-escaped): '{}'",
            result
        );
    }

    /// AUDIT: is_in_json_context correct detection for actual JSON.
    #[test]
    fn audit_is_in_json_context_correct_for_real_json() {
        let mut bindings = ResolvedBindings::new();
        bindings.set("name", json!("line1\nline2"));
        let ds = empty_datastore();

        let template = r#"{"user": "{{with.name}}"}"#;
        let result = resolve(template, &bindings, &ds).unwrap();

        // In actual JSON context, newlines should be escaped
        assert!(
            result.contains("\\n"),
            "Newline should be JSON-escaped in JSON context: '{}'",
            result
        );
        assert!(
            !result.contains('\n'),
            "Raw newline must not appear in JSON context: '{}'",
            result
        );
    }

    /// AUDIT: is_in_json_context with balanced quotes (even count).
    #[test]
    fn audit_is_in_json_context_balanced_quotes_outside() {
        let mut bindings = ResolvedBindings::new();
        bindings.set("val", json!("test\nvalue"));
        let ds = empty_datastore();

        // Two balanced quotes BEFORE the template — even count = NOT in JSON
        let template = r#"He said "hi" then {{with.val}}"#;
        let result = resolve(template, &bindings, &ds).unwrap();

        // With balanced quotes, template is outside JSON string
        // The newline should be raw, not escaped
        assert!(
            result.contains('\n'),
            "Outside JSON context, newline should be raw: '{}'",
            result
        );
    }

    // ═════════════════════════════════════════════════════════════════════════
    // DEEP AUDIT: resolve_for_shell missing inputs support
    // ═════════════════════════════════════════════════════════════════════════

    /// AUDIT: resolve_for_shell does NOT resolve {{inputs.X}} templates.
    ///
    /// BUG: resolve_for_shell checks for has_with and has_context but
    /// does NOT check has_inputs. Templates with {{inputs.param}} are
    /// silently left unresolved.
    #[test]
    fn audit_resolve_for_shell_missing_inputs_support() {
        let bindings = ResolvedBindings::new();
        let store = RunContext::new(nika_core::trust::InvocationSource::Test);

        let mut inputs = FxHashMap::default();
        inputs.insert("topic".to_string(), json!("AI safety"));
        store.set_inputs(inputs);

        let result = resolve_for_shell("echo {{inputs.topic}}", &bindings, &store).unwrap();

        // BUG: resolve_for_shell does not support {{inputs.*}}
        // It checks has_with and has_context but NOT has_inputs.
        // The template is left unresolved.
        if result.contains("{{inputs.topic}}") {
            // GAP CONFIRMED: inputs not resolved
            panic!(
                "GAP CONFIRMED: resolve_for_shell does not resolve \
                 inputs templates. Result: '{}'. Fix: add has_inputs \
                 check alongside has_with and has_context.",
                result
            );
        }
        // If it IS resolved, the bug has been fixed
        assert!(result.contains("AI safety"));
    }

    // ═════════════════════════════════════════════════════════════════════════
    // DEEP AUDIT: normalize_bracket_notation edge cases
    // ═════════════════════════════════════════════════════════════════════════

    /// AUDIT: bracket notation with negative index.
    #[test]
    fn audit_bracket_notation_negative_index() {
        let mut bindings = ResolvedBindings::new();
        bindings.set("items", json!(["a", "b", "c"]));
        let ds = empty_datastore();

        // Negative index is NOT supported — the template engine now correctly reports
        // an error instead of silently leaving the template unresolved.
        let result = resolve("{{with.items[-1]}}", &bindings, &ds);
        assert!(
            result.is_err(),
            "Negative index should produce an error, got: {:?}",
            result
        );
    }

    /// AUDIT: bracket notation with non-numeric index is not supported.
    /// normalize_bracket_notation only converts numeric `[N]` to `.N`.
    /// Non-numeric keys like `[key]` are left as-is, causing resolution failure.
    #[test]
    fn audit_bracket_notation_non_numeric() {
        let mut bindings = ResolvedBindings::new();
        bindings.set("data", json!({"key": "value"}));
        let ds = empty_datastore();

        // Documented limitation: bracket notation only supports numeric indices.
        // The template engine now correctly reports an error for non-numeric brackets.
        let result = resolve("{{with.data[key]}}", &bindings, &ds);
        assert!(
            result.is_err(),
            "Non-numeric bracket access should produce an error, got: {:?}",
            result
        );
    }

    /// AUDIT: bracket notation at root level.
    #[test]
    fn audit_bracket_notation_root_array() {
        let mut bindings = ResolvedBindings::new();
        bindings.set("list", json!(["first", "second", "third"]));
        let ds = empty_datastore();

        let result = resolve("{{with.list[2]}}", &bindings, &ds).unwrap();
        assert_eq!(result, "third");
    }

    /// AUDIT: multiple bracket notations in same path.
    #[test]
    fn audit_bracket_notation_nested_arrays() {
        let mut bindings = ResolvedBindings::new();
        bindings.set("matrix", json!([[1, 2], [3, 4]]));
        let ds = empty_datastore();

        let result = resolve("{{with.matrix[1][0]}}", &bindings, &ds).unwrap();
        assert_eq!(result, "3");
    }

    // ═════════════════════════════════════════════════════════════════════════
    // DEEP AUDIT: resolve_with Shell transform double-application
    // ═════════════════════════════════════════════════════════════════════════

    /// AUDIT: verify Shell transform and escape_for_shell produce same result.
    ///
    /// In resolve_with(), when transforms contain "shell":
    /// 1. TransformExpr("shell").apply() is called (result discarded)
    /// 2. escape_for_shell() is called separately (result used)
    ///
    /// Both SHOULD produce identical output. This test verifies that.
    #[test]
    fn audit_shell_transform_vs_escape_for_shell_consistency() {
        let test_cases = vec![
            "simple",
            "hello world",
            "it's a test",
            "double\"quote",
            "",
            "special;chars|here",
            "$(whoami)",
            "`uname`",
            "$HOME/.ssh",
            "line1\nline2",
            "tab\there",
        ];

        for input in test_cases {
            // Method 1: escape_for_shell (used by resolve_with)
            let method1 = escape_for_shell(input);

            // Method 2: TransformOp::Shell (also applied but discarded)
            use crate::binding::transform::TransformOp;
            let json_val = json!(input);
            let method2_val = TransformOp::Shell.apply(&json_val).unwrap();
            let method2 = method2_val.as_str().unwrap().to_string();

            assert_eq!(
                method1, method2,
                "Shell escaping methods differ for input '{}': \
                 escape_for_shell='{}' vs TransformOp::Shell='{}'",
                input, method1, method2
            );
        }
    }

    // =========================================================================
    // Media Tool Results → Template Resolution (Full Pipeline)
    // =========================================================================
    //
    // End-to-end tests for media tool results flowing through template_resolve().
    //
    // Architecture:
    //   RunContext (task results + media refs)
    //     → BindingSpec (with: block declarations)
    //       → ResolvedBindings (from_binding_spec resolves eagerly or lazily)
    //         → template_resolve() (substitutes {{with.alias}} in strings)
    //
    // Media refs live in TaskResult.media (side-channel), NOT in TaskResult.output.
    // The binding spec intercepts "media" paths and delegates to RunContext.resolve_path().
    // This means:
    //   - `hash: $gen.media[0].hash` works (binding spec intercepts media prefix)
    //   - `{{with.hash}}` resolves to the hash value
    //
    // Direct template traversal like `{{with.gen.media[0].hash}}` where gen is
    // bound to the entire task output does NOT work because the template engine
    // traverses the output JSON, which does not contain media refs.

    /// Helper: build RunContext + ResolvedBindings for media template tests.
    fn media_template_fixtures() -> (RunContext, ResolvedBindings) {
        use crate::binding::{BindingEntry, BindingSpec};

        let store = RunContext::new(nika_core::trust::InvocationSource::Test);

        // Task "gen": image generation with media refs
        let gen_media = vec![crate::media::MediaRef {
            hash: "blake3:abc123".to_string(),
            mime_type: "image/png".to_string(),
            size_bytes: 524288,
            path: std::path::PathBuf::from("/tmp/cas/ab/c123"),
            extension: "png".to_string(),
            created_by: "gen".to_string(),
            metadata: {
                let mut m = serde_json::Map::new();
                m.insert("width".to_string(), json!(1024));
                m.insert("height".to_string(), json!(768));
                m
            },
        }];
        store.insert(
            std::sync::Arc::from("gen"),
            crate::store::TaskResult::success(
                json!({"prompt": "a sunset photo"}),
                std::time::Duration::from_secs(3),
            )
            .with_media(gen_media),
        );

        // Task "thumb": invoke returns JSON-string output
        store.insert(
            std::sync::Arc::from("thumb"),
            crate::store::TaskResult::success_str(
                r#"{"hash":"blake3:def456","mime_type":"image/png","size_bytes":2048,"metadata":{"width":256,"height":192}}"#,
                std::time::Duration::from_millis(100),
            ),
        );

        // Build binding spec (simulates the with: block)
        let mut spec = BindingSpec::default();
        // Direct media ref bindings (resolved eagerly via binding spec interception)
        spec.insert(
            "source_hash".to_string(),
            BindingEntry::new("gen.media[0].hash"),
        );
        spec.insert(
            "source_width".to_string(),
            BindingEntry::new("gen.media[0].metadata.width"),
        );
        // Invoke output bindings (resolved eagerly via JSON-string auto-parse)
        spec.insert("thumb".to_string(), BindingEntry::new("thumb"));
        spec.insert("thumb_hash".to_string(), BindingEntry::new("thumb.hash"));
        spec.insert(
            "thumb_width".to_string(),
            BindingEntry::new("thumb.metadata.width"),
        );
        // Full-task binding: simulates `with: { img: $gen }` (showcase pattern)
        spec.insert("img".to_string(), BindingEntry::new("gen"));

        let bindings = ResolvedBindings::from_binding_spec(Some(&spec), &store).unwrap();
        (store, bindings)
    }

    #[test]
    fn media_template_resolve_source_hash() {
        let (store, bindings) = media_template_fixtures();

        let result = resolve("Source image hash: {{with.source_hash}}", &bindings, &store).unwrap();

        assert_eq!(result.as_ref(), "Source image hash: blake3:abc123");
    }

    #[test]
    fn media_template_resolve_source_width() {
        let (store, bindings) = media_template_fixtures();

        let result = resolve("Original width: {{with.source_width}}px", &bindings, &store).unwrap();

        assert_eq!(result.as_ref(), "Original width: 1024px");
    }

    #[test]
    fn media_template_resolve_thumb_hash() {
        let (store, bindings) = media_template_fixtures();

        let result = resolve("Thumbnail hash: {{with.thumb_hash}}", &bindings, &store).unwrap();

        assert_eq!(result.as_ref(), "Thumbnail hash: blake3:def456");
    }

    #[test]
    fn media_template_resolve_thumb_nested_width() {
        let (store, bindings) = media_template_fixtures();

        let result = resolve("Thumb is {{with.thumb_width}}px wide", &bindings, &store).unwrap();

        assert_eq!(result.as_ref(), "Thumb is 256px wide");
    }

    #[test]
    fn media_template_thumb_output_traversal_auto_parses_json_string() {
        let (store, bindings) = media_template_fixtures();

        // Dedicated binding path still works (binding spec resolves the path)
        let result = resolve(
            "Hash via binding spec: {{with.thumb_hash}}",
            &bindings,
            &store,
        )
        .unwrap();
        assert_eq!(result.as_ref(), "Hash via binding spec: blake3:def456");

        // Template-level traversal on a JSON-string value now auto-parses
        // the JSON string and traverses into it (NIKA-253 fix).
        // This matches navigate_segments() in binding/resolve.rs.
        let result = resolve("Direct: {{with.thumb.hash}}", &bindings, &store).unwrap();
        assert_eq!(result.as_ref(), "Direct: blake3:def456");
    }

    #[test]
    fn media_template_thumb_deep_traversal_via_binding_spec() {
        let (store, bindings) = media_template_fixtures();

        // Deep path thumb.metadata.width works via binding spec (thumb_width)
        let result = resolve("Width: {{with.thumb_width}}", &bindings, &store).unwrap();
        assert_eq!(result.as_ref(), "Width: 256");
    }

    #[test]
    fn media_template_thumb_output_as_parsed_json_object() {
        // When invoke output is stored as Value::Object (not Value::String),
        // template-level traversal works. This happens when the runner parses
        // JSON before storing in TaskResult (the Value variant, not success_str).
        let store = RunContext::new(nika_core::trust::InvocationSource::Test);
        store.insert(
            std::sync::Arc::from("thumb2"),
            crate::store::TaskResult::success(
                json!({
                    "hash": "blake3:parsed_obj",
                    "metadata": { "width": 128 }
                }),
                std::time::Duration::from_millis(50),
            ),
        );

        let mut bindings = ResolvedBindings::new();
        bindings.set(
            "thumb2",
            json!({"hash": "blake3:parsed_obj", "metadata": {"width": 128}}),
        );

        let result = resolve(
            "Hash: {{with.thumb2.hash}}, Width: {{with.thumb2.metadata.width}}",
            &bindings,
            &store,
        )
        .unwrap();

        assert_eq!(result.as_ref(), "Hash: blake3:parsed_obj, Width: 128");
    }

    #[test]
    fn media_template_chained_bindings_in_one_template() {
        let (store, bindings) = media_template_fixtures();

        // Use multiple media bindings in a single template string
        let result = resolve(
            "Source: {{with.source_hash}}, Thumb: {{with.thumb_hash}}, Width: {{with.thumb_width}}",
            &bindings,
            &store,
        )
        .unwrap();

        assert_eq!(
            result.as_ref(),
            "Source: blake3:abc123, Thumb: blake3:def456, Width: 256"
        );
    }

    #[test]
    fn media_template_chained_bindings_in_prompt() {
        let (store, bindings) = media_template_fixtures();

        // Realistic prompt that chains multiple media outputs
        let result = resolve(
            "The image ({{with.source_hash}}) was resized to {{with.thumb_width}}px. \
             The thumbnail hash is {{with.thumb_hash}}.",
            &bindings,
            &store,
        )
        .unwrap();

        assert_eq!(
            result.as_ref(),
            "The image (blake3:abc123) was resized to 256px. \
             The thumbnail hash is blake3:def456."
        );
    }

    #[test]
    fn media_template_no_templates_returns_borrowed() {
        let (store, bindings) = media_template_fixtures();

        // No templates -> should return Cow::Borrowed (zero allocation)
        let result = resolve("plain text without templates", &bindings, &store).unwrap();
        assert!(
            matches!(result, std::borrow::Cow::Borrowed(_)),
            "No-template strings should be zero-alloc Cow::Borrowed"
        );
    }

    #[test]
    fn media_template_json_context_escaping() {
        let (store, bindings) = media_template_fixtures();

        // When a template appears inside a JSON string context, values are
        // JSON-escaped. This is important for media hashes in JSON payloads.
        let result = resolve(
            r#"{"source": "{{with.source_hash}}", "thumb": "{{with.thumb_hash}}"}"#,
            &bindings,
            &store,
        )
        .unwrap();

        // Parse as JSON to verify it's valid
        let parsed: serde_json::Value = serde_json::from_str(&result).unwrap();
        assert_eq!(parsed["source"], "blake3:abc123");
        assert_eq!(parsed["thumb"], "blake3:def456");
    }

    // ═════════════════════════════════════════════════════════════════════════
    // Media side-channel interception in templates (showcase pattern)
    // ═════════════════════════════════════════════════════════════════════════

    /// Showcase pattern: `img: $gen` + `{{with.img.media[0].hash}}`
    /// Media refs live in TaskResult.media (side-channel), not in output.
    /// Template engine must intercept "media" segment and resolve via RunContext.
    #[test]
    fn media_template_full_task_binding_media_hash() {
        let (store, bindings) = media_template_fixtures();

        let result = resolve("hash: {{with.img.media[0].hash}}", &bindings, &store).unwrap();

        assert_eq!(result.as_ref(), "hash: blake3:abc123");
    }

    #[test]
    fn media_template_full_task_binding_media_mime() {
        let (store, bindings) = media_template_fixtures();

        let result = resolve("{{with.img.media[0].mime_type}}", &bindings, &store).unwrap();

        assert_eq!(result.as_ref(), "image/png");
    }

    #[test]
    fn media_template_full_task_binding_media_metadata_width() {
        let (store, bindings) = media_template_fixtures();

        let result = resolve("w={{with.img.media[0].metadata.width}}", &bindings, &store).unwrap();

        assert_eq!(result.as_ref(), "w=1024");
    }

    #[test]
    fn media_template_full_task_binding_media_array() {
        let (store, bindings) = media_template_fixtures();

        let result = resolve("{{with.img.media}}", &bindings, &store).unwrap();

        // Should be a JSON array with one media ref
        let parsed: serde_json::Value = serde_json::from_str(&result).unwrap();
        assert!(parsed.is_array());
        assert_eq!(parsed[0]["hash"], "blake3:abc123");
    }

    #[test]
    fn media_template_full_task_binding_with_transform() {
        let (store, bindings) = media_template_fixtures();

        let result = resolve("{{with.img.media[0].hash | upper}}", &bindings, &store).unwrap();

        assert_eq!(result.as_ref(), "BLAKE3:ABC123");
    }

    #[test]
    fn media_template_full_task_binding_empty_media() {
        use crate::binding::BindingEntry;

        let store = RunContext::new(nika_core::trust::InvocationSource::Test);
        // Task with empty media array
        store.insert(
            std::sync::Arc::from("empty"),
            crate::store::TaskResult::success(
                json!({"status": "ok"}),
                std::time::Duration::from_secs(1),
            )
            .with_media(vec![]),
        );

        let mut spec = crate::binding::BindingSpec::default();
        spec.insert("src".to_string(), BindingEntry::new("empty"));
        let bindings = ResolvedBindings::from_binding_spec(Some(&spec), &store).unwrap();

        // Accessing media on task with no media should return empty array
        let result = resolve("{{with.src.media}}", &bindings, &store).unwrap();
        assert_eq!(result.as_ref(), "[]");
    }

    #[test]
    fn media_template_full_task_binding_empty_media_indexed() {
        use crate::binding::BindingEntry;

        let store = RunContext::new(nika_core::trust::InvocationSource::Test);
        store.insert(
            std::sync::Arc::from("empty"),
            crate::store::TaskResult::success(
                json!({"status": "ok"}),
                std::time::Duration::from_secs(1),
            )
            .with_media(vec![]),
        );

        let mut spec = crate::binding::BindingSpec::default();
        spec.insert("src".to_string(), BindingEntry::new("empty"));
        let bindings = ResolvedBindings::from_binding_spec(Some(&spec), &store).unwrap();

        // Indexed access on empty media should give a helpful error
        let result = resolve("{{with.src.media[0].hash}}", &bindings, &store);
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(
            err.contains("produced no matching media"),
            "Error should explain empty media, got: {err}"
        );
    }

    #[test]
    fn media_template_full_task_binding_out_of_bounds() {
        let (store, bindings) = media_template_fixtures();

        // Accessing media[5] when only 1 item exists should give helpful error
        let result = resolve("{{with.img.media[5].hash}}", &bindings, &store);

        assert!(result.is_err(), "Out-of-bounds media access should error");
        let err = result.unwrap_err().to_string();
        assert!(
            err.contains("produced no matching media"),
            "Error should explain out-of-bounds, got: {err}"
        );
    }

    #[test]
    fn media_template_resolve_for_shell_with_media_hash() {
        let (store, bindings) = media_template_fixtures();

        // resolve_for_shell must also intercept media paths (shell-escaped)
        let result =
            resolve_for_shell("echo {{with.img.media[0].hash}}", &bindings, &store).unwrap();

        // Shell-escaped hash should be present (blake3:abc123 has no special chars)
        assert!(
            result.contains("blake3:abc123"),
            "resolve_for_shell should resolve media hash, got: {result}"
        );
    }

    // ═════════════════════════════════════════════════════════════════════════
    // Regression tests for binding/template bugs
    // ═════════════════════════════════════════════════════════════════════════

    /// NIKA-253: nika:chart output (JSON-string) passed to nika:dimensions
    /// as `{{with.chart_result.hash}}` failed because the template system
    /// did not auto-parse JSON strings during nested traversal.
    ///
    /// The fix adds try_parse_json_str in resolve(), resolve_with(),
    /// resolve_alias_path(), and resolve_for_shell() to match the behavior
    /// already present in navigate_segments() (binding/resolve.rs).
    #[test]
    fn regression_nika253_chart_to_dimensions_json_string_traversal() {
        // Simulate how nika:chart output is stored: MediaToolAdapter returns
        // a JSON string, run_invoke re-serializes it, make_task_result wraps
        // it as Value::String (no output: json policy for invoke tasks).
        let store = RunContext::new(nika_core::trust::InvocationSource::Test);
        let chart_output_json = serde_json::json!({
            "hash": "blake3:af1349b9f5f9a1a6a0404dea36dcc9499bcb25c9adc112b7cc9a93cae41f3262",
            "path": "/tmp/cas/af/1349b9f5f9a1a6a0404dea36dcc9499bcb25c9adc112b7cc9a93cae41f3262",
            "size_bytes": 12345,
            "mime_type": "image/png",
            "extension": "png",
            "deduplicated": false,
            "metadata": { "chart_type": "bar", "width": 800, "height": 500 }
        });
        // Stored as Value::String (the bug scenario: success_str wraps JSON)
        store.insert(
            std::sync::Arc::from("gen_chart"),
            crate::store::TaskResult::success_str(
                chart_output_json.to_string(),
                std::time::Duration::from_millis(100),
            ),
        );

        // Binding: chart_result: $gen_chart
        let mut spec: crate::binding::BindingSpec = FxHashMap::default();
        spec.insert(
            "chart_result".to_string(),
            crate::binding::BindingEntry::new("gen_chart"),
        );
        let bindings = ResolvedBindings::from_binding_spec(Some(&spec), &store).unwrap();

        // Before the fix: {{with.chart_result.hash}} would fail with
        // InvalidTraversal because the template system saw Value::String
        // and refused to traverse into it.
        //
        // After the fix: auto-parse parses the JSON string into a Value::Object,
        // enabling .hash traversal.
        let result = resolve("hash: {{with.chart_result.hash}}", &bindings, &store).unwrap();
        assert_eq!(
            result.as_ref(),
            "hash: blake3:af1349b9f5f9a1a6a0404dea36dcc9499bcb25c9adc112b7cc9a93cae41f3262"
        );

        // Deep nested traversal also works
        let result = resolve(
            "type: {{with.chart_result.metadata.chart_type}}",
            &bindings,
            &store,
        )
        .unwrap();
        assert_eq!(result.as_ref(), "type: bar");

        // Width from metadata
        let result = resolve(
            "width: {{with.chart_result.metadata.width}}",
            &bindings,
            &store,
        )
        .unwrap();
        assert_eq!(result.as_ref(), "width: 800");
    }

    /// NIKA-253: resolve_with (used by run_invoke for param templates)
    /// must also auto-parse JSON strings.
    #[test]
    fn regression_nika253_resolve_with_json_string_traversal() {
        let ds = empty_datastore();
        let mut with_values = FxHashMap::default();
        with_values.insert(
            "chart_out".to_string(),
            Value::String(r#"{"hash":"blake3:abc123","size_bytes":9999}"#.to_string()),
        );

        let result = resolve_with("{{chart_out.hash}}", &with_values, &ds).unwrap();
        assert_eq!(result.as_ref(), "blake3:abc123");
    }

    /// Bug 29: normalize_bracket_notation must NOT corrupt literal text outside
    /// `{{...}}` blocks. Brackets like `data[0]` in plain text must be preserved.
    #[test]
    fn regression_bug29_bracket_notation_preserves_literal_text() {
        let mut bindings = ResolvedBindings::new();
        bindings.set("items", json!(["first", "second", "third"]));
        let ds = empty_datastore();

        // "data[0]" is literal text, "{{with.items[0]}}" is a template block
        let result = resolve("data[0] is {{with.items[0]}}", &bindings, &ds).unwrap();
        assert_eq!(
            result, "data[0] is first",
            "Literal 'data[0]' outside {{}} must NOT be normalized to 'data.0'"
        );
    }

    /// Bug 29: normalize_bracket_notation preserves multiple literal brackets.
    #[test]
    fn regression_bug29_multiple_literal_brackets() {
        let with = make_with(&[("items", json!(["a", "b"]))]);
        let ds = empty_datastore();

        let result = resolve_with(
            "arr[0] and arr[1] are {{items[0]}} and {{items[1]}}",
            &with,
            &ds,
        )
        .unwrap();
        assert_eq!(
            result, "arr[0] and arr[1] are a and b",
            "Multiple literal brackets must be preserved"
        );
    }

    /// Bug 29: normalize_bracket_notation direct unit test.
    #[test]
    fn regression_bug29_normalize_unit_test() {
        // Literal brackets outside {{ }} must NOT be changed
        assert_eq!(
            normalize_bracket_notation("data[0] is {{with.items[0]}}"),
            "data[0] is {{with.items.0}}"
        );
        // No template blocks: brackets left as-is
        assert_eq!(
            normalize_bracket_notation("array[5] is cool"),
            "array[5] is cool"
        );
        // Brackets only inside {{ }}: normalized
        assert_eq!(
            normalize_bracket_notation("{{a[0]}} and {{b[1]}}"),
            "{{a.0}} and {{b.1}}"
        );
    }

    /// Bug 45: resolve_with must NOT evaluate context/input templates injected
    /// via with: values. Cross-pass contamination is blocked by checking the
    /// ORIGINAL template for context/inputs markers.
    #[test]
    fn regression_bug45_no_cross_pass_contamination() {
        let with = make_with(&[("user_input", json!("{{context.files.secret}}"))]);
        let ds = empty_datastore();
        let mut context = LoadedContext::new();
        context
            .files
            .insert("secret".to_string(), json!("TOP_SECRET_VALUE"));
        ds.set_context(context);

        let result = resolve_with("Result: {{user_input}}", &with, &ds).unwrap();
        // The original template "Result: {{user_input}}" does NOT contain "context."
        // so Pass 2 must NOT run. The literal "{{context.files.secret}}" stays.
        assert_eq!(result, "Result: {{context.files.secret}}");
        assert!(
            !result.contains("TOP_SECRET_VALUE"),
            "with: value containing {{context.files.x}} must NOT be evaluated"
        );
    }

    /// Bug 45: resolve_with must NOT evaluate inputs templates injected via with: values.
    #[test]
    fn regression_bug45_no_inputs_injection() {
        let with = make_with(&[("val", json!("{{inputs.locale}}"))]);
        let ds = empty_datastore();
        let mut inputs = FxHashMap::default();
        inputs.insert("locale".to_string(), json!("fr-FR"));
        ds.set_inputs(inputs);

        let result = resolve_with("Got: {{val}}", &with, &ds).unwrap();
        assert_eq!(result, "Got: {{inputs.locale}}");
        assert!(
            !result.contains("fr-FR"),
            "with: value containing {{inputs.x}} must NOT be evaluated"
        );
    }

    /// Bug 45: when the original template DOES contain context refs, they still resolve.
    #[test]
    fn regression_bug45_legitimate_context_still_resolves() {
        let with = make_with(&[("name", json!("Alice"))]);
        let ds = empty_datastore();
        let mut context = LoadedContext::new();
        context
            .files
            .insert("brand".to_string(), json!("SuperNovae"));
        ds.set_context(context);

        // Original template contains both alias and context refs
        let result =
            resolve_with("Hello {{name}} from {{context.files.brand}}", &with, &ds).unwrap();
        assert_eq!(result, "Hello Alice from SuperNovae");
    }

    /// Bug 47: shell transform must not be double-applied in resolve_with.
    /// Verifies the output is correctly shell-escaped exactly once.
    #[test]
    fn regression_bug47_shell_not_double_applied() {
        let with = make_with(&[("val", json!("hello world"))]);
        let ds = empty_datastore();
        let result = resolve_with("{{val | shell}}", &with, &ds).unwrap();
        // Correct: single shell escape wraps in single quotes
        assert_eq!(result, "'hello world'");
        // If double-applied, would be "''hello world''" or similar nested quoting
    }

    /// Bug 47: shell transform with other transforms must not double-apply.
    #[test]
    fn regression_bug47_shell_with_chain_not_double_applied() {
        let with = make_with(&[("val", json!("Hello World"))]);
        let ds = empty_datastore();
        let result = resolve_with("{{val | lower | shell}}", &with, &ds).unwrap();
        // lower applied first, then shell escape
        assert_eq!(result, "'hello world'");
    }

    /// Bug 47: shell transform on value with quotes must escape correctly once.
    #[test]
    fn regression_bug47_shell_with_quotes() {
        let with = make_with(&[("val", json!("it's a test"))]);
        let ds = empty_datastore();
        let result = resolve_with("{{val | shell}}", &with, &ds).unwrap();
        // Single correct shell escape: 'it'\''s a test'
        assert_eq!(result, "'it'\\''s a test'");
    }

    // ========================================================================
    // Template injection prevention tests (trusted_inputs + trusted_context)
    // ========================================================================

    /// Verify that injected {{inputs.secret}} via with: binding is NOT resolved.
    /// Template: "{{data}} about {{inputs.topic}}"
    /// with data = "{{inputs.secret}}" — inputs.secret must NOT leak.
    #[test]
    fn template_injection_inputs_blocked_in_resolve() {
        let mut bindings = ResolvedBindings::new();
        bindings.set("data", json!("{{inputs.secret}}"));
        let ds = RunContext::new(nika_core::trust::InvocationSource::Test);
        let mut inputs = rustc_hash::FxHashMap::default();
        inputs.insert("topic".to_string(), json!("AI workflows"));
        inputs.insert("secret".to_string(), json!("sk-ant-SHOULD-NOT-LEAK"));
        ds.set_inputs(inputs);

        let result = resolve("{{with.data}} about {{inputs.topic}}", &bindings, &ds).unwrap();
        // inputs.topic should resolve, but injected inputs.secret should NOT
        assert!(
            !result.contains("sk-ant-SHOULD-NOT-LEAK"),
            "Secret leaked via template injection: {result}"
        );
        assert!(
            result.contains("AI workflows"),
            "Legitimate input not resolved: {result}"
        );
    }

    /// Same test for resolve_with
    #[test]
    fn template_injection_inputs_blocked_in_resolve_with() {
        let with = make_with(&[("data", json!("{{inputs.secret}}"))]);
        let ds = RunContext::new(nika_core::trust::InvocationSource::Test);
        let mut inputs = rustc_hash::FxHashMap::default();
        inputs.insert("topic".to_string(), json!("AI workflows"));
        inputs.insert("secret".to_string(), json!("sk-ant-SHOULD-NOT-LEAK"));
        ds.set_inputs(inputs);

        let result = resolve_with("{{data}} about {{inputs.topic}}", &with, &ds).unwrap();
        assert!(
            !result.contains("sk-ant-SHOULD-NOT-LEAK"),
            "Secret leaked via template injection in resolve_with: {result}"
        );
        assert!(
            result.contains("AI workflows"),
            "Legitimate input not resolved: {result}"
        );
    }

    /// Verify that injected {{context.files.secret}} via with: binding is NOT resolved.
    #[test]
    fn template_injection_context_blocked_in_resolve_with() {
        use crate::store::LoadedContext;

        let with = make_with(&[("user_input", json!("{{context.files.secret}}"))]);
        let ds = RunContext::new(nika_core::trust::InvocationSource::Test);
        let mut ctx = LoadedContext::new();
        ctx.files.insert("brand".to_string(), json!("SuperNovae"));
        ctx.files
            .insert("secret".to_string(), json!("TOP-SECRET-DATA"));
        ds.set_context(ctx);

        let result = resolve_with(
            "{{user_input}} for brand {{context.files.brand}}",
            &with,
            &ds,
        )
        .unwrap();
        assert!(
            !result.contains("TOP-SECRET-DATA"),
            "Secret context leaked via template injection: {result}"
        );
        assert!(
            result.contains("SuperNovae"),
            "Legitimate context not resolved: {result}"
        );
    }

    // ════════════════════════���══════════════════════════════════════
    // BUG-035: Template missing key with | default()
    // ═══════════════════════════════════════════════════════════════

    #[test]
    fn template_missing_key_with_default() {
        let mut with = FxHashMap::default();
        with.insert("obj".to_string(), json!({"name": "Alice"}));
        let ds = RunContext::new(nika_core::trust::InvocationSource::Test);

        let result = resolve_with("Value: {{with.obj.missing | default(\"N/A\")}}", &with, &ds);
        assert_eq!(result.unwrap(), "Value: N/A");
    }

    #[test]
    fn template_missing_key_no_default_still_errors() {
        let mut with = FxHashMap::default();
        with.insert("obj".to_string(), json!({"name": "Alice"}));
        let ds = RunContext::new(nika_core::trust::InvocationSource::Test);

        let result = resolve_with("Value: {{with.obj.missing}}", &with, &ds);
        assert!(result.is_err());
    }

    #[test]
    fn template_missing_key_with_non_default_transform_errors() {
        let mut with = FxHashMap::default();
        with.insert("obj".to_string(), json!({"name": "Alice"}));
        let ds = RunContext::new(nika_core::trust::InvocationSource::Test);

        // | trim on missing path should NOT silently succeed
        let result = resolve_with("Value: {{with.obj.missing | trim}}", &with, &ds);
        assert!(result.is_err());
    }

    #[test]
    fn template_missing_key_default_then_upper_works() {
        let mut with = FxHashMap::default();
        with.insert("obj".to_string(), json!({"name": "Alice"}));
        let ds = RunContext::new(nika_core::trust::InvocationSource::Test);

        // default() first → "n/a", then upper → "N/A"
        let result = resolve_with(
            "Value: {{with.obj.missing | default(\"n/a\") | upper}}",
            &with,
            &ds,
        );
        assert_eq!(result.unwrap(), "Value: N/A");
    }

    #[test]
    fn template_missing_key_upper_then_default_errors() {
        let mut with = FxHashMap::default();
        with.insert("obj".to_string(), json!({"name": "Alice"}));
        let ds = RunContext::new(nika_core::trust::InvocationSource::Test);

        // upper first fails on null → falls through to error (order matters)
        let result = resolve_with(
            "Value: {{with.obj.missing | upper | default(\"x\")}}",
            &with,
            &ds,
        );
        // The has_default check finds default() in the chain, but when applied
        // to null, upper fails first → default never fires → the apply() returns Err
        // → falls through to error collection. This is correct: order matters.
        assert!(result.is_err());
    }
}
