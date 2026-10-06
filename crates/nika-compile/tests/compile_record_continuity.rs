// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>
//! A verified transform continues and replays, and a record keeps one identity whichever
//! reader wrote it (the E14 review of the saved-plan strictness, 2026-09-28, F7):
//! - a verified program carries its compute step's detail as its text: the seat's paraphrase
//!   (« people who share an email domain with at least one other person ») or a detail a stated
//!   rule joined with « ; ». It is anchored through the compute step it realizes, whose own
//!   evidence stays an excerpt of the request; a closed-grammar rule keeps the verbatim law.
//! - a pending or verified transform binds the plan's identity in its historical canonical form
//!   (a single clause's junction, which carries no meaning, is not part of it): the records of
//!   `fixtures/historical`, written by the fcf290a7b compiler, replay here as they replayed there,
//!   and a record written here carries the identity that compiler computes.
//! - a rule the seat states with the request's words, a line unwrapped or a double space
//!   dropped, is anchored as its evidence is; a changed letter (« inactive » for « active »)
//!   is never anchored, even two letters away.
#![allow(clippy::unwrap_used, clippy::expect_used)]
use nika_compile::{CompileOutcome, CompileRequest, CompileStatus, DiagnosticKind, compile};
use nika_compile_cognition::{
    Cognition, NoProvider, compile_with_cognition, compile_with_provider,
};
use nika_compile_reader::plan::Plan;
use serde_json::{Value, json};
use std::sync::atomic::Ordering;

mod common;
use common::{Judged, JudgedSeat, NoChoice, Rotating, keys, policy};

const SHARED: &str = "Read ./data/people.json (name, email), keep the people whose email domain appears more than once, and write them to ./out/shared.json";
const F7: &str = "Read ./data/people.json (name, email, status), keep only the rows whose status is active, keep the people whose email domain appears more than once, and write them to ./out/shared.json";

/// A record the fcf290a7b compiler wrote (E14's own bytes).
fn historical(name: &str) -> Value {
    let path = format!(
        "{}/tests/fixtures/historical/{name}.json",
        env!("CARGO_MANIFEST_DIR")
    );
    serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap()
}

/// The seat's plan for a request: a read, then one computation the typed stages cannot state,
/// whose detail is `detail` (the evidence stays the request's words).
fn seat_plan(columns: &str, detail: &str, constraints: &[&str]) -> Value {
    json!({"steps":[
        {"op":"read","detail":format!("./data/people.json ({columns})"),"evidence":format!("Read ./data/people.json ({columns})")},
        {"op":"compute","detail":detail,"evidence":"keep the people whose email domain appears more than once","computation":{"present":false}}],
      "effects":[{"verb":"write","target":"./out/shared.json","policy":"automatic","evidence":"write them to ./out/shared.json"}],
      "obligations":[],"constraints":constraints,"unknowns":[],
      "approval_bypass":{"present":false,"evidence":""}})
}

fn observed(columns: &[&str]) -> Value {
    json!({"observed":[{"path":"./data/people.json", "state":"observed",
        "columns": columns, "common_columns": columns,
        "complete":false, "peek_sha256":"e14-synthetic-observation"}]})
}

/// The program the seat first writes for the request's words (`email`, a field the source
/// does not have), then the one it regenerates once the human answered `address`.
fn email_program() -> Value {
    json!({"jq": "(.records | group_by(.email | split(\"@\")[1]) | map(select(length > 1)) | add // [])",
           "columns_read": ["email"],
           "example_input": [{"name":"a","email":"a@x.org"},{"name":"b","email":"b@x.org"},{"name":"c","email":"c@y.org"}],
           "expected_output": [{"name":"a","email":"a@x.org"},{"name":"b","email":"b@x.org"}]})
}
fn address_program() -> Value {
    json!({"jq":"(.records | group_by(.address | split(\"@\")[1]) | map(select(length > 1)) | add // [])",
        "columns_read":["address"],
        "example_input":[{"id":1,"address":"a@x.org"},{"id":2,"address":"b@x.org"},{"id":3,"address":"c@y.org"}],
        "expected_output":[{"id":1,"address":"a@x.org"},{"id":2,"address":"b@x.org"}]})
}

/// The first round: the seat's plan and its program over an unobserved field leave the
/// computation pending on the field choice.
async fn pending(intent: &str, plan: Value, columns: &[&str]) -> Value {
    let provider = Rotating::new(vec![plan.to_string(), email_program().to_string()]);
    let request = CompileRequest::create(intent)
        .with_authoring_policy(policy())
        .with_knowledge(observed(columns));
    let out = compile_with_provider(&request, &provider).await.unwrap();
    let record = out.provenance.plan.clone().unwrap();
    assert!(record.get("pending_transform").is_some(), "{out:#?}");
    record
}

/// The answer round: the field answered, one regeneration call, its first candidate judged by
/// the explicit approving double (R4 A11).
async fn continued(intent: &str, record: Value) -> (CompileOutcome, u32) {
    let provider = Rotating::new(vec![address_program().to_string()]);
    let request = CompileRequest::create(intent)
        .with_plan(record)
        .with_authoring_policy(policy())
        .answer("const.rule_field_1", "\"address\"");
    let judged = Judged::approving(&provider);
    let out = compile_with_provider(&request, &judged).await.unwrap();
    (out, provider.calls.load(Ordering::SeqCst))
}

/// A plain answer round under this round's judge, the explicit approving double over a seat
/// that settles no other choice (R4 A11).
async fn judged_replay(request: &CompileRequest) -> CompileOutcome {
    let judge = JudgedSeat::approving(&NoChoice);
    let cognition = Cognition::<NoProvider> {
        provider: None,
        seat: Some(&judge),
    };
    compile_with_cognition(request, cognition).await.unwrap()
}

/// The continuation findings that say the record no longer matches its request.
fn stale(out: &CompileOutcome) -> Vec<&str> {
    out.diagnostics
        .iter()
        .filter(|d| d.target == "pending_transform" || d.target == "recorded_plan")
        .map(|d| d.message.as_str())
        .collect()
}

fn regeneration(out: &CompileOutcome) -> Value {
    out.provenance
        .decision
        .as_ref()
        .and_then(|d| d.get("transform_regeneration").cloned())
        .unwrap_or(Value::Null)
}

/// What the fcf290a7b compiler answered for each record (E14 `old-consumes-old-r3`), in a round
/// that judges: the paraphrased computation is READY; the detail two computations share asks
/// for its expression (the assembler binds one computation), which is no staleness.
fn as_it_replayed_there(name: &str, out: &CompileOutcome) {
    assert!(stale(out).is_empty(), "{name}: {out:#?}");
    if name.starts_with("para") {
        assert_eq!(out.status, CompileStatus::Ready, "{name}: {out:#?}");
        let candidate = out.candidate.as_deref().unwrap();
        assert!(
            candidate.contains("group_by(.address"),
            "{name}: {candidate}"
        );
    } else {
        assert_eq!(out.status, CompileStatus::Incomplete, "{name}: {out:#?}");
        assert_eq!(keys(out), ["const.rule_expression"], "{name}: {out:#?}");
    }
}

/// The same records in a plain replay (Q2, R4 A11): readable and never stale, no grandfathered
/// READY. The seat's program is a clause no law reads from the bytes, so the paraphrased record
/// emits its program and names that remainder INCOMPLETE; the joined one asks as before.
fn as_q2_replays_it(name: &str, out: &CompileOutcome) {
    assert!(stale(out).is_empty(), "{name}: {out:#?}");
    assert_eq!(out.status, CompileStatus::Incomplete, "{name}: {out:#?}");
    if name.starts_with("para") {
        let candidate = out.candidate.as_deref().unwrap();
        assert!(
            candidate.contains("group_by(.address"),
            "{name}: {candidate}"
        );
        let open = &out.provenance.decision.as_ref().unwrap()["pending"]["open"];
        assert_eq!(open[0]["witness"], "unverified", "{name}: {open:#}");
    } else {
        assert_eq!(keys(out), ["const.rule_expression"], "{name}: {out:#?}");
    }
}

#[tokio::test]
async fn historical_verified_records_replay_at_zero_calls_as_they_did() {
    for name in ["para-verified", "f7-verified"] {
        let record = historical(name);
        let intent = record["verified_transform"]["intent"].as_str().unwrap();
        let request = CompileRequest::create(intent).with_plan(record.clone());
        let out = compile(&request).unwrap();
        as_q2_replays_it(name, &out);
        assert!(out.provenance.authoring.is_none(), "{name}: a paid replay");
        // A round that judges settles the remainder, and answers as the old compiler did.
        let judged = judged_replay(&request).await;
        as_it_replayed_there(name, &judged);
        assert!(
            judged.provenance.authoring.is_none(),
            "{name}: a paid judge"
        );
    }
}

/// E14 `old-answers-old-r3`: the historical record of two computations sharing one detail
/// reaches READY once the human states its expression, as it did on fcf290a7b, in a round whose
/// judge carries the model plan's whole request over the bytes it replays (R4 A11). The door
/// with no judge binds the same bytes, the answered expression in them, and holds them for it.
#[tokio::test]
async fn a_historical_joined_computation_is_ready_once_its_expression_is_answered() {
    let record = historical("f7-verified");
    let intent = record["verified_transform"]["intent"].as_str().unwrap();
    let expression = "[.records[] | select(.status == \"active\")] | group_by(.address | split(\"@\")[1]) | map(select(length > 1)) | add // []";
    let request = CompileRequest::create(intent)
        .with_plan(record.clone())
        .answer("const.rule_expression", json!(expression).to_string());
    let held = compile(&request).unwrap();
    assert!(stale(&held).is_empty(), "{held:#?}");
    common::assert_waits_for_its_judge(&held, intent);
    let out = judged_replay(&request).await;
    assert!(stale(&out).is_empty(), "{out:#?}");
    assert_eq!(out.status, CompileStatus::Ready, "{out:#?}");
    assert_eq!(out.candidate, held.candidate, "{out:#?}");
    assert!(out.candidate.as_deref().unwrap().contains(expression));
    assert!(out.provenance.authoring.is_none());
}

#[tokio::test]
async fn historical_pending_records_continue_after_their_field_answer() {
    for name in ["para-pending", "f7-pending"] {
        let record = historical(name);
        let intent = record["pending_transform"]["intent"].as_str().unwrap();
        let (out, calls) = continued(intent, record.clone()).await;
        assert_eq!(calls, 1, "{name}: one regeneration call");
        assert_eq!(regeneration(&out), json!({"accepted": true}), "{name}");
        let verified = out.provenance.plan.clone().unwrap();
        assert!(
            verified.get("verified_transform").is_some(),
            "{name}: {out:#?}"
        );
        as_it_replayed_there(name, &out);
        // The verified record it wrote replays at zero calls, the same bytes, its remainder
        // INCOMPLETE until a round judges it (Q2).
        let replay = compile(&CompileRequest::create(intent).with_plan(verified)).unwrap();
        as_q2_replays_it(name, &replay);
        assert!(
            replay.provenance.authoring.is_none(),
            "{name}: a paid replay"
        );
        assert_eq!(replay.candidate, out.candidate, "{name}");
    }
}

#[tokio::test]
async fn a_paraphrased_or_joined_detail_continues_after_its_field_answer() {
    let para = seat_plan(
        "name, email",
        "people who share an email domain with at least one other person",
        &[],
    );
    let joined = seat_plan(
        "name, email, status",
        "the people whose email domain appears more than once",
        &["keep only the rows whose status is active"],
    );
    for (name, intent, plan, columns) in [
        ("para", SHARED, para, &["id", "address"][..]),
        ("f7", F7, joined, &["id", "address", "status"][..]),
    ] {
        let record = pending(intent, plan, columns).await;
        let (out, calls) = continued(intent, record).await;
        assert_eq!(calls, 1, "{name}");
        assert_eq!(regeneration(&out), json!({"accepted": true}), "{name}");
        as_it_replayed_there(name, &out);
        // The success claim rides only the verified record the replay kept.
        assert!(claims_the_program(&out), "{name}: {out:#?}");
        let record = out.provenance.plan.as_ref().unwrap();
        assert!(record.get("verified_transform").is_some(), "{name}");
    }
}

/// Whether an outcome claims the regenerated program as applied.
fn claims_the_program(out: &CompileOutcome) -> bool {
    out.diagnostics
        .iter()
        .any(|d| d.kind == DiagnosticKind::Applied && d.target == "authoring_transform")
}

/// A verified program its own replay refuses is claimed by nothing: the call it spent stays
/// counted, the refusal stays stated with its reason, and neither an applied finding, a
/// candidate nor the verified record survives (E14 FRESH-1 saw « accepted: true » beside a
/// refused replay).
#[tokio::test]
async fn a_verified_program_its_replay_refuses_is_claimed_by_nothing() {
    // A blank computation detail: the pending record admits it, but the program regenerated
    // for it cannot carry it as its text, so the verified record's replay refuses it.
    let record = pending(
        SHARED,
        seat_plan("name, email", " ", &[]),
        &["id", "address"],
    )
    .await;
    let (out, calls) = continued(SHARED, record).await;
    assert_eq!(calls, 1, "the spent call");
    assert_eq!(
        out.provenance.authoring.as_ref().unwrap().calls,
        1,
        "{out:#?}"
    );
    let decision = regeneration(&out);
    assert_eq!(decision["accepted"], false, "{out:#?}");
    assert!(
        decision["why"].as_str().is_some_and(|why| !why.is_empty()),
        "{decision}"
    );
    assert!(
        !stale(&out).is_empty(),
        "the replay's refusal is stated: {out:#?}"
    );
    assert!(!claims_the_program(&out), "{out:#?}");
    assert!(out.candidate.is_none(), "{out:#?}");
    assert_ne!(out.status, CompileStatus::Ready, "{out:#?}");
    assert!(
        out.provenance
            .plan
            .as_ref()
            .is_none_or(|record| record.get("verified_transform").is_none()),
        "{out:#?}"
    );
}

/// The record written here is the record the fcf290a7b compiler wrote for the same request,
/// byte for byte once a single clause's explicit junction is set aside, and its identity is
/// that compiler's: a rollback reads it.
#[tokio::test]
async fn a_record_written_here_carries_the_historical_identity() {
    let plan = seat_plan(
        "name, email, status",
        "the people whose email domain appears more than once",
        &["keep only the rows whose status is active"],
    );
    let mut written = pending(F7, plan, &["id", "address", "status"]).await;
    let old = historical("f7-pending");
    assert_eq!(
        written["pending_transform"]["plan_sha256"], old["pending_transform"]["plan_sha256"],
        "the identity the historical compiler computes"
    );
    for rule in written["rules"].as_array_mut().unwrap() {
        if rule["clauses"].as_array().is_some_and(|c| c.len() == 1) {
            rule.as_object_mut().unwrap().remove("junction");
        }
    }
    assert_eq!(written, old);
}

/// A program is anchored only through a compute step of the same plan that it realizes and
/// whose evidence is an excerpt; a closed-grammar rule keeps the verbatim law.
#[test]
fn a_rule_is_anchored_by_its_words_or_by_the_step_it_realizes() {
    let record = historical("para-verified");
    let intent = record["verified_transform"]["intent"].as_str().unwrap();
    assert!(Plan::from_json(&record).unwrap().anchored(intent));
    // A program realizing no step of the plan: its text is the detail of none.
    let mut orphan = record.clone();
    orphan["rules"][0]["text"] = json!("people who share an email domain with nobody");
    assert!(!Plan::from_json(&orphan).unwrap().anchored(intent));
    // A program realizing a step whose own evidence is not an excerpt of the request.
    let mut unanchored = record.clone();
    unanchored["operations"][1]["evidence"] = json!("keep the people who share a domain");
    assert!(!Plan::from_json(&unanchored).unwrap().anchored(intent));
    // A closed-grammar rule whose words are not the request's, even as a step's detail.
    let mut closed = historical("f7-verified");
    let f7 = closed["verified_transform"]["intent"]
        .as_str()
        .unwrap()
        .to_owned();
    assert!(Plan::from_json(&closed).unwrap().anchored(&f7));
    closed["rules"][0]["text"] = json!("keep only the rows whose status is inactive");
    assert!(!Plan::from_json(&closed).unwrap().anchored(&f7));
}

/// A seat's plan that states the request's filter as a constraint, in its own spacing.
fn filter_plan(constraint: &str) -> Value {
    json!({"steps":[
        {"op":"read","detail":"./data/people.json (name, email, status)","evidence":"Read ./data/people.json (name, email, status)"}],
      "effects":[{"verb":"write","target":"./out/active.json","policy":"automatic","evidence":"write them to ./out/active.json"}],
      "obligations":[],"constraints":[constraint],"unknowns":[],
      "approval_bypass":{"present":false,"evidence":""}})
}

async fn seat_compile(intent: &str, constraint: &str) -> CompileOutcome {
    let provider = Rotating::new(vec![filter_plan(constraint).to_string()]);
    let request = CompileRequest::create(intent)
        .with_authoring_policy(policy())
        .with_knowledge(observed(&["name", "email", "status"]));
    // Judged by the explicit approving double (R4 A11): these tests read the emitted workflow.
    compile_with_provider(&request, &Judged::approving(&provider))
        .await
        .unwrap()
}

/// E14 `seat-*-r2`: the fcf290a7b compiler compiled these READY; the rule anchoring made the
/// first seat compile infeasible whenever the seat folded the request's whitespace.
#[tokio::test]
async fn a_rule_in_the_requests_words_is_anchored_whatever_their_spacing() {
    let folded = "keep only the rows whose status is active";
    // The remainder a plain replay leaves with no judge, when the plan does not close every duty
    // it states: a line break the reader reads as a boundary leaves a fragment across two named
    // elements, a remainder a round judges (Q2). Either way the model plan's whole request waits
    // for that round's judge (R4 A11): no replay with no judge is READY.
    let fragment = "email, status), keep only the rows whose status";
    for (intent, remainder) in [
        (
            "Read ./data/people.json (name, email, status), keep only the rows whose status is active, and write them to ./out/active.json",
            None,
        ),
        (
            "Read ./data/people.json (name, email, status), keep only the rows whose status  is active, and write them to ./out/active.json",
            None,
        ),
        (
            "Read ./data/people.json (name, email, status), keep only the rows whose status\nis active, and write them to ./out/active.json",
            Some(fragment),
        ),
    ] {
        let out = seat_compile(intent, folded).await;
        assert_eq!(out.status, CompileStatus::Ready, "{intent:?}: {out:#?}");
        let candidate = out.candidate.as_deref().unwrap();
        assert!(
            candidate.contains("select(.status == \"active\")"),
            "{candidate}"
        );
        // Its record replays at zero calls to the same candidate: with its duties closed, only
        // the whole request waits for the round's judge; else the remainder waits beside it.
        let record = out.provenance.plan.clone().unwrap();
        let request = CompileRequest::create(intent).with_plan(record);
        let replay = compile(&request).unwrap();
        assert_eq!(replay.candidate, out.candidate, "{intent:?}");
        if let Some(clause) = remainder {
            assert_eq!(replay.status, CompileStatus::Incomplete, "{intent:?}");
            let open = &replay.provenance.decision.as_ref().unwrap()["pending"]["open"];
            let at = intent.find(clause).unwrap();
            let expected = json!([
                {"clause": clause, "witness": null, "spans": [[at, at + clause.len()]]},
                {"clause": intent, "witness": null, "spans": [[0, intent.len()]]},
            ]);
            assert_eq!(open, &expected, "{intent:?}: {open:#}");
            let bytes = replay.candidate.as_deref().unwrap();
            let told: Vec<&str> = (replay.diagnostics.iter())
                .filter(|d| d.target == "semantic_verification")
                .map(|d| d.message.as_str())
                .collect();
            let named = [
                common::pending_finding(clause, common::NAMED_BY_NO_ELEMENT, bytes),
                common::pending_finding(intent, common::NAMED_BY_NO_ELEMENT, bytes),
            ];
            assert_eq!(told, named, "{intent:?}: {replay:#?}");
        } else {
            common::assert_waits_for_its_judge(&replay, intent);
        }
        let judged = judged_replay(&request).await;
        assert_eq!(
            judged.status,
            CompileStatus::Ready,
            "{intent:?}: {judged:#?}"
        );
        assert_eq!(judged.candidate, out.candidate, "{intent:?}");
    }
}

/// The safeguard the anchoring exists for: a letter changed inverts the filter, and no
/// candidate carries it, whether the seat or a record states it.
#[tokio::test]
async fn a_rule_with_a_changed_letter_is_never_anchored() {
    let intent = "Read ./data/people.json (name, email, status), keep only the rows whose status  is active, and write them to ./out/active.json";
    let out = seat_compile(intent, "keep only the rows whose status is inactive").await;
    assert_ne!(out.status, CompileStatus::Ready, "{out:#?}");
    let candidate = out.candidate.unwrap_or_default();
    assert!(!candidate.contains("inactive"), "{candidate}");
    let mut record = seat_compile(intent, "keep only the rows whose status is active")
        .await
        .provenance
        .plan
        .unwrap();
    let inverted = "keep only the rows whose status is inactive";
    for rule in record["rules"].as_array_mut().unwrap() {
        rule["text"] = json!(inverted);
    }
    assert!(!Plan::from_json(&record).unwrap().anchored(intent));
}
