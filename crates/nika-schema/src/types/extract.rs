// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Extract and response mode types for fetch tasks.

use std::fmt;

use serde::{Deserialize, Serialize};

/// How to extract content from a fetch response.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[non_exhaustive]
#[serde(rename_all = "snake_case")]
pub enum ExtractMode {
    /// Return the full response body as-is.
    #[default]
    Full,
    /// Extract text content (strip HTML tags).
    Text,
    /// Parse as JSON.
    Json,
    /// Use a CSS selector to extract elements.
    Selector,
    /// Use a `JMESPath` expression.
    Jmespath,
}

impl fmt::Display for ExtractMode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Full => write!(f, "full"),
            Self::Text => write!(f, "text"),
            Self::Json => write!(f, "json"),
            Self::Selector => write!(f, "selector"),
            Self::Jmespath => write!(f, "jmespath"),
        }
    }
}

/// How the fetch response body should be interpreted.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[non_exhaustive]
#[serde(rename_all = "snake_case")]
pub enum ResponseMode {
    /// Interpret as text (UTF-8).
    #[default]
    Text,
    /// Interpret as JSON.
    Json,
    /// Interpret as binary (base64-encoded in output).
    Binary,
}

impl fmt::Display for ResponseMode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Text => write!(f, "text"),
            Self::Json => write!(f, "json"),
            Self::Binary => write!(f, "binary"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extract_mode_display() {
        assert_eq!(ExtractMode::Full.to_string(), "full");
        assert_eq!(ExtractMode::Selector.to_string(), "selector");
        assert_eq!(ExtractMode::Jmespath.to_string(), "jmespath");
    }

    #[test]
    fn extract_mode_serde_roundtrip() {
        for mode in [
            ExtractMode::Full,
            ExtractMode::Text,
            ExtractMode::Json,
            ExtractMode::Selector,
            ExtractMode::Jmespath,
        ] {
            let json = serde_json::to_string(&mode).expect("serialize");
            let back: ExtractMode = serde_json::from_str(&json).expect("deserialize");
            assert_eq!(back, mode);
        }
    }

    #[test]
    fn response_mode_display() {
        assert_eq!(ResponseMode::Text.to_string(), "text");
        assert_eq!(ResponseMode::Binary.to_string(), "binary");
    }

    #[test]
    fn response_mode_serde_roundtrip() {
        for mode in [ResponseMode::Text, ResponseMode::Json, ResponseMode::Binary] {
            let json = serde_json::to_string(&mode).expect("serialize");
            let back: ResponseMode = serde_json::from_str(&json).expect("deserialize");
            assert_eq!(back, mode);
        }
    }
}
