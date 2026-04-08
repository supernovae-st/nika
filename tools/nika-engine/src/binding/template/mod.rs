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
pub(super) fn value_to_display(value: &Value) -> Cow<'_, str> {
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
        let trusted_context: FxHashSet<String> = TEMPLATE_RE
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
        let trusted_inputs: FxHashSet<String> = TEMPLATE_RE
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
        let trusted_skills: FxHashSet<String> = TEMPLATE_RE
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
        let trusted_context: FxHashSet<String> = TEMPLATE_RE
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
        let trusted_inputs: FxHashSet<String> = TEMPLATE_RE
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
        let trusted_skills: FxHashSet<String> = TEMPLATE_RE
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
mod tests;
