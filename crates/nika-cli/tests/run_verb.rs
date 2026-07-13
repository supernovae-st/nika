// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>
#![allow(clippy::expect_used, clippy::panic)]
// This suite executes the real `nika-cli` binary (CARGO_BIN_EXE) — its
// whole job is the binary contract, so it spawns processes (the same
// sanctioned carve-out as bin_smoke.rs).
#![allow(clippy::disallowed_types)]

//! `nika run` at the BINARY plane — the locked exit contract (spec §4),
//! the audit-before-run guarantee, and the two output lanes, all through
//! the real composed runtime (real subprocess effects · mock/echo infer
//! needs no network/key · hermetic).

use std::io::Write as _;
use std::process::Command;

fn bin() -> Command {
    Command::new(env!("CARGO_BIN_EXE_nika-cli"))
}

fn fixture(name: &str, yaml: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join("nika-run-verb");
    std::fs::create_dir_all(&dir).expect("tmp dir");
    let path = dir.join(name);
    let mut f = std::fs::File::create(&path).expect("fixture file");
    f.write_all(yaml.as_bytes()).expect("fixture body");
    path
}

const OK_EXEC: &str = r#"
nika: v1
workflow:
  id: run-ok
tasks:
  greet:
    exec: { command: ["echo", "hello"] }
  after:
    depends_on: [greet]
    exec: { command: ["echo", "done"] }
"#;

const FAILING: &str = r#"
nika: v1
workflow:
  id: run-fail
tasks:
  boom:
    exec: { command: ["false"] }
"#;

const CYCLE: &str = r#"
nika: v1
workflow:
  id: run-cycle
tasks:
  a:
    depends_on: [b]
    exec: { command: ["true"] }
  b:
    depends_on: [a]
    exec: { command: ["true"] }
"#;

const INFER: &str = r#"
nika: v1
workflow:
  id: run-infer
model: mock/echo
tasks:
  think:
    infer: { prompt: "hello" }
"#;

#[test]
fn clean_exec_workflow_runs_and_exits_zero() {
    let wf = fixture("ok.nika.yaml", OK_EXEC);
    let out = bin()
        .arg("run")
        .arg(&wf)
        .arg("--json")
        .args(["--color", "never"])
        .output()
        .expect("binary runs");
    assert_eq!(
        out.status.code(),
        Some(0),
        "a clean run exits 0 · stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let stdout = String::from_utf8(out.stdout).expect("utf8");
    // NDJSON lane: every line is one Event · the run completed.
    let lines: Vec<&str> = stdout.lines().collect();
    assert!(lines.len() >= 2, "the stream carries events");
    for line in &lines {
        let _: serde_json::Value = serde_json::from_str(line).expect("each line is one JSON Event");
    }
    assert!(
        stdout.contains("workflow_completed"),
        "the terminal frame is completed"
    );
    assert!(!stdout.contains('\x1b'), "--json is never coloured");
}

#[test]
fn a_failing_command_exits_one() {
    let wf = fixture("fail.nika.yaml", FAILING);
    let out = bin()
        .arg("run")
        .arg(&wf)
        .arg("--json")
        .args(["--color", "never"])
        .output()
        .expect("binary runs");
    assert_eq!(
        out.status.code(),
        Some(1),
        "a workflow that RAN and FAILED exits 1 (distinct from a finding)"
    );
    let stdout = String::from_utf8(out.stdout).expect("utf8");
    assert!(stdout.contains("workflow_failed"), "the verdict is failed");
}

#[test]
fn a_dirty_workflow_never_executes_and_exits_two() {
    let wf = fixture("cycle.nika.yaml", CYCLE);
    let out = bin()
        .arg("run")
        .arg(&wf)
        .args(["--color", "never"])
        .output()
        .expect("binary runs");
    assert_eq!(
        out.status.code(),
        Some(2),
        "audit-before-run: a cycle is a FILE finding (2) · never executes"
    );
    let stdout = String::from_utf8(out.stdout).expect("utf8");
    // No execution frames — the dirty report short-circuits.
    assert!(
        !stdout.contains("workflow_started") && !stdout.contains("task_started"),
        "the dirty workflow never reached the runtime"
    );
}

#[test]
fn an_unreadable_file_is_an_environment_error() {
    let out = bin()
        .arg("run")
        .arg("/nonexistent/ghost.nika.yaml")
        .output()
        .expect("binary runs");
    assert_eq!(
        out.status.code(),
        Some(3),
        "unreadable file = environment (3)"
    );
}

#[test]
fn an_infer_workflow_runs_over_mock_echo_without_network() {
    let wf = fixture("infer.nika.yaml", INFER);
    let out = bin()
        .arg("run")
        .arg(&wf)
        .arg("--json")
        .args(["--color", "never"])
        .output()
        .expect("binary runs");
    assert_eq!(
        out.status.code(),
        Some(0),
        "mock/echo needs no key/network · stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let stdout = String::from_utf8(out.stdout).expect("utf8");
    assert!(
        stdout.contains("\"tokens\""),
        "the infer completion reports token spend"
    );
}

#[test]
fn the_plain_lane_narrates_cleanly_when_piped() {
    // Non-TTY (piped): the fold NARRATES (#321 — header at start · one
    // storyboard line per settle · the meter as the close) with ZERO
    // cursor escapes in the captured output (the CI-capture contract).
    let wf = fixture("ok2.nika.yaml", OK_EXEC);
    let out = bin()
        .arg("run")
        .arg(&wf)
        .args(["--color", "never"])
        .arg("--ascii")
        .output()
        .expect("binary runs");
    assert_eq!(out.status.code(), Some(0));
    let stdout = String::from_utf8(out.stdout).expect("utf8");
    assert!(
        stdout.contains("greet"),
        "the final frame painted the tasks"
    );
    assert!(
        !stdout.contains('\x1b'),
        "piped (non-TTY) leaks no cursor escapes: {stdout:?}"
    );
    // The sober register stays sober: the TTY-only flow epilogue (the
    // waterfall chart + outputs pointer) never reaches a piped capture.
    assert!(
        !stdout.contains("\n  0s ") && !stdout.contains("outputs →"),
        "piped (non-TTY) carries no waterfall/epilogue art: {stdout:?}"
    );
}

// `examples run` EXECUTES — and carries the run trio (--var ·
// --no-progress · --max-cost-usd · gauntlet F7 2026-07-12). The trio
// makes a hermetic smoke POSSIBLE at last: 04-schema-retry has a
// `required:` var (unrunnable by this surface before) and infers clean
// under `--model mock/echo` — zero keys, zero network, the exact combo
// the old `#[ignore = "needs a live ollama"]` excuse said couldn't exist.
#[test]
fn examples_run_carries_the_run_trio_hermetically() {
    let out = bin()
        .args([
            "examples",
            "run",
            "04-schema-retry",
            "--model",
            "mock/echo",
            "--var",
            "text=Ada met Babbage in London",
            "--no-progress",
            "--max-cost-usd",
            "0.01",
        ])
        .output()
        .expect("binary runs");
    assert_eq!(
        out.status.code(),
        Some(0),
        "a required-var example runs hermetically with the trio · stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
}

#[test]
fn examples_run_unknown_slug_is_a_finding() {
    let out = bin()
        .arg("examples")
        .arg("run")
        .arg("no-such-example")
        .output()
        .expect("binary runs");
    assert_eq!(
        out.status.code(),
        Some(2),
        "an unknown slug is a FILE finding (2) · names the set on stderr"
    );
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("examples list"),
        "points at the set: {stderr}"
    );
}
