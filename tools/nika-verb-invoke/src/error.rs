// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Error types for the invoke verb.

use nika_kernel::builtin::BuiltinError;
use nika_kernel::mcp::McpError;

/// Errors returned by `nika_verb_invoke::run`.
///
/// The engine converts these to `NikaError` via
/// `impl From<VerbInvokeError> for NikaError`.
///
/// `#[non_exhaustive]` so new failure modes (e.g. `ResultTooLarge` from the
/// S15 McpPool trait expansion) can be added without breaking downstream
/// `From` impls. Invariant #25, applied symmetrically across all verb-crate
/// error enums.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum VerbInvokeError {
    /// Builtin tool dispatch failure.
    #[error(transparent)]
    Builtin(#[from] BuiltinError),

    /// MCP tool or resource failure.
    #[error(transparent)]
    Mcp(#[from] McpError),

    /// Task was cancelled via CancellationToken.
    #[error("invoke: task '{task_id}' cancelled")]
    Cancelled { task_id: String },

    /// Task timed out.
    #[error("invoke: task timed out after {duration_ms}ms")]
    Timeout { duration_ms: u64 },

    /// Parameter serialization error.
    #[error("invoke: {reason}")]
    InvalidParams { reason: String },

    /// Validation error — the input was malformed.
    #[error("invoke: {reason}")]
    Validation { reason: String },
}
