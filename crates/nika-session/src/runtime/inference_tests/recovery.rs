// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>
//! Recovery owns the attempted work, while chat and asides own no automation.
//! Provider effects are the existing cfg(test) loopback seam, never live keys.

use super::*;
use crate::reasoner::ScriptedReasoner;
use crate::runtime::tests::{COPY, COPY_DEST, Failing, SMALL_TALK, ready};
use std::sync::{Arc, atomic::AtomicUsize};

const CURRENT: &str = "Prépare la copie de entree.txt dans sortie.txt, budget 0,50 dollar.";
const DRAFT: &str = "Read ./notes/brief.md, draft a 3-bullet summary of it and write the summary to ./out/summary.md";

fn notes(root: &Path) {
    std::fs::create_dir_all(root.join("notes")).expect("notes");
    std::fs::write(root.join("notes/brief.md"), "original brief\n").expect("brief");
}

fn words(root: &Path, replies: &[&str]) -> SessionRuntime {
    SessionRuntime::open(
        root,
        ready(
            IntelligenceKind::Local {
                provider: "ollama".into(),
            },
            DataLocus::Local,
        ),
        Box::new(ScriptedReasoner::new(
            replies.iter().map(|s| (*s).into()).collect(),
        )),
    )
}

fn admission_card(s: &mut SessionRuntime) -> String {
    let TurnOutcome::Refusal(card) = s.turn(CURRENT) else {
        panic!("the full-context reservation must still refuse this allowance");
    };
    assert_eq!(card.class, RefusalClass::IntelligenceRefused);
    assert!(
        card.text.contains("full-context reservation"),
        "{}",
        card.text
    );
    assert!(
        card.text.contains(&format!("your request: « {CURRENT} »")),
        "{}",
        card.text
    );
    assert_eq!(s.intent.goal.as_deref(), Some(CURRENT));
    assert!(s.pending_proposal().is_none() && s.pending_question().is_none());
    let money = s.monetary_decision().expect("admission observed");
    assert_eq!(money.original_intent, CURRENT);
    assert_eq!(money.effective_usd, Some(0.5));
    assert_eq!(money.observed_cost_usd, None);
    let receipt = s.inference_receipt().expect("receipt").expect("account");
    assert!(receipt.attempts.is_empty(), "refusal is before dispatch");
    assert_eq!(receipt.estimated.nano_usd, 0);
    assert_eq!(receipt.held_unknown.nano_usd, 0);
    assert_eq!(receipt.billed, None);
    assert!(
        receipt
            .refusal
            .as_deref()
            .is_some_and(|r| r.contains("full-context reservation"))
    );
    assert!(matches!(s.turn("what happened?"), TurnOutcome::Facts(ref t) if t == &card.text));
    assert_eq!(s.inference_receipt().unwrap().unwrap(), receipt);
    assert!(matches!(s.consent("yes"), TurnOutcome::Refusal(_)));
    card.text
}

#[test]
fn s49_fresh_work_is_kept_before_provider_admission_fails() {
    let peer = Peer::start(vec![(200, response(&native()))]);
    let _transport = test_transport::install(&peer.url);
    let dir = tempfile::tempdir().unwrap();
    let mut s = open(dir.path());
    admission_card(&mut s);
    assert!(peer.bodies().is_empty());
    assert_eq!(
        std::fs::read(dir.path().join("entree.txt")).unwrap(),
        b"A\n"
    );
    assert!(!dir.path().join("sortie.txt").exists());
    assert!(!dir.path().join(COPY_DEST).exists());
}

#[test]
fn s49_failed_new_work_replaces_saved_goal_without_reusing_its_consent() {
    let peer = Peer::start(vec![(200, response(&native()))]);
    let _transport = test_transport::install(&peer.url);
    let dir = tempfile::tempdir().unwrap();
    notes(dir.path());
    let mut s = open(dir.path());
    let TurnOutcome::Proposal { id, .. } = s.turn(COPY) else {
        panic!("copy proposal")
    };
    assert!(matches!(s.consent_to(&id, "yes"), TurnOutcome::Facts(_)));
    let saved = std::fs::read(dir.path().join(COPY_DEST)).unwrap();
    let consents = crate::consent::ConsentRecord::read_all(dir.path())
        .unwrap()
        .len();
    let card = admission_card(&mut s);
    assert!(!card.contains(&format!("your request: « {COPY} »")));
    assert!(matches!(s.consent_to(&id, "yes"), TurnOutcome::Refusal(_)));
    assert_eq!(std::fs::read(dir.path().join(COPY_DEST)).unwrap(), saved);
    assert_eq!(
        crate::consent::ConsentRecord::read_all(dir.path())
            .unwrap()
            .len(),
        consents
    );
    assert!(!dir.path().join("sortie.txt").exists());
    assert!(peer.bodies().is_empty());
}

#[test]
fn s49_greeting_then_refusals_retain_current_copy_and_monetary_guards() {
    let peer = Peer::start(vec![(200, response("unexpected"))]);
    let _transport = test_transport::install(&peer.url);
    let dir = tempfile::tempdir().unwrap();
    let mut s = open(dir.path());
    // The loopback effect asserts bounded request policy. The first greeting
    // has no inference allowance, so script only that conversation and restore
    // the real provider before testing its monetary admission. The live TUI
    // covers the unmetered greeting separately; this fixture makes no such claim.
    let provider = std::mem::replace(
        &mut s.reasoner,
        Box::new(ScriptedReasoner::new(vec!["Bonjour.".into()])),
    );
    s.with_classifier(Box::new(crate::turn::ReasonerClassifier::new(Box::new(
        ScriptedReasoner::new(vec!["DISCUSS".into()]),
    ))));
    // The scripted greeting is a local fixture, not an unpriced API adapter.
    let selected = std::mem::replace(
        &mut s.intelligence,
        ready(
            IntelligenceKind::Local {
                provider: "scripted".into(),
            },
            DataLocus::Local,
        ),
    );
    let greeting = "Bonjour, réponds simplement bonjour.";
    assert!(matches!(s.turn(greeting), TurnOutcome::Reply(_)));
    assert!(s.intent.goal.is_none(), "chat is not an automation goal");
    s.reasoner = provider;
    s.intelligence = selected;
    s.classifier = None;
    let calls = peer.bodies().len();
    for input in [
        "Copie entree.txt dans sortie.txt avec un plafond de $NaN.",
        "Copie entree.txt dans sortie.txt avec un plafond de $-1.",
    ] {
        assert!(
            matches!(s.turn(input), TurnOutcome::Refusal(ref r) if r.class == RefusalClass::NotAllowed)
        );
        assert_eq!(s.monetary_decision().unwrap().effective_usd, None);
        assert!(s.money_blocks_cognition());
    }
    assert!(matches!(
        s.turn("Prépare la copie de entree.txt dans sortie.txt, à 9 heures."),
        TurnOutcome::Refusal(ref r) if r.class == RefusalClass::NotAllowed
    ));
    assert!(s.money_blocks_cognition());
    let card = admission_card(&mut s);
    assert!(!card.contains(greeting));
    assert_eq!(
        peer.bodies().len(),
        calls,
        "no call after the rejected ceilings"
    );
    assert!(!dir.path().join("sortie.txt").exists());
}

#[test]
fn s49_greeting_and_failed_chat_do_not_replace_an_automation_goal() {
    let dir = tempfile::tempdir().unwrap();
    notes(dir.path());
    let mut s = words(dir.path(), &["Bonjour."]);
    assert!(matches!(s.turn("Bonjour"), TurnOutcome::Reply(_)));
    assert!(s.intent.goal.is_none());
    assert!(matches!(s.turn(COPY), TurnOutcome::Proposal { .. }));
    assert!(matches!(s.consent("no"), TurnOutcome::Facts(_)));
    let calls = Arc::new(AtomicUsize::new(0));
    s.reasoner = Box::new(Failing(Arc::clone(&calls)));
    let TurnOutcome::Refusal(card) = s.turn(SMALL_TALK) else {
        panic!("failed chat")
    };
    assert!(
        card.text
            .contains(&format!("your request: « {SMALL_TALK} »"))
    );
    assert!(!card.text.contains(&format!("your request: « {COPY} »")));
    assert_eq!(s.intent.goal.as_deref(), Some(COPY));
    assert!(matches!(s.turn("what happened?"), TurnOutcome::Facts(ref t) if t == &card.text));
    assert_eq!(calls.load(std::sync::atomic::Ordering::SeqCst), 1);
}

#[test]
fn s49_pending_question_asides_keep_the_round_and_its_goal() {
    let dir = tempfile::tempdir().unwrap();
    notes(dir.path());
    let mut s = words(dir.path(), &["Bonjour."]);
    assert!(matches!(s.turn("Bonjour"), TurnOutcome::Reply(_)));
    assert!(matches!(s.turn(DRAFT), TurnOutcome::Question { .. }));
    for aside in ["why?", "what is this model for?", "/why"] {
        assert!(matches!(s.turn(aside), TurnOutcome::Aside(_)));
        assert_eq!(s.intent.goal.as_deref(), Some(DRAFT));
        assert_eq!(s.authoring.as_ref().unwrap().intent, DRAFT);
        assert!(s.authoring.as_ref().unwrap().answers.is_empty());
        assert_eq!(s.pending_question().unwrap().key, "model");
        assert!(s.pending_proposal().is_none());
    }
    assert!(matches!(s.turn("mock/echo"), TurnOutcome::Proposal { .. }));
    assert!(
        !dir.path().join(COPY_DEST).exists(),
        "answering is not consent"
    );
}

#[test]
fn s49_proposal_aside_keeps_exact_identity_and_requires_original_consent() {
    let dir = tempfile::tempdir().unwrap();
    notes(dir.path());
    let mut s = words(dir.path(), &[]);
    let TurnOutcome::Proposal { id, .. } = s.turn(COPY) else {
        panic!("copy proposal")
    };
    let bytes = s.pending.as_ref().unwrap().changes[0]
        .content()
        .as_bytes()
        .to_vec();
    for aside in ["why?", "what workflows are here?"] {
        let _ = s.consent(aside);
        assert_eq!(s.pending_proposal(), Some(id.clone()));
        assert_eq!(s.intent.goal.as_deref(), Some(COPY));
        assert!(!dir.path().join(COPY_DEST).exists());
    }
    assert!(matches!(s.consent_to(&id, "yes"), TurnOutcome::Facts(_)));
    assert_eq!(std::fs::read(dir.path().join(COPY_DEST)).unwrap(), bytes);
    assert!(!dir.path().join("out/copy.md").exists(), "Save is not Run");
}
