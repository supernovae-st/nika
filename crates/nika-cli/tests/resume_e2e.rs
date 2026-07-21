// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>
#![allow(clippy::expect_used, clippy::panic)]
// This suite executes the real `nika-cli` binary (CARGO_BIN_EXE) — its
// whole job is the binary contract, so it spawns processes (the same
// sanctioned carve-out as bin_smoke.rs / run_verb.rs).
#![allow(clippy::disallowed_types)]

//! ADR-099 conformance — the 3 behavioral fixtures at the BINARY plane
//! (`tests/runtime/resume/` sketch · run → interrupt → resume):
//!
//! - **(a) kill-midrun-resume-completes-remainder** — the first
//!   invocation's trace is cut after task `a` completes (the torn tail a
//!   SIGKILL leaves); the resumed run cache-hits `a`, executes only the
//!   remainder live, and ends `workflow_completed` with outputs
//!   IDENTICAL to an uninterrupted run.
//! - **(b) input-change-rehashes-and-reruns** — same trace, one input
//!   value changed (`--var` override): the consuming task re-runs (no
//!   cache hit) · an untouched sibling branch still cache-hits.
//! - **(c) paused-prompt-rearms** — a non-interactive run hits a
//!   default-less `nika:prompt` → exits `paused` (code 4), the trace
//!   carries `workflow_paused` + the prompt payload; resumed with an
//!   answer → upstream cache-hits, the prompt binds, the run completes;
//!   resumed without one → it pauses again (idempotent).

use std::io::Write as _;
use std::process::Command;

fn bin() -> Command {
    Command::new(env!("CARGO_BIN_EXE_nika-cli"))
}

fn fixture(name: &str, yaml: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join("nika-resume-e2e");
    std::fs::create_dir_all(&dir).expect("tmp dir");
    let path = dir.join(name);
    let mut f = std::fs::File::create(&path).expect("fixture file");
    f.write_all(yaml.as_bytes()).expect("fixture body");
    path
}

fn write_trace(name: &str, content: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join("nika-resume-e2e");
    std::fs::create_dir_all(&dir).expect("tmp dir");
    let path = dir.join(name);
    std::fs::write(&path, content).expect("trace written");
    path
}

/// Every `--json` line of `kind` naming `task` (empty = none did).
fn events_for<'a>(stdout: &'a str, kind: &str, task: &str) -> Vec<&'a str> {
    stdout
        .lines()
        .filter(|l| l.contains(&format!("\"kind\":\"{kind}\"")))
        .filter(|l| l.contains("\"task\"") && l.contains(&format!("\"{task}\"")))
        .collect()
}

fn has_kind(stdout: &str, kind: &str) -> bool {
    stdout
        .lines()
        .any(|l| l.contains(&format!("\"kind\":\"{kind}\"")))
}

// ─── (a) kill-midrun → resume completes the remainder ───────────────────

const CHAIN: &str = r#"
nika: v1
workflow:
  id: resume-chain
tasks:
  a:
    exec: { command: ["echo", "alpha"] }
  b:
    with:
      alpha: ${{ tasks.a.output }}
    exec: { command: ["echo", "beta", "${{ with.alpha }}"] }
outputs:
  built: ${{ tasks.b.output }}
"#;

#[test]
fn kill_midrun_resume_completes_the_remainder() {
    let wf = fixture("chain.nika.yaml", CHAIN);

    // The uninterrupted baseline — the outputs the resumed run must match.
    let full = bin()
        .args(["run", &wf.to_string_lossy(), "--output", "json"])
        .output()
        .expect("binary runs");
    assert_eq!(full.status.code(), Some(0));
    let baseline_outputs = String::from_utf8(full.stdout).expect("utf8");

    // A full --json trace, then CUT it right after `a` completes + a torn
    // half-line — the exact artifact a SIGKILL between tasks leaves.
    let run = bin()
        .args(["run", &wf.to_string_lossy(), "--json", "--color", "never"])
        .output()
        .expect("binary runs");
    assert_eq!(run.status.code(), Some(0));
    let stream = String::from_utf8(run.stdout).expect("utf8");
    let mut kept = Vec::new();
    for line in stream.lines() {
        kept.push(line);
        if line.contains("\"kind\":\"task_completed\"") && line.contains("\"a\"") {
            break;
        }
    }
    let torn = format!("{}\n{{\"id\":{{\"uuid\":\"torn-by-sigkill", kept.join("\n"));
    let trace = write_trace("chain-torn.ndjson", &torn);

    // Resume: `a` cache-hits (VISIBLE) · only `b` executes live · green.
    let resumed = bin()
        .args([
            "run",
            &wf.to_string_lossy(),
            "--resume",
            &trace.to_string_lossy(),
            "--json",
            "--color",
            "never",
        ])
        .output()
        .expect("binary runs");
    let stdout = String::from_utf8(resumed.stdout).expect("utf8");
    let stderr = String::from_utf8(resumed.stderr).expect("utf8");
    assert_eq!(resumed.status.code(), Some(0), "stderr: {stderr}");
    assert!(
        !events_for(&stdout, "task_cache_hit", "a").is_empty(),
        "events_include task.cache_hit:a — got:\n{stdout}"
    );
    assert!(
        events_for(&stdout, "task_started", "a").is_empty(),
        "`a` must NOT re-execute:\n{stdout}"
    );
    assert!(
        !events_for(&stdout, "task_started", "b").is_empty(),
        "`b` executes live:\n{stdout}"
    );
    assert!(has_kind(&stdout, "workflow_completed"), "{stdout}");
    assert!(
        stderr.contains("trace truncated"),
        "the torn tail is surfaced, never fatal: {stderr}"
    );
    assert!(
        stderr.contains("resumed · 1 skipped (cache hit) · 1 ran live"),
        "the summary line: {stderr}"
    );

    // The resumed outputs are byte-identical to the uninterrupted run.
    let resumed_outputs = bin()
        .args([
            "run",
            &wf.to_string_lossy(),
            "--resume",
            &trace.to_string_lossy(),
            "--output",
            "json",
        ])
        .output()
        .expect("binary runs");
    assert_eq!(resumed_outputs.status.code(), Some(0));
    assert_eq!(
        String::from_utf8(resumed_outputs.stdout).expect("utf8"),
        baseline_outputs,
        "outputs identical to an uninterrupted run (ADR-099 fixture a)"
    );
}

// ─── (b) input change → rehash → re-run (sibling still skips) ───────────

const FORK: &str = r#"
nika: v1
workflow:
  id: resume-fork
inputs:
  topic: { type: string, default: "news" }
tasks:
  uses_var:
    exec: { command: ["echo", "about", "${{ inputs.topic }}"] }
  sibling:
    exec: { command: ["echo", "steady"] }
"#;

#[test]
fn input_change_rehashes_and_reruns_only_the_consumer() {
    let wf = fixture("fork.nika.yaml", FORK);
    let run = bin()
        .args([
            "run",
            &wf.to_string_lossy(),
            "--json",
            "--color",
            "never",
            "--var",
            "topic=rust",
        ])
        .output()
        .expect("binary runs");
    assert_eq!(run.status.code(), Some(0));
    let trace = write_trace("fork.ndjson", &String::from_utf8(run.stdout).expect("utf8"));

    // Same trace · ONE var changed: the consumer re-runs (its
    // resolved-input hash mismatches) · the untouched sibling cache-hits.
    let resumed = bin()
        .args([
            "run",
            &wf.to_string_lossy(),
            "--resume",
            &trace.to_string_lossy(),
            "--json",
            "--color",
            "never",
            "--var",
            "topic=quantum",
        ])
        .output()
        .expect("binary runs");
    let stdout = String::from_utf8(resumed.stdout).expect("utf8");
    assert_eq!(resumed.status.code(), Some(0));
    assert!(
        events_for(&stdout, "task_cache_hit", "uses_var").is_empty(),
        "the consumer's input changed → NO cache hit:\n{stdout}"
    );
    assert!(
        !events_for(&stdout, "task_started", "uses_var").is_empty(),
        "the consumer re-runs live:\n{stdout}"
    );
    assert!(
        !events_for(&stdout, "task_cache_hit", "sibling").is_empty(),
        "the untouched sibling still cache-hits:\n{stdout}"
    );

    // Unchanged inputs → BOTH cache-hit (the control arm).
    let unchanged = bin()
        .args([
            "run",
            &wf.to_string_lossy(),
            "--resume",
            &trace.to_string_lossy(),
            "--json",
            "--color",
            "never",
            "--var",
            "topic=rust",
        ])
        .output()
        .expect("binary runs");
    let stdout = String::from_utf8(unchanged.stdout).expect("utf8");
    assert!(
        !events_for(&stdout, "task_cache_hit", "uses_var").is_empty()
            && !events_for(&stdout, "task_cache_hit", "sibling").is_empty(),
        "same inputs → everything skips:\n{stdout}"
    );
}

// ─── (c) paused prompt re-arms (idempotent · answer binds) ──────────────

const GATED: &str = r#"
nika: v1
workflow:
  id: resume-gated
tasks:
  prep:
    exec: { command: ["echo", "staged"] }
  approve:
    after:
      prep: success
    invoke:
      tool: "nika:prompt"
      args: { mode: "input", message: "ship it?" }
  ship:
    with:
      answer: ${{ tasks.approve.output }}
    exec: { command: ["echo", "shipping", "${{ with.answer }}"] }
"#;

#[test]
fn paused_prompt_rearms_and_an_answer_completes_the_run() {
    let wf = fixture("gated.nika.yaml", GATED);

    // Non-interactive run hits the default-less prompt → paused (exit 4),
    // the trace carries workflow_paused + the prompt payload.
    let run = bin()
        .args(["run", &wf.to_string_lossy(), "--json", "--color", "never"])
        .output()
        .expect("binary runs");
    assert_eq!(run.status.code(), Some(4), "run state paused → exit 4");
    let stream = String::from_utf8(run.stdout).expect("utf8");
    assert!(has_kind(&stream, "workflow_paused"), "{stream}");
    let paused_line = stream
        .lines()
        .find(|l| l.contains("\"kind\":\"workflow_paused\""))
        .expect("the paused frame");
    assert!(
        paused_line.contains("approve") && paused_line.contains("ship it?"),
        "the payload rides the frame: {paused_line}"
    );
    assert!(
        !has_kind(&stream, "task_failed") && !has_kind(&stream, "workflow_failed"),
        "a pause is never a failure:\n{stream}"
    );
    let trace = write_trace("gated.ndjson", &stream);

    // Resumed WITHOUT an answer → pauses again (idempotent) · prep skips.
    let repaused = bin()
        .args([
            "run",
            &wf.to_string_lossy(),
            "--resume",
            &trace.to_string_lossy(),
            "--json",
            "--color",
            "never",
        ])
        .output()
        .expect("binary runs");
    let stdout = String::from_utf8(repaused.stdout).expect("utf8");
    assert_eq!(
        repaused.status.code(),
        Some(4),
        "re-pauses any number of times"
    );
    assert!(!events_for(&stdout, "task_cache_hit", "prep").is_empty());
    assert!(has_kind(&stdout, "workflow_paused"));

    // Resumed WITH the answer → upstream cache-hits · the prompt binds ·
    // the run completes and downstream sees the bound answer.
    let answered = bin()
        .args([
            "run",
            &wf.to_string_lossy(),
            "--resume",
            &trace.to_string_lossy(),
            "--answer",
            "approve=yes",
            "--json",
            "--color",
            "never",
        ])
        .output()
        .expect("binary runs");
    let stdout = String::from_utf8(answered.stdout).expect("utf8");
    let stderr = String::from_utf8(answered.stderr).expect("utf8");
    assert_eq!(answered.status.code(), Some(0), "stderr: {stderr}");
    assert!(!events_for(&stdout, "task_cache_hit", "prep").is_empty());
    assert!(has_kind(&stdout, "workflow_completed"), "{stdout}");
    let ship_completed = events_for(&stdout, "task_completed", "ship");
    assert!(
        ship_completed.iter().any(|l| l.contains("shipping yes")),
        "downstream observed the bound answer:\n{stdout}"
    );
}

/// The answered-prompt path is UNCHANGED (ADR-099 rider: the pause
/// occupies only the PROMPT-001 branch): a prompt WITH `default:` under
/// `--json` completes — no pause, no exit 4.
#[test]
fn a_prompt_with_a_default_never_pauses() {
    const DEFAULTED: &str = r#"
nika: v1
workflow:
  id: defaulted-prompt
tasks:
  ask:
    invoke:
      tool: "nika:prompt"
      args: { mode: "confirm", message: "auto?", default: true }
"#;
    let wf = fixture("defaulted.nika.yaml", DEFAULTED);
    let run = bin()
        .args(["run", &wf.to_string_lossy(), "--json", "--color", "never"])
        .output()
        .expect("binary runs");
    let stdout = String::from_utf8(run.stdout).expect("utf8");
    assert_eq!(run.status.code(), Some(0), "{stdout}");
    assert!(!has_kind(&stdout, "workflow_paused"), "{stdout}");
    assert!(has_kind(&stdout, "workflow_completed"), "{stdout}");
}
