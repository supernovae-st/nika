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
//! wire formats. The protocol dispatch (`handle`) is a PURE function; this
//! module is only the I/O pump.
//!
//! Running a workflow is NOT exposed (it needs the effect-permits boundary) —
//! the server surface is read-only by construction. `nika run` stays the
//! effectful path, gated and audited.

mod protocol;
mod tools;

use std::io::{BufRead, Write};

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
/// `handle`, and writes each reply as one line (flushed · a client blocks on
/// it). A malformed line is answered with a `-32700` parse error rather than
/// killing the session. A notification (no `id`) yields no reply.
///
/// # Errors
/// Returns [`McpError::Transport`] if reading stdin or writing stdout fails.
pub fn run_stdio() -> Result<(), McpError> {
    let stdin = std::io::stdin();
    let stdout = std::io::stdout();
    run(stdin.lock(), stdout.lock())
}

/// The transport pump over arbitrary byte streams. [`run_stdio`] wires the real
/// stdio handles; tests drive it with in-memory buffers so every branch — a
/// dispatched reply, a skipped blank line, a silent notification, a `-32700`
/// parse error — is exercised without a real pipe (the wiring wrapper above is
/// the only line the lib tests cannot reach; the canary E2E covers it).
///
/// Reads one JSON-RPC message per line, dispatches it through the pure
/// `dispatch`, and writes each reply as one flushed line.
///
/// # Errors
/// Returns [`McpError::Transport`] if reading the input or writing the output
/// fails.
fn run<R: BufRead, W: Write>(reader: R, mut writer: W) -> Result<(), McpError> {
    for line in reader.lines() {
        let line = line?;
        if line.trim().is_empty() {
            continue;
        }
        match serde_json::from_str::<serde_json::Value>(&line) {
            Ok(msg) => {
                if let Some(reply) = protocol::dispatch(&msg) {
                    writeln!(writer, "{reply}")?;
                    writer.flush()?;
                }
            }
            Err(e) => {
                let reply = serde_json::json!({
                    "jsonrpc": "2.0",
                    "id": serde_json::Value::Null,
                    "error": { "code": -32700, "message": format!("parse error: {e}") }
                });
                writeln!(writer, "{reply}")?;
                writer.flush()?;
            }
        }
    }
    Ok(())
}

#[cfg(test)]
#[allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)]
mod tests {
    use super::run;

    #[test]
    fn the_pump_replies_skips_blank_lines_and_answers_parse_errors() {
        // One request (→ a reply) · a whitespace-only line (→ skipped) · a
        // notification with no id (→ silent) · a malformed line (→ -32700). The
        // pump must emit EXACTLY the two replies, in order, over a clean stream.
        let input = "{\"jsonrpc\":\"2.0\",\"id\":1,\"method\":\"initialize\",\"params\":{}}\n   \n{\"jsonrpc\":\"2.0\",\"method\":\"notifications/initialized\"}\nnot-json\n";
        let mut out = Vec::new();
        run(input.as_bytes(), &mut out)
            .expect("a clean in-memory stream never breaks the transport");
        let replies: Vec<String> = String::from_utf8(out)
            .unwrap()
            .lines()
            .map(str::to_owned)
            .collect();
        assert_eq!(
            replies.len(),
            2,
            "blank skipped + notification silent: {replies:?}"
        );
        assert!(
            replies[0].contains("\"id\":1"),
            "first is the initialize reply: {}",
            replies[0]
        );
        assert!(
            replies[1].contains("-32700"),
            "second is the parse error: {}",
            replies[1]
        );
    }

    #[test]
    fn the_pump_flushes_after_every_reply() {
        // The flush barrier is load-bearing: a stdio MCP client blocks on it
        // before reading the next message (drop it and a real client deadlocks).
        // A `Vec<u8>` writer's flush is a no-op and cannot witness this, so drive
        // the pump with a counting writer and assert flush FIRED — once per
        // written reply.
        use std::io::{self, Write};
        #[derive(Default)]
        struct Counting {
            flushes: usize,
        }
        impl Write for Counting {
            fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
                Ok(buf.len()) // discard the bytes — this test only watches flush
            }
            fn flush(&mut self) -> io::Result<()> {
                self.flushes += 1;
                Ok(())
            }
        }
        // Two replies: a request (ping) and a malformed line (-32700).
        let input = "{\"jsonrpc\":\"2.0\",\"id\":1,\"method\":\"ping\"}\nnot-json\n";
        let mut w = Counting::default();
        run(input.as_bytes(), &mut w).expect("a clean stream never breaks the transport");
        assert_eq!(
            w.flushes, 2,
            "one flush per written reply (ping + parse-error)"
        );
    }
}
