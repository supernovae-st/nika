// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>
#![allow(clippy::expect_used, clippy::panic)]
// The workspace bans std::process::Command (production spawns ride the
// kernel ShellExecutor seam). This test's WHOLE JOB is to execute the
// real `nika-cli` binary (CARGO_BIN_EXE) — the bin_smoke carve-out
// class: the contract under test IS the rendered run card.
#![allow(clippy::disallowed_types)]

//! V7-1 · the silent fan (gauntlet wave 3 · Marta BLOCKER): a
//! `for_each` whose K iterations were repaired (`recover:` fallback ·
//! `on_error: skip`) used to render `✔ for_each · N items` and a green
//! close — she found her two dead posts by counting the trace by hand.
//! The fan's own row now carries the tally: `for_each · N/M ok · K
//! recovered`.

use std::io::Write as _;
use std::process::Command;

fn scratch(name: &str) -> std::path::PathBuf {
    let dir = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .join("target")
        .join("tmp")
        .join(format!("{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("scratch dir");
    dir
}

fn write_file(dir: &std::path::Path, name: &str, body: &str) -> std::path::PathBuf {
    let path = dir.join(name);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).expect("parent");
    }
    let mut f = std::fs::File::create(&path).expect("file");
    f.write_all(body.as_bytes()).expect("body");
    path
}

const FAN: &str = r#"
nika: fan-tally
model: mock/echo
permits:
  tools: ["nika:read"]
  fs:
    read: ["items", "items/*"]
tasks:
  fan:
    for_each: { items: ["items/a.txt", "items/GHOST.txt", "items/c.txt"] }
    on_error:
      recover: null
    invoke:
      tool: "nika:read"
      args: { path: "${{ item }}" }
"#;

/// One repaired iteration out of three → the row says `2/3 ok ·
/// 1 recovered`, and the old all-green `3 items` shape is GONE from
/// this run (the exact silent card the wave-3 replay measured).
#[test]
fn a_repaired_iteration_lands_on_the_fan_row() {
    let dir = scratch("silent-fan");
    let wf = write_file(&dir, "fan.nika.yaml", FAN);
    write_file(&dir, "items/a.txt", "A\n");
    write_file(&dir, "items/c.txt", "C\n");

    let out = Command::new(env!("CARGO_BIN_EXE_nika-cli"))
        .arg("run")
        .arg(&wf)
        .arg("--no-progress")
        .current_dir(&dir)
        .output()
        .expect("binary runs");
    let text = format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    assert_eq!(
        out.status.code(),
        Some(0),
        "a recovered fan still settles green (the repair is declared): {text}"
    );
    assert!(
        text.contains("for_each · 2/3 ok · 1 recovered"),
        "the fan row carries the tally: {text}"
    );
    assert!(
        !text.contains("for_each · 3 items"),
        "the silent all-green shape is gone from a repaired fan: {text}"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

/// The healthy fan keeps its exact historical row — zero repaired
/// iterations must not grow a `0 recovered` tail (calm stays calm).
#[test]
fn a_healthy_fan_keeps_its_historical_row() {
    let dir = scratch("healthy-fan");
    let wf = write_file(&dir, "fan.nika.yaml", FAN);
    write_file(&dir, "items/a.txt", "A\n");
    write_file(&dir, "items/GHOST.txt", "G\n");
    write_file(&dir, "items/c.txt", "C\n");

    let out = Command::new(env!("CARGO_BIN_EXE_nika-cli"))
        .arg("run")
        .arg(&wf)
        .arg("--no-progress")
        .current_dir(&dir)
        .output()
        .expect("binary runs");
    let text = format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    assert_eq!(out.status.code(), Some(0), "{text}");
    assert!(
        text.contains("for_each · 3 items"),
        "no repair = the historical row, byte-stable: {text}"
    );
    assert!(
        !text.contains("recovered"),
        "calm stays calm — no zero-count tail: {text}"
    );
    let _ = std::fs::remove_dir_all(&dir);
}
