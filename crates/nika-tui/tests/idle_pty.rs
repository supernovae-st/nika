// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>
#![cfg(unix)]
#![allow(
    clippy::expect_used,
    clippy::panic,
    clippy::disallowed_types,
    reason = "a hermetic child on a real PTY proves the shell's idle ask"
)]

//! The real Workspace loop asks its conversation what changed while it sits idle, on its own
//! light clock (`Options::idle_every`), and says the news with no key pressed: a schedule's
//! trigger that fired while the door stood open shows without a keystroke, and once.

mod qa_support;

use std::time::Duration;

use nika_tui::app::{self, Options};
use nika_tui::model::{Beat, Committed, Conversation, Handoff, Kind, Presentation, Turn, Waiting};
use qa_support::{Term, assert_restored, exit_code, sized};

const CHILD: &str = "NIKA_TUI_IDLE_CHILD";
const COLS: u16 = 160;
const ROWS: u16 = 48;
/// What the conversation says on its second idle ask, and never again.
const NEWS: &str = "daily.nika fired a run";

/// A conversation with news on its second idle ask only: the first finds nothing, so the
/// line shows that the shell asked again on its own.
struct Away {
    asked: usize,
}

impl Conversation for Away {
    fn open(&mut self) -> Vec<Beat> {
        vec![Beat::Wait(Waiting::Free)]
    }

    fn submit(&mut self, _line: &str) -> Turn {
        Turn::new(Vec::new(), None)
    }

    fn perform(&mut self, _handoff: &Handoff) -> Vec<Beat> {
        Vec::new()
    }

    fn idle(&mut self) -> Vec<Beat> {
        self.asked += 1;
        if self.asked == 2 {
            vec![Beat::Say(Committed::new(Kind::Notice, NEWS))]
        } else {
            Vec::new()
        }
    }
}

#[test]
#[allow(
    clippy::disallowed_methods,
    reason = "a test-only child switch, not a secret"
)]
fn idle_child_host() {
    if std::env::var(CHILD).is_err() {
        return;
    }
    let mut options = Options::new(Presentation::Workspace);
    options.term = Some("xterm-256color".to_owned());
    options.ascii = true;
    // No reveal frame wakes the shell: only its idle ask can.
    options.reduced_motion = true;
    options.idle_every = Some(Duration::from_millis(300));
    let exit = app::run(Away { asked: 0 }, options).expect("the real shell");
    std::process::exit(i32::from(exit.code()));
}

/// With no key pressed, the news of the second idle ask shows in the conversation, once
/// while the asks go on, and the door still leaves on its two `Ctrl+C`, the terminal
/// restored.
#[test]
fn an_idle_shell_says_what_changed_without_a_keystroke() {
    let exe = std::env::current_exe().expect("this test executable");
    let mut command = sized(exe.to_str().expect("UTF-8 executable"), COLS, ROWS);
    command
        .args([
            "--exact",
            "idle_child_host",
            "--nocapture",
            "--test-threads=1",
        ])
        .env(CHILD, "1");
    let mut term = Term::spawn(command, COLS, ROWS);
    term.wait_text(NEWS);
    term.settle(Duration::from_millis(1200));
    assert_eq!(
        term.screen.text().matches(NEWS).count(),
        1,
        "{}",
        term.dump()
    );
    term.send("\x03");
    term.wait_text("Ctrl+C again leaves");
    term.send("\x03");
    let status = term.finish();
    assert_eq!(exit_code(status), Some(130), "{status:?}\n{}", term.dump());
    assert_restored(&term);
}
