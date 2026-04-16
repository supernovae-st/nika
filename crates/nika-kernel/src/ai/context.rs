// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Context compressor trait — agent-v2 kernel hook.
//!
//! Enables context window management for long agent conversations.
//! Implementations compress older messages while preserving recent turns.

use nika_error::checkpoint::CheckpointMessage;
use nika_error::compression::CompressionPolicy;

/// Result of context compression.
#[derive(Debug, Clone)]
#[non_exhaustive]
pub struct CompressedContext {
    /// Summary of the compressed messages.
    pub summary: String,
    /// Number of messages that were compressed.
    pub messages_compressed: u32,
    /// Token count before compression.
    pub tokens_before: u64,
    /// Token count after compression.
    pub tokens_after: u64,
}

impl CompressedContext {
    /// Create a new compressed context result.
    #[must_use]
    pub fn new(
        summary: impl Into<String>,
        messages_compressed: u32,
        tokens_before: u64,
        tokens_after: u64,
    ) -> Self {
        Self {
            summary: summary.into(),
            messages_compressed,
            tokens_before,
            tokens_after,
        }
    }
}

/// Context compressor for agent conversations.
///
/// Production: LLM-based summarization.
/// Tests: `NullContextCompressor` (always returns `None`).
#[trait_variant::make(ContextCompressorDyn: Send)]
pub trait ContextCompressor: Send + Sync {
    /// Compress messages according to the given policy.
    ///
    /// Returns `None` if compression is not needed or not possible.
    async fn compress(
        &self,
        messages: &[CheckpointMessage],
        policy: &CompressionPolicy,
    ) -> Option<CompressedContext>;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compressed_context_new() {
        let ctx = CompressedContext::new("summary of conversation", 10, 50000, 5000);
        assert_eq!(ctx.summary, "summary of conversation");
        assert_eq!(ctx.messages_compressed, 10);
        assert_eq!(ctx.tokens_before, 50000);
        assert_eq!(ctx.tokens_after, 5000);
    }

    fn _assert_send_sync<T: Send + Sync>() {}

    #[test]
    fn compressed_context_is_send_sync() {
        _assert_send_sync::<CompressedContext>();
    }
}
