// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! `NullContextCompressor` — no-op context compressor.

use nika_error::checkpoint::CheckpointMessage;
use nika_error::compression::CompressionPolicy;
use nika_kernel::context::{CompressedContext, ContextCompressor};

/// No-op context compressor that never compresses.
///
/// Used when tests don't exercise compression but need a value.
#[derive(Clone, Debug, Default)]
#[non_exhaustive]
pub struct NullContextCompressor;

impl NullContextCompressor {
    /// Create a new null context compressor.
    #[must_use]
    pub fn new() -> Self {
        Self
    }
}

impl ContextCompressor for NullContextCompressor {
    async fn compress(
        &self,
        _messages: &[CheckpointMessage],
        _policy: &CompressionPolicy,
    ) -> Option<CompressedContext> {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn null_returns_none() {
        let compressor = NullContextCompressor::new();
        let result = compressor.compress(&[], &CompressionPolicy::new()).await;
        assert!(result.is_none());
    }

    fn _assert_send_sync<T: Send + Sync>() {}

    #[test]
    fn null_compressor_is_send_sync() {
        _assert_send_sync::<NullContextCompressor>();
    }
}
