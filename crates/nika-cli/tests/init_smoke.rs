// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! `nika init` founding-surface smoke — the REAL binary over the
//! scriptable twins (`--recipe` · `--theme`) and the byte-stability law
//! (bare `--yes` = the historical report exactly). Split from
//! `bin_smoke.rs` under the 1500-line file law.

#![allow(clippy::expect_used, clippy::panic, clippy::disallowed_types)]

use std::path::PathBuf;
use std::process::Command;

/// The compiled binary under test (the cargo-provided path).
fn bin() -> Command {
    Command::new(env!("CARGO_BIN_EXE_nika-cli"))
}

/// A unique scratch dir per test-process (workspace-independent).
fn workspace_tmp_dir(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("mkdir");
    dir
}

#[test]
fn init_recipe_scaffolds_the_curriculum_and_audits_it() {
    let dir = workspace_tmp_dir("nika-init-recipe-smoke");
    let out = bin()
        .arg("init")
        .arg(&dir)
        .arg("--yes")
        .arg("--recipe")
        .arg("agentic")
        .arg("--theme")
        .arg("nika")
        .stdin(std::process::Stdio::null())
        .output()
        .expect("binary runs");
    assert_eq!(out.status.code(), Some(0), "founding succeeds");
    let stdout = String::from_utf8(out.stdout).expect("utf8");
    // The 4-pattern curriculum lands on disk…
    for rel in [
        "workflows/01-hello-chain.nika.yaml",
        "workflows/02-parallel-fanout.nika.yaml",
        "workflows/03-gated-ship.nika.yaml",
        "workflows/04-agent-loop.nika.yaml",
    ] {
        assert!(dir.join(rel).is_file(), "{rel} written: {stdout}");
    }
    // …each one audited on the spot (audit-before-run in the first
    // minute — the receipts say so, one per workflow)…
    assert!(
        stdout.matches("audited").count() >= 4,
        "every workflow audited: {stdout}"
    );
    // …the canvas theme is a REAL stamp in the settings JSON…
    let settings =
        std::fs::read_to_string(dir.join(".vscode/settings.json")).expect("settings written");
    let parsed: serde_json::Value = serde_json::from_str(&settings).expect("valid json");
    assert_eq!(
        parsed.get("nika.dag.theme").and_then(|v| v.as_str()),
        Some("nika"),
        "the DAG skin persisted"
    );
    // …and the hand-off names the FIRST scaffolded workflow.
    assert!(
        stdout.contains("nika run workflows/01-hello-chain.nika.yaml --model mock/echo"),
        "the next block is tailored: {stdout}"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn init_plain_yes_keeps_the_historical_bytes() {
    // The byte-stability law: `--yes` with ZERO new flags must render
    // the exact pre-wizard shape (report rows + the classic next block)
    // — scripts have parsed it since #158.
    let dir = workspace_tmp_dir("nika-init-stable-smoke");
    let out = bin()
        .arg("init")
        .arg(&dir)
        .arg("--yes")
        .stdin(std::process::Stdio::null())
        .output()
        .expect("binary runs");
    assert_eq!(out.status.code(), Some(0));
    let stdout = String::from_utf8(out.stdout).expect("utf8");
    assert!(stdout.contains("✔ created"), "{stdout}");
    assert!(
        stdout.contains("nika examples run 01-hello --model mock/echo"),
        "the classic hand-off survives: {stdout}"
    );
    assert!(
        !stdout.contains("workflows/"),
        "no recipe means no workflow set: {stdout}"
    );
    assert!(!stdout.contains('\x1b'), "piped init stays escape-free");
    let _ = std::fs::remove_dir_all(&dir);
}

/// The lazy-hands resolver: `check`/`run` with NO file — one workflow
/// auto-resolves (announced on stderr, stdout contract untouched),
/// zero routes to the founding trio, several lists copy-paste lines.
#[test]
fn bare_check_and_run_resolve_the_lazy_way() {
    let base = workspace_tmp_dir("nika-lazy-smoke");
    let hello = "nika: v1\nworkflow: solo\nmodel: mock/echo\ntasks:\n  - id: greet\n    infer: { prompt: \"hi\", max_tokens: 9 }\n";

    // ONE workflow → check runs it and says which (stderr).
    let one = base.join("one");
    std::fs::create_dir_all(&one).expect("mkdir");
    std::fs::write(one.join("solo.nika.yaml"), hello).expect("seed");
    let out = bin()
        .arg("check")
        .current_dir(&one)
        .output()
        .expect("binary runs");
    assert_eq!(out.status.code(), Some(0), "auto-resolved audit passes");
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(
        err.contains("solo.nika.yaml (the only workflow here)"),
        "the pick is announced: {err}"
    );
    assert!(
        String::from_utf8_lossy(&out.stdout).contains("audited"),
        "stdout carries the audit only"
    );

    // ZERO → the founding trio, env exit.
    let none = base.join("none");
    std::fs::create_dir_all(&none).expect("mkdir");
    let out = bin()
        .arg("check")
        .current_dir(&none)
        .output()
        .expect("binary runs");
    assert_eq!(out.status.code(), Some(3));
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(err.contains("nika init"), "routes to founding: {err}");

    // MANY → every candidate named, copy-paste ready.
    let many = base.join("many");
    std::fs::create_dir_all(&many).expect("mkdir");
    std::fs::write(many.join("a.nika.yaml"), hello).expect("seed");
    std::fs::write(many.join("b.nika.yaml"), hello).expect("seed");
    let out = bin()
        .arg("run")
        .current_dir(&many)
        .output()
        .expect("binary runs");
    assert_eq!(out.status.code(), Some(3));
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(
        err.contains("nika run a.nika.yaml") && err.contains("nika run b.nika.yaml"),
        "each candidate is a paste-ready command: {err}"
    );
    let _ = std::fs::remove_dir_all(&base);
}
