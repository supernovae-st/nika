// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>
//! E39 10C N5 (case F6-A12): an approval is single-use for the RUN, wherever the resuming process
//! lives. A paused gate approved once from one HOME and resumed again, from the same project,
//! under another HOME must never run its gated effect a second time: the replay is refused
//! NIKA-SEC-010 (`approval.replayed`), as it already is from the same HOME.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
// The workspace bans std::process::Command (production spawns ride the kernel ShellExecutor seam);
// this test's whole job is to execute the real `nika-cli` binary (CARGO_BIN_EXE), the carve-out
// class of bin_smoke.rs and access_plan_e2e.rs: the contract under test IS the binary's behavior.
#![allow(clippy::disallowed_types)]
#![cfg(unix)]
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

const FLOW: &str = r#"nika: approval-replay
permits:
  fs:
    write: ["./out.txt"]
  tools: ["nika:prompt", "nika:write"]
tasks:
  gate:
    invoke:
      tool: "nika:prompt"
      args:
        message: "Write the file?"
  write:
    with:
      ok: "${{ tasks.gate.output }}"
    when: "${{ with.ok == true }}"
    invoke:
      tool: "nika:write"
      args:
        path: "./out.txt"
        content: "written"
"#;

/// The binary on a fresh machine: a cleared environment, one HOME per call, the project as cwd,
/// and no run key (keyless, like CI).
fn nika(root: &Path, home: &str, args: &[&str]) -> Output {
    let home = root.join(home);
    std::fs::create_dir_all(&home).expect("home");
    Command::new(env!("CARGO_BIN_EXE_nika"))
        .args(args)
        .env_clear()
        .env("PATH", "/usr/bin:/bin")
        .env("HOME", &home)
        .env("TERM", "dumb")
        .env("NIKA_KEYCHAIN", "off")
        .env("NIKA_RUN_KEY_FILE", home.join("absent.key"))
        .env("NIKA_RUN_PUB_FILE", home.join("absent.pub"))
        .current_dir(root.join("work"))
        .output()
        .expect("binary runs")
}

/// The trace a run names: its receipt frame, else the stderr line.
fn trace_of(out: &Output, work: &Path) -> PathBuf {
    let stdout = String::from_utf8_lossy(&out.stdout);
    let named = stdout
        .lines()
        .filter_map(|line| serde_json::from_str::<serde_json::Value>(line).ok())
        .find_map(|frame| frame["receipt"]["trace_path"].as_str().map(str::to_owned))
        .or_else(|| {
            String::from_utf8_lossy(&out.stderr)
                .lines()
                .find_map(|line| line.split("trace: ").nth(1))
                .map(|rest| rest.split_whitespace().next().unwrap_or(rest).to_owned())
        })
        .unwrap_or_else(|| panic!("the paused run names its trace: {stdout}"));
    let path = PathBuf::from(named);
    if path.is_absolute() {
        path
    } else {
        work.join(path)
    }
}

#[test]
fn a_consumed_approval_resumed_from_another_home_never_runs_again() {
    let root = std::env::temp_dir().join(format!("nika-n5-home-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    let work = root.join("work");
    std::fs::create_dir_all(&work).expect("project");
    std::fs::write(work.join("flow.nika"), FLOW).expect("workflow");

    let paused = nika(&root, "home-a", &["run", "flow.nika", "--json"]);
    assert_eq!(paused.status.code(), Some(4), "the gate pauses: {paused:?}");
    let trace = trace_of(&paused, &work);
    let trace = trace.to_str().expect("utf-8 trace path");
    let resume = [
        "run",
        "flow.nika",
        "--json",
        "--resume",
        trace,
        "--answer",
        "gate=true",
    ];

    let first = nika(&root, "home-a", &resume);
    assert_eq!(
        first.status.code(),
        Some(0),
        "the approval runs once: {first:?}"
    );
    assert!(work.join("out.txt").exists(), "the gated write ran");
    std::fs::remove_file(work.join("out.txt")).expect("reset the effect");

    let again = nika(&root, "home-b", &resume);
    let told = format!(
        "{}{}",
        String::from_utf8_lossy(&again.stdout),
        String::from_utf8_lossy(&again.stderr)
    );
    assert!(
        !work.join("out.txt").exists(),
        "a second HOME replayed the consumed approval: {told}"
    );
    assert_ne!(again.status.code(), Some(0), "{told}");
    assert!(
        told.contains("NIKA-SEC-010") && told.contains("approval.replayed"),
        "{told}"
    );
    let _ = std::fs::remove_dir_all(&root);
}
