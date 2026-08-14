// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>
#![allow(clippy::expect_used, clippy::panic)]
// This suite executes the real `nika-cli` binary (CARGO_BIN_EXE) — its
// whole job is the binary contract, so it spawns processes (the same
// sanctioned carve-out as bin_smoke.rs / run_verb.rs).
#![allow(clippy::disallowed_types)]

//! NEP-0009 on the BUILTIN arm — the measured 2026-08-08 hole: an exact
//! `fs.read` grant whose literal is swapped for a symlink BETWEEN check
//! and run. The exec arm's dispatch re-gate refuses the pivot before
//! spawn; the in-process builtin (`nika:read`) re-judged the path the
//! task NAMED, and resolved-vs-resolved followed the plant on both
//! sides. The boundary must now judge the grant's effective path
//! identity — ancestor canonicalized, final component lexical — and
//! refuse the divergence as `fs.path_mismatch` (`NIKA-SEC-004`), never
//! rewrite it.

use std::process::Command;

fn bin() -> Command {
    Command::new(env!("CARGO_BIN_EXE_nika-cli"))
}

/// The preserved probe's shape: an exact read grant + a log of what was
/// read (any leak rides the run's own outputs).
const SONDE: &str = r#"
nika: toctou-probe

permits:
  fs:
    read: ["./allowed.txt"]
  tools: ["nika:read", "nika:log"]

tasks:
  peek:
    invoke:
      tool: "nika:read"
      args:
        path: "./allowed.txt"

  show:
    with:
      body: ${{ tasks.peek.output }}
    invoke:
      tool: "nika:log"
      args:
        level: info
        message: "read"
        data: "${{ with.body }}"
"#;

/// The measured sequence: check green on the honest tree · the grant
/// serves the honest file · the swap · the run MUST refuse, name
/// `NIKA-SEC-004` + `fs.path_mismatch` (judged prefix + resolved
/// target), and the out-of-bounds secret rides NOTHING the run emits.
#[cfg(unix)]
#[test]
fn an_exact_grant_swapped_between_check_and_run_is_refused() {
    let dir = std::env::temp_dir().join(format!("nika-toctou-e2e-{}", std::process::id()));
    std::fs::create_dir_all(dir.join("oob")).expect("scratch");
    std::fs::write(dir.join("toctou-probe.nika.yaml"), SONDE).expect("workflow");
    std::fs::write(dir.join("allowed.txt"), "IN-BOUNDS\n").expect("honest file");
    std::fs::write(dir.join("oob/secret.txt"), "OUT-OF-BOUNDS-SECRET\n").expect("secret");

    // 1 · the honest tree checks green (the house law: never run unchecked).
    let check = bin()
        .args(["check", "toctou-probe.nika.yaml"])
        .current_dir(&dir)
        .output()
        .expect("binary runs");
    assert_eq!(
        check.status.code(),
        Some(0),
        "check green on the honest tree: {}",
        String::from_utf8_lossy(&check.stderr)
    );

    // 2 · the honest run serves the honest file — the instrument is
    // qualified (the grant works before the pivot).
    let honest = bin()
        .args(["run", "toctou-probe.nika.yaml", "--color", "never"])
        .current_dir(&dir)
        .output()
        .expect("binary runs");
    assert_eq!(honest.status.code(), Some(0), "the honest run completes");
    let honest_all = String::from_utf8_lossy(&honest.stdout).into_owned()
        + &String::from_utf8_lossy(&honest.stderr);
    assert!(
        honest_all.contains("IN-BOUNDS"),
        "the honest file was served: {honest_all}"
    );

    // 3 · the pivot: the grant literal becomes a symlink OUT of the
    // judged tree.
    std::fs::remove_file(dir.join("allowed.txt")).expect("rm");
    std::os::unix::fs::symlink("./oob/secret.txt", dir.join("allowed.txt")).expect("symlink");

    // 4 · the run refuses — the builtin arm judges the effective path
    // identity, never the spelled string alone.
    let swapped = bin()
        .args(["run", "toctou-probe.nika.yaml", "--color", "never"])
        .current_dir(&dir)
        .output()
        .expect("binary runs");
    let out = String::from_utf8_lossy(&swapped.stdout).into_owned()
        + &String::from_utf8_lossy(&swapped.stderr);
    assert_ne!(
        swapped.status.code(),
        Some(0),
        "a swapped grant never completes green: {out}"
    );
    assert!(out.contains("NIKA-SEC-004"), "the boundary speaks: {out}");
    assert!(
        out.contains("fs.path_mismatch"),
        "the mismatch is named (one voice with the exec arm): {out}"
    );
    assert!(
        out.contains("secret.txt"),
        "the resolved target rides the refusal: {out}"
    );
    assert!(
        !out.contains("OUT-OF-BOUNDS-SECRET"),
        "the secret never leaves oob/: {out}"
    );
    let traces = dir.join(".nika").join("traces");
    if traces.is_dir() {
        for entry in std::fs::read_dir(&traces).expect("traces") {
            let journal = std::fs::read_to_string(entry.expect("entry").path()).expect("journal");
            assert!(
                !journal.contains("OUT-OF-BOUNDS-SECRET"),
                "no journal carries the secret"
            );
        }
    }
    let _ = std::fs::remove_dir_all(&dir);
}
