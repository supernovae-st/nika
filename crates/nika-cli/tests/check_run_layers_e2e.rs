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
        Command::new(env!("CARGO_BIN_EXE_nika"))
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

fn field<'a>(fields: &'a [(String, serde_json::Value)], key: &str) -> Option<&'a str> {
    fields
        .iter()
        .find(|(k, _)| k == key)
        .and_then(|(_, v)| v.as_str())
}

fn last_frame(stdout: &str) -> serde_json::Value {
    let line = stdout.lines().last().expect("a verdict frame");
    serde_json::from_str(line).unwrap_or_else(|e| panic!("the last line is JSON ({e}): {line}"))
}

/// A FAILED task terminal carries the lane that failed: the API path
/// pinned by `--access api` dials the dead key, and the `task_failed`
/// frame names `model` · `provider` · `access` · `access_id` · `billing`
/// — a sealed trace says which path was allowed to bill (W1-F4).
#[test]
fn a_failed_task_terminal_carries_its_lane() {
    let rig = Rig::new("failed-lane");
    rig.write("lane.nika.yaml", &workflow(256));
    let run = rig.nika(&[
        "run",
        "lane.nika.yaml",
        "--json",
        "--access",
        "api",
        "--max-cost-usd",
        "1",
    ]);
    let stdout = text(&run.stdout);
    assert_eq!(
        run.status.code(),
        Some(1),
        "the API path fails on the dead key\n{stdout}"
    );
    let fields = frame_fields(&stdout, "task_failed");
    assert_eq!(field(&fields, "access_id"), Some("openai"), "{fields:?}");
    assert_eq!(field(&fields, "access"), Some("api"), "{fields:?}");
    assert_eq!(
        field(&fields, "model"),
        Some("openai/gpt-5.2"),
        "{fields:?}"
    );
    assert_eq!(field(&fields, "provider"), Some("openai"), "{fields:?}");
    assert_eq!(field(&fields, "billing"), Some("api_metered"), "{fields:?}");
    assert!(
        field(&fields, "note").is_some_and(|n| n.contains("openai/gpt-5.2")),
        "the note names the model, never `?`: {fields:?}"
    );
    // The verdict frame carries the cause AND the lanes (W1-F7 · W1-F13).
    let settled = last_frame(&stdout);
    assert_eq!(settled["kind"], "run_settled", "{settled}");
    assert_eq!(settled["status"], "failed", "{settled}");
    assert_eq!(settled["error"]["task"], "answer", "{settled}");
    assert_eq!(settled["access_plan"][0]["access"], "openai", "{settled}");
    assert_eq!(settled["access_plan"][0]["chosen"], "api", "{settled}");
}

/// A run REFUSED before any task settles with its code on the verdict
/// frame — `--access mock` on an openai model is NIKA-1801 (a pin is a
/// pin · never a substitute), and the last line a CI reader parses names
/// it (W1-F7).
#[test]
fn a_refused_run_settles_with_its_code() {
    let rig = Rig::new("refused-code");
    rig.write("lane.nika.yaml", &workflow(256));
    let run = rig.nika(&[
        "run",
        "lane.nika.yaml",
        "--json",
        "--access",
        "mock",
        "--max-cost-usd",
        "1",
    ]);
    let stdout = text(&run.stdout);
    assert_eq!(
        run.status.code(),
        Some(3),
        "{stdout}\n{}",
        text(&run.stderr)
    );
    let settled = last_frame(&stdout);
    assert_eq!(settled["kind"], "run_settled", "{settled}");
    assert_eq!(settled["status"], "failed", "{settled}");
    assert_eq!(settled["error"]["code"], "NIKA-1801", "{settled}");
    // ADR-128 · a launch refusal names no task: the field is absent.
    assert!(settled["error"].get("task").is_none(), "{settled}");
    assert_eq!(settled["cause"], "refused", "{settled}");
    assert!(
        !stdout.contains("\"kind\":\"task_started\""),
        "nothing ran: {stdout}"
    );
}

/// A seated success settles with the lanes that served (W1-F13): the
/// verdict frame's `access_plan` rows are the ONE lane-row shape.
#[test]
fn a_seated_run_settles_with_its_lanes() {
    let rig = Rig::new("settled-lanes");
    rig.write("lane.nika.yaml", &workflow(256));
    let run = rig.nika(&["run", "lane.nika.yaml", "--json", "--max-cost-usd", "1"]);
    let stdout = text(&run.stdout);
    assert_eq!(run.status.code(), Some(0), "{}", text(&run.stderr));
    let settled = last_frame(&stdout);
    assert_eq!(settled["status"], "succeeded", "{settled}");
    assert_eq!(settled["access_plan"][0]["access"], "codex", "{settled}");
    assert_eq!(settled["access_plan"][0]["chosen"], "harness", "{settled}");
    assert!(
        settled.get("error").is_none(),
        "a green frame claims no cause: {settled}"
    );
}

/// `run --help` carries the exit ladder (W1-F6) and `nika explain`
/// teaches the resume access refusal's code.
#[test]
fn the_run_help_and_the_explain_teach_the_exit_codes() {
    let rig = Rig::new("help");
    let help = rig.nika(&["run", "--help"]);
    let out = text(&help.stdout);
    assert!(
        out.contains("exit codes") && out.contains("NIKA-1807") && out.contains("4 PAUSED"),
        "the run help lists its codes: {out}"
    );
    let explain = rig.nika(&["explain", "NIKA-1807"]);
    let out = text(&explain.stdout);
    assert_eq!(explain.status.code(), Some(0), "{out}");
    assert!(
        out.contains("recorded path"),
        "the code teaches the two flags: {out}"
    );
}

/// W3-F10 · the exit-3 lane is ONE JSON object on stdout under `--json`
/// on both verbs — a missing file never prints prose to a machine reader.
#[test]
fn the_exit_three_lane_is_json_on_both_verbs() {
    let rig = Rig::new("exit-three");
    for args in [
        vec!["check", "missing.nika.yaml", "--json"],
        vec!["run", "missing.nika.yaml", "--json", "--max-cost-usd", "1"],
    ] {
        let out = rig.nika(&args);
        let stdout = text(&out.stdout);
        let stderr = text(&out.stderr);
        assert_eq!(out.status.code(), Some(3), "{args:?}\n{stdout}\n{stderr}");
        let obj: serde_json::Value = serde_json::from_str(stdout.trim()).unwrap_or_else(|e| {
            panic!("{args:?}: stdout is one JSON object ({e}):\n{stdout}\nstderr: {stderr}")
        });
        assert_eq!(obj["parse_fatal"], true, "{obj:#}");
        assert_eq!(obj["clean"], false, "{obj:#}");
        assert!(
            !stderr.contains("nika: {"),
            "no prefixed JSON on stderr: {stderr}"
        );
    }
}

/// W3-F6 · `check --help` carries the layers legend and the `--json` gate
/// keys where a CI author looks.
#[test]
fn the_check_help_carries_the_legend_and_the_gate_keys() {
    let rig = Rig::new("check-help");
    let help = text(&rig.nika(&["check", "--help"]).stdout);
    for word in [
        "ACCESS READY (",
        "RUN READY (",
        "verdicts.{valid,access_ready,capacity_fit,run_ready,blockers}",
        "model_findings[]",
        "judged.{composition,skills}",
    ] {
        assert!(help.contains(word), "`{word}` in: {help}");
    }
}

/// W3-F3 · an outranked ready path rides the lane row: the seat wins over
/// the present API key, and the JSON says so.
#[test]
fn an_outranked_path_rides_the_lane_rows() {
    let rig = Rig::new("outranked");
    rig.write("lane.nika.yaml", &workflow(256));
    let out = rig.nika(&["check", "lane.nika.yaml", "--json"]);
    let obj: serde_json::Value =
        serde_json::from_str(text(&out.stdout).trim()).expect("one object");
    let lane = &obj["access_plan"][0];
    assert_eq!(lane["access"], "codex", "{obj:#}");
    let outranked = lane["outranked"].as_array().expect("outranked rows");
    assert!(
        outranked
            .iter()
            .any(|r| r["access"] == "openai" && r["dimension"] == "outranked"),
        "{obj:#}"
    );
    assert_eq!(lane["candidates"], 2, "{obj:#}");
}

/// W3-F9 · the operational profile SAYS it held on a green file, and the
/// mock lane reads « never dials ».
#[test]
fn the_operational_footer_prints_on_green_and_the_mock_lane_never_dials() {
    let rig = Rig::new("operational-green");
    rig.write(
        "mock.nika.yaml",
        "nika: m\nmodel: mock/echo\ntasks:\n  t:\n    infer: { prompt: hi, max_tokens: 10 }\n",
    );
    let out = rig.nika(&["check", "mock.nika.yaml", "--profile", "operational"]);
    let stdout = text(&out.stdout);
    assert_eq!(out.status.code(), Some(0), "{stdout}");
    assert!(stdout.contains("✔ operational · risk"), "{stdout}");
    assert!(stdout.contains("never dials"), "{stdout}");
}
