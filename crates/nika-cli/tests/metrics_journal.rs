// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>
#![allow(clippy::expect_used, clippy::panic)]
// The workspace bans std::process::Command (production spawns ride the
// kernel ShellExecutor seam). This test's WHOLE JOB is to execute the
// real `nika-cli` binary (CARGO_BIN_EXE) — the same carve-out class as
// bin_smoke.rs.
#![allow(clippy::disallowed_types)]

//! The W8 local metrics contract (audit UX 2026-07-30 · P1): the verbs
//! journal their content-free events to `~/.nika/metrics.ndjson` when —
//! and ONLY when — the operator opted in (`NIKA_METRICS=1`). These run
//! the real binary with a scratch HOME and read the journal back:
//!
//! - **off by default** — no env, no journal, not even the directory;
//! - **the wired events land** — `check` green → `check_passed` ·
//!   `welcome` → `context_resolved` + one `cta_impression` per move
//!   (+ `human_run_handoff` when the run CTA leads) · `guard` allow →
//!   `human_run_handoff/guard_allow` · `new <source>` → `draft_created`;
//! - **content-free by construction** — no journal line carries the
//!   scratch path, the workflow id, or anything but the whitelisted
//!   enums/bools/counters.

use std::io::Write as _;
use std::process::Command;

fn bin() -> Command {
    Command::new(env!("CARGO_BIN_EXE_nika-cli"))
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

const CLEAN_MOCK: &str = r#"
nika: metric-probe
model: mock/echo
tasks:
  greet:
    infer: { prompt: "hello", max_tokens: 10 }
"#;

fn write_workflow(dir: &std::path::Path) -> std::path::PathBuf {
    let wf = dir.join("ok.nika.yaml");
    let mut f = std::fs::File::create(&wf).expect("fixture file");
    f.write_all(CLEAN_MOCK.as_bytes()).expect("fixture body");
    wf
}

fn journal_lines(home: &std::path::Path) -> Vec<serde_json::Value> {
    let body = std::fs::read_to_string(home.join(".nika/metrics.ndjson"))
        .expect("the journal exists once opted in");
    body.lines()
        .map(|l| serde_json::from_str(l).expect("each line is json"))
        .collect()
}

/// OFF BY DEFAULT (spec H11 · zero telemetry by default): a green check
/// and a welcome with no opt-in write nothing — not even the directory.
#[test]
fn no_journal_without_the_opt_in() {
    let home = workspace_tmp_dir("nika-metrics-off");
    let proj = home.join("proj");
    std::fs::create_dir_all(&proj).expect("proj dir");
    let wf = write_workflow(&proj);

    let out = bin()
        .arg("check")
        .arg(&wf)
        .env("HOME", &home)
        .env_remove("NIKA_METRICS")
        .output()
        .expect("binary runs");
    assert_eq!(out.status.code(), Some(0), "check is green");
    let out = bin()
        .arg("welcome")
        .current_dir(&proj)
        .env("HOME", &home)
        .env_remove("NIKA_METRICS")
        .output()
        .expect("binary runs");
    assert_eq!(out.status.code(), Some(0));
    assert!(
        !home.join(".nika").exists(),
        "zero telemetry by default — no journal, not even the directory"
    );
    let _ = std::fs::remove_dir_all(&home);
}

/// The wired events land, content-free: a green check in a one-workflow
/// workspace, then the concierge's mirror over the same workspace.
#[test]
fn check_and_welcome_journal_their_events() {
    let home = workspace_tmp_dir("nika-metrics-on");
    let proj = home.join("proj");
    std::fs::create_dir_all(&proj).expect("proj dir");
    std::fs::create_dir(proj.join(".git")).expect("git marker");
    let wf = write_workflow(&proj);

    let out = bin()
        .arg("check")
        .arg(&wf)
        .env("HOME", &home)
        .env("NIKA_METRICS", "1")
        .output()
        .expect("binary runs");
    assert_eq!(out.status.code(), Some(0), "check is green");
    let out = bin()
        .arg("welcome")
        .current_dir(&proj)
        .env("HOME", &home)
        .env("NIKA_METRICS", "1")
        .output()
        .expect("binary runs");
    assert_eq!(out.status.code(), Some(0));

    let events = journal_lines(&home);
    let names: Vec<&str> = events
        .iter()
        .map(|e| e["event"].as_str().expect("event name"))
        .collect();
    assert!(
        names.contains(&"check_passed"),
        "the green check journaled: {names:?}"
    );
    assert!(
        names.contains(&"context_resolved"),
        "the envelope resolution journaled: {names:?}"
    );
    assert!(
        names.iter().filter(|n| **n == "cta_impression").count() >= 3,
        "one impression per shown move: {names:?}"
    );
    // One clean unpriced workflow + no AGENTS.md: the lead move is
    // `nika init` (founding), so the run hand-off does NOT fire here.
    let resolved = events
        .iter()
        .find(|e| e["event"] == "context_resolved")
        .expect("the context event");
    assert_eq!(resolved["facts"]["session"], "workspace");

    // Content-free by construction: nothing in the journal names the
    // workspace, the file, or the workflow id.
    let raw = std::fs::read_to_string(home.join(".nika/metrics.ndjson")).expect("journal");
    for forbidden in [
        home.to_string_lossy().as_ref(),
        proj.to_string_lossy().as_ref(),
        "ok.nika.yaml",
        "metric-probe",
    ] {
        assert!(
            !raw.contains(forbidden),
            "the journal never carries content ({forbidden}): {raw}"
        );
    }
    let _ = std::fs::remove_dir_all(&home);
}

/// The guard's ALLOW is the hook-side half of `human_run_handoff`.
#[test]
fn guard_allow_journals_the_handoff() {
    let home = workspace_tmp_dir("nika-metrics-guard");
    let proj = home.join("proj");
    std::fs::create_dir_all(&proj).expect("proj dir");
    let _wf = write_workflow(&proj);

    let out = bin()
        .arg("guard")
        .arg("--command")
        .arg("nika run ok.nika.yaml")
        .current_dir(&proj)
        .env("HOME", &home)
        .env("NIKA_METRICS", "1")
        .output()
        .expect("binary runs");
    assert_eq!(
        out.status.code(),
        Some(0),
        "a clean unpriced run flows: {}",
        String::from_utf8_lossy(&out.stdout)
    );
    let events = journal_lines(&home);
    let handoff = events
        .iter()
        .find(|e| e["event"] == "human_run_handoff")
        .expect("the allow journaled the handoff");
    assert_eq!(handoff["facts"]["handoff"], "guard_allow");
    let _ = std::fs::remove_dir_all(&home);
}

/// `nika new <example> <dest>` writes the draft and journals it.
#[test]
fn new_journals_the_draft() {
    let home = workspace_tmp_dir("nika-metrics-new");
    let proj = home.join("proj");
    std::fs::create_dir_all(&proj).expect("proj dir");

    let out = bin()
        .arg("new")
        .arg("01-hello")
        .arg("draft.nika.yaml")
        .current_dir(&proj)
        .env("HOME", &home)
        .env("NIKA_METRICS", "1")
        .output()
        .expect("binary runs");
    assert_eq!(
        out.status.code(),
        Some(0),
        "the draft lands: {}",
        String::from_utf8_lossy(&out.stdout)
    );
    assert!(proj.join("draft.nika.yaml").is_file(), "the file exists");
    let events = journal_lines(&home);
    let draft = events
        .iter()
        .find(|e| e["event"] == "draft_created")
        .expect("the write journaled");
    assert_eq!(draft["facts"]["draft"], "new");
    let _ = std::fs::remove_dir_all(&home);
}
