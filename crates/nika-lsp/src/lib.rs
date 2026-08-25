// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The Nika language server (`nika lsp`, stdio).
//!
//! `nika-lsp` is the editor brain for `.nika.yaml`. It turns the engine's
//! static guarantees into live, in-editor feedback over any LSP client
//! (VS Code, Cursor, Zed, Neovim, Helix) ·
//!
//! - **diagnostics** — the ADR-092 `nika_check::check` ladder, surfaced as
//!   red squiggles carrying the existing `NIKA-*` codes verbatim (one
//!   source of truth with `nika check`);
//! - **hover** — docs for the 4 verbs (`infer · exec · invoke · agent`) and
//!   the language keywords;
//! - **completion** — top-level keys, task fields, the verbs, `model:`
//!   providers, and the workflow's own task ids;
//! - **document symbols** — the task outline;
//! - **go-to-definition** — a `depends_on:` / `${{ tasks.X }}` reference
//!   jumps to the task that defines it.
//!
//! # Architecture
//!
//! [`analysis`] is the pure brain — every function is `(text[, offset]) ->
//! value`, no I/O, no server state, unit- and property-testable without a
//! client. [`run_stdio`] is the thin sync `lsp-server` transport shell that
//! drives it. v0.1 is single-file, full-reparse-on-change, UTF-16
//! positions.
//!
//! # Scope
//!
//! v0.1 (LOCKED): diagnostics, hover, completion, document symbols,
//! definition. v0.2 adds **code actions** (quickfix-only — the
//! `check --fix` typed-rename engine projected; one fix engine, every
//! editor). Still out: inlay hints, semantic tokens, model catalog
//! intelligence, `${{ }}` expression intelligence, multi-file /
//! includes / incremental reparse.

// Tests may use `unwrap`/`expect`/`panic`-class assertions freely (the
// production `src/` stays zero-unwrap). Mirrors the nika-schema policy.
#![cfg_attr(
    test,
    allow(
        clippy::unwrap_used,
        clippy::expect_used,
        clippy::panic,
        clippy::missing_panics_doc,
    )
)]

pub mod analysis;
pub mod capabilities;
pub mod error;
mod server;
mod watchdog;

/// Run the language server over stdio until the client disconnects.
///
/// This is what the `nika lsp` subcommand calls: it connects over
/// stdin/stdout, performs the LSP initialize handshake (advertising
/// [`capabilities::server_capabilities`]), serves requests and
/// notifications, and returns when the client sends `shutdown`/`exit` or
/// the transport closes.
///
/// # Errors
///
/// Returns an [`error::LspError`] when the transport fails, the protocol
/// handshake is violated, or a payload cannot be (de)serialized.
pub fn run_stdio() -> Result<(), error::LspError> {
    server::run_stdio(None)
}

/// Run the language server over stdio, watching the host process.
///
/// Same lifecycle as [`run_stdio`], plus the LSP `initialize` §`processId`
/// contract: *"If the parent process is not alive then the server should
/// exit its process."* `client_process_id` is the host's
/// `--clientProcessId` argv value; when it is absent the `processId` of
/// the `initialize` params is used instead. When neither names a live
/// parent, nothing is watched and this behaves exactly like [`run_stdio`].
///
/// This matters only for a host that dies **without** closing stdio. A
/// client that closes the pipe already ends the session by EOF; a client
/// that crashes does not, and before this the server ran forever (#1181).
///
/// # Errors
///
/// Returns an [`error::LspError`] on the same conditions as [`run_stdio`].
pub fn run_stdio_watching(client_process_id: Option<u32>) -> Result<(), error::LspError> {
    server::run_stdio(client_process_id)
}
