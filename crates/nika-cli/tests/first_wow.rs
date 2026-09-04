// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>
#![allow(clippy::expect_used, clippy::panic)]
#![allow(clippy::disallowed_types)]

//! First-wow binary path: empty dir → `nika new hello` → `nika run`
//! that file exits 0. Split from `bin_smoke.rs` (1500 LOC cap).

use std::process::{Command, Stdio};

fn bin() -> Command {
    Command::new(env!("CARGO_BIN_EXE_nika"))
}

fn scratch(name: &str) -> std::path::PathBuf {
    let dir = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .join("target")
        .join("tmp")
        .join(format!("{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("tmp");
    dir
}

fn nika(dir: &std::path::Path, args: &[&str]) -> std::process::Output {
    bin()
        .args(args)
        .current_dir(dir)
        .env("HOME", dir)
        .env_remove("ANTHROPIC_API_KEY")
        .env_remove("OPENAI_API_KEY")
        .env_remove("XAI_API_KEY")
        .stdin(Stdio::null())
        .output()
        .expect("nika")
}

fn next_of(out: &std::process::Output) -> String {
    String::from_utf8_lossy(&out.stdout)
        .split("Next:")
        .nth(1)
        .unwrap_or("")
        .to_owned()
}

/// Gauntlet W2: the printed Next after hello exists must run, not
/// `--access harness` (NIKA-INFER-001) and not another `new hello`.
#[test]
fn welcome_next_after_hello_is_run_not_new() {
    let dir = scratch("nika-welcome-next-after");
    let vacant = nika(&dir, &["welcome"]);
    assert_eq!(vacant.status.code(), Some(0));
    assert!(
        next_of(&vacant).contains("nika new hello"),
        "{}",
        String::from_utf8_lossy(&vacant.stdout)
    );

    let wrote = nika(&dir, &["new", "hello"]);
    assert_eq!(
        wrote.status.code(),
        Some(0),
        "{}{}",
        String::from_utf8_lossy(&wrote.stdout),
        String::from_utf8_lossy(&wrote.stderr)
    );

    let again = nika(&dir, &["welcome"]);
    let after = next_of(&again);
    assert!(after.contains("nika run hello.nika.yaml"), "{after}");
    assert!(!after.contains("--access harness"), "{after}");
    assert!(!after.contains("nika new hello"), "{after}");

    let ran = nika(&dir, &["run", "hello.nika.yaml", "--max-cost-usd", "0.01"]);
    assert_eq!(
        ran.status.code(),
        Some(0),
        "printed Next must exit 0:\n{}{}",
        String::from_utf8_lossy(&ran.stdout),
        String::from_utf8_lossy(&ran.stderr)
    );
    let _ = std::fs::remove_dir_all(&dir);
}
