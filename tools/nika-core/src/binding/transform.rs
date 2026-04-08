//! Transform Engine
//!
//! Pipeline transforms applied to binding values.
//! Transforms are chained with `|` pipes: `sort | unique | first(3)`
//!
//! # Categories (64 transforms)
//!
//! | Category | Ops |
//! |----------|-----|
//! | String | `upper`, `lower`, `trim`, `trim_start`, `trim_end`, `replace(A, B)`, `truncate(N)` |
//! | Collection | `length`, `first`, `last`, `first(N)`, `last(N)`, `keys`, `values`, `flatten`, `reverse`, `sort`, `unique`, `compact` |
//! | Data | `pluck(F)`, `where(F, [op], V)`, `pick(F…)`, `omit(F…)`, `sort_by(F)`, `group_by(F)`, `merge`, `merge(obj)` |
//! | Aggregation | `add`, `min`, `max`, `min_by(F)`, `max_by(F)`, `sum`, `avg` |
//! | Introspection | `has(K)` |
//! | Regex | `regex(P)` |
//! | Type conversion | `to_string`, `to_number`, `to_bool`, `to_json`, `parse_json` |
//! | Numeric | `round(N)`, `abs`, `ceil`, `floor` |
//! | Logic | `not` |
//! | Encoding | `base64_encode`, `base64_decode` (text-only, use `nika:import` for binary) |
//! | Predicate | `starts_with(S)`, `ends_with(S)`, `contains(S)` |
//! | Hash | `content_hash` |
//! | URL | `url_host`, `url_path`, `url_without_query`, `url_normalize`, `unique_urls` |
//! | Utility | `default(V)`, `type_of`, `join(S)`, `split(S)`, `shell`, `slice(S, E)` |
//!
//! # Null Handling
//!
//! - **Propagating**: null in → null out (`length`, `keys`, `type_of`, `to_string`, `to_json`)
//! - **Failing**: null in → NIKA-153 error (`upper`, `lower`, `sort`, etc.)
//! - Use `default()` or `??` to handle nulls safely
//!
//! # Dot-Path Access
//!
//! Data transforms (`pluck`, `where`, `sort_by`, `group_by`) support dot-paths
//! for nested field access: `pluck("address.city")`, `where("meta.score", "gt", 80)`

use serde_json::Value;
use smallvec::SmallVec;
use std::fmt;
use std::num::NonZeroUsize;
use std::sync::{Arc, LazyLock, Mutex};

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
    ParseYaml,

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

    // -- URL --
    UrlHost,
    UrlPath,
    UrlWithoutQuery,
    UrlNormalize,

    // -- Slicing --
    Slice(usize, usize),

    // -- Data (array/object manipulation) --
    Pluck(String),
    /// Filter array: `where("field", "value")` (eq) or `where("field", "op", value)`.
    /// Operators: eq, ne, gt, lt, gte, lte, contains, starts_with, ends_with.
    Where(String, String, Value),
    Pick(Vec<String>),
    Omit(Vec<String>),
    SortBy(String),
    GroupBy(String),
    /// Deep merge: no-arg = merge array of objects; with-arg = overlay param onto input object.
    Merge(Option<Value>),
    Regex(String),

    // -- Encoding --
    Base64Encode,
    Base64Decode,

    // -- Predicate (returns bool) --
    StartsWith(String),
    EndsWith(String),
    Contains(String),

    // -- Hashing --
    ContentHash,

    // -- URL dedup --
    UniqueUrls,

    // -- String manipulation --
    /// String replacement: `replace("old", "new")`
    Replace(String, String),
    /// Truncate string to N chars: `truncate(100)`
    Truncate(usize),

    // -- Aggregation --
    /// Sum numbers or concat arrays: `add`
    Add,
    /// Minimum of numeric array: `min`
    Min,
    /// Maximum of numeric array: `max`
    Max,
    /// Minimum by field: `min_by("score")`
    MinBy(String),
    /// Maximum by field: `max_by("score")`
    MaxBy(String),
    /// Sum of numeric array: `sum` (numbers only — use `add` for string/array concat)
    Sum,
    /// Average of numeric array: `avg`
    Avg,

    // -- Introspection --
    /// Check if object has key: `has("name")`
    Has(String),

    // -- Logic --
    /// Boolean negation: `not`
    Not,

    // -- jq expression --
    /// Full jq expression: `jq("[.[] | select(.score > 80)]")`
    Jq(String),

    // -- Security escaping (Nika Shield) --
    /// HTML entity escaping: `< > & " '`
    HtmlEscape,
    /// Markdown special character escaping
    MdEscape,
    /// Aggressive sanitization: strip common injection patterns
    Sanitize,
}

/// A chain of transform operations: `sort | unique | first(3)`
#[derive(Debug, Clone, PartialEq)]
pub struct TransformExpr {
    pub ops: SmallVec<[TransformOp; 2]>,
}

/// All known transform names (for "did you mean?" suggestions).
pub static KNOWN_TRANSFORM_NAMES: &[&str] = &[
    // Simple (no-arg)
    "upper",
    "lower",
    "trim",
    "trim_start",
    "trim_end",
    "length",
    "first",
    "last",
    "keys",
    "values",
    "flatten",
    "reverse",
    "sort",
    "unique",
    "compact",
    "to_string",
    "to_number",
    "to_bool",
    "to_json",
    "parse_json",
    "parse_yaml",
    "round",
    "abs",
    "ceil",
    "floor",
    "type_of",
    "shell",
    "url_host",
    "url_path",
    "url_without_query",
    "url_normalize",
    "merge",
    "base64_encode",
    "base64_decode",
    "content_hash",
    "unique_urls",
    "add",
    "min",
    "max",
    "sum",
    "avg",
    "not",
    // Parameterized
    "join",
    "split",
    "default",
    "slice",
    "pluck",
    "where",
    "pick",
    "omit",
    "sort_by",
    "group_by",
    "regex",
    "starts_with",
    "ends_with",
    "contains",
    "replace",
    "truncate",
    "has",
    "min_by",
    "max_by",
    "jq",
    // Security (Nika Shield)
    "html_escape",
    "md_escape",
    "sanitize",
];

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
                // Hint about extract: article returning an object
                if *expected == "string" && got == "object" {
                    write!(
                        f,
                        ". If this is from extract: article, use $task.text_content \
                         instead — extract: article returns an object with title, \
                         content, text_content, excerpt, byline fields"
                    )?;
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

/// Split a pipe-separated transform chain while respecting parentheses and quotes.
/// e.g. `join(" | ")` is one segment, not split on the inner `|`.
fn split_pipe_respecting_parens(input: &str) -> Vec<&str> {
    let mut result = Vec::new();
    let mut depth: u32 = 0;
    let mut quote_char: Option<char> = None;
    let mut start = 0;
    for (i, c) in input.char_indices() {
        match c {
            // Track quotes only inside parenthesized arguments.
            // This handles `join(", ")` and `join(' | ')` correctly
            // while ignoring top-level apostrophes (`it's a test | upper`).
            '"' | '\'' if depth > 0 => {
                if quote_char == Some(c) {
                    quote_char = None; // Close matching quote
                } else if quote_char.is_none() {
                    quote_char = Some(c); // Open new quote
                }
                // Mismatched quote (e.g. ' inside "...") → ignored
            }
            '(' if quote_char.is_none() => depth += 1,
            ')' if depth > 0 => {
                // Auto-close any unclosed quote at paren boundary.
                // Handles `filter(it's)` where the apostrophe is not a delimiter.
                quote_char = None;
                depth -= 1;
            }
            '|' if depth == 0 => {
                result.push(&input[start..i]);
                start = i + 1;
            }
            _ => {}
        }
    }
    result.push(&input[start..]);
    result
}

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

        let ops: SmallVec<[TransformOp; 2]> = split_pipe_respecting_parens(trimmed)
            .iter()
            .map(|s| s.trim())
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

    /// Returns true if the chain contains a `default()` transform.
    ///
    /// Used by binding resolution to allow `$env.MISSING | default("x")`
    /// even when the source returns `None` (missing, not null).
    pub fn has_default(&self) -> bool {
        self.ops
            .iter()
            .any(|op| matches!(op, TransformOp::Default(_)))
    }
}

// ═══════════════════════════════════════════════════════════════
// TransformOp::apply
// ═══════════════════════════════════════════════════════════════

impl TransformOp {
    /// Apply this single transform to a JSON value.
    ///
    /// # Null handling
    ///
    /// Two strategies exist:
    /// - **Propagating**: `length`, `keys`, `values`, `to_string`, `to_json`,
    ///   `type_of` return `Value::Null` on null input (marked `// propagating`).
    /// - **Strict**: `upper`, `trim`, `first`, `last`, etc. return `NIKA-153`
    ///   error on null input. Users should chain `| default("fallback")` before
    ///   strict transforms if null is possible.
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
                    // serde_json::Value is always serializable — unwrap is safe
                    let json = serde_json::to_string(value).expect("Value is serializable");
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
                Value::String(s) => {
                    // Last N characters (Unicode-safe)
                    let chars: Vec<char> = s.chars().collect();
                    let skip = chars.len().saturating_sub(*n);
                    let truncated: String = chars[skip..].iter().collect();
                    Ok(Value::String(truncated))
                }
                Value::Object(_) => {
                    let json = serde_json::to_string(value).expect("Value is serializable");
                    let chars: Vec<char> = json.chars().collect();
                    let skip = chars.len().saturating_sub(*n);
                    let truncated: String = chars[skip..].iter().collect();
                    Ok(Value::String(truncated))
                }
                _ => Err(type_mismatch("last", "array, string, or object", value)),
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
                Value::Null => Ok(Value::Null), // propagating (symmetric with keys)
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
                    let compacted: Vec<Value> = arr
                        .iter()
                        .filter(|v| !v.is_null() && !matches!(v, Value::String(s) if s.is_empty()))
                        .cloned()
                        .collect();
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
                    // serde_json::Value is always serializable — unwrap is safe
                    serde_json::to_string(value).expect("Value is serializable"),
                )),
            },
            TransformOp::ParseJson => match value {
                Value::Null => Err(TransformError::NullInput { op: "parse_json" }),
                Value::String(s) => {
                    // Strip markdown code blocks: ```json\n...\n``` or ```\n...\n```
                    let cleaned = strip_markdown_code_block(s);
                    // Strip UTF-8 BOM and NUL bytes that exec output may contain
                    let cleaned = strip_bom_and_control_chars(&cleaned);
                    serde_json::from_str(&cleaned).map_err(|e| TransformError::TypeMismatch {
                        op: "parse_json",
                        expected: "valid JSON string",
                        got: format!("{} (input: \"{}\")", e, truncate(s, 80)),
                    })
                }
                // Idempotent: already-parsed values pass through unchanged.
                // This handles auto-parsed exec outputs where Nika converts
                // JSON strings to values before transforms run.
                Value::Array(_) | Value::Object(_) | Value::Number(_) | Value::Bool(_) => {
                    Ok(value.clone())
                }
            },

            TransformOp::ParseYaml => match value {
                Value::Null => Err(TransformError::NullInput { op: "parse_yaml" }),
                Value::String(s) => {
                    // Strip markdown code blocks: ```yaml\n...\n``` or ```\n...\n```
                    let cleaned = strip_markdown_code_block(s);
                    let cleaned = strip_bom_and_control_chars(&cleaned);
                    crate::serde_yaml::from_str::<Value>(&cleaned).map_err(|e| {
                        TransformError::TypeMismatch {
                            op: "parse_yaml",
                            expected: "valid YAML string",
                            got: format!("{} (input: \"{}\")", e, truncate(s, 80)),
                        }
                    })
                }
                // Idempotent: already-parsed values pass through unchanged.
                Value::Array(_) | Value::Object(_) | Value::Number(_) | Value::Bool(_) => {
                    Ok(value.clone())
                }
            },

            // ── Numeric ──────────────────────────────────────
            TransformOp::Round(decimals) => match value {
                Value::Null => Err(TransformError::NullInput { op: "round" }),
                Value::Number(n) => {
                    let f = n.as_f64().unwrap_or(0.0);
                    if f.is_nan() || f.is_infinite() {
                        return Ok(Value::Null);
                    }
                    let d = decimals.unwrap_or(0);
                    if d == 0 {
                        // No decimals → return integer (consistent with ceil/floor)
                        Ok(Value::Number((f.round() as i64).into()))
                    } else {
                        let factor = 10f64.powi(d as i32);
                        let rounded = (f * factor).round() / factor;
                        Ok(serde_json::Number::from_f64(rounded)
                            .map(Value::Number)
                            .unwrap_or(Value::Null))
                    }
                }
                _ => Err(type_mismatch("round", "number", value)),
            },
            TransformOp::Abs => match value {
                Value::Null => Err(TransformError::NullInput { op: "abs" }),
                Value::Number(n) => {
                    if let Some(i) = n.as_i64() {
                        Ok(Value::Number(i.unsigned_abs().into()))
                    } else if let Some(f) = n.as_f64() {
                        if f.is_nan() || f.is_infinite() {
                            return Ok(Value::Null);
                        }
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
                    if f.is_nan() || f.is_infinite() {
                        return Ok(Value::Null);
                    }
                    Ok(Value::Number((f.ceil() as i64).into()))
                }
                _ => Err(type_mismatch("ceil", "number", value)),
            },
            TransformOp::Floor => match value {
                Value::Null => Err(TransformError::NullInput { op: "floor" }),
                Value::Number(n) => {
                    let f = n.as_f64().unwrap_or(0.0);
                    if f.is_nan() || f.is_infinite() {
                        return Ok(Value::Null);
                    }
                    Ok(Value::Number((f.floor() as i64).into()))
                }
                _ => Err(type_mismatch("floor", "number", value)),
            },

            // ── Utility ──────────────────────────────────────
            TransformOp::Default(default_val) => match value {
                Value::Null => Ok(default_val.clone()),
                Value::String(s) if s.is_empty() => Ok(default_val.clone()),
                _ => Ok(value.clone()),
            },
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
                // Shell escaping — all types get escaped, not just strings.
                // Null input is an error (not the string 'null').
                match value {
                    Value::Null => Err(TransformError::NullInput { op: "shell" }),
                    Value::String(s) => Ok(Value::String(shell_escape(s))),
                    _ => Ok(Value::String(shell_escape(&value.to_string()))),
                }
            }

            // ── URL ──────────────────────────────────────────
            TransformOp::UrlHost => match value {
                Value::Null => Err(TransformError::NullInput { op: "url_host" }),
                Value::String(s) => {
                    let parsed = url::Url::parse(s).map_err(|_| TransformError::TypeMismatch {
                        op: "url_host",
                        expected: "valid URL",
                        got: "invalid URL".to_string(),
                    })?;
                    let host = parsed.host_str().unwrap_or_default();
                    // Strip IPv6 brackets: [::1] → ::1
                    let host = host
                        .strip_prefix('[')
                        .and_then(|h| h.strip_suffix(']'))
                        .unwrap_or(host);
                    Ok(Value::String(host.to_string()))
                }
                _ => Err(type_mismatch("url_host", "string", value)),
            },
            TransformOp::UrlPath => match value {
                Value::Null => Err(TransformError::NullInput { op: "url_path" }),
                Value::String(s) => {
                    let parsed = url::Url::parse(s).map_err(|_| TransformError::TypeMismatch {
                        op: "url_path",
                        expected: "valid URL",
                        got: "invalid URL".to_string(),
                    })?;
                    Ok(Value::String(parsed.path().to_string()))
                }
                _ => Err(type_mismatch("url_path", "string", value)),
            },
            TransformOp::UrlWithoutQuery => match value {
                Value::Null => Err(TransformError::NullInput {
                    op: "url_without_query",
                }),
                Value::String(s) => {
                    let mut parsed =
                        url::Url::parse(s).map_err(|_| TransformError::TypeMismatch {
                            op: "url_without_query",
                            expected: "valid URL",
                            got: "invalid URL".to_string(),
                        })?;
                    parsed.set_query(None);
                    parsed.set_fragment(None);
                    Ok(Value::String(parsed.to_string()))
                }
                _ => Err(type_mismatch("url_without_query", "string", value)),
            },
            TransformOp::UrlNormalize => match value {
                Value::Null => Err(TransformError::NullInput {
                    op: "url_normalize",
                }),
                Value::String(s) => {
                    let mut parsed =
                        url::Url::parse(s).map_err(|_| TransformError::TypeMismatch {
                            op: "url_normalize",
                            expected: "valid URL",
                            got: "invalid URL".to_string(),
                        })?;

                    // 1. Remove default ports (:80 for http, :443 for https)
                    if (parsed.scheme() == "http" && parsed.port() == Some(80))
                        || (parsed.scheme() == "https" && parsed.port() == Some(443))
                    {
                        let _ = parsed.set_port(None);
                    }

                    // 2. Strip tracking parameters, sort remaining
                    let filtered: Vec<(String, String)> = parsed
                        .query_pairs()
                        .filter(|(key, _)| !is_tracking_param(key))
                        .map(|(k, v)| (k.to_string(), v.to_string()))
                        .collect();

                    if filtered.is_empty() {
                        parsed.set_query(None);
                    } else {
                        let mut sorted = filtered;
                        sorted.sort_by(|a, b| a.0.cmp(&b.0));
                        let query = sorted
                            .iter()
                            .map(|(k, v)| {
                                if v.is_empty() {
                                    k.clone()
                                } else {
                                    format!("{}={}", k, v)
                                }
                            })
                            .collect::<Vec<_>>()
                            .join("&");
                        parsed.set_query(Some(&query));
                    }

                    // 3. Strip fragment
                    parsed.set_fragment(None);

                    // 4. Remove trailing slash on non-root paths
                    let path = parsed.path().to_string();
                    if path.len() > 1 && path.ends_with('/') {
                        parsed.set_path(&path[..path.len() - 1]);
                    }

                    Ok(Value::String(parsed.to_string()))
                }
                _ => Err(type_mismatch("url_normalize", "string", value)),
            },

            // ── Slicing ─────────────────────────────────────
            TransformOp::Slice(start, end) => match value {
                Value::Null => Err(TransformError::NullInput { op: "slice" }),
                Value::Array(arr) => {
                    let len = arr.len();
                    let s = (*start).min(len);
                    let e = (*end).min(len);
                    Ok(Value::Array(arr[s..e].to_vec()))
                }
                Value::String(s) => {
                    let chars: Vec<char> = s.chars().collect();
                    let len = chars.len();
                    let si = (*start).min(len);
                    let ei = (*end).min(len);
                    Ok(Value::String(chars[si..ei].iter().collect()))
                }
                _ => Err(type_mismatch("slice", "array or string", value)),
            },

            // ── Data (array/object manipulation) ────────────
            TransformOp::Pluck(field) => match value {
                Value::Null => Err(TransformError::NullInput { op: "pluck" }),
                Value::Array(arr) => {
                    let result: Vec<Value> = arr
                        .iter()
                        .filter_map(|item| navigate_dot_path(item, field).cloned())
                        .collect();
                    Ok(Value::Array(result))
                }
                _ => Err(type_mismatch("pluck", "array", value)),
            },
            TransformOp::Where(field, op, expected) => match value {
                Value::Null => Err(TransformError::NullInput { op: "where" }),
                Value::Array(arr) => {
                    let result: Vec<Value> = arr
                        .iter()
                        .filter(|item| {
                            let val = navigate_dot_path(item, field);
                            match op.as_str() {
                                "eq" => val == Some(expected),
                                "ne" => val != Some(expected),
                                "gt" => val
                                    .and_then(|v| v.as_f64())
                                    .zip(expected.as_f64())
                                    .is_some_and(|(a, b)| a > b),
                                "lt" => val
                                    .and_then(|v| v.as_f64())
                                    .zip(expected.as_f64())
                                    .is_some_and(|(a, b)| a < b),
                                "gte" => val
                                    .and_then(|v| v.as_f64())
                                    .zip(expected.as_f64())
                                    .is_some_and(|(a, b)| a >= b),
                                "lte" => val
                                    .and_then(|v| v.as_f64())
                                    .zip(expected.as_f64())
                                    .is_some_and(|(a, b)| a <= b),
                                "contains" => val
                                    .and_then(|v| v.as_str())
                                    .zip(expected.as_str())
                                    .is_some_and(|(a, b)| a.contains(b)),
                                "starts_with" => val
                                    .and_then(|v| v.as_str())
                                    .zip(expected.as_str())
                                    .is_some_and(|(a, b)| a.starts_with(b)),
                                "ends_with" => val
                                    .and_then(|v| v.as_str())
                                    .zip(expected.as_str())
                                    .is_some_and(|(a, b)| a.ends_with(b)),
                                _ => false,
                            }
                        })
                        .cloned()
                        .collect();
                    Ok(Value::Array(result))
                }
                _ => Err(type_mismatch("where", "array", value)),
            },
            TransformOp::Pick(fields) => match value {
                Value::Null => Err(TransformError::NullInput { op: "pick" }),
                Value::Object(obj) => {
                    let mut result = serde_json::Map::new();
                    for field in fields {
                        if let Some(v) = obj.get(field) {
                            result.insert(field.clone(), v.clone());
                        }
                    }
                    Ok(Value::Object(result))
                }
                _ => Err(type_mismatch("pick", "object", value)),
            },
            TransformOp::Omit(fields) => match value {
                Value::Null => Err(TransformError::NullInput { op: "omit" }),
                Value::Object(obj) => {
                    let mut result = obj.clone();
                    for field in fields {
                        result.remove(field);
                    }
                    Ok(Value::Object(result))
                }
                _ => Err(type_mismatch("omit", "object", value)),
            },
            TransformOp::SortBy(field) => match value {
                Value::Null => Err(TransformError::NullInput { op: "sort_by" }),
                Value::Array(arr) => {
                    let mut sorted = arr.clone();
                    sorted.sort_by(|a, b| {
                        let va = navigate_dot_path(a, field);
                        let vb = navigate_dot_path(b, field);
                        match (va.and_then(|v| v.as_f64()), vb.and_then(|v| v.as_f64())) {
                            (Some(x), Some(y)) => {
                                x.partial_cmp(&y).unwrap_or(std::cmp::Ordering::Equal)
                            }
                            (Some(_), None) => std::cmp::Ordering::Less,
                            (None, Some(_)) => std::cmp::Ordering::Greater,
                            _ => {
                                let sa = va.map(|v| v.to_string()).unwrap_or_default();
                                let sb = vb.map(|v| v.to_string()).unwrap_or_default();
                                sa.cmp(&sb)
                            }
                        }
                    });
                    Ok(Value::Array(sorted))
                }
                _ => Err(type_mismatch("sort_by", "array", value)),
            },
            TransformOp::GroupBy(field) => match value {
                Value::Null => Err(TransformError::NullInput { op: "group_by" }),
                Value::Array(arr) => {
                    let mut groups: indexmap::IndexMap<String, Vec<Value>> =
                        indexmap::IndexMap::new();
                    for item in arr {
                        let key = match navigate_dot_path(item, field) {
                            Some(Value::String(s)) => s.clone(),
                            Some(v) => v.to_string(),
                            None => "null".to_string(),
                        };
                        groups.entry(key).or_default().push(item.clone());
                    }
                    let result: serde_json::Map<String, Value> = groups
                        .into_iter()
                        .map(|(k, v)| (k, Value::Array(v)))
                        .collect();
                    Ok(Value::Object(result))
                }
                _ => Err(type_mismatch("group_by", "array", value)),
            },
            TransformOp::Merge(None) => match value {
                // No-arg form: merge array of objects left-to-right
                Value::Null => Err(TransformError::NullInput { op: "merge" }),
                Value::Array(arr) => {
                    let mut base = serde_json::Map::new();
                    for item in arr {
                        if let Value::Object(obj) = item {
                            deep_merge(&mut base, obj);
                        } else {
                            return Err(TransformError::TypeMismatch {
                                op: "merge",
                                expected: "array of objects",
                                got: format!("array containing {}", value_type_name(item)),
                            });
                        }
                    }
                    Ok(Value::Object(base))
                }
                _ => Err(type_mismatch("merge", "array or object", value)),
            },
            TransformOp::Merge(Some(overlay)) => match value {
                // Parametric form: deep merge overlay onto input object
                Value::Null => Err(TransformError::NullInput { op: "merge" }),
                Value::Object(base_map) => {
                    if let Value::Object(overlay_map) = overlay {
                        let mut result = base_map.clone();
                        deep_merge(&mut result, overlay_map);
                        Ok(Value::Object(result))
                    } else {
                        Err(TransformError::TypeMismatch {
                            op: "merge",
                            expected: "object as merge argument",
                            got: value_type_name(overlay).to_string(),
                        })
                    }
                }
                _ => Err(type_mismatch("merge", "object", value)),
            },
            TransformOp::Regex(pattern) => match value {
                Value::Null => Err(TransformError::NullInput { op: "regex" }),
                Value::String(s) => {
                    let re = cached_regex(pattern).map_err(|e| TransformError::TypeMismatch {
                        op: "regex",
                        expected: "valid regex pattern",
                        got: format!("invalid regex: {}", e),
                    })?;
                    match re.find(s) {
                        Some(m) => Ok(Value::String(m.as_str().to_string())),
                        None => Ok(Value::Null),
                    }
                }
                _ => Err(type_mismatch("regex", "string", value)),
            },

            // ── Encoding ────────────────────────────────────
            TransformOp::Base64Encode => match value {
                Value::Null => Err(TransformError::NullInput {
                    op: "base64_encode",
                }),
                Value::String(s) => {
                    use base64::Engine;
                    Ok(Value::String(
                        base64::engine::general_purpose::STANDARD.encode(s.as_bytes()),
                    ))
                }
                _ => Err(type_mismatch("base64_encode", "string", value)),
            },
            // Note: base64_decode returns UTF-8 text only. For binary data
            // (images, audio), use `nika:import` with CAS instead.
            TransformOp::Base64Decode => match value {
                Value::Null => Err(TransformError::NullInput {
                    op: "base64_decode",
                }),
                Value::String(s) => {
                    use base64::Engine;
                    let bytes = base64::engine::general_purpose::STANDARD
                        .decode(s.as_bytes())
                        .map_err(|e| TransformError::TypeMismatch {
                            op: "base64_decode",
                            expected: "valid base64 string",
                            got: format!("decode error: {}", e),
                        })?;
                    let decoded =
                        String::from_utf8(bytes).map_err(|e| TransformError::TypeMismatch {
                            op: "base64_decode",
                            expected: "UTF-8 text (binary data not supported — use nika:import)",
                            got: format!("not valid UTF-8: {}", e),
                        })?;
                    Ok(Value::String(decoded))
                }
                _ => Err(type_mismatch("base64_decode", "string", value)),
            },

            // ── Predicate (returns bool) ────────────────────────
            TransformOp::StartsWith(prefix) => match value {
                Value::Null => Err(TransformError::NullInput { op: "starts_with" }),
                Value::String(s) => Ok(Value::Bool(s.starts_with(prefix.as_str()))),
                _ => Err(type_mismatch("starts_with", "string", value)),
            },
            TransformOp::EndsWith(suffix) => match value {
                Value::Null => Err(TransformError::NullInput { op: "ends_with" }),
                Value::String(s) => Ok(Value::Bool(s.ends_with(suffix.as_str()))),
                _ => Err(type_mismatch("ends_with", "string", value)),
            },
            TransformOp::Contains(text) => match value {
                Value::Null => Err(TransformError::NullInput { op: "contains" }),
                Value::String(s) => Ok(Value::Bool(s.contains(text.as_str()))),
                _ => Err(type_mismatch("contains", "string", value)),
            },

            // ── Hashing ─────────────────────────────────────────
            TransformOp::ContentHash => match value {
                Value::Null => Err(TransformError::NullInput { op: "content_hash" }),
                Value::String(s) => {
                    let hash = xxhash_rust::xxh3::xxh3_64(s.as_bytes());
                    Ok(Value::String(format!("{:016x}", hash)))
                }
                _ => {
                    let json = serde_json::to_string(value).expect("Value is serializable");
                    let hash = xxhash_rust::xxh3::xxh3_64(json.as_bytes());
                    Ok(Value::String(format!("{:016x}", hash)))
                }
            },

            // ── URL dedup ───────────────────────────────────────
            TransformOp::UniqueUrls => match value {
                Value::Null => Err(TransformError::NullInput { op: "unique_urls" }),
                Value::Array(arr) => {
                    let mut seen = std::collections::HashSet::new();
                    let unique: Vec<Value> = arr
                        .iter()
                        .filter(|v| {
                            let key = match TransformOp::UrlNormalize.apply(v) {
                                Ok(Value::String(normalized)) => normalized,
                                _ => v.to_string(),
                            };
                            seen.insert(key)
                        })
                        .cloned()
                        .collect();
                    Ok(Value::Array(unique))
                }
                _ => Err(type_mismatch("unique_urls", "array", value)),
            },

            // ── String manipulation ────────────────────────────────
            TransformOp::Replace(from, to) => match value {
                Value::Null => Err(TransformError::NullInput { op: "replace" }),
                Value::String(s) => Ok(Value::String(s.replace(from.as_str(), to.as_str()))),
                _ => Err(type_mismatch("replace", "string", value)),
            },
            TransformOp::Truncate(n) => match value {
                Value::Null => Err(TransformError::NullInput { op: "truncate" }),
                Value::String(s) => {
                    let truncated: String = s.chars().take(*n).collect();
                    Ok(Value::String(truncated))
                }
                _ => Err(type_mismatch("truncate", "string", value)),
            },

            // ── Aggregation ───────────────────────────────────────────
            TransformOp::Add => match value {
                Value::Null => Ok(Value::Null), // propagating
                Value::Array(arr) if arr.is_empty() => Ok(Value::Null),
                Value::Array(arr) => {
                    // Detect type from first non-null element
                    let first_non_null = arr.iter().find(|v| !v.is_null());
                    match first_non_null {
                        Some(Value::Number(_)) | None => {
                            // Sum numbers (null → 0)
                            let mut sum = 0.0_f64;
                            for item in arr {
                                match item {
                                    Value::Number(n) => {
                                        sum += n.as_f64().unwrap_or(0.0);
                                    }
                                    Value::Null => {} // skip nulls
                                    _ => {
                                        return Err(type_mismatch("add", "array of numbers", item))
                                    }
                                }
                            }
                            Ok(f64_to_json_number(sum))
                        }
                        Some(Value::String(_)) => {
                            // Concat strings
                            let mut result = String::new();
                            for item in arr {
                                match item {
                                    Value::String(s) => result.push_str(s),
                                    Value::Null => {} // skip nulls
                                    _ => {
                                        return Err(type_mismatch("add", "array of strings", item))
                                    }
                                }
                            }
                            Ok(Value::String(result))
                        }
                        Some(Value::Array(_)) => {
                            // Concat arrays
                            let mut result = Vec::new();
                            for item in arr {
                                match item {
                                    Value::Array(inner) => result.extend(inner.iter().cloned()),
                                    Value::Null => {} // skip nulls
                                    _ => return Err(type_mismatch("add", "array of arrays", item)),
                                }
                            }
                            Ok(Value::Array(result))
                        }
                        _ => Err(type_mismatch(
                            "add",
                            "array of numbers, strings, or arrays",
                            value,
                        )),
                    }
                }
                _ => Err(type_mismatch("add", "array", value)),
            },
            TransformOp::Min => match value {
                Value::Null => Ok(Value::Null), // propagating
                Value::Array(arr) if arr.is_empty() => Ok(Value::Null),
                Value::Array(arr) => {
                    let mut min_val: Option<f64> = None;
                    for item in arr {
                        match item {
                            Value::Number(n) => {
                                if let Some(v) = n.as_f64() {
                                    min_val = Some(match min_val {
                                        Some(current) => current.min(v),
                                        None => v,
                                    });
                                }
                                // skip numbers that can't be represented as f64
                            }
                            Value::Null => {} // skip nulls
                            _ => return Err(type_mismatch("min", "array of numbers", item)),
                        }
                    }
                    match min_val {
                        Some(v) => Ok(f64_to_json_number(v)),
                        None => Ok(Value::Null), // all nulls
                    }
                }
                _ => Err(type_mismatch("min", "array", value)),
            },
            TransformOp::Max => match value {
                Value::Null => Ok(Value::Null), // propagating
                Value::Array(arr) if arr.is_empty() => Ok(Value::Null),
                Value::Array(arr) => {
                    let mut max_val: Option<f64> = None;
                    for item in arr {
                        match item {
                            Value::Number(n) => {
                                if let Some(v) = n.as_f64() {
                                    max_val = Some(match max_val {
                                        Some(current) => current.max(v),
                                        None => v,
                                    });
                                }
                            }
                            Value::Null => {} // skip nulls
                            _ => return Err(type_mismatch("max", "array of numbers", item)),
                        }
                    }
                    match max_val {
                        Some(v) => Ok(f64_to_json_number(v)),
                        None => Ok(Value::Null), // all nulls
                    }
                }
                _ => Err(type_mismatch("max", "array", value)),
            },

            TransformOp::MinBy(field) => match value {
                Value::Null => Ok(Value::Null),
                Value::Array(arr) if arr.is_empty() => Ok(Value::Null),
                Value::Array(arr) => {
                    let mut best: Option<&Value> = None;
                    let mut best_val: Option<f64> = None;
                    let mut skipped = 0u32;
                    for item in arr {
                        if let Some(fv) = navigate_dot_path(item, field) {
                            if let Some(n) = fv.as_f64() {
                                if best_val.is_none() || n < best_val.unwrap() {
                                    best = Some(item);
                                    best_val = Some(n);
                                }
                            } else {
                                skipped += 1;
                            }
                        } else {
                            skipped += 1;
                        }
                    }
                    if skipped > 0 {
                        tracing::debug!(
                            "min_by('{}'): skipped {} item(s) with missing or non-numeric field",
                            field,
                            skipped
                        );
                    }
                    Ok(best.cloned().unwrap_or(Value::Null))
                }
                _ => Err(type_mismatch("min_by", "array", value)),
            },
            TransformOp::MaxBy(field) => match value {
                Value::Null => Ok(Value::Null),
                Value::Array(arr) if arr.is_empty() => Ok(Value::Null),
                Value::Array(arr) => {
                    let mut best: Option<&Value> = None;
                    let mut best_val: Option<f64> = None;
                    let mut skipped = 0u32;
                    for item in arr {
                        if let Some(fv) = navigate_dot_path(item, field) {
                            if let Some(n) = fv.as_f64() {
                                if best_val.is_none() || n > best_val.unwrap() {
                                    best = Some(item);
                                    best_val = Some(n);
                                }
                            } else {
                                skipped += 1;
                            }
                        } else {
                            skipped += 1;
                        }
                    }
                    if skipped > 0 {
                        tracing::debug!(
                            "max_by('{}'): skipped {} item(s) with missing or non-numeric field",
                            field,
                            skipped
                        );
                    }
                    Ok(best.cloned().unwrap_or(Value::Null))
                }
                _ => Err(type_mismatch("max_by", "array", value)),
            },
            TransformOp::Sum => match value {
                // Numeric-only sum (unlike `add` which also concats strings/arrays)
                Value::Null => Ok(Value::Null),
                Value::Array(arr) if arr.is_empty() => Ok(Value::Null),
                Value::Array(arr) => {
                    let mut sum = 0.0_f64;
                    for item in arr {
                        match item {
                            Value::Number(n) => {
                                sum += n.as_f64().unwrap_or(0.0);
                            }
                            Value::Null => {} // skip nulls
                            _ => return Err(type_mismatch("sum", "array of numbers", item)),
                        }
                    }
                    Ok(f64_to_json_number(sum))
                }
                _ => Err(type_mismatch("sum", "array of numbers", value)),
            },
            TransformOp::Avg => match value {
                Value::Null => Ok(Value::Null),
                Value::Array(arr) if arr.is_empty() => Ok(Value::Null),
                Value::Array(arr) => {
                    let mut sum = 0.0_f64;
                    let mut count = 0u64;
                    for item in arr {
                        match item {
                            Value::Number(n) => {
                                sum += n.as_f64().unwrap_or(0.0);
                                count += 1;
                            }
                            Value::Null => {} // skip
                            _ => return Err(type_mismatch("avg", "array of numbers", item)),
                        }
                    }
                    if count == 0 {
                        return Ok(Value::Null);
                    }
                    let avg = sum / count as f64;
                    Ok(f64_to_json_number(avg))
                }
                _ => Err(type_mismatch("avg", "array", value)),
            },

            // ── Introspection ─────────────────────────────────────────
            TransformOp::Has(key) => match value {
                Value::Null => Ok(Value::Null),
                Value::Object(map) => Ok(Value::Bool(map.contains_key(key.as_str()))),
                _ => Err(type_mismatch("has", "object", value)),
            },

            // ── Logic ─────────────────────────────────────────────────
            TransformOp::Not => match value {
                Value::Null => Ok(Value::Null), // propagating
                Value::Bool(b) => Ok(Value::Bool(!b)),
                _ => Err(type_mismatch("not", "boolean", value)),
            },

            // ── jq expression ──────────────────────────────────────
            TransformOp::Jq(expr) => {
                eval_jq(expr, value).map_err(|e| TransformError::TypeMismatch {
                    op: "jq",
                    expected: "valid jq expression",
                    got: e,
                })
            }

            // ── Security escaping (Nika Shield) ────────────────
            TransformOp::HtmlEscape => match value {
                Value::Null => Err(TransformError::NullInput { op: "html_escape" }),
                Value::String(s) => Ok(Value::String(
                    s.replace('&', "&amp;")
                        .replace('<', "&lt;")
                        .replace('>', "&gt;")
                        .replace('"', "&quot;")
                        .replace('\'', "&#x27;"),
                )),
                _ => Err(type_mismatch("html_escape", "string", value)),
            },
            TransformOp::MdEscape => match value {
                Value::Null => Err(TransformError::NullInput { op: "md_escape" }),
                Value::String(s) => Ok(Value::String(
                    s.replace('\\', "\\\\")
                        .replace('`', "\\`")
                        .replace('*', "\\*")
                        .replace('_', "\\_")
                        .replace('[', "\\[")
                        .replace(']', "\\]")
                        .replace('#', "\\#"),
                )),
                _ => Err(type_mismatch("md_escape", "string", value)),
            },
            TransformOp::Sanitize => match value {
                Value::Null => Err(TransformError::NullInput { op: "sanitize" }),
                Value::String(s) => {
                    let mut result = s.clone();
                    // Case-insensitive removal of common injection patterns
                    let patterns = [
                        "ignore previous",
                        "ignore all previous",
                        "disregard above",
                        "disregard previous",
                        "forget your instructions",
                        "you are now",
                        "new instructions:",
                        "system prompt:",
                    ];
                    for pat in &patterns {
                        // Case-insensitive single removal
                        let lower = result.to_lowercase();
                        if let Some(idx) = lower.find(pat) {
                            // Byte-safe slicing since patterns are ASCII
                            result = format!("{}{}", &result[..idx], &result[idx + pat.len()..]);
                        }
                    }
                    Ok(Value::String(result.trim().to_string()))
                }
                _ => Err(type_mismatch("sanitize", "string", value)),
            },
        }
    }
}

/// Type alias for the compiled jq filter (jaq 3.x).
type JaqFilter = jaq_core::Filter<jaq_core::data::JustLut<jaq_json::Val>>;

/// Evaluate a jq expression against a JSON value.
///
/// Used by `| jq()` transform AND `nika:jq` builtin tool.
/// Returns single value for one result, array for multiple, null for empty.
pub fn eval_jq(expr: &str, data: &Value) -> Result<Value, String> {
    let filter = compile_jq(expr)?;

    let jaq_val: jaq_json::Val =
        serde_json::from_value(data.clone()).map_err(|e| format!("jq input error: {e}"))?;

    // Wrap in catch_unwind to convert panics into clean errors.
    let run_result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let ctx: jaq_core::Ctx<jaq_core::data::JustLut<jaq_json::Val>> =
            jaq_core::Ctx::new(&filter.lut, jaq_core::Vars::new([]));
        let mut results: Vec<Value> = Vec::new();
        for r in filter.id.run((ctx, jaq_val)) {
            match r {
                Ok(val) => {
                    let json_str = format!("{val}");
                    let serde_val: Value =
                        serde_json::from_str(&json_str).unwrap_or(Value::String(json_str));
                    results.push(serde_val);
                }
                Err(e) => return Err(format!("jq runtime error: {e:?}")),
            }
        }
        Ok(results)
    }));

    let results = match run_result {
        Ok(Ok(r)) => r,
        Ok(Err(e)) => return Err(e),
        Err(_) => return Err("jq expression panicked (likely regex on null input)".into()),
    };

    match results.len() {
        0 => Ok(Value::Null),
        1 => Ok(results.into_iter().next().unwrap()),
        _ => Ok(Value::Array(results)),
    }
}

/// Global LRU cache for compiled jq filters.
/// `Filter` is not Clone in jaq 3.x, so we wrap in Arc.
/// 64 entries covers realistic workflow diversity (most workflows use <10 distinct expressions).
static JQ_FILTER_CACHE: LazyLock<Mutex<lru::LruCache<String, Arc<JaqFilter>>>> =
    LazyLock::new(|| Mutex::new(lru::LruCache::new(NonZeroUsize::new(64).unwrap())));

/// Compile a jq expression using jaq-core 3.x.
/// Results are cached in a global LRU — for_each loops with the same expression
/// compile once instead of N times.
fn compile_jq(expr: &str) -> Result<Arc<JaqFilter>, String> {
    // Fast path: check cache
    {
        let mut cache = JQ_FILTER_CACHE.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(filter) = cache.get(expr) {
            return Ok(Arc::clone(filter));
        }
    }

    // Slow path: parse + compile
    let defs = jaq_core::defs()
        .chain(jaq_std::defs())
        .chain(jaq_json::defs());
    let funs = jaq_core::funs()
        .chain(jaq_std::funs())
        .chain(jaq_json::funs());

    let loader = jaq_core::load::Loader::new(defs);
    let arena = jaq_core::load::Arena::default();
    let program = jaq_core::load::File {
        code: expr,
        path: (),
    };

    let modules = loader.load(&arena, program).map_err(|errs| {
        format!(
            "parse error: {}",
            errs.into_iter()
                .map(|e| format!("{e:?}"))
                .collect::<Vec<_>>()
                .join(", ")
        )
    })?;

    let filter = jaq_core::Compiler::default()
        .with_funs(funs)
        .compile(modules)
        .map_err(|errs| {
            format!(
                "compile error: {}",
                errs.into_iter()
                    .map(|e| format!("{e:?}"))
                    .collect::<Vec<_>>()
                    .join(", ")
            )
        })?;

    let filter = Arc::new(filter);

    // Store in cache
    {
        let mut cache = JQ_FILTER_CACHE.lock().unwrap_or_else(|e| e.into_inner());
        cache.put(expr.to_string(), Arc::clone(&filter));
    }

    Ok(filter)
}

/// Deep merge overlay into base (RFC 7396 semantics).
pub fn deep_merge(
    base: &mut serde_json::Map<String, Value>,
    overlay: &serde_json::Map<String, Value>,
) {
    for (key, value) in overlay {
        match (base.get_mut(key), value) {
            (Some(Value::Object(base_obj)), Value::Object(overlay_obj)) => {
                deep_merge(base_obj, overlay_obj);
            }
            _ => {
                base.insert(key.clone(), value.clone());
            }
        }
    }
}

/// Navigate a dot-separated path into a JSON value.
/// e.g. `navigate_dot_path(obj, "address.city")` → `obj["address"]["city"]`
pub fn navigate_dot_path<'a>(value: &'a Value, path: &str) -> Option<&'a Value> {
    let mut current = value;
    for segment in path.split('.') {
        current = current.get(segment)?;
    }
    Some(current)
}

/// Cache for compiled regexes (dynamic patterns from user expressions).
/// Bounded LRU (128 entries) to prevent unbounded growth from user-supplied patterns.
static REGEX_CACHE: LazyLock<Mutex<lru::LruCache<String, regex::Regex>>> =
    LazyLock::new(|| Mutex::new(lru::LruCache::new(NonZeroUsize::new(128).unwrap())));

/// Get or compile a regex pattern, caching the result.
fn cached_regex(pattern: &str) -> Result<regex::Regex, regex::Error> {
    let mut cache = REGEX_CACHE.lock().unwrap_or_else(|e| e.into_inner());
    if let Some(re) = cache.get(pattern) {
        return Ok(re.clone());
    }
    let re = regex::Regex::new(pattern)?;
    cache.put(pattern.to_string(), re.clone());
    Ok(re)
}

/// Known tracking / analytics parameters that don't affect page content.
/// Sources: Firecrawl, Scrapy w3lib, Google SEO guidelines, industry standard.
fn is_tracking_param(key: &str) -> bool {
    let lower = key.to_ascii_lowercase();
    matches!(
        lower.as_str(),
        // Google Analytics
        "utm_source"
            | "utm_medium"
            | "utm_campaign"
            | "utm_term"
            | "utm_content"
            | "utm_id"
            // Google Ads
            | "gclid"
            | "gclsrc"
            | "dclid"
            | "gbraid"
            | "wbraid"
            // Facebook / Meta
            | "fbclid"
            | "fb_action_ids"
            | "fb_action_types"
            | "fb_source"
            | "fb_ref"
            // Microsoft / Bing
            | "msclkid"
            // Mailchimp
            | "mc_cid"
            | "mc_eid"
            // HubSpot
            | "hsa_cam"
            | "hsa_grp"
            | "hsa_mt"
            | "hsa_src"
            | "hsa_ad"
            | "hsa_acc"
            | "hsa_net"
            | "hsa_ver"
            | "hsa_la"
            | "hsa_ol"
            | "hsa_kw"
            | "hsa_tgt"
            // Other common trackers
            | "_ga"
            | "_gl"
            | "_hsenc"
            | "_hsmi"
            | "mkt_tok"
            | "igshid"
            | "si"
            | "s_kwcid"
            | "ef_id"
            // TikTok
            | "ttclid"
            // Twitter / X
            | "twclid"
            // Adobe
            | "s_cid"
            // Matomo / Piwik
            | "mtm_source"
            | "mtm_medium"
            | "mtm_campaign"
            | "mtm_keyword"
            | "mtm_content"
            | "pk_source"
            | "pk_medium"
            | "pk_campaign"
            | "pk_keyword"
            | "pk_content"
    )
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
            "slice" => {
                let parts: Vec<&str> = arg.split(',').map(|s| s.trim()).collect();
                if parts.len() != 2 {
                    return Err(TransformParseError {
                        input: full_input.to_string(),
                        reason: format!(
                            "slice() requires 2 arguments (start, end), got {}",
                            parts.len()
                        ),
                    });
                }
                let start: usize = parts[0].parse().map_err(|_| TransformParseError {
                    input: full_input.to_string(),
                    reason: format!("invalid start for slice(): '{}'", parts[0]),
                })?;
                let end: usize = parts[1].parse().map_err(|_| TransformParseError {
                    input: full_input.to_string(),
                    reason: format!("invalid end for slice(): '{}'", parts[1]),
                })?;
                Ok(TransformOp::Slice(start, end))
            }
            "pluck" => {
                let field = strip_quotes(arg);
                Ok(TransformOp::Pluck(field.to_string()))
            }
            "where" => {
                // where("field", "value") — eq (default)
                // where("field", "op", value) — explicit operator
                let parts = split_parametric_args(arg);
                match parts.len() {
                    2 => {
                        let field = strip_quotes(parts[0].trim()).to_string();
                        let val_str = parts[1].trim();
                        let val =
                            parse_default_value(val_str).map_err(|reason| TransformParseError {
                                input: full_input.to_string(),
                                reason,
                            })?;
                        Ok(TransformOp::Where(field, "eq".to_string(), val))
                    }
                    3 => {
                        let field = strip_quotes(parts[0].trim()).to_string();
                        let op = strip_quotes(parts[1].trim()).to_string();
                        let valid_ops = [
                            "eq",
                            "ne",
                            "gt",
                            "lt",
                            "gte",
                            "lte",
                            "contains",
                            "starts_with",
                            "ends_with",
                        ];
                        if !valid_ops.contains(&op.as_str()) {
                            return Err(TransformParseError {
                                input: full_input.to_string(),
                                reason: format!(
                                    "unknown where() operator '{}', expected one of: {}",
                                    op,
                                    valid_ops.join(", ")
                                ),
                            });
                        }
                        let val_str = parts[2].trim();
                        let val =
                            parse_default_value(val_str).map_err(|reason| TransformParseError {
                                input: full_input.to_string(),
                                reason,
                            })?;
                        Ok(TransformOp::Where(field, op, val))
                    }
                    _ => Err(TransformParseError {
                        input: full_input.to_string(),
                        reason: format!(
                            "where() requires 2 or 3 arguments (field, [op], value), got {}",
                            parts.len()
                        ),
                    }),
                }
            }
            "pick" => {
                let fields: Vec<String> = split_parametric_args(arg)
                    .iter()
                    .map(|s| strip_quotes(s.trim()).to_string())
                    .collect();
                if fields.is_empty() {
                    return Err(TransformParseError {
                        input: full_input.to_string(),
                        reason: "pick() requires at least 1 field".to_string(),
                    });
                }
                Ok(TransformOp::Pick(fields))
            }
            "omit" => {
                let fields: Vec<String> = split_parametric_args(arg)
                    .iter()
                    .map(|s| strip_quotes(s.trim()).to_string())
                    .collect();
                if fields.is_empty() {
                    return Err(TransformParseError {
                        input: full_input.to_string(),
                        reason: "omit() requires at least 1 field".to_string(),
                    });
                }
                Ok(TransformOp::Omit(fields))
            }
            "sort_by" => {
                let field = strip_quotes(arg);
                Ok(TransformOp::SortBy(field.to_string()))
            }
            "group_by" => {
                let field = strip_quotes(arg);
                Ok(TransformOp::GroupBy(field.to_string()))
            }
            "merge" => {
                let val = parse_default_value(arg).map_err(|reason| TransformParseError {
                    input: full_input.to_string(),
                    reason,
                })?;
                Ok(TransformOp::Merge(Some(val)))
            }
            "regex" => {
                let pattern = strip_quotes(arg);
                Ok(TransformOp::Regex(pattern.to_string()))
            }
            "starts_with" => {
                let prefix = strip_quotes(arg);
                Ok(TransformOp::StartsWith(prefix.to_string()))
            }
            "ends_with" => {
                let suffix = strip_quotes(arg);
                Ok(TransformOp::EndsWith(suffix.to_string()))
            }
            "contains" => {
                let text = strip_quotes(arg);
                Ok(TransformOp::Contains(text.to_string()))
            }
            "min_by" => {
                let field = strip_quotes(arg);
                Ok(TransformOp::MinBy(field.to_string()))
            }
            "max_by" => {
                let field = strip_quotes(arg);
                Ok(TransformOp::MaxBy(field.to_string()))
            }
            "has" => {
                let key = strip_quotes(arg);
                Ok(TransformOp::Has(key.to_string()))
            }
            "replace" => {
                let parts = split_parametric_args(arg);
                if parts.len() != 2 {
                    return Err(TransformParseError {
                        input: full_input.to_string(),
                        reason: format!(
                            "replace() requires 2 arguments (from, to), got {}",
                            parts.len()
                        ),
                    });
                }
                let from = strip_quotes(parts[0].trim()).to_string();
                let to = strip_quotes(parts[1].trim()).to_string();
                Ok(TransformOp::Replace(from, to))
            }
            "truncate" => {
                let n: usize = arg.parse().map_err(|_| TransformParseError {
                    input: full_input.to_string(),
                    reason: format!("invalid argument for truncate(): '{}'", arg),
                })?;
                Ok(TransformOp::Truncate(n))
            }
            "jq" => {
                let expr = strip_quotes(arg);
                Ok(TransformOp::Jq(expr.to_string()))
            }
            _ => {
                let hint = crate::ast::analyzer::suggestions::find_similar(
                    name,
                    KNOWN_TRANSFORM_NAMES,
                    0.7,
                );
                let reason = match hint {
                    Some(ref s) => format!("unknown transform: '{}'. Did you mean '{}'?", name, s),
                    None => format!("unknown transform: '{}'", name),
                };
                Err(TransformParseError {
                    input: full_input.to_string(),
                    reason,
                })
            }
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
            "parse_yaml" => Ok(TransformOp::ParseYaml),
            "round" => Ok(TransformOp::Round(None)),
            "abs" => Ok(TransformOp::Abs),
            "ceil" => Ok(TransformOp::Ceil),
            "floor" => Ok(TransformOp::Floor),
            "type_of" => Ok(TransformOp::TypeOf),
            "shell" => Ok(TransformOp::Shell),
            "url_host" => Ok(TransformOp::UrlHost),
            "url_path" => Ok(TransformOp::UrlPath),
            "url_without_query" => Ok(TransformOp::UrlWithoutQuery),
            "url_normalize" => Ok(TransformOp::UrlNormalize),
            "merge" => Ok(TransformOp::Merge(None)),
            "base64_encode" => Ok(TransformOp::Base64Encode),
            "base64_decode" => Ok(TransformOp::Base64Decode),
            "content_hash" => Ok(TransformOp::ContentHash),
            "unique_urls" => Ok(TransformOp::UniqueUrls),
            "add" => Ok(TransformOp::Add),
            "min" => Ok(TransformOp::Min),
            "max" => Ok(TransformOp::Max),
            "sum" => Ok(TransformOp::Sum),
            "avg" => Ok(TransformOp::Avg),
            "not" => Ok(TransformOp::Not),
            "html_escape" => Ok(TransformOp::HtmlEscape),
            "md_escape" => Ok(TransformOp::MdEscape),
            "sanitize" => Ok(TransformOp::Sanitize),
            _ => {
                let hint = crate::ast::analyzer::suggestions::find_similar(
                    trimmed,
                    KNOWN_TRANSFORM_NAMES,
                    0.7,
                );
                let reason = match hint {
                    Some(ref s) => {
                        format!("unknown transform: '{}'. Did you mean '{}'?", trimmed, s)
                    }
                    None => format!("unknown transform: '{}'", trimmed),
                };
                Err(TransformParseError {
                    input: full_input.to_string(),
                    reason,
                })
            }
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

/// Convert f64 to JSON Value with NaN/Infinity guard.
/// Returns integer if no fractional part, null for non-finite values.
fn f64_to_json_number(v: f64) -> Value {
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
fn split_parametric_args(input: &str) -> Vec<&str> {
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

/// Strip UTF-8 BOM and NUL bytes from a string.
///
/// Exec output may contain a BOM (`\u{FEFF}`) or stray NUL bytes that break
/// JSON parsing. This is safe to call on any string — it only removes
/// characters that are never valid in JSON content.
fn strip_bom_and_control_chars(s: &str) -> String {
    let s = s.strip_prefix('\u{FEFF}').unwrap_or(s);
    if s.contains('\0') {
        s.replace('\0', "")
    } else {
        s.to_string()
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
    fn parse_unknown_with_suggestion() {
        let err = TransformExpr::parse("uper").unwrap_err();
        assert!(
            err.reason.contains("Did you mean 'upper'"),
            "should suggest 'upper' for 'uper', got: {}",
            err.reason
        );
    }

    #[test]
    fn parse_unknown_parametric_with_suggestion() {
        let err = TransformExpr::parse("jion(',')").unwrap_err();
        assert!(
            err.reason.contains("Did you mean 'join'"),
            "should suggest 'join' for 'jion', got: {}",
            err.reason
        );
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
    fn apply_compact_filters_empty_strings() {
        let result = TransformOp::Compact
            .apply(&json!(["hello", "", null, "world", ""]))
            .unwrap();
        assert_eq!(result, json!(["hello", "world"]));
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

    #[test]
    fn apply_parse_json_unicode() {
        // E17: CJK, accents, emoji, RTL — must survive parse_json roundtrip
        let input = r#"{"fr":"Café crème à Paris","ja":"東京タワー","ar":"مرحبا بالعالم","emoji":"🦋🚀✨"}"#;
        let result = TransformOp::ParseJson
            .apply(&Value::String(input.to_string()))
            .unwrap();
        assert_eq!(result["fr"], "Café crème à Paris");
        assert_eq!(result["ja"], "東京タワー");
        assert_eq!(result["ar"], "مرحبا بالعالم");
        assert_eq!(result["emoji"], "🦋🚀✨");
    }

    #[test]
    fn apply_parse_json_with_bom() {
        // E17: UTF-8 BOM at start of exec output
        let input = "\u{FEFF}{\"a\":1}";
        let result = TransformOp::ParseJson
            .apply(&Value::String(input.to_string()))
            .unwrap();
        assert_eq!(result, json!({"a": 1}));
    }

    #[test]
    fn apply_parse_json_with_nul() {
        // E17: stray NUL byte in exec output
        let input = "{\"a\":1}\0";
        let result = TransformOp::ParseJson
            .apply(&Value::String(input.to_string()))
            .unwrap();
        assert_eq!(result, json!({"a": 1}));
    }

    #[test]
    fn apply_parse_json_error_includes_detail() {
        // E17: error message should include serde's actual error
        let err = TransformOp::ParseJson
            .apply(&json!("not json"))
            .unwrap_err();
        match err {
            TransformError::TypeMismatch { got, .. } => {
                // Should contain serde error detail, not just truncated input
                assert!(
                    got.contains("expected"),
                    "error should include serde detail: {}",
                    got
                );
            }
            _ => panic!("expected TypeMismatch"),
        }
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
        // round with 0 decimals returns integer (consistent with ceil/floor)
        let result = TransformOp::Round(None).apply(&json!(3.7)).unwrap();
        assert_eq!(result, json!(4));
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
    fn apply_default_with_empty_string() {
        let result = TransformOp::Default(json!("FALLBACK"))
            .apply(&json!(""))
            .unwrap();
        assert_eq!(result, json!("FALLBACK"));
    }

    #[test]
    fn apply_default_preserves_whitespace_only_string() {
        let result = TransformOp::Default(json!("FALLBACK"))
            .apply(&json!("  "))
            .unwrap();
        assert_eq!(result, json!("  "), "whitespace-only strings are NOT empty");
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

    #[test]
    fn apply_shell_null_errors() {
        let err = TransformOp::Shell.apply(&Value::Null).unwrap_err();
        assert!(matches!(err, TransformError::NullInput { op: "shell" }));
    }

    // ─────────────────────────────────────────────────────────────
    // URL transform tests
    // ─────────────────────────────────────────────────────────────

    #[test]
    fn parse_url_transforms() {
        assert_eq!(
            TransformExpr::parse("url_host").unwrap().ops[0],
            TransformOp::UrlHost
        );
        assert_eq!(
            TransformExpr::parse("url_path").unwrap().ops[0],
            TransformOp::UrlPath
        );
        assert_eq!(
            TransformExpr::parse("url_without_query").unwrap().ops[0],
            TransformOp::UrlWithoutQuery
        );
    }

    #[test]
    fn apply_url_host() {
        let url = json!("https://blog.example.com:8080/posts/123?page=2#top");
        assert_eq!(
            TransformOp::UrlHost.apply(&url).unwrap(),
            json!("blog.example.com")
        );
    }

    #[test]
    fn apply_url_path() {
        let url = json!("https://example.com/posts/123?page=2");
        assert_eq!(
            TransformOp::UrlPath.apply(&url).unwrap(),
            json!("/posts/123")
        );
    }

    #[test]
    fn apply_url_without_query() {
        let url = json!("https://example.com/posts/123?page=2&sort=new#comments");
        assert_eq!(
            TransformOp::UrlWithoutQuery.apply(&url).unwrap(),
            json!("https://example.com/posts/123")
        );
    }

    #[test]
    fn url_host_ipv6() {
        let url = json!("https://[::1]:3000/api");
        // URL-002: strip IPv6 brackets for cleaner output
        assert_eq!(TransformOp::UrlHost.apply(&url).unwrap(), json!("::1"));
    }

    #[test]
    fn url_transforms_invalid_url() {
        let bad = json!("not a url");
        assert!(TransformOp::UrlHost.apply(&bad).is_err());
        assert!(TransformOp::UrlPath.apply(&bad).is_err());
        assert!(TransformOp::UrlWithoutQuery.apply(&bad).is_err());
    }

    #[test]
    fn url_transforms_null_errors() {
        assert!(matches!(
            TransformOp::UrlHost.apply(&Value::Null).unwrap_err(),
            TransformError::NullInput { op: "url_host" }
        ));
    }

    #[test]
    fn url_pipeline_host_then_lower() {
        let url = json!("https://EXAMPLE.COM/Page");
        let expr = TransformExpr::parse("url_host | lower").unwrap();
        assert_eq!(expr.apply(&url).unwrap(), json!("example.com"));
    }

    // ─────────────────────────────────────────────────────────────
    // url_normalize tests
    // ─────────────────────────────────────────────────────────────

    #[test]
    fn url_normalize_strips_utm() {
        let url = json!("https://example.com/page?utm_source=google&utm_medium=cpc&id=123");
        let result = TransformOp::UrlNormalize.apply(&url).unwrap();
        assert_eq!(result, json!("https://example.com/page?id=123"));
    }

    #[test]
    fn url_normalize_removes_default_port() {
        let url = json!("https://example.com:443/page");
        let result = TransformOp::UrlNormalize.apply(&url).unwrap();
        assert_eq!(result, json!("https://example.com/page"));
    }

    #[test]
    fn url_normalize_removes_default_port_http() {
        let url = json!("http://example.com:80/page");
        let result = TransformOp::UrlNormalize.apply(&url).unwrap();
        assert_eq!(result, json!("http://example.com/page"));
    }

    #[test]
    fn url_normalize_sorts_params() {
        let url = json!("https://example.com/page?z=1&a=2&m=3");
        let result = TransformOp::UrlNormalize.apply(&url).unwrap();
        assert_eq!(result, json!("https://example.com/page?a=2&m=3&z=1"));
    }

    #[test]
    fn url_normalize_strips_fragment() {
        let url = json!("https://example.com/page#section");
        let result = TransformOp::UrlNormalize.apply(&url).unwrap();
        assert_eq!(result, json!("https://example.com/page"));
    }

    #[test]
    fn url_normalize_strips_trailing_slash() {
        let url = json!("https://example.com/page/");
        let result = TransformOp::UrlNormalize.apply(&url).unwrap();
        assert_eq!(result, json!("https://example.com/page"));
    }

    #[test]
    fn url_normalize_preserves_root_slash() {
        let url = json!("https://example.com/");
        let result = TransformOp::UrlNormalize.apply(&url).unwrap();
        assert_eq!(result, json!("https://example.com/"));
    }

    #[test]
    fn url_normalize_strips_all_tracking() {
        let url = json!("https://example.com/page?fbclid=abc&gclid=def&page=2");
        let result = TransformOp::UrlNormalize.apply(&url).unwrap();
        assert_eq!(result, json!("https://example.com/page?page=2"));
    }

    #[test]
    fn url_normalize_no_query_no_change() {
        let url = json!("https://example.com/page");
        let result = TransformOp::UrlNormalize.apply(&url).unwrap();
        assert_eq!(result, json!("https://example.com/page"));
    }

    #[test]
    fn url_normalize_all_tracking_removed() {
        let url = json!("https://example.com/page?utm_source=a&fbclid=b");
        let result = TransformOp::UrlNormalize.apply(&url).unwrap();
        assert_eq!(result, json!("https://example.com/page"));
    }

    #[test]
    fn url_normalize_preserves_non_default_port() {
        let url = json!("https://example.com:8443/page");
        let result = TransformOp::UrlNormalize.apply(&url).unwrap();
        assert_eq!(result, json!("https://example.com:8443/page"));
    }

    #[test]
    fn url_normalize_chaining_with_host() {
        let url = json!("https://WWW.Example.COM:443/page?utm_source=x#top");
        let expr = TransformExpr::parse("url_normalize | url_host").unwrap();
        let result = expr.apply(&url).unwrap();
        assert_eq!(result, json!("www.example.com"));
    }

    #[test]
    fn url_normalize_null_errors() {
        assert!(matches!(
            TransformOp::UrlNormalize.apply(&Value::Null).unwrap_err(),
            TransformError::NullInput {
                op: "url_normalize"
            }
        ));
    }

    #[test]
    fn url_normalize_invalid_url_errors() {
        let bad = json!("not a url");
        assert!(TransformOp::UrlNormalize.apply(&bad).is_err());
    }

    #[test]
    fn parse_url_normalize() {
        assert_eq!(
            TransformExpr::parse("url_normalize").unwrap().ops[0],
            TransformOp::UrlNormalize
        );
    }

    // ─────────────────────────────────────────────────────────────
    // slice tests
    // ─────────────────────────────────────────────────────────────

    #[test]
    fn slice_array_basic() {
        let arr = json!(["a", "b", "c", "d", "e"]);
        assert_eq!(
            TransformOp::Slice(1, 3).apply(&arr).unwrap(),
            json!(["b", "c"])
        );
    }

    #[test]
    fn slice_array_from_start() {
        let arr = json!([1, 2, 3, 4, 5]);
        assert_eq!(
            TransformOp::Slice(0, 3).apply(&arr).unwrap(),
            json!([1, 2, 3])
        );
    }

    #[test]
    fn slice_array_to_end() {
        let arr = json!([1, 2, 3, 4, 5]);
        assert_eq!(
            TransformOp::Slice(3, 100).apply(&arr).unwrap(),
            json!([4, 5])
        );
    }

    #[test]
    fn slice_array_empty_range() {
        let arr = json!([1, 2, 3]);
        assert_eq!(TransformOp::Slice(5, 10).apply(&arr).unwrap(), json!([]));
    }

    #[test]
    fn slice_string() {
        let s = json!("Hello World");
        assert_eq!(TransformOp::Slice(0, 5).apply(&s).unwrap(), json!("Hello"));
    }

    #[test]
    fn slice_null_errors() {
        assert!(TransformOp::Slice(0, 1).apply(&Value::Null).is_err());
    }

    #[test]
    fn parse_slice_transform() {
        let expr = TransformExpr::parse("slice(0, 100)").unwrap();
        assert_eq!(expr.ops[0], TransformOp::Slice(0, 100));
    }

    #[test]
    fn slice_pipeline() {
        let arr = json!(["x", "y", "z", "w"]);
        let expr = TransformExpr::parse("slice(1, 3) | length").unwrap();
        assert_eq!(expr.apply(&arr).unwrap(), json!(2));
    }

    #[test]
    fn display_url_normalize() {
        assert_eq!(TransformOp::UrlNormalize.to_string(), "url_normalize");
    }

    #[test]
    fn display_slice() {
        assert_eq!(TransformOp::Slice(0, 10).to_string(), "slice(0, 10)");
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
    fn error_display_object_hints_extract_article() {
        let err = TransformError::TypeMismatch {
            op: "trim",
            expected: "string",
            got: "object".to_string(),
        };
        let msg = err.to_string();
        assert!(
            msg.contains("text_content"),
            "should hint about extract: article fields, got: {}",
            msg
        );
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
    fn last_n_string() {
        let result = TransformOp::LastN(5).apply(&json!("hello world")).unwrap();
        assert_eq!(result, json!("world"));
    }

    #[test]
    fn last_n_string_unicode() {
        let result = TransformOp::LastN(2).apply(&json!("日本語")).unwrap();
        assert_eq!(result, json!("本語"));
    }

    #[test]
    fn last_n_string_exceeds_length() {
        let result = TransformOp::LastN(100).apply(&json!("short")).unwrap();
        assert_eq!(result, json!("short"));
    }

    #[test]
    fn last_n_empty_string() {
        let result = TransformOp::LastN(5).apply(&json!("")).unwrap();
        assert_eq!(result, json!(""));
    }

    #[test]
    fn last_n_object() {
        let obj = json!({"a": 1});
        let result = TransformOp::LastN(5).apply(&obj).unwrap();
        // Last 5 chars of JSON serialization
        assert!(result.is_string());
        assert!(result.as_str().unwrap().len() <= 5);
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
    fn parse_chain_with_pipe_in_join_arg() {
        // B4: join(" | ") must NOT split on the | inside parentheses
        let expr = TransformExpr::parse(r#"trim | split(",") | join(" | ")"#).unwrap();
        assert_eq!(expr.ops.len(), 3);
        assert_eq!(
            expr.ops.as_slice(),
            &[
                TransformOp::Trim,
                TransformOp::Split(",".to_string()),
                TransformOp::Join(" | ".to_string()),
            ]
        );
    }

    #[test]
    fn apply_chain_with_pipe_in_join_arg() {
        // B4: end-to-end — split then join with pipe separator
        let expr = TransformExpr::parse(r#"split(",") | join(" | ")"#).unwrap();
        let result = expr.apply(&json!("a,b,c")).unwrap();
        assert_eq!(result, json!("a | b | c"));
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

    // ── parse_yaml ───────────────────────────────────────────────

    #[test]
    fn parse_parse_yaml() {
        let expr = TransformExpr::parse("parse_yaml").unwrap();
        assert_eq!(expr.ops.as_slice(), &[TransformOp::ParseYaml]);
    }

    #[test]
    fn apply_parse_yaml_object() {
        let yaml = json!("name: hello\ncount: 42\n");
        let result = TransformOp::ParseYaml.apply(&yaml).unwrap();
        assert_eq!(result["name"], "hello");
        assert_eq!(result["count"], 42);
    }

    #[test]
    fn apply_parse_yaml_array() {
        let yaml = json!("- one\n- two\n- three\n");
        let result = TransformOp::ParseYaml.apply(&yaml).unwrap();
        assert_eq!(result, json!(["one", "two", "three"]));
    }

    #[test]
    fn apply_parse_yaml_nested() {
        let yaml = json!("locale: fr-FR\ncommunication:\n  formality: tu\n  tone: warm\n");
        let result = TransformOp::ParseYaml.apply(&yaml).unwrap();
        assert_eq!(result["locale"], "fr-FR");
        assert_eq!(result["communication"]["formality"], "tu");
        assert_eq!(result["communication"]["tone"], "warm");
    }

    #[test]
    fn apply_parse_yaml_strips_markdown_code_block() {
        let yaml = json!("```yaml\nname: test\nvalue: 42\n```");
        let result = TransformOp::ParseYaml.apply(&yaml).unwrap();
        assert_eq!(result["name"], "test");
        assert_eq!(result["value"], 42);
    }

    #[test]
    fn apply_parse_yaml_strips_yml_code_block() {
        let yaml = json!("```yml\n- a\n- b\n```");
        let result = TransformOp::ParseYaml.apply(&yaml).unwrap();
        assert_eq!(result, json!(["a", "b"]));
    }

    #[test]
    fn apply_parse_yaml_unicode() {
        let yaml = json!("fr: Café crème\nja: 東京タワー\nemoji: 🦋\n");
        let result = TransformOp::ParseYaml.apply(&yaml).unwrap();
        assert_eq!(result["fr"], "Café crème");
        assert_eq!(result["ja"], "東京タワー");
        assert_eq!(result["emoji"], "🦋");
    }

    #[test]
    fn apply_parse_yaml_null_fails() {
        let err = TransformOp::ParseYaml.apply(&Value::Null).unwrap_err();
        assert!(matches!(err, TransformError::NullInput { .. }));
    }

    #[test]
    fn apply_parse_yaml_idempotent_on_object() {
        let obj = json!({"key": "value"});
        let result = TransformOp::ParseYaml.apply(&obj).unwrap();
        assert_eq!(result, json!({"key": "value"}));
    }

    #[test]
    fn apply_parse_yaml_invalid() {
        let yaml = json!("{{invalid:\nyaml: [");
        let err = TransformOp::ParseYaml.apply(&yaml).unwrap_err();
        assert!(matches!(err, TransformError::TypeMismatch { .. }));
    }

    #[test]
    fn apply_parse_yaml_scalar_string() {
        // Plain YAML string should parse to a string value
        let yaml = json!("hello world");
        let result = TransformOp::ParseYaml.apply(&yaml).unwrap();
        assert_eq!(result, json!("hello world"));
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

    // ─────────────────────────────────────────────────────────────
    // Data transforms — pluck, where, pick, omit, sort_by, group_by, merge
    // ─────────────────────────────────────────────────────────────

    #[test]
    fn parse_pluck() {
        let expr = TransformExpr::parse("pluck('name')").unwrap();
        assert_eq!(
            expr.ops.as_slice(),
            &[TransformOp::Pluck("name".to_string())]
        );
    }

    #[test]
    fn parse_pluck_double_quotes() {
        let expr = TransformExpr::parse(r#"pluck("status")"#).unwrap();
        assert_eq!(
            expr.ops.as_slice(),
            &[TransformOp::Pluck("status".to_string())]
        );
    }

    #[test]
    fn apply_pluck_basic() {
        let data = json!([
            {"name": "Alice", "age": 30},
            {"name": "Bob", "age": 25}
        ]);
        let result = TransformOp::Pluck("name".to_string()).apply(&data).unwrap();
        assert_eq!(result, json!(["Alice", "Bob"]));
    }

    #[test]
    fn apply_pluck_missing_field() {
        let data = json!([
            {"name": "Alice", "age": 30},
            {"age": 25},
            {"name": "Charlie"}
        ]);
        let result = TransformOp::Pluck("name".to_string()).apply(&data).unwrap();
        assert_eq!(result, json!(["Alice", "Charlie"]));
    }

    #[test]
    fn apply_pluck_empty_array() {
        let result = TransformOp::Pluck("x".to_string())
            .apply(&json!([]))
            .unwrap();
        assert_eq!(result, json!([]));
    }

    #[test]
    fn apply_pluck_null_errors() {
        assert!(TransformOp::Pluck("x".to_string())
            .apply(&Value::Null)
            .is_err());
    }

    #[test]
    fn apply_pluck_not_array_errors() {
        assert!(TransformOp::Pluck("x".to_string())
            .apply(&json!({"x": 1}))
            .is_err());
    }

    #[test]
    fn parse_where() {
        let expr = TransformExpr::parse("where('status', 'active')").unwrap();
        assert_eq!(
            expr.ops.as_slice(),
            &[TransformOp::Where(
                "status".to_string(),
                "eq".to_string(),
                json!("active")
            )]
        );
    }

    #[test]
    fn parse_where_numeric() {
        let expr = TransformExpr::parse("where('age', 30)").unwrap();
        assert_eq!(
            expr.ops.as_slice(),
            &[TransformOp::Where(
                "age".to_string(),
                "eq".to_string(),
                json!(30)
            )]
        );
    }

    #[test]
    fn apply_where_basic() {
        let data = json!([
            {"name": "Alice", "status": "active"},
            {"name": "Bob", "status": "inactive"},
            {"name": "Charlie", "status": "active"}
        ]);
        let result = TransformOp::Where("status".to_string(), "eq".to_string(), json!("active"))
            .apply(&data)
            .unwrap();
        assert_eq!(
            result,
            json!([
                {"name": "Alice", "status": "active"},
                {"name": "Charlie", "status": "active"}
            ])
        );
    }

    #[test]
    fn apply_where_no_match() {
        let data = json!([{"a": 1}, {"a": 2}]);
        let result = TransformOp::Where("a".to_string(), "eq".to_string(), json!(99))
            .apply(&data)
            .unwrap();
        assert_eq!(result, json!([]));
    }

    #[test]
    fn apply_where_null_errors() {
        assert!(
            TransformOp::Where("x".to_string(), "eq".to_string(), json!("y"))
                .apply(&Value::Null)
                .is_err()
        );
    }

    #[test]
    fn parse_pick() {
        let expr = TransformExpr::parse("pick('name', 'age')").unwrap();
        assert_eq!(
            expr.ops.as_slice(),
            &[TransformOp::Pick(vec![
                "name".to_string(),
                "age".to_string()
            ])]
        );
    }

    #[test]
    fn apply_pick_basic() {
        let data = json!({"name": "Alice", "age": 30, "secret": "xxx", "role": "admin"});
        let result = TransformOp::Pick(vec!["name".to_string(), "age".to_string()])
            .apply(&data)
            .unwrap();
        assert_eq!(result, json!({"name": "Alice", "age": 30}));
    }

    #[test]
    fn apply_pick_missing_field() {
        let data = json!({"name": "Alice"});
        let result = TransformOp::Pick(vec!["name".to_string(), "email".to_string()])
            .apply(&data)
            .unwrap();
        assert_eq!(result, json!({"name": "Alice"}));
    }

    #[test]
    fn apply_pick_null_errors() {
        assert!(TransformOp::Pick(vec!["x".to_string()])
            .apply(&Value::Null)
            .is_err());
    }

    #[test]
    fn apply_pick_not_object_errors() {
        assert!(TransformOp::Pick(vec!["x".to_string()])
            .apply(&json!([1, 2]))
            .is_err());
    }

    #[test]
    fn parse_omit() {
        let expr = TransformExpr::parse("omit('password', 'secret')").unwrap();
        assert_eq!(
            expr.ops.as_slice(),
            &[TransformOp::Omit(vec![
                "password".to_string(),
                "secret".to_string()
            ])]
        );
    }

    #[test]
    fn apply_omit_basic() {
        let data = json!({"name": "Alice", "password": "xxx", "secret": "yyy"});
        let result = TransformOp::Omit(vec!["password".to_string(), "secret".to_string()])
            .apply(&data)
            .unwrap();
        assert_eq!(result, json!({"name": "Alice"}));
    }

    #[test]
    fn apply_omit_missing_field() {
        let data = json!({"name": "Alice"});
        let result = TransformOp::Omit(vec!["nonexistent".to_string()])
            .apply(&data)
            .unwrap();
        assert_eq!(result, json!({"name": "Alice"}));
    }

    #[test]
    fn apply_omit_null_errors() {
        assert!(TransformOp::Omit(vec!["x".to_string()])
            .apply(&Value::Null)
            .is_err());
    }

    #[test]
    fn parse_sort_by() {
        let expr = TransformExpr::parse("sort_by('age')").unwrap();
        assert_eq!(
            expr.ops.as_slice(),
            &[TransformOp::SortBy("age".to_string())]
        );
    }

    #[test]
    fn apply_sort_by_numeric() {
        let data = json!([
            {"name": "Bob", "age": 25},
            {"name": "Alice", "age": 30},
            {"name": "Charlie", "age": 20}
        ]);
        let result = TransformOp::SortBy("age".to_string()).apply(&data).unwrap();
        assert_eq!(result[0]["name"], "Charlie");
        assert_eq!(result[1]["name"], "Bob");
        assert_eq!(result[2]["name"], "Alice");
    }

    #[test]
    fn apply_sort_by_string() {
        let data = json!([
            {"name": "Charlie"},
            {"name": "Alice"},
            {"name": "Bob"}
        ]);
        let result = TransformOp::SortBy("name".to_string())
            .apply(&data)
            .unwrap();
        assert_eq!(result[0]["name"], "Alice");
        assert_eq!(result[1]["name"], "Bob");
        assert_eq!(result[2]["name"], "Charlie");
    }

    #[test]
    fn apply_sort_by_missing_field() {
        let data = json!([
            {"name": "Alice", "score": 90},
            {"name": "Bob"},
            {"name": "Charlie", "score": 80}
        ]);
        let result = TransformOp::SortBy("score".to_string())
            .apply(&data)
            .unwrap();
        // Items with field come first (numeric sort), missing last
        assert_eq!(result[0]["name"], "Charlie");
        assert_eq!(result[1]["name"], "Alice");
    }

    #[test]
    fn apply_sort_by_null_errors() {
        assert!(TransformOp::SortBy("x".to_string())
            .apply(&Value::Null)
            .is_err());
    }

    #[test]
    fn parse_group_by() {
        let expr = TransformExpr::parse("group_by('locale')").unwrap();
        assert_eq!(
            expr.ops.as_slice(),
            &[TransformOp::GroupBy("locale".to_string())]
        );
    }

    #[test]
    fn apply_group_by_basic() {
        let data = json!([
            {"locale": "fr", "text": "Bonjour"},
            {"locale": "en", "text": "Hello"},
            {"locale": "fr", "text": "Merci"}
        ]);
        let result = TransformOp::GroupBy("locale".to_string())
            .apply(&data)
            .unwrap();
        assert_eq!(result["fr"].as_array().unwrap().len(), 2);
        assert_eq!(result["en"].as_array().unwrap().len(), 1);
    }

    #[test]
    fn apply_group_by_empty() {
        let result = TransformOp::GroupBy("x".to_string())
            .apply(&json!([]))
            .unwrap();
        assert_eq!(result, json!({}));
    }

    #[test]
    fn apply_group_by_null_errors() {
        assert!(TransformOp::GroupBy("x".to_string())
            .apply(&Value::Null)
            .is_err());
    }

    #[test]
    fn apply_merge_basic() {
        let data = json!([{"a": 1}, {"b": 2}, {"c": 3}]);
        let result = TransformOp::Merge(None).apply(&data).unwrap();
        assert_eq!(result, json!({"a": 1, "b": 2, "c": 3}));
    }

    #[test]
    fn apply_merge_deep() {
        let data = json!([
            {"nested": {"x": 1}},
            {"nested": {"y": 2}}
        ]);
        let result = TransformOp::Merge(None).apply(&data).unwrap();
        assert_eq!(result["nested"]["x"], 1);
        assert_eq!(result["nested"]["y"], 2);
    }

    #[test]
    fn apply_merge_override() {
        let data = json!([{"a": 1, "b": "old"}, {"b": "new", "c": 3}]);
        let result = TransformOp::Merge(None).apply(&data).unwrap();
        assert_eq!(result, json!({"a": 1, "b": "new", "c": 3}));
    }

    #[test]
    fn apply_merge_empty_array() {
        let result = TransformOp::Merge(None).apply(&json!([])).unwrap();
        assert_eq!(result, json!({}));
    }

    #[test]
    fn apply_merge_non_objects_errors() {
        let data = json!([{"a": 1}, "not an object"]);
        assert!(TransformOp::Merge(None).apply(&data).is_err());
    }

    #[test]
    fn apply_merge_null_errors() {
        assert!(TransformOp::Merge(None).apply(&Value::Null).is_err());
    }

    #[test]
    fn parse_merge() {
        let expr = TransformExpr::parse("merge").unwrap();
        assert_eq!(expr.ops.as_slice(), &[TransformOp::Merge(None)]);
    }

    // ─────────────────────────────────────────────────────────────
    // regex transform
    // ─────────────────────────────────────────────────────────────

    #[test]
    fn parse_regex() {
        let expr = TransformExpr::parse(r#"regex('\d+')"#).unwrap();
        assert_eq!(
            expr.ops.as_slice(),
            &[TransformOp::Regex(r"\d+".to_string())]
        );
    }

    #[test]
    fn apply_regex_match() {
        let result = TransformOp::Regex(r"\d+\.\d+".to_string())
            .apply(&json!("Price: $42.50"))
            .unwrap();
        assert_eq!(result, json!("42.50"));
    }

    #[test]
    fn apply_regex_integer() {
        let result = TransformOp::Regex(r"\d+".to_string())
            .apply(&json!("There are 42 items"))
            .unwrap();
        assert_eq!(result, json!("42"));
    }

    #[test]
    fn apply_regex_no_match() {
        let result = TransformOp::Regex(r"\d+".to_string())
            .apply(&json!("no numbers here"))
            .unwrap();
        assert_eq!(result, Value::Null);
    }

    #[test]
    fn apply_regex_invalid_pattern() {
        let result = TransformOp::Regex(r"[invalid".to_string()).apply(&json!("test"));
        assert!(result.is_err());
    }

    #[test]
    fn apply_regex_null_errors() {
        assert!(TransformOp::Regex(r"\d+".to_string())
            .apply(&Value::Null)
            .is_err());
    }

    #[test]
    fn apply_regex_not_string_errors() {
        assert!(TransformOp::Regex(r"\d+".to_string())
            .apply(&json!(42))
            .is_err());
    }

    // ─────────────────────────────────────────────────────────────
    // base64 transforms
    // ─────────────────────────────────────────────────────────────

    #[test]
    fn parse_base64_encode() {
        let expr = TransformExpr::parse("base64_encode").unwrap();
        assert_eq!(expr.ops.as_slice(), &[TransformOp::Base64Encode]);
    }

    #[test]
    fn parse_base64_decode() {
        let expr = TransformExpr::parse("base64_decode").unwrap();
        assert_eq!(expr.ops.as_slice(), &[TransformOp::Base64Decode]);
    }

    #[test]
    fn apply_base64_encode() {
        let result = TransformOp::Base64Encode
            .apply(&json!("Hello, World!"))
            .unwrap();
        assert_eq!(result, json!("SGVsbG8sIFdvcmxkIQ=="));
    }

    #[test]
    fn apply_base64_decode() {
        let result = TransformOp::Base64Decode
            .apply(&json!("SGVsbG8sIFdvcmxkIQ=="))
            .unwrap();
        assert_eq!(result, json!("Hello, World!"));
    }

    #[test]
    fn apply_base64_roundtrip() {
        let original = json!("Nika 🦋 workflow engine");
        let encoded = TransformOp::Base64Encode.apply(&original).unwrap();
        let decoded = TransformOp::Base64Decode.apply(&encoded).unwrap();
        assert_eq!(decoded, original);
    }

    #[test]
    fn apply_base64_decode_invalid() {
        let result = TransformOp::Base64Decode.apply(&json!("not!!valid!!base64"));
        assert!(result.is_err());
    }

    #[test]
    fn apply_base64_encode_null_errors() {
        assert!(TransformOp::Base64Encode.apply(&Value::Null).is_err());
    }

    #[test]
    fn apply_base64_decode_null_errors() {
        assert!(TransformOp::Base64Decode.apply(&Value::Null).is_err());
    }

    // ─────────────────────────────────────────────────────────────
    // Pipeline tests with new transforms
    // ─────────────────────────────────────────────────────────────

    #[test]
    fn pipeline_pluck_then_sort() {
        let data = json!([
            {"name": "Charlie", "age": 20},
            {"name": "Alice", "age": 30},
            {"name": "Bob", "age": 25}
        ]);
        let expr = TransformExpr::parse("pluck('name') | sort").unwrap();
        let result = expr.apply(&data).unwrap();
        assert_eq!(result, json!(["Alice", "Bob", "Charlie"]));
    }

    #[test]
    fn pipeline_where_then_pluck() {
        let data = json!([
            {"name": "Alice", "status": "active"},
            {"name": "Bob", "status": "inactive"},
            {"name": "Charlie", "status": "active"}
        ]);
        let expr = TransformExpr::parse("where('status', 'active') | pluck('name')").unwrap();
        let result = expr.apply(&data).unwrap();
        assert_eq!(result, json!(["Alice", "Charlie"]));
    }

    #[test]
    fn pipeline_sort_by_then_pluck() {
        let data = json!([
            {"name": "Bob", "score": 85},
            {"name": "Alice", "score": 95},
            {"name": "Charlie", "score": 70}
        ]);
        let expr = TransformExpr::parse("sort_by('score') | pluck('name')").unwrap();
        let result = expr.apply(&data).unwrap();
        assert_eq!(result, json!(["Charlie", "Bob", "Alice"]));
    }

    #[test]
    fn pipeline_pluck_join() {
        let data = json!([{"name": "Alice"}, {"name": "Bob"}]);
        let expr = TransformExpr::parse("pluck('name') | join(', ')").unwrap();
        let result = expr.apply(&data).unwrap();
        assert_eq!(result, json!("Alice, Bob"));
    }

    #[test]
    fn pipeline_pick_then_to_json() {
        let data = json!({"name": "Alice", "age": 30, "secret": "xxx"});
        let expr = TransformExpr::parse("pick('name', 'age') | to_json").unwrap();
        let result = expr.apply(&data).unwrap();
        assert!(result.as_str().unwrap().contains("name"));
        assert!(!result.as_str().unwrap().contains("secret"));
    }

    #[test]
    fn pipeline_base64_roundtrip() {
        let expr = TransformExpr::parse("base64_encode | base64_decode").unwrap();
        let result = expr.apply(&json!("test data 🦋")).unwrap();
        assert_eq!(result, json!("test data 🦋"));
    }

    // ─────────────────────────────────────────────────────────────
    // Display for new transforms
    // ─────────────────────────────────────────────────────────────

    #[test]
    fn display_new_transforms() {
        assert_eq!(
            TransformOp::Pluck("name".to_string()).to_string(),
            "pluck('name')"
        );
        assert_eq!(
            TransformOp::Where("status".to_string(), "eq".to_string(), json!("active")).to_string(),
            "where('status', \"active\")"
        );
        assert_eq!(
            TransformOp::Pick(vec!["a".to_string(), "b".to_string()]).to_string(),
            "pick('a', 'b')"
        );
        assert_eq!(
            TransformOp::Omit(vec!["x".to_string()]).to_string(),
            "omit('x')"
        );
        assert_eq!(
            TransformOp::SortBy("age".to_string()).to_string(),
            "sort_by('age')"
        );
        assert_eq!(
            TransformOp::GroupBy("locale".to_string()).to_string(),
            "group_by('locale')"
        );
        assert_eq!(TransformOp::Merge(None).to_string(), "merge");
        assert_eq!(
            TransformOp::Regex(r"\d+".to_string()).to_string(),
            r"regex('\d+')"
        );
        assert_eq!(TransformOp::Base64Encode.to_string(), "base64_encode");
        assert_eq!(TransformOp::Base64Decode.to_string(), "base64_decode");
    }

    // ─────────────────────────────────────────────────────────────
    // Edge case tests (code review findings)
    // ─────────────────────────────────────────────────────────────

    #[test]
    fn where_with_boolean_value() {
        let data = json!([
            {"name": "Alice", "active": true},
            {"name": "Bob", "active": false},
            {"name": "Charlie", "active": true}
        ]);
        let result = TransformOp::Where("active".to_string(), "eq".to_string(), json!(true))
            .apply(&data)
            .unwrap();
        assert_eq!(result.as_array().unwrap().len(), 2);
        assert_eq!(result[0]["name"], "Alice");
        assert_eq!(result[1]["name"], "Charlie");
    }

    #[test]
    fn where_with_numeric_value() {
        let data = json!([
            {"id": 1, "score": 90},
            {"id": 2, "score": 80},
            {"id": 3, "score": 90}
        ]);
        let result = TransformOp::Where("score".to_string(), "eq".to_string(), json!(90))
            .apply(&data)
            .unwrap();
        assert_eq!(result.as_array().unwrap().len(), 2);
    }

    #[test]
    fn pluck_nested_field_with_dot_path() {
        // pluck supports dot-paths for nested field access
        let data = json!([{"a": {"b": 1}}, {"a": {"b": 2}}]);
        let result = TransformOp::Pluck("a.b".to_string()).apply(&data).unwrap();
        assert_eq!(result, json!([1, 2]));
    }

    #[test]
    fn pluck_top_level_field() {
        let data = json!([{"a": {"b": 1}}, {"a": {"b": 2}}]);
        let result = TransformOp::Pluck("b".to_string()).apply(&data).unwrap();
        assert_eq!(result, json!([]), "top-level 'b' doesn't exist");
    }

    #[test]
    fn group_by_numeric_field() {
        // group_by with numeric values converts them to string keys
        let data = json!([{"score": 90}, {"score": 80}, {"score": 90}]);
        let result = TransformOp::GroupBy("score".to_string())
            .apply(&data)
            .unwrap();
        assert_eq!(result["90"].as_array().unwrap().len(), 2);
        assert_eq!(result["80"].as_array().unwrap().len(), 1);
    }

    #[test]
    fn group_by_missing_field() {
        // items without the field get grouped under "null"
        let data = json!([{"a": 1}, {"b": 2}]);
        let result = TransformOp::GroupBy("a".to_string()).apply(&data).unwrap();
        assert!(result.get("1").is_some());
        assert!(result.get("null").is_some());
    }

    #[test]
    fn pick_preserves_order() {
        let data = json!({"z": 3, "a": 1, "m": 2});
        let result = TransformOp::Pick(vec!["a".to_string(), "z".to_string()])
            .apply(&data)
            .unwrap();
        let keys: Vec<&String> = result.as_object().unwrap().keys().collect();
        assert_eq!(keys, vec!["a", "z"]);
    }

    #[test]
    fn sort_by_stable_on_equal_values() {
        let data = json!([
            {"name": "Alice", "score": 90},
            {"name": "Bob", "score": 90},
            {"name": "Charlie", "score": 90}
        ]);
        let result = TransformOp::SortBy("score".to_string())
            .apply(&data)
            .unwrap();
        // All scores equal — order should be preserved (stable sort)
        assert_eq!(result[0]["name"], "Alice");
        assert_eq!(result[1]["name"], "Bob");
        assert_eq!(result[2]["name"], "Charlie");
    }

    #[test]
    fn merge_single_object() {
        let data = json!([{"a": 1, "b": 2}]);
        let result = TransformOp::Merge(None).apply(&data).unwrap();
        assert_eq!(result, json!({"a": 1, "b": 2}));
    }

    #[test]
    fn regex_captures_first_match_only() {
        let result = TransformOp::Regex(r"\d+".to_string())
            .apply(&json!("item1 item2 item3"))
            .unwrap();
        assert_eq!(result, json!("1"), "regex returns first match only");
    }

    #[test]
    fn base64_encode_empty_string() {
        let result = TransformOp::Base64Encode.apply(&json!("")).unwrap();
        assert_eq!(result, json!(""));
    }

    #[test]
    fn base64_decode_empty_string() {
        let result = TransformOp::Base64Decode.apply(&json!("")).unwrap();
        assert_eq!(result, json!(""));
    }

    // ─────────────────────────────────────────────────────────────
    // FIX-1: merge parametric form
    // ─────────────────────────────────────────────────────────────

    #[test]
    fn merge_parametric_basic() {
        let base = json!({"a": 1, "b": 2});
        let overlay = json!({"c": 3});
        let result = TransformOp::Merge(Some(overlay)).apply(&base).unwrap();
        assert_eq!(result, json!({"a": 1, "b": 2, "c": 3}));
    }

    #[test]
    fn merge_parametric_override() {
        let base = json!({"a": 1, "b": "old"});
        let overlay = json!({"b": "new", "c": 3});
        let result = TransformOp::Merge(Some(overlay)).apply(&base).unwrap();
        assert_eq!(result, json!({"a": 1, "b": "new", "c": 3}));
    }

    #[test]
    fn merge_parametric_deep() {
        let base = json!({"nested": {"x": 1, "y": 2}});
        let overlay = json!({"nested": {"z": 3}});
        let result = TransformOp::Merge(Some(overlay)).apply(&base).unwrap();
        assert_eq!(result["nested"]["x"], 1);
        assert_eq!(result["nested"]["y"], 2);
        assert_eq!(result["nested"]["z"], 3);
    }

    #[test]
    fn merge_parametric_non_object_input_errors() {
        assert!(TransformOp::Merge(Some(json!({"a": 1})))
            .apply(&json!([1, 2]))
            .is_err());
    }

    #[test]
    fn parse_merge_parametric() {
        let expr = TransformExpr::parse(r#"merge({"key": "val"})"#).unwrap();
        match &expr.ops[0] {
            TransformOp::Merge(Some(v)) => assert_eq!(v, &json!({"key": "val"})),
            _ => panic!("expected Merge(Some(...))"),
        }
    }

    #[test]
    fn display_merge_parametric() {
        let s = TransformOp::Merge(Some(json!({"a": 1}))).to_string();
        assert!(s.starts_with("merge("), "should display as merge(...)");
    }

    // ─────────────────────────────────────────────────────────────
    // FIX-2: where operators
    // ─────────────────────────────────────────────────────────────

    #[test]
    fn where_gt_operator() {
        let data = json!([
            {"name": "A", "score": 90},
            {"name": "B", "score": 40},
            {"name": "C", "score": 75}
        ]);
        let result = TransformOp::Where("score".to_string(), "gt".to_string(), json!(70))
            .apply(&data)
            .unwrap();
        assert_eq!(result.as_array().unwrap().len(), 2);
        assert_eq!(result[0]["name"], "A");
        assert_eq!(result[1]["name"], "C");
    }

    #[test]
    fn where_lt_operator() {
        let data = json!([{"v": 1}, {"v": 5}, {"v": 10}]);
        let result = TransformOp::Where("v".to_string(), "lt".to_string(), json!(6))
            .apply(&data)
            .unwrap();
        assert_eq!(result.as_array().unwrap().len(), 2);
    }

    #[test]
    fn where_ne_operator() {
        let data = json!([{"s": "active"}, {"s": "deleted"}, {"s": "active"}]);
        let result = TransformOp::Where("s".to_string(), "ne".to_string(), json!("deleted"))
            .apply(&data)
            .unwrap();
        assert_eq!(result.as_array().unwrap().len(), 2);
    }

    #[test]
    fn where_contains_operator() {
        let data = json!([
            {"label": "hello world"},
            {"label": "goodbye"},
            {"label": "hello there"}
        ]);
        let result =
            TransformOp::Where("label".to_string(), "contains".to_string(), json!("hello"))
                .apply(&data)
                .unwrap();
        assert_eq!(result.as_array().unwrap().len(), 2);
    }

    #[test]
    fn where_gte_lte_operators() {
        let data = json!([{"v": 1}, {"v": 5}, {"v": 10}]);
        let gte = TransformOp::Where("v".to_string(), "gte".to_string(), json!(5))
            .apply(&data)
            .unwrap();
        assert_eq!(gte.as_array().unwrap().len(), 2); // 5, 10

        let lte = TransformOp::Where("v".to_string(), "lte".to_string(), json!(5))
            .apply(&data)
            .unwrap();
        assert_eq!(lte.as_array().unwrap().len(), 2); // 1, 5
    }

    #[test]
    fn parse_where_3_args() {
        let expr = TransformExpr::parse("where('score', 'gt', 80)").unwrap();
        assert_eq!(
            expr.ops.as_slice(),
            &[TransformOp::Where(
                "score".to_string(),
                "gt".to_string(),
                json!(80)
            )]
        );
    }

    #[test]
    fn parse_where_invalid_operator() {
        let result = TransformExpr::parse("where('score', 'invalid_op', 80)");
        assert!(result.is_err());
    }

    // ─────────────────────────────────────────────────────────────
    // FIX-5: dot-path support
    // ─────────────────────────────────────────────────────────────

    #[test]
    fn where_dot_path() {
        let data = json!([
            {"meta": {"score": 90}},
            {"meta": {"score": 40}},
            {"meta": {"score": 75}}
        ]);
        let result = TransformOp::Where("meta.score".to_string(), "gt".to_string(), json!(70))
            .apply(&data)
            .unwrap();
        assert_eq!(result.as_array().unwrap().len(), 2);
    }

    #[test]
    fn sort_by_dot_path() {
        let data = json!([
            {"info": {"age": 30}},
            {"info": {"age": 20}},
            {"info": {"age": 25}}
        ]);
        let result = TransformOp::SortBy("info.age".to_string())
            .apply(&data)
            .unwrap();
        assert_eq!(result[0]["info"]["age"], 20);
        assert_eq!(result[1]["info"]["age"], 25);
        assert_eq!(result[2]["info"]["age"], 30);
    }

    #[test]
    fn group_by_dot_path() {
        let data = json!([
            {"user": {"role": "admin"}, "id": 1},
            {"user": {"role": "user"}, "id": 2},
            {"user": {"role": "admin"}, "id": 3}
        ]);
        let result = TransformOp::GroupBy("user.role".to_string())
            .apply(&data)
            .unwrap();
        assert_eq!(result["admin"].as_array().unwrap().len(), 2);
        assert_eq!(result["user"].as_array().unwrap().len(), 1);
    }

    // ─────────────────────────────────────────────────────────────
    // FIX-6: regex cache (functional — verify same results)
    // ─────────────────────────────────────────────────────────────

    #[test]
    fn regex_cache_consistency() {
        // Same pattern applied multiple times should produce consistent results
        let pattern = r"\d+".to_string();
        let r1 = TransformOp::Regex(pattern.clone())
            .apply(&json!("abc123"))
            .unwrap();
        let r2 = TransformOp::Regex(pattern).apply(&json!("xyz456")).unwrap();
        assert_eq!(r1, json!("123"));
        assert_eq!(r2, json!("456"));
    }

    // ─────────────────────────────────────────────────────────────
    // FEAT-4: jq() transform
    // ─────────────────────────────────────────────────────────────

    #[test]
    fn jq_identity() {
        let data = json!({"a": 1, "b": 2});
        let result = TransformOp::Jq(".".to_string()).apply(&data).unwrap();
        assert_eq!(result, json!({"a": 1, "b": 2}));
    }

    #[test]
    fn jq_field_access() {
        let data = json!({"name": "Alice", "age": 30});
        let result = TransformOp::Jq(".name".to_string()).apply(&data).unwrap();
        assert_eq!(result, json!("Alice"));
    }

    #[test]
    fn jq_array_index() {
        let data = json!([10, 20, 30]);
        let result = TransformOp::Jq(".[1]".to_string()).apply(&data).unwrap();
        assert_eq!(result, json!(20));
    }

    #[test]
    fn jq_nested_access() {
        let data = json!({"user": {"address": {"city": "Paris"}}});
        let result = TransformOp::Jq(".user.address.city".to_string())
            .apply(&data)
            .unwrap();
        assert_eq!(result, json!("Paris"));
    }

    #[test]
    fn jq_object_construction() {
        let data = json!({"first": "Alice", "last": "Smith", "age": 30});
        let result = TransformOp::Jq("{name: .first, years: .age}".to_string())
            .apply(&data)
            .unwrap();
        assert_eq!(result, json!({"name": "Alice", "years": 30}));
    }

    #[test]
    fn jq_arithmetic() {
        let data = json!({"a": 10, "b": 3});
        let result = TransformOp::Jq(".a + .b".to_string()).apply(&data).unwrap();
        assert_eq!(result, json!(13));
    }

    #[test]
    fn jq_map_expression() {
        let data = json!([1, 2, 3]);
        let result = TransformOp::Jq("[.[] + 10]".to_string())
            .apply(&data)
            .unwrap();
        assert_eq!(result, json!([11, 12, 13]));
    }

    #[test]
    fn jq_null_input() {
        let result = TransformOp::Jq(".".to_string())
            .apply(&Value::Null)
            .unwrap();
        assert_eq!(result, Value::Null);
    }

    #[test]
    fn jq_stdlib_group_by() {
        let data = json!([
            {"locale": "en", "section": "blog"},
            {"locale": "en", "section": "docs"},
            {"locale": "fr", "section": "blog"}
        ]);
        let result = TransformOp::Jq(
            "[group_by(.locale)[] | {name: .[0].locale, count: length}]".to_string(),
        )
        .apply(&data)
        .unwrap();
        assert_eq!(
            result,
            json!([{"name": "en", "count": 2}, {"name": "fr", "count": 1}])
        );
    }

    #[test]
    fn jq_stdlib_map_select() {
        let data = json!([1, 2, 3, 4, 5]);
        let result = TransformOp::Jq("[.[] | select(. > 3)]".to_string())
            .apply(&data)
            .unwrap();
        assert_eq!(result, json!([4, 5]));
    }

    #[test]
    fn jq_stdlib_keys_length() {
        let data = json!({"a": 1, "b": 2, "c": 3});
        let result = TransformOp::Jq("keys | length".to_string())
            .apply(&data)
            .unwrap();
        assert_eq!(result, json!(3));
    }

    #[test]
    fn jq_stdlib_to_entries() {
        let data = json!({"name": "Alice", "age": 30});
        let result = TransformOp::Jq("to_entries | length".to_string())
            .apply(&data)
            .unwrap();
        assert_eq!(result, json!(2));
    }

    #[test]
    fn jq_stdlib_sort_by() {
        let data = json!([{"n": 3}, {"n": 1}, {"n": 2}]);
        let result = TransformOp::Jq("[sort_by(.n)[] | .n]".to_string())
            .apply(&data)
            .unwrap();
        assert_eq!(result, json!([1, 2, 3]));
    }

    #[test]
    fn jq_stdlib_nested_group_by() {
        // The exact pattern needed for site-audit dashboard TREE
        let data = json!([
            {"locale": "en", "section": "blog"},
            {"locale": "en", "section": "blog"},
            {"locale": "en", "section": "docs"},
            {"locale": "fr", "section": "blog"}
        ]);
        let result = TransformOp::Jq(
            "[group_by(.locale)[] | {name: .[0].locale, children: [group_by(.section)[] | {name: .[0].section, value: length}]}]".to_string()
        ).apply(&data).unwrap();
        assert_eq!(
            result,
            json!([
                {"name": "en", "children": [{"name": "blog", "value": 2}, {"name": "docs", "value": 1}]},
                {"name": "fr", "children": [{"name": "blog", "value": 1}]}
            ])
        );
    }

    #[test]
    fn jq_parse_error() {
        let result = TransformOp::Jq("[invalid!!!".to_string()).apply(&json!(1));
        assert!(result.is_err());
    }

    #[test]
    fn parse_jq() {
        let expr = TransformExpr::parse("jq('.name')").unwrap();
        assert_eq!(expr.ops.as_slice(), &[TransformOp::Jq(".name".to_string())]);
    }

    #[test]
    fn display_jq() {
        assert_eq!(
            TransformOp::Jq(".name".to_string()).to_string(),
            "jq('.name')"
        );
    }

    #[test]
    fn jq_regex_on_null_no_panic() {
        // jaq 1.5.x panicked on test("x") with null input.
        // jaq 3.x should return an error, not panic.
        let result = eval_jq("test(\"foo\")", &Value::Null);
        assert!(
            result.is_err(),
            "regex test() on null should error, not panic"
        );
    }

    // ── replace ─────────────────────────────────────────────

    #[test]
    fn replace_basic() {
        let val = json!("hello world");
        let result = TransformOp::Replace("world".into(), "rust".into())
            .apply(&val)
            .unwrap();
        assert_eq!(result, json!("hello rust"));
    }

    #[test]
    fn replace_multiple_occurrences() {
        let val = json!("aaa");
        let result = TransformOp::Replace("a".into(), "bb".into())
            .apply(&val)
            .unwrap();
        assert_eq!(result, json!("bbbbbb"));
    }

    #[test]
    fn replace_no_match() {
        let val = json!("hello");
        let result = TransformOp::Replace("xyz".into(), "abc".into())
            .apply(&val)
            .unwrap();
        assert_eq!(result, json!("hello"));
    }

    #[test]
    fn replace_to_empty() {
        let val = json!("remove-dashes");
        let result = TransformOp::Replace("-".into(), "".into())
            .apply(&val)
            .unwrap();
        assert_eq!(result, json!("removedashes"));
    }

    #[test]
    fn replace_null_fails() {
        assert!(TransformOp::Replace("a".into(), "b".into())
            .apply(&Value::Null)
            .is_err());
    }

    #[test]
    fn replace_non_string_fails() {
        assert!(TransformOp::Replace("a".into(), "b".into())
            .apply(&json!(42))
            .is_err());
    }

    #[test]
    fn replace_parse() {
        let expr = TransformExpr::parse("replace('hello', 'world')").unwrap();
        assert_eq!(expr.ops.len(), 1);
        assert_eq!(
            expr.ops[0],
            TransformOp::Replace("hello".into(), "world".into())
        );
    }

    #[test]
    fn replace_display() {
        assert_eq!(
            TransformOp::Replace("a".into(), "b".into()).to_string(),
            "replace('a', 'b')"
        );
    }

    // ── truncate ────────────────────────────────────────────

    #[test]
    fn truncate_basic() {
        let val = json!("hello world");
        let result = TransformOp::Truncate(5).apply(&val).unwrap();
        assert_eq!(result, json!("hello"));
    }

    #[test]
    fn truncate_longer_than_string() {
        let val = json!("hi");
        let result = TransformOp::Truncate(100).apply(&val).unwrap();
        assert_eq!(result, json!("hi"));
    }

    #[test]
    fn truncate_zero() {
        let val = json!("hello");
        let result = TransformOp::Truncate(0).apply(&val).unwrap();
        assert_eq!(result, json!(""));
    }

    #[test]
    fn truncate_unicode() {
        let val = json!("héllo wörld");
        let result = TransformOp::Truncate(5).apply(&val).unwrap();
        assert_eq!(result, json!("héllo"));
    }

    #[test]
    fn truncate_null_fails() {
        assert!(TransformOp::Truncate(5).apply(&Value::Null).is_err());
    }

    #[test]
    fn truncate_parse() {
        let expr = TransformExpr::parse("truncate(10)").unwrap();
        assert_eq!(expr.ops.len(), 1);
        assert_eq!(expr.ops[0], TransformOp::Truncate(10));
    }

    // ── add ─────────────────────────────────────────────────

    #[test]
    fn add_numbers() {
        let val = json!([1, 2, 3, 4, 5]);
        let result = TransformOp::Add.apply(&val).unwrap();
        assert_eq!(result, json!(15));
    }

    #[test]
    fn add_floats() {
        let val = json!([1.5, 2.3]);
        let result = TransformOp::Add.apply(&val).unwrap();
        // 1.5 + 2.3 = 3.8 — has fractional part, stays float
        assert_eq!(result.as_f64().unwrap(), 3.8);
    }

    #[test]
    fn add_strings() {
        let val = json!(["hello", " ", "world"]);
        let result = TransformOp::Add.apply(&val).unwrap();
        assert_eq!(result, json!("hello world"));
    }

    #[test]
    fn add_arrays() {
        let val = json!([[1, 2], [3, 4]]);
        let result = TransformOp::Add.apply(&val).unwrap();
        assert_eq!(result, json!([1, 2, 3, 4]));
    }

    #[test]
    fn add_empty_array() {
        let val = json!([]);
        let result = TransformOp::Add.apply(&val).unwrap();
        // Empty array → null (consistent with min/max/avg)
        assert_eq!(result, Value::Null);
    }

    #[test]
    fn add_with_nulls_skipped() {
        let val = json!([1, null, 3]);
        let result = TransformOp::Add.apply(&val).unwrap();
        assert_eq!(result, json!(4));
    }

    #[test]
    fn add_null_propagates() {
        assert_eq!(TransformOp::Add.apply(&Value::Null).unwrap(), Value::Null);
    }

    #[test]
    fn add_non_array_fails() {
        assert!(TransformOp::Add.apply(&json!("not an array")).is_err());
    }

    #[test]
    fn add_mixed_types_fails() {
        assert!(TransformOp::Add.apply(&json!([1, "two"])).is_err());
    }

    // ── min ─────────────────────────────────────────────────

    #[test]
    fn min_basic() {
        let val = json!([3, 1, 4, 1, 5]);
        let result = TransformOp::Min.apply(&val).unwrap();
        assert_eq!(result, json!(1));
    }

    #[test]
    fn min_floats() {
        let val = json!([3.16, 2.73, 1.41]);
        let result = TransformOp::Min.apply(&val).unwrap();
        assert_eq!(result, json!(1.41));
    }

    #[test]
    fn min_single_element() {
        let val = json!([42]);
        let result = TransformOp::Min.apply(&val).unwrap();
        assert_eq!(result, json!(42));
    }

    #[test]
    fn min_empty_array() {
        let val = json!([]);
        let result = TransformOp::Min.apply(&val).unwrap();
        assert_eq!(result, Value::Null);
    }

    #[test]
    fn min_with_nulls() {
        let val = json!([5, null, 2, null]);
        let result = TransformOp::Min.apply(&val).unwrap();
        assert_eq!(result, json!(2));
    }

    #[test]
    fn min_all_nulls() {
        let val = json!([null, null]);
        let result = TransformOp::Min.apply(&val).unwrap();
        assert_eq!(result, Value::Null);
    }

    #[test]
    fn min_null_propagates() {
        assert_eq!(TransformOp::Min.apply(&Value::Null).unwrap(), Value::Null);
    }

    #[test]
    fn min_negative() {
        let val = json!([-10, -5, -20]);
        let result = TransformOp::Min.apply(&val).unwrap();
        assert_eq!(result, json!(-20));
    }

    // ── max ─────────────────────────────────────────────────

    #[test]
    fn max_basic() {
        let val = json!([3, 1, 4, 1, 5]);
        let result = TransformOp::Max.apply(&val).unwrap();
        assert_eq!(result, json!(5));
    }

    #[test]
    fn max_floats() {
        let val = json!([3.16, 2.73, 1.41]);
        let result = TransformOp::Max.apply(&val).unwrap();
        assert_eq!(result, json!(3.16));
    }

    #[test]
    fn max_empty_array() {
        let val = json!([]);
        let result = TransformOp::Max.apply(&val).unwrap();
        assert_eq!(result, Value::Null);
    }

    #[test]
    fn max_with_nulls() {
        let val = json!([null, 3, null, 7]);
        let result = TransformOp::Max.apply(&val).unwrap();
        assert_eq!(result, json!(7));
    }

    #[test]
    fn max_null_propagates() {
        assert_eq!(TransformOp::Max.apply(&Value::Null).unwrap(), Value::Null);
    }

    #[test]
    fn max_non_array_fails() {
        assert!(TransformOp::Max.apply(&json!(42)).is_err());
    }

    // ── not ─────────────────────────────────────────────────

    #[test]
    fn not_true() {
        assert_eq!(TransformOp::Not.apply(&json!(true)).unwrap(), json!(false));
    }

    #[test]
    fn not_false() {
        assert_eq!(TransformOp::Not.apply(&json!(false)).unwrap(), json!(true));
    }

    #[test]
    fn not_null_propagates() {
        assert_eq!(TransformOp::Not.apply(&Value::Null).unwrap(), Value::Null);
    }

    #[test]
    fn not_non_bool_fails() {
        assert!(TransformOp::Not.apply(&json!("true")).is_err());
        assert!(TransformOp::Not.apply(&json!(1)).is_err());
    }

    // ── parsing and chaining ────────────────────────────────

    #[test]
    fn parse_add_min_max_not() {
        assert_eq!(
            TransformExpr::parse("add").unwrap().ops[0],
            TransformOp::Add
        );
        assert_eq!(
            TransformExpr::parse("min").unwrap().ops[0],
            TransformOp::Min
        );
        assert_eq!(
            TransformExpr::parse("max").unwrap().ops[0],
            TransformOp::Max
        );
        assert_eq!(
            TransformExpr::parse("not").unwrap().ops[0],
            TransformOp::Not
        );
    }

    #[test]
    fn chain_add_round() {
        let val = json!([1.1, 2.2, 3.3]);
        let expr = TransformExpr::parse("add | round").unwrap();
        let result = expr.apply(&val).unwrap();
        assert_eq!(result, json!(7));
    }

    #[test]
    fn chain_replace_upper() {
        let val = json!("hello world");
        let expr = TransformExpr::parse("replace('world', 'rust') | upper").unwrap();
        let result = expr.apply(&val).unwrap();
        assert_eq!(result, json!("HELLO RUST"));
    }

    #[test]
    fn chain_pluck_add() {
        let val = json!([{"score": 10}, {"score": 20}, {"score": 30}]);
        let expr = TransformExpr::parse("pluck('score') | add").unwrap();
        let result = expr.apply(&val).unwrap();
        assert_eq!(result, json!(60));
    }

    #[test]
    fn display_v069_transforms() {
        assert_eq!(TransformOp::Add.to_string(), "add");
        assert_eq!(TransformOp::Min.to_string(), "min");
        assert_eq!(TransformOp::Max.to_string(), "max");
        assert_eq!(TransformOp::Not.to_string(), "not");
        assert_eq!(TransformOp::Truncate(10).to_string(), "truncate(10)");
        assert_eq!(
            TransformOp::Replace("a".into(), "b".into()).to_string(),
            "replace('a', 'b')"
        );
        assert_eq!(TransformOp::Sum.to_string(), "sum");
        assert_eq!(TransformOp::Avg.to_string(), "avg");
        assert_eq!(
            TransformOp::MinBy("score".into()).to_string(),
            "min_by('score')"
        );
        assert_eq!(
            TransformOp::MaxBy("score".into()).to_string(),
            "max_by('score')"
        );
        assert_eq!(TransformOp::Has("name".into()).to_string(), "has('name')");
    }

    // ── min_by / max_by ─────────────────────────────────────

    #[test]
    fn min_by_basic() {
        let val = json!([{"name": "a", "score": 30}, {"name": "b", "score": 10}, {"name": "c", "score": 20}]);
        let result = TransformOp::MinBy("score".into()).apply(&val).unwrap();
        assert_eq!(result["name"], json!("b"));
        assert_eq!(result["score"], json!(10));
    }

    #[test]
    fn max_by_basic() {
        let val = json!([{"name": "a", "score": 30}, {"name": "b", "score": 10}]);
        let result = TransformOp::MaxBy("score".into()).apply(&val).unwrap();
        assert_eq!(result["name"], json!("a"));
    }

    #[test]
    fn min_by_empty_array() {
        assert_eq!(
            TransformOp::MinBy("x".into()).apply(&json!([])).unwrap(),
            Value::Null
        );
    }

    #[test]
    fn max_by_null_propagates() {
        assert_eq!(
            TransformOp::MaxBy("x".into()).apply(&Value::Null).unwrap(),
            Value::Null
        );
    }

    #[test]
    fn min_by_dot_path() {
        let val = json!([{"meta": {"score": 5}}, {"meta": {"score": 2}}]);
        let result = TransformOp::MinBy("meta.score".into()).apply(&val).unwrap();
        assert_eq!(result["meta"]["score"], json!(2));
    }

    // ── sum / avg ───────────────────────────────────────────

    #[test]
    fn sum_numeric_only() {
        let val = json!([10, 20, 30]);
        assert_eq!(TransformOp::Sum.apply(&val).unwrap(), json!(60));
    }

    #[test]
    fn sum_rejects_strings() {
        // sum is numeric-only (unlike add which also concats strings)
        let val = json!(["a", "b", "c"]);
        assert!(TransformOp::Sum.apply(&val).is_err());
    }

    #[test]
    fn sum_rejects_arrays() {
        // sum is numeric-only (unlike add which also concats arrays)
        let val = json!([[1], [2], [3]]);
        assert!(TransformOp::Sum.apply(&val).is_err());
    }

    #[test]
    fn sum_null_and_empty() {
        assert_eq!(TransformOp::Sum.apply(&Value::Null).unwrap(), Value::Null);
        assert_eq!(TransformOp::Sum.apply(&json!([])).unwrap(), Value::Null);
    }

    #[test]
    fn sum_with_nulls_skips_them() {
        let val = json!([10, null, 20]);
        assert_eq!(TransformOp::Sum.apply(&val).unwrap(), json!(30));
    }

    #[test]
    fn add_still_concats_strings() {
        // add retains its polymorphic behavior
        let val = json!(["hello", " ", "world"]);
        assert_eq!(TransformOp::Add.apply(&val).unwrap(), json!("hello world"));
    }

    #[test]
    fn avg_basic() {
        let val = json!([10, 20, 30]);
        let result = TransformOp::Avg.apply(&val).unwrap();
        assert_eq!(result.as_f64().unwrap(), 20.0);
    }

    #[test]
    fn avg_with_nulls() {
        let val = json!([10, null, 20]);
        let result = TransformOp::Avg.apply(&val).unwrap();
        assert_eq!(result.as_f64().unwrap(), 15.0); // only 2 numbers
    }

    #[test]
    fn avg_empty_array() {
        assert_eq!(TransformOp::Avg.apply(&json!([])).unwrap(), Value::Null);
    }

    #[test]
    fn avg_null_propagates() {
        assert_eq!(TransformOp::Avg.apply(&Value::Null).unwrap(), Value::Null);
    }

    // ── has ─────────────────────────────────────────────────

    #[test]
    fn has_key_present() {
        let val = json!({"name": "Alice", "age": 30});
        assert_eq!(
            TransformOp::Has("name".into()).apply(&val).unwrap(),
            json!(true)
        );
    }

    #[test]
    fn has_key_absent() {
        let val = json!({"name": "Alice"});
        assert_eq!(
            TransformOp::Has("age".into()).apply(&val).unwrap(),
            json!(false)
        );
    }

    #[test]
    fn has_null_propagates() {
        assert_eq!(
            TransformOp::Has("x".into()).apply(&Value::Null).unwrap(),
            Value::Null
        );
    }

    #[test]
    fn has_non_object_fails() {
        assert!(TransformOp::Has("x".into()).apply(&json!([1, 2])).is_err());
    }

    // ── parsing ─────────────────────────────────────────────

    #[test]
    fn parse_new_s6_transforms() {
        assert_eq!(
            TransformExpr::parse("sum").unwrap().ops[0],
            TransformOp::Sum
        );
        assert_eq!(
            TransformExpr::parse("avg").unwrap().ops[0],
            TransformOp::Avg
        );
        assert_eq!(
            TransformExpr::parse("min_by('score')").unwrap().ops[0],
            TransformOp::MinBy("score".into())
        );
        assert_eq!(
            TransformExpr::parse("max_by('score')").unwrap().ops[0],
            TransformOp::MaxBy("score".into())
        );
        assert_eq!(
            TransformExpr::parse("has('name')").unwrap().ops[0],
            TransformOp::Has("name".into())
        );
    }

    #[test]
    fn chain_pluck_avg() {
        let val = json!([{"score": 10}, {"score": 20}, {"score": 30}]);
        let expr = TransformExpr::parse("pluck('score') | avg").unwrap();
        let result = expr.apply(&val).unwrap();
        assert_eq!(result.as_f64().unwrap(), 20.0);
    }
}

// ═══════════════════════════════════════════════════════════════
// Property-based tests (proptest)
// ═══════════════════════════════════════════════════════════════

#[cfg(test)]
mod proptest_tests {
    use super::*;
    use proptest::prelude::*;
    use serde_json::{json, Number, Value};

    /// Strategy for arbitrary JSON values (bounded depth)
    fn arb_json_value() -> impl Strategy<Value = Value> {
        let leaf = prop_oneof![
            Just(Value::Null),
            any::<bool>().prop_map(Value::Bool),
            any::<i64>().prop_map(|n| Value::Number(n.into())),
            any::<f64>()
                .prop_filter("finite", |f| f.is_finite())
                .prop_map(|f| {
                    Number::from_f64(f)
                        .map(Value::Number)
                        .unwrap_or(Value::Null)
                }),
            "\\PC{0,100}".prop_map(Value::String),
        ];
        leaf.prop_recursive(
            3,  // depth
            64, // max nodes
            8,  // items per collection
            |inner| {
                prop_oneof![
                    prop::collection::vec(inner.clone(), 0..8).prop_map(Value::Array),
                    prop::collection::hash_map("\\PC{1,20}", inner, 0..5)
                        .prop_map(|m| Value::Object(m.into_iter().collect())),
                ]
            },
        )
    }

    /// All simple (non-parameterized) transform op names
    fn arb_simple_transform_name() -> impl Strategy<Value = String> {
        prop_oneof![
            Just("upper".into()),
            Just("lower".into()),
            Just("trim".into()),
            Just("trim_start".into()),
            Just("trim_end".into()),
            Just("length".into()),
            Just("first".into()),
            Just("last".into()),
            Just("keys".into()),
            Just("values".into()),
            Just("flatten".into()),
            Just("reverse".into()),
            Just("sort".into()),
            Just("unique".into()),
            Just("compact".into()),
            Just("to_string".into()),
            Just("to_number".into()),
            Just("to_bool".into()),
            Just("to_json".into()),
            Just("parse_json".into()),
            Just("parse_yaml".into()),
            Just("round".into()),
            Just("abs".into()),
            Just("ceil".into()),
            Just("floor".into()),
            Just("type_of".into()),
            Just("shell".into()),
        ]
    }

    /// Parameterized transform names
    fn arb_param_transform_name() -> impl Strategy<Value = String> {
        prop_oneof![
            (0usize..100).prop_map(|n| format!("first({})", n)),
            (0usize..100).prop_map(|n| format!("last({})", n)),
            (0u32..10).prop_map(|n| format!("round({})", n)),
            "\\PC{0,20}".prop_map(|s| format!("join('{}')", s.replace('\'', ""))),
            "\\PC{0,20}".prop_map(|s| format!("split('{}')", s.replace('\'', ""))),
            "\\PC{0,20}".prop_map(|s| format!("default('{}')", s.replace('\'', ""))),
        ]
    }

    proptest! {
        // ── Property 1: No transform panics on any JSON value ──
        #[test]
        fn transform_never_panics(
            value in arb_json_value(),
            op_name in prop_oneof![arb_simple_transform_name(), arb_param_transform_name()]
        ) {
            if let Ok(expr) = TransformExpr::parse(&op_name) {
                let _ = expr.apply(&value); // MUST NOT panic — Err is fine
            }
        }

        // ── Property 2: Null input on failing transforms returns NullInput error ──
        #[test]
        fn null_on_failing_transform_returns_error(
            op_name in prop_oneof![
                Just("upper".to_string()), Just("lower".to_string()),
                Just("trim".to_string()), Just("trim_start".to_string()),
                Just("trim_end".to_string()), Just("first".to_string()),
                Just("last".to_string()),
                Just("flatten".to_string()), Just("reverse".to_string()),
                Just("sort".to_string()), Just("unique".to_string()),
                Just("compact".to_string()), Just("to_number".to_string()),
                Just("to_bool".to_string()), Just("parse_json".to_string()), Just("parse_yaml".to_string()),
                Just("round".to_string()), Just("abs".to_string()),
                Just("ceil".to_string()), Just("floor".to_string()),
                Just("join(',')".to_string()), Just("split(',')".to_string()),
            ]
        ) {
            let expr = TransformExpr::parse(&op_name).unwrap();
            let result = expr.apply(&Value::Null);
            prop_assert!(result.is_err(), "Expected error for {} on null, got {:?}", op_name, result);
            match result {
                Err(TransformError::NullInput { .. }) => {} // Expected
                other => prop_assert!(false, "Expected NullInput for {}, got {:?}", op_name, other),
            }
        }

        // ── Property 3: Propagating transforms on null return ok ──
        #[test]
        fn null_on_propagating_transform_returns_ok(
            op_name in prop_oneof![
                Just("length".to_string()), Just("keys".to_string()),
                Just("values".to_string()),
                Just("to_string".to_string()), Just("to_json".to_string()),
                Just("type_of".to_string()),
            ]
        ) {
            let expr = TransformExpr::parse(&op_name).unwrap();
            let result = expr.apply(&Value::Null);
            prop_assert!(result.is_ok(), "Expected ok for {} on null, got {:?}", op_name, result);
        }

        // ── Property 4: default() always returns non-null ──
        #[test]
        fn default_on_null_returns_non_null(
            default_val in "[a-zA-Z0-9 ]{0,30}"
        ) {
            let expr_str = format!("default('{}')", default_val);
            if let Ok(expr) = TransformExpr::parse(&expr_str) {
                let result = expr.apply(&Value::Null);
                prop_assert!(result.is_ok(), "default should always succeed");
                prop_assert!(!result.unwrap().is_null(), "default on null must not return null");
            }
        }

        // ── Property 5: shell escape always wraps in single quotes ──
        #[test]
        fn shell_escape_always_single_quoted(input in "\\PC{0,100}") {
            let expr = TransformExpr::parse("shell").unwrap();
            let result = expr.apply(&Value::String(input)).unwrap();
            if let Value::String(s) = result {
                prop_assert!(s.starts_with('\''), "shell escape must start with '");
                prop_assert!(s.ends_with('\''), "shell escape must end with '");
            } else {
                prop_assert!(false, "shell should return a string");
            }
        }

        // ── Property 6: sort is idempotent ──
        #[test]
        fn sort_is_idempotent(items in prop::collection::vec(any::<i64>(), 0..20)) {
            let arr = Value::Array(items.iter().map(|n| json!(n)).collect());
            let expr = TransformExpr::parse("sort").unwrap();
            let once = expr.apply(&arr).unwrap();
            let twice = expr.apply(&once).unwrap();
            prop_assert_eq!(once, twice);
        }

        // ── Property 7: unique is idempotent ──
        #[test]
        fn unique_is_idempotent(items in prop::collection::vec(0i64..10, 0..20)) {
            let arr = Value::Array(items.iter().map(|n| json!(n)).collect());
            let expr = TransformExpr::parse("unique").unwrap();
            let once = expr.apply(&arr).unwrap();
            let twice = expr.apply(&once).unwrap();
            prop_assert_eq!(once, twice);
        }

        // ── Property 8: reverse is involution (f(f(x)) == x) ──
        #[test]
        fn reverse_is_involution(items in prop::collection::vec(any::<i64>(), 0..20)) {
            let arr = Value::Array(items.iter().map(|n| json!(n)).collect());
            let expr = TransformExpr::parse("reverse").unwrap();
            let once = expr.apply(&arr).unwrap();
            let twice = expr.apply(&once).unwrap();
            prop_assert_eq!(arr, twice);
        }

        // ── Property 9: compact removes all nulls and empty strings ──
        #[test]
        fn compact_no_nulls_or_empty(items in prop::collection::vec(
            prop_oneof![
                Just(Value::Null),
                any::<i64>().prop_map(|n| json!(n)),
                Just(json!("hello")),
                Just(json!("")),
            ],
            0..20
        )) {
            let arr = Value::Array(items);
            let result = TransformExpr::parse("compact").unwrap().apply(&arr).unwrap();
            if let Value::Array(ref compacted) = result {
                for v in compacted {
                    prop_assert!(!v.is_null(), "compact must remove nulls");
                    prop_assert!(v != &json!(""), "compact must remove empty strings");
                }
            }
        }

        // ── Property 10: to_json then parse_json roundtrip for integers ──
        #[test]
        fn to_json_parse_json_roundtrip(n in any::<i64>()) {
            let val = json!(n);
            let as_json = TransformExpr::parse("to_json").unwrap().apply(&val).unwrap();
            let back = TransformExpr::parse("parse_json").unwrap().apply(&as_json).unwrap();
            prop_assert_eq!(val, back);
        }

        // ── Property 11: parse never panics on arbitrary strings ──
        #[test]
        fn transform_parse_no_panic(input in "\\PC{0,200}") {
            let _ = TransformExpr::parse(&input); // Must not panic
        }

        // ── Property 12: pipe chain parse never panics ──
        #[test]
        fn pipe_chain_parse_no_panic(
            ops in prop::collection::vec(arb_simple_transform_name(), 1..10)
        ) {
            let chain = ops.join(" | ");
            let _ = TransformExpr::parse(&chain); // Must not panic
        }

        // ── Property 13: flatten total == sum of inner lengths ──
        #[test]
        fn flatten_total_equals_sum_of_inner(
            items in prop::collection::vec(
                prop::collection::vec(any::<i64>(), 0..5)
                    .prop_map(|v| Value::Array(v.into_iter().map(|n| json!(n)).collect())),
                0..10,
            )
        ) {
            let expected_len: usize = items.iter().map(|v| {
                if let Value::Array(a) = v { a.len() } else { 0 }
            }).sum();
            let arr = Value::Array(items);
            let flat = TransformExpr::parse("flatten").unwrap().apply(&arr).unwrap();
            if let Value::Array(ref f) = flat {
                prop_assert_eq!(f.len(), expected_len, "flatten total must equal sum of inner lengths");
            }
        }
    }

    // ─────────────────────────────────────────────────────────────
    // split_pipe_respecting_parens — quote tracking
    // ─────────────────────────────────────────────────────────────

    #[test]
    fn pipe_split_unquoted_apostrophe_in_parens() {
        // "filter(it's)" should NOT break the pipe parser
        let parts = split_pipe_respecting_parens("join(it's) | upper");
        assert_eq!(
            parts.len(),
            2,
            "apostrophe inside parens must not break split"
        );
        assert_eq!(parts[0].trim(), "join(it's)");
        assert_eq!(parts[1].trim(), "upper");
    }

    #[test]
    fn pipe_split_quoted_pipe_in_parens() {
        let parts = split_pipe_respecting_parens(r#"join(" | ") | upper"#);
        assert_eq!(parts.len(), 2);
        assert_eq!(parts[0].trim(), r#"join(" | ")"#);
    }

    #[test]
    fn pipe_split_top_level_apostrophe() {
        // Apostrophe at top level (no parens) should not break splitting
        let parts = split_pipe_respecting_parens("it's a test | upper");
        assert_eq!(parts.len(), 2);
    }

    // ─────────────────────────────────────────────────────────────
    // has_default() tests
    // ─────────────────────────────────────────────────────────────

    #[test]
    fn has_default_true_for_default_transform() {
        let expr = TransformExpr::parse("default(\"x\")").unwrap();
        assert!(expr.has_default());
    }

    #[test]
    fn has_default_true_in_chain() {
        let expr = TransformExpr::parse("default(\"x\") | upper").unwrap();
        assert!(expr.has_default());
    }

    #[test]
    fn has_default_false_without_default() {
        let expr = TransformExpr::parse("upper | trim").unwrap();
        assert!(!expr.has_default());
    }

    // ─────────────────────────────────────────────────────────────
    // starts_with / ends_with / contains
    // ─────────────────────────────────────────────────────────────

    #[test]
    fn starts_with_true() {
        let expr = TransformExpr::parse("starts_with('/api')").unwrap();
        assert_eq!(expr.apply(&json!("/api/users")).unwrap(), json!(true));
    }

    #[test]
    fn starts_with_false() {
        let expr = TransformExpr::parse("starts_with('/api')").unwrap();
        assert_eq!(expr.apply(&json!("/blog/post")).unwrap(), json!(false));
    }

    #[test]
    fn ends_with_true() {
        let expr = TransformExpr::parse("ends_with('.html')").unwrap();
        assert_eq!(expr.apply(&json!("page.html")).unwrap(), json!(true));
    }

    #[test]
    fn ends_with_false() {
        let expr = TransformExpr::parse("ends_with('.html')").unwrap();
        assert_eq!(expr.apply(&json!("page.json")).unwrap(), json!(false));
    }

    #[test]
    fn contains_true() {
        let expr = TransformExpr::parse("contains('world')").unwrap();
        assert_eq!(expr.apply(&json!("hello world")).unwrap(), json!(true));
    }

    #[test]
    fn contains_false() {
        let expr = TransformExpr::parse("contains('xyz')").unwrap();
        assert_eq!(expr.apply(&json!("hello world")).unwrap(), json!(false));
    }

    // ─────────────────────────────────────────────────────────────
    // content_hash
    // ─────────────────────────────────────────────────────────────

    #[test]
    fn content_hash_deterministic() {
        let expr = TransformExpr::parse("content_hash").unwrap();
        let h1 = expr.apply(&json!("hello")).unwrap();
        let h2 = expr.apply(&json!("hello")).unwrap();
        assert_eq!(h1, h2);
        // Verify it's a 16-char hex string
        assert_eq!(h1.as_str().unwrap().len(), 16);
    }

    #[test]
    fn content_hash_different_input() {
        let expr = TransformExpr::parse("content_hash").unwrap();
        let h1 = expr.apply(&json!("hello")).unwrap();
        let h2 = expr.apply(&json!("world")).unwrap();
        assert_ne!(h1, h2);
    }

    #[test]
    fn content_hash_object() {
        let expr = TransformExpr::parse("content_hash").unwrap();
        let result = expr.apply(&json!({"a": 1})).unwrap();
        assert_eq!(result.as_str().unwrap().len(), 16);
    }

    // ─────────────────────────────────────────────────────────────
    // unique_urls
    // ─────────────────────────────────────────────────────────────

    #[test]
    fn unique_urls_dedup_tracking_params() {
        let expr = TransformExpr::parse("unique_urls").unwrap();
        let input = json!([
            "https://example.com/page?utm_source=twitter",
            "https://example.com/page?utm_source=facebook",
            "https://example.com/other"
        ]);
        let result = expr.apply(&input).unwrap();
        let arr = result.as_array().unwrap();
        assert_eq!(arr.len(), 2); // first two normalize to same URL
    }

    #[test]
    fn unique_urls_preserves_order() {
        let expr = TransformExpr::parse("unique_urls").unwrap();
        let input = json!(["https://b.com", "https://a.com", "https://b.com"]);
        let result = expr.apply(&input).unwrap();
        let arr = result.as_array().unwrap();
        assert_eq!(arr.len(), 2);
        assert_eq!(arr[0], json!("https://b.com"));
        assert_eq!(arr[1], json!("https://a.com"));
    }
}
