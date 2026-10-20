// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Two outputs, one whose start the person's words leave open: the person's pick of where it
//! starts, or the path they type for it, reaches the verifier as the Session describes it,
//! settles that name and no other, and both outputs stay bound, each on its own.

use crate::scenarios::{PAYLOAD_NAME, PAYLOAD_PATH, RELIRE, REPORT};
use crate::{binding, offered_values, outcome_kind, reached_the_author, run};

/// The question offers the payload's readings; the person picks the path alone. The proposal
/// door reads that pick within its scope: the document writing `out/notification.json` is
/// proposed, the pick and the report each bound.
#[test]
fn a_native_pick_settles_where_a_name_starts_beside_another_output() {
    let journey = run("two_outputs_answered");
    let asked = journey.step(0);
    assert_eq!(outcome_kind(asked), "question", "{asked:#}");
    assert_eq!(
        offered_values(asked),
        [PAYLOAD_NAME, PAYLOAD_PATH],
        "{asked:#}"
    );
    let proposed = journey.step(1);
    assert_eq!(outcome_kind(proposed), "proposal", "{proposed:#}");
    assert!(binding(proposed, PAYLOAD_PATH).is_some(), "{proposed:#}");
    assert!(binding(proposed, REPORT).is_some(), "{proposed:#}");
}

/// The person types the payload's file in their own words: the Session describes that free entry
/// within its question, the proposal door settles the payload's name with it, and the document
/// writing `out/notification.json` is proposed, both outputs bound.
#[test]
fn a_typed_reply_settles_where_a_name_starts() {
    let journey = run("two_outputs_typed");
    let asked = journey.step(0);
    assert_eq!(outcome_kind(asked), "question", "{asked:#}");
    let proposed = journey.step(1);
    assert_eq!(outcome_kind(proposed), "proposal", "{proposed:#}");
    assert!(binding(proposed, PAYLOAD_PATH).is_some(), "{proposed:#}");
    assert!(binding(proposed, REPORT).is_some(), "{proposed:#}");
}

/// The same typed answer settles only the name its question quotes: a second name sharing the
/// path stays owed whole, the author is told so, and nothing is proposed.
#[test]
fn a_typed_reply_never_settles_another_name() {
    let journey = run("two_outputs_typed_elsewhere");
    let refused = journey.step(1);
    assert_ne!(outcome_kind(refused), "proposal", "{refused:#}");
    assert!(binding(refused, PAYLOAD_PATH).is_some(), "{refused:#}");
    let owed = format!("the request names `{RELIRE}`");
    assert!(reached_the_author(&journey, &owed), "{refused:#}");
}

/// The same path written again as an ordinary line after the reply is the person's own fact: it
/// settles the second name by its folder, as their words always did, and the document is
/// proposed. Only the reply's own located line is left out of that settlement.
#[test]
fn a_reply_restated_as_an_ordinary_line_settles_historically() {
    let journey = run("two_outputs_typed_restated");
    let proposed = journey.step(2);
    assert_eq!(outcome_kind(proposed), "proposal", "{proposed:#}");
}

/// A reply the Session recorded for the payload's question keeps that question when the author
/// binds it under another key: it settles neither name, the author is told the second one is owed,
/// and nothing is proposed.
#[test]
fn a_reply_rebound_under_another_key_keeps_its_question() {
    let journey = run("two_outputs_typed_rebound");
    let refused = journey.step(1);
    assert_ne!(outcome_kind(refused), "proposal", "{refused:#}");
    let owed = format!("the request names `{RELIRE}`");
    assert!(reached_the_author(&journey, &owed), "{refused:#}");
}
