// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Context configuration — the `context:` section at workflow root.

use serde::{Deserialize, Serialize};

/// Context configuration for workflow-level shared state.
///
/// Controls how context is managed across tasks: max tokens, compression
/// strategy, and whether to include system-level context.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[non_exhaustive]
pub struct ContextConfig {
    /// Maximum number of tokens to include in context window.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_tokens: Option<u32>,
    /// Whether to enable automatic context compression.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub compress: Option<bool>,
    /// Number of recent messages to always preserve uncompressed.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub preserve_recent: Option<u32>,
}

impl ContextConfig {
    /// Create a new empty context configuration.
    #[must_use]
    pub fn new() -> Self {
        Self {
            max_tokens: None,
            compress: None,
            preserve_recent: None,
        }
    }
}

impl Default for ContextConfig {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_is_empty() {
        let c = ContextConfig::new();
        assert!(c.max_tokens.is_none());
        assert!(c.compress.is_none());
        assert!(c.preserve_recent.is_none());
    }

    #[test]
    fn serde_roundtrip() {
        let c = ContextConfig {
            max_tokens: Some(4096),
            compress: Some(true),
            preserve_recent: Some(3),
        };
        let json = serde_json::to_string(&c).expect("serialize");
        let back: ContextConfig = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(back.max_tokens, Some(4096));
        assert_eq!(back.compress, Some(true));
        assert_eq!(back.preserve_recent, Some(3));
    }

    #[test]
    fn serde_empty_object() {
        let c: ContextConfig = serde_json::from_str("{}").expect("deserialize");
        assert!(c.max_tokens.is_none());
    }
}
