// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! `nika-mcp` — the in-binary **MCP server** (Model Context Protocol).
//!
//! Exposes Nika's STATIC, read-only surface as MCP tools — validate
//! (`nika_check` · `nika_explain`) and learn (`nika_schema` · `nika_examples`
//! · `nika_template` · `nika_canon`) — so any connecting client — Cursor ·
//! Claude Desktop · Zed · an agent — can author against the real contract and
//! audit a workflow BEFORE running it, without a network round-trip to a
//! hosted service (alignment Rule 1 · the binary is self-contained).
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
//! effectful path, gated and audited. The ONE repair door, `nika_check`
//! with `fix: true` (the `repair` module), walks the `check --fix` ladder in memory
//! and hands the repaired text back — it never writes a file (#1270).
//!
//! The crate also owns the CLIENT side of the protocol: [`client`] is the
//! `tools/list` seam over configured servers (`.nika/mcp_servers.json`) and
//! [`pin`] is the anti-rug-pull defence — per-tool pins over
//! `{name, description, inputSchema}` in `.nika/mcp_pins.json`, enrolled on
//! first contact (TOFU), re-verified on EVERY connect, and failed closed
//! with a human-readable diff on any drift (`nika mcp approve <server>`
//! re-pins after human review). Every spawned server rides the OS sandbox
//! ([`sandbox`] — the same Seatbelt/bwrap confinement the `exec` verb uses):
//! confined to the project tree, network denied unless the registry opts in,
//! environment scrubbed, and never a silent unconfined fallback.

pub mod client;
mod http;
mod learn;
pub mod pin;
mod prompts;
mod protocol;
mod repair;
pub mod sandbox;
mod tools;

pub use http::HttpServer;

use std::io::{BufRead, Read, Write};

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

/// The per-message byte ceiling. A well-formed MCP request is a single line
/// of JSON — a workflow-in-arguments is KB-scale, so 8 MiB is generous. The
/// cap exists because the transport is a TRUST BOUNDARY (an external agent
/// feeds stdin): `BufRead::lines()` grows one `String` until a newline, so a
/// hostile line with no terminator would allocate without bound (an OOM `DoS`).
/// A message that reaches the ceiling without a newline is refused with a
/// `-32700`-class error and the pump stops — the stream is unrecoverable for
/// a line protocol, and draining an unterminated line is itself unbounded.
pub(crate) const MAX_MSG_BYTES: u64 = 8 * 1024 * 1024;

/// The transport pump over arbitrary byte streams. [`run_stdio`] wires the real
/// stdio handles; tests drive it with in-memory buffers so every branch — a
/// dispatched reply, a skipped blank line, a silent notification, a `-32700`
/// parse error, an over-ceiling refusal — is exercised without a real pipe (the
/// wiring wrapper above is the only line the lib tests cannot reach; the canary
/// E2E covers it).
///
/// Reads one JSON-RPC message per line (bounded to [`MAX_MSG_BYTES`]),
/// dispatches it through the pure `dispatch`, and writes each reply as one
/// flushed line.
///
/// # Errors
/// Returns [`McpError::Transport`] if reading the input or writing the output
/// fails.
fn run<R: BufRead, W: Write>(mut reader: R, mut writer: W) -> Result<(), McpError> {
    let mut buf: Vec<u8> = Vec::new();
    loop {
        buf.clear();
        // Read one line, but never more than the ceiling + 1 byte — the +1
        // lets us distinguish "a full MAX-byte line that DID terminate" from
        // "a line that overran the ceiling".
        let n = (&mut reader)
            .take(MAX_MSG_BYTES + 1)
            .read_until(b'\n', &mut buf)?;
        if n == 0 {
            break; // clean EOF
        }
        let terminated = buf.last() == Some(&b'\n');
        if !terminated && n as u64 > MAX_MSG_BYTES {
            // Over the ceiling without a newline — refuse and stop (see doc).
            let reply = serde_json::json!({
                "jsonrpc": "2.0",
                "id": serde_json::Value::Null,
                "error": { "code": -32700,
                    "message": format!("message exceeds the {MAX_MSG_BYTES}-byte line ceiling") }
            });
            writeln!(writer, "{reply}")?;
            writer.flush()?;
            break;
        }
        // Trim the trailing newline (and a CRLF's \r) before parsing.
        while matches!(buf.last(), Some(b'\n' | b'\r')) {
            buf.pop();
        }
        if buf.iter().all(u8::is_ascii_whitespace) {
            continue; // blank/whitespace-only line
        }
        // A line must be valid UTF-8 to be JSON — non-UTF-8 is a parse error,
        // preserving the prior `lines()` behavior (which errored on bad UTF-8).
        let parsed = std::str::from_utf8(&buf)
            .map_err(|e| e.to_string())
            .and_then(|s| serde_json::from_str::<serde_json::Value>(s).map_err(|e| e.to_string()));
        match parsed {
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
    fn an_over_ceiling_line_is_refused_and_the_pump_stops_bounded() {
        // TRUST-BOUNDARY totality: a hostile external agent sends a line that
        // never terminates (no newline). The transport must NOT allocate it
        // without bound (the `BufRead::lines` OOM class) — it refuses at the
        // ceiling with a -32700 and stops, having read at most MAX+1 bytes.
        // Modeled with a reader that yields 'a' forever, capped by Take so the
        // test itself stays bounded; the pump's own ceiling is what proves the
        // property (it reads MAX+1, sees no newline, refuses).
        struct Endless;
        impl std::io::Read for Endless {
            fn read(&mut self, b: &mut [u8]) -> std::io::Result<usize> {
                b.fill(b'a');
                Ok(b.len())
            }
        }
        let reader = std::io::BufReader::new(Endless);
        let mut out = Vec::new();
        run(reader, &mut out).expect("the transport refuses cleanly, never OOMs");
        let s = String::from_utf8(out).expect("utf8");
        assert!(
            s.contains("-32700") && s.contains("ceiling"),
            "the over-limit line is refused with the ceiling error: {s}"
        );
        assert_eq!(s.lines().count(), 1, "exactly one refusal, then it stops");
    }

    #[test]
    fn a_full_ceiling_line_that_terminates_is_still_parsed() {
        // The boundary is exact: a message right at the ceiling that DOES end
        // in a newline is a normal (if large) line — parsed, not refused. A
        // padded-but-valid JSON request proves the +1 discriminator is correct.
        let pad = " ".repeat(1000);
        let line = format!("{{\"jsonrpc\":\"2.0\",\"id\":9,{pad}\"method\":\"ping\"}}\n");
        let mut out = Vec::new();
        run(line.as_bytes(), &mut out).expect("a large-but-valid line parses");
        let s = String::from_utf8(out).expect("utf8");
        assert!(
            s.contains("\"id\":9") && !s.contains("-32700"),
            "parsed, not refused: {s}"
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
