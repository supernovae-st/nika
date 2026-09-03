// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>
#![allow(clippy::expect_used, clippy::panic)]
// The workspace bans std::process::Command (production spawns ride the
// kernel ShellExecutor seam). This test's WHOLE JOB is to execute the
// real `nika-cli` binary (CARGO_BIN_EXE) — the same carve-out class as
// bin_smoke.rs: the contract under test IS the binary's behavior.
#![allow(clippy::disallowed_types)]
#![cfg(unix)]

//! One Door · wave 2 · the layered verdicts on both doors.
//!
//! `check` answers four questions (VALID · ACCESS READY · CAPACITY FIT ·
//! RUN READY) and `run` refuses what `check` refuses: the MODELS rung's
//! judgments (resolution · thinking · capacity) gate the run before task
//! 1, the ONE lane-row shape rides `check --json`, `run --dry-run --json`
//! and the trace's boot manifest, and `check --access` pins the plan the
//! way `run --access` does. The W1 gauntlet measured the gaps this suite
//! closes: a reasoning seat under a tiny cap was red on `check` and ran
//! on `run`; three JSON shapes described one access decision.

use std::io::Write as _;
use std::os::unix::fs::PermissionsExt;
use std::process::Command;

/// A `codex exec --json` that answers one turn and never reads a key.
const FAKE_CODEX: &str = r#"#!/bin/sh
set -eu
IFS= read -r _prompt || true
printf '%s\n' '{"type":"thread.started","thread_id":"t"}'
printf '%s\n' '{"type":"turn.started"}'
printf '%s\n' '{"type":"item.completed","item":{"id":"i","type":"agent_message","text":"seated-answer"}}'
printf '%s\n' '{"type":"turn.completed","usage":{"input_tokens":3,"output_tokens":2}}'
"#;

fn workflow(max_tokens: u32) -> String {
    format!(
        "nika: layers\nmodel: openai/gpt-5.2\ntasks:\n  answer:\n    infer:\n      prompt: classify this\n      max_tokens: {max_tokens}\n"
    )
}

struct Rig {
    root: std::path::PathBuf,
}

impl Rig {
    fn new(name: &str) -> Self {
        let root =
            std::env::temp_dir().join(format!("nika-one-door-w2-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        for sub in ["bin", "home", "work"] {
            std::fs::create_dir_all(root.join(sub)).expect("rig dir");
        }
        for bin in ["codex", "codex-acp"] {
            let path = root.join("bin").join(bin);
            let mut f = std::fs::File::create(&path).expect("fake bin");
            f.write_all(FAKE_CODEX.as_bytes()).expect("fake body");
            let mut perm = std::fs::metadata(&path).expect("meta").permissions();
            perm.set_mode(0o755);
            std::fs::set_permissions(&path, perm).expect("chmod");
        }
        Self { root }
    }

    fn write(&self, name: &str, body: &str) {
        std::fs::write(self.root.join("work").join(name), body).expect("workflow");
    }

    /// The binary with a dead key aimed at a closed port and the stub
    /// seat on PATH — the seat is the admitted lane, the key is present.
    fn nika(&self, args: &[&str]) -> std::process::Output {
        let path = format!("{}:/usr/bin:/bin", self.root.join("bin").display());
        Command::new(env!("CARGO_BIN_EXE_nika-cli"))
            .args(args)
            .env_clear()
            .env("PATH", path)
            .env("HOME", self.root.join("home"))
            .env("TERM", "dumb")
            .env("OPENAI_API_KEY", "sk-dead-key-never-accepted")
            .env("NIKA_OPENAI_BASE_URL", "http://127.0.0.1:9/v1")
            .current_dir(self.root.join("work"))
            .output()
            .expect("binary runs")
    }
}

fn text(bytes: &[u8]) -> String {
    String::from_utf8_lossy(bytes).into_owned()
}

fn sorted_rows(rows: &serde_json::Value) -> Vec<serde_json::Value> {
    let mut rows: Vec<serde_json::Value> = rows.as_array().cloned().unwrap_or_default();
    rows.sort_by(|a, b| a["model"].as_str().cmp(&b["model"].as_str()));
    rows
}

/// The reasoning-seat floor: red on `check`, and now red on `run` before
/// any task — the same finding text on both doors.
#[test]
fn run_refuses_what_check_refuses_on_the_reasoning_floor() {
    let rig = Rig::new("floor");
    rig.write("floor.nika.yaml", &workflow(32));
    let check = rig.nika(&["check", "floor.nika.yaml"]);
    let check_out = text(&check.stdout);
    assert_eq!(check.status.code(), Some(2), "check refuses: {check_out}");
    assert!(
        check_out.contains("MODELS") && check_out.contains("max_tokens"),
        "{check_out}"
    );
    let run = rig.nika(&["run", "floor.nika.yaml", "--json", "--max-cost-usd", "1"]);
    let stdout = text(&run.stdout);
    let stderr = text(&run.stderr);
    assert_eq!(
        run.status.code(),
        Some(2),
        "the run refuses the same finding\nstdout: {stdout}\nstderr: {stderr}"
    );
    assert!(
        !stdout.contains("\"kind\":\"task_started\"") && !stdout.contains("seated-answer"),
        "nothing ran: {stdout}"
    );
    assert!(
        stderr.contains("max_tokens") || stdout.contains("max_tokens"),
        "the run names the finding\nstdout: {stdout}\nstderr: {stderr}"
    );
}

/// CAPACITY FIT judged on both doors: an output cap above the seat's max
/// output is a finding on `check` and a refusal on `run`.
#[test]
fn capacity_is_judged_on_both_doors() {
    let rig = Rig::new("capacity");
    rig.write("cap.nika.yaml", &workflow(200_000));
    let check = rig.nika(&["check", "cap.nika.yaml", "--json"]);
    let out = text(&check.stdout);
    assert_eq!(check.status.code(), Some(2), "{out}");
    let verdict: serde_json::Value = serde_json::from_str(&out).expect("check json");
    assert_eq!(verdict["verdicts"]["valid"], true, "{verdict}");
    assert_eq!(verdict["verdicts"]["capacity_fit"], false, "{verdict}");
    assert_eq!(verdict["verdicts"]["run_ready"], false, "{verdict}");
    assert_eq!(verdict["clean"], false, "{verdict}");
    let run = rig.nika(&["run", "cap.nika.yaml", "--json", "--max-cost-usd", "1"]);
    assert_eq!(run.status.code(), Some(2), "{}", text(&run.stderr));
    assert!(!text(&run.stdout).contains("\"kind\":\"task_started\""));
}

/// The ONE lane-row shape: `check --json`, `run --dry-run --json` and the
/// boot manifest carry the same rows for the same file on the same
/// machine.
#[test]
fn one_lane_shape_on_three_surfaces() {
    let rig = Rig::new("shape");
    rig.write("lane.nika.yaml", &workflow(256));
    let check = rig.nika(&["check", "lane.nika.yaml", "--json"]);
    let check_json: serde_json::Value =
        serde_json::from_str(&text(&check.stdout)).expect("check json");
    let check_rows = sorted_rows(&check_json["access_plan"]);
    assert_eq!(check_rows.len(), 1, "{check_json}");
    assert_eq!(check_rows[0]["access"], "codex");
    assert_eq!(check_rows[0]["chosen"], "harness");
    assert!(check_rows[0].get("rejected").is_some());

    let dry = rig.nika(&[
        "run",
        "lane.nika.yaml",
        "--dry-run",
        "--json",
        "--max-cost-usd",
        "1",
    ]);
    let dry_json: serde_json::Value = serde_json::from_str(&text(&dry.stdout)).expect("dry json");
    let dry_rows = sorted_rows(&dry_json["access"]["plans"]);
    assert_eq!(
        dry_rows, check_rows,
        "dry-run rows ≠ check rows: {dry_json}"
    );
    assert_eq!(dry_json["access"]["seat"], "codex", "{dry_json}");

    let run = rig.nika(&["run", "lane.nika.yaml", "--json", "--max-cost-usd", "1"]);
    let stdout = text(&run.stdout);
    assert_eq!(run.status.code(), Some(0), "{}", text(&run.stderr));
    let started = stdout
        .lines()
        .find(|l| l.contains("\"kind\":\"workflow_started\""))
        .expect("boot frame");
    let frame: serde_json::Value = serde_json::from_str(started).expect("frame");
    let manifest = frame["fields"]
        .as_array()
        .expect("fields")
        .iter()
        .find(|f| f["key"] == "access_plan")
        .and_then(|f| f["value"].as_str())
        .expect("access_plan on the boot manifest");
    let boot_rows = sorted_rows(&serde_json::from_str(manifest).expect("manifest rows"));
    assert_eq!(boot_rows, check_rows, "boot rows ≠ check rows: {manifest}");
}

/// `check --access` pins the plan the way `run --access` does: the API
/// pin rides the key, the mock pin on an openai model refuses (advisory
/// on the default profile, exit 2 under `--profile operational`).
#[test]
fn check_access_pins_like_run() {
    let rig = Rig::new("pin");
    rig.write("lane.nika.yaml", &workflow(256));
    let api = rig.nika(&["check", "lane.nika.yaml", "--json", "--access", "api"]);
    let api_json: serde_json::Value = serde_json::from_str(&text(&api.stdout)).expect("json");
    assert_eq!(api.status.code(), Some(0), "{api_json}");
    assert_eq!(api_json["access_plan"][0]["pinned"], true, "{api_json}");
    assert_eq!(api_json["access_plan"][0]["chosen"], "api", "{api_json}");
    assert_eq!(api_json["verdicts"]["access_ready"], true, "{api_json}");

    let mock = rig.nika(&["check", "lane.nika.yaml", "--json", "--access", "mock"]);
    let mock_json: serde_json::Value = serde_json::from_str(&text(&mock.stdout)).expect("json");
    assert_eq!(
        mock.status.code(),
        Some(0),
        "advisory by default: {mock_json}"
    );
    assert_eq!(
        mock_json["access_plan"][0]["resolved"], false,
        "{mock_json}"
    );
    assert_eq!(mock_json["verdicts"]["access_ready"], false, "{mock_json}");
    assert_eq!(mock_json["verdicts"]["run_ready"], false, "{mock_json}");
    assert_eq!(mock_json["clean"], true, "VALID is untouched: {mock_json}");

    let operational = rig.nika(&[
        "check",
        "lane.nika.yaml",
        "--access",
        "mock",
        "--profile",
        "operational",
    ]);
    let out = text(&operational.stdout);
    assert_eq!(operational.status.code(), Some(2), "{out}");
    assert!(out.contains("access not ready"), "{out}");
}

/// The human face: an ACCESS rung under MODELS and the layers line after
/// the audited line, naming the four questions.
#[test]
fn the_layers_line_names_the_four_questions() {
    let rig = Rig::new("layers");
    rig.write("lane.nika.yaml", &workflow(256));
    let check = rig.nika(&["check", "lane.nika.yaml"]);
    let out = text(&check.stdout);
    assert_eq!(check.status.code(), Some(0), "{out}");
    assert!(
        out.contains("ACCESS") && out.contains("codex (harness"),
        "the ACCESS rung names the seat: {out}"
    );
    assert!(
        out.contains("layers · valid")
            && out.contains("access ready")
            && out.contains("capacity fit")
            && out.contains("run ready"),
        "the four questions: {out}"
    );
    assert!(
        !out.contains("key presence on this machine not judged"),
        "the old parenthetical is retired: {out}"
    );
}
