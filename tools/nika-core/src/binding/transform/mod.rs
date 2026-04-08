// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

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

mod apply;
mod helpers;
mod parser;

pub use apply::{deep_merge, eval_jq, navigate_dot_path};

#[cfg(test)]
mod tests;
