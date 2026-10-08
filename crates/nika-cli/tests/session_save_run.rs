// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! `save & run` on the native machine door, the real binary (NIK-14): one closed consent word
//! lands the proposal shown and runs it once through the native lane, whose child writes the
//! brief's exact bytes; the command replayed answers from its record and runs nothing again.
//! Keyless: an isolated HOME and an empty environment, the intent read by the deterministic
//! compiler.
#![allow(clippy::expect_used, clippy::panic, clippy::disallowed_types)]

use std::io::{BufRead as _, BufReader, Write as _};
use std::path::Path;
use std::process::{Command, Stdio};

use serde_json::{Value, json};

const COPY: &str = "Read ./notes/brief.md and write it to ./out/copy.md";
const BRIEF: &str = "# Brief\n\nOctobre — « vite ».\n";

fn submit(command: &str, snapshot: &Value, line: &str) -> String {
    let frame = json!({
        "contract": "nika/session-host@1", "op": "submit", "command": command,
        "snapshot": snapshot, "line": line,
    });
    frame.to_string()
}

fn kinds(result: &Value) -> Vec<&str> {
    (result["outcomes"].as_array().expect("outcomes").iter())
        .map(|outcome| outcome["kind"].as_str().expect("kind"))
        .collect()
}

/// The traces runs left under `root`.
fn traces(root: &Path) -> usize {
    std::fs::read_dir(root.join(".nika/traces")).map_or(0, Iterator::count)
}

#[test]
fn save_and_run_lands_the_proposal_and_runs_it_once_through_the_native_lane() {
    let project = tempfile::tempdir().expect("project");
    let home = tempfile::tempdir().expect("home");
    std::fs::create_dir_all(project.path().join("notes")).expect("notes");
    std::fs::write(project.path().join("notes/brief.md"), BRIEF).expect("brief");
    let mut child = Command::new(env!("CARGO_BIN_EXE_nika"))
        .args(["session", "--json"])
        .current_dir(project.path())
        .env_clear()
        .env("HOME", home.path())
        .env("PATH", "/usr/bin:/bin")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .expect("nika session --json");
    let mut stdin = child.stdin.take().expect("stdin");
    let mut lines = BufReader::new(child.stdout.take().expect("stdout")).lines();
    let mut next = |kind: &str| -> Value {
        loop {
            let line = lines.next().expect("a frame").expect("utf-8");
            let frame: Value = serde_json::from_str(&line).expect("one JSON object per line");
            if frame["frame"] == kind {
                return frame;
            }
        }
    };
    let opened = next("opened");
    writeln!(
        stdin,
        "{}",
        submit("c-1", &opened["snapshot"]["snapshot"], COPY)
    )
    .expect("submit");
    let proposed = next("result");
    assert_eq!(kinds(&proposed), ["proposal"], "{proposed}");
    let shown = proposed["snapshot"]["snapshot"].clone();
    writeln!(stdin, "{}", submit("c-2", &shown, "save & run")).expect("save & run");
    let ran = next("result");
    assert_eq!(kinds(&ran), ["run_requested", "facts"], "{ran}");
    let observed = ran["outcomes"][1]["text"].as_str().expect("observation");
    assert!(observed.contains("run observed · exit 0"), "{observed}");
    let copied = std::fs::read(project.path().join("out/copy.md")).expect("the run's output");
    assert_eq!(copied, BRIEF.as_bytes(), "the brief's exact bytes");
    assert_eq!(traces(project.path()), 1, "one run");
    writeln!(stdin, "{}", submit("c-2", &shown, "save & run")).expect("replay");
    let replay = next("result");
    assert_eq!(replay["replayed"], true, "{replay}");
    assert_eq!(traces(project.path()), 1, "a replay runs nothing again");
    drop(stdin);
    next("closed");
    assert!(child.wait().expect("exit").success());
}
