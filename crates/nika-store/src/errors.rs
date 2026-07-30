// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The store error surface — NIKA-605 (the signed-memory family · the
//! one-voice registry). A rejected ENTRY is not an error: recall names it
//! in the per-entry [`crate::RecallVerdict`]. `StoreError` is for the store
//! itself failing (IO · serialize · layout).

use nika_error::codes::{self, NikaCode};
use nika_error::traits::NikaErrorCode;

/// A store-level failure (F-P8 · the `.nika/memory/<store>/` envelope).
#[derive(Debug, thiserror::Error, miette::Diagnostic)]
#[non_exhaustive]
pub enum StoreError {
    /// Filesystem IO failed (read · write · rename · `create_dir`).
    #[error("memory store io: {reason}")]
    #[diagnostic(help("{}", codes::code_help(codes::NIKA_605)))]
    Io {
        /// What the OS reported.
        reason: String,
    },
    /// An entry could not serialize to its envelope JSON (or the signer
    /// refused the preimage).
    #[error("memory entry serialize: {reason}")]
    #[diagnostic(help("{}", codes::code_help(codes::NIKA_605)))]
    Serialize {
        /// What the codec / signer reported.
        reason: String,
    },
    /// The store dir layout is unusable (a non-dir rides the store path).
    #[error("memory store layout: {reason}")]
    #[diagnostic(help("{}", codes::code_help(codes::NIKA_605)))]
    DirLayout {
        /// What violates the layout.
        reason: String,
    },
    /// A commit's canonical file name already holds DIFFERENT bytes — an
    /// out-of-engine plant at that name is tamper evidence: the write is
    /// refused, never a silent clobber (identical bytes are the idempotent
    /// rewrite — a no-op, not an error).
    #[error("memory entry conflict: {reason}")]
    #[diagnostic(help("{}", codes::code_help(codes::NIKA_605)))]
    Conflict {
        /// Which name carries divergent bytes.
        reason: String,
    },
}

impl NikaErrorCode for StoreError {
    fn nika_code(&self) -> NikaCode {
        // ONE row for the store family (the variant carries the detail) —
        // the registry's sub-allocation reserves 605 for this crate.
        codes::NIKA_605
    }

    /// IO is transient (retry-eligible) · serialize/layout are deterministic.
    fn is_transient(&self) -> bool {
        matches!(self, Self::Io { .. })
    }
}
