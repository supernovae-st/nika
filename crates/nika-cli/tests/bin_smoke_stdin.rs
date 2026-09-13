// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>
#![allow(clippy::expect_used, clippy::panic)]
// The workspace bans std::process::Command (production spawns ride the
// kernel ShellExecutor seam). This test's WHOLE JOB is to execute the
// real `nika` binary (CARGO_BIN_EXE) — routing through the kernel
// seam would test the lib, not the binary contract. The one legitimate
// carve-out class (same exemption as a future assert_cmd harness).
#![allow(clippy::disallowed_types)]

//! Binary smoke · stdin (`-`) — the editor wire without the tmp-file
//! dance: the REAL `nika` executable reading workflows from stdin through
//! its locked exit-code contract (spec §4 · `0` ok · `2` file findings ·
//! `3` environment error). Split out of `bin_smoke.rs` (the 1 500-LOC
//! ratchet) — the dash seam every `load_checked` verb inherits.
//!
//! Zero harness deps: `CARGO_BIN_EXE_<name>` is cargo's native bin-test
//! mechanism.

use std::io::Write as _;
use std::process::Command;

fn bin() -> Command {
    Command::new(env!("CARGO_BIN_EXE_nika"))
}

const VALID: &str = r#"
nika: smoke
permits: { exec: ["echo"] }
tasks:
  greet:
    exec: { command: ["echo", "hello"] }
"#;

const INVALID: &str = r#"
nika: smoke-broken
tasks:
  a:
    after:
      b: success
    exec: { command: ["true"] }
  b:
    after:
      a: success
    exec: { command: ["true"] }
"#;

/// `nika check - --json` reads the workflow from stdin: exit 0 + clean
/// report for a valid doc — the seam every `load_checked` verb inherits.
#[test]
fn check_dash_reads_stdin_valid() {
    use std::process::Stdio;
    let mut child = bin()
        .args(["check", "-", "--json"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .expect("spawn");
    child
        .stdin
        .take()
        .expect("stdin")
        .write_all(VALID.as_bytes())
        .expect("pipe body");
    let out = child.wait_with_output().expect("wait");
    assert_eq!(out.status.code(), Some(0), "clean stdin doc exits 0");
    let doc: serde_json::Value =
        serde_json::from_slice(&out.stdout).expect("json report on stdout");
    assert_eq!(doc["clean"], true);
}

/// Findings on a stdin doc keep the exit-code contract (`2` = file).
#[test]
fn check_dash_reads_stdin_findings_exit_2() {
    use std::process::Stdio;
    let mut child = bin()
        .args(["check", "-", "--json"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .expect("spawn");
    child
        .stdin
        .take()
        .expect("stdin")
        .write_all(INVALID.as_bytes())
        .expect("pipe body");
    let out = child.wait_with_output().expect("wait");
    assert_eq!(out.status.code(), Some(2), "stdin findings exit 2");
    let doc: serde_json::Value =
        serde_json::from_slice(&out.stdout).expect("json report on stdout");
    assert_eq!(doc["clean"], false);
}

/// `nika inspect - --format json` inherits the dash (`load_checked` seam).
#[test]
fn graph_dash_reads_stdin() {
    use std::process::Stdio;
    let mut child = bin()
        .args(["inspect", "-", "--format", "json"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .expect("spawn");
    child
        .stdin
        .take()
        .expect("stdin")
        .write_all(VALID.as_bytes())
        .expect("pipe body");
    let out = child.wait_with_output().expect("wait");
    assert_eq!(out.status.code(), Some(0), "graph on stdin doc exits 0");
    let doc: serde_json::Value = serde_json::from_slice(&out.stdout).expect("graph json on stdout");
    assert!(doc["nodes"].is_array());
}

/// `nika test -` is REFUSED with guidance (exit 3): the golden lives
/// beside the file, and a dirty doc would re-read consumed stdin.
#[test]
fn test_dash_is_refused_with_guidance() {
    use std::process::Stdio;
    let mut child = bin()
        .args(["test", "-"])
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn");
    child
        .stdin
        .take()
        .expect("stdin")
        .write_all(VALID.as_bytes())
        .expect("pipe body");
    let out = child.wait_with_output().expect("wait");
    assert_eq!(out.status.code(), Some(3), "refused as environment");
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(
        err.contains("golden lives beside the file"),
        "guidance: {err}"
    );
}
