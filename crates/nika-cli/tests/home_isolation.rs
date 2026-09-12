// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>
#![allow(clippy::expect_used, clippy::disallowed_types)]

//! The actual doctor door explains HOME isolation even when stderr is piped.
use std::process::Command;

#[test]
fn doctor_warns_once_for_an_alternate_home_without_polluting_json() {
    let room = tempfile::tempdir().expect("owned project");
    let home = room.path().join("custom home");
    let empty_path = room.path().join("empty-bin");
    std::fs::create_dir(&home).expect("owned home");
    std::fs::create_dir(&empty_path).expect("no harness executables");
    let call = |args: &[&str]| {
        Command::new(env!("CARGO_BIN_EXE_nika"))
            .args(args)
            .env_clear()
            .env("HOME", &home)
            .env("PATH", &empty_path)
            .env("NIKA_KEYCHAIN", "off")
            .env("NO_COLOR", "1")
            .current_dir(room.path())
            .output()
            .expect("isolated CLI")
    };
    let help = call(&["--help"]);
    assert!(help.status.success(), "{help:?}");
    assert!(String::from_utf8_lossy(&help.stdout).contains("env -i HOME=$scratch"));
    let human = call(&["doctor"]);
    assert!(human.status.success(), "{human:?}");
    let warning = String::from_utf8(human.stderr).expect("UTF-8 warning");
    assert_eq!(warning.matches("home isolation:").count(), 1, "{warning}");
    assert!(
        warning.contains("other environment variables still apply"),
        "{warning}"
    );
    assert!(warning.contains("env -i HOME=\"$HOME\""), "{warning}");
    assert!(!warning.contains("HOME alone does not move"), "{warning}");
    let machine = call(&["doctor", "--json"]);
    assert!(machine.status.success(), "{machine:?}");
    assert!(machine.stderr.is_empty(), "{machine:?}");
    let _: serde_json::Value = serde_json::from_slice(&machine.stdout).expect("one JSON document");
}
