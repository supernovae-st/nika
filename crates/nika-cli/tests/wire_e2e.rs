// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>
#![allow(clippy::expect_used, clippy::panic)]
// This suite executes the real `nika-cli` binary (CARGO_BIN_EXE) — its
// whole job is the binary contract, so it spawns processes (the same
// sanctioned carve-out as bin_smoke.rs / guard_e2e.rs).
#![allow(clippy::disallowed_types)]

//! `nika wire`'s consent door at the REAL binary (H7 · audit UX
//! 2026-07-30): the `all` sweep owes a preview, a live `y`, or an
//! explicit `--yes` — a bare `all` in a pipe is REFUSED naming the
//! doors; `--dry-run` renders the plan and writes nothing; `detected`
//! in a bare HOME is honest about finding no client. The per-client
//! patch contract lives in `bin_smoke.rs`; this suite is the door itself.

use std::process::Command;

fn bin() -> Command {
    Command::new(env!("CARGO_BIN_EXE_nika"))
}

/// `wire all` with stdin piped closed: no terminal, no `--yes` → the
/// refusal (exit 3 · environment class) naming all three doors.
#[test]
fn wire_all_in_a_pipe_is_refused_naming_the_doors() {
    let home = tempfile::tempdir().expect("home");
    let out = bin()
        .args(["wire", "all"])
        .env("HOME", home.path())
        .current_dir(home.path()) // a bare cwd too — no workspace rows
        .stdin(std::process::Stdio::null()) // piped closed: never interactive
        .output()
        .expect("binary runs");
    assert_eq!(
        out.status.code(),
        Some(3),
        "the sweep refuses without consent · stdout: {} · stderr: {}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    let all = format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    for door in ["--dry-run", "wire detected", "--yes"] {
        assert!(all.contains(door), "the refusal names `{door}`: {all}");
    }
    assert!(
        !home.path().join(".cursor").exists(),
        "a refused sweep writes nothing"
    );
}

/// `wire all --dry-run` in a fake HOME with a seeded client: the plan
/// renders, exit 0, and the seeded config's bytes are UNTOUCHED (the
/// `run --dry-run` law: plan only, zero effects).
#[test]
fn wire_all_dry_run_plans_and_writes_nothing() {
    let home = tempfile::tempdir().expect("home");
    let cursor = home.path().join(".cursor");
    std::fs::create_dir_all(&cursor).expect("cursor dir");
    let mcp = cursor.join("mcp.json");
    std::fs::write(&mcp, "{}").expect("seeded config");

    let out = bin()
        .args(["wire", "all", "--dry-run"])
        .env("HOME", home.path())
        .current_dir(home.path())
        .output()
        .expect("binary runs");
    assert_eq!(
        out.status.code(),
        Some(0),
        "a dry run is a success · stdout: {} · stderr: {}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    let stdout = String::from_utf8(out.stdout).expect("utf8");
    assert!(stdout.contains("dry"), "the plan says what it is: {stdout}");
    assert_eq!(
        std::fs::read_to_string(&mcp).expect("seeded config readable"),
        "{}",
        "the seeded config's bytes are untouched"
    );
}

/// `wire detected` in a bare HOME: no client shows, the line is honest
/// (and hands over to the two doors) — a success, never a fabricated
/// sweep.
#[test]
fn wire_detected_in_a_bare_home_says_so_and_exits_zero() {
    let home = tempfile::tempdir().expect("home");
    let out = bin()
        .args(["wire", "detected"])
        .env("HOME", home.path())
        .current_dir(home.path()) // bare cwd: no workspace-anchored rows either
        .output()
        .expect("binary runs");
    assert_eq!(
        out.status.code(),
        Some(0),
        "an honest nothing-found is a success · stdout: {} · stderr: {}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    let stdout = String::from_utf8(out.stdout).expect("utf8");
    assert!(
        stdout.contains("no MCP client detected"),
        "the honest line: {stdout}"
    );
}
