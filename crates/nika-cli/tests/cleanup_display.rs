// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>
#![allow(clippy::expect_used, clippy::panic, clippy::disallowed_types)]

//! The real live and replay doors read the same cleanup witnesses, offline.
use std::process::Command;

fn run_and_replay(yaml: &str) -> (String, String) {
    let room = tempfile::tempdir().expect("isolated room");
    std::fs::create_dir(room.path().join("home")).expect("home");
    std::fs::write(room.path().join("flow.nika.yaml"), yaml).expect("workflow");
    let call = |args: &[&str]| {
        Command::new(env!("CARGO_BIN_EXE_nika"))
            .args(args)
            .env_clear()
            .env("PATH", "/usr/bin:/bin")
            .env("HOME", room.path().join("home"))
            .env("NIKA_KEYCHAIN", "off")
            .env("NO_COLOR", "1")
            .current_dir(room.path())
            .output()
            .expect("isolated CLI")
    };
    let live = call(&["run", "flow.nika.yaml", "--plain"]);
    let live_text = format!(
        "{}\n{}",
        String::from_utf8_lossy(&live.stdout),
        String::from_utf8_lossy(&live.stderr)
    );
    assert!(live.status.success(), "{live_text}");
    let trace = std::fs::read_dir(room.path().join(".nika/traces"))
        .expect("traces")
        .filter_map(Result::ok)
        .map(|e| e.path())
        .find(|p| p.extension().is_some_and(|e| e == "ndjson"))
        .expect("recorded trace");
    let replay = call(&["trace", "show", trace.to_str().expect("path")]);
    let replay_text = format!(
        "{}\n{}",
        String::from_utf8_lossy(&replay.stdout),
        String::from_utf8_lossy(&replay.stderr)
    );
    assert!(replay.status.success(), "{replay_text}");
    (live_text, replay_text)
}

#[test]
fn cleanup_live_and_trace_show_agree_without_inflating_main_count() {
    let (live_text, replay_text) = run_and_replay(
        "nika: cleanup-display\nmodel: mock/echo\npermits: { tools: [nika:log, nika:read], fs: { read: ['./missing.txt'] } }\ntasks:\n  main:\n    infer: { prompt: hello }\n  cleaned:\n    after: { main: unwind }\n    invoke: { tool: nika:log, args: { message: done } }\n  failed:\n    after: { main: unwind }\n    invoke: { tool: nika:read, args: { path: './missing.txt' } }\n  skipped:\n    after: { main: unwind }\n    when: ${{ false }}\n    invoke: { tool: nika:log, args: { message: never } }\n",
    );
    for text in [&live_text, &replay_text] {
        for label in [
            "cleanup of main · success",
            "cleanup of main · failed",
            "cleanup of main · skipped",
            "1/1 done",
        ] {
            assert!(text.contains(label), "missing {label}: {text}");
        }
        assert!(!text.contains("4/4 done"), "{text}");
        assert!(!text.contains("4 tasks"), "{text}");
    }
}

#[test]
fn shared_cleanup_does_not_lend_success_to_the_following_parent_cleanup() {
    let (live, replay) = run_and_replay(
        "nika: shared-cleanup\nmodel: mock/echo\npermits: { tools: [nika:log, nika:read], fs: { read: ['./missing.txt'] } }\ntasks:\n  a:\n    infer: { prompt: a }\n  b:\n    infer: { prompt: b }\n  shared:\n    after: { a: unwind, b: unwind }\n    invoke: { tool: nika:log, args: { message: shared } }\n  a_tail:\n    after: { a: unwind }\n    invoke: { tool: nika:log, args: { message: a tail } }\n  b_tail:\n    after: { b: unwind }\n    invoke: { tool: nika:read, args: { path: './missing.txt' } }\n",
    );
    for text in [&live, &replay] {
        assert!(
            text.lines().any(|l| l.contains("shared")
                && l.contains("cleanup of a · success")
                && l.contains("cleanup of b · success")),
            "{text}"
        );
        assert!(
            text.lines()
                .any(|l| l.contains("a_tail") && l.contains("cleanup of a · success")),
            "{text}"
        );
        assert!(
            text.lines()
                .any(|l| l.contains("b_tail") && l.contains("cleanup of b · failed")),
            "{text}"
        );
        assert!(
            !text
                .lines()
                .any(|l| l.contains("b_tail") && l.contains("cleanup of b · success")),
            "{text}"
        );
        assert!(text.contains("2/2 done"), "{text}");
        assert!(text.contains("2 tasks"), "{text}");
        assert!(!text.contains("5 tasks"), "{text}");
    }
}
