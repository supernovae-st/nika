// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>
#![cfg(unix)]
#![allow(clippy::expect_used, clippy::panic, clippy::disallowed_types)]
//! Display preferences through the real bare `nika` host, with isolated HOME,
//! no keys, no harness on PATH, and no submitted request. The existing workspace
//! suite qualifies inspection; these proofs cover the host's layout checkpoint.

#[path = "../../nika-tui/tests/qa_support/vt.rs"]
#[allow(dead_code, reason = "this suite uses part of the shared VT test model")]
mod vt;

use std::io::Write as _;
use std::process::Command;
use std::time::{Duration, Instant};

use expectrl::process::unix::{Signal, WaitStatus};
use expectrl::session::OsSession;
use serde_json::{Value, json};

const F4: &str = "\x1b[14~";
const F6: &str = "\x1b[17~";
const WAIT: Duration = Duration::from_secs(30);

struct Room {
    project: tempfile::TempDir,
    home: tempfile::TempDir,
}

impl Room {
    fn new() -> Self {
        let project = tempfile::tempdir().expect("project");
        let home = tempfile::tempdir().expect("home");
        std::fs::create_dir(home.path().join(".nika")).expect("home state");
        std::fs::write(
            home.path().join(".nika/session-intelligence.json"),
            r#"{"kind":{"kind":"none"},"model":null,"chosen_at":"2026-10-07T00:00:00Z"}"#,
        )
        .expect("kept No AI");
        Self { project, home }
    }

    fn preferences(&self) -> std::path::PathBuf {
        self.home.path().join(".nika/tui-layout.json")
    }

    fn spawn(&self, cols: u16, rows: u16) -> Term {
        let mut command = Command::new("/bin/sh");
        command
            .arg("-c")
            .arg(format!("stty cols {cols} rows {rows} && exec \"$0\""))
            .arg(env!("CARGO_BIN_EXE_nika"))
            .current_dir(self.project.path())
            .env_clear()
            .env("PATH", "/usr/bin:/bin")
            .env("TERM", "xterm-256color")
            .env("LANG", "en_US.UTF-8")
            .env("HOME", self.home.path())
            .env("NIKA_KEYCHAIN", "off")
            .env("NIKA_REDUCED_MOTION", "1");
        Term::new(command, cols, rows)
    }
}

struct Term {
    pty: OsSession,
    screen: vt::Screen,
    eof: bool,
}

impl Term {
    fn new(command: Command, cols: u16, rows: u16) -> Self {
        let mut pty = OsSession::spawn(command).expect("spawn PTY");
        pty.get_process_mut()
            .set_window_size(cols, rows)
            .expect("size");
        let mut screen = vt::Screen::new(cols, rows);
        screen.park_at_bottom();
        Self {
            pty,
            screen,
            eof: false,
        }
    }

    fn pump(&mut self) {
        let mut buf = [0; 16 * 1024];
        while !self.eof {
            match self.pty.try_read(&mut buf) {
                Ok(0) => self.eof = true,
                Ok(n) => {
                    self.screen.feed(&buf[..n]);
                    for reply in self.screen.take_replies() {
                        self.pty.write_all(&reply).expect("terminal reply");
                        self.pty.flush().expect("reply flush");
                    }
                }
                Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => break,
                Err(_) => self.eof = true,
            }
        }
    }

    fn wait(&mut self, needle: &str) {
        let deadline = Instant::now() + WAIT;
        loop {
            self.pump();
            if self.screen.contains(needle) {
                return;
            }
            assert!(
                !self.eof && Instant::now() < deadline,
                "missing {needle:?}\n{}",
                self.screen.text()
            );
            std::thread::sleep(Duration::from_millis(5));
        }
    }

    fn keys(&mut self, text: &str) {
        self.pty.write_all(text.as_bytes()).expect("keys");
        self.pty.flush().expect("key flush");
    }

    fn leave(&mut self) {
        self.keys("\x03");
        self.wait("Ctrl+C again leaves");
        self.keys("\x03");
        let deadline = Instant::now() + WAIT;
        while !self.eof {
            self.pump();
            assert!(
                Instant::now() < deadline,
                "exit timeout\n{}",
                self.screen.text()
            );
            std::thread::sleep(Duration::from_millis(5));
        }
        assert!(matches!(
            self.pty.get_process().wait().expect("wait"),
            WaitStatus::Exited(_, 130)
        ));
        for (mode, on) in [(2004, false), (1004, false), (25, true)] {
            assert_eq!(self.screen.mode(mode), Some(on), "mode {mode}");
        }
        assert!(!self.screen.on_alt());
    }
}

impl Drop for Term {
    fn drop(&mut self) {
        if !self.eof {
            let _ = self.pty.get_process_mut().kill(Signal::SIGKILL);
        }
    }
}

#[test]
fn an_expanded_object_is_kept_and_reopened_without_submitting_the_draft() {
    let room = Room::new();
    let mut term = room.spawn(180, 48);
    term.wait("[+] Expand");
    assert!(
        !room.preferences().exists(),
        "opening is read only for preferences"
    );
    term.keys("keep this unsent draft");
    term.wait("nika › keep this unsent draft");
    let session_record = room.project.path().join(".nika/session-state.json");
    let before = std::fs::read(&session_record).ok();
    term.keys(F4);
    term.wait("[-] Restore");
    term.wait("nika › keep this unsent draft");
    // Size the project column while automatic object expansion remains in
    // view: keyboard resizing settles through the same host seam.
    term.keys(&format!("{F6}{F6}+"));
    let deadline = Instant::now() + WAIT;
    let kept = loop {
        term.pump();
        if let Ok(bytes) = std::fs::read(room.preferences())
            && let Ok(value) = serde_json::from_slice::<Value>(&bytes)
            && value["aside_width"] == 189
        {
            break value;
        }
        assert!(
            Instant::now() < deadline,
            "preference checkpoint did not settle"
        );
        std::thread::sleep(Duration::from_millis(5));
    };
    assert_eq!(kept["schema"], "nika/tui-layout@1");
    assert_eq!(kept["layout"], "workbench");
    assert_eq!(
        kept["aside_width"], 189,
        "34 of 180 columns, rounded to thousandths"
    );
    assert_eq!(kept["conversation_width"], Value::Null);
    assert_eq!(kept["conversation_height"], Value::Null);
    assert_eq!(
        std::fs::read(&session_record).ok(),
        before,
        "layout submitted no turn"
    );
    term.keys("\x1b");
    term.wait("nika › keep this unsent draft");
    term.leave();

    let mut reopened = room.spawn(120, 40);
    reopened.wait("[-] Restore");
    assert_eq!(
        serde_json::from_slice::<Value>(&std::fs::read(room.preferences()).expect("kept bytes"))
            .expect("kept JSON"),
        kept
    );
    assert!(
        !reopened.screen.seen("keep this unsent draft"),
        "display preferences restore no draft"
    );
    reopened.leave();
}

#[test]
fn an_unknown_preference_file_stays_intact_while_expansion_remains_usable() {
    let room = Room::new();
    let future = json!({"schema":"nika/tui-layout@999", "layout":"future"}).to_string();
    std::fs::write(room.preferences(), &future).expect("future preferences");
    let mut term = room.spawn(120, 40);
    term.wait("[+] Expand");
    // The exact notice wraps inside the conversation at 120 columns.
    term.wait("Stored preferences are");
    term.wait("left unchanged.");
    term.keys(F4);
    term.wait("[-] Restore");
    term.keys("still usable");
    term.wait("nika › still usable");
    term.leave();
    assert_eq!(
        std::fs::read_to_string(room.preferences()).expect("preserved bytes"),
        future
    );
}
