// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! `nika session --json`, the real binary (ADR-148): the same Session as bare `nika`, NDJSON on
//! stdio, keyless. An isolated HOME and an empty environment: no kept choice, no provider, the
//! intent reaches the deterministic compiler.
#![allow(clippy::expect_used, clippy::panic, clippy::disallowed_types)]

use std::io::{BufRead as _, BufReader, Write as _};
use std::process::{Command, Stdio};

use serde_json::Value;

const COPY: &str = "Read ./notes/brief.md and write it to ./out/copy.md";

#[test]
fn the_machine_door_opens_proposes_saves_and_closes_on_stdin_end() {
    let project = tempfile::tempdir().expect("project");
    let home = tempfile::tempdir().expect("home");
    std::fs::create_dir_all(project.path().join("notes")).expect("notes");
    std::fs::write(
        project.path().join("notes/brief.md"),
        "# Brief\n\nOctobre — « vite ».\n",
    )
    .expect("brief");
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
            assert_eq!(frame["contract"], "nika/session-host@1");
            if frame["frame"] == kind {
                return frame;
            }
        }
    };
    let opened = next("opened");
    assert_eq!(opened["event"], 1);
    let submit = serde_json::json!({
        "contract": "nika/session-host@1", "op": "submit", "command": "c-1",
        "snapshot": opened["snapshot"]["snapshot"], "line": COPY,
    });
    writeln!(stdin, "{submit}").expect("submit");
    let proposed = next("result");
    assert_eq!(proposed["outcomes"][0]["kind"], "proposal", "{proposed}");
    let path = proposed["snapshot"]["work"]["candidate"]["files"][0]["path"]
        .as_str()
        .expect("path")
        .to_owned();
    assert!(
        !project.path().join(&path).exists(),
        "nothing lands before the yes"
    );
    let yes = serde_json::json!({
        "contract": "nika/session-host@1", "op": "submit", "command": "c-2",
        "snapshot": proposed["snapshot"]["snapshot"], "line": "yes",
    });
    writeln!(stdin, "{yes}").expect("yes");
    let saved = next("result");
    assert_eq!(
        saved["snapshot"]["work"]["saved"]["workflow"],
        path.as_str(),
        "{saved}"
    );
    assert!(
        project.path().join(&path).exists(),
        "the yes saved the previewed bytes"
    );
    assert!(
        !project.path().join("out/copy.md").exists(),
        "Save is never a Run"
    );
    drop(stdin);
    let closed = next("closed");
    assert!(closed["event"].as_u64() > saved["event"].as_u64());
    assert!(child.wait().expect("exit").success());
}

/// The first frame of `nika session --json` plus `extra`, in an isolated HOME and an empty
/// environment, with stdin closed; and the exit.
fn first_frame(extra: &[&str], project: &std::path::Path, home: &std::path::Path) -> (Value, bool) {
    let out = Command::new(env!("CARGO_BIN_EXE_nika"))
        .args(["session", "--json"])
        .args(extra)
        .current_dir(project)
        .env_clear()
        .env("HOME", home)
        .env("PATH", "/usr/bin:/bin")
        .stdin(Stdio::null())
        .stderr(Stdio::null())
        .output()
        .expect("nika session --json");
    let stdout = String::from_utf8(out.stdout).expect("utf-8");
    let line = stdout.lines().next().expect("a first frame");
    let frame: Value = serde_json::from_str(line).expect("one JSON object per line");
    assert_eq!(frame["contract"], "nika/session-host@1");
    (frame, out.status.success())
}

/// The opener names this conversation's intelligence on the command line, in the census's own
/// words: it holds for this conversation and the operator's kept choice is never written; words
/// the census does not read open nothing. The native identity advertises the word.
#[test]
fn the_machine_door_opens_with_the_conversation_s_named_choice() {
    let project = tempfile::tempdir().expect("project");
    let home = tempfile::tempdir().expect("home");
    let (opened, _) = first_frame(&["--intelligence", "4"], project.path(), home.path());
    assert_eq!(opened["frame"], "opened", "{opened}");
    let selected = &opened["snapshot"]["work"]["intelligence"]["selected"];
    assert_eq!(selected["kind"], "none", "{opened}");
    assert_eq!(selected["scope"], "conversation", "{opened}");
    let (refused, success) = first_frame(&["--intelligence=9"], project.path(), home.path());
    assert_eq!(refused["frame"], "refused", "{refused}");
    assert_eq!(refused["error"], "session_unavailable", "{refused}");
    assert!(!success, "a refused open is not a success");
    let kept = nika_session::intelligence::UserIntelligencePreference::path_under(home.path());
    assert!(!kept.exists(), "the operator's choice is never written");
    let identity = Command::new(env!("CARGO_BIN_EXE_nika"))
        .arg("--sdk-identity")
        .current_dir(project.path())
        .env_clear()
        .env("HOME", home.path())
        .output()
        .expect("identity");
    let identity: Value = serde_json::from_slice(&identity.stdout).expect("identity JSON");
    let words = identity["supportedCapabilities"].as_array().expect("words");
    assert!(
        words.contains(&Value::from("sessionIntelligence")),
        "{identity}"
    );
}
