// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>
#![allow(clippy::expect_used, clippy::disallowed_types)]

//! The real trace doors expose the same gate attestation as the journal.
use std::process::Command;

#[test]
fn trace_show_and_replay_expose_the_recorded_gate_without_reexecuting() {
    let room = tempfile::tempdir().expect("isolated room");
    let home = room.path().join("home");
    std::fs::create_dir(&home).expect("home");
    std::fs::write(room.path().join("flow.nika.yaml"),
        "nika: gate-display\nmodel: mock/echo\npermits: { tools: [nika:prompt] }\ntasks:\n  approve:\n    invoke:\n      tool: nika:prompt\n      args: { mode: confirm, message: 'ship it?' }\n"
    ).expect("workflow");
    let call = |args: &[&str], operator: &str| {
        Command::new(env!("CARGO_BIN_EXE_nika"))
            .args(args)
            .env_clear()
            .env("PATH", "/usr/bin:/bin")
            .env("HOME", &home)
            .env("NIKA_KEYCHAIN", "off")
            .env("NIKA_OPERATOR", operator)
            .env("NO_COLOR", "1")
            .current_dir(room.path())
            .output()
            .expect("isolated CLI")
    };
    let run = call(
        &[
            "run",
            "flow.nika.yaml",
            "--answer",
            "approve=true",
            "--json",
        ],
        "alice-ci",
    );
    assert!(
        run.status.success(),
        "{}",
        String::from_utf8_lossy(&run.stderr)
    );
    let trace = std::fs::read_dir(room.path().join(".nika/traces"))
        .expect("traces")
        .filter_map(Result::ok)
        .map(|e| e.path())
        .find(|p| p.extension().is_some_and(|e| e == "ndjson"))
        .expect("journal");
    let before = std::fs::read(&trace).expect("journal bytes");
    std::fs::remove_file(room.path().join("flow.nika.yaml")).expect("reading needs no workflow");
    for verb in ["show", "replay"] {
        let read = call(&["trace", verb, trace.to_str().expect("path")], "bob-now");
        let text = String::from_utf8(read.stdout).expect("text");
        assert!(
            read.status.success(),
            "{text}\n{}",
            String::from_utf8_lossy(&read.stderr)
        );
        for expected in [
            "gate \"approve\"",
            "decision: \"allow\"",
            "source: \"cli\"",
            "operator (declared): \"alice-ci\"",
            "question: \"ship it?\"",
            "answer: \"true\"",
        ] {
            assert!(text.contains(expected), "missing {expected}: {text}");
        }
        assert!(
            !text.contains("bob-now"),
            "the reader must not invent today's operator"
        );
        assert_eq!(std::fs::read(&trace).expect("unchanged journal"), before);
    }
}
