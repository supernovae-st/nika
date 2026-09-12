// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>
#![allow(clippy::expect_used, clippy::disallowed_types)]

//! A real refusal retains its complete lesson through check and explain.
use std::process::Command;

fn assert_teaching(source: &str, code: &str) {
    let refusal = nika_schema::parse(
        source,
        nika_schema::FileId::new(0),
        nika_schema::ParseMode::Strict,
    )
    .expect_err("fixture is refused");
    let lesson = match &refusal {
        nika_schema::SchemaError::Validation { message, .. } => message.clone(),
        _ => refusal.to_string(),
    };
    let room = tempfile::tempdir().expect("isolated room");
    let home = room.path().join("home");
    std::fs::create_dir(&home).expect("home");
    std::fs::write(room.path().join("source-frame-only.nika.yaml"), source).expect("fixture");
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
    let human = call(&["check", "source-frame-only.nika.yaml"]);
    assert_eq!(human.status.code(), Some(2));
    assert!(
        String::from_utf8_lossy(&human.stdout).contains(&lesson),
        "{human:?}"
    );
    let machine = call(&["check", "source-frame-only.nika.yaml", "--json"]);
    assert_eq!(machine.status.code(), Some(2));
    let payload: serde_json::Value = serde_json::from_slice(&machine.stdout).expect("finding JSON");
    let finding = &payload["findings"][0];
    assert_eq!(finding["code"], code);
    let message = finding["message"].as_str().expect("message");
    assert!(message.contains(&lesson), "{payload}");
    assert!(
        !message.contains("source-frame-only"),
        "source frame leaked: {payload}"
    );
    let explain = call(&["explain", code]);
    assert!(explain.status.success());
    assert!(
        String::from_utf8_lossy(&explain.stdout).contains(&lesson),
        "{explain:?}"
    );
}

#[test]
fn values_finding_and_explain_share_full_teaching() {
    assert_teaching(
        "nika: teaching\nvars: {x: hi}\ntasks: {}\n",
        "NIKA-VALUES-001",
    );
}

#[test]
fn scalar_exec_finding_and_explain_share_full_teaching() {
    assert_teaching(
        "nika: teaching\ntasks:\n  say:\n    exec: ls -la\n",
        "NIKA-PARSE-019",
    );
}

#[test]
fn string_command_finding_and_explain_share_full_teaching() {
    assert_teaching(
        "nika: teaching\ntasks:\n  say:\n    exec: {command: 'ls -la'}\n",
        "NIKA-PARSE-019",
    );
}
