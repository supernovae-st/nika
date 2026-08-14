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
//!   resumed without one → it pauses again (idempotent). F-P4 (NEP-0013):
//!   the mint rides the pause frame, the decision lands hash-chained,
//!   and an answer whose content drifted HALTS typed (`NIKA-SEC-010` ·
//!   `approval.content_mismatch`).
//! - **(d) tampered-trace-refused-never-laundered** (trust amendment ·
//!   2026-08-08) — a trace whose chain fails the walk is REFUSED (exit
//!   2 · the FILE class, one voice with `trace verify`) naming the
//!   `--resume-unverified` opt-out; under it the run proceeds LOUDLY and
//!   the NEW journal attests the finding on its boot manifest — a
//!   laundered trace can never claim a clean ancestry silently.

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

/// One string field off an NDJSON event line (the journal's
/// `fields: [{key, value}]` shape) — the F-P4 `approval_*` reader.
fn field_str(line: &str, key: &str) -> String {
    let frame: serde_json::Value = serde_json::from_str(line).expect("one JSON event");
    frame["fields"]
        .as_array()
        .expect("fields")
        .iter()
        .find(|kv| kv["key"] == key)
        .and_then(|kv| kv["value"].as_str().map(str::to_owned))
        .unwrap_or_else(|| panic!("the frame carries {key}: {line}"))
}

// ─── (a) kill-midrun → resume completes the remainder ───────────────────

const CHAIN: &str = r#"
nika: resume-chain
permits: { exec: ["echo"] }
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
nika: resume-fork
inputs:
  topic: { type: string, default: "news" }
permits: { exec: ["echo"] }
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
nika: resume-gated
permits: { exec: ["echo"], tools: ["nika:prompt"] }
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
    // F-P4 (NEP-0013) — the mint rides the pause frame: the shown hash,
    // the ticket digest, the run nonce, the TTL. THIS is what a resume
    // signs against (WYSIWYS).
    for field in [
        "approval_shown_hash",
        "approval_digest",
        "approval_nonce",
        "approval_ttl_seconds",
    ] {
        assert!(
            paused_line.contains(field),
            "the ticket rides the pause frame ({field}): {paused_line}"
        );
    }
    let shown_digest = field_str(paused_line, "approval_digest");
    assert_eq!(shown_digest.len(), 64, "blake3 hex: {shown_digest}");
    let shown_hash = field_str(paused_line, "approval_shown_hash");
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
    // F-P4 — the decision is attested, hash-chained, and binds the SAME
    // digest the pause frame showed (montré = signé · NEP-0013 law 1+4).
    let decided = events_for(&stdout, "approval_decided", "approve");
    assert_eq!(decided.len(), 1, "one attestation:\n{stdout}");
    let frame = &decided[0];
    assert!(
        frame.contains("\"decision\",\"value\":\"allow\""),
        "{frame}"
    );
    assert!(
        frame.contains("\"source\",\"value\":\"resumed\""),
        "{frame}"
    );
    assert!(
        frame.contains(&format!("\"digest\",\"value\":\"{shown_digest}\"")),
        "the signed digest equals the shown digest:\n{frame}\nvs {shown_digest}"
    );
    assert!(
        frame.contains(&format!("\"shown_hash\",\"value\":\"{shown_hash}\"")),
        "the signed content equals the shown content:\n{frame}"
    );
}

// ─── the TEXT lane pauses too (the first-run gate · 2026-07-31) ─────────

/// A headless TEXT run (no `--json`/`--output json` — a pipe · CI · an
/// agent) hits the default-less prompt: the run PAUSES durably (exit 4 ·
/// never a NIKA-BUILTIN-PROMPT-001 red card) and teaches its exact
/// resume command on the frame. The taught line, replayed verbatim with
/// the answer, completes the run. (The seo-live-review first-run killer,
/// 2026-07-31: the rider was armed on the output FLAG, not the surface —
/// a text pipe died at its own gate in 13ms with 22 cancelled rows.)
#[test]
fn a_headless_text_run_pauses_at_the_gate_and_teaches_the_resume() {
    let wf = fixture("gated-text.nika.yaml", GATED);
    let dir = std::env::temp_dir().join("nika-resume-e2e");
    let run = bin()
        .current_dir(&dir)
        .args(["run", &wf.to_string_lossy(), "--color", "never"])
        .output()
        .expect("binary runs");
    let stdout = String::from_utf8(run.stdout).expect("utf8");
    let stderr = String::from_utf8(run.stderr).expect("utf8");
    assert_eq!(
        run.status.code(),
        Some(4),
        "paused, never failed:\n{stdout}\n{stderr}"
    );
    assert!(
        !stdout.contains("NIKA-BUILTIN-PROMPT-001") && !stderr.contains("NIKA-BUILTIN-PROMPT-001"),
        "a pause is a state, not an error:\n{stdout}"
    );
    // The teaching line: file · trace · task · the mode's answer shape.
    let hint = stdout
        .lines()
        .find(|l| l.trim_start().starts_with("resume:"))
        .unwrap_or_else(|| panic!("the pause teaches its resume line:\n{stdout}"));
    assert!(
        hint.contains("--resume") && hint.contains("--answer approve="),
        "{hint}"
    );
    // The trace the hint names exists and journals the pause, not a failure.
    let trace_rel = hint
        .split_whitespace()
        .skip_while(|w| *w != "--resume")
        .nth(1)
        .expect("the hint names its trace");
    let trace = dir.join(trace_rel);
    let journal = std::fs::read_to_string(&trace).expect("trace file exists");
    assert!(
        journal.contains("\"kind\":\"workflow_paused\""),
        "{journal}"
    );
    assert!(
        !journal.contains("\"kind\":\"workflow_failed\""),
        "a pause is never a failure:\n{journal}"
    );
    // The taught command, replayed with the answer → the run completes.
    let answered = bin()
        .current_dir(&dir)
        .args([
            "run",
            &wf.to_string_lossy(),
            "--resume",
            &trace.to_string_lossy(),
            "--answer",
            "approve=yes",
            "--color",
            "never",
        ])
        .output()
        .expect("binary runs");
    let stderr = String::from_utf8(answered.stderr).expect("utf8");
    assert_eq!(answered.status.code(), Some(0), "stderr: {stderr}");
}

/// The answered-prompt path is UNCHANGED (ADR-099 rider: the pause
/// occupies only the PROMPT-001 branch): a prompt WITH `default:` under
/// `--json` completes — no pause, no exit 4.
#[test]
fn a_prompt_with_a_default_never_pauses() {
    const DEFAULTED: &str = r#"
nika: defaulted-prompt
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

// ─── F-P4 (b) · a stale --answer signs what was never shown ──────────

/// NEP-0013 law 1 at the binary plane: the run pauses on « ship it? »,
/// the operator edits the QUESTION before resuming — the `--answer`'s
/// resolved content hash no longer matches the shown hash, so the
/// resume HALTS typed (`NIKA-SEC-010` · `approval.content_mismatch`)
/// and journals the deny. The gate never re-asks: it refuses.
#[test]
fn an_answer_against_edited_content_halts_with_content_mismatch() {
    let wf = fixture("gated.nika.yaml", GATED);

    let run = bin()
        .args(["run", &wf.to_string_lossy(), "--json", "--color", "never"])
        .output()
        .expect("binary runs");
    assert_eq!(run.status.code(), Some(4), "the first run pauses");
    let stream = String::from_utf8(run.stdout).expect("utf8");
    let trace = write_trace("gated-mismatch.ndjson", &stream);

    // The question changes under the operator's feet (message edit).
    let edited = fixture(
        "gated-edited.nika.yaml",
        &GATED.replace("ship it?", "ship it NOW?"),
    );
    let resumed = bin()
        .args([
            "run",
            &edited.to_string_lossy(),
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
    let stdout = String::from_utf8(resumed.stdout).expect("utf8");
    assert_ne!(
        resumed.status.code(),
        Some(0),
        "the mismatched answer never completes the run:\n{stdout}"
    );
    assert!(
        stdout.contains("NIKA-SEC-010") && stdout.contains("approval.content_mismatch"),
        "the typed refusal names the law:\n{stdout}"
    );
    let decided = events_for(&stdout, "approval_decided", "approve");
    assert_eq!(decided.len(), 1, "the deny is attested:\n{stdout}");
    assert!(
        decided[0].contains("\"decision\",\"value\":\"deny\""),
        "{}",
        decided[0]
    );
    assert!(
        decided[0].contains("approval.content_mismatch"),
        "{}",
        decided[0]
    );
    // The gated action never ran.
    assert!(
        events_for(&stdout, "task_started", "ship").is_empty(),
        "the gated exec never starts:\n{stdout}"
    );
}

// ─── (e) resume ACROSS a composition (spec 14 law 10 · condition 8) ─────
//
// The def_hash-tier demonstration: a parent's `invoke: workflow:` call
// participates in resume with the child's transitive source closure in
// its identity. Unchanged files cache-hit across the boundary; an
// edited child (or grandchild) re-runs the call instead of serving the
// old child's output (the wrong-skip this suite exists to forbid); a
// run torn mid-composition re-runs the child WHOLE (the coarse tier —
// within-child granularity rides the W6 semantic IR, not claimed here).

/// A per-test directory (composition needs parent+child side by side,
/// and the edited-child cases MUTATE files — never share fixtures).
fn comp_dir(name: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir()
        .join("nika-resume-e2e")
        .join(format!("{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("comp dir");
    dir
}

fn write_in(dir: &std::path::Path, name: &str, body: &str) -> std::path::PathBuf {
    let path = dir.join(name);
    std::fs::write(&path, body).expect("fixture written");
    path
}

const COMP_CHILD: &str = r#"
nika: greet-child
inputs:
  name: { type: string, required: true }
permits: { exec: ["echo"] }
tasks:
  greet:
    exec: { command: ["echo", "hello ${{ inputs.name }}"] }
outputs:
  greeting: { value: "${{ tasks.greet.output }}", type: string }
"#;

const COMP_PARENT: &str = r#"
nika: greet-parent
permits: { exec: ["echo"] }
tasks:
  before:
    exec: { command: ["echo", "pre"] }
  call:
    after:
      before: success
    invoke:
      workflow: "./child.nika.yaml"
      args: { name: "composition" }
    returns: { object: { greeting: string } }
outputs:
  echoed: { value: "${{ tasks.call.output.greeting }}", type: string }
"#;

/// Run `parent` in `dir` with `--json`, return the stream (rc asserted).
fn comp_run_json(dir: &std::path::Path, parent: &std::path::Path, extra: &[&str]) -> String {
    let mut args = vec![
        "run",
        parent.to_str().expect("utf8"),
        "--json",
        "--color",
        "never",
    ];
    args.extend_from_slice(extra);
    let out = bin()
        .current_dir(dir)
        .args(&args)
        .output()
        .expect("binary runs");
    assert_eq!(
        out.status.code(),
        Some(0),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8(out.stdout).expect("utf8")
}

/// The parent's `--output json` line (the outputs object).
fn comp_outputs(dir: &std::path::Path, parent: &std::path::Path, extra: &[&str]) -> String {
    let mut args = vec!["run", parent.to_str().expect("utf8"), "--output", "json"];
    args.extend_from_slice(extra);
    let out = bin()
        .current_dir(dir)
        .args(&args)
        .output()
        .expect("binary runs");
    assert_eq!(
        out.status.code(),
        Some(0),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8(out.stdout).expect("utf8")
}

/// Condition 8, the happy half: with NOTHING changed, the resume
/// cache-hits ACROSS the composition — the `call` task skips (the
/// child never re-executes) and the outputs are identical to the
/// uninterrupted run.
#[test]
fn resume_across_a_composition_cache_hits_the_call() {
    let dir = comp_dir("comp-hit");
    write_in(&dir, "child.nika.yaml", COMP_CHILD);
    let parent = write_in(&dir, "parent.nika.yaml", COMP_PARENT);

    let baseline = comp_outputs(&dir, &parent, &[]);
    let stream = comp_run_json(&dir, &parent, &[]);
    let trace = write_in(&dir, "full.ndjson", &stream);

    let resumed = comp_run_json(&dir, &parent, &["--resume", trace.to_str().expect("utf8")]);
    assert!(
        !events_for(&resumed, "task_cache_hit", "call").is_empty(),
        "the call skips across the boundary:\n{resumed}"
    );
    assert!(
        events_for(&resumed, "task_started", "call").is_empty(),
        "the child never re-executes:\n{resumed}"
    );
    assert!(has_kind(&resumed, "workflow_completed"), "{resumed}");

    let resumed_outputs = comp_outputs(&dir, &parent, &["--resume", trace.to_str().expect("utf8")]);
    assert_eq!(
        resumed_outputs, baseline,
        "outputs identical to the uninterrupted run"
    );
}

/// Condition 8, the law's teeth (ADR-099 trap 6 across the file
/// boundary): the child is EDITED between the run and the resume — the
/// call must NOT cache-hit (the old child's output would be wrong), it
/// re-runs live and the outputs match a fresh run of the edited tree.
#[test]
fn an_edited_child_reruns_the_call_instead_of_serving_stale_output() {
    let dir = comp_dir("comp-edit");
    write_in(&dir, "child.nika.yaml", COMP_CHILD);
    let parent = write_in(&dir, "parent.nika.yaml", COMP_PARENT);

    let stream = comp_run_json(&dir, &parent, &[]);
    let trace = write_in(&dir, "before-edit.ndjson", &stream);

    // The child's behavior changes under the trace's feet.
    write_in(
        &dir,
        "child.nika.yaml",
        &COMP_CHILD.replace("hello ${{ inputs.name }}", "goodbye ${{ inputs.name }}"),
    );

    let resumed = comp_run_json(&dir, &parent, &["--resume", trace.to_str().expect("utf8")]);
    assert!(
        events_for(&resumed, "task_cache_hit", "call").is_empty(),
        "an edited child NEVER cache-hits the call (trap 6):\n{resumed}"
    );
    assert!(
        !events_for(&resumed, "task_started", "call").is_empty(),
        "the call re-runs live:\n{resumed}"
    );
    // The untouched sibling still skips — invalidation is exact.
    assert!(
        !events_for(&resumed, "task_cache_hit", "before").is_empty(),
        "the untouched task still skips:\n{resumed}"
    );

    let resumed_outputs = comp_outputs(&dir, &parent, &["--resume", trace.to_str().expect("utf8")]);
    assert!(
        resumed_outputs.contains("goodbye composition"),
        "the resumed run serves the EDITED child's output, never the stale one: {resumed_outputs}"
    );
}

/// Condition 8, transitive: a GRANDCHILD edit re-keys the whole call
/// chain — the closure digest is a Merkle fold, so the parent's call
/// re-runs even though the direct child's bytes are unchanged.
#[test]
fn an_edited_grandchild_reruns_the_call_transitively() {
    let dir = comp_dir("comp-grand");
    let leaf = r#"
nika: leaf
permits: { exec: ["echo"] }
tasks:
  speak:
    exec: { command: ["echo", "leaf-v1"] }
outputs:
  word: { value: "${{ tasks.speak.output }}", type: string }
"#;
    let mid = r#"
nika: mid
permits: { exec: ["echo"] }
tasks:
  descend:
    invoke: { workflow: "./leaf.nika.yaml" }
    returns: { object: { word: string } }
outputs:
  relayed: { value: "${{ tasks.descend.output.word }}", type: string }
"#;
    let parent = r#"
nika: root
permits: { exec: ["echo"] }
tasks:
  call:
    invoke: { workflow: "./mid.nika.yaml" }
    returns: { object: { relayed: string } }
outputs:
  heard: { value: "${{ tasks.call.output.relayed }}", type: string }
"#;
    write_in(&dir, "leaf.nika.yaml", leaf);
    write_in(&dir, "mid.nika.yaml", mid);
    let root = write_in(&dir, "root.nika.yaml", parent);

    let stream = comp_run_json(&dir, &root, &[]);
    let trace = write_in(&dir, "grand.ndjson", &stream);

    // Only the LEAF changes — two files above it, the call must re-run.
    write_in(&dir, "leaf.nika.yaml", &leaf.replace("leaf-v1", "leaf-v2"));

    let resumed = comp_run_json(&dir, &root, &["--resume", trace.to_str().expect("utf8")]);
    assert!(
        events_for(&resumed, "task_cache_hit", "call").is_empty(),
        "a grandchild edit re-keys the whole chain:\n{resumed}"
    );
    let outputs = comp_outputs(&dir, &root, &["--resume", trace.to_str().expect("utf8")]);
    assert!(
        outputs.contains("leaf-v2"),
        "the resumed run reflects the grandchild edit: {outputs}"
    );
}

/// Condition 8, the torn half: the first run's trace is CUT before the
/// call completed (a kill mid-composition). The resume skips the
/// completed upstream and re-runs the child WHOLE — the honest coarse
/// tier (within-child granularity is W6's, stated, never faked).
#[test]
fn a_composition_torn_mid_child_reruns_the_child_whole() {
    let dir = comp_dir("comp-torn");
    write_in(&dir, "child.nika.yaml", COMP_CHILD);
    let parent = write_in(&dir, "parent.nika.yaml", COMP_PARENT);

    let stream = comp_run_json(&dir, &parent, &[]);
    let mut kept = Vec::new();
    for line in stream.lines() {
        kept.push(line);
        if line.contains("\"kind\":\"task_completed\"") && line.contains("\"before\"") {
            break;
        }
    }
    let trace = write_in(&dir, "torn.ndjson", &format!("{}\n", kept.join("\n")));

    let resumed = comp_run_json(&dir, &parent, &["--resume", trace.to_str().expect("utf8")]);
    assert!(
        !events_for(&resumed, "task_cache_hit", "before").is_empty(),
        "the completed upstream skips:\n{resumed}"
    );
    assert!(
        !events_for(&resumed, "task_started", "call").is_empty(),
        "the interrupted call re-runs (the child runs whole):\n{resumed}"
    );
    assert!(has_kind(&resumed, "workflow_completed"), "{resumed}");
}

// ─── (d) the cross-version judgment (F-P21 · NEP-0014 law 4) ───────────

/// Re-stamp every line's `chain` field after a content edit: the
/// doctored journal stays chain-VALID — a foreign engine's honest
/// artifact, NOT a forgery. Without it the edit would trip the chain
/// precondition (the trust amendment's whole point) and the VERSION
/// judgment these tests exercise would never be reached.
fn rechain(raw: &str) -> String {
    use sha2::{Digest as _, Sha256};
    let mut prev = format!("{:x}", Sha256::digest(b"nika-trace-v1"));
    let mut out = String::new();
    for line in raw.lines().filter(|l| !l.trim().is_empty()) {
        let mut v: serde_json::Value = serde_json::from_str(line).expect("a JSON line");
        v["chain"] = serde_json::Value::String(prev.clone());
        let stamped = serde_json::to_string(&v).expect("re-serializes");
        prev = format!("{:x}", Sha256::digest(stamped.as_bytes()));
        out.push_str(&stamped);
        out.push('\n');
    }
    out
}

/// Rewrite the trace's recorded `engine_version` (the `workflow_started`
/// line) — the exact artifact a DIFFERENT engine's journal is.
fn trace_with_engine_version(trace: &str, version: &str) -> String {
    let started = trace
        .lines()
        .find(|l| l.contains("\"kind\":\"workflow_started\""))
        .expect("a started frame");
    let recorded = field_str(started, "engine_version");
    let doctored = trace.replace(
        &format!("\"key\":\"engine_version\",\"value\":\"{recorded}\""),
        &format!("\"key\":\"engine_version\",\"value\":\"{version}\""),
    );
    rechain(&doctored)
}

/// F-P21 negative — a resume under a DIFFERENT engine refuses, naming
/// both versions and the exact `--resume-compat` teaching; a WRONG
/// compat token is its own named refusal (never a blanket force).
#[test]
fn a_cross_version_resume_refuses_naming_both_versions() {
    let wf = fixture("fork.nika.yaml", FORK);
    let run = bin()
        .args(["run", &wf.to_string_lossy(), "--json", "--color", "never"])
        .output()
        .expect("binary runs");
    assert_eq!(run.status.code(), Some(0));
    let stream = String::from_utf8(run.stdout).expect("utf8");
    let started = stream
        .lines()
        .find(|l| l.contains("\"kind\":\"workflow_started\""))
        .expect("a started frame");
    let current = field_str(started, "engine_version");
    let older = trace_with_engine_version(&stream, "0.0.0-test");
    assert!(older.contains("0.0.0-test"), "the version is doctored");
    let trace = write_trace("older-engine.ndjson", &older);

    // Undeclared → the refusal names BOTH versions + the teaching.
    let refused = bin()
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
    let stderr = String::from_utf8(refused.stderr).expect("utf8");
    assert_eq!(refused.status.code(), Some(3), "the ENV refusal: {stderr}");
    assert!(
        stderr.contains("0.0.0-test"),
        "the recorded version: {stderr}"
    );
    assert!(stderr.contains(&current), "this engine's version: {stderr}");
    assert!(
        stderr.contains("--resume-compat 0.0.0-test"),
        "the exact teaching: {stderr}"
    );
    assert!(
        refused.stdout.is_empty(),
        "a judged resume never starts:\n{}",
        String::from_utf8_lossy(&refused.stdout)
    );

    // A WRONG token is its own named refusal.
    let wrong = bin()
        .args([
            "run",
            &wf.to_string_lossy(),
            "--resume",
            &trace.to_string_lossy(),
            "--resume-compat",
            "0.0.0-wrong",
            "--json",
            "--color",
            "never",
        ])
        .output()
        .expect("binary runs");
    let stderr = String::from_utf8(wrong.stderr).expect("utf8");
    assert_eq!(
        wrong.status.code(),
        Some(3),
        "a wrong token refuses: {stderr}"
    );
    assert!(
        stderr.contains("0.0.0-wrong") && stderr.contains("0.0.0-test"),
        "both named: {stderr}"
    );
}

/// F-P21 positive — the declared compat discharges the crossing, the
/// run resumes, and the NEW run's boot manifest ATTESTS it
/// (`resumed_from_engine` + `resume_compat: declared`).
#[test]
fn a_declared_compat_resumes_and_attests_the_crossing() {
    let wf = fixture("fork.nika.yaml", FORK);
    let run = bin()
        .args(["run", &wf.to_string_lossy(), "--json", "--color", "never"])
        .output()
        .expect("binary runs");
    assert_eq!(run.status.code(), Some(0));
    let older =
        trace_with_engine_version(&String::from_utf8(run.stdout).expect("utf8"), "0.0.0-test");
    let trace = write_trace("compat-engine.ndjson", &older);

    let resumed = bin()
        .args([
            "run",
            &wf.to_string_lossy(),
            "--resume",
            &trace.to_string_lossy(),
            "--resume-compat",
            "0.0.0-test",
            "--json",
            "--color",
            "never",
        ])
        .output()
        .expect("binary runs");
    let stdout = String::from_utf8(resumed.stdout).expect("utf8");
    let stderr = String::from_utf8(resumed.stderr).expect("utf8");
    assert_eq!(
        resumed.status.code(),
        Some(0),
        "the declared compat proceeds: {stderr}"
    );
    assert!(
        stderr.contains("cross-version compat declared"),
        "the crossing is said: {stderr}"
    );
    let new_started = stdout
        .lines()
        .find(|l| l.contains("\"kind\":\"workflow_started\""))
        .expect("the new run's started frame");
    assert_eq!(field_str(new_started, "resumed_from_engine"), "0.0.0-test");
    assert_eq!(field_str(new_started, "resume_compat"), "declared");
    assert!(
        has_kind(&stdout, "workflow_completed"),
        "the resume completes:\n{stdout}"
    );
}

/// F-P21 — a pre-versioning trace (no `engine_version`) refuses with
/// the `unrecorded` token; declared, it proceeds (the same law).
#[test]
fn a_versionless_trace_is_judged_with_the_unrecorded_token() {
    let wf = fixture("fork.nika.yaml", FORK);
    let run = bin()
        .args(["run", &wf.to_string_lossy(), "--json", "--color", "never"])
        .output()
        .expect("binary runs");
    assert_eq!(run.status.code(), Some(0));
    let stream = String::from_utf8(run.stdout).expect("utf8");
    // Strip the engine_version field wholesale — a pre-A5 journal's shape.
    let started = stream
        .lines()
        .find(|l| l.contains("\"kind\":\"workflow_started\""))
        .expect("a started frame");
    let recorded = field_str(started, "engine_version");
    let stripped = stream.replace(
        &format!("{{\"key\":\"engine_version\",\"value\":\"{recorded}\"}},"),
        "",
    );
    assert!(
        !stripped.contains("engine_version"),
        "the field is gone from every line"
    );
    // Chain-valid (a pre-A5 journal is an honest artifact), so the
    // trust precondition passes and the VERSION judgment speaks.
    let trace = write_trace("versionless.ndjson", &rechain(&stripped));

    let refused = bin()
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
    let stderr = String::from_utf8(refused.stderr).expect("utf8");
    assert_eq!(
        refused.status.code(),
        Some(3),
        "judged, never assumed: {stderr}"
    );
    assert!(
        stderr.contains("records no engine version"),
        "the unrecorded class: {stderr}"
    );
    assert!(
        stderr.contains("--resume-compat unrecorded"),
        "the token teaching: {stderr}"
    );

    let declared = bin()
        .args([
            "run",
            &wf.to_string_lossy(),
            "--resume",
            &trace.to_string_lossy(),
            "--resume-compat",
            "unrecorded",
            "--json",
            "--color",
            "never",
        ])
        .output()
        .expect("binary runs");
    assert_eq!(
        declared.status.code(),
        Some(0),
        "the unrecorded compat proceeds: {}",
        String::from_utf8_lossy(&declared.stderr)
    );
}

// ─── (d) the chain-trust precondition (ADR-099 trust amendment) ──────

/// Stage the laundering setup: a unique run dir with the CHAIN workflow
/// and its REAL journal (`.nika/traces/` — the chained artifact `trace
/// verify` and the resume precondition both walk; the `--json` stream
/// carries the same chain since the ADR-099 §5 follow-on — either
/// lane's journal serves), plus the same journal with ONE recorded byte
/// flipped (length unchanged — the report's forgery): `a`'s completion
/// now claims "bravo", and the NEXT line's chain breaks.
fn staged_tampered_trace(
    name: &str,
) -> (std::path::PathBuf, std::path::PathBuf, std::path::PathBuf) {
    let dir =
        std::env::temp_dir().join(format!("nika-resume-launder-{name}-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("run dir");
    let wf = dir.join("chain.nika.yaml");
    std::fs::write(&wf, CHAIN).expect("workflow");
    let run = bin()
        .args(["run", "chain.nika.yaml"])
        .current_dir(&dir)
        .output()
        .expect("binary runs");
    assert_eq!(
        run.status.code(),
        Some(0),
        "the honest run: {}",
        String::from_utf8_lossy(&run.stderr)
    );
    let journal = std::fs::read_dir(dir.join(".nika").join("traces"))
        .expect("trace dir")
        .map(|e| e.expect("entry").path())
        .next()
        .expect("one journal");
    let raw = std::fs::read_to_string(&journal).expect("journal reads");
    let mut lines: Vec<String> = raw.lines().map(str::to_owned).collect();
    let idx = lines
        .iter()
        .position(|l| l.contains("\"kind\":\"task_completed\"") && l.contains("\"a\""))
        .expect("`a` completed");
    lines[idx] = lines[idx].replacen("alpha", "bravo", 1);
    let tampered = dir.join("tampered.ndjson");
    std::fs::write(&tampered, lines.join("\n") + "\n").expect("tampered written");
    (wf, journal, tampered)
}

/// Default: the forgery is REFUSED before the fold (exit 2 · the FILE
/// class, one voice with `trace verify`) — the run never starts, the
/// refusal names the finding and the opt-out. Measured 2026-08-07 on
/// 0.108.0: the same journal resumed silently, exit 0.
#[test]
fn a_tampered_trace_is_refused_naming_the_opt_out() {
    let (wf, _journal, tampered) = staged_tampered_trace("refuse");

    // The detector still works (the conjunction's first role was never
    // broken — the report's step 2).
    let verified = bin()
        .args(["trace", "verify", &tampered.to_string_lossy()])
        .output()
        .expect("binary runs");
    assert_eq!(verified.status.code(), Some(2), "the forgery is detected");

    let journals_before = trace_count(&wf);
    let refused = bin()
        .args([
            "run",
            &wf.to_string_lossy(),
            "--resume",
            &tampered.to_string_lossy(),
            "--json",
            "--color",
            "never",
        ])
        .output()
        .expect("binary runs");
    let stderr = String::from_utf8(refused.stderr).expect("utf8");
    assert_eq!(
        refused.status.code(),
        Some(2),
        "the tamper class is the FILE class: {stderr}"
    );
    assert!(
        stderr.contains("chain BROKEN at line"),
        "the finding: {stderr}"
    );
    assert!(
        stderr.contains("--resume-unverified"),
        "the opt-out is named: {stderr}"
    );
    assert!(
        refused.stdout.is_empty(),
        "a refused resume never starts:\n{}",
        String::from_utf8_lossy(&refused.stdout)
    );
    assert_eq!(
        trace_count(&wf),
        journals_before,
        "no new journal descends from a broken ancestor (the laundering plane)"
    );
}

/// The journals a run dir holds (`.nika/traces/*.ndjson`) — the
/// laundering plane: a refused resume must leave the count untouched.
fn trace_count(wf: &std::path::Path) -> usize {
    std::fs::read_dir(wf.parent().expect("a dir").join(".nika").join("traces"))
        .expect("trace dir")
        .count()
}

/// The STRIP attack: tamper, then delete every `chain` field — the
/// walker's `Broken` (refusal) becomes `Unchained` (the chainless
/// compat, indistinguishable-by-shape from an honest `--json` capture).
/// The compat proceeds — and the NEW journal ATTESTS the unverified
/// ancestry (`unchained`, never `declared`: no opt-out was named), so
/// one resume cannot launder a forged journal into a clean one silently.
#[test]
fn a_stripped_trace_proceeds_but_attests_the_unchained_trust() {
    let (wf, _journal, tampered) = staged_tampered_trace("strip");
    let raw = std::fs::read_to_string(&tampered).expect("tampered reads");
    let stripped = raw
        .lines()
        .map(|line| {
            let mut v: serde_json::Value = serde_json::from_str(line).expect("one JSON event");
            v.as_object_mut().expect("an object").remove("chain");
            serde_json::to_string(&v).expect("re-serialized")
        })
        .collect::<Vec<_>>()
        .join("\n");
    let stripped_path = wf.with_file_name("stripped.ndjson");
    std::fs::write(&stripped_path, format!("{stripped}\n")).expect("stripped written");

    // The detector now reads the compat class, not the tamper class —
    // the strip is exactly why the attestation below must exist.
    let verified = bin()
        .args(["trace", "verify", &stripped_path.to_string_lossy()])
        .output()
        .expect("binary runs");
    assert_eq!(
        verified.status.code(),
        Some(3),
        "a stripped journal reads unchained (the ENV class)"
    );

    let resumed = bin()
        .args([
            "run",
            &wf.to_string_lossy(),
            "--resume",
            &stripped_path.to_string_lossy(),
            "--json",
            "--color",
            "never",
        ])
        .output()
        .expect("binary runs");
    let stdout = String::from_utf8(resumed.stdout).expect("utf8");
    let stderr = String::from_utf8(resumed.stderr).expect("utf8");
    assert_eq!(
        resumed.status.code(),
        Some(0),
        "the chainless compat proceeds: {stderr}"
    );
    assert!(
        stderr.contains("trusted WITHOUT verification"),
        "said on stderr: {stderr}"
    );
    let started = stdout
        .lines()
        .find(|l| l.contains("\"kind\":\"workflow_started\""))
        .expect("the new run's started frame");
    assert_eq!(
        field_str(started, "resume_unverified"),
        "unchained",
        "attested as the chainless compat — never `declared` (no opt-out was named): {started}"
    );
    assert!(
        field_str(started, "resume_unverified_finding").contains("no tamper-evidence chain"),
        "the reason rides the new journal: {started}"
    );
}

/// The NAMED opt-out: the run proceeds LOUDLY, and the NEW journal
/// attests the finding on its boot manifest — a laundered trace can
/// never claim a clean ancestry silently. The control arm: the SAME flag
/// over the INTACT journal journals no claim (the journal says what
/// HAPPENED — never a flag echo).
#[test]
fn the_named_opt_out_proceeds_loudly_and_attests() {
    let (wf, journal, tampered) = staged_tampered_trace("optout");
    let opted = bin()
        .args([
            "run",
            &wf.to_string_lossy(),
            "--resume",
            &tampered.to_string_lossy(),
            "--resume-unverified",
            "--json",
            "--color",
            "never",
        ])
        .output()
        .expect("binary runs");
    let stdout = String::from_utf8(opted.stdout).expect("utf8");
    let stderr = String::from_utf8(opted.stderr).expect("utf8");
    assert_eq!(
        opted.status.code(),
        Some(0),
        "the named opt-out proceeds: {stderr}"
    );
    assert!(
        stderr.contains("proceeding under --resume-unverified"),
        "said out loud: {stderr}"
    );
    let new_started = stdout
        .lines()
        .find(|l| l.contains("\"kind\":\"workflow_started\""))
        .expect("the new run's started frame");
    assert_eq!(field_str(new_started, "resume_unverified"), "declared");
    assert!(
        field_str(new_started, "resume_unverified_finding").contains("chain BROKEN at line"),
        "the finding rides the new journal: {new_started}"
    );

    let control = bin()
        .args([
            "run",
            &wf.to_string_lossy(),
            "--resume",
            &journal.to_string_lossy(),
            "--resume-unverified",
            "--json",
            "--color",
            "never",
        ])
        .output()
        .expect("binary runs");
    let stdout = String::from_utf8(control.stdout).expect("utf8");
    assert_eq!(
        control.status.code(),
        Some(0),
        "the intact control resumes: {}",
        String::from_utf8_lossy(&control.stderr)
    );
    let started = stdout
        .lines()
        .find(|l| l.contains("\"kind\":\"workflow_started\""))
        .expect("the control run's started frame");
    assert!(
        !started.contains("resume_unverified"),
        "a verified trace journals no claim: {started}"
    );
}

/// The class RETIRED (ADR-099 §5 follow-on): the `--json` stdout stream
/// carries the chain since the sink learned it — a capture resumes
/// VERIFIED (no notice · no attestation), and a capture with one
/// recorded byte flipped is REFUSED like any broken journal. The
/// Unchained compat shrinks to pre-chain captures and stripped
/// forgeries.
#[test]
fn a_json_capture_resumes_verified_and_its_forgery_is_refused() {
    let dir = std::env::temp_dir().join(format!("nika-resume-capture-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("run dir");
    std::fs::write(dir.join("chain.nika.yaml"), CHAIN).expect("workflow");
    let run = bin()
        .args(["run", "chain.nika.yaml", "--json", "--color", "never"])
        .current_dir(&dir)
        .output()
        .expect("binary runs");
    assert_eq!(
        run.status.code(),
        Some(0),
        "the honest run: {}",
        String::from_utf8_lossy(&run.stderr)
    );
    let raw = String::from_utf8(run.stdout).expect("utf8");
    assert!(
        raw.lines().all(|l| l.contains("\"chain\":\"")),
        "every streamed line carries the chain"
    );
    let capture = dir.join("capture.ndjson");
    std::fs::write(&capture, &raw).expect("capture written");

    // The capture VERIFIES — the stream is a first-class journal now.
    let verified = bin()
        .args(["trace", "verify", &capture.to_string_lossy()])
        .output()
        .expect("binary runs");
    assert_eq!(
        verified.status.code(),
        Some(0),
        "the capture's chain is intact: {}",
        String::from_utf8_lossy(&verified.stderr)
    );

    // The resume from it rides the VERIFIED lane — never the chainless
    // compat (no notice on stderr · no attestation on the boot frame).
    let resumed = bin()
        .args([
            "run",
            &dir.join("chain.nika.yaml").to_string_lossy(),
            "--resume",
            &capture.to_string_lossy(),
            "--json",
            "--color",
            "never",
        ])
        .output()
        .expect("binary runs");
    let stdout = String::from_utf8(resumed.stdout).expect("utf8");
    let stderr = String::from_utf8(resumed.stderr).expect("utf8");
    assert_eq!(resumed.status.code(), Some(0), "the resume: {stderr}");
    assert!(
        !stderr.contains("trusted WITHOUT verification"),
        "the chainless notice is gone: {stderr}"
    );
    let started = stdout
        .lines()
        .find(|l| l.contains("\"kind\":\"workflow_started\""))
        .expect("the resumed run's started frame");
    assert!(
        !started.contains("resume_unverified"),
        "a verified capture journals no claim: {started}"
    );

    // And its forgery is REFUSED — the stream is no longer a blind spot.
    let mut lines: Vec<String> = raw.lines().map(str::to_owned).collect();
    let idx = lines
        .iter()
        .position(|l| l.contains("\"kind\":\"task_completed\"") && l.contains("\"a\""))
        .expect("`a` completed");
    lines[idx] = lines[idx].replacen("alpha", "bravo", 1);
    let forged = dir.join("forged-capture.ndjson");
    std::fs::write(&forged, lines.join("\n") + "\n").expect("forged written");
    let refused = bin()
        .args([
            "run",
            &dir.join("chain.nika.yaml").to_string_lossy(),
            "--resume",
            &forged.to_string_lossy(),
            "--json",
            "--color",
            "never",
        ])
        .output()
        .expect("binary runs");
    let stderr = String::from_utf8(refused.stderr).expect("utf8");
    assert_eq!(
        refused.status.code(),
        Some(2),
        "the forged capture is refused: {stderr}"
    );
    assert!(
        stderr.contains("chain BROKEN at line"),
        "the finding: {stderr}"
    );
}
