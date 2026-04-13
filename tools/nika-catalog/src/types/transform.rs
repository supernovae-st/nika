// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Pipe transform definitions (65 transforms).
//!
//! Stored in a sorted array for case-sensitive binary search.
//! Transform execution lives in `nika-binding` — this is metadata only.

/// A known pipe transform (e.g. `| upper`, `| join(", ")`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TransformDef {
    /// Transform name (e.g. `"upper"`). Always lowercase.
    pub name: &'static str,
    /// How many arguments the transform takes.
    pub arity: TransformArity,
    /// What happens when the input is null.
    pub null_behavior: NullBehavior,
    /// Functional category for documentation grouping.
    pub category: TransformCategory,
}

/// Number of arguments a transform accepts.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "kebab-case"))]
#[non_exhaustive]
pub enum TransformArity {
    /// No arguments (e.g. `| upper`).
    Nullary,
    /// One argument (e.g. `| join(", ")`).
    Unary,
    /// Variable arguments (e.g. `| pick(f1, f2, f3)`).
    Variadic,
}

/// Behavior when the transform receives a null/missing input.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "kebab-case"))]
#[non_exhaustive]
pub enum NullBehavior {
    /// Null passes through unchanged.
    Propagate,
    /// Null causes an error.
    Fail,
}

/// Functional category for transform grouping.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "kebab-case"))]
#[non_exhaustive]
pub enum TransformCategory {
    /// String operations: upper, lower, trim, etc.
    String,
    /// Array operations: first, last, flatten, sort, etc.
    Array,
    /// Aggregation operations: sum, avg, min, max, etc.
    Aggregation,
    /// Numeric operations: `round`, `abs`, `ceil`, `floor`, etc.
    Numeric,
    /// Type conversion: `to_string`, `to_number`, `to_bool`, etc.
    Type,
    /// Logic operations: not.
    Logic,
    /// Introspection: `has`, `type_of`.
    Introspection,
    /// Parametric operations: join, split, default, slice, etc.
    Parametric,
    /// Query operations: pluck, where, pick, omit, etc.
    Query,
    /// String test operations: `starts_with`, `ends_with`, `contains`.
    StringTest,
    /// URL operations: `url_host`, `url_path`, etc.
    Url,
    /// Encoding operations: `content_hash`, `unique_urls`, `base64_encode`, `base64_decode`.
    Encoding,
    /// JQ operations: jq.
    Jq,
    /// System operations: shell.
    System,
    /// HTML/Markdown escape operations.
    Escape,
}
