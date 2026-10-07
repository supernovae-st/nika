// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>
//! Foundry knowledge qualified by the selected decision seat before the author reads it (R3 · A1):
//! a lexical recall never decides alone what the author is shown. A reference the seat finds
//! unrelated never reaches the author; one it cannot judge stays shown as a hypothesis; the
//! record says what was found, shown, discarded and unqualified. Hermetic doubles only.
#![allow(clippy::unwrap_used, clippy::expect_used)]
use nika_compile::{AuthoringKnowledge, CompileOutcome, CompileRequest, KnowledgeReference};
use nika_compile_cognition::{
    Cognition, compile_with_cognition,
    decide::{ChoiceAnswer, ChoiceFuture, ChoiceQuestion, DecisionError, DecisionSeat},
};
use serde_json::Value;
use std::sync::Mutex;

mod common;
use common::{INTENT, Rotating, plan, policy};

/// A seat that qualifies references by id (`applies` · `unrelated`, NONE for any other), and
/// answers nothing else: every other question is a failure the verifier records.
struct Qualifying {
    asked: Mutex<Vec<String>>,
}

impl DecisionSeat for Qualifying {
    fn name(&self) -> &'static str {
        "double/qualifying"
    }
    fn choose<'a>(&'a self, question: &'a ChoiceQuestion) -> ChoiceFuture<'a> {
        self.asked.lock().unwrap().push(question.id.clone());
        let id = question.state["reference"]["id"]
            .as_str()
            .map(str::to_owned);
        Box::pin(async move {
            match id.as_deref() {
                Some("block:refund-gate") => Ok(ChoiceAnswer::new("applies", "double-1")),
                Some("block:snapshot-diff") => Ok(ChoiceAnswer::new("unrelated", "double-1")),
                Some(_) => Ok(ChoiceAnswer::new("none", "double-1")),
                None => Err(DecisionError("this double judges no candidate".to_owned())),
            }
        })
    }
}

fn reference(kind: &str, id: &str, text: &str) -> KnowledgeReference {
    KnowledgeReference {
        kind: kind.to_owned(),
        id: id.to_owned(),
        text: text.to_owned(),
    }
}

fn pack() -> AuthoringKnowledge {
    AuthoringKnowledge {
        references: vec![
            reference(
                "block",
                "block:refund-gate",
                "A human approves a refund first.",
            ),
            reference(
                "block",
                "block:snapshot-diff",
                "Compare two snapshots of a table.",
            ),
        ],
        ..AuthoringKnowledge::default()
    }
}

/// The reference ids the first authoring call carried.
fn sent(out: &CompileOutcome) -> Vec<String> {
    let calls = &out.provenance.authoring.as_ref().unwrap().context;
    let references = calls[0]["references"].as_array().unwrap();
    references
        .iter()
        .map(|r| r["id"].as_str().unwrap().to_owned())
        .collect()
}

#[tokio::test]
async fn the_seat_qualifies_the_recall_and_the_unrelated_never_reaches_the_author() {
    let seat = Qualifying {
        asked: Mutex::new(Vec::new()),
    };
    let author = Rotating::new(vec![plan().to_string()]);
    let request = CompileRequest::create(INTENT)
        .with_authoring_policy(policy())
        .with_authoring_knowledge(pack());
    let cognition = Cognition {
        provider: Some(&author),
        seat: Some(&seat as &dyn DecisionSeat),
    };
    let out = compile_with_cognition(&request, cognition).await.unwrap();
    let carried = sent(&out);
    assert!(
        carried.iter().any(|id| id == "block:refund-gate"),
        "{carried:?}"
    );
    assert!(
        !carried.iter().any(|id| id == "block:snapshot-diff"),
        "{carried:?}"
    );
    let decision = out.provenance.decision.as_ref().unwrap();
    let record = &decision["knowledge_qualification"];
    assert_eq!(record["by"], "decision_seat", "{record:#}");
    assert_eq!(record["seat"], "double/qualifying");
    assert_eq!(record["discarded"], 1, "{record:#}");
    assert_eq!(record["applies"], 1, "{record:#}");
    // The embedded recall (skeletons, families) is folded into the pack and judged with it: what
    // the seat could not tell stays shown, unqualified, and nothing is shown twice.
    let found = record["found"].as_u64().unwrap();
    assert!(found >= 2, "{record:#}");
    assert_eq!(record["shown"].as_u64().unwrap(), found - 1);
    assert_eq!(record["unqualified"].as_u64().unwrap(), found - 2);
    let mut unique = carried.clone();
    unique.sort();
    unique.dedup();
    assert_eq!(unique.len(), carried.len(), "{carried:?}");
    let rows = record["references"].as_array().unwrap();
    let verdict = |id: &str| {
        rows.iter()
            .find(|r| r["id"] == id)
            .map_or(Value::Null, |r| r["verdict"].clone())
    };
    assert_eq!(verdict("block:snapshot-diff"), "unrelated");
    assert_eq!(verdict("block:refund-gate"), "applies");
    // Every reference was asked of the seat, by its own question.
    let asked = seat.asked.lock().unwrap().clone();
    let qualifications = asked.iter().filter(|id| id.starts_with("reference-"));
    assert_eq!(qualifications.count() as u64, found);
    // The agenda says which action the author's composition was.
    assert_eq!(decision["agenda"][0]["action"], "compose: plan");
}

#[tokio::test]
async fn without_a_seat_the_recall_is_shown_unqualified_and_said_so() {
    let author = Rotating::new(vec![plan().to_string()]);
    let request = CompileRequest::create(INTENT)
        .with_authoring_policy(policy())
        .with_authoring_knowledge(pack());
    let cognition = Cognition {
        provider: Some(&author),
        seat: None,
    };
    let out = compile_with_cognition(&request, cognition).await.unwrap();
    let carried = sent(&out);
    assert!(
        carried.iter().any(|id| id == "block:snapshot-diff"),
        "{carried:?}"
    );
    let decision = out.provenance.decision.as_ref().unwrap();
    let record = &decision["knowledge_qualification"];
    assert_eq!(record["by"], Value::Null, "{record:#}");
    assert_eq!(record["found"], record["unqualified"]);
    assert!(record["why"].as_str().unwrap().contains("no decision seat"));
}
