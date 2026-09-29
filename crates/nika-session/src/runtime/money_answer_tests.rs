// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! An answer that replaces or restates the request, and the money its words state (C11 · the
//! admitted replacement budget): Session admits the amount at the answer, and the request it hands
//! the compiler carries the monetary spans of exactly its own bytes — freshly admitted for a
//! replacement (an explicit Create clarification becomes the replacement Create text), bound to
//! the constructed string for a restatement — never offsets carried from another text. The
//! account's ceiling stays apart from those spans. These component tests exercise the request
//! Session hands the compiler and its admitted reading. Restatement tests call the helper
//! directly; they do not establish that conversational routing selected that helper.

use std::ops::Range;
use std::path::Path;

use nika_onboard::compile::reading::CLARIFICATION_KEY;
use nika_onboard::compile::{CompileRequest, QuestionType, compile, intent_sha256, money};

use super::*;
use crate::authoring::AuthoringRound;
use crate::intelligence::{DataLocus, IntelligenceKind};
use crate::reasoner::NoReasoner;

/// The first line: free intent the compiler asks to replace, with the ceiling its gate admitted.
const ORIGINAL: &str = "Sort the payments. Budget: $2.";

/// A session with no intelligence chosen: the deterministic reader, no seat is ever asked.
fn literal(root: &Path) -> SessionRuntime {
    let none = ResolvedSessionIntelligence {
        kind: IntelligenceKind::None,
        model: None,
        locus: DataLocus::None,
        ready: false,
        why: None,
    };
    SessionRuntime::open(root, none, Box::new(NoReasoner))
}

/// The session waits on `intent.clarification` after `original`, whose money the gate admitted
/// as a fresh request's, the round keeping those spans (as a first turn keeps them).
fn waiting(root: &Path, original: &str) -> SessionRuntime {
    let mut s = literal(root);
    assert!(
        s.admit_money(original, false).is_ok(),
        "the first line is admitted"
    );
    let asked = compile(&CompileRequest::create("bounded-batch"))
        .expect("compiles")
        .questions;
    let mut clarification = asked[0].clone();
    clarification.key = CLARIFICATION_KEY.to_owned();
    clarification.answer_type = QuestionType::Text;
    let mut round = AuthoringRound::new(original);
    round.money.clone_from(&s.money.admitted);
    round.questions = vec![clarification];
    s.authoring = Some(round);
    s
}

/// Every monetary directive of `text`, as the shared finder reads its exact bytes.
fn directives(text: &str) -> Vec<Range<usize>> {
    money::directives(text)
        .expect("directives")
        .found
        .into_iter()
        .map(|d| d.span)
        .collect()
}

/// The round waiting again (a skeleton replacement asks its own questions).
fn again(s: &SessionRuntime) -> &AuthoringRound {
    s.authoring.as_ref().expect("the round waits again")
}

/// The ceiling the account admitted, apart from any lexical span.
fn ceiling(s: &SessionRuntime) -> Option<f64> {
    s.money.draft.as_ref().and_then(|d| d.effective_usd)
}

/// What the compiler's own record says it read as admitted money: the identity of the request
/// text, and each directive's words as the span it names cuts them out of `text`.
fn read_as_money(s: &SessionRuntime, text: &str) -> (String, Vec<String>) {
    let decision = s
        .last_outcome
        .as_ref()
        .and_then(|out| out.provenance.decision.clone())
        .unwrap_or_default();
    let cut = |d: &serde_json::Value| {
        let at = |i: usize| usize::try_from(d["span"][i].as_u64().unwrap_or(0)).unwrap_or(0);
        let words = text.get(at(0)..at(1)).unwrap_or("(not a span of the text)");
        assert_eq!(d["text"], words, "the span names these very bytes");
        words.to_owned()
    };
    let words = decision["money"]["directives"]
        .as_array()
        .map(|all| all.iter().map(cut).collect())
        .unwrap_or_default();
    (
        decision["intent_sha256"]
            .as_str()
            .unwrap_or_default()
            .to_owned(),
        words,
    )
}

/// The replacement is the request: its own bytes and its own freshly admitted span, the chosen
/// seat and the account kept.
#[test]
fn a_replacement_stating_its_own_budget_is_the_request_with_its_own_span() {
    let dir = tempfile::tempdir().expect("root");
    let mut s = waiting(dir.path(), ORIGINAL);
    let replacement = "Tidy the invoices. Budget: $3.";
    let _ = s.turn(replacement);
    assert_eq!(
        read_as_money(&s, replacement),
        (intent_sha256(replacement), vec!["Budget: $3".to_owned()]),
        "the compiler read the replacement, its budget as money"
    );
    assert_eq!(
        ceiling(&s),
        Some(3.0),
        "the account admitted the new ceiling"
    );
    assert!(matches!(
        s.seat,
        crate::authoring::AuthoringSeat::Deterministic { .. }
    ));
}

/// Changed words without money: no old span rides them, and the admitted ceiling stays.
#[test]
fn a_replacement_without_money_carries_no_old_span_and_keeps_the_ceiling() {
    let dir = tempfile::tempdir().expect("root");
    let mut s = waiting(dir.path(), ORIGINAL);
    let _ = s.turn("bounded-batch");
    let round = again(&s);
    assert_eq!(
        round.intent, "bounded-batch",
        "the replacement is the Create text"
    );
    assert!(round.money.is_empty(), "{:?}", round.money);
    assert!(!round.answers.contains_key(CLARIFICATION_KEY));
    assert_eq!(ceiling(&s), Some(2.0));
}

/// The same bytes again keep their valid spans.
#[test]
fn an_identical_replacement_keeps_its_valid_spans() {
    let dir = tempfile::tempdir().expect("root");
    let mut s = waiting(dir.path(), ORIGINAL);
    let _ = s.turn(ORIGINAL);
    assert_eq!(
        read_as_money(&s, ORIGINAL),
        (intent_sha256(ORIGINAL), vec!["Budget: $2".to_owned()])
    );
    assert_eq!(ceiling(&s), Some(2.0));
}

/// Leading whitespace and multibyte words: the span is of the installed bytes, never shifted
/// offsets of the raw line.
#[test]
fn leading_whitespace_and_multibyte_words_bind_to_the_installed_bytes() {
    let dir = tempfile::tempdir().expect("root");
    let mut s = waiting(dir.path(), ORIGINAL);
    let installed = "Range les paiements « réglés » d’été, budget 3 USD";
    let _ = s.turn(&format!("   {installed}"));
    assert_eq!(
        read_as_money(&s, installed),
        (intent_sha256(installed), vec!["budget 3 USD".to_owned()])
    );
    assert_eq!(directives(installed).len(), 1);
    assert_eq!(ceiling(&s), Some(3.0));
}

/// A zero ceiling is money too, read by the deterministic reader alone.
#[test]
fn a_zero_ceiling_is_admitted_as_money_with_no_seat() {
    let dir = tempfile::tempdir().expect("root");
    let mut s = waiting(dir.path(), ORIGINAL);
    let replacement = "Tidy the invoices. Budget: $0.";
    let _ = s.turn(replacement);
    assert_eq!(
        read_as_money(&s, replacement),
        (intent_sha256(replacement), vec!["Budget: $0".to_owned()])
    );
    assert_eq!(ceiling(&s), Some(0.0));
    assert!(matches!(
        s.seat,
        crate::authoring::AuthoringSeat::Deterministic { .. }
    ));
}

/// Conflicting or malformed money in a replacement refuses the line under the money law: the
/// authority it would have used expires, and nothing is compiled.
#[test]
fn conflicting_or_malformed_money_in_a_replacement_refuses_and_nothing_is_compiled() {
    for line in [
        "Tidy the invoices. Budget: $3. budget 4 USD",
        "Tidy the invoices, budget=0.5oopsUSD",
    ] {
        let dir = tempfile::tempdir().expect("root");
        let mut s = waiting(dir.path(), ORIGINAL);
        let outcome = s.turn(line);
        assert!(
            matches!(outcome, TurnOutcome::Refusal(_)),
            "{line}: {outcome:?}"
        );
        assert!(s.authoring.is_none() && s.last_outcome.is_none(), "{line}");
    }
}

/// A restatement admits the directive its words add, as a span of the string it builds (the
/// line trimmed after the request), beside the request's own: never the raw line's offsets.
/// (Called on the waiting round as the route calls it: in a session with no intelligence no
/// door reads the line, so the route itself cannot choose to restate here.)
#[test]
fn a_restatement_admits_its_added_directive_on_the_string_it_builds() {
    for (original, amount) in [("Sort the payments.", "3"), (ORIGINAL, "2")] {
        let dir = tempfile::tempdir().expect("root");
        let mut s = waiting(dir.path(), original);
        let round = s.authoring.take().expect("a round waits");
        let line = format!("also the « réglés » ones, budget {amount} USD");
        let _ = s.restate_round(&round, &format!("   {line}"));
        let built = format!("{original}. {line}");
        let bound: Vec<String> = directives(&built)
            .into_iter()
            .map(|span| built[span].to_owned())
            .collect();
        assert_eq!(
            bound.last().map(String::as_str),
            Some(format!("budget {amount} USD").as_str())
        );
        assert_eq!(read_as_money(&s, &built), (intent_sha256(&built), bound));
        // The request alone reads « Budget: $2 »; built, its words read « Budget: $2. »: the offsets
        // carried from the request name no directive of the built string.
        for carried in &round.money {
            assert!(!directives(&built).contains(carried), "{carried:?}");
        }
    }
}

/// A restatement whose words state another ceiling than the request's refuses under the money
/// law, and nothing is compiled.
#[test]
fn a_restatement_stating_another_ceiling_refuses_and_nothing_is_compiled() {
    let dir = tempfile::tempdir().expect("root");
    let mut s = waiting(dir.path(), ORIGINAL);
    let round = s.authoring.take().expect("a round waits");
    let outcome = s.restate_round(&round, "also the late ones, budget 3 USD");
    assert!(matches!(outcome, TurnOutcome::Refusal(_)), "{outcome:?}");
    assert!(s.authoring.is_none() && s.last_outcome.is_none());
}

/// A business amount in a replacement is data, never the ceiling: the gate reads the
/// replacement's directives, not its whole line.
#[test]
fn a_business_amount_in_a_replacement_is_never_admitted_as_the_ceiling() {
    let dir = tempfile::tempdir().expect("root");
    let mut s = waiting(dir.path(), ORIGINAL);
    let _ = s.turn("bounded-batch for rows whose price is under $5");
    assert_eq!(ceiling(&s), Some(2.0), "the admitted ceiling stays");
    assert!(
        s.authoring
            .as_ref()
            .is_none_or(|round| round.money.is_empty()),
        "no span for data"
    );
}

/// Unchanged positives: a request and a replacement with no money stay free of spans and
/// keep the existing account ceiling; a value answer keeps its round; a revision keeps its change.
#[test]
fn money_free_answers_value_answers_and_revisions_keep_their_request() {
    let dir = tempfile::tempdir().expect("root");
    let mut s = waiting(dir.path(), "Sort the payments.");
    let before = ceiling(&s);
    let _ = s.turn("bounded-batch");
    let round = again(&s);
    assert_eq!(
        (round.intent.as_str(), round.money.is_empty()),
        ("bounded-batch", true)
    );
    assert_eq!(ceiling(&s), before, "no money named, the ceiling unchanged");

    let asked = compile(&CompileRequest::create("bounded-batch"))
        .expect("compiles")
        .questions;
    let mut value = AuthoringRound::new(ORIGINAL);
    value.money = directives(ORIGINAL);
    value.questions = vec![asked[0].clone()];
    value.answer_current("5");
    assert_eq!(
        (value.intent.as_str(), &value.money),
        (ORIGINAL, &directives(ORIGINAL))
    );

    let mut revision = AuthoringRound::new(ORIGINAL);
    revision.edit = Some(("nika: base\n".to_owned(), "add a step".to_owned(), None));
    let mut clarification = asked[0].clone();
    clarification.key = CLARIFICATION_KEY.to_owned();
    clarification.answer_type = QuestionType::Text;
    revision.questions = vec![clarification];
    revision.answer_current("something else");
    assert_eq!(
        revision.intent, ORIGINAL,
        "a revision never replaces its change"
    );
    assert!(revision.answers.contains_key(CLARIFICATION_KEY));
}
