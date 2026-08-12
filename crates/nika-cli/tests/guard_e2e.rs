// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>
#![allow(clippy::expect_used, clippy::panic)]
// This suite executes the real `nika-cli` binary (CARGO_BIN_EXE) — its
// whole job is the binary contract, so it spawns processes (the same
// sanctioned carve-out as bin_smoke.rs / resume_e2e.rs).
#![allow(clippy::disallowed_types)]

//! `nika guard --stdin` at the REAL binary — the hook's judge over the
//! wire the shims actually speak. The judge's matrix (bypasses ·
//! fail-open cohort · dialect sniffs) lives at the lib seam in
//! `src/verbs/guard/tests.rs`; this suite pins the four shapes the HOST
//! meets end to end: a deny in each dialect, the clean allow, and the
//! ghost-path `guard_unavailable` — exit codes included (0 · 2 · 3).

use std::io::Write as _;
use std::process::Command;

fn bin() -> Command {
    Command::new(env!("CARGO_BIN_EXE_nika-cli"))
}

/// The clean shape (the same one-task infer the guard's lib matrix
/// audits) — unpriced, so the allow carries no cap demand.
const GOOD: &str =
    "nika: good\nmodel: mock/echo\ntasks:\n  a:\n    infer: { prompt: \"x\", max_tokens: 10 }\n";

/// The consent violation (NEP-0020 · spec conformance
/// `core/policy/011-consent-bare-after-refused`): a bare `after:` edge
/// carries a REFUSED confirm straight into the irreversible exec —
/// NIKA-SEC-014, red.
const DIRTY: &str = "nika: dirty\npermits:\n  exec: [\"git\"]\n  tools: [\"nika:prompt\"]\ntasks:\n  ask:\n    invoke:\n      tool: \"nika:prompt\"\n      args: { mode: confirm, message: \"push?\", default: false }\n  push:\n    after: { ask: success }\n    exec: { command: [\"git\", \"push\"] }\n";

/// Pipe `payload` to `guard --stdin`; return (stdout, exit code).
fn guard_stdin(payload: &str) -> (String, i32) {
    let mut child = bin()
        .arg("guard")
        .arg("--stdin")
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::null())
        .spawn()
        .expect("guard spawns");
    child
        .stdin
        .as_mut()
        .expect("stdin")
        .write_all(payload.as_bytes())
        .expect("payload written");
    let out = child.wait_with_output().expect("guard completes");
    (
        String::from_utf8(out.stdout).expect("utf8 stdout"),
        out.status.code().unwrap_or(-1),
    )
}

/// Claude Code dialect: a dirty run denies on `hookSpecificOutput`,
/// exit 2 (a FILE finding), the reason voicing the consent code.
#[test]
fn claude_payload_denies_a_consent_dirty_run() {
    let dir = tempfile::tempdir().expect("dir");
    let dirty = dir.path().join("dirty.nika.yaml");
    std::fs::write(&dirty, DIRTY).expect("fixture");
    let payload = format!(
        r#"{{"hook_event_name":"PreToolUse","tool_input":{{"command":"nika run {}"}},"cwd":"{}"}}"#,
        dirty.display(),
        dir.path().display()
    );
    let (stdout, rc) = guard_stdin(&payload);
    assert_eq!(rc, 2, "a file finding exits 2: {stdout}");
    assert!(
        stdout.contains(r#""permissionDecision":"deny""#),
        "the Claude deny envelope: {stdout}"
    );
    assert!(
        stdout.contains("NIKA-SEC-014"),
        "the refusal voices the consent law: {stdout}"
    );
}

/// Cursor dialect: the same judgement on the generic permission
/// envelope — exit 2, `"permission":"deny"`.
#[test]
fn cursor_payload_denies_a_consent_dirty_run() {
    let dir = tempfile::tempdir().expect("dir");
    let dirty = dir.path().join("dirty.nika.yaml");
    std::fs::write(&dirty, DIRTY).expect("fixture");
    let payload = format!(
        r#"{{"command":"nika run {}","cwd":"{}"}}"#,
        dirty.display(),
        dir.path().display()
    );
    let (stdout, rc) = guard_stdin(&payload);
    assert_eq!(rc, 2, "a file finding exits 2: {stdout}");
    assert!(
        stdout.contains(r#""permission":"deny""#),
        "the Cursor deny envelope: {stdout}"
    );
    assert!(
        stdout.contains("NIKA-SEC-014"),
        "the refusal voices the consent law: {stdout}"
    );
}

/// The clean run flows: exit 0, the Claude no-opinion `{}` (the hook
/// teaches, it never widens the user's own permission flow).
#[test]
fn claude_payload_allows_a_clean_run() {
    let dir = tempfile::tempdir().expect("dir");
    let good = dir.path().join("good.nika.yaml");
    std::fs::write(&good, GOOD).expect("fixture");
    let payload = format!(
        r#"{{"hook_event_name":"PreToolUse","tool_input":{{"command":"nika run {}"}},"cwd":"{}"}}"#,
        good.display(),
        dir.path().display()
    );
    let (stdout, rc) = guard_stdin(&payload);
    assert_eq!(rc, 0, "a judged-clean run flows: {stdout}");
    assert_eq!(stdout.trim(), "{}", "the no-opinion pass: {stdout}");
}

/// A ghost path is an ENVIRONMENT failure (exit 3) — deny-shaped with
/// the degradation named, never a silent allow (the P0-15 class).
#[test]
fn ghost_path_is_a_visible_guard_unavailable() {
    let dir = tempfile::tempdir().expect("dir");
    let ghost = dir.path().join("ghost.nika.yaml");
    let payload = format!(
        r#"{{"hook_event_name":"PreToolUse","tool_input":{{"command":"nika run {}"}},"cwd":"{}"}}"#,
        ghost.display(),
        dir.path().display()
    );
    let (stdout, rc) = guard_stdin(&payload);
    assert_eq!(rc, 3, "the environment failed the judge: {stdout}");
    assert!(
        stdout.contains(r#""permissionDecision":"deny""#),
        "unavailable is deny-SHAPED: {stdout}"
    );
    assert!(
        stdout.contains("guard_unavailable"),
        "the degradation is named: {stdout}"
    );
}
