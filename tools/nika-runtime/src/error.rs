// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Runtime error types for verb dispatch.

/// Errors returned by [`crate::dispatch::dispatch`].
///
/// Each verb crate defines its own error type; this enum aggregates them
/// at the dispatch boundary. The engine converts via
/// `impl From<RuntimeError> for NikaError`.
#[derive(Debug, thiserror::Error)]
pub enum RuntimeError {
    /// A verb that has not been extracted yet was dispatched.
    #[error("verb '{verb}' dispatch not yet implemented (deferred to S14)")]
    NotImplemented { verb: &'static str },
}
