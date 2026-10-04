// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! A retrieval clause never loses its own words on the way to READY. A record the request
//! selects by an identifier in a structured file, or a clause it states past the source, is
//! either applied exactly or left as named, unresolved work: never a copy of the whole file,
//! a projection over every record, or a search keyed on a runtime input instead of the stated
//! literal. Each request is read whole, through the public doors only, and its two rounds
//! (the first, then its answer round replaying the recorded plan) are both judged.
#![allow(clippy::expect_used, clippy::panic)]

use nika_compile::{CompileOutcome, CompileRequest, CompileStatus, compile};
use nika_compile_cognition::{
    Cognition, NoProvider, compile_with_cognition,
    decide::{ChoiceAnswer, ChoiceFuture, ChoiceQuestion, DecisionSeat},
};
use std::sync::atomic::{AtomicUsize, Ordering};

/// A seat that answers every closed choice with the same option and counts its requests.
struct Seat {
    choice: &'static str,
    asked: AtomicUsize,
}

impl Seat {
    const fn new(choice: &'static str) -> Self {
        Self {
            choice,
            asked: AtomicUsize::new(0),
        }
    }
}

impl DecisionSeat for Seat {
    fn name(&self) -> &'static str {
        "double/seat"
    }
    fn choose<'a>(&'a self, _question: &'a ChoiceQuestion) -> ChoiceFuture<'a> {
        Box::pin(async move {
            self.asked.fetch_add(1, Ordering::SeqCst);
            Ok(ChoiceAnswer::new(self.choice, "double-1.0"))
        })
    }
}

/// The one structured source a request names (the answer the first round's corpus question takes).
fn source(intent: &str) -> &str {
    intent
        .split_whitespace()
        .map(|w| w.trim_end_matches([',', '.', ';']))
        .find(|w| {
            w.starts_with("./")
                && std::path::Path::new(w)
                    .extension()
                    .is_some_and(|ext| ext.eq_ignore_ascii_case("json"))
        })
        .expect("a JSON source")
}

/// The first round under `seat` (or the deterministic door alone), then, when it recorded a
/// plan and asked questions, its answer round: the recorded plan, each constant question
/// answered with the request's own source, no seat. Returns both outcomes.
async fn rounds(intent: &str, seat: Option<&Seat>) -> (CompileOutcome, Option<CompileOutcome>) {
    let first = match seat {
        Some(seat) => compile_with_cognition::<NoProvider>(
            &CompileRequest::create(intent),
            Cognition {
                provider: None,
                seat: Some(seat),
            },
        )
        .await
        .expect("first round"),
        None => compile(&CompileRequest::create(intent)).expect("first round"),
    };
    let (Some(plan), false) = (first.provenance.plan.clone(), first.questions.is_empty()) else {
        return (first, None);
    };
    let mut request = CompileRequest::create(intent).with_plan(plan);
    for question in first
        .questions
        .iter()
        .filter(|q| q.key.starts_with("const."))
    {
        request = request.answer(question.key.clone(), format!("\"{}\"", source(intent)));
    }
    let answered = compile(&request).expect("answer round");
    (first, Some(answered))
}

fn never_ready(intent: &str, outcomes: &(CompileOutcome, Option<CompileOutcome>)) {
    for out in std::iter::once(&outcomes.0).chain(outcomes.1.as_ref()) {
        assert_ne!(out.status, CompileStatus::Ready, "{intent}: {out:#?}");
        assert!(out.candidate.is_none(), "{intent}: {:?}", out.candidate);
    }
}

fn names(out: &CompileOutcome, words: &str) -> bool {
    out.diagnostics.iter().any(|d| d.message.contains(words))
}

/// An identifier named before a structured source selects a part of it: the deterministic
/// door leaves the clause unresolved, never a whole-file read.
#[tokio::test]
async fn a_record_named_before_its_source_is_never_read_whole() {
    for (intent, clause) in [
        (
            "Read the stock of item Z-31 from ./inventory.json and write it to ./out/stock.txt.",
            "Read the stock of item Z-31 from ./inventory.json",
        ),
        (
            "Lis la fiche K-77 de ./fiches.json, garde seulement son champ ville, et écris-le dans ./out/ville.txt.",
            "Lis la fiche K-77 de ./fiches.json",
        ),
    ] {
        let outcomes = rounds(intent, None).await;
        never_ready(intent, &outcomes);
        assert!(names(&outcomes.0, clause), "{intent}: {:#?}", outcomes.0);
    }
}

/// A clause continued past its source (« …, keep only its phone field ») is its own clause:
/// unresolved, so no decision seat is asked to settle the retrieval around it.
#[tokio::test]
async fn a_continuation_past_the_source_is_unresolved_before_any_seat() {
    let intent = "Find entry Q-12 in ./clients.json, keep only its phone field, and write it to ./out/phone.txt.";
    let seat = Seat::new("search");
    let outcomes = rounds(intent, Some(&seat)).await;
    never_ready(intent, &outcomes);
    assert_eq!(
        seat.asked.load(Ordering::SeqCst),
        0,
        "no gratuitous seat call"
    );
    assert!(
        names(&outcomes.0, "keep only its phone field"),
        "{:#?}",
        outcomes.0
    );
}

/// A seat's `search` cannot apply the identifier the clause names: the compiler refuses that
/// settlement and keeps the seat's own answer on record; nothing becomes READY in either round.
#[tokio::test]
async fn a_searched_identifier_is_never_ready_and_the_seat_answer_stays_recorded() {
    let intent = "Busca el pedido W-5 en ./pedidos.json y escríbelo en ./out/pedido.json.";
    let seat = Seat::new("search");
    let outcomes = rounds(intent, Some(&seat)).await;
    never_ready(intent, &outcomes);
    assert_eq!(seat.asked.load(Ordering::SeqCst), 1);
    let decision = outcomes.0.provenance.decision.clone().expect("decision");
    assert_eq!(decision["questions"][0]["choice"], "search", "{decision:#}");
    assert!(names(&outcomes.0, "W-5"), "{:#?}", outcomes.0);
}

/// The same request settled as a lookup keeps its exact selection: the record's field is
/// asked, then the answer round is READY with the identifier as a stated literal.
#[tokio::test]
async fn a_looked_up_identifier_keeps_its_exact_selection() {
    let intent = "Busca el pedido W-5 en ./pedidos.json y escríbelo en ./out/pedido.json.";
    let seat = Seat::new("lookup");
    let first = compile_with_cognition::<NoProvider>(
        &CompileRequest::create(intent),
        Cognition {
            provider: None,
            seat: Some(&seat),
        },
    )
    .await
    .expect("first round");
    assert_eq!(seat.asked.load(Ordering::SeqCst), 1);
    assert!(!names(&first, "cannot select"), "{first:#?}");
    let keys: Vec<&str> = first.questions.iter().map(|q| q.key.as_str()).collect();
    let field = keys
        .iter()
        .find(|k| k.ends_with("_id_field"))
        .unwrap_or_else(|| panic!("the record field is asked: {first:#?}"));
    let plan = first.provenance.plan.clone().expect("recorded plan");
    let mut request = CompileRequest::create(intent)
        .with_plan(plan)
        .answer(*field, "\"id\"");
    for key in keys.iter().filter(|k| *k != field) {
        request = request.answer(*key, "\"./pedidos.json\"");
    }
    let answered = compile(&request).expect("answer round");
    assert_eq!(answered.status, CompileStatus::Ready, "{answered:#?}");
    let candidate = answered.candidate.expect("candidate");
    assert!(candidate.contains("W-5"), "{candidate}");
    assert!(!candidate.contains("inputs.item"), "{candidate}");
}

/// What these laws leave alone: a whole-file read with no record named, a text source whose
/// name carries an identifier, and a lookup head that selects one record exactly.
#[tokio::test]
async fn unrelated_reads_and_lookups_keep_their_route() {
    for intent in [
        "Read ./prices.json and write it to ./out/copy.json.",
        "Read the Q3 notes in ./q3.md and write them to ./out/q3.md.",
    ] {
        let out = compile(&CompileRequest::create(intent)).expect("compile");
        assert_eq!(out.status, CompileStatus::Ready, "{intent}: {out:#?}");
    }
    let intent = "Look up ticket T-12 in ./tickets.json and write it to ./out/ticket.json.";
    let first = compile(&CompileRequest::create(intent)).expect("compile");
    assert!(
        first.questions.iter().any(|q| q.key.ends_with("_id_field")),
        "{first:#?}"
    );
}
