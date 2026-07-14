// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Exec decode mode — the `exec.decode:` field.
//!
//! Per spec `09-types.md` §decode · « `decode:` applies to the captured
//! **raw byte stream** … the pipeline is `raw bytes → decode → value`,
//! never `bytes → lossy string → decode` » · `text` (default · strict
//! UTF-8) · `json` · `jsonl` · `bytes`. Illegal with `capture:
//! structured` (`NIKA-PARSE-025` — that capture already IS an object).

use std::fmt;

use serde::{Deserialize, Serialize};

/// How an `exec:` task's captured bytes become a value (spec 09 §decode).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[non_exhaustive]
#[serde(rename_all = "lowercase")]
pub enum DecodeMode {
    /// Strict UTF-8 text (the default) — invalid UTF-8 settles the task
    /// `failure`, honestly; trailing newline trimmed as today.
    #[default]
    Text,
    /// Parse one JSON document from the bytes.
    Json,
    /// Newline-delimited JSON — one value per non-empty line, into an array.
    Jsonl,
    /// No decoding — the value is the opaque octets (base64 at any JSON
    /// boundary).
    Bytes,
}

impl DecodeMode {
    /// Parse the YAML `decode:` scalar (closed enum · `decode:
    /// artifact-ref` is reserved for the artifact lanes · W5).
    #[must_use]
    pub fn from_str_opt(s: &str) -> Option<Self> {
        match s {
            "text" => Some(Self::Text),
            "json" => Some(Self::Json),
            "jsonl" => Some(Self::Jsonl),
            "bytes" => Some(Self::Bytes),
            _ => None,
        }
    }
}

impl fmt::Display for DecodeMode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Text => write!(f, "text"),
            Self::Json => write!(f, "json"),
            Self::Jsonl => write!(f, "jsonl"),
            Self::Bytes => write!(f, "bytes"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_is_text() {
        assert_eq!(DecodeMode::default(), DecodeMode::Text);
    }

    #[test]
    fn closed_enum_parse() {
        assert_eq!(DecodeMode::from_str_opt("text"), Some(DecodeMode::Text));
        assert_eq!(DecodeMode::from_str_opt("json"), Some(DecodeMode::Json));
        assert_eq!(DecodeMode::from_str_opt("jsonl"), Some(DecodeMode::Jsonl));
        assert_eq!(DecodeMode::from_str_opt("bytes"), Some(DecodeMode::Bytes));
        assert_eq!(
            DecodeMode::from_str_opt("artifact-ref"),
            None,
            "reserved · lands with the artifact lanes (W5)"
        );
        assert_eq!(DecodeMode::from_str_opt("utf8"), None);
    }

    #[test]
    fn display_round_trips() {
        for m in [
            DecodeMode::Text,
            DecodeMode::Json,
            DecodeMode::Jsonl,
            DecodeMode::Bytes,
        ] {
            assert_eq!(DecodeMode::from_str_opt(&m.to_string()), Some(m));
        }
    }
}
