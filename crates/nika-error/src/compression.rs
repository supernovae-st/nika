// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Context compression policy — shared between agent and context compressor.

/// Context compression policy.
#[derive(Debug, Clone)]
#[non_exhaustive]
pub struct CompressionPolicy {
    /// Token count threshold to trigger compression.
    pub token_threshold: u64,
    /// Number of recent messages to preserve uncompressed.
    pub preserve_recent: u32,
    /// Whether to compress tool results.
    pub compress_tool_results: bool,
}

impl CompressionPolicy {
    /// Create a new compression policy with defaults.
    #[must_use]
    pub fn new() -> Self {
        Self {
            token_threshold: 80_000,
            preserve_recent: 4,
            compress_tool_results: true,
        }
    }
}

impl Default for CompressionPolicy {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compression_policy_defaults() {
        let policy = CompressionPolicy::new();
        assert_eq!(policy.token_threshold, 80_000);
        assert_eq!(policy.preserve_recent, 4);
        assert!(policy.compress_tool_results);
    }

    #[test]
    fn compression_policy_default_trait() {
        let a = CompressionPolicy::new();
        let b = CompressionPolicy::default();
        assert_eq!(a.token_threshold, b.token_threshold);
        assert_eq!(a.preserve_recent, b.preserve_recent);
        assert_eq!(a.compress_tool_results, b.compress_tool_results);
    }

    fn _assert_send_sync<T: Send + Sync>() {}

    #[test]
    fn compression_policy_is_send_sync() {
        _assert_send_sync::<CompressionPolicy>();
    }
}
