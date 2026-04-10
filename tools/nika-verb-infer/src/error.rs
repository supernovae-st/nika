// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Errors returned by `nika_verb_infer::run()`.
//!
//! The engine bridge converts these to `NikaError` via its own
//! `From<VerbInferError>` impl — the verb crate never depends on
//! `nika-engine::error::NikaError`.

use nika_kernel::provider::ProviderError;

/// Errors returned by [`crate::run`].
#[derive(Debug, thiserror::Error)]
pub enum VerbInferError {
    /// Input validation failed before dispatch (empty prompt, invalid tokens).
    #[error("infer: invalid input — {reason}")]
    Validation { reason: String },

    /// Provider-level error (API, auth, rate limit, etc.).
    #[error("infer: provider error — {0}")]
    Provider(#[from] ProviderError),

    /// Task was cancelled via the cancellation token before the provider
    /// call completed.
    #[error("infer: task '{task_id}' cancelled")]
    Cancelled { task_id: String },

    /// Provider returned no text content in the response (tool-only call
    /// that the verb crate cannot yet handle on its own — future extension).
    #[error("infer: provider returned no text content")]
    NoTextContent,
}
