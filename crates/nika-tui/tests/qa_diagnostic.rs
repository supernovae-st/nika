// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>
#![cfg(unix)]
#![allow(clippy::expect_used, clippy::panic)]
//! Native shell routing over a typed display fixture, not Live/provider proof.

#[path = "qa_support/child.rs"]
mod child;
mod qa_support;

use std::time::Duration;

use qa_support::{FREE, QUESTION, REPLY, Term, assert_restored, exit_code};

#[test]
fn qa_child_host() {
    child::host();
}

/// The turn cannot finish until after the diagnostic closes. Its palette
/// route and plain Enter return work from all three regions without sending
/// the kept draft, answering a gate or queuing another turn.
#[test]
fn the_palette_diagnostic_returns_without_sending_while_a_turn_is_still_held() {
    for origin in 0..3 {
        let release = child::Release::new(&format!("diagnostic-{origin}"));
        let mut term = child::spawn("slow-diagnostic:0:workspace", Some(release.path()), 120, 40);
        term.wait_text(FREE);
        term.wait_text("release.nika");
        term.send("begin\r");
        term.wait_text(child::BUSY);
        term.send("keep this held draft");
        term.wait_text("nika › keep this held draft");
        for _ in 0..origin {
            term.send("\x1b[17~");
        }
        term.send("\x0f");
        term.wait_text("commands ›");
        term.send("diagnostic");
        term.wait_text("Full diagnostic");
        term.send("\r");
        term.wait_until("the diagnostic opened before release", |screen| {
            screen.lines()[0].contains("Full diagnostic")
                && screen.lines()[0].contains("the Session's words, read only")
        });
        assert!(!release.path().exists());
        assert!(!term.screen.seen(child::DONE), "{}", term.dump());
        assert!(
            term.screen.contains("ADMISSION_UNTRUSTED"),
            "{}",
            term.dump()
        );
        term.send("\r");
        term.wait_until("Enter returned to the still-working shell", |screen| {
            !screen.lines()[0].contains("Full diagnostic")
                && screen.contains(child::BUSY)
                && screen.contains("keep this held draft")
        });
        term.settle(Duration::from_millis(200));
        assert!(
            !term.screen.contains("Enter sends"),
            "Enter passed through the reader into the busy composer: {}",
            term.dump()
        );
        assert!(!term.screen.seen(child::DONE), "{}", term.dump());
        release.open();
        term.wait_text(child::DONE);
        term.wait_text("nika › keep this held draft");
        term.send("\x1b");
        term.wait_text("nika › keep this held draft");
        term.settle(Duration::from_millis(200));
        for unexpected in [child::SECOND, QUESTION, "Save these changes"] {
            assert!(
                !term.screen.seen(unexpected),
                "{unexpected}: {}",
                term.dump()
            );
        }
        leave(&mut term);
    }
}

fn leave(term: &mut Term) {
    term.send("\x03");
    term.wait_text("Ctrl+C again leaves");
    term.send("\x03");
    let status = term.finish();
    assert_eq!(exit_code(status), Some(130), "{status:?}\n{}", term.dump());
    assert_restored(term);
}

/// Choosing F2 must reach the same read-only reader as pressing F2, while
/// idle and during a turn. Neither route submits the kept draft or answers
/// the next question, and the diagnostic retains the raw admission evidence.
#[test]
fn the_palette_diagnostic_opens_at_rest_and_while_a_turn_is_working() {
    for busy in [false, true] {
        let pace = if busy { "2000" } else { "0" };
        let mut term = Term::proto(
            &["--focus", "--demo-diagnostic", "--demo-pace", pace],
            120,
            40,
        );
        term.wait_prompt(FREE);
        if busy {
            term.send("begin\r");
            term.wait_text("the demo holds this turn");
        }
        term.send("keep this exact draft");
        term.wait_text("nika › keep this exact draft");
        term.send("\x0f");
        term.wait_text("commands ›");
        term.send("diagnostic");
        term.wait_text("Full diagnostic");
        term.send("\r");
        term.wait_until("the read-only diagnostic frame", |screen| {
            screen.lines()[0].contains("Full diagnostic")
                && screen.lines()[0].contains("the Session's words, read only")
        });
        assert!(
            term.screen.contains("ADMISSION_UNTRUSTED"),
            "{}",
            term.dump()
        );
        let words = term
            .screen
            .lines()
            .iter()
            .map(|row| row.trim().trim_matches('│').trim())
            .collect::<Vec<_>>()
            .join(" ");
        assert!(
            words.contains("this workflow-authoring request was not sent"),
            "{}",
            term.dump()
        );
        term.send(if busy { "\x1b" } else { "\r" });
        term.wait_text("keep this exact draft");
        if busy {
            term.wait_prompt(REPLY);
            assert!(
                term.screen.contains("keep this exact draft"),
                "{}",
                term.dump()
            );
        } else {
            term.wait_text("nika › keep this exact draft");
            assert!(!term.screen.seen(QUESTION), "{}", term.dump());
        }
        term.settle(Duration::from_millis(100));
        assert!(!term.screen.seen("Save these changes"), "{}", term.dump());
        leave(&mut term);
    }
}

/// Inline diagnostics scroll inside the actual twelve-row viewport even
/// when the terminal is taller; the full raw suffix remains reachable.
#[test]
fn an_inline_diagnostic_reaches_its_last_raw_row() {
    let mut term = Term::proto(&["--demo-diagnostic"], 60, 40);
    term.wait_prompt(FREE);
    term.send("keep inline draft");
    term.wait_text("nika › keep inline draft");
    term.send("\x1bOQ");
    term.wait_text("Full diagnostic");
    term.send("\x1b[F");
    term.wait_until("the raw suffix inside the inline diagnostic", |screen| {
        let rows = screen.lines();
        rows.iter()
            .position(|row| row.contains("Full diagnostic"))
            .is_some_and(|start| {
                rows.iter()
                    .skip(start + 1)
                    .take(10)
                    .any(|row| row.contains("session again"))
            })
    });
    term.send("\x1b");
    term.wait_text("nika › keep inline draft");
    assert!(!term.screen.seen(QUESTION), "{}", term.dump());
    leave(&mut term);
}
