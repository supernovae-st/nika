// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Runtime error types for verb dispatch.

/// Errors returned by [`crate::dispatch::dispatch`].
///
/// Each verb crate defines its own error type; this enum aggregates them
/// at the dispatch boundary. The engine converts via
/// `impl From<RuntimeError> for NikaError`.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum RuntimeError {
    /// Exec verb error.
    #[error(transparent)]
    Exec(#[from] nika_verb_exec::VerbExecError),

    /// Invoke verb error.
    #[error(transparent)]
    Invoke(#[from] nika_verb_invoke::VerbInvokeError),

    /// Fetch verb error.
    #[error(transparent)]
    Fetch(#[from] nika_verb_fetch::VerbFetchError),

    /// Infer verb error.
    #[error(transparent)]
    Infer(#[from] nika_verb_infer::VerbInferError),

    /// A verb that has not been extracted yet was dispatched.
    #[error("verb '{verb}' dispatch not yet implemented")]
    NotImplemented { verb: &'static str },
}
