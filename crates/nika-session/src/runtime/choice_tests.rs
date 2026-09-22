// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The choice of an intelligence, asked in context: the first screen the
//! first time a turn needs one, a kept choice this machine cannot serve,
//! the re-choice in session, a path that does not answer (the recovery
//! card), and what is asked of the human in words — never as code.

use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use super::tests::{COPY, COPY_DEST, Failing, SMALL_TALK, Seat, UNSETTLED, ready, tree};
use super::*;
use crate::intelligence::{DataLocus, IntelligenceKind};
use crate::reasoner::{NoReasoner, ScriptedReasoner};

/// The first run opens without a choice: the facts and the deterministic
/// compiler answer at once; the first turn that needs an intelligence asks
/// the first screen in context and keeps the line, a typo keeps it
/// waiting, `cancel` drops it without a choice, and a choice resumes it
/// exactly as typed under the chosen intelligence.
#[test]
fn an_unchosen_session_asks_in_context_and_resumes_the_waiting_line() {
    let dir = tree();
    let home = tempfile::tempdir().expect("home");
    let census = IntelligenceCensus {
        seats: vec![crate::intelligence::SeatSeen {
            id: "codex".to_owned(),
            product_present: true,
            configured: true,
            answers_here: true,
        }],
        api_keys: vec![],
        locals: vec![],
    };
    let factory: ReasonerFactory = Box::new(|resolved| match &resolved.kind {
        IntelligenceKind::None => Box::new(NoReasoner),
        _ => Box::new(ScriptedReasoner::new(vec!["seated".to_owned()])),
    });
    let mut s = SessionRuntime::open_unchosen(dir.path(), census, Some(home.path()), factory);
    assert!(!s.intelligence_chosen());
    assert!(!s.pending_choice());
    assert!(s.status().contains("not chosen yet"), "{}", s.status());
    // The facts and work need no choice.
    assert!(
        matches!(s.turn("what workflows are here?"), TurnOutcome::Facts(ref t) if t.contains("alpha.nika"))
    );
    assert!(
        matches!(s.turn(COPY), TurnOutcome::Proposal { .. }),
        "deterministic work compiles before any choice"
    );
    assert!(!s.pending_choice(), "nothing asked so far");
    // The first line only an intelligence answers asks, in context.
    let TurnOutcome::Ask(screen) = s.turn(SMALL_TALK) else {
        panic!("asks in context");
    };
    assert!(
        screen.contains("Nika needs an intelligence for this part")
            && screen.contains("to answer this in words")
            && screen.contains("resumes after the choice")
            && screen.contains("4  No AI"),
        "{screen}"
    );
    assert!(
        !screen.contains("Choose which AI"),
        "not the cold first screen: {screen}"
    );
    assert!(s.pending_choice());
    // A typo keeps the screen and the line.
    assert!(
        matches!(s.choose("9"), TurnOutcome::Refusal(ref r) if r.text.contains("not a choice"))
    );
    assert!(s.pending_choice(), "the choice still waits after a typo");
    // A cancel drops the line, chooses nothing, and the session goes on.
    assert!(
        matches!(s.choose("cancel"), TurnOutcome::Facts(ref t) if t.contains("not sent anywhere"))
    );
    assert!(!s.pending_choice() && !s.intelligence_chosen());
    assert!(
        UserIntelligencePreference::load(home.path()).is_none(),
        "nothing kept on a cancel"
    );
    // Asked again, a choice resumes the very line under the intelligence.
    assert!(matches!(s.turn(SMALL_TALK), TurnOutcome::Ask(_)));
    let TurnOutcome::Resumed { notice, outcome } = s.choose("1") else {
        panic!("the choice resumes the waiting line");
    };
    assert!(
        notice.contains("codex") && notice.contains("kept"),
        "{notice}"
    );
    assert!(
        matches!(*outcome, TurnOutcome::Reply(ref t) if t.contains("seated")),
        "the waiting line ran under the chosen intelligence: {outcome:?}"
    );
    assert!(s.intelligence_chosen() && !s.pending_choice());
    assert!(UserIntelligencePreference::load(home.path()).is_some());
    // Chosen, the session never asks again on its own.
    assert!(matches!(s.turn(SMALL_TALK), TurnOutcome::Reply(_)));
}

/// Choosing « no AI » in context resumes the line too: the honest refusal
/// that names the facts, never a silent drop.
#[test]
fn choosing_no_intelligence_in_context_resumes_with_the_facts() {
    let dir = tree();
    let factory: ReasonerFactory = Box::new(|_| Box::new(NoReasoner));
    let mut s =
        SessionRuntime::open_unchosen(dir.path(), IntelligenceCensus::empty(), None, factory);
    assert!(matches!(s.turn(SMALL_TALK), TurnOutcome::Ask(_)));
    let TurnOutcome::Resumed { notice, outcome } = s.choose("4") else {
        panic!("resumes");
    };
    assert!(notice.contains("no conversational AI"), "{notice}");
    assert!(
        matches!(*outcome, TurnOutcome::Refusal(ref r) if r.class == RefusalClass::NoIntelligence && r.text.contains("facts still answer")),
        "{outcome:?}"
    );
    assert!(s.intelligence_chosen());
    assert!(
        matches!(s.turn(SMALL_TALK), TurnOutcome::Refusal(_)),
        "an explicit none is never re-asked"
    );
}

/// A rule the compiler can only ask as code never reaches the human as
/// code: the clause is asked in words, the words take its place in the
/// request and the compiler reads it again (Ready here); a clause that
/// stays code after the words, or a clause the request does not carry as
/// quoted, is an honest incomplete naming the way on — never « which jq
/// expression ».
#[test]
fn a_rule_the_compiler_asks_as_code_is_asked_in_words_and_restated() {
    let dir = tree();
    std::fs::write(
        dir.path().join("sales.csv"),
        "date,client,amount,status\n2026-09-01,Acme,120.50,paid\n",
    )
    .expect("csv");
    let mut s = SessionRuntime::open(
        dir.path(),
        ready(IntelligenceKind::None, DataLocus::None),
        Box::new(NoReasoner),
    );
    let intent = "Read ./sales.csv, compute the total and write it to ./total.txt";
    let TurnOutcome::Question { key, question } = s.turn(intent) else {
        panic!("the compiler asks for the rule of « the total »");
    };
    assert_eq!(key, "const.rule_expression", "the compiler's own key stays");
    assert!(
        question.contains("in words") && question.contains("« the total »"),
        "{question}"
    );
    assert!(
        !question.to_ascii_lowercase().contains("jq") && !question.contains("expression"),
        "no syntax is asked of a human: {question}"
    );
    // « why? » explains in words too, and the question still waits.
    let TurnOutcome::Aside(aside) = s.turn("why?") else {
        panic!("why? is an aside");
    };
    assert!(
        aside.contains("never asks you for code")
            && !aside.to_ascii_lowercase().contains("jq")
            && !aside.contains("const.rule_expression"),
        "{aside}"
    );
    assert!(s.authoring.is_some(), "the question still waits");
    // The words take the clause's place: the request reads again, Ready.
    let outcome = s.turn("the total of the amount column");
    assert!(
        matches!(outcome, TurnOutcome::Proposal { ref preview, .. } if preview.contains("total")),
        "{outcome:?}"
    );
    assert!(
        s.recent
            .iter()
            .any(|(_, a)| a.contains("restated « the total » in words")),
        "{:?}",
        s.recent
    );
    assert!(matches!(s.consent("no"), TurnOutcome::Facts(_)));
    // Words that leave the rule as code: the honest incomplete, no syntax.
    assert!(matches!(s.turn(intent), TurnOutcome::Question { .. }));
    let TurnOutcome::Facts(text) = s.turn("something clever") else {
        panic!("a second code question is an honest incomplete");
    };
    assert!(
        text.starts_with("I read this as work but cannot build")
            && text.contains("I never ask you for code")
            && !text.to_ascii_lowercase().contains("jq"),
        "{text}"
    );
    assert!(s.authoring.is_none(), "the round is dropped");
    assert!(
        !dir.path().join(COPY_DEST).exists() && !dir.path().join("total.txt").exists(),
        "nothing written"
    );
}

/// A kept choice this machine cannot serve (an app that is here but
/// cannot answer): the banner says so in plain words; the first line that
/// needs an intelligence — a conversation line or work the reader cannot
/// settle — is kept and the first screen asked with the problem and the
/// ways on this machine holds; the path is never called; a choice that
/// answers resumes the very line, and the kept choice moves to it.
#[test]
fn a_kept_choice_that_cannot_answer_asks_in_context_and_resumes_the_line() {
    let dir = tree();
    let home = tempfile::tempdir().expect("home");
    let census = IntelligenceCensus {
        seats: vec![crate::intelligence::SeatSeen {
            id: "gemini-cli".to_owned(),
            product_present: true,
            configured: true,
            answers_here: false,
        }],
        api_keys: vec!["mistral".to_owned()],
        locals: vec![],
    };
    let pref = UserIntelligencePreference::new(
        IntelligenceKind::Harness {
            seat: "gemini-cli".to_owned(),
        },
        None,
    );
    pref.save(home.path()).expect("the kept choice");
    let calls = Arc::new(AtomicUsize::new(0));
    let seat_calls = Arc::clone(&calls);
    let factory: ReasonerFactory = Box::new(move |resolved| match &resolved.kind {
        IntelligenceKind::Api { .. } => Box::new(ScriptedReasoner::new(vec!["seated".to_owned()])),
        IntelligenceKind::Harness { .. } => Box::new(Failing(Arc::clone(&seat_calls))),
        _ => Box::new(NoReasoner),
    });
    let mut s = SessionRuntime::open_with(dir.path(), census, &pref, Some(home.path()), factory);
    assert!(s.intelligence_chosen() && !s.intelligence.ready);
    assert!(
        s.banner()
            .contains("⚠ `gemini-cli` is installed and signed in, but Nika cannot get an answer through it yet"),
        "{}",
        s.banner()
    );
    // The facts and deterministic work never needed it.
    assert!(matches!(s.turn(COPY), TurnOutcome::Proposal { .. }));
    // A conversation line: the problem, the ways on, the line kept.
    let TurnOutcome::Ask(screen) = s.turn(SMALL_TALK) else {
        panic!("asks in context, never calls the seat");
    };
    assert!(
        screen.contains("Nika needs an intelligence for this part")
            && screen.contains("⚠ `gemini-cli` is installed and signed in")
            && screen.contains("2 (an API key is here for mistral)")
            && screen.contains("seen but not usable here yet: gemini-cli")
            && screen.contains("resumes after the choice"),
        "{screen}"
    );
    assert_eq!(calls.load(Ordering::SeqCst), 0, "the seat is never called");
    assert!(s.pending_choice());
    // Choosing the same app again is refused with the ways on; the line waits.
    assert!(
        matches!(s.choose("1 gemini-cli"), TurnOutcome::Refusal(ref r) if r.text.contains("cannot get an answer through it") && r.text.contains("previous choice stands"))
    );
    assert!(s.pending_choice());
    // The API answers, the line resumes as typed, the kept choice moves.
    let TurnOutcome::Resumed { notice, outcome } = s.choose("2") else {
        panic!("the choice resumes the waiting line");
    };
    assert!(
        notice.contains("mistral") && notice.contains("kept"),
        "{notice}"
    );
    assert!(
        matches!(*outcome, TurnOutcome::Reply(ref t) if t.contains("seated")),
        "{outcome:?}"
    );
    assert!(s.intelligence.ready && !s.pending_choice());
    let back = UserIntelligencePreference::load(home.path()).expect("kept");
    assert_eq!(
        back.kind,
        IntelligenceKind::Api {
            provider: "mistral".to_owned()
        }
    );
    // Work the reader cannot settle asks the same way under such a choice.
    let mut again = SessionRuntime::open_with(
        dir.path(),
        IntelligenceCensus {
            seats: vec![crate::intelligence::SeatSeen {
                id: "gemini-cli".to_owned(),
                product_present: true,
                configured: false,
                answers_here: false,
            }],
            api_keys: vec![],
            locals: vec![],
        },
        &pref,
        None,
        Box::new(|_| Box::new(NoReasoner)),
    );
    let TurnOutcome::Ask(screen) = again.turn(UNSETTLED) else {
        panic!("unsettled work asks in context");
    };
    assert!(
        screen.contains("to finish reading this request")
            && screen.contains(
                "⚠ `gemini-cli` is installed, but Nika cannot get an answer through it yet"
            )
            && screen.contains("`export <PROVIDER>_API_KEY=…`"),
        "{screen}"
    );
}

/// A path that does not answer leaves a recovery card — what happened,
/// what is kept, what did not happen, the ways on — and « what happened? »
/// repeats it from memory: exactly one call was made.
#[test]
fn a_failed_intelligence_leaves_a_recovery_card_repeated_without_a_call() {
    let dir = tree();
    let calls = Arc::new(AtomicUsize::new(0));
    let mut s = SessionRuntime::open(
        dir.path(),
        ready(
            IntelligenceKind::Api {
                provider: "mistral".to_owned(),
            },
            DataLocus::Metered {
                provider: "mistral".to_owned(),
            },
        ),
        Box::new(Failing(Arc::clone(&calls))),
    );
    let TurnOutcome::Refusal(card) = s.turn(SMALL_TALK) else {
        panic!("the failure is a refusal");
    };
    assert_eq!(card.class, RefusalClass::IntelligenceRefused);
    assert!(
        card.text.starts_with(
            "I couldn't use mistral API (the conversational intelligence) for this part — "
        ) && card.text.matches("I couldn't use").count() == 1,
        "the headline once, never nested: {}",
        card.text
    );
    assert!(
        card.text.contains("I couldn't use mistral API")
            && card.text.contains("HTTP 429")
            && card.text.contains("I still have")
            && card.text.contains("your request: «")
            && card
                .text
                .contains("Nothing was written and nothing was sent elsewhere")
            && card.text.contains("/intelligence"),
        "{}",
        card.text
    );
    assert_eq!(calls.load(Ordering::SeqCst), 1);
    let TurnOutcome::Facts(again) = s.turn("what happened?") else {
        panic!("the card repeats");
    };
    assert_eq!(again, card.text);
    assert!(matches!(s.turn("de quoi ?"), TurnOutcome::Facts(ref t) if *t == card.text));
    assert_eq!(
        calls.load(Ordering::SeqCst),
        1,
        "repeating the card never calls the path again"
    );
    assert!(
        matches!(s.turn("what workflows are here?"), TurnOutcome::Facts(ref t) if t.contains("alpha.nika")),
        "the facts still answer after a failure"
    );
}

/// Work the deterministic reader cannot settle asks the first screen in
/// context with the authoring reason; the choice resumes the request.
#[test]
fn unsettled_work_asks_in_context_with_the_authoring_reason() {
    let dir = tree();
    std::fs::write(dir.path().join("a.md"), "alpha").expect("a");
    let factory: ReasonerFactory = Box::new(|_| Box::new(NoReasoner));
    let mut s =
        SessionRuntime::open_unchosen(dir.path(), IntelligenceCensus::empty(), None, factory);
    let TurnOutcome::Ask(screen) = s.turn(UNSETTLED) else {
        panic!("asks in context");
    };
    assert!(
        screen.contains("to finish reading this request"),
        "the authoring reason: {screen}"
    );
    let TurnOutcome::Resumed { outcome, .. } = s.choose("4") else {
        panic!("resumes");
    };
    assert!(
        matches!(*outcome, TurnOutcome::Facts(_)),
        "under no seat the request is an honest incomplete: {outcome:?}"
    );
}

/// `/intelligence` asks the first screen again in-session; the next
/// line is the answer, kept under the home, the reasoner rebuilt and the
/// authoring seat re-derived; an unserved pick is refused and the
/// previous choice stands.
#[test]
fn the_intelligence_can_be_rechosen_in_session() {
    let dir = tree();
    let home = tempfile::tempdir().expect("home");
    let census = IntelligenceCensus {
        seats: vec![crate::intelligence::SeatSeen {
            id: "codex".to_owned(),
            product_present: true,
            configured: true,
            answers_here: true,
        }],
        api_keys: vec![],
        locals: vec![],
    };
    let pref = UserIntelligencePreference::new(IntelligenceKind::None, None);
    let factory: ReasonerFactory = Box::new(|resolved| match &resolved.kind {
        IntelligenceKind::None => Box::new(NoReasoner),
        _ => Box::new(ScriptedReasoner::new(vec!["seated".to_owned()])),
    });
    let mut s = SessionRuntime::open_with(dir.path(), census, &pref, Some(home.path()), factory);
    assert!(matches!(s.turn(SMALL_TALK), TurnOutcome::Refusal(_)));
    let TurnOutcome::Ask(screen) = s.turn("/intelligence") else {
        panic!("asks");
    };
    assert!(screen.contains("Choose which AI"), "{screen}");
    assert!(
        matches!(s.choose("2"), TurnOutcome::Refusal(ref r) if r.text.contains("previous choice stands"))
    );
    assert!(
        matches!(s.turn(SMALL_TALK), TurnOutcome::Refusal(_)),
        "still none"
    );
    let TurnOutcome::Facts(chosen) = s.choose("1") else {
        panic!("the choice is kept");
    };
    assert!(
        chosen.contains("codex") && chosen.contains("kept"),
        "{chosen}"
    );
    assert!(
        chosen.contains("authoring · deterministic"),
        "a harness seat reasons in words; authoring stays deterministic: {chosen}"
    );
    assert!(matches!(s.turn(SMALL_TALK), TurnOutcome::Reply(ref t) if t.contains("seated")));
    let back = UserIntelligencePreference::load(home.path()).expect("kept under the home");
    assert_eq!(
        back.kind,
        IntelligenceKind::Harness {
            seat: "codex".to_owned()
        }
    );
}

/// An explicit choice this machine cannot serve refuses every
/// conversational turn with its fix — the facts still answer, and work
/// still compiles (the compiler needs no seat).
#[test]
fn an_unserved_choice_refuses_with_its_fix() {
    let dir = tree();
    let unserved = ResolvedSessionIntelligence {
        kind: IntelligenceKind::Harness {
            seat: "claude-code".to_owned(),
        },
        model: None,
        locus: DataLocus::Remote {
            product: "claude-code".to_owned(),
        },
        ready: false,
        why: Some("`claude-code` is not installed on this machine — install it".to_owned()),
    };
    let mut s = SessionRuntime::open(dir.path(), unserved, Box::new(Seat("claude-code")));
    assert!(
        s.banner().contains("⚠ `claude-code` is not installed"),
        "an unserved choice is the one warning the banner carries: {}",
        s.banner()
    );
    assert!(
        s.status().contains("intelligence: claude-code · uses")
            && !s.status().contains("claude-code · claude-code"),
        "the seat is named once: {}",
        s.status()
    );
    assert!(
        s.status().contains("authoring · deterministic"),
        "{}",
        s.status()
    );
    assert!(
        matches!(s.turn(SMALL_TALK), TurnOutcome::Refusal(ref r) if r.text.contains("not installed"))
    );
    assert!(matches!(
        s.turn("what workflows are here?"),
        TurnOutcome::Facts(_)
    ));
    assert!(
        matches!(s.turn(COPY), TurnOutcome::Proposal { .. }),
        "work compiles without the seat"
    );
}
