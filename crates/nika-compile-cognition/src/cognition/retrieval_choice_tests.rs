// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! A seat's `search` cannot apply the identifier a clause names in one structured file (no
//! stated term binds the search, and its grep matches substrings): the compiler refuses that
//! settlement under its own name and keeps the seat's actual answer on record. A `lookup`
//! keeps its exact selection, and a seat's own NONE stays the seat's.

use crate::decide::{ChoiceAnswer, ChoiceFuture, ChoiceQuestion, DecisionSeat};
use crate::{Cognition, NoProvider, compile_with_cognition};
use nika_compile::{CompileOutcome, CompileRequest, CompileStatus};
use serde_json::Value;
use std::sync::atomic::{AtomicUsize, Ordering};

const INTENT: &str = "Trova la voce B-8 in ./voci.json e scrivila in ./out/voce.json.";

struct Seat {
    choice: &'static str,
    asked: AtomicUsize,
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

async fn under(choice: &'static str) -> (CompileOutcome, Value, usize) {
    let seat = Seat {
        choice,
        asked: AtomicUsize::new(0),
    };
    let out = compile_with_cognition::<NoProvider>(
        &CompileRequest::create(INTENT),
        Cognition {
            provider: None,
            seat: Some(&seat),
        },
    )
    .await
    .expect("compile");
    let decision = out.provenance.decision.clone().expect("decision");
    (out, decision, seat.asked.load(Ordering::SeqCst))
}

fn refusals(out: &CompileOutcome) -> Vec<&str> {
    out.diagnostics
        .iter()
        .filter(|d| d.target == "retrieval_choice")
        .map(|d| d.message.as_str())
        .collect()
}

fn routed(decision: &Value, step: &str) -> bool {
    decision["route"]
        .as_array()
        .is_some_and(|route| route.iter().any(|r| r == step))
}

#[tokio::test]
async fn a_searched_identifier_is_refused_by_the_compiler_and_the_seat_answer_kept() {
    let (out, decision, asked) = under("search").await;
    assert_eq!(asked, 1);
    assert_eq!(decision["questions"][0]["choice"], "search", "{decision:#}");
    assert!(routed(&decision, "warm: refused"), "{decision:#}");
    let refused = refusals(&out);
    assert!(
        refused.len() == 1 && refused[0].contains("B-8") && refused[0].contains("./voci.json"),
        "{out:#?}"
    );
    assert_ne!(out.status, CompileStatus::Ready);
    assert!(out.candidate.is_none());
    assert!(
        out.diagnostics.iter().any(|d| d
            .message
            .contains("Unresolved clause: Trova la voce B-8 in ./voci.json")),
        "{out:#?}"
    );
}

#[tokio::test]
async fn a_looked_up_identifier_keeps_its_settlement() {
    let (out, decision, asked) = under("lookup").await;
    assert_eq!(asked, 1);
    assert!(refusals(&out).is_empty(), "{out:#?}");
    assert!(routed(&decision, "warm"), "{decision:#}");
    let plan = out.provenance.plan.clone().expect("plan");
    assert!(
        plan["operations"]
            .as_array()
            .is_some_and(|ops| ops.iter().any(|op| op["op"] == "lookup")),
        "{plan:#}"
    );
}

#[tokio::test]
async fn the_seats_none_stays_the_seats() {
    let (out, decision, asked) = under(crate::decide::NONE_OPTION).await;
    assert_eq!(asked, 1);
    assert!(refusals(&out).is_empty(), "{out:#?}");
    assert!(routed(&decision, "warm: none"), "{decision:#}");
    assert!(!routed(&decision, "warm: refused"), "{decision:#}");
}
