// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Transport / protocol failures for the language server.
//!
//! These are NOT a `NIKA-XXXX` author-facing range — a workflow author
//! never sees them. They are the failures of the JSON-RPC transport
//! itself (a broken pipe, a malformed initialize handshake, a serde
//! mismatch on a request payload) which the [`run_stdio`](crate::run_stdio)
//! loop returns to its caller (the `nika lsp` subcommand) to surface as a
//! process exit. The author-facing diagnostics flow through
//! [`analysis::diagnostics`](crate::analysis::diagnostics), carrying the
//! existing `NIKA-*` spec codes verbatim.

use thiserror::Error;

/// A language-server transport or protocol failure.
///
/// `#[non_exhaustive]` — new transport failure classes may be added
/// without a breaking change (the forward-compat ratchet on every public
/// enum).
#[derive(Debug, Error)]
#[non_exhaustive]
pub enum LspError {
    /// The JSON-RPC transport (stdin/stdout pipe) failed — a disconnected
    /// client, a broken pipe, an I/O error on the message stream.
    #[error("language-server transport failed")]
    Transport(#[source] std::io::Error),

    /// The LSP protocol handshake or message exchange was violated — the
    /// `initialize` request never arrived, an unexpected message landed
    /// during shutdown, or the channel disconnected mid-protocol.
    #[error("language-server protocol violation: {0}")]
    Protocol(String),

    /// A request or notification payload could not be (de)serialized
    /// against its declared LSP type.
    #[error("language-server payload (de)serialization failed")]
    Serde(#[source] serde_json::Error),
}

impl From<serde_json::Error> for LspError {
    fn from(err: serde_json::Error) -> Self {
        Self::Serde(err)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn transport_displays_without_leaking_inner() {
        let err = LspError::Transport(std::io::Error::other("pipe gone"));
        assert_eq!(err.to_string(), "language-server transport failed");
        // the inner io::Error is reachable as the source
        assert!(std::error::Error::source(&err).is_some());
    }

    #[test]
    fn protocol_carries_the_detail() {
        let err = LspError::Protocol("no initialize request".to_owned());
        assert_eq!(
            err.to_string(),
            "language-server protocol violation: no initialize request"
        );
    }

    #[test]
    fn serde_error_converts_via_from() {
        let bad = serde_json::from_str::<lsp_types::Position>("not json");
        let err: LspError = bad.expect_err("invalid json").into();
        assert!(matches!(err, LspError::Serde(_)));
        assert!(std::error::Error::source(&err).is_some());
    }
}
