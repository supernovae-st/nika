// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>
#![allow(clippy::expect_used, clippy::panic)]
// The workspace bans std::process::Command (production spawns ride the
// kernel ShellExecutor seam). This test's WHOLE JOB is to execute the
// real `nika-cli` binary (CARGO_BIN_EXE) — routing through the kernel
// seam would test the lib, not the binary contract. The one legitimate
// carve-out class (same exemption as a future assert_cmd harness).
#![allow(clippy::disallowed_types)]

//! Binary smoke — the REAL `nika-cli` executable through its locked
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
    Command::new(env!("CARGO_BIN_EXE_nika-cli"))
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
nika: v1
workflow: smoke
tasks:
  - id: greet
    exec: { command: "echo hello" }
"#;

const INVALID: &str = r#"
nika: v1
workflow: smoke-broken
tasks:
  - id: a
    depends_on: [b]
    exec: { command: "true" }
  - id: b
    depends_on: [a]
    exec: { command: "true" }
"#;

const FAILING: &str = r#"
nika: v1
workflow: smoke-fail
tasks:
  - id: boom
    exec: { command: "exit 7" }
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
        .arg("--no-color")
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
    // The render/plan flags are HUMAN surfaces · clap REFUSES them alongside
    // the `--json`/`--output json` machine modes, so a parsed-but-ignored flag
    // can never silently corrupt the `capture: stdout` JSON contract.
    let dir = std::env::temp_dir().join(format!("nika-bin-conflict-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("tmp dir");
    let wf = write_fixture(&dir, "ok.nika.yaml", VALID);

    for human in ["--dry-run", "--quiet", "--no-progress"] {
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
    // Re-run · the human keeps the hand · existing files are SKIPPED, exit 0.
    let again = bin().arg("init").arg(&dir).output().expect("binary runs");
    assert_eq!(again.status.code(), Some(0));
    let stdout = String::from_utf8(again.stdout).expect("utf8");
    assert!(
        stdout.contains("skipped"),
        "idempotent re-run skips: {stdout}"
    );
    let _ = std::fs::remove_dir_all(&dir);
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

/// `nika graph - --format json` inherits the dash (`load_checked` seam).
#[test]
fn graph_dash_reads_stdin() {
    use std::process::Stdio;
    let mut child = bin()
        .args(["graph", "-", "--format", "json"])
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
