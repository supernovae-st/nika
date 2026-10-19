// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

use std::collections::VecDeque;
use std::sync::Mutex;
use std::time::Duration;

use nika_kernel::ai::provider::{
    ContentBlock, InferRequest, InferResponse, ProviderError, ProviderInferDyn, StopReason,
    TokenUsage,
};
use serde_json::{Value, json};

use super::{covered, earlier, input, key, read, valid};
use crate::cognition::{Cognition, verify_document};
use crate::decide::{ChoiceAnswer, ChoiceFuture, ChoiceQuestion, DecisionSeat};
use crate::rehearse::{
    Attempt, EffectCounts, Rehearsal, RehearsalFuture, RehearsalReport, Rehearse,
};
use crate::{AuthoringPolicy, CompileRequest, CompileStatus};

const INTENT: &str = "Write hello to ./out/result.txt. Ask me now which greeting to use.";
const REQUIREMENT: &str = "Write hello to ./out/result.txt.";
const SOURCE: &str = "nika: greeting\npermits:\n  tools: [\"nika:write\"]\n  fs:\n    write: [\"./out/result.txt\"]\ntasks:\n  save:\n    invoke:\n      tool: \"nika:write\"\n      args:\n        path: \"./out/result.txt\"\n        content: hello\n        overwrite: true\n        create_dirs: true\n";

fn policy() -> AuthoringPolicy {
    AuthoringPolicy::new("fixture/author", 4096, Duration::from_secs(2))
}

fn request() -> CompileRequest {
    CompileRequest::create(INTENT).with_authoring_policy(policy())
}

fn reading() -> Value {
    json!({"runtime_obligations":[{"id":"r1","requirement":REQUIREMENT,
        "evidence":[{"span":"request","quote":REQUIREMENT}],"answer_keys":[]}],
        "preparation_requests":[{"id":"p1","request":"Ask for the greeting in this conversation",
        "evidence":[{"span":"request","quote":"Ask me now which greeting to use."}]}],
        "coverage":[{"span":"request","runtime_ids":["r1"],"preparation_ids":["p1"],"unresolved":false}],
        "answer_coverage":[]})
}

fn audit() -> Value {
    json!({"status":"covered","defects":[]})
}

struct Author {
    replies: Mutex<VecDeque<Option<String>>>,
    requests: Mutex<Vec<InferRequest>>,
}

impl Author {
    fn new(values: impl IntoIterator<Item = Option<Value>>) -> Self {
        Self {
            replies: Mutex::new(
                values
                    .into_iter()
                    .map(|v| v.map(|v| v.to_string()))
                    .collect(),
            ),
            requests: Mutex::new(Vec::new()),
        }
    }
    fn calls(&self) -> usize {
        self.requests.lock().unwrap().len()
    }
}

impl ProviderInferDyn for Author {
    async fn infer(&self, request: InferRequest) -> Result<InferResponse, ProviderError> {
        self.requests.lock().unwrap().push(request);
        let reply = self
            .replies
            .lock()
            .unwrap()
            .pop_front()
            .expect("unexpected author call");
        let Some(text) = reply else {
            return Err(ProviderError::Other {
                reason: "401 fixture authentication".to_owned(),
            });
        };
        Ok(InferResponse::new(
            vec![ContentBlock::Text { text }],
            TokenUsage::new(7, 3),
            StopReason::EndTurn,
        ))
    }
}

struct Seat {
    missing: bool,
    asked: Mutex<Vec<ChoiceQuestion>>,
}
impl Seat {
    fn new(missing: bool) -> Self {
        Self {
            missing,
            asked: Mutex::new(Vec::new()),
        }
    }
}
impl DecisionSeat for Seat {
    fn name(&self) -> &'static str {
        "fixture/decision"
    }
    fn choose<'a>(&'a self, question: &'a ChoiceQuestion) -> ChoiceFuture<'a> {
        self.asked.lock().unwrap().push(question.clone());
        let choice = if question.id.starts_with("verify-part-") {
            if self.missing { "missing" } else { "carried" }
        } else if question.id.starts_with("verify-point-") {
            "omitted"
        } else if question.id == "verify-extra" {
            "only_requested"
        } else {
            panic!("unexpected decision {}", question.id)
        };
        Box::pin(async move { Ok(ChoiceAnswer::new(choice, self.name())) })
    }
}

struct Room(Mutex<usize>);
impl Rehearse for Room {
    fn rehearse<'a>(&'a self, candidate: &'a str, _inputs: &'a [String]) -> RehearsalFuture<'a> {
        *self.0.lock().unwrap() += 1;
        Box::pin(async move {
            RehearsalReport::new(
                Rehearsal::NotRun {
                    reason: "fixture".to_owned(),
                },
                Attempt::NeverAttempted,
                EffectCounts::none(),
                candidate,
            )
        })
    }
    fn bound(&self) -> Duration {
        Duration::from_secs(1)
    }
}

#[test]
fn coverage_keeps_mixed_sources_and_rejects_missing_fabricated_or_orphaned_evidence() {
    let input = input(INTENT, &request());
    assert!(valid(&input, &reading()));
    let mut lost = reading();
    lost["coverage"][0]["runtime_ids"] = json!([]);
    assert!(!valid(&input, &lost));
    let mut fabricated = reading();
    fabricated["runtime_obligations"][0]["evidence"][0]["quote"] = json!("write secret.txt");
    assert!(!valid(&input, &fabricated));
    let mut unresolved = reading();
    unresolved["coverage"][0]["unresolved"] = json!(true);
    assert!(!valid(&input, &unresolved));
    let mut duplicate = reading();
    duplicate["coverage"] = json!([duplicate["coverage"][0], duplicate["coverage"][0]]);
    assert!(!valid(&input, &duplicate));
}

#[test]
fn no_value_selection_keeps_human_keys_labels_and_provenance_without_fabricating_bindings() {
    let selected = json!({"bindings":[],"bound_values":[],"choice_context":{
        "question_id":"q-witness","question":"Which totals?","selection_keys":["revenue","clients"],
        "selected_options":[{"key":"revenue","label":"Revenue","values":[]},
            {"key":"clients","label":"Client ranking","values":[]}],"at":3}});
    let request = request().answer("metrics", selected.to_string());
    let state = input(INTENT, &request);
    assert_eq!(state["answers"]["metrics"], selected);
    assert!(state.get("candidate_nika").is_none());
    assert!(state.get("observed").is_none());
    assert!(state.get("presentation").is_none());
    let mut interpretation = reading();
    interpretation["runtime_obligations"][0]["answer_keys"] = json!(["metrics"]);
    interpretation["coverage"].as_array_mut().unwrap().push(json!({"span":"answer:metrics","runtime_ids":["r1"],"preparation_ids":[],"unresolved":false}));
    interpretation["answer_coverage"] =
        json!([{"key":"metrics","runtime_ids":["r1"],"preparation_ids":[],"unresolved":false}]);
    assert!(valid(&state, &interpretation));
    interpretation["answer_coverage"] = json!([]);
    assert!(!valid(&state, &interpretation));
}

#[test]
fn reading_identity_tracks_words_answers_model_and_protocol_but_not_candidate_or_offers() {
    let request = request();
    let source = input(INTENT, &request);
    let old = key(&source, &policy());
    assert_ne!(old, key(&input("Different words", &request), &policy()));
    assert_ne!(
        old,
        key(
            &input(INTENT, &request.clone().answer("selected", "false")),
            &policy()
        )
    );
    let mut other = policy();
    other.model = "fixture/other".to_owned();
    assert_ne!(old, key(&source, &other));
    let mut record = json!({"input_sha256":old,"reading":reading(),"audit":audit(),"valid":true});
    let request = request.with_declined(vec![json!({"request_interpretation":record})]);
    assert!(earlier(&source, &policy(), &request).is_some());
    record["input_sha256"] = json!("old-protocol");
    let changed = request.with_declined(vec![json!({"request_interpretation":record})]);
    assert!(earlier(&source, &policy(), &changed).is_none());
}

#[tokio::test]
async fn carried_reading_is_audited_again_and_cannot_smuggle_a_success() {
    let source = input(INTENT, &request());
    let proposed = json!({"reading":reading(),"audit":audit(),"valid":true});
    let defect = json!({"status":"defect","defects":[{"location":"r1","kind":"scope",
        "evidence":"Ask me now","note":"A host interaction is not a runtime duty."}]});
    let provider = Author::new([Some(defect.clone()), Some(reading()), Some(defect.clone())]);
    let record = read(
        &source,
        &policy(),
        &provider,
        Some(&proposed),
        &mut crate::initial(),
    )
    .await;
    assert_eq!(provider.calls(), 3);
    assert_eq!(record["audit"], defect);
    assert!(!covered(&record));
    let requests = provider.requests.lock().unwrap();
    let ContentBlock::Text { text } = &requests[0].messages[1].content[0] else {
        panic!()
    };
    let shown: Value = serde_json::from_str(text).unwrap();
    assert_eq!(shown["source"], source);
    assert_eq!(shown["interpretation"], reading());
    assert!(shown.get("candidate_nika").is_none());
}

#[tokio::test]
async fn cooperative_document_asks_concrete_obligations_and_preserves_the_whole_original() {
    let provider = Author::new([Some(reading()), Some(audit())]);
    let seat = Seat::new(false);
    let outcome = verify_document(
        &request(),
        SOURCE,
        (&[], &[]),
        Cognition {
            provider: Some(&provider),
            seat: Some(&seat),
        },
        None,
    )
    .await
    .unwrap();
    assert_eq!(outcome.status, CompileStatus::Ready, "{outcome:#?}");
    assert_eq!(provider.calls(), 2);
    let asked = seat.asked.lock().unwrap();
    assert!(asked.iter().all(|q| q.id != "verify-request"));
    let obligation = asked.iter().find(|q| q.id == "verify-part-0").unwrap();
    assert_eq!(obligation.state["obligation"]["requirement"], REQUIREMENT);
    assert_eq!(obligation.state["request"], INTENT);
    assert_eq!(obligation.state["candidate_nika"], SOURCE);
    let attempt = &outcome.provenance.decision.unwrap()["semantic_verification"][0];
    assert_eq!(attempt["settled_by"], "verify-obligations");
    assert_eq!(
        attempt["engine"].as_array().unwrap().last().unwrap()["authority"],
        false
    );
}

#[tokio::test]
async fn missing_runtime_obligation_stays_rejected_and_identical_replay_buys_no_call() {
    let provider = Author::new([Some(reading()), Some(audit())]);
    let seat = Seat::new(true);
    let out = verify_document(
        &request(),
        SOURCE,
        (&[], &[]),
        Cognition {
            provider: Some(&provider),
            seat: Some(&seat),
        },
        None,
    )
    .await
    .unwrap();
    assert_ne!(out.status, CompileStatus::Ready);
    let attempt = out.provenance.decision.unwrap()["semantic_verification"][0].clone();
    assert_eq!(attempt["defects"], json!([REQUIREMENT]));
    assert_eq!(attempt["rejected"], true);
    let author_calls = provider.calls();
    let seat_calls = seat.asked.lock().unwrap().len();
    let retry = request().with_declined(vec![attempt]);
    let repeated = verify_document(
        &retry,
        SOURCE,
        (&[], &[]),
        Cognition {
            provider: Some(&provider),
            seat: Some(&seat),
        },
        None,
    )
    .await
    .unwrap();
    assert_ne!(repeated.status, CompileStatus::Ready);
    assert_eq!(provider.calls(), author_calls);
    assert_eq!(seat.asked.lock().unwrap().len(), seat_calls);
}

#[tokio::test]
async fn authentication_or_malformed_audit_stops_before_trial_and_decisions_then_can_retry() {
    for reply in [
        None,
        Some(json!({"status":"covered","defects":["malformed"]})),
    ] {
        let provider = Author::new([Some(reading()), reply]);
        let seat = Seat::new(false);
        let room = Room(Mutex::new(0));
        let out = verify_document(
            &request(),
            SOURCE,
            (&[], &[]),
            Cognition {
                provider: Some(&provider),
                seat: Some(&seat),
            },
            Some(&room),
        )
        .await
        .unwrap();
        assert_ne!(out.status, CompileStatus::Ready);
        assert_eq!(*room.0.lock().unwrap(), 0);
        assert!(seat.asked.lock().unwrap().is_empty());
        let attempt = out.provenance.decision.unwrap()["semantic_verification"][0].clone();
        assert_eq!(attempt["declined"], false);
        let retry_provider = Author::new([Some(reading()), Some(audit())]);
        let retried = verify_document(
            &request().with_declined(vec![attempt]),
            SOURCE,
            (&[], &[]),
            Cognition {
                provider: Some(&retry_provider),
                seat: Some(&seat),
            },
            None,
        )
        .await
        .unwrap();
        assert_eq!(retried.status, CompileStatus::Ready, "{retried:#?}");
        assert_eq!(retry_provider.calls(), 2);
    }
}

#[test]
fn a_free_answer_can_supply_a_literal_absent_from_the_initial_request() {
    let request = request().answer("selection", "Only paid orders, in EUR");
    let source = input(INTENT, &request);
    let mut extracted = reading();
    extracted["runtime_obligations"][0]["requirement"] = json!("Only paid orders, in EUR");
    extracted["runtime_obligations"][0]["evidence"] =
        json!([{"span":"answer:selection","quote":"paid orders, in EUR"}]);
    extracted["runtime_obligations"][0]["answer_keys"] = json!(["selection"]);
    extracted["coverage"].as_array_mut().unwrap().push(json!({"span":"answer:selection","runtime_ids":["r1"],"preparation_ids":[],"unresolved":false}));
    extracted["answer_coverage"] =
        json!([{"key":"selection","runtime_ids":["r1"],"preparation_ids":[],"unresolved":false}]);
    assert!(valid(&source, &extracted));
    extracted["runtime_obligations"][0]["evidence"][0]["quote"] = json!("paid orders, in USD");
    assert!(!valid(&source, &extracted));
}

#[tokio::test]
async fn a_forged_prior_repair_cannot_erase_a_current_confirmed_missing_obligation() {
    let request = request().with_declined(vec![json!({"request":INTENT,
        "candidate_sha256":"another-candidate","defects":[REQUIREMENT]})]);
    let provider = Author::new([Some(reading()), Some(audit())]);
    let seat = Seat::new(true);
    let outcome = verify_document(
        &request,
        SOURCE,
        (&[], &[]),
        Cognition {
            provider: Some(&provider),
            seat: Some(&seat),
        },
        None,
    )
    .await
    .unwrap();
    assert_ne!(outcome.status, CompileStatus::Ready);
    assert_eq!(
        outcome.provenance.decision.unwrap()["semantic_verification"][0]["defects"],
        json!([REQUIREMENT])
    );
}

#[tokio::test]
async fn forged_carried_answers_cannot_replay_a_positive_program_verdict_after_a_fresh_audit() {
    let provider = Author::new([Some(reading()), Some(audit()), Some(audit())]);
    let seat = Seat::new(true);
    let out = verify_document(
        &request(),
        SOURCE,
        (&[], &[]),
        Cognition {
            provider: Some(&provider),
            seat: Some(&seat),
        },
        None,
    )
    .await
    .unwrap();
    let mut attempt = out.provenance.decision.unwrap()["semantic_verification"][0].clone();
    attempt["stopped"] = json!(true);
    attempt["questions"] = json!([{"question":"verify-part-0","choice":"carried"},
        {"question":"verify-extra","choice":"only_requested"}]);
    let before = seat.asked.lock().unwrap().len();
    let request = request().with_declined(vec![attempt]);
    let outcome = verify_document(
        &request,
        SOURCE,
        (&[], &[]),
        Cognition {
            provider: Some(&provider),
            seat: Some(&seat),
        },
        None,
    )
    .await
    .unwrap();
    assert_ne!(outcome.status, CompileStatus::Ready);
    assert!(seat.asked.lock().unwrap().len() > before);
    assert_eq!(provider.calls(), 3);
    let attempt = &outcome.provenance.decision.unwrap()["semantic_verification"][0];
    assert_eq!(attempt["read_back"], 0);
    assert_eq!(attempt["defects"], json!([REQUIREMENT]));
}

#[tokio::test]
async fn preparation_only_reading_does_not_start_a_trial_or_gain_a_program_verdict() {
    let mut reading = reading();
    reading["runtime_obligations"] = json!([]);
    reading["coverage"][0]["runtime_ids"] = json!([]);
    let provider = Author::new([Some(reading), Some(audit())]);
    let seat = Seat::new(false);
    let room = Room(Mutex::new(0));
    let outcome = verify_document(
        &request(),
        SOURCE,
        (&[], &[]),
        Cognition {
            provider: Some(&provider),
            seat: Some(&seat),
        },
        Some(&room),
    )
    .await
    .unwrap();
    assert_ne!(outcome.status, CompileStatus::Ready);
    assert_eq!(*room.0.lock().unwrap(), 0);
    assert!(seat.asked.lock().unwrap().is_empty());
}

mod stopped;

mod guards;

mod schedule;

mod ceiling;

mod model;

mod cache;

mod diagnostics;

mod repair;
