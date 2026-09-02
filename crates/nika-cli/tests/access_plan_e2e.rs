// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>
#![allow(clippy::expect_used, clippy::panic)]
// The workspace bans std::process::Command (production spawns ride the
// kernel ShellExecutor seam). This test's WHOLE JOB is to execute the
// real `nika-cli` binary (CARGO_BIN_EXE) — the same carve-out class as
// bin_smoke.rs: the contract under test IS the binary's behavior.
#![allow(clippy::disallowed_types)]
#![cfg(unix)]

//! One Door · wave 1 · the non-divergence pin.
//!
//! The access path a run ANNOUNCES is the access path the run RIDES,
//! because `check`, the announce, the boot manifest and the dispatcher
//! all read ONE frozen plan resolved once per execution attempt. Before
//! this arc the same question was answered five times on the run path
//! (census B on the shipped 0.116.2): the announce said `codex`, the
//! runtime seated only under a typed `--access`, and the task dialed
//! the API with whatever key was lying around.
//!
//! The rig: a scripted `codex` on PATH (the subscription seat · presence
//! is the admission fact) beside a DEAD provider key pointed at an
//! unreachable loopback endpoint (the API path). Which path served is
//! then unambiguous: the seat answers `seated-answer`; the API refuses
//! the connection.

use std::io::Write as _;
use std::os::unix::fs::PermissionsExt;
use std::process::Command;

const WORKFLOW: &str = "nika: one-door-lane
model: openai/gpt-5.2
tasks:
  answer:
    infer:
      prompt: classify this
      max_tokens: 256
outputs:
  answer: ${{ tasks.answer.output }}
";

/// A `codex exec --json` that answers one turn and never reads a key.
const FAKE_CODEX: &str = r#"#!/bin/sh
set -eu
IFS= read -r _prompt || true
printf '%s\n' '{"type":"thread.started","thread_id":"t"}'
printf '%s\n' '{"type":"turn.started"}'
printf '%s\n' '{"type":"item.completed","item":{"id":"i","type":"agent_message","text":"seated-answer"}}'
printf '%s\n' '{"type":"turn.completed","usage":{"input_tokens":3,"output_tokens":2}}'
"#;

struct Rig {
    root: std::path::PathBuf,
}

impl Rig {
    /// `bin/` (the fakes) · `home/` (empty · no auth files) · `work/`
    /// (the project root with the workflow).
    fn new(name: &str, with_codex: bool) -> Self {
        let root =
            std::env::temp_dir().join(format!("nika-one-door-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        for sub in ["bin", "home", "work"] {
            std::fs::create_dir_all(root.join(sub)).expect("rig dir");
        }
        if with_codex {
            for bin in ["codex", "codex-acp"] {
                let path = root.join("bin").join(bin);
                let mut f = std::fs::File::create(&path).expect("fake bin");
                f.write_all(FAKE_CODEX.as_bytes()).expect("fake body");
                let mut perm = std::fs::metadata(&path).expect("meta").permissions();
                perm.set_mode(0o755);
                std::fs::set_permissions(&path, perm).expect("chmod");
            }
        }
        std::fs::write(root.join("work").join("lane.nika.yaml"), WORKFLOW).expect("workflow");
        Self { root }
    }

    /// The binary on a fresh machine: cleared env, our PATH first, a
    /// scratch HOME, the project as cwd. `dead_key` plants a provider key
    /// that no server will ever accept, aimed at a closed loopback port.
    fn nika(&self, args: &[&str], dead_key: bool) -> std::process::Output {
        let path = format!("{}:/usr/bin:/bin", self.root.join("bin").display());
        let mut cmd = Command::new(env!("CARGO_BIN_EXE_nika-cli"));
        cmd.args(args)
            .env_clear()
            .env("PATH", path)
            .env("HOME", self.root.join("home"))
            .env("TERM", "dumb")
            .current_dir(self.root.join("work"));
        if dead_key {
            cmd.env("OPENAI_API_KEY", "sk-dead-key-never-accepted")
                .env("NIKA_OPENAI_BASE_URL", "http://127.0.0.1:9/v1");
        }
        cmd.output().expect("binary runs")
    }
}

fn text(bytes: &[u8]) -> String {
    String::from_utf8_lossy(bytes).into_owned()
}

/// The `fields` of the first frame of `kind`, as (key, value) pairs.
fn frame_fields(stdout: &str, kind: &str) -> Vec<(String, serde_json::Value)> {
    let needle = format!("\"kind\":\"{kind}\"");
    let line = stdout
        .lines()
        .find(|l| l.contains(&needle))
        .unwrap_or_else(|| panic!("no {kind} frame in:\n{stdout}"));
    let frame: serde_json::Value = serde_json::from_str(line).expect("one JSON frame");
    frame["fields"]
        .as_array()
        .expect("fields array")
        .iter()
        .map(|f| {
            (
                f["key"].as_str().expect("field key").to_owned(),
                f["value"].clone(),
            )
        })
        .collect()
}

fn field<'a>(
    fields: &'a [(String, serde_json::Value)],
    key: &str,
) -> Option<&'a serde_json::Value> {
    fields.iter().find(|(k, _)| k == key).map(|(_, v)| v)
}

/// The plan admits the seat (sovereign order: harness before api), so
/// the run RIDES the seat — the dead key is never dialed, the task
/// terminal stamps the lane that served (`access_id: codex`).
#[test]
fn the_run_rides_the_seat_the_plan_admitted() {
    let rig = Rig::new("seat", true);
    let out = rig.nika(
        &["run", "lane.nika.yaml", "--json", "--max-cost-usd", "1"],
        true,
    );
    let stdout = text(&out.stdout);
    let stderr = text(&out.stderr);
    assert_eq!(
        out.status.code(),
        Some(0),
        "the seated run completes\nstdout: {stdout}\nstderr: {stderr}"
    );
    let fields = frame_fields(&stdout, "task_completed");
    assert_eq!(
        field(&fields, "access_id").and_then(serde_json::Value::as_str),
        Some("codex"),
        "the terminal names the seat that served: {fields:?}"
    );
    assert_eq!(
        field(&fields, "access").and_then(serde_json::Value::as_str),
        Some("harness"),
        "{fields:?}"
    );
    assert_eq!(
        field(&fields, "provider").and_then(serde_json::Value::as_str),
        Some("openai"),
        "{fields:?}"
    );
    assert!(
        !stdout.contains("127.0.0.1:9") && !stderr.contains("127.0.0.1:9"),
        "the API endpoint was never dialed\nstdout: {stdout}\nstderr: {stderr}"
    );
}

/// The human announce and the executed path come from the same plan:
/// what stderr says before the first frame is what the trace records.
#[test]
fn the_announce_names_the_path_the_run_takes() {
    let rig = Rig::new("announce", true);
    let out = rig.nika(&["run", "lane.nika.yaml", "--max-cost-usd", "1"], true);
    let stderr = text(&out.stderr);
    assert_eq!(
        out.status.code(),
        Some(0),
        "the seated run completes\nstdout: {}\nstderr: {stderr}",
        text(&out.stdout)
    );
    let announce = stderr
        .lines()
        .find(|l| l.starts_with("access: openai/gpt-5.2 →"))
        .unwrap_or_else(|| panic!("the rich announce is spoken once:\n{stderr}"));
    assert!(
        announce.contains("codex") && announce.contains("harness"),
        "the announce names the seat the run rode: {announce}"
    );
}

/// `--access api` pins the API path: the seat is present and never
/// borrowed, the dead key is dialed and refused, the run fails honestly.
#[test]
fn a_pinned_api_path_never_borrows_the_seat() {
    let rig = Rig::new("pin-api", true);
    let out = rig.nika(
        &[
            "run",
            "lane.nika.yaml",
            "--access",
            "api",
            "--max-cost-usd",
            "1",
        ],
        true,
    );
    let stdout = text(&out.stdout);
    let stderr = text(&out.stderr);
    assert_ne!(
        out.status.code(),
        Some(0),
        "the API path cannot succeed against a closed port\nstdout: {stdout}\nstderr: {stderr}"
    );
    assert!(
        stderr.contains("access: pinned `api`"),
        "the pin is announced: {stderr}"
    );
    assert!(
        !stdout.contains("seated-answer") && !stderr.contains("seated-answer"),
        "the seat never served a pinned API run\nstdout: {stdout}\nstderr: {stderr}"
    );
}

/// `--model mock/echo` swaps the run's model BEFORE the plan resolves:
/// the announce speaks of the run, never of the file's model (the
/// shipped door announced `codex` for a mock run · census B).
#[test]
fn the_mock_override_leaves_the_seat_unannounced() {
    let rig = Rig::new("mock", true);
    let out = rig.nika(
        &[
            "run",
            "lane.nika.yaml",
            "--model",
            "mock/echo",
            "--max-cost-usd",
            "1",
        ],
        true,
    );
    let stderr = text(&out.stderr);
    assert_eq!(
        out.status.code(),
        Some(0),
        "the mock run completes\nstdout: {}\nstderr: {stderr}",
        text(&out.stdout)
    );
    assert!(
        !stderr.contains("access:"),
        "a one-candidate mock lane has nothing to announce: {stderr}"
    );
}

/// `check --json` reads the same plan: its `access_plan` row names the
/// seat the run will ride — the machine surface cannot disagree with
/// the run either.
#[test]
fn check_names_the_path_the_run_takes() {
    let rig = Rig::new("check", true);
    let out = rig.nika(&["check", "lane.nika.yaml", "--json"], true);
    let stdout = text(&out.stdout);
    assert_eq!(out.status.code(), Some(0), "clean: {stdout}");
    let verdict: serde_json::Value = serde_json::from_str(&stdout).expect("check json");
    let row = &verdict["access_plan"][0];
    assert_eq!(row["model"], "openai/gpt-5.2", "{verdict}");
    assert_eq!(row["resolved"], true, "{verdict}");
    assert_eq!(row["access"], "codex", "{verdict}");
    assert_eq!(row["chosen"], "harness", "{verdict}");
}

/// No key, no seat: the plan refuses BEFORE the first task with the
/// witnesses, on the environment exit — never a provider error after a
/// task already started.
#[test]
fn no_path_refuses_before_the_first_task() {
    let rig = Rig::new("no-path", false);
    let out = rig.nika(&["run", "lane.nika.yaml", "--max-cost-usd", "1"], false);
    let stdout = text(&out.stdout);
    let stderr = text(&out.stderr);
    assert_eq!(
        out.status.code(),
        Some(3),
        "an environment refusal, not a run failure\nstdout: {stdout}\nstderr: {stderr}"
    );
    let all = format!("{stdout}\n{stderr}");
    assert!(
        all.contains("no access path is ready for `openai/gpt-5.2`"),
        "the refusal names the model with no path: {all}"
    );
    assert!(
        !all.contains("task_started") && !all.contains("answer ·"),
        "nothing ran: {all}"
    );
}
