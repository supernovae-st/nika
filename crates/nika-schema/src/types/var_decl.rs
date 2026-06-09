// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Workflow input declarations — the envelope `vars:` block.
//!
//! Per spec `01-envelope.md` §vars · « The **untyped form** (`name: value`)
//! is the value's default … The **typed form** (`name: { type, required,
//! default, description }`) lets the engine validate inputs and **generate
//! a callable schema** ».

use std::fmt;

use serde::{Deserialize, Serialize};

/// The closed type vocabulary for typed `vars:` / typed `outputs:`.
///
/// Spec `01-envelope.md` §vars · « type: string · number · integer ·
/// boolean · array · object ».
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[non_exhaustive]
#[serde(rename_all = "lowercase")]
pub enum VarType {
    /// A UTF-8 string.
    String,
    /// Any JSON number.
    Number,
    /// A JSON integer.
    Integer,
    /// A boolean.
    Boolean,
    /// A JSON array.
    Array,
    /// A JSON object.
    Object,
}

impl VarType {
    /// Parse the YAML `type:` scalar. Returns `None` outside the closed
    /// 6-value vocabulary.
    #[must_use]
    pub fn from_str_opt(s: &str) -> Option<Self> {
        match s {
            "string" => Some(Self::String),
            "number" => Some(Self::Number),
            "integer" => Some(Self::Integer),
            "boolean" => Some(Self::Boolean),
            "array" => Some(Self::Array),
            "object" => Some(Self::Object),
            _ => None,
        }
    }
}

impl fmt::Display for VarType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::String => write!(f, "string"),
            Self::Number => write!(f, "number"),
            Self::Integer => write!(f, "integer"),
            Self::Boolean => write!(f, "boolean"),
            Self::Array => write!(f, "array"),
            Self::Object => write!(f, "object"),
        }
    }
}

/// One `vars:` entry — untyped (the value IS the default) or typed
/// (validation + callable-schema generation).
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub enum VarDecl {
    /// Untyped form · `output_dir: "./output"` — the value is the default.
    Untyped(serde_json::Value),
    /// Typed form · `topic: { type, required, default, description }`.
    Typed {
        /// The declared type (closed 6-value vocabulary).
        r#type: VarType,
        /// Whether the caller must provide this input (default `false`).
        required: bool,
        /// Default used when the caller omits the input.
        default: Option<serde_json::Value>,
        /// Human-readable description (LSP hover · callable schema).
        description: Option<String>,
    },
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn var_type_closed_vocabulary() {
        assert_eq!(VarType::from_str_opt("string"), Some(VarType::String));
        assert_eq!(VarType::from_str_opt("number"), Some(VarType::Number));
        assert_eq!(VarType::from_str_opt("integer"), Some(VarType::Integer));
        assert_eq!(VarType::from_str_opt("boolean"), Some(VarType::Boolean));
        assert_eq!(VarType::from_str_opt("array"), Some(VarType::Array));
        assert_eq!(VarType::from_str_opt("object"), Some(VarType::Object));
        assert_eq!(VarType::from_str_opt("str"), None);
        assert_eq!(VarType::from_str_opt("String"), None); // case-sensitive
    }

    #[test]
    fn display_round_trips_with_parse() {
        for t in [
            VarType::String,
            VarType::Number,
            VarType::Integer,
            VarType::Boolean,
            VarType::Array,
            VarType::Object,
        ] {
            assert_eq!(VarType::from_str_opt(&t.to_string()), Some(t));
        }
    }

    #[test]
    fn untyped_holds_value() {
        let v = VarDecl::Untyped(serde_json::json!("./output"));
        assert!(matches!(v, VarDecl::Untyped(ref val) if val == "./output"));
    }

    #[test]
    fn typed_defaults() {
        let v = VarDecl::Typed {
            r#type: VarType::String,
            required: true,
            default: None,
            description: Some("Subject to research".into()),
        };
        let VarDecl::Typed { required, .. } = v else {
            panic!("expected Typed");
        };
        assert!(required);
    }
}
