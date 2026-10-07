// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>
//! Admitted sentence budgets cannot turn a non-work question into a compiler request.
//! Loopback mechanics only: the original words, account and guards remain the owners'.
use super::*;
use crate::authoring::{Reading, compile_in};
use crate::runtime::{authoring::DETERMINISTIC, route};

#[test]
fn question_budget_sentence_and_attached_forms_use_only_conversation() {
    for input in [
        "What can you tell me about stars? Budget: 2 USD.",
        "What can you tell me about stars, budget 2 USD?",
        "How do tides work?\nBudget: 2 USD.",
        "Budget: 2 USD. Why should we not email anyone?",
        "Que peux-tu dire des étoiles ? Budget: 2 USD; Cap: 2 USD.",
    ] {
        let peer = Peer::start(vec![(200, response("A conversation answer."))]);
        let _transport = test_transport::install(&peer.url);
        let dir = tempfile::tempdir().unwrap();
        let mut s = open(dir.path());
        let out = s.turn(input);
        assert!(matches!(out, TurnOutcome::Reply(_)), "{input}: {out:?}");
        assert!(
            s.intent.goal.is_none(),
            "a question owns no automation goal"
        );
        assert!(s.pending_proposal().is_none() && s.pending_question().is_none());
        assert!(s.routes().is_empty(), "no compiler or label call");
        let bodies = peer.bodies();
        assert_eq!(bodies.len(), 1, "{input}");
        assert!(
            bodies[0]["messages"].as_array().unwrap().iter().any(|m| {
                m["content"]
                    .as_str()
                    .is_some_and(|text| text.contains(input))
            }),
            "the conversation receives the exact original words"
        );
        let account = s.inference_receipt().unwrap().unwrap();
        assert_eq!(account.limit.nano_usd, 2_000_000_000);
        assert_eq!(account.attempts.len(), 1);
        assert!(account.estimated.nano_usd > 0);
        assert_eq!(account.billed, None);
        assert_eq!(s.monetary_decision().unwrap().original_intent, input);
        assert!(!dir.path().join("sortie.txt").exists());
    }
}

#[test]
fn question_budget_does_not_reclassify_recognized_work_as_conversation() {
    let peer = Peer::start(vec![]);
    let _transport = test_transport::install(&peer.url);
    for input in [
        "Read ./entree.txt and write it to ./sortie.txt. Budget: 0 USD.",
        "Read ./entree.txt and write it to ./sortie.txt? Budget: 0 USD.",
    ] {
        let dir = tempfile::tempdir().unwrap();
        let mut s = open(dir.path());
        let out = s.turn(input);
        assert!(
            matches!(out, TurnOutcome::Proposal { .. }),
            "{input}: {out:?}"
        );
        assert!(s.intent.goal.is_some());
        assert_eq!(s.monetary_decision().unwrap().original_intent, input);
        assert_eq!(s.monetary_decision().unwrap().effective_usd, Some(0.0));
        assert!(
            !dir.path().join("sortie.txt").exists(),
            "Save is still separate"
        );
    }
    assert!(
        peer.bodies().is_empty(),
        "deterministic work stays deterministic"
    );
}

#[test]
fn question_budget_bad_or_zero_money_still_refuses_before_cognition() {
    let peer = Peer::start(vec![]);
    let _transport = test_transport::install(&peer.url);
    for suffix in [
        "Budget: 0 USD.",
        "Budget: -1 USD.",
        "Budget: NaN.",
        "Budget: $abc.",
        "Budget: 2 USD. Cap: 3 USD.",
    ] {
        let dir = tempfile::tempdir().unwrap();
        let mut s = open(dir.path());
        let input = format!("How do tides work? {suffix}");
        let out = s.turn(&input);
        assert!(matches!(out, TurnOutcome::Refusal(_)), "{input}: {out:?}");
        assert!(s.pending_proposal().is_none() && s.pending_question().is_none());
        assert!(s.routes().is_empty());
        assert!(s.money_blocks_cognition());
    }
    assert!(peer.bodies().is_empty());
}

#[test]
fn question_budget_preserves_later_business_words_and_negations() {
    for input in [
        "Why? Do not send email. Budget: 2 USD.",
        "Why? Read ./entree.txt and write it to ./sortie.txt. Budget: 2 USD.",
        "Why? Do not spend 2 USD.",
        "Why? The budget field is 2 USD.",
        "Why? Write \"Budget: 2 USD\" to ./sortie.txt.",
    ] {
        let parsed = money_parse::directives(input).unwrap();
        let spans: Vec<_> = parsed.found.into_iter().map(|d| d.span).collect();
        assert!(!route::question_outside_money(input, &spans), "{input}");
    }
    assert!(!route::question_outside_money("Why? Budget: 2 USD.", &[]));
}

#[test]
fn question_budget_false_or_invalid_spans_cannot_gain_the_question_fast_path() {
    let dir = tempfile::tempdir().unwrap();
    let s = open(dir.path());
    let input = "How do tides work? Budget: 2 USD.";
    let context = s.project_context();
    for span in [0..input.len(), 0..usize::MAX, 1..2] {
        let mut round = AuthoringRound::new(input);
        round.money.push(span);
        let out = compile_in(&DETERMINISTIC, &context, &round.request(), input).unwrap();
        let reading = route::as_written(Reading::of(out), &round, &context, input);
        assert!(matches!(reading, Reading::Refused(_)), "{reading:?}");
    }
}
