// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Internal errors for the local inference sidecar.
//!
//! These are the sidecar's *own* failure modes. The engine-facing surface is
//! the OpenAI-compat HTTP error shape (mapped at the server boundary, a later
//! commit); the client side already reports through the kernel `ProviderError`
//! (`nika-providers`), so this crate owns no shared `NIKA-*` registry codes
//! (`err_prefix = "none"`, same posture as `nika-providers`). The canonical
//! `NIKA-460..479` band is reserved for the engine-facing mapping (ADR-091).

/// What can go wrong inside the local inference sidecar.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum InferLocalError {
    /// The requested model is not available in the local store.
    #[error("model not found: {model}")]
    ModelNotFound {
        /// The model id the request asked for.
        model: String,
    },

    /// The prompt (after templating) exceeds the model's context window.
    #[error("context overflow: {tokens} tokens > {limit} limit")]
    ContextOverflow {
        /// Token count of the rendered prompt.
        tokens: usize,
        /// The model's context window.
        limit: usize,
    },

    /// Generation exceeded its wall-clock budget.
    ///
    /// Constructed by the SERVER layer (which owns wall-clock budgets and
    /// can drop the generation future) — the synchronous backend loop has
    /// no preemption point, so it never returns this itself.
    #[error("generation timed out after {ms}ms")]
    Timeout {
        /// The elapsed budget in milliseconds.
        ms: u64,
    },

    /// The backend ran out of memory loading or running the model.
    #[error("out of memory: {reason}")]
    OutOfMemory {
        /// What allocation failed.
        reason: String,
    },

    /// A malformed request (bad role, empty messages, unknown field).
    #[error("invalid request: {reason}")]
    InvalidRequest {
        /// Why the request was rejected.
        reason: String,
    },

    /// Any other backend failure (model load, tokenizer, forward pass).
    #[error("backend error: {reason}")]
    Backend {
        /// Error description.
        reason: String,
    },
}
