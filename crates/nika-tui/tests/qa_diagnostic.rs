// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>
#![cfg(unix)]
#![allow(clippy::expect_used, clippy::panic)]
//! Native shell routing over a typed display fixture, not Live/provider proof.

mod qa_support;

use std::time::Duration;

use qa_support::{FREE, QUESTION, REPLY, Term, assert_restored, exit_code};

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
        assert!(
            term.screen
                .contains("nothing was sent to the authoring model"),
            "{}",
            term.dump()
        );
        term.send("\x1b");
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
