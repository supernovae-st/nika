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
//! connections: open → intent → the intelligence choice → « No AI » → the
//! question or clarification the deterministic path asks → `Ctrl+C` twice,
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

use qa_support::{FREE, REPLY, Term, assert_restored, exit_code, sized};

/// Every provider of the canon but `mock`: each gets the listener as its
/// base URL (`NIKA_<ID>_BASE_URL`, the override a run itself honours).
const PROVIDERS: [&str; 16] = [
    "ollama",
    "lmstudio",
    "llamacpp",
    "localai",
    "vllm",
    "mistral",
    "anthropic",
    "openai",
    "gemini",
    "deepseek",
    "xai",
    "groq",
    "openrouter",
    "huggingface",
    "nvidia",
    "moonshot",
];
/// The choice screen's own words and hint (nika-session intelligence.rs,
/// nika-tui model.rs `Waiting::Choosing`).
const CHOICE: &str = "No AI in this conversation";
const CHOICE_HINT: &str = "1 account";
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
    term.wait_prompt(FREE);
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

#[test]
#[ignore = "needs the candidate's nika binary: NIKA_TUI_QA_NIKA=<path> ... -- --ignored"]
fn first_contact_at_80x24_asks_the_intelligence_and_calls_no_provider() {
    let room = Room::new("contact");
    let listener = listener();
    let mut term = first_contact(&room, &listener);
    term.send(&format!("{INTENT}\r"));
    term.wait_until("the intelligence choice", |screen| {
        screen.seen(CHOICE)
            && [CHOICE_HINT, "2 API", "3 local", "4 no AI", "cancel"]
                .iter()
                .all(|choice| screen.contains(choice))
    });
    term.send("4\r");
    term.wait_until("a question, a clarification or the free prompt", |screen| {
        !screen.contains(CHOICE_HINT)
            && (screen.row_starting(REPLY).is_some() || screen.row_starting(FREE).is_some())
    });
    term.settle(Duration::from_millis(500));
    leave(&mut term);
    assert_eq!(
        connections(&listener),
        0,
        "a provider was called\n{}",
        term.dump()
    );
    assert_eq!(term.screen.beyond(), 0, "addressed past the 80x24 screen");
}

/// The choice screen is a decision: a `4` typed with the intent, before the
/// screen exists, must not pick « No AI » for the human. This regression
/// was first observed at 513ca8465; the candidate's decision/typeahead path
/// is exercised here, with the same explicit binary prerequisite as J0.
#[test]
#[ignore = "needs the candidate's nika binary: NIKA_TUI_QA_NIKA=<path> ... -- --ignored"]
fn typeahead_never_picks_an_intelligence_on_a_screen_painted_after_it() {
    let room = Room::new("choice");
    let listener = listener();
    let mut term = first_contact(&room, &listener);
    term.send(&format!("{INTENT}\r4\r"));
    term.wait_until("the intelligence choice", |screen| screen.seen(CHOICE));
    term.settle(Duration::from_millis(800));
    assert!(
        term.screen.contains(CHOICE_HINT),
        "a 4 typed before the choice screen picked an intelligence\n{}",
        term.dump()
    );
    leave(&mut term);
    assert_eq!(connections(&listener), 0, "a provider was called");
}
