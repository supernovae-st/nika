// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! A cargo-built `nika` is ad-hoc linker-signed: the very binary
//! the keychain password prompt came from on every run. On a terminal, with
//! `NIKA_KEYCHAIN` unset (the environment cleared) and a scratch HOME, it never
//! opens the keychain: `nika doctor` names why in its run-key row, and a run
//! ends without it (its journal unsigned, no key file in that HOME). The
//! doctor row is red on 292c7bee0, which had no run-key row at all; the run
//! half hangs there only on a machine whose login keychain holds the
//! `nika/run-signing-key` item (the prompt waits for a password).
#![cfg(target_os = "macos")]
#![allow(
    clippy::expect_used,
    clippy::unwrap_used,
    clippy::panic,
    clippy::disallowed_types,
    clippy::disallowed_methods
)]

use std::path::Path;
use std::process::Command;
use std::time::Duration;

use expectrl::{Eof, Expect};

/// `nika <args>` in a PTY from `root`, HOME `root`, the environment cleared
/// (`NIKA_KEYCHAIN` unset).
fn pty(root: &Path, args: &[&str]) -> expectrl::session::OsSession {
    let mut command = Command::new(env!("CARGO_BIN_EXE_nika"));
    command
        .args(args)
        .current_dir(root)
        .env_clear()
        .env("TERM", "xterm-256color")
        .env("HOME", root)
        .env("PATH", "/usr/bin:/bin");
    let mut session = expectrl::session::OsSession::spawn(command).expect("a PTY");
    session.set_expect_timeout(Some(Duration::from_secs(60)));
    session
}

#[test]
fn doctor_says_an_ad_hoc_build_never_opens_the_keychain() {
    let root = tempfile::tempdir().expect("scratch home");
    // Machine output: the row's words arrive whole, whatever the PTY's width.
    let mut doctor = pty(root.path(), &["doctor", "--json"]);
    doctor
        .expect("the keychain is not opened by this ad-hoc build")
        .expect("the run-key row names why, never a password prompt");
    doctor.expect(Eof).expect("doctor ends");
}

/// A public question opens no item either: `nika key trust`, which once read
/// the secret item to prove the pair, answers from this HOME alone.
/// With the keychain's items present it would print their key; it says there
/// is none, whatever this machine's keychain holds.
#[test]
fn key_trust_on_a_terminal_reads_no_keychain_item() {
    let root = tempfile::tempdir().expect("scratch home");
    let mut trust = pty(root.path(), &["key", "trust"]);
    trust
        .expect("no run-signing key")
        .expect("an ad-hoc build answers from its own HOME, never the keychain");
    trust.expect(Eof).expect("key trust ends");
}

#[test]
fn a_run_on_a_terminal_ends_without_the_keychain() {
    let root = tempfile::tempdir().expect("scratch home");
    std::fs::write(
        root.path().join("hello.nika"),
        "nika: hello\npermits:\n  tools: [\"nika:log\"]\ntasks:\n  say:\n    invoke:\n      tool: \"nika:log\"\n      args: { message: hi }\n",
    )
    .expect("fixture workflow");
    let mut run = pty(
        root.path(),
        &["run", "hello.nika", "--max-cost-usd", "0.01"],
    );
    run.expect(Eof)
        .expect("the run ends: no keychain prompt holds it");
    let traces = root.path().join(".nika/traces");
    let journal = std::fs::read_dir(&traces)
        .expect("the run's journal directory")
        .map(|entry| entry.expect("journal entry").path())
        .find(|path| path.extension().is_some_and(|ext| ext == "ndjson"))
        .expect("one journal");
    let text = std::fs::read_to_string(journal).expect("journal text");
    assert!(
        !text.contains("\"run_sealed\""),
        "no key file in this HOME and no keychain: the run is unsigned, never prompted"
    );
}
