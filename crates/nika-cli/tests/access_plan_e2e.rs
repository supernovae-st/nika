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
    frame_fields_of(stdout, kind, None)
}

/// The `fields` of the first frame of `kind` that names `task` (any
/// task when `None`), as (key, value) pairs.
fn frame_fields_of(
    stdout: &str,
    kind: &str,
    task: Option<&str>,
) -> Vec<(String, serde_json::Value)> {
    let needle = format!("\"kind\":\"{kind}\"");
    let task_needle = task.map(|t| format!("\"value\":\"{t}\""));
    let line = stdout
        .lines()
        .find(|l| l.contains(&needle) && task_needle.as_ref().is_none_or(|t| l.contains(t)))
        .unwrap_or_else(|| panic!("no {kind} frame for {task:?} in:\n{stdout}"));
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

/// A seated infer, then a human gate, then a second infer that reads
/// the gate — the shape a `--resume` needs (the pause is the fold point).
const GATED: &str = "nika: one-door-gate
model: openai/gpt-5.2
permits: { tools: [\"nika:prompt\"] }
tasks:
  answer:
    infer:
      prompt: classify this
      max_tokens: 256
  gate:
    after: { answer: success }
    invoke: { tool: \"nika:prompt\", args: { message: \"ship it?\" } }
  after_gate:
    with: { ok: \"${{ tasks.gate.output }}\" }
    infer:
      prompt: \"the gate said ${{ with.ok }}\"
      max_tokens: 256
";

impl Rig {
    fn write(&self, name: &str, body: &str) {
        std::fs::write(self.root.join("work").join(name), body).expect("workflow");
    }

    /// The machine loses its seat between the pause and the resume.
    fn drop_codex(&self) {
        for bin in ["codex", "codex-acp"] {
            std::fs::remove_file(self.root.join("bin").join(bin)).expect("fake removed");
        }
    }

    /// The newest trace the project wrote (`.nika/traces/<ts>-<id>.ndjson`).
    fn newest_trace(&self) -> String {
        let dir = self.root.join("work").join(".nika").join("traces");
        let mut names: Vec<String> = std::fs::read_dir(&dir)
            .expect("traces dir")
            .filter_map(Result::ok)
            .map(|e| e.file_name().to_string_lossy().into_owned())
            .filter(|n| n.ends_with(".ndjson"))
            .collect();
        names.sort();
        let last = names.pop().expect("one trace");
        dir.join(last).to_string_lossy().into_owned()
    }

    /// Run the gated workflow up to its pause on the seat; the trace path.
    fn pause_on_the_seat(&self) -> String {
        self.write("gate.nika.yaml", GATED);
        let out = self.nika(&["run", "gate.nika.yaml", "--max-cost-usd", "1"], true);
        assert_eq!(
            out.status.code(),
            Some(4),
            "the gate pauses the seated run\nstdout: {}\nstderr: {}",
            text(&out.stdout),
            text(&out.stderr)
        );
        self.newest_trace()
    }
}

/// « Resume cannot switch access silently » (the one-door pack): the
/// trace rode `codex`; the machine lost its seat and now resolves the
/// API path for the same model — a flag-less resume refuses, naming
/// both paths and the two explicit flags. Nothing is dialed.
#[test]
fn resume_cannot_switch_access_silently() {
    let rig = Rig::new("resume-switch", true);
    let trace = rig.pause_on_the_seat();
    rig.drop_codex();
    let out = rig.nika(
        &[
            "run",
            "gate.nika.yaml",
            "--resume",
            &trace,
            "--answer",
            "gate=yes",
            "--max-cost-usd",
            "1",
        ],
        true,
    );
    let stdout = text(&out.stdout);
    let stderr = text(&out.stderr);
    assert_eq!(
        out.status.code(),
        Some(3),
        "an environment refusal\nstdout: {stdout}\nstderr: {stderr}"
    );
    assert!(
        stderr.contains("NIKA-1807")
            && stderr.contains("ran `openai/gpt-5.2` on `codex`")
            && stderr.contains("now resolves `openai`")
            && stderr.contains("--access codex")
            && stderr.contains("--access api"),
        "the refusal carries its code and names both paths and both flags: {stderr}"
    );
    assert!(
        !stdout.contains("127.0.0.1:9") && !stderr.contains("127.0.0.1:9"),
        "nothing was dialed\nstdout: {stdout}\nstderr: {stderr}"
    );
}

/// The explicit `--access api` NAMES the change: the resume proceeds
/// with a notice, the seated task's cached output is not served on the
/// other path (its lane joined the identity), and the API path is what
/// runs — and fails honestly on the dead key.
#[test]
fn an_explicit_pin_declares_the_access_change_on_resume() {
    let rig = Rig::new("resume-declare", true);
    let trace = rig.pause_on_the_seat();
    rig.drop_codex();
    let out = rig.nika(
        &[
            "run",
            "gate.nika.yaml",
            "--resume",
            &trace,
            "--answer",
            "gate=yes",
            "--access",
            "api",
            "--max-cost-usd",
            "1",
        ],
        true,
    );
    let stdout = text(&out.stdout);
    let stderr = text(&out.stderr);
    assert!(
        stderr.contains("access change declared")
            && stderr.contains("on `codex`")
            && stderr.contains("on `openai`"),
        "the change is noticed, never silent: {stderr}"
    );
    assert!(
        out.status.code() != Some(3) && out.status.code() != Some(0),
        "the API path is dialed and fails on the dead key\nstdout: {stdout}\nstderr: {stderr}"
    );
    assert!(
        !stdout.contains("seated-answer") && !stderr.contains("seated-answer"),
        "the seat's cached answer is not served on the API lane\nstdout: {stdout}\nstderr: {stderr}"
    );
}

/// The lane unchanged: the resume serves the seated task from the trace
/// (no churn in the identity) and runs the rest on the same seat.
#[test]
fn a_seated_resume_keeps_its_lane_and_its_cache() {
    let rig = Rig::new("resume-keep", true);
    let trace = rig.pause_on_the_seat();
    let out = rig.nika(
        &[
            "run",
            "gate.nika.yaml",
            "--resume",
            &trace,
            "--answer",
            "gate=yes",
            "--json",
            "--max-cost-usd",
            "1",
        ],
        true,
    );
    let stdout = text(&out.stdout);
    let stderr = text(&out.stderr);
    assert_eq!(
        out.status.code(),
        Some(0),
        "the resumed run completes on the seat\nstdout: {stdout}\nstderr: {stderr}"
    );
    assert!(
        !stderr.contains("access change"),
        "no change, no notice: {stderr}"
    );
    assert!(
        stdout.contains("\"kind\":\"task_cache_hit\""),
        "the seated task is served from the trace: {stdout}"
    );
    let fields = frame_fields_of(&stdout, "task_completed", Some("after_gate"));
    assert_eq!(
        field(&fields, "access_id").and_then(serde_json::Value::as_str),
        Some("codex"),
        "the leg after the gate rides the same seat: {fields:?}"
    );
}

const SEAT_ONLY: &str = "nika: seat-only
tasks:
  answer:
    infer:
      prompt: say hi
      max_tokens: 256
";

fn last_frame(stdout: &str) -> serde_json::Value {
    let line = stdout.lines().last().expect("a verdict frame");
    serde_json::from_str(line).unwrap_or_else(|e| panic!("the last line is JSON ({e}): {line}"))
}

fn json_object(stdout: &str) -> serde_json::Value {
    serde_json::from_str(stdout.trim())
        .unwrap_or_else(|e| panic!("one JSON object ({e}):\n{stdout}"))
}

/// W3-F1 · the seat is two binaries. With the product (`codex`) gone and
/// only the ACP speaker (`codex-acp`) on PATH, a pin serving an `infer:`
/// is refused on the three doors — `check --access`, the dry-run, the
/// run — before task 1, with the same code, never « admission satisfied ».
#[test]
fn a_pinned_seat_without_its_product_binary_refuses_on_three_doors() {
    let rig = Rig::new("no-product", true);
    std::fs::remove_file(rig.root.join("bin").join("codex")).expect("the product goes");
    std::fs::write(rig.root.join("work").join("seat.nika.yaml"), SEAT_ONLY).expect("workflow");
    let check = rig.nika(
        &["check", "seat.nika.yaml", "--json", "--access", "codex"],
        false,
    );
    let obj = json_object(&text(&check.stdout));
    assert_eq!(obj["verdicts"]["access_ready"], false, "{obj:#}");
    assert!(
        obj["verdicts"]["blockers"][0]
            .as_str()
            .is_some_and(|b| b.contains("pin `codex` refused")),
        "{obj:#}"
    );
    let dry = rig.nika(
        &["run", "seat.nika.yaml", "--dry-run", "--access", "codex"],
        false,
    );
    let dry_out = text(&dry.stdout);
    assert_eq!(
        dry.status.code(),
        Some(3),
        "the preview exits like the run\n{dry_out}"
    );
    let dry_all = format!("{dry_out}\n{}", text(&dry.stderr));
    assert!(
        dry_all.contains("NIKA-1803") && dry_all.contains("not installed"),
        "the preview refuses with the code and the fix: {dry_all}"
    );
    let run = rig.nika(
        &[
            "run",
            "seat.nika.yaml",
            "--json",
            "--access",
            "codex",
            "--max-cost-usd",
            "1",
        ],
        false,
    );
    let stdout = text(&run.stdout);
    assert_eq!(
        run.status.code(),
        Some(3),
        "{stdout}\n{}",
        text(&run.stderr)
    );
    let settled = last_frame(&stdout);
    assert_eq!(settled["kind"], "run_settled", "{settled}");
    assert_eq!(settled["error"]["code"], "NIKA-1803", "{settled}");
    assert!(
        !stdout.contains("\"kind\":\"task_started\""),
        "nothing ran: {stdout}"
    );
}

/// W3-F13 · an `infer:` that names no model rides a seat or nothing:
/// unpinned, the three doors refuse before task 1 (NIKA-1800) and the
/// operational profile is red; pinned to a present seat, `check` is
/// ready and the run rides the seat.
#[test]
fn a_model_less_infer_needs_a_model_or_a_seat() {
    let rig = Rig::new("model-less", true);
    std::fs::write(rig.root.join("work").join("seat.nika.yaml"), SEAT_ONLY).expect("workflow");
    let check = rig.nika(&["check", "seat.nika.yaml", "--json"], false);
    assert_eq!(check.status.code(), Some(0), "advisory: legal");
    let obj = json_object(&text(&check.stdout));
    assert_eq!(obj["verdicts"]["access_ready"], false, "{obj:#}");
    assert!(
        obj["verdicts"]["blockers"][0]
            .as_str()
            .is_some_and(|b| b.contains("names no model") && b.contains("--access")),
        "{obj:#}"
    );
    let operational = rig.nika(
        &["check", "seat.nika.yaml", "--profile", "operational"],
        false,
    );
    assert_eq!(
        operational.status.code(),
        Some(2),
        "{}",
        text(&operational.stdout)
    );
    let dry = rig.nika(&["run", "seat.nika.yaml", "--dry-run"], false);
    assert_eq!(dry.status.code(), Some(3), "{}", text(&dry.stdout));
    assert!(
        text(&dry.stdout).contains("names no model"),
        "{}",
        text(&dry.stdout)
    );
    let run = rig.nika(
        &["run", "seat.nika.yaml", "--json", "--max-cost-usd", "1"],
        false,
    );
    let stdout = text(&run.stdout);
    assert_eq!(
        run.status.code(),
        Some(3),
        "{stdout}\n{}",
        text(&run.stderr)
    );
    let settled = last_frame(&stdout);
    assert_eq!(settled["error"]["code"], "NIKA-1800", "{settled}");
    assert_eq!(settled["error"]["task"], "-", "{settled}");
    // Pinned to the present seat: ready, and the run rides it.
    let pinned = rig.nika(
        &["check", "seat.nika.yaml", "--json", "--access", "codex"],
        false,
    );
    let obj = json_object(&text(&pinned.stdout));
    assert_eq!(obj["verdicts"]["access_ready"], true, "{obj:#}");
    let run = rig.nika(
        &[
            "run",
            "seat.nika.yaml",
            "--json",
            "--access",
            "codex",
            "--max-cost-usd",
            "1",
        ],
        false,
    );
    let stdout = text(&run.stdout);
    assert_eq!(
        run.status.code(),
        Some(0),
        "{stdout}\n{}",
        text(&run.stderr)
    );
    assert_eq!(last_frame(&stdout)["status"], "succeeded", "{stdout}");
}
