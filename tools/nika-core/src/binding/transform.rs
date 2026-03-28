//! Transform Engine
//!
//! Pipeline transforms applied to binding values.
//! Transforms are chained with `|` pipes: `sort | unique | first(3)`
//!
//! # Categories
//!
//! | Category | Ops |
//! |----------|-----|
//! | String | `upper`, `lower`, `trim`, `trim_start`, `trim_end` |
//! | Collection | `length`, `first`, `last`, `first(N)`, `last(N)`, `keys`, `values`, `flatten`, `reverse`, `sort`, `unique`, `compact` |
//! | Type conversion | `to_string`, `to_number`, `to_bool`, `to_json`, `parse_json` |
//! | Numeric | `round(N)`, `abs`, `ceil`, `floor` |
//! | Utility | `default(V)`, `type_of`, `join(S)`, `split(S)`, `shell` |
//!
//! # Null Handling
//!
//! - **Propagating**: null in → null out (`length`, `keys`, `type_of`, `to_string`, `to_json`)
//! - **Failing**: null in → NIKA-153 error (`upper`, `lower`, `sort`, etc.)
//! - Use `default()` or `??` to handle nulls safely

use serde_json::Value;
use smallvec::SmallVec;
use std::fmt;

/// A single transform operation
#[derive(Debug, Clone, PartialEq)]
pub enum TransformOp {
    // -- String --
    Upper,
    Lower,
    Trim,
    TrimStart,
    TrimEnd,

    // -- Collection --
    Length,
    First,
    Last,
    FirstN(usize),
    LastN(usize),
    Keys,
    Values,
    Flatten,
    Reverse,
    Sort,
    Unique,
    Compact, // remove nulls

    // -- Type conversion --
    ToString,
    ToNumber,
    ToBool,
    ToJson,
    ParseJson,

    // -- Numeric --
    Round(Option<u32>),
    Abs,
    Ceil,
    Floor,

    // -- Utility --
    Default(Value),
    TypeOf,
    Join(String),
    Split(String),
    Shell,
}

/// A chain of transform operations: `sort | unique | first(3)`
#[derive(Debug, Clone, PartialEq)]
pub struct TransformExpr {
    pub ops: SmallVec<[TransformOp; 2]>,
}

/// Error parsing a transform expression (NIKA-151)
#[derive(Debug, Clone, PartialEq)]
pub struct TransformParseError {
    pub input: String,
    pub reason: String,
}

impl fmt::Display for TransformParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "[NIKA-151] Transform parse error in '{}': {}",
            self.input, self.reason
        )
    }
}

impl std::error::Error for TransformParseError {}

/// Error applying a transform (NIKA-152 type mismatch, NIKA-153 null input)
#[derive(Debug, Clone, PartialEq)]
pub enum TransformError {
    /// NIKA-152: Type mismatch
    TypeMismatch {
        op: &'static str,
        expected: &'static str,
        got: String,
    },
    /// NIKA-153: Null input on a failing transform
    NullInput { op: &'static str },
}

impl fmt::Display for TransformError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            TransformError::TypeMismatch { op, expected, got } => {
                write!(
                    f,
                    "[NIKA-152] Transform '{}' failed: expected {}, got {}",
                    op, expected, got
                )?;
                // Suggest to_string when a string transform receives a non-string
                if *expected == "string"
                    && (got == "object" || got == "array" || got == "number" || got == "boolean")
                {
                    write!(f, " — try: to_string | {}", op)?;
                }
                Ok(())
            }
            TransformError::NullInput { op } => {
                write!(
                    f,
                    "[NIKA-153] Transform '{}' received null — use default() to handle",
                    op
                )
            }
        }
    }
}

impl std::error::Error for TransformError {}

// ═══════════════════════════════════════════════════════════════
// TransformExpr
// ═══════════════════════════════════════════════════════════════

impl TransformExpr {
    /// Parse a pipe-separated transform expression.
    ///
    /// Examples: `"sort | unique | first(3)"`, `"upper"`, `""`
    pub fn parse(input: &str) -> Result<Self, TransformParseError> {
        let trimmed = input.trim();
        if trimmed.is_empty() {
            return Ok(TransformExpr {
                ops: SmallVec::new(),
            });
        }

        let ops: SmallVec<[TransformOp; 2]> = trimmed
            .split('|')
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(|s| parse_single_op(s, input))
            .collect::<Result<_, _>>()?;

        Ok(TransformExpr { ops })
    }

    /// Apply all transforms in sequence to a value.
    pub fn apply(&self, value: &Value) -> Result<Value, TransformError> {
        let mut current = value.clone();
        for op in &self.ops {
            current = op.apply(&current)?;
        }
        Ok(current)
    }

    /// Returns true if this expression is empty (no-op).
    pub fn is_empty(&self) -> bool {
        self.ops.is_empty()
    }
}

// ═══════════════════════════════════════════════════════════════
// TransformOp::apply
// ═══════════════════════════════════════════════════════════════

impl TransformOp {
    /// Apply this single transform to a JSON value.
    pub fn apply(&self, value: &Value) -> Result<Value, TransformError> {
        match self {
            // ── String ───────────────────────────────────────
            TransformOp::Upper => match value {
                Value::Null => Err(TransformError::NullInput { op: "upper" }),
                Value::String(s) => Ok(Value::String(s.to_uppercase())),
                _ => Err(type_mismatch("upper", "string", value)),
            },
            TransformOp::Lower => match value {
                Value::Null => Err(TransformError::NullInput { op: "lower" }),
                Value::String(s) => Ok(Value::String(s.to_lowercase())),
                _ => Err(type_mismatch("lower", "string", value)),
            },
            TransformOp::Trim => match value {
                Value::Null => Err(TransformError::NullInput { op: "trim" }),
                Value::String(s) => Ok(Value::String(s.trim().to_string())),
                _ => Err(type_mismatch("trim", "string", value)),
            },
            TransformOp::TrimStart => match value {
                Value::Null => Err(TransformError::NullInput { op: "trim_start" }),
                Value::String(s) => Ok(Value::String(s.trim_start().to_string())),
                _ => Err(type_mismatch("trim_start", "string", value)),
            },
            TransformOp::TrimEnd => match value {
                Value::Null => Err(TransformError::NullInput { op: "trim_end" }),
                Value::String(s) => Ok(Value::String(s.trim_end().to_string())),
                _ => Err(type_mismatch("trim_end", "string", value)),
            },

            // ── Collection ───────────────────────────────────
            TransformOp::Length => match value {
                Value::Null => Ok(Value::Null), // propagating
                Value::Array(arr) => Ok(Value::Number(arr.len().into())),
                Value::String(s) => Ok(Value::Number(s.chars().count().into())),
                Value::Object(obj) => Ok(Value::Number(obj.len().into())),
                _ => Err(type_mismatch("length", "array, string, or object", value)),
            },
            TransformOp::First => match value {
                Value::Null => Err(TransformError::NullInput { op: "first" }),
                Value::Array(arr) => Ok(arr.first().cloned().unwrap_or(Value::Null)),
                _ => Err(type_mismatch("first", "array", value)),
            },
            TransformOp::Last => match value {
                Value::Null => Err(TransformError::NullInput { op: "last" }),
                Value::Array(arr) => Ok(arr.last().cloned().unwrap_or(Value::Null)),
                _ => Err(type_mismatch("last", "array", value)),
            },
            TransformOp::FirstN(n) => match value {
                Value::Null => Err(TransformError::NullInput { op: "first" }),
                Value::Array(arr) => {
                    let taken: Vec<Value> = arr.iter().take(*n).cloned().collect();
                    Ok(Value::Array(taken))
                }
                Value::String(s) => {
                    // Truncate string to N characters
                    let truncated: String = s.chars().take(*n).collect();
                    Ok(Value::String(truncated))
                }
                Value::Object(_) => {
                    // Serialize object to JSON string and truncate to N characters
                    let json = serde_json::to_string(value).unwrap_or_default();
                    let truncated: String = json.chars().take(*n).collect();
                    Ok(Value::String(truncated))
                }
                _ => Err(type_mismatch("first", "array, string, or object", value)),
            },
            TransformOp::LastN(n) => match value {
                Value::Null => Err(TransformError::NullInput { op: "last" }),
                Value::Array(arr) => {
                    let skip = arr.len().saturating_sub(*n);
                    let taken: Vec<Value> = arr.iter().skip(skip).cloned().collect();
                    Ok(Value::Array(taken))
                }
                _ => Err(type_mismatch("last", "array", value)),
            },
            TransformOp::Keys => match value {
                Value::Null => Ok(Value::Null), // propagating
                Value::Object(obj) => {
                    let keys: Vec<Value> = obj.keys().map(|k| Value::String(k.clone())).collect();
                    Ok(Value::Array(keys))
                }
                _ => Err(type_mismatch("keys", "object", value)),
            },
            TransformOp::Values => match value {
                Value::Null => Err(TransformError::NullInput { op: "values" }),
                Value::Object(obj) => {
                    let vals: Vec<Value> = obj.values().cloned().collect();
                    Ok(Value::Array(vals))
                }
                _ => Err(type_mismatch("values", "object", value)),
            },
            TransformOp::Flatten => match value {
                Value::Null => Err(TransformError::NullInput { op: "flatten" }),
                Value::Array(arr) => {
                    let mut flat = Vec::new();
                    for item in arr {
                        match item {
                            Value::Array(inner) => flat.extend(inner.iter().cloned()),
                            other => flat.push(other.clone()),
                        }
                    }
                    Ok(Value::Array(flat))
                }
                _ => Err(type_mismatch("flatten", "array", value)),
            },
            TransformOp::Reverse => match value {
                Value::Null => Err(TransformError::NullInput { op: "reverse" }),
                Value::Array(arr) => {
                    let mut rev = arr.clone();
                    rev.reverse();
                    Ok(Value::Array(rev))
                }
                _ => Err(type_mismatch("reverse", "array", value)),
            },
            TransformOp::Sort => match value {
                Value::Null => Err(TransformError::NullInput { op: "sort" }),
                Value::Array(arr) => {
                    let mut sorted = arr.clone();
                    sorted.sort_by(|a, b| match (a.as_f64(), b.as_f64()) {
                        (Some(x), Some(y)) => {
                            x.partial_cmp(&y).unwrap_or(std::cmp::Ordering::Equal)
                        }
                        (Some(_), None) => std::cmp::Ordering::Less,
                        (None, Some(_)) => std::cmp::Ordering::Greater,
                        _ => a.to_string().cmp(&b.to_string()),
                    });
                    Ok(Value::Array(sorted))
                }
                _ => Err(type_mismatch("sort", "array", value)),
            },
            TransformOp::Unique => match value {
                Value::Null => Err(TransformError::NullInput { op: "unique" }),
                Value::Array(arr) => {
                    let mut seen = Vec::new();
                    let mut unique = Vec::new();
                    for item in arr {
                        let s = item.to_string();
                        if !seen.contains(&s) {
                            seen.push(s);
                            unique.push(item.clone());
                        }
                    }
                    Ok(Value::Array(unique))
                }
                _ => Err(type_mismatch("unique", "array", value)),
            },
            TransformOp::Compact => match value {
                Value::Null => Err(TransformError::NullInput { op: "compact" }),
                Value::Array(arr) => {
                    let compacted: Vec<Value> =
                        arr.iter().filter(|v| !v.is_null()).cloned().collect();
                    Ok(Value::Array(compacted))
                }
                _ => Err(type_mismatch("compact", "array", value)),
            },

            // ── Type conversion ──────────────────────────────
            TransformOp::ToString => match value {
                Value::Null => Ok(Value::Null), // propagating
                Value::String(_) => Ok(value.clone()),
                Value::Number(n) => Ok(Value::String(n.to_string())),
                Value::Bool(b) => Ok(Value::String(b.to_string())),
                _ => Ok(Value::String(value.to_string())),
            },
            TransformOp::ToNumber => match value {
                Value::Null => Err(TransformError::NullInput { op: "to_number" }),
                Value::Number(_) => Ok(value.clone()),
                Value::String(s) => {
                    if let Ok(n) = s.parse::<i64>() {
                        Ok(Value::Number(n.into()))
                    } else if let Ok(f) = s.parse::<f64>() {
                        Ok(serde_json::Number::from_f64(f)
                            .map(Value::Number)
                            .unwrap_or(Value::Null))
                    } else {
                        Err(TransformError::TypeMismatch {
                            op: "to_number",
                            expected: "numeric string",
                            got: format!("\"{}\"", s),
                        })
                    }
                }
                Value::Bool(b) => Ok(Value::Number(if *b { 1 } else { 0 }.into())),
                _ => Err(type_mismatch("to_number", "string, number, or bool", value)),
            },
            TransformOp::ToBool => match value {
                Value::Null => Err(TransformError::NullInput { op: "to_bool" }),
                Value::Bool(_) => Ok(value.clone()),
                Value::Number(n) => Ok(Value::Bool(n.as_f64().map(|f| f != 0.0).unwrap_or(false))),
                Value::String(s) => match s.as_str() {
                    "true" | "1" | "yes" => Ok(Value::Bool(true)),
                    "false" | "0" | "no" | "" => Ok(Value::Bool(false)),
                    _ => Err(TransformError::TypeMismatch {
                        op: "to_bool",
                        expected: "truthy/falsy value",
                        got: format!("\"{}\"", s),
                    }),
                },
                _ => Err(type_mismatch("to_bool", "string, number, or bool", value)),
            },
            TransformOp::ToJson => match value {
                Value::Null => Ok(Value::Null), // propagating
                _ => Ok(Value::String(
                    serde_json::to_string(value).unwrap_or_default(),
                )),
            },
            TransformOp::ParseJson => match value {
                Value::Null => Err(TransformError::NullInput { op: "parse_json" }),
                Value::String(s) => {
                    // Strip markdown code blocks: ```json\n...\n``` or ```\n...\n```
                    let cleaned = strip_markdown_code_block(s);
                    serde_json::from_str(&cleaned).map_err(|_| TransformError::TypeMismatch {
                        op: "parse_json",
                        expected: "valid JSON string",
                        got: format!("\"{}\"", truncate(s, 50)),
                    })
                }
                // Idempotent: already-parsed values pass through unchanged.
                // This handles auto-parsed exec outputs where Nika converts
                // JSON strings to values before transforms run.
                Value::Array(_) | Value::Object(_) | Value::Number(_) | Value::Bool(_) => {
                    Ok(value.clone())
                }
            },

            // ── Numeric ──────────────────────────────────────
            TransformOp::Round(decimals) => match value {
                Value::Null => Err(TransformError::NullInput { op: "round" }),
                Value::Number(n) => {
                    let f = n.as_f64().unwrap_or(0.0);
                    let d = decimals.unwrap_or(0);
                    let factor = 10f64.powi(d as i32);
                    let rounded = (f * factor).round() / factor;
                    Ok(serde_json::Number::from_f64(rounded)
                        .map(Value::Number)
                        .unwrap_or(Value::Null))
                }
                _ => Err(type_mismatch("round", "number", value)),
            },
            TransformOp::Abs => match value {
                Value::Null => Err(TransformError::NullInput { op: "abs" }),
                Value::Number(n) => {
                    if let Some(i) = n.as_i64() {
                        Ok(Value::Number(i.unsigned_abs().into()))
                    } else if let Some(f) = n.as_f64() {
                        Ok(serde_json::Number::from_f64(f.abs())
                            .map(Value::Number)
                            .unwrap_or(Value::Null))
                    } else {
                        Ok(value.clone())
                    }
                }
                _ => Err(type_mismatch("abs", "number", value)),
            },
            TransformOp::Ceil => match value {
                Value::Null => Err(TransformError::NullInput { op: "ceil" }),
                Value::Number(n) => {
                    let f = n.as_f64().unwrap_or(0.0);
                    Ok(Value::Number((f.ceil() as i64).into()))
                }
                _ => Err(type_mismatch("ceil", "number", value)),
            },
            TransformOp::Floor => match value {
                Value::Null => Err(TransformError::NullInput { op: "floor" }),
                Value::Number(n) => {
                    let f = n.as_f64().unwrap_or(0.0);
                    Ok(Value::Number((f.floor() as i64).into()))
                }
                _ => Err(type_mismatch("floor", "number", value)),
            },

            // ── Utility ──────────────────────────────────────
            TransformOp::Default(default_val) => {
                if value.is_null() {
                    Ok(default_val.clone())
                } else {
                    Ok(value.clone())
                }
            }
            TransformOp::TypeOf => {
                let name = value_type_name(value);
                Ok(Value::String(name.to_string()))
            }
            TransformOp::Join(sep) => match value {
                Value::Null => Err(TransformError::NullInput { op: "join" }),
                Value::Array(arr) => {
                    let strings: Vec<String> = arr
                        .iter()
                        .map(|v| match v {
                            Value::String(s) => s.clone(),
                            other => other.to_string(),
                        })
                        .collect();
                    Ok(Value::String(strings.join(sep)))
                }
                _ => Err(type_mismatch("join", "array", value)),
            },
            TransformOp::Split(sep) => match value {
                Value::Null => Err(TransformError::NullInput { op: "split" }),
                Value::String(s) => {
                    let parts: Vec<Value> = s
                        .split(sep.as_str())
                        .map(|p| Value::String(p.to_string()))
                        .collect();
                    Ok(Value::Array(parts))
                }
                _ => Err(type_mismatch("split", "string", value)),
            },
            TransformOp::Shell => {
                // Shell escaping — all types get escaped, not just strings
                match value {
                    Value::String(s) => Ok(Value::String(shell_escape(s))),
                    _ => Ok(Value::String(shell_escape(&value.to_string()))),
                }
            }
        }
    }
}

// ═══════════════════════════════════════════════════════════════
// Pipe Parser
// ═══════════════════════════════════════════════════════════════

/// Parse a single transform op from string.
///
/// Examples: `"upper"`, `"first(3)"`, `"join(', ')"`, `"default('N/A')"`, `"round(2)"`
fn parse_single_op(input: &str, full_input: &str) -> Result<TransformOp, TransformParseError> {
    let trimmed = input.trim();

    // Check for parameterized form: name(arg)
    if let Some(paren_pos) = trimmed.find('(') {
        let name = trimmed[..paren_pos].trim();
        let rest = &trimmed[paren_pos + 1..];
        let arg = rest
            .strip_suffix(')')
            .ok_or_else(|| TransformParseError {
                input: full_input.to_string(),
                reason: format!("unclosed parenthesis in '{}'", trimmed),
            })?
            .trim();

        match name {
            "first" => {
                let n: usize = arg.parse().map_err(|_| TransformParseError {
                    input: full_input.to_string(),
                    reason: format!("invalid argument for first(): '{}'", arg),
                })?;
                Ok(TransformOp::FirstN(n))
            }
            "last" => {
                let n: usize = arg.parse().map_err(|_| TransformParseError {
                    input: full_input.to_string(),
                    reason: format!("invalid argument for last(): '{}'", arg),
                })?;
                Ok(TransformOp::LastN(n))
            }
            "round" => {
                let d: u32 = arg.parse().map_err(|_| TransformParseError {
                    input: full_input.to_string(),
                    reason: format!("invalid argument for round(): '{}'", arg),
                })?;
                Ok(TransformOp::Round(Some(d)))
            }
            "join" => {
                let sep = strip_quotes(arg);
                Ok(TransformOp::Join(sep.to_string()))
            }
            "split" => {
                let sep = strip_quotes(arg);
                Ok(TransformOp::Split(sep.to_string()))
            }
            "default" => {
                let val = parse_default_value(arg).map_err(|reason| TransformParseError {
                    input: full_input.to_string(),
                    reason,
                })?;
                Ok(TransformOp::Default(val))
            }
            _ => Err(TransformParseError {
                input: full_input.to_string(),
                reason: format!("unknown transform: '{}'", name),
            }),
        }
    } else {
        // Simple name (no args)
        match trimmed {
            "upper" => Ok(TransformOp::Upper),
            "lower" => Ok(TransformOp::Lower),
            "trim" => Ok(TransformOp::Trim),
            "trim_start" => Ok(TransformOp::TrimStart),
            "trim_end" => Ok(TransformOp::TrimEnd),
            "length" => Ok(TransformOp::Length),
            "first" => Ok(TransformOp::First),
            "last" => Ok(TransformOp::Last),
            "keys" => Ok(TransformOp::Keys),
            "values" => Ok(TransformOp::Values),
            "flatten" => Ok(TransformOp::Flatten),
            "reverse" => Ok(TransformOp::Reverse),
            "sort" => Ok(TransformOp::Sort),
            "unique" => Ok(TransformOp::Unique),
            "compact" => Ok(TransformOp::Compact),
            "to_string" => Ok(TransformOp::ToString),
            "to_number" => Ok(TransformOp::ToNumber),
            "to_bool" => Ok(TransformOp::ToBool),
            "to_json" => Ok(TransformOp::ToJson),
            "parse_json" => Ok(TransformOp::ParseJson),
            "round" => Ok(TransformOp::Round(None)),
            "abs" => Ok(TransformOp::Abs),
            "ceil" => Ok(TransformOp::Ceil),
            "floor" => Ok(TransformOp::Floor),
            "type_of" => Ok(TransformOp::TypeOf),
            "shell" => Ok(TransformOp::Shell),
            _ => Err(TransformParseError {
                input: full_input.to_string(),
                reason: format!("unknown transform: '{}'", trimmed),
            }),
        }
    }
}

// ═══════════════════════════════════════════════════════════════
// Helpers
// ═══════════════════════════════════════════════════════════════

/// Get a human-readable type name for a JSON value.
fn value_type_name(value: &Value) -> &'static str {
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
fn type_mismatch(op: &'static str, expected: &'static str, got: &Value) -> TransformError {
    TransformError::TypeMismatch {
        op,
        expected,
        got: value_type_name(got).to_string(),
    }
}

/// Strip surrounding quotes (single or double) from a string argument.
fn strip_quotes(s: &str) -> &str {
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
fn parse_default_value(arg: &str) -> Result<Value, String> {
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
fn truncate(s: &str, max: usize) -> String {
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
fn strip_markdown_code_block(s: &str) -> String {
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

/// Shell-escape a string (single-quote wrapping).
fn shell_escape(s: &str) -> String {
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
        }
    }
}

// ═══════════════════════════════════════════════════════════════
// Tests
// ═══════════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    // ─────────────────────────────────────────────────────────────
    // Parse tests
    // ─────────────────────────────────────────────────────────────

    #[test]
    fn parse_upper() {
        let expr = TransformExpr::parse("upper").unwrap();
        assert_eq!(expr.ops.as_slice(), &[TransformOp::Upper]);
    }

    #[test]
    fn parse_lower() {
        let expr = TransformExpr::parse("lower").unwrap();
        assert_eq!(expr.ops.as_slice(), &[TransformOp::Lower]);
    }

    #[test]
    fn parse_trim() {
        let expr = TransformExpr::parse("trim").unwrap();
        assert_eq!(expr.ops.as_slice(), &[TransformOp::Trim]);
    }

    #[test]
    fn parse_length() {
        let expr = TransformExpr::parse("length").unwrap();
        assert_eq!(expr.ops.as_slice(), &[TransformOp::Length]);
    }

    #[test]
    fn parse_first() {
        let expr = TransformExpr::parse("first").unwrap();
        assert_eq!(expr.ops.as_slice(), &[TransformOp::First]);
    }

    #[test]
    fn parse_first_n() {
        let expr = TransformExpr::parse("first(3)").unwrap();
        assert_eq!(expr.ops.as_slice(), &[TransformOp::FirstN(3)]);
    }

    #[test]
    fn parse_last_n() {
        let expr = TransformExpr::parse("last(5)").unwrap();
        assert_eq!(expr.ops.as_slice(), &[TransformOp::LastN(5)]);
    }

    #[test]
    fn parse_join() {
        let expr = TransformExpr::parse("join(', ')").unwrap();
        assert_eq!(expr.ops.as_slice(), &[TransformOp::Join(", ".to_string())]);
    }

    #[test]
    fn parse_split() {
        let expr = TransformExpr::parse("split('/')").unwrap();
        assert_eq!(expr.ops.as_slice(), &[TransformOp::Split("/".to_string())]);
    }

    #[test]
    fn parse_default_string() {
        let expr = TransformExpr::parse("default('N/A')").unwrap();
        assert_eq!(
            expr.ops.as_slice(),
            &[TransformOp::Default(Value::String("N/A".to_string()))]
        );
    }

    #[test]
    fn parse_default_number() {
        let expr = TransformExpr::parse("default(42)").unwrap();
        assert_eq!(expr.ops.as_slice(), &[TransformOp::Default(json!(42))]);
    }

    #[test]
    fn parse_round() {
        let expr = TransformExpr::parse("round(2)").unwrap();
        assert_eq!(expr.ops.as_slice(), &[TransformOp::Round(Some(2))]);
    }

    #[test]
    fn parse_round_no_arg() {
        let expr = TransformExpr::parse("round").unwrap();
        assert_eq!(expr.ops.as_slice(), &[TransformOp::Round(None)]);
    }

    #[test]
    fn parse_shell() {
        let expr = TransformExpr::parse("shell").unwrap();
        assert_eq!(expr.ops.as_slice(), &[TransformOp::Shell]);
    }

    #[test]
    fn parse_to_json() {
        let expr = TransformExpr::parse("to_json").unwrap();
        assert_eq!(expr.ops.as_slice(), &[TransformOp::ToJson]);
    }

    #[test]
    fn parse_parse_json() {
        let expr = TransformExpr::parse("parse_json").unwrap();
        assert_eq!(expr.ops.as_slice(), &[TransformOp::ParseJson]);
    }

    #[test]
    fn parse_unknown() {
        let err = TransformExpr::parse("bogus").unwrap_err();
        assert!(err.reason.contains("unknown transform"));
    }

    #[test]
    fn parse_pipeline() {
        let expr = TransformExpr::parse("sort | unique | first(3)").unwrap();
        assert_eq!(
            expr.ops.as_slice(),
            &[
                TransformOp::Sort,
                TransformOp::Unique,
                TransformOp::FirstN(3),
            ]
        );
    }

    #[test]
    fn parse_empty() {
        let expr = TransformExpr::parse("").unwrap();
        assert!(expr.is_empty());
    }

    #[test]
    fn parse_single() {
        let expr = TransformExpr::parse("upper").unwrap();
        assert_eq!(expr.ops.len(), 1);
    }

    // ─────────────────────────────────────────────────────────────
    // Apply tests — String
    // ─────────────────────────────────────────────────────────────

    #[test]
    fn apply_upper_string() {
        let result = TransformOp::Upper.apply(&json!("hello")).unwrap();
        assert_eq!(result, json!("HELLO"));
    }

    #[test]
    fn apply_upper_non_string() {
        let err = TransformOp::Upper.apply(&json!(42)).unwrap_err();
        assert!(matches!(err, TransformError::TypeMismatch { .. }));
    }

    #[test]
    fn apply_upper_null() {
        let err = TransformOp::Upper.apply(&Value::Null).unwrap_err();
        assert!(matches!(err, TransformError::NullInput { .. }));
    }

    #[test]
    fn apply_lower_string() {
        let result = TransformOp::Lower.apply(&json!("HELLO")).unwrap();
        assert_eq!(result, json!("hello"));
    }

    #[test]
    fn apply_trim() {
        let result = TransformOp::Trim.apply(&json!(" hello ")).unwrap();
        assert_eq!(result, json!("hello"));
    }

    #[test]
    fn apply_trim_start() {
        let result = TransformOp::TrimStart.apply(&json!("  hello  ")).unwrap();
        assert_eq!(result, json!("hello  "));
    }

    #[test]
    fn apply_trim_end() {
        let result = TransformOp::TrimEnd.apply(&json!("  hello  ")).unwrap();
        assert_eq!(result, json!("  hello"));
    }

    // ─────────────────────────────────────────────────────────────
    // Apply tests — Collection
    // ─────────────────────────────────────────────────────────────

    #[test]
    fn apply_length_array() {
        let result = TransformOp::Length.apply(&json!([1, 2, 3])).unwrap();
        assert_eq!(result, json!(3));
    }

    #[test]
    fn apply_length_string() {
        let result = TransformOp::Length.apply(&json!("abc")).unwrap();
        assert_eq!(result, json!(3));
    }

    #[test]
    fn apply_length_object() {
        let result = TransformOp::Length.apply(&json!({"a": 1, "b": 2})).unwrap();
        assert_eq!(result, json!(2));
    }

    #[test]
    fn apply_length_null() {
        let result = TransformOp::Length.apply(&Value::Null).unwrap();
        assert_eq!(result, Value::Null); // propagating
    }

    #[test]
    fn apply_first_array() {
        let result = TransformOp::First.apply(&json!([1, 2, 3])).unwrap();
        assert_eq!(result, json!(1));
    }

    #[test]
    fn apply_first_empty() {
        let result = TransformOp::First.apply(&json!([])).unwrap();
        assert_eq!(result, Value::Null);
    }

    #[test]
    fn apply_last_array() {
        let result = TransformOp::Last.apply(&json!([1, 2, 3])).unwrap();
        assert_eq!(result, json!(3));
    }

    #[test]
    fn apply_first_n() {
        let result = TransformOp::FirstN(3)
            .apply(&json!([1, 2, 3, 4, 5]))
            .unwrap();
        assert_eq!(result, json!([1, 2, 3]));
    }

    #[test]
    fn apply_last_n() {
        let result = TransformOp::LastN(2)
            .apply(&json!([1, 2, 3, 4, 5]))
            .unwrap();
        assert_eq!(result, json!([4, 5]));
    }

    #[test]
    fn apply_keys() {
        let result = TransformOp::Keys.apply(&json!({"a": 1, "b": 2})).unwrap();
        // serde_json::Map preserves insertion order
        assert_eq!(result, json!(["a", "b"]));
    }

    #[test]
    fn apply_keys_null() {
        let result = TransformOp::Keys.apply(&Value::Null).unwrap();
        assert_eq!(result, Value::Null); // propagating
    }

    #[test]
    fn apply_values() {
        let result = TransformOp::Values.apply(&json!({"a": 1, "b": 2})).unwrap();
        assert_eq!(result, json!([1, 2]));
    }

    #[test]
    fn apply_sort() {
        let result = TransformOp::Sort.apply(&json!([3, 1, 2])).unwrap();
        assert_eq!(result, json!([1, 2, 3]));
    }

    #[test]
    fn apply_unique() {
        let result = TransformOp::Unique.apply(&json!([1, 2, 2, 3])).unwrap();
        assert_eq!(result, json!([1, 2, 3]));
    }

    #[test]
    fn apply_compact() {
        let result = TransformOp::Compact
            .apply(&json!([1, null, 2, null]))
            .unwrap();
        assert_eq!(result, json!([1, 2]));
    }

    #[test]
    fn apply_flatten() {
        let result = TransformOp::Flatten.apply(&json!([[1, 2], [3]])).unwrap();
        assert_eq!(result, json!([1, 2, 3]));
    }

    #[test]
    fn apply_reverse() {
        let result = TransformOp::Reverse.apply(&json!([1, 2, 3])).unwrap();
        assert_eq!(result, json!([3, 2, 1]));
    }

    // ─────────────────────────────────────────────────────────────
    // Apply tests — Type conversion
    // ─────────────────────────────────────────────────────────────

    #[test]
    fn apply_to_string() {
        let result = TransformOp::ToString.apply(&json!(42)).unwrap();
        assert_eq!(result, json!("42"));
    }

    #[test]
    fn apply_to_string_null() {
        let result = TransformOp::ToString.apply(&Value::Null).unwrap();
        assert_eq!(result, Value::Null); // propagating
    }

    #[test]
    fn apply_to_number() {
        let result = TransformOp::ToNumber.apply(&json!("42")).unwrap();
        assert_eq!(result, json!(42));
    }

    #[test]
    fn apply_to_number_float() {
        let result = TransformOp::ToNumber.apply(&json!("3.12")).unwrap();
        assert_eq!(result, json!(3.12));
    }

    #[test]
    fn apply_to_bool_number() {
        assert_eq!(TransformOp::ToBool.apply(&json!(1)).unwrap(), json!(true));
        assert_eq!(TransformOp::ToBool.apply(&json!(0)).unwrap(), json!(false));
    }

    #[test]
    fn apply_to_bool_string() {
        assert_eq!(
            TransformOp::ToBool.apply(&json!("true")).unwrap(),
            json!(true)
        );
        assert_eq!(
            TransformOp::ToBool.apply(&json!("false")).unwrap(),
            json!(false)
        );
    }

    #[test]
    fn apply_to_json() {
        let result = TransformOp::ToJson.apply(&json!([1, 2])).unwrap();
        assert_eq!(result, json!("[1,2]"));
    }

    #[test]
    fn apply_parse_json() {
        let result = TransformOp::ParseJson.apply(&json!(r#"{"a":1}"#)).unwrap();
        assert_eq!(result, json!({"a": 1}));
    }

    // ─────────────────────────────────────────────────────────────
    // Apply tests — Numeric
    // ─────────────────────────────────────────────────────────────

    #[test]
    fn apply_round() {
        let result = TransformOp::Round(Some(2)).apply(&json!(4.56789)).unwrap();
        assert_eq!(result, json!(4.57));
    }

    #[test]
    fn apply_round_no_decimals() {
        let result = TransformOp::Round(None).apply(&json!(3.7)).unwrap();
        assert_eq!(result, json!(4.0));
    }

    #[test]
    fn apply_abs() {
        let result = TransformOp::Abs.apply(&json!(-5)).unwrap();
        assert_eq!(result, json!(5));
    }

    #[test]
    fn apply_abs_float() {
        let result = TransformOp::Abs.apply(&json!(-3.12)).unwrap();
        assert_eq!(result, json!(3.12));
    }

    #[test]
    fn apply_ceil() {
        let result = TransformOp::Ceil.apply(&json!(3.2)).unwrap();
        assert_eq!(result, json!(4));
    }

    #[test]
    fn apply_floor() {
        let result = TransformOp::Floor.apply(&json!(3.8)).unwrap();
        assert_eq!(result, json!(3));
    }

    // ─────────────────────────────────────────────────────────────
    // Apply tests — Utility
    // ─────────────────────────────────────────────────────────────

    #[test]
    fn apply_join() {
        let result = TransformOp::Join(", ".to_string())
            .apply(&json!(["a", "b"]))
            .unwrap();
        assert_eq!(result, json!("a, b"));
    }

    #[test]
    fn apply_split() {
        let result = TransformOp::Split("/".to_string())
            .apply(&json!("a/b/c"))
            .unwrap();
        assert_eq!(result, json!(["a", "b", "c"]));
    }

    #[test]
    fn apply_default_with_null() {
        let result = TransformOp::Default(json!("N/A"))
            .apply(&Value::Null)
            .unwrap();
        assert_eq!(result, json!("N/A"));
    }

    #[test]
    fn apply_default_with_value() {
        let result = TransformOp::Default(json!("N/A"))
            .apply(&json!("hello"))
            .unwrap();
        assert_eq!(result, json!("hello"));
    }

    #[test]
    fn apply_typeof() {
        assert_eq!(
            TransformOp::TypeOf.apply(&json!(42)).unwrap(),
            json!("number")
        );
        assert_eq!(
            TransformOp::TypeOf.apply(&json!("x")).unwrap(),
            json!("string")
        );
        assert_eq!(
            TransformOp::TypeOf.apply(&Value::Null).unwrap(),
            json!("null")
        );
        assert_eq!(
            TransformOp::TypeOf.apply(&json!(true)).unwrap(),
            json!("boolean")
        );
        assert_eq!(
            TransformOp::TypeOf.apply(&json!([1])).unwrap(),
            json!("array")
        );
        assert_eq!(
            TransformOp::TypeOf.apply(&json!({"a": 1})).unwrap(),
            json!("object")
        );
    }

    #[test]
    fn apply_shell() {
        let result = TransformOp::Shell.apply(&json!("hello world")).unwrap();
        assert_eq!(result, json!("'hello world'"));
    }

    // ─────────────────────────────────────────────────────────────
    // Pipeline tests
    // ─────────────────────────────────────────────────────────────

    #[test]
    fn pipeline_sort_unique() {
        let expr = TransformExpr::parse("sort | unique").unwrap();
        let result = expr.apply(&json!([3, 1, 2, 1])).unwrap();
        assert_eq!(result, json!([1, 2, 3]));
    }

    #[test]
    fn pipeline_sort_first_n() {
        let expr = TransformExpr::parse("sort | first(2)").unwrap();
        let result = expr.apply(&json!([3, 1, 2])).unwrap();
        assert_eq!(result, json!([1, 2]));
    }

    #[test]
    fn pipeline_upper_trim() {
        let expr = TransformExpr::parse("trim | upper").unwrap();
        let result = expr.apply(&json!(" hello ")).unwrap();
        assert_eq!(result, json!("HELLO"));
    }

    #[test]
    fn pipeline_empty() {
        let expr = TransformExpr::parse("").unwrap();
        let result = expr.apply(&json!("unchanged")).unwrap();
        assert_eq!(result, json!("unchanged"));
    }

    #[test]
    fn pipeline_single() {
        let expr = TransformExpr::parse("upper").unwrap();
        assert_eq!(expr.ops.len(), 1);
    }

    #[test]
    fn pipeline_default_then_upper() {
        let expr = TransformExpr::parse("default('unknown') | upper").unwrap();
        let result = expr.apply(&Value::Null).unwrap();
        assert_eq!(result, json!("UNKNOWN"));
    }

    // ─────────────────────────────────────────────────────────────
    // Display
    // ─────────────────────────────────────────────────────────────

    #[test]
    fn display_ops() {
        assert_eq!(TransformOp::Upper.to_string(), "upper");
        assert_eq!(TransformOp::FirstN(3).to_string(), "first(3)");
        assert_eq!(
            TransformOp::Join(", ".to_string()).to_string(),
            "join(', ')"
        );
        assert_eq!(TransformOp::Round(Some(2)).to_string(), "round(2)");
        assert_eq!(TransformOp::Round(None).to_string(), "round");
        assert_eq!(
            TransformOp::Default(json!("N/A")).to_string(),
            "default(\"N/A\")"
        );
    }

    // ─────────────────────────────────────────────────────────────
    // Error display
    // ─────────────────────────────────────────────────────────────

    #[test]
    fn error_display_parse() {
        let err = TransformParseError {
            input: "bogus".to_string(),
            reason: "unknown transform: 'bogus'".to_string(),
        };
        assert!(err.to_string().contains("NIKA-151"));
    }

    #[test]
    fn error_display_type_mismatch() {
        let err = TransformError::TypeMismatch {
            op: "upper",
            expected: "string",
            got: "number".to_string(),
        };
        assert!(err.to_string().contains("NIKA-152"));
    }

    #[test]
    fn error_display_null_input() {
        let err = TransformError::NullInput { op: "sort" };
        assert!(err.to_string().contains("NIKA-153"));
    }

    // ─────────────────────────────────────────────────────────────
    // Edge cases
    // ─────────────────────────────────────────────────────────────

    #[test]
    fn parse_default_bool() {
        let expr = TransformExpr::parse("default(true)").unwrap();
        assert_eq!(expr.ops.as_slice(), &[TransformOp::Default(json!(true))]);
    }

    #[test]
    fn parse_default_null() {
        let expr = TransformExpr::parse("default(null)").unwrap();
        assert_eq!(expr.ops.as_slice(), &[TransformOp::Default(Value::Null)]);
    }

    #[test]
    fn parse_default_array() {
        let expr = TransformExpr::parse("default([])").unwrap();
        assert_eq!(expr.ops.as_slice(), &[TransformOp::Default(json!([]))]);
    }

    #[test]
    fn first_n_larger_than_array() {
        let result = TransformOp::FirstN(10).apply(&json!([1, 2, 3])).unwrap();
        assert_eq!(result, json!([1, 2, 3])); // takes what's available
    }

    #[test]
    fn last_n_larger_than_array() {
        let result = TransformOp::LastN(10).apply(&json!([1, 2, 3])).unwrap();
        assert_eq!(result, json!([1, 2, 3]));
    }

    #[test]
    fn flatten_mixed() {
        let result = TransformOp::Flatten
            .apply(&json!([[1, 2], 3, [4]]))
            .unwrap();
        assert_eq!(result, json!([1, 2, 3, 4]));
    }

    #[test]
    fn unclosed_paren() {
        let err = TransformExpr::parse("first(3").unwrap_err();
        assert!(err.reason.contains("unclosed parenthesis"));
    }

    #[test]
    fn join_mixed_types() {
        let result = TransformOp::Join(", ".to_string())
            .apply(&json!(["a", 1, true]))
            .unwrap();
        assert_eq!(result, json!("a, 1, true"));
    }

    #[test]
    fn parse_json_invalid() {
        let err = TransformOp::ParseJson
            .apply(&json!("not json"))
            .unwrap_err();
        assert!(matches!(err, TransformError::TypeMismatch { .. }));
    }

    #[test]
    fn to_number_invalid() {
        let err = TransformOp::ToNumber.apply(&json!("abc")).unwrap_err();
        assert!(matches!(err, TransformError::TypeMismatch { .. }));
    }

    #[test]
    fn to_bool_invalid_string() {
        let err = TransformOp::ToBool.apply(&json!("maybe")).unwrap_err();
        assert!(matches!(err, TransformError::TypeMismatch { .. }));
    }

    // ─────────────────────────────────────────────────────────────
    // Bug fix tests
    // ─────────────────────────────────────────────────────────────

    #[test]
    fn first_n_on_object_serializes_and_truncates() {
        // BUG 3: first(N) on an object should serialize to JSON and truncate
        let obj = json!({"links": [1, 2, 3], "count": 3});
        let result = TransformOp::FirstN(10).apply(&obj).unwrap();
        // Should be a truncated JSON string
        assert!(result.is_string());
        let s = result.as_str().unwrap();
        assert_eq!(s.len(), 10);
    }

    #[test]
    fn first_n_on_object_full() {
        // first(N) with N larger than JSON length returns full JSON
        let obj = json!({"a": 1});
        let result = TransformOp::FirstN(1000).apply(&obj).unwrap();
        assert!(result.is_string());
        assert_eq!(result.as_str().unwrap(), r#"{"a":1}"#);
    }

    #[test]
    fn first_n_on_string_truncates() {
        let result = TransformOp::FirstN(5).apply(&json!("hello world")).unwrap();
        assert_eq!(result, json!("hello"));
    }

    #[test]
    fn parse_json_idempotent_on_array() {
        // BUG 7: parse_json on an already-parsed array should be a no-op
        let arr = json!([1, 2, 3]);
        let result = TransformOp::ParseJson.apply(&arr).unwrap();
        assert_eq!(result, json!([1, 2, 3]));
    }

    #[test]
    fn parse_json_idempotent_on_object() {
        // BUG 7: parse_json on an already-parsed object should be a no-op
        let obj = json!({"key": "value"});
        let result = TransformOp::ParseJson.apply(&obj).unwrap();
        assert_eq!(result, json!({"key": "value"}));
    }

    #[test]
    fn parse_json_idempotent_on_number_and_bool() {
        // parse_json on auto-parsed primitives should be a no-op
        assert_eq!(TransformOp::ParseJson.apply(&json!(42)).unwrap(), json!(42));
        assert_eq!(
            TransformOp::ParseJson.apply(&json!(true)).unwrap(),
            json!(true)
        );
    }

    #[test]
    fn parse_json_strips_markdown_code_block() {
        let input = json!("```json\n{\"name\": \"test\"}\n```");
        let result = TransformOp::ParseJson.apply(&input).unwrap();
        assert_eq!(result, json!({"name": "test"}));
    }

    #[test]
    fn parse_json_strips_generic_code_block() {
        let input = json!("```\n[1, 2, 3]\n```");
        let result = TransformOp::ParseJson.apply(&input).unwrap();
        assert_eq!(result, json!([1, 2, 3]));
    }

    #[test]
    fn parse_json_handles_bare_json() {
        let input = json!("{\"key\": \"value\"}");
        let result = TransformOp::ParseJson.apply(&input).unwrap();
        assert_eq!(result, json!({"key": "value"}));
    }

    #[test]
    fn parse_json_strips_whitespace_around_code_block() {
        let input = json!("  ```json\n  [\"a\", \"b\"]\n  ```  ");
        let result = TransformOp::ParseJson.apply(&input).unwrap();
        assert_eq!(result, json!(["a", "b"]));
    }

    #[test]
    fn to_json_then_length_returns_char_count() {
        // BUG 8: to_json | length should return string character count
        let obj = json!({"countries": ["FR", "US"]});
        let json_str = TransformOp::ToJson.apply(&obj).unwrap();
        assert!(json_str.is_string());
        let length = TransformOp::Length.apply(&json_str).unwrap();
        // Should be the character count of the JSON string, not 1
        assert!(length.as_u64().unwrap() > 1);
    }

    /// Bug 30: |length must return character count, not byte count for Unicode.
    #[test]
    fn regression_bug30_length_unicode_chars_not_bytes() {
        // "日本語" is 3 characters but 9 bytes in UTF-8
        let result = TransformOp::Length.apply(&json!("日本語")).unwrap();
        assert_eq!(
            result,
            json!(3),
            "|length on Unicode string must count chars, not bytes"
        );
    }

    /// Bug 30: additional Unicode edge cases for |length.
    #[test]
    fn regression_bug30_length_unicode_emoji() {
        // Emoji: "👋🌍" is 2 characters but 8 bytes
        let result = TransformOp::Length.apply(&json!("👋🌍")).unwrap();
        assert_eq!(result, json!(2), "|length on emoji string must count chars");
    }

    /// Bug 30: |length on ASCII should remain unchanged.
    #[test]
    fn regression_bug30_length_ascii_unchanged() {
        let result = TransformOp::Length.apply(&json!("abc")).unwrap();
        assert_eq!(result, json!(3), "|length on ASCII string is still correct");
    }

    /// Bug 46: |sort must use numeric ordering for numbers, not lexicographic.
    #[test]
    fn regression_bug46_sort_numeric_ordering() {
        let result = TransformOp::Sort.apply(&json!([1, 10, 2, 20, 3])).unwrap();
        assert_eq!(
            result,
            json!([1, 2, 3, 10, 20]),
            "|sort on numbers must use numeric ordering, not lexicographic"
        );
    }

    /// Bug 46: |sort with mixed types (numbers and strings).
    #[test]
    fn regression_bug46_sort_mixed_types() {
        let result = TransformOp::Sort.apply(&json!([10, 2, "b", "a"])).unwrap();
        assert_eq!(result, json!([2, 10, "a", "b"]));
    }

    /// Bug 46: |sort preserves string lexicographic ordering.
    #[test]
    fn regression_bug46_sort_strings_unchanged() {
        let result = TransformOp::Sort
            .apply(&json!(["banana", "apple", "cherry"]))
            .unwrap();
        assert_eq!(result, json!(["apple", "banana", "cherry"]));
    }

    /// Bug 46: |sort with floats.
    #[test]
    fn regression_bug46_sort_floats() {
        let result = TransformOp::Sort
            .apply(&json!([1.5, 0.1, 2.3, 0.9]))
            .unwrap();
        assert_eq!(result, json!([0.1, 0.9, 1.5, 2.3]));
    }
}
