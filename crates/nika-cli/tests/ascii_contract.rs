// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>
#![allow(clippy::expect_used, clippy::panic)]
// The workspace bans std::process::Command (production spawns ride the
// kernel ShellExecutor seam). This test's WHOLE JOB is to execute the
// real `nika-cli` binary (CARGO_BIN_EXE) — the same carve-out class as
// bin_smoke.rs.
#![allow(clippy::disallowed_types)]

//! The `--ascii` byte contract (audit UX 2026-07-30 · P1 « --plain
//! --ascii réellement ASCII »): when the operator asks for the ASCII
//! register, EVERY byte the binary emits is < 128 — the glyph twins
//! (✔/ok · ✖/X · ⚠/!) are the primary mechanism and the render
//! boundary folds the chrome punctuation (· → - · — → -- · → → -> ·
//! ↳ · … · ⭐) so no literal can leak through. `check` and `welcome`
//! are the two surfaces the audit caught; the unicode register keeps
//! its glyphs (the flag is a twin, never a global sobering).

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

/// One clean workflow that still exercises the hint lane (a declared
/// input with no default rides the `↳` HINT marker — the leak class).
const VALID: &str = r#"
nika: ascii-probe
model: mock/echo
inputs:
  topic:
    type: string
    required: true
tasks:
  greet:
    infer: { prompt: "hello ${{ inputs.topic }}", max_tokens: 10 }
"#;

fn assert_all_ascii(what: &str, bytes: &[u8]) {
    if let Some(pos) = bytes.iter().position(|b| !b.is_ascii()) {
        let byte = bytes[pos];
        let text = String::from_utf8_lossy(bytes);
        let line = text
            .lines()
            .find(|l| !l.is_ascii())
            .expect("a non-ascii line");
        panic!("{what}: byte {byte:#04x} at offset {pos} — the row: {line}");
    }
}

/// `nika --ascii check` — every byte of stdout AND stderr is ASCII.
#[test]
fn check_ascii_emits_only_ascii_bytes() {
    let dir = workspace_tmp_dir("nika-ascii-check");
    let wf = dir.join("ok.nika.yaml");
    let mut f = std::fs::File::create(&wf).expect("fixture file");
    f.write_all(VALID.as_bytes()).expect("fixture body");

    let out = bin()
        .arg("--ascii")
        .arg("check")
        .arg(&wf)
        .env("HOME", &dir)
        .output()
        .expect("binary runs");
    assert_all_ascii("check --ascii stdout", &out.stdout);
    assert_all_ascii("check --ascii stderr", &out.stderr);

    // The unicode register keeps its glyphs — the flag is a twin, not a
    // global sobering (a fold that always fires would pass the assertion
    // above while breaking the default surface).
    let uni = bin()
        .arg("check")
        .arg(&wf)
        .args(["--color", "never"])
        .env("HOME", &dir)
        .output()
        .expect("binary runs");
    assert!(
        uni.stdout.iter().any(|b| !b.is_ascii()),
        "the default register keeps the unicode chrome: {}",
        String::from_utf8_lossy(&uni.stdout)
    );
    let _ = std::fs::remove_dir_all(&dir);
}

/// `nika --ascii welcome` — the mirror in a workspace (one clean
/// workflow, so the run CTA and its gate render) is byte-pure too.
#[test]
fn welcome_ascii_emits_only_ascii_bytes() {
    let dir = workspace_tmp_dir("nika-ascii-welcome");
    let proj = dir.join("proj");
    std::fs::create_dir_all(&proj).expect("proj dir");
    std::fs::create_dir(proj.join(".git")).expect("git marker");
    let wf = proj.join("ok.nika.yaml");
    let mut f = std::fs::File::create(&wf).expect("fixture file");
    f.write_all(VALID.as_bytes()).expect("fixture body");

    let out = bin()
        .arg("--ascii")
        .arg("welcome")
        .current_dir(&proj)
        .env("HOME", &dir)
        .output()
        .expect("binary runs");
    assert_eq!(
        out.status.code(),
        Some(0),
        "a greeting is never a failure: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert_all_ascii("welcome --ascii stdout", &out.stdout);
    assert_all_ascii("welcome --ascii stderr", &out.stderr);
    let _ = std::fs::remove_dir_all(&dir);
}

/// `--plain` is the sober umbrella — it includes the ASCII half, so it
/// obeys the same byte contract (the audit's phrasing: « --plain
/// --ascii réellement ASCII »).
#[test]
fn plain_implies_the_ascii_byte_contract() {
    let dir = workspace_tmp_dir("nika-plain-check");
    let wf = dir.join("ok.nika.yaml");
    let mut f = std::fs::File::create(&wf).expect("fixture file");
    f.write_all(VALID.as_bytes()).expect("fixture body");

    let out = bin()
        .arg("--plain")
        .arg("check")
        .arg(&wf)
        .env("HOME", &dir)
        .output()
        .expect("binary runs");
    assert_all_ascii("check --plain stdout", &out.stdout);
    assert_all_ascii("check --plain stderr", &out.stderr);
    let _ = std::fs::remove_dir_all(&dir);
}
