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

    /// Coerce a CLI `--var KEY=VALUE` raw string into a JSON value that
    /// HONORS this declared type — the declared type DRIVES the parse, it
    /// does not merely validate a type-blind JSON guess. A `string` var
    /// takes the raw text verbatim (`--var name=5` is the string `"5"`,
    /// never the number 5); the scalar types parse their own lexical form
    /// and reject anything else (`--var count=notanumber` on an `integer`
    /// input is refused UP FRONT, not silently embedded); `array`/`object`
    /// JSON-parse and shape-check.
    ///
    /// # Errors
    ///
    /// A one-line message naming the expected type + the offending value,
    /// ready for the CLI's `--var <key>: …` frame.
    pub fn coerce_cli(self, raw: &str) -> Result<serde_json::Value, String> {
        use serde_json::Value;
        let wrong = |want: &str| format!("expects {want}, got `{raw}`");
        match self {
            // A string takes the raw text verbatim — CLI values are strings
            // by nature, so a `string` input never surprises the caller.
            Self::String => Ok(Value::String(raw.to_owned())),
            Self::Integer => raw
                .parse::<i64>()
                .map(|n| Value::Number(n.into()))
                .map_err(|_| wrong("an integer")),
            Self::Number => raw
                .parse::<f64>()
                .ok()
                .and_then(serde_json::Number::from_f64)
                .map(Value::Number)
                .ok_or_else(|| wrong("a number")),
            Self::Boolean => match raw {
                "true" => Ok(Value::Bool(true)),
                "false" => Ok(Value::Bool(false)),
                _ => Err(wrong("a boolean (`true` or `false`)")),
            },
            Self::Array => match serde_json::from_str::<Value>(raw) {
                Ok(v @ Value::Array(_)) => Ok(v),
                _ => Err(wrong("a JSON array (e.g. `[1,2]`)")),
            },
            Self::Object => match serde_json::from_str::<Value>(raw) {
                Ok(v @ Value::Object(_)) => Ok(v),
                _ => Err(wrong("a JSON object (e.g. `{\"k\":1}`)")),
            },
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
    fn coerce_cli_lets_the_declared_type_drive() {
        use serde_json::json;
        // string · verbatim, never JSON-coerced (`5` stays "5").
        assert_eq!(VarType::String.coerce_cli("5").unwrap(), json!("5"));
        assert_eq!(
            VarType::String.coerce_cli("hi there").unwrap(),
            json!("hi there")
        );
        // integer · whole numbers only (5.5 and words rejected).
        assert_eq!(VarType::Integer.coerce_cli("42").unwrap(), json!(42));
        assert!(VarType::Integer.coerce_cli("5.5").is_err());
        assert!(VarType::Integer.coerce_cli("notanumber").is_err());
        // number · any finite float; NaN/inf have no JSON form → rejected.
        assert_eq!(VarType::Number.coerce_cli("2.5").unwrap(), json!(2.5));
        assert_eq!(VarType::Number.coerce_cli("7").unwrap(), json!(7.0));
        assert!(VarType::Number.coerce_cli("NaN").is_err());
        // boolean · exactly true|false.
        assert_eq!(VarType::Boolean.coerce_cli("true").unwrap(), json!(true));
        assert_eq!(VarType::Boolean.coerce_cli("false").unwrap(), json!(false));
        assert!(VarType::Boolean.coerce_cli("maybe").is_err());
        // array/object · JSON-parse AND shape-check (a bare number is neither).
        assert_eq!(VarType::Array.coerce_cli("[1,2]").unwrap(), json!([1, 2]));
        assert!(VarType::Array.coerce_cli("{}").is_err());
        assert_eq!(
            VarType::Object.coerce_cli(r#"{"k":1}"#).unwrap(),
            json!({"k": 1})
        );
        assert!(VarType::Object.coerce_cli("[1]").is_err());
        // the message names the type + the offending value.
        let err = VarType::Integer.coerce_cli("nope").unwrap_err();
        assert!(err.contains("integer") && err.contains("nope"), "{err}");
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
