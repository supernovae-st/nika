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
workflow: run-ok
tasks:
  - id: greet
    exec: { command: "echo hello" }
  - id: after
    depends_on: [greet]
    exec: { command: "echo done" }
"#;

const FAILING: &str = r#"
nika: v1
workflow: run-fail
tasks:
  - id: boom
    exec: { command: "false" }
"#;

const CYCLE: &str = r#"
nika: v1
workflow: run-cycle
tasks:
  - id: a
    depends_on: [b]
    exec: { command: "true" }
  - id: b
    depends_on: [a]
    exec: { command: "true" }
"#;

const INFER: &str = r#"
nika: v1
workflow: run-infer
model: mock/echo
tasks:
  - id: think
    infer: { prompt: "hello" }
"#;

#[test]
fn clean_exec_workflow_runs_and_exits_zero() {
    let wf = fixture("ok.nika.yaml", OK_EXEC);
    let out = bin()
        .arg("run")
        .arg(&wf)
        .arg("--json")
        .arg("--no-color")
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
        .arg("--no-color")
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
        .arg("--no-color")
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
        .arg("--no-color")
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
fn the_live_lane_paints_one_clean_frame_when_piped() {
    // Non-TTY (piped): the fold degrades to a single final frame · ZERO
    // cursor escapes in the captured output (the CI-capture contract).
    let wf = fixture("ok2.nika.yaml", OK_EXEC);
    let out = bin()
        .arg("run")
        .arg(&wf)
        .arg("--no-color")
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
}

// `examples run` EXECUTES (no longer refuses). 01-hello infers against
// `ollama/llama3.1` — a LIVE local model, NOT hermetic (the prior "mock/echo"
// comment was stale · the example's `model:` is a real provider). So this is
// `#[ignore]`d: it runs only where ollama serves llama3.1
// (`cargo test -- --ignored`), and never fails an offline box or the mutation
// baseline. No embedded example runs clean with zero external deps, so the
// `examples run` execution path has no hermetic smoke today.
#[test]
#[ignore = "needs a live ollama/llama3.1 — run with `cargo test -- --ignored`"]
fn examples_run_executes_an_embedded_workflow() {
    let out = bin()
        .arg("examples")
        .arg("run")
        .arg("01-hello")
        .output()
        .expect("binary runs");
    assert_eq!(
        out.status.code(),
        Some(0),
        "the embedded mock/echo example runs clean · stderr: {}",
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
