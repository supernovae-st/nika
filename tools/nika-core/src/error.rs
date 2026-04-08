// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Minimal error types for nika-core.
//!
//! Contains only the error variants needed by core binding modules.
//! The full NikaError enum lives in the `nika` crate and implements
//! `From<CoreError>` for seamless interop.

use thiserror::Error;

/// Core error type for nika-core modules.
///
/// Kept intentionally minimal -- only variants required by pure-type
/// modules (binding/entry.rs) that were extracted from the main crate.
#[derive(Debug, Error)]
pub enum CoreError {
    /// NIKA-050: Invalid binding path syntax.
    #[error("[NIKA-050] Invalid path syntax: {path}")]
    InvalidPath { path: String },

    /// NIKA-056: Invalid default value in a binding expression.
    #[error("[NIKA-056] Invalid default value '{raw}': {reason}")]
    InvalidDefault { raw: String, reason: String },

    /// Validation error (generic).
    ///
    /// Used by AST modules (limits, include) for semantic validation failures.
    #[error("{reason}")]
    ValidationError { reason: String },
}
