// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>
#![cfg(unix)]
#![allow(
    clippy::expect_used,
    clippy::panic,
    reason = "a proof that cannot read its screen fails"
)]
#![allow(
    clippy::disallowed_types,
    reason = "the real binary on a PTY, as tests/pty_restore.rs drives the proto"
)]
//! Journey 0 on the REAL binary (`docs/qa/tui/RECEPTION.md`, J0): bare
//! `nika` at 80x24 in an empty temporary project with a temporary `HOME`,
//! no key anywhere (the environment is cleared), the keychain off, the run
//! keys absent, a minimal `PATH` (no coding-assistant app on it), and every
//! provider's base URL pointed at a loopback listener that counts
//! connections: open → the intelligence guide → invalid/offline choices refused
//! with the guide retained → `Ctrl+C` twice,
//! with ZERO connections.
//!
//! nika-tui cannot build nika-cli's binary for its own tests: the candidate's
//! `nika` is named by `NIKA_TUI_QA_NIKA`, and these proofs run only with
//! `--ignored`:
//!
//! ```text
//! NIKA_TUI_QA_NIKA=<target>/debug/nika cargo test -p nika-tui --test qa_journey0 -- --ignored
//! ```

mod qa_support;

use std::net::TcpListener;
use std::path::PathBuf;
use std::process::Command;
use std::time::Duration;

use qa_support::{Term, assert_restored, exit_code, sized};

/// Network providers in the engine's `profile::CANONICAL_IDS` (the in-process mock cannot
/// connect). Each gets this proof's listener as its base URL, including Scaleway.
const PROVIDERS: [&str; 17] = [
    "anthropic",
    "openai",
    "scaleway",
    "gemini",
    "deepseek",
    "mistral",
    "xai",
    "groq",
    "openrouter",
    "huggingface",
    "nvidia",
    "moonshot",
    "ollama",
    "lmstudio",
    "llamacpp",
    "localai",
    "vllm",
];

/// The native connection picker's heading (`session/intelligence/groups.rs`).
const CHOICE_HINT: &str = "Choose an AI connection";
const INTENT: &str = "digest my monday notes";

/// `std::env::var` for the harness switch naming the binary (not a secret).
#[allow(
    clippy::disallowed_methods,
    reason = "a test-harness switch, never a secret"
)]
fn candidate() -> String {
    std::env::var("NIKA_TUI_QA_NIKA")
        .expect("NIKA_TUI_QA_NIKA names the candidate's nika binary (see the module docs)")
}

/// An empty project and home, removed when the proof ends.
struct Room {
    root: PathBuf,
}

impl Room {
    fn new(tag: &str) -> Self {
        let root =
            std::env::temp_dir().join(format!("nika-tui-qa-j0-{}-{tag}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(root.join("project")).expect("project dir");
        std::fs::create_dir_all(root.join("home")).expect("home dir");
        Self { root }
    }
}

impl Drop for Room {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

/// Connections the listener accepted so far.
fn connections(listener: &TcpListener) -> usize {
    let mut count = 0;
    while listener.accept().is_ok() {
        count += 1;
    }
    count
}

/// Bare `nika` at 80x24 in the room, the environment cleared and rebuilt.
fn first_contact(room: &Room, listener: &TcpListener) -> Term {
    let port = listener.local_addr().expect("listener address").port();
    let mut command: Command = sized(&candidate(), 80, 24);
    command
        .current_dir(room.root.join("project"))
        .env_clear()
        .env("PATH", "/usr/bin:/bin")
        .env("HOME", room.root.join("home"))
        .env("TERM", "xterm-256color")
        .env("NIKA_KEYCHAIN", "off")
        .env("NIKA_RUN_KEY_FILE", room.root.join("absent-run.key"))
        .env("NIKA_RUN_PUB_FILE", room.root.join("absent-run.pub"));
    for id in PROVIDERS {
        command.env(
            format!("NIKA_{}_BASE_URL", id.to_uppercase()),
            format!("http://127.0.0.1:{port}"),
        );
    }
    let mut term = Term::spawn(command, 80, 24);
    term.wait_text(CHOICE_HINT);
    term
}

fn listener() -> TcpListener {
    let listener = TcpListener::bind("127.0.0.1:0").expect("a loopback listener");
    listener.set_nonblocking(true).expect("non-blocking");
    listener
}

fn leave(term: &mut Term) {
    term.send("\x03");
    term.wait_text("Ctrl+C again leaves");
    term.send("\x03");
    let status = term.finish();
    assert_eq!(exit_code(status), Some(130), "{status:?}\n{}", term.dump());
    assert_restored(term);
}

/// Wait for the response AND its idle input row. A response painted before a turn finishes
/// is not permission to send another line: typeahead is deliberately returned to the draft.
fn answer(term: &mut Term, text: &str, feedback: &str) {
    term.paste(text);
    term.wait_workspace_frame("the choice text is ready to send", |screen| {
        screen.lines().get(22).is_some_and(|row| row.contains(text))
    });
    term.send("\r");
    term.wait_workspace_frame(feedback, |screen| {
        let rows = screen.lines();
        screen.contains(feedback)
            && rows
                .last()
                .is_some_and(|row| row.contains("choose · Enter") || row.contains("not taken"))
    });
}

fn still_requires_intelligence(term: &Term) {
    assert!(term.screen.contains(CHOICE_HINT), "{}", term.dump());
    assert!(
        term.screen.contains("Intelligence · required"),
        "{}",
        term.dump()
    );
    assert!(
        !term.screen.contains("No AI in this conversation"),
        "{}",
        term.dump()
    );
    assert!(
        !term.screen.contains("Keep the current selection"),
        "{}",
        term.dump()
    );
    assert!(term.screen.contains("not selected"), "{}", term.dump());
}

#[test]
#[ignore = "needs the candidate's nika binary: NIKA_TUI_QA_NIKA=<path> ... -- --ignored"]
fn first_contact_at_80x24_asks_the_intelligence_and_calls_no_provider() {
    let room = Room::new("contact");
    let listener = listener();
    let mut term = first_contact(&room, &listener);
    still_requires_intelligence(&term);
    answer(&mut term, "4", "offline choice");
    term.settle(Duration::from_millis(500));
    still_requires_intelligence(&term);
    assert_eq!(
        connections(&listener),
        0,
        "a provider was called before exit"
    );
    assert!(
        !room
            .root
            .join("home/.nika/session-intelligence.json")
            .exists()
    );
    leave(&mut term);
    assert_eq!(
        connections(&listener),
        0,
        "a provider was called\n{}",
        term.dump()
    );
    assert_eq!(term.screen.beyond(), 0, "addressed past the 80x24 screen");
}

/// Initial control lines cannot opt the native conversation into offline mode.
/// The guide stays available for a real connection, with no provider call.
#[test]
#[ignore = "needs the candidate's nika binary: NIKA_TUI_QA_NIKA=<path> ... -- --ignored"]
fn initial_control_lines_never_select_offline_mode() {
    let room = Room::new("choice");
    let listener = listener();
    let mut term = first_contact(&room, &listener);
    answer(&mut term, INTENT, "Your message is kept");
    answer(&mut term, "/intelligence", "Choose an intelligence below");
    answer(&mut term, "4", "offline choice");
    answer(&mut term, "cancel", "Your waiting message is kept");
    term.settle(Duration::from_millis(800));
    still_requires_intelligence(&term);
    assert_eq!(
        connections(&listener),
        0,
        "a provider was called before exit"
    );
    assert!(
        !room
            .root
            .join("home/.nika/session-intelligence.json")
            .exists()
    );
    leave(&mut term);
    assert_eq!(connections(&listener), 0, "a provider was called");
}

#[test]
#[ignore = "needs the candidate's nika binary: NIKA_TUI_QA_NIKA=<path> ... -- --ignored"]
fn a_missing_api_key_is_explained_without_selecting_or_sending_the_waiting_request() {
    let room = Room::new("missing-key");
    let listener = listener();
    let mut term = first_contact(&room, &listener);
    answer(&mut term, INTENT, "Your message is kept");
    answer(
        &mut term,
        "/intelligence models 2 deepseek",
        "NIKA_DEEPSEEK_API_KEY",
    );
    assert!(term.screen.contains("key required"), "{}", term.dump());
    assert!(!term.screen.contains("1 models"), "{}", term.dump());
    answer(&mut term, "2 deepseek", "no key for `deepseek`");
    assert!(
        term.screen.contains("NIKA_DEEPSEEK_API_KEY"),
        "{}",
        term.dump()
    );
    assert!(term.screen.contains("not selected"), "{}", term.dump());
    // A refused choice is restored for editing. Remove that exact draft as a person
    // would before entering a different command; nothing automatically consumes it.
    assert!(
        term.screen.contains("choice › 2 deepseek"),
        "{}",
        term.dump()
    );
    term.send(&"\x7f".repeat("2 deepseek".len()));
    term.wait_workspace_frame("the refused choice was cleared", |screen| {
        screen.contains("choice ›  or type your own reply")
    });
    answer(&mut term, "/intelligence", "Choose an intelligence below");
    answer(&mut term, "cancel", "Your waiting message is kept");
    term.settle(Duration::from_millis(500));
    still_requires_intelligence(&term);
    assert_eq!(
        connections(&listener),
        0,
        "a provider was called before exit"
    );
    assert!(
        !room
            .root
            .join("home/.nika/session-intelligence.json")
            .exists()
    );
    leave(&mut term);
    assert_eq!(connections(&listener), 0, "a provider was called");
    assert_eq!(term.screen.beyond(), 0, "addressed past the 80x24 screen");
}

/// A freshly painted idle decision accepts a new bracketed paste and Enter together.
/// This must not be mistaken for a repeated Enter from the previous choice.
#[test]
#[ignore = "needs the candidate's nika binary: NIKA_TUI_QA_NIKA=<path> ... -- --ignored"]
fn a_new_paste_and_enter_after_an_idle_choice_sends_once() {
    let room = Room::new("idle-paste-burst");
    let listener = listener();
    let mut term = first_contact(&room, &listener);
    answer(&mut term, INTENT, "Your message is kept");
    term.send("\x1b[200~2 deepseek\x1b[201~\r");
    term.wait_workspace_frame("the new pasted choice was processed", |screen| {
        screen.contains("no key for `deepseek`")
            && screen
                .lines()
                .last()
                .is_some_and(|row| row.contains("not taken"))
    });
    still_requires_intelligence(&term);
    assert!(
        term.screen.contains("choice › 2 deepseek"),
        "{}",
        term.dump()
    );
    assert_eq!(connections(&listener), 0, "a provider was called");
    leave(&mut term);
    assert_eq!(connections(&listener), 0, "a provider was called");
}
