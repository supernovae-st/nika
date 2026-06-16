// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! `nika-mcp` — the in-binary **MCP server** (Model Context Protocol).
//!
//! Exposes Nika's STATIC, read-only surface (`nika_check` · `nika_explain`) as
//! MCP tools so any connecting client — Cursor · Claude Desktop · Zed · an
//! agent — can audit a workflow BEFORE running it, without a network round-trip
//! to a hosted service (alignment Rule 1 · the binary is self-contained).
//!
//! **Sovereign, zero-SDK** · the transport is hand-rolled newline-delimited
//! JSON-RPC 2.0 over stdio (the MCP stdio framing · one compact JSON message
//! per line, no embedded newlines) — `serde_json` is the only wire dependency,
//! the same « talk the protocol directly » discipline Diamond uses for provider
//! wire formats. The protocol dispatch ([`handle`]) is a PURE function; this
//! module is only the I/O pump.
//!
//! Running a workflow is NOT exposed (it needs the effect-permits boundary) —
//! the server surface is read-only by construction. `nika run` stays the
//! effectful path, gated and audited.

mod protocol;
mod tools;

pub use protocol::{dispatch, handle};

use std::io::{BufRead as _, Write as _};

/// MCP transport failures — the stdio pipe broke (a disconnected client, a
/// closed pipe). Protocol-level errors travel IN-BAND as JSON-RPC error
/// replies; only a dead transport surfaces here.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum McpError {
    /// The stdio transport (stdin/stdout pipe) failed.
    #[error("MCP stdio transport failed: {0}")]
    Transport(#[from] std::io::Error),
}

/// Serve MCP over stdio until EOF (the client disconnects · a clean shutdown).
///
/// Reads one JSON-RPC message per line, dispatches it through the pure
/// [`handle`], and writes each reply as one line (flushed · a client blocks on
/// it). A malformed line is answered with a `-32700` parse error rather than
/// killing the session. A notification (no `id`) yields no reply.
///
/// # Errors
/// Returns [`McpError::Transport`] if reading stdin or writing stdout fails.
pub fn run_stdio() -> Result<(), McpError> {
    let stdin = std::io::stdin();
    let mut stdout = std::io::stdout();
    for line in stdin.lock().lines() {
        let line = line?;
        if line.trim().is_empty() {
            continue;
        }
        match serde_json::from_str::<serde_json::Value>(&line) {
            Ok(msg) => {
                if let Some(reply) = protocol::dispatch(&msg) {
                    writeln!(stdout, "{reply}")?;
                    stdout.flush()?;
                }
            }
            Err(e) => {
                let reply = serde_json::json!({
                    "jsonrpc": "2.0",
                    "id": serde_json::Value::Null,
                    "error": { "code": -32700, "message": format!("parse error: {e}") }
                });
                writeln!(stdout, "{reply}")?;
                stdout.flush()?;
            }
        }
    }
    Ok(())
}
