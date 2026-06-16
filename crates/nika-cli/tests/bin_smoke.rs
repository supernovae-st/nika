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
