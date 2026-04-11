// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Security Module — Command validation and blocklist
//!
//! Provides security validation for exec: commands:
//! - Control character detection (blocks null bytes, escape sequences)
//! - Blocklist for dangerous command patterns
//! - Unicode NFKC normalization to prevent confusable bypass
//! - Shell injection detection (data vs template)
//! - Environment variable validation (library injection prevention)
//! - Artifact path validation (directory traversal prevention)
//!
//! ## Unicode Confusable Protection
//!
//! Attackers may attempt to bypass the blocklist using Unicode confusables:
//! - Fullwidth characters: `rm` vs `ｒｍ` (U+FF52, U+FF4D)
//! - Math bold/italic: `sudo` vs `𝘀𝘂𝗱𝗼` (U+1D600 range)
//! - Combining characters: `rm` with zero-width joiners
//!
//! NFKC (Compatibility Decomposition + Canonical Composition) normalizes
//! these variants to their ASCII equivalents before blocklist checking.

/// Security validation error, independent of `NikaError` so this crate stays L1.
///
/// The `Display` format matches `NikaError::BlockedCommand` so existing
/// callers asserting `err.to_string().contains("NIKA-053")` continue to
/// work unchanged. The engine bridge translates via a `From` impl in
/// `nika-engine/src/error.rs`.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum SecurityError {
    #[error("[NIKA-053] Command blocked: '{command}' - {reason}")]
    BlockedCommand { command: String, reason: String },

    #[error("[NIKA-280] Artifact path error for '{path}': {reason}")]
    ArtifactPath { path: String, reason: String },
}

// ── Modules ──────────────────────────────────────────────────────────────────

mod blocklist;
mod command;
mod env;
mod injection;
pub(crate) mod normalize;
pub mod path;

// ── Public API re-exports ────────────────────────────────────────────────────

pub use blocklist::{check_blocklist, check_blocklist_with_intent};
pub use command::{validate_command_string, validate_exec_command_full, validate_exec_command_with_shell};
pub use env::{sensitive_env_vars, validate_env_vars};
pub use injection::{check_shell_data_injection, check_shell_mode_blocklist};

// ── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests;
