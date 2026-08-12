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
nika: run-ok
permits: { exec: ["echo"] }
tasks:
  greet:
    exec: { command: ["echo", "hello"] }
  after:
    after:
      greet: success
    exec: { command: ["echo", "done"] }
"#;

const FAILING: &str = r#"
nika: run-fail
permits: { exec: ["false"] }
tasks:
  boom:
    exec: { command: ["false"] }
"#;

const CYCLE: &str = r#"
nika: run-cycle
permits: { exec: ["true"] }
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

const INFER: &str = r#"
nika: run-infer
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

// `try` EXECUTES — and carries the run trio (--var · --no-progress ·
// --max-cost-usd · gauntlet F7 2026-07-12). V5 sharpens the law: try is
// OFFLINE BY DEFAULT (RAMS-4) — no `--model` here, and the run must
// still be zero-keys zero-network green. 04-schema-retry has a
// `required:` var (unrunnable by this surface before) and infers clean
// on the default rehearsal seat.
#[test]
fn try_carries_the_run_trio_hermetically_offline_by_default() {
    let out = bin()
        .args([
            "try",
            "04-schema-retry",
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
fn try_unknown_slug_is_a_finding() {
    let out = bin()
        .arg("try")
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
        stderr.contains("nika try"),
        "points at the showroom: {stderr}"
    );
}

/// The #603 admission repro (the 2026-07-21 four-authority translation):
/// `needle` is `required: true` with no `default:` — without `--var` the
/// run must refuse at ADMISSION, before wave 1 spends a task.
fn required_input_wf(out_path: &std::path::Path) -> String {
    r#"nika: req-input-admission
inputs:
  needle: { type: string, required: true }
permits:
  fs: { write: ["OUT"] }
  tools: ["nika:write"]
tasks:
  first:
    invoke: { tool: "nika:write", args: { path: "OUT", content: "spends before the crash" } }
  use:
    after:
      first: success
    invoke: { tool: "nika:write", args: { path: "OUT", content: "${{ inputs.needle }}" } }
"#
    .replace("OUT", &out_path.display().to_string())
}

#[test]
fn a_missing_required_input_is_refused_before_any_task_event() {
    let out_file = std::env::temp_dir()
        .join("nika-run-verb")
        .join("req-input-missing-out.txt");
    let _ = std::fs::remove_file(&out_file);
    let wf = fixture("req-input-missing.nika.yaml", &required_input_wf(&out_file));
    let out = bin()
        .arg("run")
        .arg(&wf)
        .arg("--json")
        .args(["--color", "never"])
        .output()
        .expect("binary runs");
    assert_eq!(
        out.status.code(),
        Some(3),
        "the admission refusal is the ENV class · stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let stdout = String::from_utf8(out.stdout).expect("utf8");
    assert!(
        stdout.trim().is_empty(),
        "refused BEFORE any event — not even the prologue on the journal stream: {stdout}"
    );
    let stderr = String::from_utf8(out.stderr).expect("utf8");
    assert!(stderr.contains("NIKA-1708"), "the launch class: {stderr}");
    assert!(stderr.contains("`needle`"), "the input is named: {stderr}");
    assert!(
        stderr.contains("--var needle=<value>"),
        "the satisfaction is taught: {stderr}"
    );
    assert!(
        !out_file.exists(),
        "wave 1 never spent — `first` never wrote the file"
    );
}

#[test]
fn a_var_override_satisfies_the_required_input() {
    // The control: `--var needle=…` IS the input's value (F4) — the SAME
    // workflow completes.
    let out_file = std::env::temp_dir()
        .join("nika-run-verb")
        .join("req-input-satisfied-out.txt");
    let _ = std::fs::remove_file(&out_file);
    let wf = fixture(
        "req-input-satisfied.nika.yaml",
        &required_input_wf(&out_file),
    );
    let out = bin()
        .arg("run")
        .arg(&wf)
        .arg("--json")
        .args(["--color", "never"])
        .arg("--var")
        .arg("needle=ok")
        .output()
        .expect("binary runs");
    assert_eq!(
        out.status.code(),
        Some(0),
        "the override satisfies admission · stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let stdout = String::from_utf8(out.stdout).expect("utf8");
    assert!(stdout.contains("workflow_completed"), "the run completes");
    let written = std::fs::read_to_string(&out_file).expect("the use task wrote");
    assert!(
        written.contains("ok"),
        "the override reached the read: {written}"
    );
}

/// M1 · the offline agent rehearsal, end to end at the binary: an agent
/// granted `["nika:wait", "nika:done"]` watches its FIRST tool error
/// once (the mock's synthesized args fail the real `nika:wait`
/// contract), then the mock prefers the granted `nika:done` — the loop
/// completes in two turns instead of stalling byte-identically
/// (NIKA-467, the pre-fix verdict on this exact shape).
#[test]
fn a_wait_done_agent_errors_once_then_completes_via_done() {
    const WAIT_DONE: &str = r#"
nika: run-agent-mock-done
model: mock/echo
permits:
  tools: ["nika:wait", "nika:done"]
tasks:
  loop:
    agent:
      prompt: "wait once, then finish"
      tools: ["nika:wait", "nika:done"]
      max_turns: 5
"#;
    let wf = fixture("agent-wait-done.nika.yaml", WAIT_DONE);
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
        "the rehearsal completes · stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let stdout = String::from_utf8(out.stdout).expect("utf8");
    assert!(
        !stdout.contains("agent_stalled") && !stdout.contains("NIKA-467"),
        "no byte-identical stall"
    );
    // The wait errored EXACTLY once (turn 1) — then the mock preferred
    // the granted done (turn 2 is loop-owned: no dispatch frame rides).
    let wait_frames: Vec<&str> = stdout
        .lines()
        .filter(|l| l.contains("\"kind\":\"tool_invoked\"") && l.contains("nika:wait"))
        .collect();
    assert_eq!(
        wait_frames.len(),
        1,
        "one wait call, errored, never repeated: {stdout}"
    );
    let frame: serde_json::Value = serde_json::from_str(wait_frames[0]).expect("one JSON event");
    assert!(
        frame["fields"]
            .as_array()
            .expect("fields")
            .iter()
            .any(|f| f["key"] == "error" && f["value"] == true),
        "the wait's synthesized args error deterministically: {}",
        wait_frames[0]
    );
    let completed = stdout
        .lines()
        .find(|l| l.contains("\"kind\":\"task_completed\""))
        .expect("the loop completed");
    assert!(
        completed.contains("agent · 2 turns"),
        "done on turn two, deterministically: {completed}"
    );
}
