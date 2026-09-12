// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>
#![allow(clippy::expect_used, clippy::disallowed_types)]

//! The beginner's actual scaffold runs offline and teaches the local timeout.
use std::process::Command;

#[test]
fn hello_scaffold_runs_offline_and_teaches_the_local_timeout_override() {
    let room = tempfile::tempdir().expect("isolated room");
    let home = room.path().join("home");
    std::fs::create_dir(&home).expect("home");
    let call = |args: &[&str]| {
        Command::new(env!("CARGO_BIN_EXE_nika"))
            .args(args)
            .env_clear()
            .env("PATH", "/usr/bin:/bin")
            .env("HOME", &home)
            .env("NIKA_KEYCHAIN", "off")
            .env("NO_COLOR", "1")
            .current_dir(room.path())
            .output()
            .expect("isolated CLI")
    };
    let new = call(&["new", "01-hello", "hello.nika.yaml"]);
    assert!(
        new.status.success(),
        "{}",
        String::from_utf8_lossy(&new.stderr)
    );
    let text = std::fs::read_to_string(room.path().join("hello.nika.yaml")).expect("scaffold");
    let yaml: serde_yaml_bw::Value = serde_yaml_bw::from_str(&text).expect("yaml");
    assert_eq!(yaml["model"].as_str(), Some("mock/echo"));
    assert!(text.contains("ollama pull qwen2.5:0.5b"), "{text}");
    assert!(!text.contains("qwen3.5"), "{text}");
    assert!(
        text.contains("300s") && text.contains("timeout: 7m"),
        "{text}"
    );
    let run = call(&["run", "hello.nika.yaml", "--output", "json"]);
    assert!(
        run.status.success(),
        "{}",
        String::from_utf8_lossy(&run.stderr)
    );
    let output: serde_json::Value = serde_json::from_slice(&run.stdout).expect("typed output");
    assert!(
        output["greeting"].as_str().is_some_and(|s| !s.is_empty()),
        "{output}"
    );
    let explain = call(&["explain", "NIKA-INFER-001"]);
    assert!(explain.status.success());
    let help = String::from_utf8(explain.stdout).expect("help");
    for expected in ["300s local", "30s cloud", "timeout: 7m", "next to infer:"] {
        assert!(help.contains(expected), "{help}");
    }
}
