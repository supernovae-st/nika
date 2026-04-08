// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Error types for nika-init crate.

/// Errors from the init/course/showcase subsystem.
///
/// Uses NIKA-500+ range to avoid collisions with nika-engine error codes.
#[derive(Debug, thiserror::Error)]
pub enum NikaInitError {
    /// I/O error during file operations
    #[error("[NIKA-501] I/O error: {0}")]
    IoError(#[from] std::io::Error),

    /// Configuration error — TOML parse/serialize failures
    #[error("[NIKA-502] Config error: {reason}")]
    ConfigError { reason: String },

    /// Validation error — e.g. "course already exists"
    #[error("[NIKA-503] Validation error: {reason}")]
    ValidationError { reason: String },
}

/// Convenience Result alias.
pub type Result<T> = std::result::Result<T, NikaInitError>;
