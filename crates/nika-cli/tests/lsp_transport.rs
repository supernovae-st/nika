// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>
#![allow(clippy::expect_used, clippy::panic)]
// Same carve-out as bin_smoke.rs: this file's WHOLE JOB is to drive the
// real binary over stdio (CARGO_BIN_EXE) — the kernel seam would test
// the lib, not the transport.
#![allow(clippy::disallowed_types)]

//! LSP transport survival — the REAL `nika-cli lsp` process over stdio,
//! descended from `bin_smoke.rs` at the 1500-line file cap (the descent
//! law: extraction, never exemption). Same zero-dep `CARGO_BIN_EXE`
//! mechanism; these are transport-robustness pins, not verb-contract
//! smoke.

use std::io::Write as _;
use std::process::Command;

fn bin() -> Command {
    Command::new(env!("CARGO_BIN_EXE_nika-cli"))
}

/// A minimal valid workflow (mirrors `bin_smoke`'s fixture — duplicated
/// by design: integration files share no code, and the 5 lines beat a
/// support-crate ceremony).
const VALID: &str = r#"
nika: smoke
permits: { exec: ["echo"] }
tasks:
  greet:
    exec: { command: ["echo", "hello"] }
"#;

/// One LSP stdio frame (Content-Length header + JSON body).
fn lsp_frame(body: &serde_json::Value) -> Vec<u8> {
    let b = body.to_string();
    format!("Content-Length: {}\r\n\r\n{b}", b.len()).into_bytes()
}

/// The compact-block-sequence bomb (`- ` ×4000) that once aborted the whole
/// process (#282 · CRITICAL, marked-yaml recurses one stack frame per level)
/// reaches the server as a DOCUMENT via didOpen — the exact path an editor
/// takes when a user opens such a file. It must produce ONE graceful
/// stack-safety diagnostic, and — the survival proof — the server must keep
/// serving: a SECOND document opened AFTER the bomb still publishes, and a
/// request after it still gets a reply. A regression (a real stack overflow)
/// aborts the process → the exit code is non-zero and none of the post-bomb
/// work appears. This is the LSP-level guard the unit fix (#282) did not have.
#[test]
fn lsp_survives_a_compact_block_bomb_and_keeps_serving() {
    use std::process::Stdio;
    let bomb = format!("{}x", "- ".repeat(4000));
    let mut child = bin()
        .arg("lsp")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .expect("spawn nika lsp");
    {
        let mut stdin = child.stdin.take().expect("stdin");
        let msgs = [
            serde_json::json!({"jsonrpc":"2.0","id":1,"method":"initialize",
                "params":{"processId":null,"rootUri":null,"capabilities":{}}}),
            serde_json::json!({"jsonrpc":"2.0","method":"initialized","params":{}}),
            // the bomb, delivered as a document.
            serde_json::json!({"jsonrpc":"2.0","method":"textDocument/didOpen",
                "params":{"textDocument":{"uri":"file:///t/bomb.nika.yaml",
                    "languageId":"nika","version":1,"text":bomb}}}),
            // a SECOND document, opened AFTER the bomb — its diagnostics only
            // publish if the server survived and kept draining the stream.
            serde_json::json!({"jsonrpc":"2.0","method":"textDocument/didOpen",
                "params":{"textDocument":{"uri":"file:///t/after.nika.yaml",
                    "languageId":"nika","version":1,"text":VALID}}}),
            // a request AFTER the bomb — a reply proves it still answers.
            serde_json::json!({"jsonrpc":"2.0","id":7,"method":"textDocument/hover",
                "params":{"textDocument":{"uri":"file:///t/after.nika.yaml"},
                    "position":{"line":0,"character":0}}}),
            serde_json::json!({"jsonrpc":"2.0","id":2,"method":"shutdown","params":null}),
            serde_json::json!({"jsonrpc":"2.0","method":"exit"}),
        ];
        for m in &msgs {
            stdin.write_all(&lsp_frame(m)).expect("write frame");
        }
    }
    let out = child.wait_with_output().expect("wait");
    // (1) the bomb did not abort the process — the #282 regression guard.
    assert_eq!(
        out.status.code(),
        Some(0),
        "the compact-block bomb must not crash the server (shutdown → exit 0)"
    );
    let stdout = String::from_utf8_lossy(&out.stdout);
    // (2) it produced the graceful stack-safety diagnostic (not silence, not
    // a crash) — the exact bounded-parse rejection, pinned by code.
    assert!(
        stdout.contains("compact block") && stdout.contains("NIKA-PARSE-001"),
        "the bomb produced the graceful stack-safety diagnostic: {stdout}"
    );
    // (3) survival: the post-bomb document published AND the post-bomb request
    // was answered. A crashed or wedged server would show neither.
    assert!(
        stdout.contains("after.nika.yaml"),
        "the server kept serving — the post-bomb document published: {stdout}"
    );
    assert!(
        stdout.contains("\"id\":7"),
        "the server kept answering requests after the bomb (hover id 7): {stdout}"
    );
}

/// Requests carrying adversarial `Position`s — past-EOF, `u32::MAX`, and
/// mid-multibyte (a byte inside a 🦋 surrogate pair) — must never panic the
/// sync serve loop. Driven over real stdio against a multibyte document
/// across all four read features. A panic in any handler aborts the process
/// → non-zero exit and the four request replies never arrive.
#[test]
fn lsp_survives_adversarial_request_positions_over_stdio() {
    use std::process::Stdio;
    // butterfly + é in a value, a multibyte task id — every position below
    // lands in, or past, one of this document's multibyte spans.
    let multi = "nika: butterfly\ntasks:\n  café:\n    exec: { command: [\"echo\", \"${{ tasks.café.output }}\"] }\n";
    let uri = "file:///t/multi.nika.yaml";
    let max = u32::MAX;
    let mut child = bin()
        .arg("lsp")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .expect("spawn nika lsp");
    {
        let mut stdin = child.stdin.take().expect("stdin");
        let msgs = [
            serde_json::json!({"jsonrpc":"2.0","id":1,"method":"initialize",
                "params":{"processId":null,"rootUri":null,"capabilities":{}}}),
            serde_json::json!({"jsonrpc":"2.0","method":"initialized","params":{}}),
            serde_json::json!({"jsonrpc":"2.0","method":"textDocument/didOpen",
                "params":{"textDocument":{"uri":uri,"languageId":"nika","version":1,"text":multi}}}),
            // hover at (u32::MAX, u32::MAX) — line and column both past the doc
            serde_json::json!({"jsonrpc":"2.0","id":50,"method":"textDocument/hover",
                "params":{"textDocument":{"uri":uri},"position":{"line":max,"character":max}}}),
            // completion at (0, u32::MAX) — column far past the first line
            serde_json::json!({"jsonrpc":"2.0","id":51,"method":"textDocument/completion",
                "params":{"textDocument":{"uri":uri},"position":{"line":0,"character":max}}}),
            // definition mid-surrogate — line 1 is `workflow: 🦋é`, UTF-16
            // column 11 is the second half of the 🦋 surrogate pair
            serde_json::json!({"jsonrpc":"2.0","id":52,"method":"textDocument/definition",
                "params":{"textDocument":{"uri":uri},"position":{"line":1,"character":11}}}),
            // documentSymbol over the whole multibyte document
            serde_json::json!({"jsonrpc":"2.0","id":53,"method":"textDocument/documentSymbol",
                "params":{"textDocument":{"uri":uri}}}),
            serde_json::json!({"jsonrpc":"2.0","id":2,"method":"shutdown","params":null}),
            serde_json::json!({"jsonrpc":"2.0","method":"exit"}),
        ];
        for m in &msgs {
            stdin.write_all(&lsp_frame(m)).expect("write frame");
        }
    }
    let out = child.wait_with_output().expect("wait");
    assert_eq!(
        out.status.code(),
        Some(0),
        "no adversarial position may crash the server (shutdown → exit 0)"
    );
    let stdout = String::from_utf8_lossy(&out.stdout);
    for id in [50, 51, 52, 53] {
        assert!(
            stdout.contains(&format!("\"id\":{id}")),
            "request {id} got a reply (the server answered, did not crash): {stdout}"
        );
    }
}
