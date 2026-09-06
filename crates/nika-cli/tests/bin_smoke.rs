// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>
#![allow(clippy::expect_used, clippy::panic)]
// The workspace bans std::process::Command (production spawns ride the
// kernel ShellExecutor seam). This test's WHOLE JOB is to execute the
// real `nika` binary (CARGO_BIN_EXE) — routing through the kernel
// seam would test the lib, not the binary contract. The one legitimate
// carve-out class (same exemption as a future assert_cmd harness).
#![allow(clippy::disallowed_types)]

//! Binary smoke — the REAL `nika` executable through its locked
//! exit-code contract (spec §4 · `0` ok · `2` file findings · `3`
//! environment error) + the `--json` purity law (never coloured ·
//! machine-parseable even under failure).
//!
//! Zero harness deps: `CARGO_BIN_EXE_<name>` is cargo's native bin-test
//! mechanism — the S6 admission owed this coverage (the verbs were
//! tested at the lib seam · the BINARY contract was not).

use std::io::Write as _;
use std::process::Command;

fn bin() -> Command {
    Command::new(env!("CARGO_BIN_EXE_nika"))
}

fn write_fixture(dir: &std::path::Path, name: &str, yaml: &str) -> std::path::PathBuf {
    let path = dir.join(name);
    let mut f = std::fs::File::create(&path).expect("fixture file");
    f.write_all(yaml.as_bytes()).expect("fixture body");
    path
}

fn workspace_tmp_dir(name: &str) -> std::path::PathBuf {
    let base = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .join("target")
        .join("tmp");
    let dir = base.join(format!("{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("tmp dir");
    dir
}

const VALID: &str = r#"
nika: smoke
permits: { exec: ["echo"] }
tasks:
  greet:
    exec: { command: ["echo", "hello"] }
"#;

const INVALID: &str = r#"
nika: smoke-broken
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

const FAILING: &str = r#"
nika: smoke-fail
permits:
  exec: true
tasks:
  boom:
    exec: { shell: "exit 7" }
"#;

#[test]
fn check_valid_exits_zero() {
    let dir = std::env::temp_dir().join("nika-bin-smoke-ok");
    std::fs::create_dir_all(&dir).expect("tmp dir");
    let wf = write_fixture(&dir, "ok.nika.yaml", VALID);

    let out = bin().arg("check").arg(&wf).output().expect("binary runs");
    assert_eq!(
        out.status.code(),
        Some(0),
        "spec §4 · a clean workflow checks at 0 · stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
}

#[test]
fn list_prints_exactly_the_two_workflows_below_cwd() {
    let dir = workspace_tmp_dir("nika-bin-list-two");
    std::fs::create_dir_all(dir.join("nested")).expect("nested dir");
    write_fixture(&dir, "alpha.nika.yaml", "nika: alpha\n");
    write_fixture(&dir.join("nested"), "beta.nika.yml", "nika: beta\n");
    write_fixture(&dir, "nika.yaml", "nika: v1\n");

    let out = bin()
        .arg("list")
        .current_dir(&dir)
        .output()
        .expect("binary runs");

    assert_eq!(
        out.status.code(),
        Some(0),
        "the workflow inventory exits 0 · stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert_eq!(
        String::from_utf8_lossy(&out.stdout),
        "alpha.nika.yaml\nnested/beta.nika.yml\n"
    );
    assert!(out.stderr.is_empty(), "list stderr must stay empty");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn run_clean_workflow_exits_zero() {
    // The `run` verb — the most-used one — had ZERO binary-contract
    // coverage. A clean workflow runs through the L3 runtime and exits 0.
    let dir = std::env::temp_dir().join(format!("nika-bin-run-ok-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("tmp dir");
    let wf = write_fixture(&dir, "ok.nika.yaml", VALID);

    let out = bin()
        .arg("run")
        .arg(&wf)
        .arg("--json")
        .output()
        .expect("binary runs");
    assert_eq!(
        out.status.code(),
        Some(0),
        "spec §4 · a clean run exits 0 · stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn run_failed_task_exits_one_not_two() {
    // spec §4 · the run-vs-file distinction made executable: a TASK that
    // fails (the engine is healthy · the workflow actually RAN) exits 1,
    // NEVER 2 (a FILE finding caught before any task runs). This is the
    // split CI/alerting depend on ("CI gates on 2, alerting gates on 1")
    // and the one locked exit code the smoke suite never pinned.
    let dir = std::env::temp_dir().join(format!("nika-bin-run-fail-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("tmp dir");
    let wf = write_fixture(&dir, "fail.nika.yaml", FAILING);

    let out = bin()
        .arg("run")
        .arg(&wf)
        .arg("--json")
        .output()
        .expect("binary runs");
    assert_eq!(
        out.status.code(),
        Some(1),
        "a failed task exits 1 (workflow failed), never 2 (file finding) · stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
}

#[test]
fn run_invalid_dag_exits_two_not_one() {
    // The other side of the 1-vs-2 split: `run` pre-flights the static
    // ladder, so a FILE-level fault (a DAG cycle) is caught BEFORE any task
    // runs and exits 2 — the same code `check` returns, distinct from the
    // workflow-failed 1. Pins that `run` never collapses the two.
    let dir = std::env::temp_dir().join(format!("nika-bin-run-cyc-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("tmp dir");
    let wf = write_fixture(&dir, "cyc.nika.yaml", INVALID);

    let out = bin()
        .arg("run")
        .arg(&wf)
        .arg("--json")
        .output()
        .expect("binary runs");
    assert_eq!(
        out.status.code(),
        Some(2),
        "an invalid DAG is a file finding (2), not a workflow failure (1) · stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let _ = std::fs::remove_dir_all(&dir);
}

/// #411 · the `--task` help's promise made executable: « the full
/// workflow still audits (findings stay whole-file faithful) ». A
/// PERMITS violation in a branch OUTSIDE the target's ancestor cone
/// refuses the scoped run (exit 2, the finding printed) exactly like
/// the unscoped one — and a clean file still scopes and runs at 0.
#[test]
fn run_task_scope_still_audits_the_whole_file() {
    let dir = workspace_tmp_dir("nika-bin-run-task-411");
    let poisoned = r#"
nika: i411
permits:
  exec: ["echo"]
tasks:
  fetch_data:
    exec: { command: ["echo", "data"] }
  render_page:
    after:
      fetch_data: success
    exec: { command: ["echo", "page"] }
  compress:
    exec: { command: ["tar", "--version"] }
"#;
    let wf = write_fixture(&dir, "poisoned.nika.yaml", poisoned);

    let out = bin()
        .arg("run")
        .arg(&wf)
        .arg("--task")
        .arg("render_page")
        .arg("--no-progress")
        .output()
        .expect("binary runs");
    assert_eq!(
        out.status.code(),
        Some(2),
        "the out-of-cone PERMITS finding refuses the scoped run · stdout: {} · stderr: {}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    let all = format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(
        all.contains("tar"),
        "the finding names the out-of-cone program: {all}"
    );

    let clean = r#"
nika: i411-clean
permits:
  exec: ["echo"]
tasks:
  fetch_data:
    exec: { command: ["echo", "data"] }
  render_page:
    after:
      fetch_data: success
    exec: { command: ["echo", "page"] }
  compress:
    exec: { command: ["echo", "z"] }
"#;
    let ok = write_fixture(&dir, "clean.nika.yaml", clean);
    let out = bin()
        .arg("run")
        .arg(&ok)
        .arg("--task")
        .arg("render_page")
        .arg("--no-progress")
        .output()
        .expect("binary runs");
    assert_eq!(
        out.status.code(),
        Some(0),
        "a clean file still scopes and runs · stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let stdout = String::from_utf8(out.stdout).expect("utf8");
    assert!(
        !stdout.contains("compress"),
        "the scope held — the independent branch never ran: {stdout}"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn run_dry_run_executes_zero_effects() {
    // spec §10 `--dry-run` · "plan only · zero effects". The proof: FAILING
    // exits 1 when actually run (run_failed_task_exits_one_not_two pins that).
    // Under --dry-run the task is NEVER executed, so the failing command can't
    // fail the run — it exits 0 after showing the plan.
    let dir = std::env::temp_dir().join(format!("nika-bin-dry-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("tmp dir");
    let wf = write_fixture(&dir, "dry.nika.yaml", FAILING);

    let out = bin()
        .arg("run")
        .arg(&wf)
        .arg("--dry-run")
        .output()
        .expect("binary runs");
    assert_eq!(
        out.status.code(),
        Some(0),
        "--dry-run never executes the (failing) task · exits 0 · stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let stdout = String::from_utf8(out.stdout).expect("utf8");
    assert!(
        stdout.contains("dry-run") && stdout.contains("no effects"),
        "the dry-run banner shows: {stdout}"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn run_quiet_is_compact_no_storyboard() {
    // `--quiet`: the final verdict card only · NO per-task storyboard row.
    let dir = std::env::temp_dir().join(format!("nika-bin-quiet-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("tmp dir");
    let wf = write_fixture(&dir, "ok.nika.yaml", VALID);

    let out = bin()
        .arg("run")
        .arg(&wf)
        .arg("--quiet")
        .args(["--color", "never"])
        .output()
        .expect("binary runs");
    assert_eq!(out.status.code(), Some(0), "clean run exits 0");
    let stdout = String::from_utf8(out.stdout).expect("utf8");
    assert!(
        stdout.contains("smoke"),
        "the verdict names the workflow: {stdout}"
    );
    assert!(
        !stdout.contains("greet"),
        "quiet suppresses the per-task rows (no `greet`): {stdout}"
    );
}

#[test]
fn run_no_progress_emits_no_ansi() {
    // `--no-progress`: the plain final storyboard · deterministic · zero ANSI
    // cursor escapes even on a TTY (CI-stable capture).
    let dir = std::env::temp_dir().join(format!("nika-bin-plain-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("tmp dir");
    let wf = write_fixture(&dir, "ok.nika.yaml", VALID);

    let out = bin()
        .arg("run")
        .arg(&wf)
        .arg("--no-progress")
        .output()
        .expect("binary runs");
    assert_eq!(out.status.code(), Some(0), "clean run exits 0");
    let stdout = String::from_utf8(out.stdout).expect("utf8");
    assert!(
        !stdout.contains('\x1b'),
        "--no-progress leaks no ANSI escapes: {stdout:?}"
    );
    assert!(
        stdout.contains("greet"),
        "plain shows the full storyboard: {stdout}"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn run_human_flags_conflict_with_machine_modes() {
    // The render flags are HUMAN surfaces · clap REFUSES them alongside
    // the `--json`/`--output json` machine modes, so a parsed-but-ignored flag
    // can never silently corrupt the `capture: stdout` JSON contract.
    // `--dry-run` left this list in #332: with `--json` it IS a machine
    // surface (the versioned plan object) — only `--output` still refuses
    // it (an outputs export of a run that never executed would be a lie).
    let dir = std::env::temp_dir().join(format!("nika-bin-conflict-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("tmp dir");
    let wf = write_fixture(&dir, "ok.nika.yaml", VALID);

    for human in ["--quiet", "--no-progress"] {
        let out = bin()
            .arg("run")
            .arg(&wf)
            .arg(human)
            .arg("--json")
            .output()
            .expect("binary runs");
        assert!(
            !out.status.success(),
            "{human} + --json must be a usage error, got {:?}",
            out.status.code()
        );
        let stderr = String::from_utf8_lossy(&out.stderr);
        assert!(
            stderr.contains("cannot be used with") || stderr.contains("conflict"),
            "clap names the {human}/--json conflict: {stderr}"
        );
    }

    // --dry-run + --json: the #332 plan object (exit 0 · plan_version 1 ·
    // zero effects — no trace dir appears).
    let out = bin()
        .arg("run")
        .arg(&wf)
        .arg("--dry-run")
        .arg("--json")
        .current_dir(&dir)
        .output()
        .expect("binary runs");
    assert!(
        out.status.success(),
        "--dry-run --json is the machine plan: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let plan: serde_json::Value =
        serde_json::from_slice(&out.stdout).expect("stdout is ONE JSON object");
    assert_eq!(plan["plan_version"], 1);
    assert_eq!(plan["effects_executed"], false);
    assert!(
        !dir.join(".nika").exists(),
        "a dry run never writes the trace store"
    );

    // --dry-run + --output json: still a usage error.
    let out = bin()
        .arg("run")
        .arg(&wf)
        .arg("--dry-run")
        .arg("--output")
        .arg("json")
        .output()
        .expect("binary runs");
    assert!(
        !out.status.success(),
        "--dry-run + --output must stay refused, got {:?}",
        out.status.code()
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn check_cycle_exits_two_with_findings() {
    let dir = std::env::temp_dir().join("nika-bin-smoke-bad");
    std::fs::create_dir_all(&dir).expect("tmp dir");
    let wf = write_fixture(&dir, "cycle.nika.yaml", INVALID);

    let out = bin().arg("check").arg(&wf).output().expect("binary runs");
    assert_eq!(
        out.status.code(),
        Some(2),
        "spec §4 · file findings exit at 2"
    );
}

#[test]
fn check_json_is_uncoloured_and_parseable_both_ways() {
    let dir = std::env::temp_dir().join("nika-bin-smoke-json");
    std::fs::create_dir_all(&dir).expect("tmp dir");

    for (name, yaml, expect_clean) in [
        ("ok.nika.yaml", VALID, true),
        ("bad.nika.yaml", INVALID, false),
    ] {
        let wf = write_fixture(&dir, name, yaml);
        let out = bin()
            .arg("check")
            .arg(&wf)
            .arg("--json")
            // Even under a colour-forcing env the JSON surface stays pure.
            .env("CLICOLOR_FORCE", "1")
            .output()
            .expect("binary runs");
        let stdout = String::from_utf8(out.stdout).expect("utf8");
        assert!(
            !stdout.contains('\x1b'),
            "--json is NEVER coloured (presentation canon) · {name}"
        );
        let parsed: serde_json::Value =
            serde_json::from_str(stdout.trim()).expect("one JSON document");
        assert_eq!(
            parsed["clean"].as_bool(),
            Some(expect_clean),
            "{name} verdict field"
        );
    }
}

#[test]
fn missing_file_is_an_environment_error() {
    let out = bin()
        .arg("check")
        .arg("/nonexistent/ghost.nika.yaml")
        .output()
        .expect("binary runs");
    assert_eq!(
        out.status.code(),
        Some(3),
        "spec §4 · environment error exits at 3"
    );
}

#[test]
fn explain_known_code_exits_zero() {
    let out = bin()
        .arg("explain")
        .arg("NIKA-1700")
        .output()
        .expect("binary runs");
    assert_eq!(
        out.status.code(),
        Some(0),
        "the runtime range is registered"
    );
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        stdout.contains("NIKA-1700"),
        "explain names the code: {stdout}"
    );
}

#[test]
fn doctor_diagnoses_the_environment_and_exits_zero() {
    // `nika doctor` is informational — the canonical catalog always offers an
    // inference path (local providers), so it exits 0. With a secret-shaped key
    // set it must report PRESENCE only · the value never reaches stdout.
    let out = bin()
        .arg("doctor")
        .env("ANTHROPIC_API_KEY", "sk-smoke-SHOULD-NOT-LEAK")
        .output()
        .expect("binary runs");
    assert_eq!(out.status.code(), Some(0), "a diagnosis is informational");
    let stdout = String::from_utf8(out.stdout).expect("utf8");
    assert!(
        stdout.contains("binary"),
        "renders the binary line: {stdout}"
    );
    assert!(
        stdout.contains("anthropic"),
        "names the providers: {stdout}"
    );
    assert!(
        !stdout.contains("sk-smoke"),
        "PRESENT-NOT-PRINTED · no secret value leaks: {stdout}"
    );
}

/// The `model serve` launch surface (ADR-091) through the real binary:
/// the required flag refuses at parse time, the port default (8712 —
/// clear of `nika mcp`'s 8123) is a help-visible contract, and the
/// no-model refusal is honest PER BUILD AXIS — the default binary
/// teaches the `local-infer` build recipe, a feature build reaches the
/// real path resolver (both exit 3, environment class).
#[test]
fn model_serve_pins_its_surface_and_refuses_honestly() {
    let out = bin()
        .args(["model", "serve"])
        .output()
        .expect("binary runs");
    assert_ne!(out.status.code(), Some(0), "a bare form must refuse");
    let err = String::from_utf8(out.stderr).expect("utf8");
    assert!(err.contains("--model"), "names the required flag: {err}");

    let out = bin()
        .args(["model", "serve", "--help"])
        .output()
        .expect("binary runs");
    let help = String::from_utf8(out.stdout).expect("utf8");
    assert!(help.contains("8712"), "the port default is visible: {help}");

    let out = bin()
        .args(["model", "serve", "--model", "absent.gguf"])
        .output()
        .expect("binary runs");
    assert_eq!(out.status.code(), Some(3), "environment-class refusal");
    let err = String::from_utf8(out.stderr).expect("utf8");
    #[cfg(not(feature = "local-infer"))]
    {
        assert!(err.contains("local-infer"), "teaches the feature: {err}");
        assert!(err.contains("cargo build"), "prints the recipe: {err}");
    }
    #[cfg(feature = "local-infer")]
    assert!(err.contains("no model file"), "the real resolver: {err}");
}

#[test]
fn init_scaffolds_a_repo_and_is_idempotent() {
    let dir = workspace_tmp_dir("nika-init-smoke");
    // First run · creates schema wiring + agent guide + Cursor rule · exit 0.
    let out = bin().arg("init").arg(&dir).output().expect("binary runs");
    assert_eq!(out.status.code(), Some(0), "scaffold succeeds");
    assert!(
        dir.join(".vscode/settings.json").is_file(),
        "schema wiring written"
    );
    assert!(dir.join("AGENTS.md").is_file(), "agent guide written");
    assert!(
        dir.join(".cursor/rules/nika.mdc").is_file(),
        "Cursor agent rule written"
    );
    assert!(dir.join(".cursor/mcp.json").is_file(), "Cursor MCP written");
    // Re-run · the human keeps the hand · existing files are SKIPPED, exit 0.
    let again = bin().arg("init").arg(&dir).output().expect("binary runs");
    assert_eq!(again.status.code(), Some(0));
    let stdout = String::from_utf8(again.stdout).expect("utf8");
    assert!(
        stdout.contains("skipped"),
        "idempotent re-run skips: {stdout}"
    );
    // Off-terminal (output() nulls stdin) the classic hand-off block is
    // byte-stable — scripts and CI never meet a prompt (clig.dev).
    assert!(
        stdout.contains("next ·"),
        "non-interactive init keeps the hand-off: {stdout}"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn bare_new_in_a_pipe_fails_fast_naming_the_flag() {
    // clig.dev: never REQUIRE interactivity. Bare `nika new` without a
    // terminal must not hang waiting on stdin — it fails fast, names the
    // missing flag, and still hands over the template set (wire line).
    let out = bin()
        .arg("new")
        .stdin(std::process::Stdio::null())
        .output()
        .expect("binary runs");
    assert_eq!(out.status.code(), Some(2), "fail fast, not a hang");
    let text = format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(
        text.contains("nika new '?'"),
        "names the discovery form: {text}"
    );
    assert!(text.contains("embedded set:"), "hands over the set: {text}");
}

#[test]
fn discovery_query_is_a_success_at_the_binary_plane() {
    // `nika new '?'` is the documented discovery command — exit 0
    // (a question answered is a success), the wire-contract line intact.
    let out = bin()
        .arg("new")
        .arg("?")
        .stdin(std::process::Stdio::null())
        .output()
        .expect("binary runs");
    assert_eq!(out.status.code(), Some(0), "discovery is a success");
    let stdout = String::from_utf8(out.stdout).expect("utf8");
    assert!(stdout.contains("embedded set:"), "{stdout}");
}

#[test]
fn wire_cursor_migrates_stale_mcp_config() {
    let home = workspace_tmp_dir("nika-wire-smoke");
    let cursor_dir = home.join(".cursor");
    std::fs::create_dir_all(&cursor_dir).expect("cursor dir");
    std::fs::write(
        cursor_dir.join("mcp.json"),
        r#"{
  "mcpServers": {
    "github": { "command": "gh", "args": ["mcp"] },
    "nika": { "command": "nika", "args": ["mcp", "serve", "--stdio"] }
  }
}
"#,
    )
    .expect("fixture");

    let out = bin()
        .arg("wire")
        .arg("cursor")
        .env("HOME", &home)
        .output()
        .expect("binary runs");
    assert_eq!(out.status.code(), Some(0), "wire succeeds");
    let stdout = String::from_utf8(out.stdout).expect("utf8");
    assert!(stdout.contains("migrated"), "{stdout}");

    let doc: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(cursor_dir.join("mcp.json")).expect("json"))
            .expect("valid json");
    assert_eq!(doc["mcpServers"]["github"]["command"], "gh");
    assert_eq!(
        doc["mcpServers"]["nika"]["args"],
        serde_json::json!(["mcp"])
    );
    let _ = std::fs::remove_dir_all(&home);
}

/// Variadic `check` through the real binary — the pre-commit shape:
/// a broken file in the MIDDLE exits 2 while the file after it still
/// audits, and `--json` with several files refuses (exit 3, teach line)
/// because `report_version: 1` is a per-file contract.
#[test]
fn check_many_files_keeps_worst_exit_and_json_stays_single() {
    let dir = workspace_tmp_dir("nika-check-many-smoke");
    let clean =
        "nika: ok\ntasks:\n  t:\n    infer: { prompt: hi, max_tokens: 10, model: \"mock/echo\" }\n";
    let broken = "nika: bad\ntasks:\n  t:\n    infer: { prompt: \"${{ tasks.ghost.output }}\", max_tokens: 10, model: \"mock/echo\" }\n";
    let a = dir.join("a.nika.yaml");
    let b = dir.join("broken.nika.yaml");
    let c = dir.join("c.nika.yaml");
    std::fs::write(&a, clean).expect("fixture a");
    std::fs::write(&b, broken).expect("fixture b");
    std::fs::write(&c, clean).expect("fixture c");

    let out = bin()
        .arg("check")
        .args([&a, &b, &c])
        .output()
        .expect("binary runs");
    assert_eq!(out.status.code(), Some(2), "worst exit survives");
    let stdout = String::from_utf8(out.stdout).expect("utf8");
    let tail = stdout
        .split_once("broken.nika.yaml")
        .map(|s| s.1)
        .expect("broken report present");
    assert!(
        tail.contains("c.nika.yaml"),
        "the file after the failure still audited: {stdout}"
    );

    let refuse = bin()
        .arg("check")
        .args([&a, &c])
        .arg("--json")
        .output()
        .expect("binary runs");
    assert_eq!(
        refuse.status.code(),
        Some(3),
        "invocation error, no file judged"
    );
    let msg = String::from_utf8(refuse.stderr).expect("utf8");
    assert!(
        msg.contains("ONE file per call"),
        "the refusal teaches: {msg}"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

/// #384 · the wave-2 targets through the real binary: gemini + lmstudio
/// resolve under HOME, junie under the project `--dir`.
#[test]
fn wire_wave2_targets_create_their_configs() {
    let home = workspace_tmp_dir("nika-wire-wave2");
    std::fs::create_dir_all(home.join(".lmstudio")).expect("lmstudio root");
    let project = home.join("project");
    std::fs::create_dir_all(&project).expect("project dir");

    for target in ["gemini", "lmstudio", "junie"] {
        let out = bin()
            .arg("wire")
            .arg(target)
            .arg("--dir")
            .arg(&project)
            .env("HOME", &home)
            .output()
            .expect("binary runs");
        assert_eq!(out.status.code(), Some(0), "wire {target} succeeds");
        let stdout = String::from_utf8(out.stdout).expect("utf8");
        assert!(stdout.contains("created"), "{target}: {stdout}");
    }

    for config in [
        home.join(".gemini").join("settings.json"),
        home.join(".lmstudio").join("mcp.json"),
        project.join(".junie").join("mcp").join("mcp.json"),
    ] {
        let doc: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(&config).expect("config written"))
                .expect("valid json");
        assert_eq!(
            doc["mcpServers"]["nika"]["command"],
            "nika",
            "{}",
            config.display()
        );
        assert_eq!(
            doc["mcpServers"]["nika"]["args"],
            serde_json::json!(["mcp"]),
            "{}",
            config.display()
        );
    }
    let _ = std::fs::remove_dir_all(&home);
}

/// Client-doors W1.2 · the wave-4 targets through the real binary: grok
/// writes the Codex-shaped TOML table under `~/.grok/`, antigravity the
/// standalone `mcp_config.json` under `~/.gemini/config/`.
#[test]
fn wire_wave4_targets_create_their_configs() {
    let home = workspace_tmp_dir("nika-wire-wave4");
    std::fs::create_dir_all(&home).expect("home dir");

    for target in ["grok", "antigravity", "kimi", "kiro", "copilot", "amp"] {
        let out = bin()
            .arg("wire")
            .arg(target)
            .env("HOME", &home)
            .output()
            .expect("binary runs");
        assert_eq!(out.status.code(), Some(0), "wire {target} succeeds");
        let stdout = String::from_utf8(out.stdout).expect("utf8");
        assert!(stdout.contains("created"), "{target}: {stdout}");
    }

    let grok = std::fs::read_to_string(home.join(".grok").join("config.toml"))
        .expect("grok config written");
    assert!(grok.contains("[mcp_servers.nika]"), "{grok}");
    assert!(grok.contains("command = \"nika\""), "{grok}");

    let agy: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string(home.join(".gemini").join("config").join("mcp_config.json"))
            .expect("antigravity config written"),
    )
    .expect("valid json");
    assert_eq!(agy["mcpServers"]["nika"]["command"], "nika");
    assert_eq!(
        agy["mcpServers"]["nika"]["args"],
        serde_json::json!(["mcp"])
    );

    for config in [
        home.join(".kimi-code").join("mcp.json"),
        home.join(".kiro").join("settings").join("mcp.json"),
        home.join(".copilot").join("mcp-config.json"),
    ] {
        let doc: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(&config).expect("config written"))
                .expect("valid json");
        assert_eq!(
            doc["mcpServers"]["nika"]["command"],
            "nika",
            "{}",
            config.display()
        );
    }
    let _ = std::fs::remove_dir_all(&home);
}

#[test]
fn mcp_serves_initialize_and_lists_tools() {
    use std::process::Stdio;
    let mut child = bin()
        .arg("mcp")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .expect("spawn nika mcp");
    {
        let mut stdin = child.stdin.take().expect("stdin");
        stdin
            .write_all(b"{\"jsonrpc\":\"2.0\",\"id\":1,\"method\":\"initialize\",\"params\":{}}\n")
            .expect("write initialize");
        stdin
            .write_all(b"{\"jsonrpc\":\"2.0\",\"id\":2,\"method\":\"tools/list\"}\n")
            .expect("write tools/list");
        // Drop stdin → EOF → the server shuts down cleanly (exit 0).
    }
    let out = child.wait_with_output().expect("wait");
    assert!(out.status.success(), "clean EOF shutdown");
    let stdout = String::from_utf8(out.stdout).expect("utf8");
    assert!(
        stdout.contains("\"protocolVersion\""),
        "initialize replied: {stdout}"
    );
    assert!(
        stdout.contains("nika_check") && stdout.contains("nika_explain"),
        "tools/list named the catalog: {stdout}"
    );
}

// ─── stdin (`-`) · the editor wire without the tmp-file dance ────────────────

/// `nika check - --json` reads the workflow from stdin: exit 0 + clean
/// report for a valid doc — the seam every `load_checked` verb inherits.
#[test]
fn check_dash_reads_stdin_valid() {
    use std::process::Stdio;
    let mut child = bin()
        .args(["check", "-", "--json"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .expect("spawn");
    child
        .stdin
        .take()
        .expect("stdin")
        .write_all(VALID.as_bytes())
        .expect("pipe body");
    let out = child.wait_with_output().expect("wait");
    assert_eq!(out.status.code(), Some(0), "clean stdin doc exits 0");
    let doc: serde_json::Value =
        serde_json::from_slice(&out.stdout).expect("json report on stdout");
    assert_eq!(doc["clean"], true);
}

/// Findings on a stdin doc keep the exit-code contract (`2` = file).
#[test]
fn check_dash_reads_stdin_findings_exit_2() {
    use std::process::Stdio;
    let mut child = bin()
        .args(["check", "-", "--json"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .expect("spawn");
    child
        .stdin
        .take()
        .expect("stdin")
        .write_all(INVALID.as_bytes())
        .expect("pipe body");
    let out = child.wait_with_output().expect("wait");
    assert_eq!(out.status.code(), Some(2), "stdin findings exit 2");
    let doc: serde_json::Value =
        serde_json::from_slice(&out.stdout).expect("json report on stdout");
    assert_eq!(doc["clean"], false);
}

/// `nika inspect - --format json` inherits the dash (`load_checked` seam).
#[test]
fn graph_dash_reads_stdin() {
    use std::process::Stdio;
    let mut child = bin()
        .args(["inspect", "-", "--format", "json"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .expect("spawn");
    child
        .stdin
        .take()
        .expect("stdin")
        .write_all(VALID.as_bytes())
        .expect("pipe body");
    let out = child.wait_with_output().expect("wait");
    assert_eq!(out.status.code(), Some(0), "graph on stdin doc exits 0");
    let doc: serde_json::Value = serde_json::from_slice(&out.stdout).expect("graph json on stdout");
    assert!(doc["nodes"].is_array());
}

/// `nika test -` is REFUSED with guidance (exit 3): the golden lives
/// beside the file, and a dirty doc would re-read consumed stdin.
#[test]
fn test_dash_is_refused_with_guidance() {
    use std::process::Stdio;
    let mut child = bin()
        .args(["test", "-"])
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn");
    child
        .stdin
        .take()
        .expect("stdin")
        .write_all(VALID.as_bytes())
        .expect("pipe body");
    let out = child.wait_with_output().expect("wait");
    assert_eq!(out.status.code(), Some(3), "refused as environment");
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(
        err.contains("golden lives beside the file"),
        "guidance: {err}"
    );
}

// ─── the agent surface, on the real binary ───────────────────────────────────
// tools/list proves the CATALOG; these prove the DISPATCH — an agent's
// actual round-trip: a typo'd workflow in, the did-you-mean out. And the
// LSP's framed wire (Content-Length · the OTHER stdio protocol) end to end:
// initialize → didOpen → published diagnostics → clean shutdown. Both were
// proven by hand against the released 0.95.0 (2026-07-06 e2e sweep); these
// pin that proof to every future binary.

/// `tools/call nika_check` round-trips a typo'd workflow: the reply carries
/// the did-you-mean (the fix an agent applies), and `isError` stays false —
/// findings are CONTENT (the tool ran fine), not a tool failure.
#[test]
fn mcp_tools_call_check_carries_the_did_you_mean() {
    use std::process::Stdio;
    let mut child = bin()
        .arg("mcp")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .expect("spawn nika mcp");
    {
        let mut stdin = child.stdin.take().expect("stdin");
        stdin
            .write_all(b"{\"jsonrpc\":\"2.0\",\"id\":1,\"method\":\"initialize\",\"params\":{}}\n")
            .expect("write initialize");
        let wf = "nika: agent-authored\ntasks:\n  log_it:\n    invoke:\n      tool: \"nika:log\"\n      args:\n        mesage: \"typo'd by the agent\"\n";
        let call = serde_json::json!({
            "jsonrpc": "2.0", "id": 2, "method": "tools/call",
            "params": { "name": "nika_check", "arguments": { "workflow": wf } }
        });
        stdin
            .write_all(format!("{call}\n").as_bytes())
            .expect("write tools/call");
    }
    let out = child.wait_with_output().expect("wait");
    assert!(out.status.success(), "clean EOF shutdown");
    let stdout = String::from_utf8(out.stdout).expect("utf8");
    let reply = stdout
        .lines()
        .find(|l| l.contains("\"id\":2"))
        .expect("tools/call reply");
    let doc: serde_json::Value = serde_json::from_str(reply).expect("json reply");
    assert_eq!(
        doc["result"]["isError"], true,
        "a dirty check is isError:true so the agent's repair loop fires \
         (mirrors the CLI's exit-2-on-dirty): {reply}"
    );
    let text = doc["result"]["content"][0]["text"].as_str().expect("text");
    assert!(
        text.contains("mesage") && text.contains("message"),
        "the did-you-mean still rides the reply so the agent repairs: {text}"
    );
}

/// One LSP-framed message (`Content-Length` header + JSON body).
fn lsp_frame(body: &serde_json::Value) -> Vec<u8> {
    let b = body.to_string();
    format!("Content-Length: {}\r\n\r\n{b}", b.len()).into_bytes()
}

/// `nika lsp` over real stdio: initialize advertises the v0.1 quartet,
/// didOpen publishes diagnostics (hint-tier parity with `nika check`),
/// and shutdown → exit ends the process with code 0.
#[test]
fn lsp_serves_initialize_diagnostics_and_clean_exit() {
    use std::process::Stdio;
    let mut child = bin()
        .arg("lsp")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .expect("spawn nika lsp");
    {
        let mut stdin = child.stdin.take().expect("stdin");
        let msgs = [
            serde_json::json!({"jsonrpc":"2.0","id":1,"method":"initialize",
                "params":{"processId":null,"rootUri":null,"capabilities":{}}}),
            serde_json::json!({"jsonrpc":"2.0","method":"initialized","params":{}}),
            serde_json::json!({"jsonrpc":"2.0","method":"textDocument/didOpen",
                "params":{"textDocument":{"uri":"file:///t/probe.nika.yaml",
                    "languageId":"nika","version":1,"text":VALID}}}),
            serde_json::json!({"jsonrpc":"2.0","id":2,"method":"shutdown","params":null}),
            serde_json::json!({"jsonrpc":"2.0","method":"exit"}),
        ];
        for m in &msgs {
            stdin.write_all(&lsp_frame(m)).expect("write frame");
        }
    }
    let out = child.wait_with_output().expect("wait");
    assert_eq!(out.status.code(), Some(0), "shutdown → exit is code 0");
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        stdout.contains("\"hoverProvider\":true") && stdout.contains("completionProvider"),
        "initialize advertises the surface: {stdout}"
    );
    assert!(
        stdout.contains("textDocument/publishDiagnostics"),
        "didOpen published diagnostics: {stdout}"
    );
}

/// The mirror renders offline, exits 0, and never leaks a key VALUE —
/// canary env vars seeded here must be absent from every output byte
/// (presence may be COUNTED, the value must not exist in the render).
#[test]
fn welcome_greets_offline_and_leaks_no_secret() {
    let canary = "hunter2-THE-CANARY-VALUE-nobody-prints";
    // An EMPTY temp dir pins the stranger state deterministically — the
    // start-here block is contextual now (0 workflows → the mock/echo
    // proof line leads; in this repo's own cwd it would say `context`).
    let dir = std::env::temp_dir().join(format!("nika-welcome-smoke-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("mkdir");
    let out = bin()
        .arg("welcome")
        .current_dir(&dir)
        .env("ANTHROPIC_API_KEY", canary)
        .env("OPENAI_API_KEY", canary)
        .stdin(std::process::Stdio::null())
        .output()
        .expect("binary runs");
    assert_eq!(out.status.code(), Some(0), "a greeting is never a failure");
    let text = format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    for needle in ["Local first", "Next:", "Nika Gear One"] {
        assert!(text.contains(needle), "welcome carries `{needle}`: {text}");
    }
    assert!(
        !text.contains(canary),
        "a key VALUE must never surface: {text}"
    );

    // The machine mirror: versioned, parseable, value-free.
    let json_out = bin()
        .arg("welcome")
        .arg("--json")
        .env("ANTHROPIC_API_KEY", canary)
        .stdin(std::process::Stdio::null())
        .output()
        .expect("binary runs");
    assert_eq!(json_out.status.code(), Some(0));
    let raw = String::from_utf8_lossy(&json_out.stdout);
    let v: serde_json::Value = serde_json::from_str(raw.trim()).expect("welcome --json parses");
    assert_eq!(v["welcome_version"], 1);
    assert!(
        v.get("inference_choice").is_some(),
        "cascade rides welcome --json"
    );
    assert!(!raw.contains(canary), "no value in the JSON mirror: {raw}");
}

/// `explain <file>` narrates a checked workflow end to end at the binary
/// plane — and the SAME positional still teaches error codes.
#[test]
fn explain_narrates_a_file_and_still_teaches_codes() {
    let dir = std::env::temp_dir().join(format!("nika-smoke-explain-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("mkdir");
    let wf = write_fixture(
        &dir,
        "story.nika.yaml",
        "nika: smoke-story\n\nmodel: mock/echo\n\ntasks:\n  draft:\n    infer: { prompt: \"draft\", max_tokens: 10 }\n  polish:\n    after:\n      draft: success\n    infer: { prompt: \"polish\", max_tokens: 10 }\noutputs:\n  result: ${{ tasks.polish.output }}\n",
    );
    let out = bin()
        .arg("explain")
        .arg(&wf)
        .stdin(std::process::Stdio::null())
        .output()
        .expect("binary runs");
    let text = String::from_utf8_lossy(&out.stdout);
    assert_eq!(out.status.code(), Some(0), "clean file narrates: {text}");
    for needle in [
        "smoke-story",
        "the story",
        "cost before a token is spent",
        "run it",
        "flight recorder",
    ] {
        assert!(text.contains(needle), "explain carries `{needle}`: {text}");
    }

    // The code form is untouched by the overload.
    let code_out = bin()
        .arg("explain")
        .arg("NIKA-DAG-003")
        .stdin(std::process::Stdio::null())
        .output()
        .expect("binary runs");
    assert_eq!(code_out.status.code(), Some(0));
    assert!(
        String::from_utf8_lossy(&code_out.stdout).contains("NIKA-DAG-003"),
        "codes still teach"
    );

    // The machine twin parses and speaks the report dialect.
    let json_out = bin()
        .arg("explain")
        .arg(&wf)
        .arg("--json")
        .stdin(std::process::Stdio::null())
        .output()
        .expect("binary runs");
    assert_eq!(json_out.status.code(), Some(0));
    let v: serde_json::Value =
        serde_json::from_str(String::from_utf8_lossy(&json_out.stdout).trim())
            .expect("explain --json parses");
    assert_eq!(v["explain_version"], 1);
    assert_eq!(v["clean"], true);
    let _ = std::fs::remove_dir_all(&dir);
}

/// `init` scaffolds the per-client briefs alongside the contract — and
/// the briefs route back to AGENTS.md instead of forking the truth.
#[test]
fn init_scaffolds_the_client_briefs() {
    let dir = std::env::temp_dir().join(format!("nika-smoke-init6-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("mkdir");
    let out = bin()
        .arg("init")
        .arg(&dir)
        .arg("--yes")
        .stdin(std::process::Stdio::null())
        .output()
        .expect("binary runs");
    assert_eq!(out.status.code(), Some(0));
    for rel in [".github/copilot-instructions.md", "CLAUDE.md"] {
        let body = std::fs::read_to_string(dir.join(rel)).expect(rel);
        assert!(body.contains("AGENTS.md"), "{rel} routes to the contract");
        assert!(body.contains("nika check"), "{rel} teaches the loop");
    }
    let _ = std::fs::remove_dir_all(&dir);
}

/// The terminal DAG reaches both surfaces: `graph --format ascii` draws
/// real wires for a diamond, and `explain <file>` opens with the shape.
#[test]
fn the_dag_draws_in_the_terminal() {
    let dir = std::env::temp_dir().join(format!("nika-smoke-wires-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("mkdir");
    let wf = write_fixture(
        &dir,
        "diamond.nika.yaml",
        "nika: smoke-diamond\nmodel: mock/echo\ntasks:\n  fetch:\n    infer: { prompt: \"g\", max_tokens: 10 }\n  sum:\n    after:\n      fetch: success\n    infer: { prompt: \"s\", max_tokens: 10 }\n  crit:\n    after:\n      fetch: success\n    infer: { prompt: \"c\", max_tokens: 10 }\n  publish:\n    after:\n      sum: success\n      crit: success\n    infer: { prompt: \"p\", max_tokens: 10 }\n",
    );
    let out = bin()
        .arg("inspect")
        .arg(&wf)
        .arg("--format")
        .arg("ascii")
        .stdin(std::process::Stdio::null())
        .output()
        .expect("binary runs");
    let text = String::from_utf8_lossy(&out.stdout);
    assert_eq!(out.status.code(), Some(0), "{text}");
    assert!(
        text.contains("fetch ─┬─▶ ◇ sum") && text.contains("╰─▶ ◇ crit"),
        "the diamond draws real wires with verb chips: {text}"
    );

    let out = bin()
        .arg("explain")
        .arg(&wf)
        .stdin(std::process::Stdio::null())
        .output()
        .expect("binary runs");
    let text = String::from_utf8_lossy(&out.stdout);
    assert_eq!(out.status.code(), Some(0), "{text}");
    assert!(
        text.contains("the shape") && text.contains("─▶"),
        "explain opens with the drawing: {text}"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

/// `nika welcome --deep` — the workspace aggregate at the binary plane: the
/// inventory audits, the wire versions, relative paths only, and the
/// canary key VALUE never reaches a byte (the aggregate reads the SAME
/// presence-only probe as welcome/doctor).
#[test]
fn context_aggregates_the_workspace_value_free() {
    let canary = "hunter2-CONTEXT-CANARY-never-printed";
    let dir = std::env::temp_dir().join(format!("nika-smoke-context-{}", std::process::id()));
    std::fs::create_dir_all(dir.join("flows")).expect("mkdir");
    write_fixture(
        &dir,
        "good.nika.yaml",
        "nika: smoke-good\nmodel: mock/echo\ntasks:\n  a:\n    infer: { prompt: \"x\", max_tokens: 10 }\n",
    );
    write_fixture(
        &dir.join("flows"),
        "bad.nika.yaml",
        "nika: smoke-bad\ntasks:\n  a:\n    exec: { command: [\"echo\", \"x\"] }\n  b:\n    after:\n      a: success\n    when: maybe\n    exec: { command: [\"echo\", \"y\"] }\n",
    );
    let out = bin()
        .args(["welcome", "--deep"])
        .arg("--json")
        .current_dir(&dir)
        .env("ANTHROPIC_API_KEY", canary)
        .stdin(std::process::Stdio::null())
        .output()
        .expect("binary runs");
    let raw = String::from_utf8_lossy(&out.stdout);
    assert_eq!(out.status.code(), Some(0), "{raw}");
    assert!(
        !raw.contains(canary),
        "no key VALUE in the aggregate: {raw}"
    );
    let v: serde_json::Value = serde_json::from_str(raw.trim()).expect("context --json parses");
    assert_eq!(v["context_version"], 1);
    let flows = v["workspace"]["workflows"].as_array().expect("array");
    assert_eq!(flows.len(), 2, "{raw}");
    assert!(
        flows
            .iter()
            .all(|f| !f["path"].as_str().unwrap_or("/").starts_with('/')),
        "relative paths only: {raw}"
    );
    assert_eq!(v["rollups"]["workflows_clean"], 1);
    assert_eq!(v["rollups"]["workflows_with_findings"], 1);

    // The human map renders both rows and hands over to the twin.
    let human = bin()
        .args(["welcome", "--deep"])
        .current_dir(&dir)
        .stdin(std::process::Stdio::null())
        .output()
        .expect("binary runs");
    let text = String::from_utf8_lossy(&human.stdout);
    assert_eq!(human.status.code(), Some(0), "{text}");
    assert!(
        text.contains("good.nika.yaml") && text.contains("clean"),
        "{text}"
    );
    assert!(text.contains("nika welcome --deep --json"), "{text}");
    let _ = std::fs::remove_dir_all(&dir);
}

// -- registry refs (issue #452) — the NETWORK-FREE half of the binary
// contract: ref-shape refusals and the --fix guard fire at parse time,
// before any resolution, so these tests never touch the network.

#[test]
fn registry_ref_that_cannot_parse_exits_env_and_teaches_the_form() {
    // check side: owner is required in v1 — the refusal must teach the form.
    let out = bin()
        .args(["check", "registry:just-a-name"])
        .output()
        .expect("binary runs");
    assert_eq!(
        out.status.code(),
        Some(3),
        "a ref that cannot parse is an environment error, not a file finding"
    );
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(
        err.contains("registry:owner/name"),
        "the refusal teaches the form · stderr: {err}"
    );

    // run side rides the same seam.
    let out = bin()
        .args(["run", "registry:acme/Bad_Name"])
        .output()
        .expect("binary runs");
    assert_eq!(out.status.code(), Some(3));
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(err.contains("registry:owner/name"), "stderr: {err}");
}

#[test]
fn fix_refuses_registry_refs_before_any_network() {
    // --fix rewrites a file; a registry artifact is pinned by its digest —
    // editing the cache would poison it. Refused at arg-handling, network-free.
    let out = bin()
        .args(["check", "--fix", "registry:acme/greet"])
        .output()
        .expect("binary runs");
    assert_eq!(out.status.code(), Some(3));
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(
        err.contains("digest") && err.contains("copy"),
        "teaches WHY and the workspace-copy fix · stderr: {err}"
    );
}

#[test]
fn help_teaches_the_registry_form_on_check_and_run() {
    for verb in ["check", "run"] {
        let out = bin().args([verb, "--help"]).output().expect("binary runs");
        let text = String::from_utf8_lossy(&out.stdout);
        assert!(
            text.contains("registry:owner/name"),
            "`nika {verb} --help` teaches the registry ref · got: {text}"
        );
        assert!(
            text.contains("permits"),
            "`nika {verb} --help` says permits do not govern the fetch · got: {text}"
        );
    }
}

/// Gauntlet 2026-07-31: bare `nika` piped = welcome mirror, exit 0
/// (§4 reserves 2 for FILE findings · `--help` stays the reference).
#[test]
fn bare_nika_greets_and_exits_zero_in_a_pipe() {
    let out = bin().output().expect("binary runs");
    assert_eq!(out.status.code(), Some(0), "a greeting, not a finding");
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        stdout.contains("Local first") && stdout.contains("Next:"),
        "the cascade names the product and the next door:\n{stdout}"
    );
}

#[test]
fn nika_json_on_the_front_door_is_the_cascade_not_a_clap_fail() {
    let out = bin().arg("--json").output().expect("binary runs");
    assert_eq!(
        out.status.code(),
        Some(0),
        "nika --json greets: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let raw = String::from_utf8_lossy(&out.stdout);
    let v: serde_json::Value = serde_json::from_str(raw.trim()).expect("front door --json parses");
    assert_eq!(v["welcome_version"], 1);
    assert!(v.get("inference_choice").is_some(), "{raw}");
}

#[test]
fn nika_version_equals_dash_dash_version() {
    let a = bin().arg("version").output().expect("version");
    let b = bin().arg("--version").output().expect("--version");
    assert_eq!(a.status.code(), Some(0));
    assert_eq!(b.status.code(), Some(0));
    let ta = String::from_utf8_lossy(&a.stdout);
    let tb = String::from_utf8_lossy(&b.stdout);
    assert!(!ta.trim().is_empty());
    assert_eq!(ta.trim(), tb.trim(), "nika version == nika --version");
}

#[test]
fn default_help_is_at_most_six_lines() {
    let out = bin().arg("--help").output().expect("help");
    assert_eq!(out.status.code(), Some(0));
    let text = String::from_utf8_lossy(&out.stdout);
    let n = text.lines().filter(|l| !l.is_empty()).count();
    assert!(n <= 6, "human help ≤ 6 lines, got {n}:\n{text}");
}

/// A clean machine (`HOME` empty · no vendor keys) still gets a file
/// that parses, names no dead vendor, and points at `nika model pull`.
#[test]
fn new_hello_writes_the_first_wow_file() {
    let dir = workspace_tmp_dir("nika-new-hello");
    let mut cmd = bin();
    cmd.args(["new", "hello"])
        .current_dir(&dir)
        .env("HOME", &dir)
        .env_remove("ANTHROPIC_API_KEY")
        .env_remove("OPENAI_API_KEY")
        .env_remove("XAI_API_KEY")
        .env_remove("MISTRAL_API_KEY")
        .env_remove("HF_TOKEN")
        .env_remove("GEMINI_API_KEY")
        .stdin(std::process::Stdio::null());
    let out = cmd.output().expect("binary runs");
    let stdout = String::from_utf8_lossy(&out.stdout);
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert_eq!(
        out.status.code(),
        Some(0),
        "nika new hello must write: {stdout}{stderr}"
    );
    let dest = dir.join("hello.nika.yaml");
    assert!(
        dest.is_file(),
        "hello.nika.yaml landed in {}",
        dir.display()
    );
    let body = std::fs::read_to_string(&dest).expect("body");
    assert!(body.contains("nika: hello"), "{body}");
    assert!(!body.contains("nika: v1"), "{body}");
    assert!(!body.contains("ollama/"), "{body}");
    assert!(!body.contains("xai/grok-4"), "{body}");
    assert!(body.contains("infer:") || body.contains("agent:"), "{body}");
    assert!(
        stdout.contains("wrote"),
        "receipt names the write: {stdout}"
    );
    assert!(
        stdout.contains("nika run") || stdout.contains("nika model pull"),
        "receipt names the next door: {stdout}"
    );

    let check = bin()
        .args(["check", "hello.nika.yaml"])
        .current_dir(&dir)
        .env("HOME", &dir)
        .stdin(std::process::Stdio::null())
        .output()
        .expect("check runs");
    assert_eq!(
        check.status.code(),
        Some(0),
        "first-wow must check clean: {}{}",
        String::from_utf8_lossy(&check.stdout),
        String::from_utf8_lossy(&check.stderr)
    );
    let _ = std::fs::remove_dir_all(&dir);
}
