// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Error types for the exec verb.

/// Errors returned by [`crate::run`].
///
/// The engine converts these to `NikaError` via `impl From<VerbExecError> for NikaError`.
///
/// `#[non_exhaustive]` so new failure modes can be added without breaking
/// downstream `From` impls (invariant #25, applied symmetrically across all
/// verb-crate error enums).
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum VerbExecError {
    /// Command exited with non-zero status.
    #[error("exec: command failed (exit {exit_code}): {stderr}")]
    NonZeroExit { exit_code: i32, stderr: String },

    /// Command was cancelled via CancellationToken.
    #[error("exec: task '{task_id}' cancelled during execution")]
    Cancelled { task_id: String },

    /// Command timed out.
    #[error("exec: command timed out after {duration_ms}ms")]
    Timeout { duration_ms: u64 },

    /// Program not found.
    #[error("exec: program '{program}' not found")]
    NotFound { program: String },

    /// Command parsing error (shlex).
    #[error("exec: {reason}")]
    Parse { reason: String },

    /// Shell executor error.
    #[error("exec: {reason}")]
    Shell { reason: String },
}
