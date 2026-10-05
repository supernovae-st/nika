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

use std::sync::Mutex;
use std::time::Duration;

use nika_compile::{AuthoringPolicy, CompileOutcome, CompileRequest, CompileStatus, NativeMode};
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
    /// The admission layer's call ceiling, as `semantic_verification`'s `Scripted::ceiling`:
    /// every call from this index is refused locally, counted as attempted, never answered.
    refused_from: usize,
}

impl Semantic {
    fn new(answers: Vec<Value>) -> Self {
        Self {
            answers: answers
                .into_iter()
                .map(|answer| answer.to_string())
                .collect(),
            asked: Mutex::new(Vec::new()),
            refused_from: usize::MAX,
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
            // A judge's closed choice: approve.
            Some(keys) if keys.iter().any(|k| k == "faithful") => r#"{"choice":"faithful"}"#.into(),
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
    let seat = Semantic::new(vec![graph("paid"), fills("paid")]);
    let request = CompileRequest::create(INTENT).with_authoring_policy(policy(1));
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
