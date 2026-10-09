// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The run cost review the session holds while the host keeps the child: one yes runs it once,
//! a decline, leaving or an interruption sends nothing, the evidence and the read-only commands
//! answer beside it, and a line typed for anything else decides nothing.

use super::RUN_STILL_WAITS;
use crate::outcome::{RefusalClass, ReviewId};
use crate::runtime::tests::{ready_with, tree};
use crate::runtime::{REVIEW_NOT_SHOWN, SessionRuntime, TurnOutcome};
use crate::work::Waiting;

const QUESTION: &str =
    "Fresh Run cost decision · mock/priced at https://api.example.test\nApproves this Run once.";
const DETAILS: &str = "Run cost decision details · challenge n-1 · reading them approves nothing\nSource SHA-256: abc";

/// A session whose requested run's child waits at its cost review.
fn reviewing() -> (tempfile::TempDir, SessionRuntime, ReviewId) {
    let dir = tree();
    let mut s = ready_with(dir.path(), vec![]);
    let review = s.run_review_asked(QUESTION, DETAILS);
    (dir, s, review)
}

fn shown(review: &ReviewId) -> Waiting {
    Waiting::RunReview {
        review: review.clone(),
    }
}

fn aside(outcome: TurnOutcome) -> String {
    let TurnOutcome::Aside(text) = outcome else {
        panic!("an answer beside the review: {outcome:?}");
    };
    text
}

#[test]
fn a_review_waits_before_anything_else_and_the_snapshot_names_only_its_identity() {
    let (_dir, s, review) = reviewing();
    assert_eq!(s.waiting(), shown(&review));
    let json = serde_json::to_value(s.work()).expect("serializes");
    assert_eq!(json["waiting"]["kind"], "run_review");
    assert_eq!(json["waiting"]["review"], review.as_str());
    let text = json.to_string();
    assert!(
        !text.contains("api.example.test") && !text.contains("challenge n-1"),
        "the screen and the evidence stay with the host: {text}"
    );
}

#[test]
fn one_yes_shown_runs_the_child_once_and_the_review_is_gone() {
    for yes in ["yes", "oui", " OK "] {
        let (_dir, mut s, review) = reviewing();
        assert_eq!(
            s.submit(yes, &shown(&review)),
            TurnOutcome::RunReviewed {
                review: review.clone(),
                approve: true
            },
            "{yes:?}"
        );
        assert_eq!(s.waiting(), Waiting::Free);
        // A second yes has nothing left to approve, and it is no new request either.
        let TurnOutcome::Refusal(again) = s.submit(yes, &shown(&review)) else {
            panic!("a decided review takes no second answer: {yes:?}");
        };
        assert_eq!(again.class, RefusalClass::StaleRevision, "{again}");
        assert!(again.text.ends_with("nothing was sent"), "{again}");
    }
}

#[test]
fn a_decline_sends_nothing_and_ends_the_review() {
    for no in ["no", "non", "cancel", "non, pas maintenant"] {
        let (_dir, mut s, review) = reviewing();
        assert_eq!(
            s.submit(no, &shown(&review)),
            TurnOutcome::RunReviewed {
                review: review.clone(),
                approve: false
            },
            "{no:?}"
        );
        assert_eq!(s.waiting(), Waiting::Free);
    }
}

#[test]
fn evidence_help_status_and_unknown_lines_answer_beside_and_the_review_keeps_waiting() {
    let (_dir, mut s, review) = reviewing();
    for evidence in ["details", "/details", "/why", "why?"] {
        assert_eq!(aside(s.submit(evidence, &shown(&review))), DETAILS);
    }
    for command in ["/help", "/status"] {
        let text = aside(s.submit(command, &shown(&review)));
        assert!(text.ends_with(RUN_STILL_WAITS), "{command}");
    }
    for unknown in ["change input to revised", "/proof", "maybe"] {
        let text = aside(s.submit(unknown, &shown(&review)));
        assert!(
            text.starts_with(&format!(
                "« {unknown} » is not a yes or a no · nothing was sent"
            )),
            "the unknown line"
        );
    }
    assert_eq!(s.waiting(), shown(&review), "nothing decided it");
}

#[test]
fn a_line_typed_for_anything_else_decides_nothing_at_the_review() {
    let (_dir, mut s, review) = reviewing();
    // Typed ahead, before the review was shown: refused, never a yes.
    let TurnOutcome::Refusal(why) = s.submit("yes", &Waiting::Free) else {
        panic!("an unshown review takes no answer");
    };
    assert_eq!(why.class, RefusalClass::StaleRevision);
    assert_eq!(why.text, REVIEW_NOT_SHOWN);
    // An answer naming an earlier review, even with the same screen, is stale.
    let earlier = review.clone();
    let later = s.run_review_asked(QUESTION, DETAILS);
    assert_ne!(earlier, later, "each review is its own identity");
    let TurnOutcome::Refusal(stale) = s.submit("yes", &shown(&earlier)) else {
        panic!("the earlier review is not the one waiting");
    };
    assert_eq!(stale.class, RefusalClass::StaleRevision);
    assert_eq!(s.waiting(), shown(&later));
}

#[test]
fn leaving_or_an_interruption_sends_nothing() {
    let (_dir, mut s, _review) = reviewing();
    // Leaving goes through even when the line was typed at another prompt.
    assert_eq!(s.submit("/quit", &Waiting::Free), TurnOutcome::Quit);
    assert_eq!(s.waiting(), Waiting::Free);

    let (_dir, mut s, review_two) = reviewing();
    assert_eq!(
        s.decline_run_review(),
        Some(TurnOutcome::RunReviewed {
            review: review_two,
            approve: false
        })
    );
    assert_eq!(s.waiting(), Waiting::Free);
    assert_eq!(
        s.decline_run_review(),
        None,
        "an interruption with nothing waiting"
    );
}
