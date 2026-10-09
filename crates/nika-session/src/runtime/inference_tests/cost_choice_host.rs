// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>
//! The one-time cost choice as every host meets it: its evidence is read beside it without a
//! turn, an interruption declines it like a typed `cancel`, and only a fresh line may answer it.
//! Fixture mechanics only, never model or billing qualification.
use super::unknown_cost::{asked, open_unknown, unpriced_response};
use super::*;
use crate::work::Waiting;

#[test]
fn the_cost_choice_evidence_is_read_beside_it_and_nothing_is_recorded() {
    let peer = Peer::start(vec![(200, unpriced_response("Hello"))]);
    let _transport = test_transport::install(&peer.url);
    let dir = tempfile::tempdir().unwrap();
    let mut s = open_unknown(dir.path());
    asked(&s.turn("hello"));
    let shown = s.waiting();
    assert_eq!(shown, Waiting::CostChoice);
    assert!(shown.requires_fresh_input());
    let details = s
        .cost_choice_details()
        .expect("the pending review's details");
    let recent = s.recent.len();
    for line in ["details", "Details", "/details", "/why", "why?"] {
        let TurnOutcome::Aside(text) = s.submit(line, &shown) else {
            panic!("{line}: the evidence answers beside the choice");
        };
        assert!(
            text.starts_with(
                "Authoring cost decision details · the same review; reading them approves nothing\n"
            ),
            "{line}"
        );
        assert!(text.contains(&details), "{line}");
        assert!(text.ends_with("Continue once? yes / no"), "{line}");
    }
    assert!(
        s.waiting_cost_choice(),
        "reading the evidence answered nothing"
    );
    assert_eq!(s.recent.len(), recent, "reading the evidence is not a turn");
    assert!(peer.bodies().is_empty(), "nothing was sent");
}

#[test]
fn an_interruption_declines_the_cost_choice_like_a_typed_cancel() {
    let peer = Peer::start(vec![(200, unpriced_response("Hello"))]);
    let _transport = test_transport::install(&peer.url);
    let dir = tempfile::tempdir().unwrap();
    let mut s = open_unknown(dir.path());
    asked(&s.turn("hello"));
    let declined = s.decline_cost_choice().expect("a choice waited");
    assert!(
        !matches!(declined, TurnOutcome::Reply(_) | TurnOutcome::Refusal(_)),
        "an interruption is a decline, never a yes nor a failure: {declined:?}"
    );
    assert!(!s.waiting_cost_choice());
    assert!(s.decline_cost_choice().is_none(), "nothing waits any more");
    assert!(matches!(s.turn("yes"), TurnOutcome::Refusal(_)));
    assert!(peer.bodies().is_empty(), "nothing was sent");
}
