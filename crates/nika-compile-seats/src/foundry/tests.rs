// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

use std::sync::Mutex;

use nika_compile::{AuthoringKnowledge, KnowledgeReference};
use serde_json::{Value, json};

use super::{APPLIES, UNRELATED, Verdict, qualify, question, trace, verdict};
use crate::decide::{
    ChoiceAnswer, ChoiceBatch, ChoiceFuture, ChoiceQuestion, DecisionError, DecisionSeat,
    NONE_OPTION,
};

/// A seat that answers by the reference id its question carries, and counts what it was asked.
struct ById {
    answers: Vec<(&'static str, Result<&'static str, &'static str>)>,
    asked: Mutex<Vec<String>>,
    batches: Mutex<u32>,
}

impl ById {
    fn new(answers: Vec<(&'static str, Result<&'static str, &'static str>)>) -> Self {
        Self {
            answers,
            asked: Mutex::new(Vec::new()),
            batches: Mutex::new(0),
        }
    }
}

impl DecisionSeat for ById {
    fn name(&self) -> &'static str {
        "test/by-id"
    }
    fn choose<'a>(&'a self, question: &'a ChoiceQuestion) -> ChoiceFuture<'a> {
        let id = question.state["reference"]["id"]
            .as_str()
            .unwrap_or_default()
            .to_owned();
        self.asked.lock().unwrap().push(id.clone());
        let answer = (self.answers.iter())
            .find(|(of, _)| *of == id)
            .map_or(Err("unscripted"), |(_, answer)| *answer);
        Box::pin(async move {
            match answer {
                Ok(choice) => Ok(ChoiceAnswer::new(choice, "test/by-id")),
                Err(why) => Err(DecisionError(why.to_owned())),
            }
        })
    }
    fn choose_each<'a>(&'a self, batch: &'a ChoiceBatch) -> crate::decide::BatchFuture<'a> {
        *self.batches.lock().unwrap() += 1;
        crate::decide::each_alone(batch, |question| self.choose(question))
    }
}

fn reference(kind: &str, id: &str, text: &str) -> KnowledgeReference {
    KnowledgeReference {
        kind: kind.to_owned(),
        id: id.to_owned(),
        text: text.to_owned(),
    }
}

fn pack(references: Vec<KnowledgeReference>) -> AuthoringKnowledge {
    AuthoringKnowledge {
        identity: json!({"door": {"pack_sha256": "abc"}}),
        selection: json!({"retriever": "bm25"}),
        references,
        repairs: std::collections::BTreeMap::new(),
    }
}

const INTENT: &str = "Read ./calendar.json and save one reminder per appointment to ./out/r.json";

fn ids(knowledge: &AuthoringKnowledge) -> Vec<&str> {
    knowledge.references.iter().map(|r| r.id.as_str()).collect()
}

#[tokio::test]
async fn the_seat_discards_the_unrelated_and_an_abstention_or_a_failure_stays_a_hypothesis() {
    let seat = ById::new(vec![
        ("block:reminders", Ok(APPLIES)),
        ("block:snapshot-diff", Ok(UNRELATED)),
        ("pattern:guard", Ok(NONE_OPTION)),
        ("example:x", Err("transport failed")),
    ]);
    let found = pack(vec![
        reference("block", "block:reminders", "a"),
        reference("block", "block:snapshot-diff", "b"),
        reference("pattern", "pattern:guard", "c"),
        reference("example", "example:x", "d"),
    ]);
    let qualified = qualify(INTENT, &found, &seat, "decision_seat").await;
    // Asked together, once each.
    assert_eq!(*seat.batches.lock().unwrap(), 1);
    assert_eq!(seat.asked.lock().unwrap().len(), 4);
    assert_eq!(
        ids(&qualified.knowledge),
        ["block:reminders", "pattern:guard", "example:x"],
        "only the reference the seat found unrelated leaves the pack"
    );
    assert_eq!(qualified.knowledge.identity, found.identity);
    assert_eq!(qualified.knowledge.selection, found.selection);
    let record = &qualified.record;
    let counts: Vec<&Value> = ["found", "shown", "applies", "discarded", "unqualified"]
        .iter()
        .map(|k| &record[*k])
        .collect();
    assert_eq!(
        counts,
        [&json!(4), &json!(3), &json!(1), &json!(1), &json!(2)]
    );
    assert_eq!(record["by"], "decision_seat");
    assert_eq!(record["seat"], "test/by-id");
    let verdicts: Vec<&str> = (record["references"].as_array().unwrap().iter())
        .map(|r| r["verdict"].as_str().unwrap())
        .collect();
    assert_eq!(
        verdicts,
        ["applies", "unrelated", "unqualified", "unqualified"]
    );
    let rows = record["references"].as_array().unwrap();
    assert_eq!(rows[2]["reason"], "the seat could not tell (none)");
    assert_eq!(rows[3]["reason"], "transport failed");
    assert_eq!(rows[3]["answer"]["error"], "transport failed");
    // The record keeps digests, never a reference's text.
    assert!(
        rows.iter()
            .all(|r| r.get("text").is_none() && r["sha256"].as_str().map(str::len) == Some(64))
    );
}

#[test]
fn an_answer_outside_the_options_never_qualifies_a_reference() {
    let asked = question(INTENT, 0, &reference("block", "block:a", "x"));
    let outside = ChoiceAnswer::new("lookup", "m");
    assert!(matches!(
        verdict(&asked, Ok(&outside)),
        Verdict::Unqualified(_)
    ));
    let applies = ChoiceAnswer::new(APPLIES, "m");
    assert_eq!(verdict(&asked, Ok(&applies)), Verdict::Applies);
    // The question offers exactly the two verdicts and NONE, and carries the whole texts.
    assert_eq!(asked.keys(), [APPLIES, UNRELATED, NONE_OPTION]);
    assert_eq!(asked.state["request"], INTENT);
    assert_eq!(asked.state["reference"]["text"], "x");
}

#[tokio::test]
async fn an_empty_recall_asks_nothing() {
    let seat = ById::new(Vec::new());
    let qualified = qualify(INTENT, &pack(Vec::new()), &seat, "decision_seat").await;
    assert_eq!(*seat.batches.lock().unwrap(), 0);
    assert_eq!(qualified.record["found"], 0);
    assert_eq!(qualified.record["seat_calls"], 0);
}

#[test]
fn the_trace_reads_which_shown_code_the_candidate_kept() {
    let block = "Remind — one per row\n```yaml\n  remind:\n    invoke: { tool: \"nika:jq\" }\n    args: { filter: \"map(select(.status != \\\"cancelled\\\"))\" }\n```";
    let adapted = "Other\n```yaml\n    args: { filter: \"map(select(.status != \\\"cancelled\\\"))\" }\n    with: { rows: \"${{ tasks.read_calendar.output }}\" }\n    with: { more: \"${{ tasks.something_else.output }}\" }\n```";
    let unused = "Diff\n```yaml\n    invoke: { tool: \"nika:json_diff\" }\n```";
    let shown = [
        reference("block", "block:remind", block),
        reference("example", "example:adapted", adapted),
        reference("block", "block:diff", unused),
        reference(
            "pattern",
            "pattern:guard",
            "- pattern:guard — a long enough description line",
        ),
    ];
    let candidate = "tasks:\n  remind:\n    invoke: { tool: \"nika:jq\" }\n    args: { filter: \"map(select(.status != \\\"cancelled\\\"))\" }\n";
    let traced = trace(&shown, candidate);
    let uses: Vec<&str> = (traced["references"].as_array().unwrap().iter())
        .map(|r| r["use"].as_str().unwrap())
        .collect();
    assert_eq!(uses, ["instantiated", "adapted", "not_traced", "consulted"]);
    assert_eq!(traced["instantiated"], 1);
    assert_eq!(traced["adapted"], 1);
    assert_eq!(traced["not_traced"], 1);
    assert_eq!(traced["consulted"], 1);
}
