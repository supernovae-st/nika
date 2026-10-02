// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The harness of the TUI reception suites (`docs/qa/tui/RECEPTION.md`).
//!
//! One real binary on one real PTY of a chosen size. Everything it writes is
//! kept as raw bytes, for the proofs about the stream (restore sequences,
//! colour, silence), and read into a [`vt::Screen`], for the proofs about what
//! the human sees. The screen answers the renderer's terminal queries from its
//! own state, so no proof hand-types a cursor row. Every wait is bounded and
//! dumps the screen and the tail of the stream when it expires.

#![allow(
    dead_code,
    reason = "each QA suite uses its own part of the shared harness"
)]
#![allow(
    clippy::expect_used,
    clippy::panic,
    reason = "a harness that cannot drive its PTY fails the proof it serves"
)]
#![allow(
    clippy::disallowed_types,
    reason = "the renderer refuses anything but a terminal: only a PTY reaches it (tests/pty_restore.rs)"
)]

pub(crate) mod vt;

use std::io::Write as _;
use std::process::Command;
use std::time::{Duration, Instant};

use expectrl::process::unix::{Signal, WaitStatus};
use expectrl::session::OsSession;

/// The four sizes the terminal matrix qualifies (RECEPTION.md, J6).
pub(crate) const SIZES: [(u16, u16); 4] = [(80, 24), (100, 32), (120, 40), (160, 48)];
/// The longest any single wait may take before the proof fails with a dump.
pub(crate) const WAIT: Duration = Duration::from_secs(20);

pub(crate) const PASTE_OFF: &str = "\x1b[?2004l";
pub(crate) const FOCUS_OFF: &str = "\x1b[?1004l";
pub(crate) const CURSOR_SHOW: &str = "\x1b[?25h";
pub(crate) const ALT_OFF: &str = "\x1b[?1049l";

/// The prompts of the four waiting states the demo fixture walks through.
pub(crate) const FREE: &str = "nika ›";
pub(crate) const REPLY: &str = "reply ›";
pub(crate) const APPLY: &str = "apply? ›";
pub(crate) const ANSWER: &str = "answer ›";
/// What each turn of `Script::demo` prints, by a phrase that turn alone says.
pub(crate) const QUESTION: &str = "Which file holds the notes to digest?";
pub(crate) const PROPOSAL: &str = "identity · 9f3c1a";
pub(crate) const SAVED: &str = "saved ./digest-notes.nika";
pub(crate) const GATE: &str = "overwrite the existing file?";
pub(crate) const RESULT: &str = "produced ./digest.md";
/// The hint rows of the idle prompt and of the gate.
pub(crate) const FREE_HINT: &str = "describe work · /help";
pub(crate) const GATE_HINT: &str = "approve or refuse · nothing else answers a gate";

/// One turn of the demo journey: what the human sends, what the turn prints,
/// the prompt it leaves.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Step {
    pub(crate) send: &'static str,
    pub(crate) shows: &'static str,
    pub(crate) prompt: &'static str,
}

/// `Script::demo` from a sentence to a result through a gate.
pub(crate) const JOURNEY: [Step; 5] = [
    Step {
        send: "digest my monday notes\r",
        shows: QUESTION,
        prompt: REPLY,
    },
    Step {
        send: "./notes/lundi.md\r",
        shows: PROPOSAL,
        prompt: APPLY,
    },
    Step {
        send: "yes\r",
        shows: SAVED,
        prompt: FREE,
    },
    Step {
        send: "run it\r",
        shows: GATE,
        prompt: ANSWER,
    },
    Step {
        send: "yes\r",
        shows: RESULT,
        prompt: FREE,
    },
];

/// `program` under `/bin/sh -c 'stty … && exec …'`: the PTY has its size
/// BEFORE the program starts, so the first paint already knows it (a size set
/// after the spawn races the program's first read of it). The colour
/// variables of the caller's shell are cleared (crossterm itself reads
/// `NO_COLOR`); a proof that wants one sets it.
pub(crate) fn sized(program: &str, cols: u16, rows: u16) -> Command {
    let mut command = Command::new("/bin/sh");
    command
        .arg("-c")
        .arg(format!(
            "stty cols {cols} rows {rows} && exec \"$0\" \"$@\""
        ))
        .arg(program)
        .env("TERM", "xterm-256color")
        .env_remove("NO_COLOR")
        .env_remove("CLICOLOR")
        .env_remove("CLICOLOR_FORCE");
    command
}

/// A process on a PTY, its raw output and the screen it painted.
pub(crate) struct Term {
    pty: OsSession,
    pub(crate) screen: vt::Screen,
    raw: Vec<u8>,
    eof: bool,
}

impl Term {
    /// `nika-tui-proto` with `args` on a `cols` × `rows` PTY.
    pub(crate) fn proto(args: &[&str], cols: u16, rows: u16) -> Self {
        Self::proto_with(args, cols, rows, &[])
    }

    /// [`Term::proto`] with extra environment.
    pub(crate) fn proto_with(args: &[&str], cols: u16, rows: u16, env: &[(&str, &str)]) -> Self {
        let mut command = sized(env!("CARGO_BIN_EXE_nika-tui-proto"), cols, rows);
        command.args(args);
        for (key, value) in env {
            command.env(key, value);
        }
        Self::spawn(command, cols, rows)
    }

    /// Any command on a PTY of that size (the command sets the size itself,
    /// see [`sized`]); the screen starts with the cursor on its last row.
    pub(crate) fn spawn(command: Command, cols: u16, rows: u16) -> Self {
        let pty = OsSession::spawn(command).expect("pty spawn");
        let mut screen = vt::Screen::new(cols, rows);
        screen.park_at_bottom();
        Self {
            pty,
            screen,
            raw: Vec::new(),
            eof: false,
        }
    }

    /// The process id.
    pub(crate) fn pid(&self) -> i32 {
        self.pty.get_process().pid().as_raw()
    }

    /// Read everything the process wrote so far into the screen and answer
    /// its queries; the byte count read.
    pub(crate) fn pump(&mut self) -> usize {
        let mut total = 0;
        let mut buf = [0u8; 16 * 1024];
        while !self.eof {
            match self.pty.try_read(&mut buf) {
                Ok(0) => self.eof = true,
                Ok(n) => {
                    total += n;
                    self.raw.extend_from_slice(&buf[..n]);
                    self.screen.feed(&buf[..n]);
                    for reply in self.screen.take_replies() {
                        // The process waits on this answer; a write that
                        // fails means it is gone, which the next read says.
                        let _ = self.pty.write_all(&reply);
                        let _ = self.pty.flush();
                    }
                }
                Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => break,
                Err(_) => self.eof = true,
            }
        }
        total
    }

    /// Pump until `done` holds on the screen; a dump on timeout or exit.
    pub(crate) fn wait_until(&mut self, what: &str, done: impl Fn(&vt::Screen) -> bool) {
        let deadline = Instant::now() + WAIT;
        loop {
            let read = self.pump();
            if done(&self.screen) {
                return;
            }
            assert!(
                !self.eof && Instant::now() <= deadline,
                "{what}: never reached ({}).\n{}",
                if self.eof {
                    "the process ended"
                } else {
                    "timeout"
                },
                self.dump()
            );
            if read == 0 {
                std::thread::sleep(Duration::from_millis(2));
            }
        }
    }

    /// Wait until a visible row contains `needle`.
    pub(crate) fn wait_text(&mut self, needle: &str) {
        self.wait_until(needle, |screen| screen.contains(needle));
    }

    /// Wait until a visible row starts with `prompt` (the live prompt row;
    /// an echoed line starts with its own marker instead).
    pub(crate) fn wait_prompt(&mut self, prompt: &str) {
        self.wait_until(prompt, |screen| screen.row_starting(prompt).is_some());
    }

    /// Pump for `window`, whatever arrives; the byte count read.
    pub(crate) fn settle(&mut self, window: Duration) -> usize {
        let deadline = Instant::now() + window;
        let mut total = 0;
        while Instant::now() < deadline && !self.eof {
            let read = self.pump();
            total += read;
            if read == 0 {
                std::thread::sleep(Duration::from_millis(5));
            }
        }
        total
    }

    /// Type (or paste) `text` as the terminal would send it.
    pub(crate) fn send(&mut self, text: &str) {
        self.pty
            .write_all(text.as_bytes())
            .expect("write to the pty");
        self.pty.flush().expect("flush the pty");
    }

    /// A bracketed paste of `text`, as a terminal wraps it.
    pub(crate) fn paste(&mut self, text: &str) {
        self.send(&format!("\x1b[200~{text}\x1b[201~"));
    }

    /// Resize the terminal: the screen first (it reads what follows at the
    /// new size), then the PTY, which sends `SIGWINCH`.
    pub(crate) fn resize(&mut self, cols: u16, rows: u16) {
        self.pump();
        self.screen.resize(cols, rows);
        self.pty
            .get_process_mut()
            .set_window_size(cols, rows)
            .expect("resize the pty");
    }

    /// Send a signal to the process.
    pub(crate) fn signal(&mut self, signal: Signal) {
        self.pty.get_process_mut().kill(signal).expect("signal");
    }

    /// Walk the demo journey from its first step through `steps`, each turn
    /// awaited on screen (what it prints, then the prompt it leaves).
    pub(crate) fn walk(&mut self, steps: &[Step]) {
        for step in steps {
            self.send(step.send);
            let (shows, prompt) = (step.shows, step.prompt);
            self.wait_until(shows, |screen| {
                screen.seen(shows) && screen.row_starting(prompt).is_some()
            });
        }
    }

    /// Pump until the process ends; its exit status.
    pub(crate) fn finish(&mut self) -> WaitStatus {
        let deadline = Instant::now() + WAIT;
        while !self.eof {
            if self.pump() == 0 {
                assert!(
                    Instant::now() < deadline,
                    "the process never ended.\n{}",
                    self.dump()
                );
                std::thread::sleep(Duration::from_millis(2));
            }
        }
        self.pty.get_process().wait().expect("wait for the process")
    }

    /// How far the raw stream has come (a mark for [`Term::raw_since`]).
    pub(crate) fn mark(&self) -> usize {
        self.raw.len()
    }

    /// Everything written, lossily decoded.
    pub(crate) fn raw_text(&self) -> String {
        String::from_utf8_lossy(&self.raw).into_owned()
    }

    /// Everything written after `mark`, lossily decoded.
    pub(crate) fn raw_since(&self, mark: usize) -> String {
        String::from_utf8_lossy(self.raw.get(mark..).unwrap_or_default()).into_owned()
    }

    /// The raw bytes written after `mark`.
    pub(crate) fn bytes_since(&self, mark: usize) -> &[u8] {
        self.raw.get(mark..).unwrap_or_default()
    }

    /// Spin (no sleep) until the bytes after `mark` contain `needle`; the
    /// instant it was read. For latency probes only: it burns one core.
    pub(crate) fn spin_until_bytes(&mut self, mark: usize, needle: &[u8]) -> Instant {
        let deadline = Instant::now() + WAIT;
        loop {
            self.pump();
            let now = Instant::now();
            if self
                .bytes_since(mark)
                .windows(needle.len())
                .any(|w| w == needle)
            {
                return now;
            }
            assert!(
                now < deadline && !self.eof,
                "{needle:?} never echoed.\n{}",
                self.dump()
            );
        }
    }

    /// The screen, the history's tail and the stream's tail, for a failure.
    pub(crate) fn dump(&self) -> String {
        let lines = self.screen.lines();
        let screen: Vec<String> = lines
            .iter()
            .enumerate()
            .map(|(row, line)| format!("{row:>3}|{line}"))
            .collect();
        let history = self.screen.history();
        let tail_from = history.len().saturating_sub(12);
        let raw = self.raw_text();
        let from = raw.char_indices().rev().nth(2499).map_or(0, |(at, _)| at);
        let raw_tail = &raw[from..];
        format!(
            "--- screen {:?} cursor {:?} alt {} ---\n{}\n--- history tail ---\n{}\n--- stream tail ---\n{:?}",
            self.screen.size(),
            self.screen.cursor(),
            self.screen.on_alt(),
            screen.join("\n"),
            history[tail_from..].join("\n"),
            raw_tail
        )
    }
}

/// The exit code of an exited process, `None` for a signal death.
pub(crate) fn exit_code(status: WaitStatus) -> Option<i32> {
    match status {
        WaitStatus::Exited(_, code) => Some(code),
        _ => None,
    }
}

/// The terminal handed back, read from the screen's FINAL state (every
/// sequence the process wrote, in order, the last frame included):
/// bracketed paste and focus reports off, the cursor shown, the main screen
/// on. Raw mode is the PTY's line discipline; its proof is the exit itself.
pub(crate) fn assert_restored(term: &Term) {
    let screen = &term.screen;
    for (mode, on, what) in [
        (2004, false, "bracketed paste left on"),
        (1004, false, "focus reporting left on"),
        (25, true, "cursor left hidden"),
    ] {
        assert_eq!(screen.mode(mode), Some(on), "{what}.\n{}", term.dump());
    }
    assert!(
        !screen.on_alt(),
        "left on the alternate screen.\n{}",
        term.dump()
    );
}
