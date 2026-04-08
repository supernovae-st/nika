// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Transform helpers — JQ evaluation, deep merge, dot-path navigation, Display impls.

use serde_json::Value;
use std::fmt;

use super::{TransformError, TransformOp};

// ═══════════════════════════════════════════════════════════════
// Helpers
// ═══════════════════════════════════════════════════════════════

/// Get a human-readable type name for a JSON value.
pub(super) fn value_type_name(value: &Value) -> &'static str {
    match value {
        Value::Null => "null",
        Value::Bool(_) => "boolean",
        Value::Number(_) => "number",
        Value::String(_) => "string",
        Value::Array(_) => "array",
        Value::Object(_) => "object",
    }
}

/// Helper to create TypeMismatch errors.
pub(super) fn type_mismatch(op: &'static str, expected: &'static str, got: &Value) -> TransformError {
    TransformError::TypeMismatch {
        op,
        expected,
        got: value_type_name(got).to_string(),
    }
}

/// Convert f64 to JSON Value with NaN/Infinity guard.
/// Returns integer if no fractional part, null for non-finite values.
pub(super) fn f64_to_json_number(v: f64) -> Value {
    if v.is_nan() || v.is_infinite() {
        return Value::Null;
    }
    if v.fract() == 0.0 && v >= i64::MIN as f64 && v <= i64::MAX as f64 {
        Value::Number((v as i64).into())
    } else {
        serde_json::Number::from_f64(v)
            .map(Value::Number)
            .unwrap_or(Value::Null)
    }
}

/// Split parametric arguments on commas, respecting quoted strings.
/// e.g. `"name", "active"` → `["name", "active"]`
pub(super) fn split_parametric_args(input: &str) -> Vec<&str> {
    let mut result = Vec::new();
    let mut quote_char: Option<char> = None;
    let mut start = 0;
    for (i, c) in input.char_indices() {
        match c {
            '"' | '\'' => {
                if quote_char == Some(c) {
                    quote_char = None;
                } else if quote_char.is_none() {
                    quote_char = Some(c);
                }
            }
            ',' if quote_char.is_none() => {
                result.push(&input[start..i]);
                start = i + 1;
            }
            _ => {}
        }
    }
    result.push(&input[start..]);
    result
}

/// Strip surrounding quotes (single or double) from a string argument.
pub(super) fn strip_quotes(s: &str) -> &str {
    let trimmed = s.trim();
    if (trimmed.starts_with('\'') && trimmed.ends_with('\''))
        || (trimmed.starts_with('"') && trimmed.ends_with('"'))
    {
        &trimmed[1..trimmed.len() - 1]
    } else {
        trimmed
    }
}

/// Parse a default value argument: string, number, bool, null, or JSON.
pub(super) fn parse_default_value(arg: &str) -> Result<Value, String> {
    let trimmed = arg.trim();

    // Quoted string
    if (trimmed.starts_with('\'') && trimmed.ends_with('\''))
        || (trimmed.starts_with('"') && trimmed.ends_with('"'))
    {
        return Ok(Value::String(trimmed[1..trimmed.len() - 1].to_string()));
    }

    // null
    if trimmed == "null" {
        return Ok(Value::Null);
    }

    // boolean
    if trimmed == "true" {
        return Ok(Value::Bool(true));
    }
    if trimmed == "false" {
        return Ok(Value::Bool(false));
    }

    // integer
    if let Ok(n) = trimmed.parse::<i64>() {
        return Ok(Value::Number(n.into()));
    }

    // float
    if let Ok(f) = trimmed.parse::<f64>() {
        if let Some(n) = serde_json::Number::from_f64(f) {
            return Ok(Value::Number(n));
        }
    }

    // JSON object or array
    if (trimmed.starts_with('{') && trimmed.ends_with('}'))
        || (trimmed.starts_with('[') && trimmed.ends_with(']'))
    {
        return serde_json::from_str(trimmed).map_err(|e| format!("invalid JSON default: {}", e));
    }

    // Bare string (unquoted) — treat as string
    Ok(Value::String(trimmed.to_string()))
}

/// Truncate a string for error messages (UTF-8 safe).
pub(super) fn truncate(s: &str, max: usize) -> String {
    if s.len() <= max {
        s.to_string()
    } else {
        // Find a valid UTF-8 char boundary at or before `max`
        let mut end = max;
        while end > 0 && !s.is_char_boundary(end) {
            end -= 1;
        }
        format!("{}...", &s[..end])
    }
}

/// Strip markdown code block wrappers from a string.
/// Handles: `` ```json\n...\n``` ``, `` ```\n...\n``` ``, and bare strings.
pub(super) fn strip_markdown_code_block(s: &str) -> String {
    let trimmed = s.trim();
    if trimmed.starts_with("```") {
        // Find the end of the opening fence line
        let after_fence = if let Some(newline_pos) = trimmed.find('\n') {
            &trimmed[newline_pos + 1..]
        } else {
            return trimmed.to_string();
        };
        // Remove closing fence
        if let Some(stripped) = after_fence.strip_suffix("```") {
            stripped.trim().to_string()
        } else {
            after_fence.trim().to_string()
        }
    } else {
        trimmed.to_string()
    }
}

/// Strip UTF-8 BOM and NUL bytes from a string.
///
/// Exec output may contain a BOM (`\u{FEFF}`) or stray NUL bytes that break
/// JSON parsing. This is safe to call on any string — it only removes
/// characters that are never valid in JSON content.
pub(super) fn strip_bom_and_control_chars(s: &str) -> String {
    let s = s.strip_prefix('\u{FEFF}').unwrap_or(s);
    if s.contains('\0') {
        s.replace('\0', "")
    } else {
        s.to_string()
    }
}

/// Shell-escape a string (single-quote wrapping).
pub(super) fn shell_escape(s: &str) -> String {
    // Wrap in single quotes, escaping any internal single quotes
    format!("'{}'", s.replace('\'', "'\\''"))
}

impl fmt::Display for TransformOp {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            TransformOp::Upper => write!(f, "upper"),
            TransformOp::Lower => write!(f, "lower"),
            TransformOp::Trim => write!(f, "trim"),
            TransformOp::TrimStart => write!(f, "trim_start"),
            TransformOp::TrimEnd => write!(f, "trim_end"),
            TransformOp::Length => write!(f, "length"),
            TransformOp::First => write!(f, "first"),
            TransformOp::Last => write!(f, "last"),
            TransformOp::FirstN(n) => write!(f, "first({})", n),
            TransformOp::LastN(n) => write!(f, "last({})", n),
            TransformOp::Keys => write!(f, "keys"),
            TransformOp::Values => write!(f, "values"),
            TransformOp::Flatten => write!(f, "flatten"),
            TransformOp::Reverse => write!(f, "reverse"),
            TransformOp::Sort => write!(f, "sort"),
            TransformOp::Unique => write!(f, "unique"),
            TransformOp::Compact => write!(f, "compact"),
            TransformOp::ToString => write!(f, "to_string"),
            TransformOp::ToNumber => write!(f, "to_number"),
            TransformOp::ToBool => write!(f, "to_bool"),
            TransformOp::ToJson => write!(f, "to_json"),
            TransformOp::ParseJson => write!(f, "parse_json"),
            TransformOp::ParseYaml => write!(f, "parse_yaml"),
            TransformOp::Round(None) => write!(f, "round"),
            TransformOp::Round(Some(d)) => write!(f, "round({})", d),
            TransformOp::Abs => write!(f, "abs"),
            TransformOp::Ceil => write!(f, "ceil"),
            TransformOp::Floor => write!(f, "floor"),
            TransformOp::Default(v) => write!(f, "default({})", v),
            TransformOp::TypeOf => write!(f, "type_of"),
            TransformOp::Join(sep) => write!(f, "join('{}')", sep),
            TransformOp::Split(sep) => write!(f, "split('{}')", sep),
            TransformOp::Shell => write!(f, "shell"),
            TransformOp::UrlHost => write!(f, "url_host"),
            TransformOp::UrlPath => write!(f, "url_path"),
            TransformOp::UrlWithoutQuery => write!(f, "url_without_query"),
            TransformOp::UrlNormalize => write!(f, "url_normalize"),
            TransformOp::Slice(s, e) => write!(f, "slice({}, {})", s, e),
            TransformOp::Pluck(field) => write!(f, "pluck('{}')", field),
            TransformOp::Where(field, op, val) => {
                if op == "eq" {
                    write!(f, "where('{}', {})", field, val)
                } else {
                    write!(f, "where('{}', '{}', {})", field, op, val)
                }
            }
            TransformOp::Pick(fields) => {
                let quoted: Vec<String> = fields.iter().map(|f| format!("'{}'", f)).collect();
                write!(f, "pick({})", quoted.join(", "))
            }
            TransformOp::Omit(fields) => {
                let quoted: Vec<String> = fields.iter().map(|f| format!("'{}'", f)).collect();
                write!(f, "omit({})", quoted.join(", "))
            }
            TransformOp::SortBy(field) => write!(f, "sort_by('{}')", field),
            TransformOp::GroupBy(field) => write!(f, "group_by('{}')", field),
            TransformOp::Merge(None) => write!(f, "merge"),
            TransformOp::Merge(Some(v)) => write!(f, "merge({})", v),
            TransformOp::Regex(pattern) => write!(f, "regex('{}')", pattern),
            TransformOp::Base64Encode => write!(f, "base64_encode"),
            TransformOp::Base64Decode => write!(f, "base64_decode"),
            TransformOp::StartsWith(prefix) => write!(f, "starts_with('{}')", prefix),
            TransformOp::EndsWith(suffix) => write!(f, "ends_with('{}')", suffix),
            TransformOp::Contains(text) => write!(f, "contains('{}')", text),
            TransformOp::ContentHash => write!(f, "content_hash"),
            TransformOp::UniqueUrls => write!(f, "unique_urls"),
            TransformOp::Replace(from, to) => write!(f, "replace('{}', '{}')", from, to),
            TransformOp::Truncate(n) => write!(f, "truncate({})", n),
            TransformOp::Add => write!(f, "add"),
            TransformOp::Min => write!(f, "min"),
            TransformOp::Max => write!(f, "max"),
            TransformOp::MinBy(f_name) => write!(f, "min_by('{}')", f_name),
            TransformOp::MaxBy(f_name) => write!(f, "max_by('{}')", f_name),
            TransformOp::Sum => write!(f, "sum"),
            TransformOp::Avg => write!(f, "avg"),
            TransformOp::Has(key) => write!(f, "has('{}')", key),
            TransformOp::Not => write!(f, "not"),
            TransformOp::Jq(expr) => write!(f, "jq('{}')", expr),
            TransformOp::HtmlEscape => write!(f, "html_escape"),
            TransformOp::MdEscape => write!(f, "md_escape"),
            TransformOp::Sanitize => write!(f, "sanitize"),
        }
    }
}

