// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Structured JSON output capability level.
//!
//! Replaces `ParamFlag::StructuredOutputNative` (Session 4a) with a 3-level
//! enum that distinguishes `json_object` (format-only) from `json_schema`
//! (full schema enforcement).

/// Structured JSON output capability of a model.
///
/// Three levels, ordered by enforcement strength:
///
/// | Level | Wire-protocol meaning | Providers |
/// |-------|----------------------|-----------|
/// | [`JsonMode::Unavailable`] | No JSON mode at all | `Voyage`, native GGUF |
/// | [`JsonMode::Object`] | `json_object` format — valid JSON, no schema | `DeepSeek`, `AI21` |
/// | [`JsonMode::Schema`] | `json_schema` — provider enforces schema | `OpenAI`, `Anthropic`, `Gemini`, `Mistral`, `xAI` |
///
/// The runtime uses this to decide: `Schema` → emit `response_format =
/// { type: "json_schema", … }`; `Object` → emit `{ type: "json_object" }`;
/// `Unavailable` → prompt-engineering fallback.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "kebab-case"))]
#[non_exhaustive]
pub enum JsonMode {
    /// Model does not support structured JSON output at all.
    Unavailable,
    /// Model supports `json_object` response format — guarantees valid JSON
    /// but does NOT enforce a schema. `DeepSeek`, `AI21`/Jamba.
    Object,
    /// Model supports `json_schema` enforcement — provider validates output
    /// against a caller-supplied schema. `OpenAI`, `Anthropic`, `Gemini`,
    /// `Mistral`, `xAI`, `Perplexity`.
    Schema,
}

impl JsonMode {
    /// Canonical kebab-case string used in TOML and serde output.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Unavailable => "unavailable",
            Self::Object => "object",
            Self::Schema => "schema",
        }
    }
}

impl core::fmt::Display for JsonMode {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Failure when parsing a [`JsonMode`] from its canonical string.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct ParseJsonModeError {
    /// The raw input that failed to match.
    pub input: String,
}

impl ParseJsonModeError {
    /// Explicit constructor — `#[non_exhaustive]` invariant #19.
    #[must_use]
    pub const fn new(input: String) -> Self {
        Self { input }
    }
}

impl core::fmt::Display for ParseJsonModeError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(
            f,
            "unknown json_mode {:?} — expected one of: unavailable, object, schema",
            self.input,
        )
    }
}

impl std::error::Error for ParseJsonModeError {}

impl core::str::FromStr for JsonMode {
    type Err = ParseJsonModeError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "unavailable" => Ok(Self::Unavailable),
            "object" => Ok(Self::Object),
            "schema" => Ok(Self::Schema),
            _ => Err(ParseJsonModeError::new(s.to_string())),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{JsonMode, ParseJsonModeError};
    use core::str::FromStr;

    #[test]
    fn as_str_roundtrip() {
        for mode in [JsonMode::Unavailable, JsonMode::Object, JsonMode::Schema] {
            let s = mode.as_str();
            let parsed = JsonMode::from_str(s).expect("round-trip must succeed");
            assert_eq!(parsed, mode, "as_str({mode:?}) → {s:?} did not round-trip");
        }
    }

    #[test]
    fn display_matches_as_str() {
        assert_eq!(format!("{}", JsonMode::Schema), "schema");
        assert_eq!(format!("{}", JsonMode::Object), "object");
        assert_eq!(format!("{}", JsonMode::Unavailable), "unavailable");
    }

    #[test]
    fn ord_matches_declaration_order() {
        assert!(JsonMode::Unavailable < JsonMode::Object);
        assert!(JsonMode::Object < JsonMode::Schema);
    }

    #[test]
    fn from_str_unknown_returns_error() {
        let err = JsonMode::from_str("json").unwrap_err();
        assert_eq!(err, ParseJsonModeError::new("json".to_string()));
        let msg = format!("{err}");
        assert!(msg.contains("json"));
        assert!(msg.contains("schema"));
    }

    #[test]
    fn copy_and_hash() {
        fn assert_copy<T: Copy>() {}
        fn assert_hash<T: core::hash::Hash>() {}
        assert_copy::<JsonMode>();
        assert_hash::<JsonMode>();
    }
}
