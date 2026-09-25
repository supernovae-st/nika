// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>
//! The fresh-input boundary: its drain of a Rust-buffered source, its bound, and its
//! refusal without a terminal. This test process never touches its own terminal: the
//! refusal runs in a child whose stdin is /dev/null. The PTY proofs live in nika-cli.
#![allow(
    clippy::expect_used,
    clippy::unwrap_used,
    clippy::panic,
    clippy::disallowed_types,
    clippy::disallowed_macros,
    clippy::print_stdout
)]
use super::*;
use std::io::{BufReader, Cursor};

#[test]
fn the_drain_empties_a_rust_buffer_and_its_source_without_waiting() {
    let text = b"yes\nrun it\n/quit\n".to_vec();
    let mut source = BufReader::with_capacity(4, Cursor::new(text));
    assert_eq!(
        source.fill_buf().unwrap(),
        b"yes\n",
        "a line already buffered"
    );
    discard(&mut source, 1 << 20).unwrap();
    assert!(source.fill_buf().unwrap().is_empty());
}

#[test]
fn typeahead_past_the_bound_refuses_and_the_bound_itself_drains() {
    let mut flood = BufReader::with_capacity(4, Cursor::new(vec![b'y'; 64]));
    assert!(discard(&mut flood, 16).is_err());
    let mut exact = Cursor::new(vec![b'y'; 16]);
    discard(&mut exact, 16).unwrap();
    assert!(exact.fill_buf().unwrap().is_empty());
}

/// Invoked only by the parent below, with stdin redirected from /dev/null.
#[test]
fn non_terminal_child() {
    if !std::path::Path::new(".s94-non-terminal").exists() {
        return;
    }
    let mut question = Vec::new();
    let verdict = match fresh_terminal(&mut question) {
        Ok(()) => "FRESH_ACCEPTED",
        Err(_) => "FRESH_REFUSED",
    };
    println!("{verdict}");
}

#[test]
fn a_non_terminal_stdin_never_establishes_a_fresh_boundary() {
    let root = tempfile::tempdir().unwrap();
    std::fs::write(root.path().join(".s94-non-terminal"), "test only").unwrap();
    let out = std::process::Command::new(std::env::current_exe().unwrap())
        .args(["--exact", "lines::tests::non_terminal_child", "--nocapture"])
        .current_dir(root.path())
        .stdin(std::process::Stdio::null())
        .output()
        .unwrap();
    let text = String::from_utf8_lossy(&out.stdout);
    assert!(out.status.success(), "{text}");
    assert!(text.contains("FRESH_REFUSED"), "{text}");
    assert!(!text.contains("FRESH_ACCEPTED"), "{text}");
}
