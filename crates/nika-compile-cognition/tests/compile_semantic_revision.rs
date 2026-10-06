// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>
//! A nonconstant revision of a recorded semantic candidate goes through the semantic compiler
//! (R4 F, test E): the stored record must reassemble the exact base bytes, the seat proposes a
//! bounded change against the base graph and the explicit change, unaffected tasks, edges,
//! outputs, gates and prohibitions stay, and the obligation delta is explicit. A tampered base
//! or record, a historical base, a constant edit and a one-call ceiling are each held. The
//! seats are injected doubles: they script answers and refuse any whole-source schema; they test
//! mechanics, never model capability.
//!
//! A failure of any `HARNESS_INVALID` expectation (fixture, harness or base creation) is fixed
//! or archived as harness-invalid before any RED is counted; every positive assertion must be
//! reached by the run that counts it.
#![allow(clippy::unwrap_used, clippy::expect_used)]

/// A revision whose bytes the judge rejected in an earlier round, kept beside this file to bound
/// its size.
#[path = "compile_semantic_revision/carried.rs"]
mod carried;

use std::sync::Mutex;
use std::time::Duration;

use nika_compile::{
    AuthoringPolicy, CompileOutcome, CompileRequest, CompileStatus, DiagnosticKind, NativeMode,
};
use nika_compile_cognition::compile_with_provider;
use nika_kernel::ai::provider::{
    ContentBlock, InferRequest, InferResponse, ProviderError, ProviderInferDyn, ResponseFormat,
    StopReason, TokenUsage,
};
use serde_json::{Value, json};

/// Two outputs, one approval before either write, one prohibition.
const INTENT: &str = "Read ./data/orders.json. Keep the orders whose status is paid and write them to ./out/paid.json. Keep only the orders whose total is above 10. Count all orders and write the count to ./out/count.txt. Ask me before writing anything. Never delete any file.";
/// The explicit change: one business rule, nothing else.
const CHANGE: &str = "Keep the orders whose status is shipped instead of paid.";

/// A seat that answers in order and REFUSES any request whose schema asks for whole source
/// (`candidate` / `candidate_lines`): a production revision must stay semantic.
struct Semantic {
    answers: Vec<String>,
    asked: Mutex<Vec<Value>>,
    /// The messages of every attempt, in order, as sent.
    sent: Mutex<Vec<Value>>,
    /// The admission layer's call ceiling, as `semantic_verification`'s `Scripted::ceiling`:
    /// every call from this index is refused locally, counted as attempted, never answered.
    refused_from: usize,
    /// The whole-request verdicts to answer, in order (`faithful` once spent). After a verdict
    /// that does not carry the request, the part the change replaced, asked alone, is `missing`
    /// ([`judged_part`]) and its task question is answered `pointed`.
    verdicts: Mutex<Vec<&'static str>>,
    /// The answer to the task question of the part judged missing (`task-<id>`, `omitted` or
    /// `no_task`): by default the task that keeps the orders.
    pointed: &'static str,
    /// The state of every whole-request question, as the judge read it.
    states: Mutex<Vec<Value>>,
}

impl Semantic {
    fn new(answers: Vec<Value>) -> Self {
        Self {
            answers: answers
                .into_iter()
                .map(|answer| answer.to_string())
                .collect(),
            asked: Mutex::new(Vec::new()),
            sent: Mutex::new(Vec::new()),
            refused_from: usize::MAX,
            verdicts: Mutex::new(Vec::new()),
            pointed: "task-keep",
            states: Mutex::new(Vec::new()),
        }
    }
    fn judging(answers: Vec<Value>, verdicts: Vec<&'static str>, ceiling: usize) -> Self {
        Self {
            verdicts: Mutex::new(verdicts),
            ..Self::ceiling(answers, ceiling)
        }
    }
    /// The same double answering the missing part's task question `choice`.
    fn pointing(self, choice: &'static str) -> Self {
        Self {
            pointed: choice,
            ..self
        }
    }
    fn ceiling(answers: Vec<Value>, ceiling: usize) -> Self {
        Self {
            refused_from: ceiling,
            ..Self::new(answers)
        }
    }
    /// Every attempt the double saw, refused ones included.
    fn calls(&self) -> usize {
        self.asked.lock().unwrap().len()
    }
    /// The attempts it answered (sent and answered): refused ones excluded.
    fn answered(&self) -> usize {
        self.calls().min(self.refused_from)
    }
}

impl ProviderInferDyn for Semantic {
    async fn infer(&self, request: InferRequest) -> Result<InferResponse, ProviderError> {
        let schema = match &request.response_format {
            ResponseFormat::JsonSchema(schema) => schema.clone(),
            _ => Value::Null,
        };
        let props = schema["properties"].clone();
        assert!(
            props.get("candidate").is_none() && props.get("candidate_lines").is_none(),
            "a whole-source schema was requested: {schema}"
        );
        let messages = serde_json::to_value(&request.messages).unwrap();
        self.sent.lock().unwrap().push(messages);
        let at = {
            let mut asked = self.asked.lock().unwrap();
            asked.push(schema);
            asked.len() - 1
        };
        if at >= self.refused_from {
            return Err(ProviderError::AdmissionDenied {
                reason: format!(
                    "the authoring call ceiling of {} calls is reached",
                    self.refused_from
                ),
            });
        }
        let text = match props["choice"]["enum"].as_array() {
            // A judge's closed choice: the scripted verdict, else approve.
            Some(keys) if keys.iter().any(|k| k == "faithful") => {
                self.states.lock().unwrap().push(shown(&request));
                let mut verdicts = self.verdicts.lock().unwrap();
                let verdict = if verdicts.is_empty() {
                    "faithful"
                } else {
                    verdicts.remove(0)
                };
                json!({"choice": verdict}).to_string()
            }
            Some(keys) if keys.iter().any(|k| k == "superseded") => {
                json!({"choice": judged_part(&shown(&request))}).to_string()
            }
            Some(keys) if keys.iter().any(|k| k == "no_task") => {
                json!({"choice": self.pointed}).to_string()
            }
            Some(keys) if keys.iter().any(|k| k == "only_requested") => {
                r#"{"choice":"only_requested"}"#.into()
            }
            Some(keys) if keys.iter().any(|k| k == "carried") => r#"{"choice":"carried"}"#.into(),
            _ => self.answers.get(at).cloned().unwrap_or_default(),
        };
        Ok(InferResponse::new(
            vec![ContentBlock::Text { text }],
            TokenUsage::new(1, 1),
            StopReason::EndTurn,
        ))
    }
}

/// The STATE a closed choice showed its judge (the JSON before its OPTIONS).
fn shown(request: &InferRequest) -> Value {
    let said = (request.messages.last()).and_then(|m| match m.content.first() {
        Some(ContentBlock::Text { text }) => text.strip_prefix("STATE:\n"),
        _ => None,
    });
    let state = said.and_then(|s| s.split("\n\nOPTIONS:").next());
    state
        .and_then(|s| serde_json::from_str(s).ok())
        .unwrap_or(Value::Null)
}

/// A part of the request asked alone: the part the change replaced is `missing` (its task
/// question follows), every other part `carried`.
fn judged_part(state: &Value) -> &'static str {
    let part = state["clause"]["text"].as_str().unwrap_or_default();
    if part.starts_with(SHIPPED) {
        "missing"
    } else {
        "carried"
    }
}

fn policy(repairs: u32) -> AuthoringPolicy {
    AuthoringPolicy::new("mock/semantic", 4096, Duration::from_secs(2))
        .with_native(NativeMode::Sketch)
        .with_repairs(repairs)
}

/// The base graph: read → keep (jq) → write paid; read → count (jq) → write count; one prompt
/// gate dominating both writes; two named outputs.
fn graph(status: &str) -> Value {
    json!({"name": "orders", "tasks": [
        {"id": "read_orders", "verb": "invoke", "tool": "nika:read", "reads": ["./data/orders.json"], "purpose": "the orders"},
        {"id": "keep", "verb": "invoke", "tool": "nika:jq", "with": [{"name": "document", "from": "read_orders"}], "purpose": format!("keep the {status} orders")},
        {"id": "count", "verb": "invoke", "tool": "nika:jq", "with": [{"name": "document", "from": "read_orders"}], "purpose": "count all orders"},
        {"id": "approve", "verb": "invoke", "tool": "nika:prompt", "with": [{"name": "kept", "from": "keep"}], "purpose": "ask before writing"},
        {"id": "write_kept", "verb": "invoke", "tool": "nika:write", "writes": ["./out/paid.json"], "with": [{"name": "kept", "from": "keep"}], "gated_by": "approve", "purpose": "write the kept orders"},
        {"id": "write_count", "verb": "invoke", "tool": "nika:write", "writes": ["./out/count.txt"], "with": [{"name": "total", "from": "count"}], "gated_by": "approve", "purpose": "write the count"}],
        "outputs": [{"name": "kept", "from": "write_kept"}, {"name": "count", "from": "write_count"}],
        "questions": [{"key": "const.approval_message", "label": "the approval message",
                       "answer_type": "text", "why": "the request leaves it open"}],
        "gaps": [], "notes": "graph"})
}

/// The original clause the change replaces and the change clause that replaces it, as the
/// reader reads them (the seat copies them from the clauses it is shown).
const PAID: &str = "Keep the orders whose status is paid";
const SHIPPED: &str = "Keep the orders whose status is shipped instead of paid";

/// The seat's first revision answer: the supersessions it states and no addition (the graph
/// stays the base's).
fn revised(links: Value) -> Value {
    let mut answer = json!({"notes": "links", "adds": []});
    answer["supersedes"] = links;
    answer
}

fn link(replaces: &str, by: &str) -> Value {
    json!([{"replaces": replaces, "by": by}])
}

fn fills(status: &str) -> Value {
    json!({"fills": [
        {"task": "keep", "field": "expression", "value": format!("fromjson | map(select(.total > 10)) | map(select(.status == \"{status}\"))")},
        {"task": "count", "field": "expression", "value": "fromjson | length"},
        {"task": "approve", "field": "args.message", "value": "${{ const.approval_message }}"}],
        "notes": "fills"})
}

/// The recorded base: a real semantic CREATE through the public seam, in its two rounds. The
/// graph leaves the approval message open (`const.approval_message`); its answer round replays
/// the record with zero calls and bakes the answer into a real constant.
async fn base() -> (CompileOutcome, String, Value) {
    base_observed(None).await
}

async fn base_observed(world: Option<Value>) -> (CompileOutcome, String, Value) {
    let seat = Semantic::new(vec![graph("paid"), fills("paid")]);
    let mut request = CompileRequest::create(INTENT).with_authoring_policy(policy(1));
    if let Some(world) = world {
        request = request.with_knowledge(world);
    }
    let asked = compile_with_provider(&request, &seat).await.unwrap();
    assert!(
        asked
            .questions
            .iter()
            .any(|q| q.key == "const.approval_message"),
        "HARNESS_INVALID base question: {asked:#?}"
    );
    let first = asked
        .provenance
        .plan
        .clone()
        .expect("HARNESS_INVALID: first record");
    let replay = Semantic::new(vec![]);
    let answered = request
        .with_plan(first)
        .answer("const.approval_message", "\"Write the files?\"");
    let out = compile_with_provider(&answered, &replay).await.unwrap();
    // The answer round replays the record: no authoring call, only the round's own judge.
    let roles: Vec<String> = (out.provenance.authoring.iter())
        .flat_map(|receipt| receipt.context.iter())
        .filter_map(|call| call["call"].as_str().map(str::to_owned))
        .collect();
    assert!(
        roles.iter().all(|role| role.starts_with("judge_")),
        "HARNESS_INVALID: the answer round replays without authoring: {roles:?}"
    );
    assert_eq!(
        out.status,
        CompileStatus::Ready,
        "HARNESS_INVALID base: {out:#?}"
    );
    let record = out
        .provenance
        .plan
        .clone()
        .expect("HARNESS_INVALID: semantic record");
    assert_eq!(record["semantic_record"], 1, "HARNESS_INVALID");
    let bytes = out.candidate.clone().unwrap();
    (out, bytes, record)
}

fn revise(bytes: &str, record: &Value, repairs: u32) -> CompileRequest {
    CompileRequest::edit(bytes, CHANGE)
        .with_original_intent(INTENT)
        .with_plan(record.clone())
        .with_authoring_policy(policy(repairs))
}

fn doc(out: &CompileOutcome) -> Value {
    serde_yaml_bw::from_str(out.candidate.as_deref().unwrap()).unwrap()
}

/// The independent expected result, written by hand from the base document: only the `keep`
/// program's literal changes.
fn expected(base: &Value) -> Value {
    let mut want = base.clone();
    want["tasks"]["keep"]["invoke"]["args"]["expression"] =
        json!("fromjson | map(select(.total > 10)) | map(select(.status == \"shipped\"))");
    want
}

#[tokio::test]
async fn a_semantic_revision_changes_one_rule_and_keeps_the_rest() {
    let (_, bytes, record) = base().await;
    // The seat proposes the revised graph (same IDs/edges/gate/outputs) and the changed fill.
    let seat = Semantic::new(vec![revised(link(PAID, SHIPPED)), fills("shipped")]);
    let out = compile_with_provider(&revise(&bytes, &record, 1), &seat)
        .await
        .unwrap();
    assert_eq!(out.status, CompileStatus::Ready, "{out:#?}");
    // Never source authoring: the double refuses any whole-source schema, and the route is the
    // semantic revision (the sketch door labels its candidates `native`, as at creation).
    let decision = out.provenance.decision.as_ref().unwrap().to_string();
    assert!(
        decision.contains("edit: semantic revision through the sketch door"),
        "{decision}"
    );
    // The forensic record names the revision's own door, never an answer-round replay of the
    // record it binds, and the compiler as the source's owner.
    let door = &out.provenance.decision.as_ref().unwrap()["forensic"]["door"];
    assert_eq!(door["name"], "sketch", "{door}");
    assert_eq!(door["reason"], "nonconstant_revision", "{door}");
    assert_eq!(door["source_owner"], "compiler_from_model_sketch_and_fills");
    let base_doc: Value = serde_yaml_bw::from_str(&bytes).unwrap();
    assert_eq!(
        doc(&out),
        expected(&base_doc),
        "only the changed rule moved"
    );
    // Unaffected: second output, its task and edge, the gate on both writes, the outputs list.
    for kept in ["count", "write_count", "approve", "read_orders"] {
        assert_eq!(doc(&out)["tasks"][kept], base_doc["tasks"][kept], "{kept}");
    }
    assert_eq!(doc(&out)["outputs"], base_doc["outputs"]);
    // The new record is a semantic record of the revision, bound to the new bytes.
    let next = out.provenance.plan.as_ref().expect("revised record");
    assert_eq!(next["semantic_record"], 1);
    // The original request is kept, the change is explicit, the delta resolved (one rule).
    let decision = out.provenance.decision.as_ref().unwrap();
    let delta = &decision["revision"]; // exact key pinned at lease (existing projection first)
    assert_eq!(delta["original"], INTENT);
    assert_eq!(delta["change"], CHANGE);
    assert_eq!(
        delta["superseded"].as_array().map(Vec::len),
        Some(1),
        "{delta:#}"
    );
    assert_eq!(delta["superseded"][0]["evidence"], PAID);
    // Same-kind siblings stay: both writes and the prohibition are `effect` duties, all kept.
    let kept: Vec<(&str, &str)> = (delta["kept"].as_array().unwrap().iter())
        .filter_map(|d| Some((d["evidence"].as_str()?, d["kind"].as_str()?)))
        .collect();
    for clause in [
        "write them to ./out/paid.json",
        "write the count to ./out/count.txt",
        "Never delete any file",
    ] {
        assert!(kept.contains(&(clause, "effect")), "{clause}: {kept:?}");
    }
    // The prohibition stays a designated obligation of the revision: the same duty entry (kind
    // and evidence) in the base record's reading ledger and in the revised record's, and listed
    // as kept by the delta, never superseded.
    let duty = |record: &Value| -> Option<Value> {
        (record["basis"]["read"]["ledger"].as_array()?.iter())
            .find(|d| {
                d["evidence"]
                    .as_str()
                    .is_some_and(|e| e.contains("Never delete any file"))
            })
            .cloned()
    };
    let before = duty(&record).expect("HARNESS_INVALID: the base reads the prohibition");
    let after = duty(next).expect("the revision keeps the prohibition");
    assert_eq!(
        (&after["kind"], &after["evidence"]),
        (&before["kind"], &before["evidence"])
    );
    assert!(
        !delta["superseded"]
            .to_string()
            .contains("Never delete any file"),
        "{delta:#}"
    );
}

#[tokio::test]
async fn a_tampered_base_or_record_is_refused_before_any_call() {
    let (_, bytes, record) = base().await;
    let mut tampered_bytes = bytes.clone();
    tampered_bytes.push_str("# one byte more\n");
    let mut tampered_record = record.clone();
    tampered_record["fills"][0]["value"] = json!("fromjson");
    // The control: the same untampered pair reaches the seat (a revision is attempted), so the
    // tampered refusals below are discriminating, not a refusal of every record-bearing EDIT.
    let control = Semantic::new(vec![revised(link(PAID, SHIPPED)), fills("shipped")]);
    let _ = compile_with_provider(&revise(&bytes, &record, 1), &control)
        .await
        .unwrap();
    assert!(control.calls() >= 1, "the untampered pair is revised");
    for (own_base, request) in [
        (&tampered_bytes, revise(&tampered_bytes, &record, 1)),
        (&bytes, revise(&bytes, &tampered_record, 1)),
    ] {
        let seat = Semantic::new(vec![revised(link(PAID, SHIPPED)), fills("shipped")]);
        let out = compile_with_provider(&request, &seat).await.unwrap();
        assert_eq!(seat.calls(), 0, "{out:#?}");
        assert_ne!(out.status, CompileStatus::Ready);
        // This request's own base is preserved, never rewritten.
        assert!(
            out.candidate.is_none() || out.candidate.as_deref() == Some(own_base.as_str()),
            "{out:#?}"
        );
    }
}

#[tokio::test]
async fn a_historical_base_is_preserved_with_a_precise_limitation() {
    // A hand-written source with no semantic record, outside any proven import subset.
    let historical = "nika: legacy\npermits:\n  tools: [\"nika:write\"]\n  fs:\n    write: [\"./out/a.txt\"]\ntasks:\n  save:\n    invoke:\n      tool: \"nika:write\"\n      args: {path: \"./out/a.txt\", content: \"a\"}\n";
    let seat = Semantic::new(vec![graph("shipped"), fills("shipped")]);
    let request = CompileRequest::edit(historical, CHANGE)
        .with_original_intent(INTENT)
        .with_authoring_policy(policy(1));
    let out = compile_with_provider(&request, &seat).await.unwrap();
    // One typed reading call (choice A; the double refuses any whole-source schema), whose
    // answer is no revision: never a source rewrite, the base kept with the reason named.
    assert_eq!(seat.calls(), 1, "{out:#?}");
    assert_ne!(out.status, CompileStatus::Ready);
    assert!(out.candidate.is_none(), "{out:#?}");
    assert!(
        out.diagnostics
            .iter()
            .any(|d| d.message.contains("not a revision answer")),
        "a precise limitation: {out:#?}"
    );
}

#[tokio::test]
async fn a_constant_edit_stays_zero_call_and_changes_exactly_that_constant() {
    let (_, bytes, record) = base().await;
    let base_doc: Value = serde_yaml_bw::from_str(&bytes).unwrap();
    // A real constant of the base, chosen from its own `const:` map (not a guessed name).
    let consts = base_doc["const"]
        .as_object()
        .expect("HARNESS_INVALID: the base declares const");
    let (name, old) = consts
        .iter()
        .next()
        .expect("HARNESS_INVALID: one const exists");
    let new = json!(format!(
        "{}-revised",
        old.as_str().expect("HARNESS_INVALID: a text const")
    ));
    let seat = Semantic::new(vec![]);
    let request = CompileRequest::set_constant(&bytes, name.as_str(), new.to_string())
        .with_plan(record)
        .with_authoring_policy(policy(1));
    let mut want = base_doc.clone();
    want["const"][name.as_str()] = new.clone();
    // The ablation control: the same constant edit without the record is the existing zero-call
    // door, READY with exactly that constant changed (the positive is reachable).
    let bare = CompileRequest::set_constant(&bytes, name.as_str(), new.to_string());
    let control = compile_with_provider(&bare, &seat).await.unwrap();
    assert_eq!(
        control.status,
        CompileStatus::Ready,
        "control: {control:#?}"
    );
    assert_eq!(
        doc(&control),
        want,
        "control: exactly that constant changed"
    );
    let out = compile_with_provider(&request, &seat).await.unwrap();
    assert_eq!(seat.calls(), 0, "{out:#?}");
    assert_eq!(out.status, CompileStatus::Ready, "{out:#?}");
    assert_eq!(doc(&out), want, "exactly that constant changed");
}

#[tokio::test]
async fn a_one_call_ceiling_sends_one_request_and_never_a_hidden_second() {
    let (_, bytes, record) = base().await;
    // Admission refuses every call after the first: the graph is asked, the fills are refused
    // locally; the outcome names the refusal, is not READY, and no second answered request exists.
    let seat = Semantic::ceiling(vec![revised(link(PAID, SHIPPED)), fills("shipped")], 1);
    let out = compile_with_provider(&revise(&bytes, &record, 0), &seat)
        .await
        .unwrap();
    assert_eq!(
        seat.answered(),
        1,
        "exactly one request was sent and answered: {out:#?}"
    );
    assert_eq!(
        seat.calls(),
        2,
        "the second was attempted and refused by admission: {out:#?}"
    );
    assert_ne!(out.status, CompileStatus::Ready);
    // The receipt counts attempts; the refused one is journaled as refused by admission before
    // any transport and is never told as sent (the convention `semantic_verification` pins).
    let receipt = out.provenance.authoring.as_ref().expect("receipt");
    let refused: Vec<&Value> = (receipt.context.iter())
        .filter(|call| call["result"]["failure_kind"] == "admission_refused")
        .collect();
    assert_eq!(refused.len(), 1, "{receipt:#?}");
    let told: Vec<&str> = out.diagnostics.iter().map(|d| d.message.as_str()).collect();
    assert!(!told.iter().any(|m| m.contains("was sent")), "{told:?}");
}

#[tokio::test]
async fn a_revision_without_its_link_or_with_a_malicious_one_is_never_kept() {
    let (_, bytes, record) = base().await;
    for (links, why) in [
        (json!([]), "neither supersedes a clause with it nor adds it"),
        (link("Never delete any file", SHIPPED), "a prohibition"),
        (
            link("Keep the paid orders", SHIPPED),
            "not stated exactly once",
        ),
        (
            link(PAID, "ship everything"),
            "not a clause the change states",
        ),
    ] {
        let seat = Semantic::new(vec![revised(links), fills("shipped")]);
        let out = compile_with_provider(&revise(&bytes, &record, 0), &seat)
            .await
            .unwrap();
        assert_ne!(out.status, CompileStatus::Ready, "{why}: {out:#?}");
        assert!(
            out.candidate.is_none() && out.provenance.plan.is_none(),
            "{why}: {out:#?}"
        );
        assert!(
            out.diagnostics
                .iter()
                .any(|d| d.target == "revision" && d.message.contains(why)),
            "{why}: {out:#?}"
        );
    }
}

/// The revision's own record binds its bytes as the next base (B → C): a next revision binds B
/// by B's record (a zero-call constant edit is READY), never by A's words or record, and B's
/// tampered record is refused before any call.
#[tokio::test]
async fn a_revised_record_binds_the_next_revision() {
    let (_, bytes, record) = base().await;
    let seat = Semantic::new(vec![revised(link(PAID, SHIPPED)), fills("shipped")]);
    let b = compile_with_provider(&revise(&bytes, &record, 1), &seat)
        .await
        .unwrap();
    assert_eq!(b.status, CompileStatus::Ready, "HARNESS_INVALID B: {b:#?}");
    let (b_bytes, b_record) = (
        b.candidate.clone().unwrap(),
        b.provenance.plan.clone().unwrap(),
    );
    let none = Semantic::new(vec![]);
    let next = CompileRequest::set_constant(&b_bytes, "approval_message", "\"Write both?\"")
        .with_plan(b_record.clone())
        .with_authoring_policy(policy(1));
    let c = compile_with_provider(&next, &none).await.unwrap();
    assert_eq!(none.calls(), 0, "{c:#?}");
    assert_eq!(
        c.status,
        CompileStatus::Ready,
        "C binds B by B's record: {c:#?}"
    );
    // A's record never binds B's bytes.
    let stale = CompileRequest::set_constant(&b_bytes, "approval_message", "\"Write both?\"")
        .with_plan(record)
        .with_authoring_policy(policy(1));
    let refused = compile_with_provider(&stale, &none).await.unwrap();
    assert_ne!(refused.status, CompileStatus::Ready, "{refused:#?}");
    let mut tampered = b_record;
    tampered["fills"][0]["value"] = json!("fromjson");
    let request = revise(&b_bytes, &tampered, 1);
    let out = compile_with_provider(&request, &none).await.unwrap();
    assert_eq!(none.calls(), 0, "{out:#?}");
    assert_ne!(out.status, CompileStatus::Ready);
}

/// The second change in words: delivered instead of shipped, stated against B's resolved request.
const DELIVERED_CHANGE: &str = "Keep the orders whose status is delivered instead of shipped.";
const DELIVERED: &str = "Keep the orders whose status is delivered instead of shipped";
/// The second filter of the same kind every revision keeps. This reader records one filter
/// clause per request (which one depends on the words around it), so the revision guarantees it
/// in the resolved words and in the program, never through a ledger the reader may not fill.
const TOTAL: &str = "Keep only the orders whose total is above 10";

/// The reader's clauses of a record's own contract (its basis ledger), each once.
fn contract(record: &Value) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for d in record["basis"]["read"]["ledger"].as_array().unwrap() {
        let evidence = d["evidence"].as_str().unwrap().to_owned();
        if !out.contains(&evidence) {
            out.push(evidence);
        }
    }
    out
}

/// A → B → C in words (R4 F): each revision consumes the resolved request of the one before, so
/// C binds B's contract, never A's stale words; a superseded clause is no obligation of the next
/// contract, and the second filter of the same kind, the writes and the prohibition stay.
#[tokio::test]
async fn a_second_revision_in_words_consumes_the_first_ones_resolved_contract() {
    let (_, a_bytes, a_record) = base().await;
    let seat = Semantic::new(vec![revised(link(PAID, SHIPPED)), fills("shipped")]);
    let b = compile_with_provider(&revise(&a_bytes, &a_record, 1), &seat)
        .await
        .unwrap();
    assert_eq!(b.status, CompileStatus::Ready, "B: {b:#?}");
    let b_record = b.provenance.plan.clone().unwrap();
    // The independent oracle: B's resolved request, written by hand.
    let b_words = "Read ./data/orders.json. Keep the orders whose status is shipped instead of paid and write them to ./out/paid.json. Keep only the orders whose total is above 10. Count all orders and write the count to ./out/count.txt. Ask me before writing anything. Never delete any file.";
    assert_eq!(b_record["basis"]["read"]["effective"], b_words);
    assert_eq!(
        b.provenance.decision.as_ref().unwrap()["revision"]["resolved"],
        b_words
    );
    let b_contract = contract(&b_record);
    assert!(!b_contract.iter().any(|c| c == PAID), "{b_contract:?}");
    for clause in [
        SHIPPED,
        "Never delete any file",
        "write the count to ./out/count.txt",
    ] {
        assert!(
            b_contract.iter().any(|c| c == clause),
            "B keeps {clause}: {b_contract:?}"
        );
    }
    assert!(
        INTENT.contains(TOTAL) && b_words.contains(TOTAL),
        "the second filter stays"
    );
    // B → C in words; the caller restates A's words, which are never trusted.
    let b_bytes = b.candidate.clone().unwrap();
    let request = CompileRequest::edit(b_bytes.as_str(), DELIVERED_CHANGE)
        .with_original_intent(INTENT)
        .with_plan(b_record.clone())
        .with_authoring_policy(policy(1));
    let seat = Semantic::new(vec![revised(link(SHIPPED, DELIVERED)), fills("delivered")]);
    let c = compile_with_provider(&request, &seat).await.unwrap();
    assert_eq!(c.status, CompileStatus::Ready, "C: {c:#?}");
    let c_words = "Read ./data/orders.json. Keep the orders whose status is delivered instead of shipped and write them to ./out/paid.json. Keep only the orders whose total is above 10. Count all orders and write the count to ./out/count.txt. Ask me before writing anything. Never delete any file.";
    let c_record = c.provenance.plan.clone().unwrap();
    assert_eq!(c_record["basis"]["read"]["effective"], c_words);
    let delta = &c.provenance.decision.as_ref().unwrap()["revision"];
    assert_eq!(delta["original"], b_words, "C revises B's contract");
    assert_eq!(delta["superseded"][0]["evidence"], SHIPPED);
    let c_contract = contract(&c_record);
    for gone in [PAID, SHIPPED] {
        assert!(
            !c_contract.iter().any(|c| c == gone),
            "{gone}: {c_contract:?}"
        );
    }
    assert!(
        c_words.contains(TOTAL),
        "the second filter stays in C's resolved words"
    );
    for clause in [
        DELIVERED,
        "Never delete any file",
        "write the count to ./out/count.txt",
    ] {
        assert!(
            c_contract.iter().any(|c| c == clause),
            "C keeps {clause}: {c_contract:?}"
        );
    }
    // The program: only the status literal moved, the total filter and the rest stay.
    let base_doc: Value = serde_yaml_bw::from_str(&a_bytes).unwrap();
    let mut want = base_doc.clone();
    want["tasks"]["keep"]["invoke"]["args"]["expression"] =
        json!("fromjson | map(select(.total > 10)) | map(select(.status == \"delivered\"))");
    assert_eq!(doc(&c), want);
}

/// The seat's revision notes are never journaled as text, on an accepted revision as on a refused
/// one: a sentinel in them appears nowhere in the outcome (its record, decision or receipt).
#[tokio::test]
async fn the_seats_revision_notes_are_withheld_from_the_journal() {
    const SENTINEL: &str = "REVISION-NOTES-SENTINEL-7f3a";
    let (_, bytes, record) = base().await;
    for links in [link(PAID, SHIPPED), link("Never delete any file", SHIPPED)] {
        let mut answer = revised(links);
        answer["notes"] = json!(format!("private reasoning {SENTINEL}"));
        let seat = Semantic::new(vec![answer, fills("shipped")]);
        let out = compile_with_provider(&revise(&bytes, &record, 0), &seat)
            .await
            .unwrap();
        assert!(
            seat.calls() >= 1,
            "HARNESS_INVALID: the seat answered the links"
        );
        let journal = format!("{out:?}");
        assert!(!journal.contains(SENTINEL), "{journal}");
        let rounds = out.provenance.decision.as_ref().unwrap().to_string();
        assert!(
            rounds.contains("revision notes"),
            "the notes are withheld, not dropped: {rounds}"
        );
    }
}

/// A room that records every candidate it is shown and declines every one of them before an
/// attempt: a coherent refusal, never an observation of the program.
struct Room {
    shown: Mutex<Vec<String>>,
}

impl nika_compile_cognition::rehearse::Rehearse for Room {
    fn bound(&self) -> Duration {
        Duration::from_secs(10)
    }
    fn rehearse<'a>(
        &'a self,
        candidate: &'a str,
        inputs: &'a [String],
    ) -> nika_compile_cognition::rehearse::RehearsalFuture<'a> {
        self.rehearse_reading(candidate, inputs, &[])
    }
    fn rehearse_reading<'a>(
        &'a self,
        candidate: &'a str,
        _inputs: &'a [String],
        _targets: &'a [String],
    ) -> nika_compile_cognition::rehearse::RehearsalFuture<'a> {
        use nika_compile_cognition::rehearse::{
            Attempt, EffectCounts, Observation, Refusal, Rehearsal, RehearsalReport,
        };
        Box::pin(async move {
            self.shown.lock().unwrap().push(candidate.to_owned());
            RehearsalReport::new(
                Rehearsal::NotRun {
                    reason: "declined before an attempt".into(),
                },
                Attempt::NeverAttempted,
                EffectCounts::none(),
                nika_compile::surface::sha256(candidate),
            )
            .with_observation(Observation::refused(Refusal::Effect))
        })
    }
}

/// The edit's rehearsal is of its revised bytes, once: the base is never shown to the room.
#[tokio::test]
async fn a_revision_is_rehearsed_on_its_revised_bytes_and_never_on_the_base() {
    let (_, bytes, record) = base().await;
    let seat = Semantic::new(vec![revised(link(PAID, SHIPPED)), fills("shipped")]);
    let room = Room {
        shown: Mutex::new(Vec::new()),
    };
    let cognition = nika_compile_cognition::Cognition {
        provider: Some(&seat),
        seat: None,
    };
    let request = revise(&bytes, &record, 1);
    let out =
        nika_compile_cognition::compile_with_cognition_rehearsed(&request, cognition, Some(&room))
            .await
            .unwrap();
    assert_eq!(out.status, CompileStatus::Ready, "{out:#?}");
    let revised = out.candidate.clone().unwrap();
    assert_ne!(revised, bytes);
    assert_eq!(room.shown.lock().unwrap().as_slice(), [revised]);
}

/// The clause an addition-bearing change adds beside its supersession (an independent filter).
const ADDED: &str = "Keep only the orders whose currency is EUR";

/// A change that supersedes one rule AND adds an independent one: the added clause is consumed
/// as the supersession is (the resolved request, the revised record's basis, the delta), never
/// only recorded beside a contract that lost it.
#[tokio::test]
async fn an_added_clause_is_consumed_with_the_supersession_never_only_recorded() {
    let (_, bytes, record) = base().await;
    let change = format!("{SHIPPED}. {ADDED}.");
    let mut both = fills("shipped");
    both["fills"][0]["value"] = json!(
        "fromjson | map(select(.total > 10)) | map(select(.status == \"shipped\")) | map(select(.currency == \"EUR\"))"
    );
    let mut stated = revised(link(PAID, SHIPPED));
    stated["adds"] = json!([ADDED]);
    let seat = Semantic::new(vec![stated, both]);
    let request = CompileRequest::edit(bytes.as_str(), change.as_str())
        .with_original_intent(INTENT)
        .with_plan(record.clone())
        .with_authoring_policy(policy(1));
    let out = compile_with_provider(&request, &seat).await.unwrap();
    assert_eq!(out.status, CompileStatus::Ready, "{out:#?}");
    let delta = &out.provenance.decision.as_ref().unwrap()["revision"];
    let added: Vec<&str> = (delta["added"].as_array().into_iter().flatten())
        .filter_map(|duty| duty["evidence"].as_str())
        .collect();
    assert!(added.contains(&ADDED), "HARNESS_INVALID reader: {delta:#}");
    let resolved = delta["resolved"].as_str().unwrap();
    assert!(
        resolved.contains(ADDED),
        "the added clause is consumed: {resolved}"
    );
    assert!(
        resolved.contains(SHIPPED) && !resolved.contains(PAID),
        "{resolved}"
    );
    let next = out.provenance.plan.as_ref().unwrap();
    let effective = next["basis"]["read"]["effective"].as_str().unwrap();
    assert!(
        effective.contains(ADDED),
        "the record binds it: {effective}"
    );
}

/// An added effect the base graph's structure would have to carry (a new destination) is
/// refused before any fill is asked: never an added duty a fill-only revision drops.
#[tokio::test]
async fn an_added_effect_is_refused_before_any_fill_and_never_ready() {
    let (_, bytes, record) = base().await;
    let change = format!("{SHIPPED}. Also write the kept orders to ./out/eur.json.");
    let mut stated = revised(link(PAID, SHIPPED));
    stated["adds"] = json!(["Also write the kept orders to ./out/eur.json"]);
    let seat = Semantic::new(vec![stated, fills("shipped")]);
    let request = CompileRequest::edit(bytes.as_str(), change.as_str())
        .with_original_intent(INTENT)
        .with_plan(record.clone())
        .with_authoring_policy(policy(1));
    let out = compile_with_provider(&request, &seat).await.unwrap();
    assert_ne!(out.status, CompileStatus::Ready, "{out:#?}");
    assert_eq!(seat.calls(), 1, "the links only, no fill: {out:#?}");
    assert!(
        (out.diagnostics.iter()).any(|d| d.message.contains("./out/eur.json")),
        "the addition is named: {out:#?}"
    );
}

/// A destination change of a record-bound semantic base is a structural edit no fill carries:
/// it is proven by the source laws on the very bytes the record binds, under the record's own
/// words, from the same typed answer — no fill call — and every other part of the base (the
/// other destination, the gate, the filter, the outputs) is kept byte for byte.
#[tokio::test]
async fn a_destination_change_of_a_semantic_base_is_proven_on_its_bound_bytes() {
    const COUNT: &str = "write the count to ./out/count.txt";
    const TOTAL: &str = "Write the count to ./out/total.txt";
    let (_, bytes, record) = base().await;
    let links = json!({"supersedes": [{"replaces": COUNT, "by": TOTAL}], "adds": [],
        "notes": "links"});
    let seat = Semantic::new(vec![links]);
    let request = CompileRequest::edit(bytes.as_str(), format!("{TOTAL}."))
        .with_original_intent(INTENT)
        .with_plan(record.clone())
        .with_authoring_policy(policy(1));
    let out = compile_with_provider(&request, &seat).await.unwrap();
    assert_eq!(out.status, CompileStatus::Ready, "{out:#?}");
    assert_eq!(
        out.candidate.as_deref(),
        Some(bytes.replace("./out/count.txt", "./out/total.txt").as_str()),
        "only the destination's slots changed"
    );
    // One links call and the judge; never a fill.
    assert_eq!(seat.calls(), 2, "{out:#?}");
    let asked = seat.asked.lock().unwrap().clone();
    assert!(
        asked
            .iter()
            .all(|schema| schema["properties"].get("fills").is_none())
    );
    let next = out.provenance.plan.as_ref().unwrap();
    assert_eq!(next["superseded"][0]["path"], "./out/count.txt");
    assert_eq!(next["superseded"][0]["by"], "./out/total.txt");
    assert_eq!(
        next["semantic_base_sha256"],
        nika_compile::surface::sha256(&record.to_string())
    );
}

/// A host observation before and after a successful write. No I/O or provider is used here.
fn written_world(version: u8) -> Value {
    let mut world = json!({"observed": [
        {"path": "./data/orders.json", "state": "observed", "kind": "json", "complete": true,
         "columns": ["status", "total"], "common_columns": ["status", "total"], "peek_sha256": "source"},
        {"path": "./out/paid.json", "state": "absent", "complete": false},
        {"path": "./out/count.txt", "state": "absent", "complete": false}],
        "kinds": {"./data/orders.json": {"sampled": 2, "keys": {"status": {"text": 2}, "total": {"number": 2}}}}});
    if version > 0 {
        world["observed"][1] = json!({"path": "./out/paid.json", "state": "observed", "kind": "json",
            "columns": ["status", "total"], "complete": true, "peek_sha256": format!("output-{version}")});
        // The host stops observing an existing unsupported .txt; old absence is still recoverable.
        world["observed"].as_array_mut().unwrap().pop();
    }
    world
}

#[tokio::test]
async fn a_written_destination_survives_migration_reopen_and_a_second_semantic_edit() {
    let (_, bytes, mut legacy) = base_observed(Some(written_world(0))).await;
    legacy["basis"].as_object_mut().unwrap().remove("world");
    // The next original includes a destination restatement, as an interactive EDIT does.
    let change = format!("{CHANGE} Garde le même fichier ./out/paid.json.");
    let mut links = revised(link(PAID, SHIPPED));
    links["adds"] = json!(["Garde le même fichier ./out/paid.json"]);
    let seat = Semantic::new(vec![links, fills("shipped")]);
    let request = CompileRequest::edit(&bytes, change)
        .with_original_intent(INTENT)
        .with_plan(legacy)
        .with_knowledge(written_world(1))
        .with_authoring_policy(policy(1));
    let b = compile_with_provider(&request, &seat).await.unwrap();
    assert_eq!(b.status, CompileStatus::Ready, "migration: {b:#?}");
    assert_eq!(doc(&b), expected(&serde_yaml_bw::from_str(&bytes).unwrap()));
    let b_record = b.provenance.plan.as_ref().unwrap();
    assert!(b_record["basis"]["world"]["value"]["observed"].is_array());
    let words = b_record["basis"]["read"]["effective"].as_str().unwrap();
    assert!(words.contains("Garde le même fichier ./out/paid.json"));
    assert!(nika_compile_reader::hot::stated_sources(words).contains(&"./out/paid.json".into()));
    // The exact opaque record survives close/reopen serialization, with no process-only cache.
    let reopened: Value = serde_json::from_str(&serde_json::to_string(b_record).unwrap()).unwrap();
    let next = CompileRequest::edit(b.candidate.as_deref().unwrap(), DELIVERED_CHANGE)
        .with_original_intent(INTENT)
        .with_plan(reopened.clone())
        .with_knowledge(written_world(2))
        .with_authoring_policy(policy(1));
    let seat = Semantic::new(vec![revised(link(SHIPPED, DELIVERED)), fills("delivered")]);
    let c = compile_with_provider(&next, &seat).await.unwrap();
    assert_eq!(c.status, CompileStatus::Ready, "second EDIT: {c:#?}");
    let mut want = doc(&b);
    want["tasks"]["keep"]["invoke"]["args"]["expression"] =
        json!("fromjson | map(select(.total > 10)) | map(select(.status == \"delivered\"))");
    assert_eq!(doc(&c), want, "only the requested filter changes");
    assert_eq!(
        next.knowledge,
        Some(written_world(2)),
        "current world is never replaced"
    );
    // The same recovery cannot hide a changed input, tampered record, or changed base bytes.
    for fault in ["source", "history", "bytes"] {
        let mut bad = next.clone();
        match fault {
            "source" => {
                bad.knowledge.as_mut().unwrap()["observed"][0]["peek_sha256"] = json!("changed");
            }
            "history" => {
                bad.plan.as_mut().unwrap()["basis"]["world"]["value"]["observed"][0]["state"] =
                    json!("forged");
            }
            _ => {
                bad = CompileRequest::edit(
                    format!("{}# changed\n", b.candidate.as_deref().unwrap()),
                    DELIVERED_CHANGE,
                )
                .with_original_intent(INTENT)
                .with_plan(reopened.clone())
                .with_knowledge(written_world(2))
                .with_authoring_policy(policy(1));
            }
        }
        let none = Semantic::new(vec![]);
        let out = compile_with_provider(&bad, &none).await.unwrap();
        assert_eq!(none.calls(), 0, "{fault}: {out:#?}");
        assert_ne!(out.status, CompileStatus::Ready, "{fault}: {out:#?}");
    }
}

/// The authoring calls an outcome's receipt names, in order.
fn roles(out: &CompileOutcome) -> Vec<String> {
    (out.provenance.authoring.iter())
        .flat_map(|receipt| receipt.context.iter())
        .filter_map(|call| call["call"].as_str().map(str::to_owned))
        .collect()
}

/// Two links of one original clause: their spans overlap in the original request (as a clause
/// nested in another's does), and the clause is linked twice.
fn overlapping() -> Value {
    revised(json!([{"replaces": PAID, "by": SHIPPED}, {"replaces": PAID, "by": SHIPPED}]))
}

/// Whether any attempt the seat saw asked for fills.
fn asked_fills(seat: &Semantic) -> bool {
    (seat.asked.lock().unwrap().iter()).any(|schema| schema["properties"].get("fills").is_some())
}

/// Links that break a law decidable before any fill are refused with every law named, then
/// stated again by the seat in the same talk, its opening (the base graph, the original request
/// and both clause lists) unchanged; the valid answer is filled exactly once and READY.
#[tokio::test]
async fn invalid_links_are_repaired_before_any_fill_and_the_valid_answer_fills_once() {
    let (_, bytes, record) = base().await;
    let answers = vec![
        overlapping(),
        revised(link(PAID, SHIPPED)),
        fills("shipped"),
    ];
    let seat = Semantic::new(answers);
    let out = compile_with_provider(&revise(&bytes, &record, 1), &seat)
        .await
        .unwrap();
    assert_eq!(out.status, CompileStatus::Ready, "{out:#?}");
    let base_doc: Value = serde_yaml_bw::from_str(&bytes).unwrap();
    assert_eq!(
        doc(&out),
        expected(&base_doc),
        "only the changed rule moved"
    );
    let roles = roles(&out);
    assert_eq!(
        &roles[..3],
        ["revision", "revision-repair", "fill"],
        "{roles:?}"
    );
    let filled = roles.iter().filter(|role| role.starts_with("fill")).count();
    assert_eq!(
        filled, 1,
        "one fill, never before the links hold: {roles:?}"
    );
    // The repair re-sends the same talk, then the refused answer and the laws it broke.
    let messages = seat.sent.lock().unwrap().clone();
    let (first, second) = (
        messages[0].as_array().unwrap(),
        messages[1].as_array().unwrap(),
    );
    assert_eq!(second.len(), first.len() + 2, "{second:#?}");
    assert_eq!(
        &second[..first.len()],
        first.as_slice(),
        "the opening is unchanged"
    );
    let repair = second.last().unwrap().to_string();
    for needle in [
        "overlap in the request it revises",
        "is linked twice",
        "State the links again",
    ] {
        assert!(repair.contains(needle), "{needle}: {repair}");
    }
    let decision = out.provenance.decision.as_ref().unwrap();
    let delta = &decision["revision"];
    assert_eq!(
        delta["superseded"].as_array().map(Vec::len),
        Some(1),
        "{delta:#}"
    );
    assert_eq!(delta["superseded"][0]["evidence"], PAID);
    assert_eq!(delta["original"], INTENT, "the original request is kept");
    let journal = decision.to_string();
    assert!(
        journal.contains("refused revision links"),
        "the refused round is journaled by digest: {journal}"
    );
}

/// Links still invalid when the allowance is spent, restated without progress, or followed by an
/// answer that is no links object end Incomplete: no fill is asked, no candidate or record is
/// kept, and a refusal names its laws. No allowance asks once.
#[tokio::test]
async fn invalid_links_that_stay_invalid_end_incomplete_with_no_fill() {
    let (_, bytes, record) = base().await;
    let valid = || revised(link(PAID, SHIPPED));
    let cases = [
        (
            "no allowance",
            0,
            vec![overlapping(), valid()],
            1,
            Some("overlap"),
        ),
        (
            "spent",
            1,
            vec![
                overlapping(),
                revised(link(PAID, "ship everything")),
                valid(),
            ],
            2,
            Some("not a clause the change states"),
        ),
        (
            "no progress",
            2,
            vec![overlapping(), overlapping(), valid()],
            2,
            Some("overlap"),
        ),
        (
            "not a links object",
            1,
            vec![overlapping(), json!("no links"), valid()],
            2,
            None,
        ),
    ];
    for (case, repairs, mut answers, calls, law) in cases {
        answers.push(fills("shipped"));
        let seat = Semantic::new(answers);
        let out = compile_with_provider(&revise(&bytes, &record, repairs), &seat)
            .await
            .unwrap();
        assert_eq!(seat.calls(), calls, "{case}: {out:#?}");
        assert!(!asked_fills(&seat), "{case}: no fill is asked: {out:#?}");
        assert_ne!(out.status, CompileStatus::Ready, "{case}: {out:#?}");
        assert!(
            out.candidate.is_none() && out.provenance.plan.is_none(),
            "{case}: {out:#?}"
        );
        if let Some(law) = law {
            let named =
                (out.diagnostics.iter()).any(|d| d.target == "revision" && d.message.contains(law));
            assert!(named, "{case}: {law}: {out:#?}");
        }
    }
}

/// The fill of `fills("shipped")` with the kept rule's two filters in the other order: the same
/// literals the laws admit, other bytes.
fn reordered() -> Value {
    let mut answer = fills("shipped");
    answer["fills"][0]["value"] =
        json!("fromjson | map(select(.status == \"shipped\")) | map(select(.total > 10))");
    answer
}

/// The questions a judgment that does not carry the revised request asks: the whole request, its
/// six parts each asked alone, and the task question of the replaced part (judged missing).
const JUDGED: usize = 1 + 6 + 1;

/// The replaced part of the revised request as the judge finds it: missing.
const REPLACED: &str =
    "Keep the orders whose status is shipped instead of paid and write them to ./out/paid.json";
/// The judge's reason for [`REPLACED`]: the task its task question names.
const POINTED: &str = "the judge points to the task keep";

/// What a held candidate offers, as the verifier states it.
const HELD: &str = "The candidate was judged and not accepted, with no defect a repair could start from: it is shown, never offered, and nothing was written. A correction of the request or another verifier can decide it.";

/// The finding a whole request the judge doubted leaves when no trial run decides it.
const CONTESTED: &str = "The judge did not accept the request as carried (unfaithful) and located no defect a repair could start from; the same judge asked again decides nothing (no trial run of these exact bytes exists in this compile). Nothing is READY on it. Next: a correction of the request, or another verifier.";

/// The finding the replaced part leaves once the judge found it missing and named no task
/// failing it: named apart from the whole request, by its own words.
const CONTESTED_PART: &str = "The judge found « Keep the orders whose status is shipped instead of paid and write them to ./out/paid.json » missing but then named no task that fails it and no operation it lacks: nothing decided it, and nothing is READY on it.";

/// The seat's answers of a revision whose first READY candidate the judge refuses: the links, a
/// first fill, the slots of the judge's [`JUDGED`] questions (answered by the double), the
/// refill, the judge again.
fn refused_then(first: Value, refill: Value) -> Vec<Value> {
    let judged = || vec![json!(null); JUDGED];
    let mut answers = vec![revised(link(PAID, SHIPPED)), first];
    answers.extend(judged());
    answers.push(refill);
    answers.extend(judged());
    answers
}

/// A READY revision the judge refuses is filled again from the part it names, in the same talk
/// with its graph kept and within the door's one round count: the repaired bytes are bound and
/// judged afresh, never under the refused candidate's verdict or delta. Every judgment reads the
/// request the revision resolves and the change as stated, never the superseded original words.
#[tokio::test]
async fn a_judged_defect_refills_the_revision_and_only_the_repaired_bytes_are_ready() {
    let (_, bytes, record) = base().await;
    let answers = refused_then(reordered(), fills("shipped"));
    let seat = Semantic::judging(answers, vec!["unfaithful"], usize::MAX);
    let out = compile_with_provider(&revise(&bytes, &record, 1), &seat)
        .await
        .unwrap();
    assert_eq!(out.status, CompileStatus::Ready, "{out:#?}");
    let base_doc: Value = serde_yaml_bw::from_str(&bytes).unwrap();
    assert_eq!(doc(&out), expected(&base_doc), "the repaired fill is kept");
    // The refused fill's judgment: the whole request, its parts alone (the replaced part, second,
    // with the task question it needs), then the refill and its own judgment.
    let mut want = vec!["revision", "fill", "judge_request"];
    want.extend(["judge_part", "judge_part", "judge_point"]);
    want.extend(["judge_part"; 4]);
    want.extend(["fill", "judge_request"]);
    assert_eq!(roles(&out), want, "{out:#?}");
    // Every question of that judgment is told the candidate revises an earlier workflow.
    for at in 2..2 + JUDGED {
        let asked = seat.sent.lock().unwrap()[at].to_string();
        assert!(
            asked.contains("REVISES an earlier workflow"),
            "{at}: {asked}"
        );
    }
    // The refill continues the talk: the judge's defect with its reason, and the holes, in one
    // user turn.
    let refill = seat.sent.lock().unwrap()[2 + JUDGED].clone();
    let turn = refill.as_array().unwrap().last().unwrap().to_string();
    let defect = format!(
        "[semantic_verification] the judge compared the whole request with the candidate's bytes: it does not carry « {REPLACED} » · the judge's reason: {POINTED}"
    );
    for needle in [
        defect.as_str(),
        "refused by the judge",
        "Fill exactly these holes",
    ] {
        assert!(turn.contains(needle), "{needle}: {turn}");
    }
    // Each judgment read its own candidate's bytes and the revised request, the earlier request
    // kept apart as history beside the change, and was told which one it judges.
    let states = seat.states.lock().unwrap().clone();
    let candidate = out.candidate.as_deref().unwrap();
    assert_eq!(states.len(), 2, "{states:#?}");
    assert_ne!(states[0]["candidate_nika"], candidate, "{states:#?}");
    assert_eq!(states[1]["candidate_nika"], candidate, "{states:#?}");
    for state in &states {
        assert!(state["original_request"].is_null(), "{state:#}");
        assert_eq!(state["revision"]["change"], CHANGE, "{state:#}");
        assert_eq!(state["revision"]["base_request"], INTENT, "{state:#}");
        let asked = state["request"].as_str().unwrap();
        assert!(
            asked.contains(SHIPPED) && !asked.contains(PAID),
            "{state:#}"
        );
        assert!(
            asked.contains(TOTAL),
            "an unchanged clause is still asked: {state:#}"
        );
    }
    let judged = seat.sent.lock().unwrap()[2].to_string();
    assert!(judged.contains("REVISES an earlier workflow"), "{judged}");
    // The record binds the repaired bytes; both verifications stay in the decision, numbered.
    let next = out.provenance.plan.as_ref().expect("revised record");
    let bound = nika_compile::surface::sha256(candidate);
    assert_eq!(next["final"]["candidate_sha256"], bound, "{next:#}");
    let decision = out.provenance.decision.as_ref().unwrap();
    let attempts: Vec<(u64, usize)> = (decision["semantic_verification"].as_array().unwrap())
        .iter()
        .map(|a| {
            (
                a["attempt"].as_u64().unwrap(),
                a["defects"].as_array().unwrap().len(),
            )
        })
        .collect();
    assert_eq!(attempts, [(0, 1), (1, 0)], "{decision:#}");
    let refused = &decision["semantic_verification"][0];
    assert_eq!(refused["defects"], json!([REPLACED]), "{refused:#}");
    let noted = json!([{"defect": REPLACED, "note": POINTED}]);
    assert_eq!(refused["notes"], noted, "{refused:#}");
    let pointed = (refused["questions"].as_array().into_iter().flatten())
        .find(|question| question["role"] == "judge_point")
        .cloned()
        .unwrap_or_default();
    assert_eq!(pointed["choice"], "task-keep", "{refused:#}");
    assert_eq!(pointed["clause"]["text"], REPLACED, "{refused:#}");
    let approved = &decision["semantic_verification"][1];
    assert_eq!(approved["settled_by"], "verify-request", "{approved:#}");
    assert_eq!(decision["revision"]["superseded"][0]["evidence"], PAID);
    let route = decision["route"].to_string();
    assert!(route.contains("verify: repair 1"), "{route}");
}

/// A judged defect the refill does not settle ends the revision, never in a loop: the same part
/// named again is no progress under no repair count, a refused refill call ends the talk, and a
/// spent count withdraws the refused bytes. Nothing is READY, no candidate, record or delta of a
/// refused candidate is kept (the host keeps its saved base), and the finding names the repairs
/// actually made.
#[tokio::test]
async fn a_judged_defect_that_does_not_settle_ends_with_its_repairs_named() {
    let (_, bytes, record) = base().await;
    let unbounded = policy(1).with_unbounded_repairs();
    // The links and the first fill, each refused judgment's questions, the refill between them.
    let cases = [
        (
            "no progress",
            Some(unbounded),
            2,
            usize::MAX,
            2 + JUDGED + 1 + JUDGED,
            Some("1 repair(s)"),
        ),
        ("refused refill", None, 1, 2 + JUDGED, 2 + JUDGED + 1, None),
        (
            "spent",
            Some(policy(0)),
            1,
            usize::MAX,
            2 + JUDGED,
            Some("0 repair(s)"),
        ),
    ];
    for (case, under, refusals, ceiling, calls, repairs) in cases {
        let answers = refused_then(fills("shipped"), reordered());
        let seat = Semantic::judging(answers, vec!["unfaithful"; refusals], ceiling);
        let mut request = revise(&bytes, &record, 1);
        if let Some(policy) = under {
            request = request.with_authoring_policy(policy);
        }
        let out = compile_with_provider(&request, &seat).await.unwrap();
        assert_eq!(seat.calls(), calls, "{case}: {out:#?}");
        assert_ne!(out.status, CompileStatus::Ready, "{case}: {out:#?}");
        assert!(
            out.candidate.is_none() && out.provenance.plan.is_none(),
            "{case}: {out:#?}"
        );
        let decision = out.provenance.decision.as_ref().unwrap();
        assert!(decision.get("revision").is_none(), "{case}: {decision:#}");
        let route = decision["route"].to_string();
        let stalled = route.contains("native: no progress");
        assert_eq!(stalled, case == "no progress", "{case}: {route}");
        if let Some(repairs) = repairs {
            let named = (out.diagnostics.iter())
                .any(|d| d.target == "semantic_verification" && d.message.contains(repairs));
            assert!(named, "{case}: {repairs}: {out:#?}");
        }
    }
}

/// A part the judge finds missing in a revision but names no task failing stays contested (R6):
/// no defect, so the fills are never reopened though a round is left (the refill above shows one
/// would be under this policy), and nothing is READY. The judge did not carry the revised
/// request and no trial run decides it: the revised bytes are held, shown as the preview and
/// never offered, with no record a later round could replay to the same judge and no delta (the
/// host keeps its saved base).
#[tokio::test]
async fn a_revision_restriction_no_task_violates_is_contested_and_never_refilled() {
    let (_, bytes, record) = base().await;
    let answers = vec![revised(link(PAID, SHIPPED)), fills("shipped")];
    let seat = Semantic::judging(answers, vec!["unfaithful"], usize::MAX).pointing("no_task");
    let out = compile_with_provider(&revise(&bytes, &record, 1), &seat)
        .await
        .unwrap();
    assert_eq!(out.status, CompileStatus::Incomplete, "{out:#?}");
    let judged = seat.states.lock().unwrap()[0].clone();
    assert_eq!(
        out.candidate.as_deref(),
        judged["candidate_nika"].as_str(),
        "the judged bytes are the preview: {out:#?}"
    );
    assert!(out.check_preview.is_some(), "{out:#?}");
    assert!(out.provenance.plan.is_none(), "{out:#?}");
    assert!(out.questions.is_empty() && out.requested_boundary.is_none());
    let held: Vec<(&DiagnosticKind, &str)> = (out.diagnostics.iter())
        .filter(|d| d.target == "verify_held")
        .map(|d| (&d.kind, d.message.as_str()))
        .collect();
    assert_eq!(held, [(&DiagnosticKind::Applied, HELD)], "{out:#?}");
    // One judgment, its extra question asked since no defect was located; no second fill.
    let mut want = vec!["revision", "fill", "judge_request"];
    want.extend(["judge_part", "judge_part", "judge_point"]);
    want.extend(["judge_part"; 4]);
    want.push("judge_extra");
    assert_eq!(roles(&out), want, "{out:#?}");
    assert_eq!(seat.calls(), JUDGED + 3);
    let decision = out.provenance.decision.as_ref().unwrap();
    assert!(decision.get("revision").is_none(), "{decision:#}");
    let verified = &decision["semantic_verification"];
    assert_eq!(verified.as_array().map(Vec::len), Some(1), "{verified:#}");
    let resolved = judged["request"].clone();
    let contested = json!([REPLACED, resolved]);
    assert_eq!(verified[0]["contested"], contested, "{verified:#}");
    assert_eq!(verified[0]["defects"], json!([]), "{verified:#}");
    assert_eq!(verified[0]["unknown"], json!([]), "{verified:#}");
    assert_eq!(verified[0]["doubt"], json!(["unfaithful"]), "{verified:#}");
    let unsettled = json!(["no trial run of these exact bytes exists in this compile"]);
    assert_eq!(verified[0]["unsettled"], unsettled, "{verified:#}");
    let route = decision["route"].to_string();
    assert!(
        route.contains("verify: not ready, candidate held"),
        "{route}"
    );
    assert!(!route.contains("verify: repair"), "{route}");
    // The disagreement is named, never a defect: the part by its own words, then the whole
    // request.
    let told: Vec<&str> = (out.diagnostics.iter())
        .filter(|d| d.target == "semantic_verification")
        .map(|d| d.message.as_str())
        .collect();
    assert_eq!(told, [CONTESTED_PART, CONTESTED], "{told:?}");
    assert!(
        told.iter().all(|m| !m.contains("does not carry")),
        "{told:?}"
    );
}
