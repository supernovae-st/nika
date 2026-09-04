// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>
#![allow(clippy::expect_used, clippy::panic)]
// This suite executes the real `nika-cli` binary (CARGO_BIN_EXE) — its
// whole job is the binary contract, so it spawns processes (the same
// sanctioned carve-out as bin_smoke.rs / run_verb.rs).
#![allow(clippy::disallowed_types)]

//! NEP-0007 law 2 on the BUILTIN arm — the declared v1 residual, closed:
//! the in-process fs boundary's enforcement decisions must ride the run
//! journal as `permit_checked` frames (`plane: "fs"`), granted AND
//! refused alike, not only as the task's coded failure. An auditor
//! reconstructs WHAT authority the builtin arm exercised; the hash
//! chain binds each decision to the task that took it (the frame lands
//! between `task_started` and the terminal frame).

use std::process::Command;

fn bin() -> Command {
    Command::new(env!("CARGO_BIN_EXE_nika"))
}

/// One permitted read + one refused read, in two workflows.
const WF_ALLOW: &str = r#"
nika: witness-allow

permits:
  fs:
    read: ["./allowed.txt"]
  tools: ["nika:read"]

tasks:
  peek:
    invoke:
      tool: "nika:read"
      args:
        path: "./allowed.txt"
"#;

/// The deny arm MUST reach the builtin boundary dynamically: a literal
/// out-of-boundary path is the AUDIT's refusal (the check gate, before
/// any task — never the builtin's verdict), so the probe computes the
/// path at run time (a jq output the static analysis honestly calls
/// opaque — « computed paths + symlinks are the RUN's verdict »).
const WF_DENY: &str = r#"
nika: witness-deny

permits:
  fs:
    read: ["./allowed.txt"]
  tools: ["nika:read", "nika:jq"]

tasks:
  make:
    invoke:
      tool: "nika:jq"
      args:
        input: {}
        expression: '"./secret.txt"'

  peek:
    with:
      p: ${{ tasks.make.output }}
    invoke:
      tool: "nika:read"
      args:
        path: ${{ with.p }}
"#;

/// The journal's frames for one probe run (stdout is the NDJSON lane).
fn frames(stdout: &str) -> Vec<serde_json::Value> {
    stdout
        .lines()
        .filter_map(|l| serde_json::from_str(l).ok())
        .collect()
}

/// The frame positions of one kind, in journal order.
fn positions(frames: &[serde_json::Value], kind: &str) -> Vec<usize> {
    frames
        .iter()
        .enumerate()
        .filter(|(_, f)| f["kind"] == kind)
        .map(|(i, _)| i)
        .collect()
}

/// One payload value of a frame (the wire shape: `fields` is a
/// key/value pair array, not a flat object).
fn field<'a>(frame: &'a serde_json::Value, name: &str) -> Option<&'a str> {
    frame["fields"]
        .as_array()?
        .iter()
        .find(|kv| kv["key"] == name)?["value"]
        .as_str()
}

/// The fs-plane `permit_checked` frames of a run.
fn fs_witnesses(frames: &[serde_json::Value]) -> Vec<&serde_json::Value> {
    frames
        .iter()
        .filter(|f| f["kind"] == "permit_checked" && field(f, "plane") == Some("fs"))
        .collect()
}

#[test]
fn a_permitted_builtin_read_journals_an_allow_witness() {
    let dir = std::env::temp_dir().join(format!("nika-witness-allow-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("scratch");
    std::fs::write(dir.join("wf.nika.yaml"), WF_ALLOW).expect("workflow");
    std::fs::write(dir.join("allowed.txt"), "IN-BOUNDS\n").expect("honest file");

    let run = bin()
        .args(["run", "wf.nika.yaml", "--json", "--color", "never"])
        .current_dir(&dir)
        .output()
        .expect("binary runs");
    let stdout = String::from_utf8(run.stdout).expect("utf8");
    assert_eq!(
        run.status.code(),
        Some(0),
        "the permitted run: {}",
        String::from_utf8_lossy(&run.stderr)
    );
    let frames = frames(&stdout);
    let witnesses = fs_witnesses(&frames);
    assert_eq!(
        witnesses.len(),
        1,
        "exactly one fs decision witnessed: {stdout}"
    );
    let w = witnesses[0];
    assert_eq!(
        field(w, "decision"),
        Some("allow"),
        "the granted read is witnessed"
    );
    assert!(
        field(w, "gate").expect("a gate").contains("./allowed.txt"),
        "the gate names the judged path: {w}"
    );
    assert!(field(w, "why").is_some(), "the law one-liner rides: {w}");
    // The chain binds the decision to the task that took it: the frame
    // lands between task_started and task_completed.
    let started = positions(&frames, "task_started");
    let completed = positions(&frames, "task_completed");
    let wpos = frames
        .iter()
        .position(|f| std::ptr::eq(f, w))
        .expect("the witness is a frame");
    assert!(
        started.iter().any(|s| *s < wpos) && completed.iter().any(|c| *c > wpos),
        "between task_started and task_completed: {stdout}"
    );
}

#[test]
fn a_refused_builtin_read_journals_a_deny_witness_before_the_failure() {
    let dir = std::env::temp_dir().join(format!("nika-witness-deny-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("scratch");
    std::fs::write(dir.join("wf.nika.yaml"), WF_DENY).expect("workflow");
    std::fs::write(dir.join("allowed.txt"), "IN-BOUNDS\n").expect("honest file");
    std::fs::write(dir.join("secret.txt"), "THE-SECRET\n").expect("secret");

    let run = bin()
        .args(["run", "wf.nika.yaml", "--json", "--color", "never"])
        .current_dir(&dir)
        .output()
        .expect("binary runs");
    let stdout = String::from_utf8(run.stdout).expect("utf8");
    assert_ne!(
        run.status.code(),
        Some(0),
        "the refused read fails the task"
    );
    assert!(
        !stdout.contains("THE-SECRET"),
        "zero secret bytes ride the run: {stdout}"
    );
    let frames = frames(&stdout);
    let witnesses = fs_witnesses(&frames);
    assert_eq!(
        witnesses.len(),
        1,
        "exactly one fs decision witnessed: {stdout}"
    );
    let w = witnesses[0];
    assert_eq!(
        field(w, "decision"),
        Some("deny"),
        "the refusal is witnessed"
    );
    // The deny precedes the terminal failure frame — the refusal is
    // attested as a witnessed decision, not only as the coded failure.
    let wpos = frames
        .iter()
        .position(|f| std::ptr::eq(f, w))
        .expect("the witness is a frame");
    let failed = positions(&frames, "task_failed");
    assert!(
        failed.iter().any(|f| *f > wpos),
        "the deny lands before task_failed: {stdout}"
    );
}
