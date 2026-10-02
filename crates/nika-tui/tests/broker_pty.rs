// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>
#![cfg(unix)]
#![allow(clippy::expect_used, clippy::panic, clippy::disallowed_types)]
//! Fresh consent drains bytes still in the terminal while its reader is parked.

#[path = "qa_support/child.rs"]
mod child;
mod qa_support;

use std::io::Write as _;
use std::path::Path;
use std::time::{Duration, Instant};

use crossterm::event::KeyCode;
use nika_tui::events::{Broker, UiEvent};
use qa_support::{Term, exit_code, sized};

const RELEASE: &str = "NIKA_TUI_BROKER_RELEASE";

fn say(text: &str) {
    write!(std::io::stdout(), "\r\n{text}\r\n").expect("write marker");
    std::io::stdout().flush().expect("flush marker");
}

#[test]
#[allow(
    clippy::disallowed_methods,
    reason = "a parent-owned fixture release path"
)]
fn parked_broker_child() {
    let Ok(release) = std::env::var(RELEASE) else {
        return;
    };
    crossterm::terminal::enable_raw_mode().expect("raw mode");
    let mut broker = Broker::start();
    broker.pause();
    assert!(broker.try_recv().is_none(), "the broker starts empty");
    say("reader parked");
    let deadline = Instant::now() + Duration::from_secs(20);
    while !Path::new(&release).exists() {
        assert!(Instant::now() < deadline, "parent never released drain");
        std::thread::sleep(Duration::from_millis(2));
    }
    assert!(
        broker.try_recv().is_none(),
        "the parked reader consumed input"
    );
    let drained = broker.discard_typeahead().expect("fresh input boundary");
    let keys: Vec<_> = drained
        .iter()
        .filter_map(|event| match event {
            UiEvent::Key(key) => Some(key.code),
            _ => None,
        })
        .collect();
    assert_eq!(
        keys,
        [
            KeyCode::Char('y'),
            KeyCode::Char('e'),
            KeyCode::Char('s'),
            KeyCode::Enter
        ]
    );
    say("old answer drained");
    let deadline = Instant::now() + Duration::from_secs(20);
    let mut fresh = Vec::new();
    while fresh.len() < 2 {
        assert!(Instant::now() < deadline, "fresh answer never arrived");
        if let Some(UiEvent::Key(key)) = broker.try_recv() {
            fresh.push(key.code);
        } else {
            std::thread::sleep(Duration::from_millis(2));
        }
    }
    assert_eq!(fresh, [KeyCode::Char('n'), KeyCode::Enter]);
    broker.stop();
    crossterm::terminal::disable_raw_mode().expect("restore raw mode");
    say("fresh answer received");
}

#[test]
fn a_fresh_boundary_drains_terminal_bytes_before_accepting_new_keys() {
    let release = child::Release::new("parked-broker");
    let exe = std::env::current_exe().expect("test executable");
    let mut command = sized(exe.to_str().expect("executable path"), 100, 32);
    command
        .args([
            "--exact",
            "parked_broker_child",
            "--nocapture",
            "--test-threads=1",
        ])
        .env(RELEASE, release.path());
    let mut term = Term::spawn(command, 100, 32);
    term.wait_text("reader parked");
    term.send("yes\r");
    release.open();
    term.wait_text("old answer drained");
    term.send("n\r");
    term.wait_text("fresh answer received");
    assert_eq!(exit_code(term.finish()), Some(0));
}
