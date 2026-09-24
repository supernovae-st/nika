// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>
//! A computation the typed stages cannot state asks the seat for a program the compiler
//! verifies before it binds it (treatment B of the mandate's A/B/C comparison): the seat's
//! own example is the test, the runtime's jq the judge, and a human is never asked for a jq
//! expression. Measured: 15/60 sealed seeds asked `const.rule_expression` on lane10.
#![allow(clippy::unwrap_used, clippy::expect_used)]
use nika_compile::{CompileRequest, CompileStatus, compile};
use nika_compile_cognition::compile_with_provider;
use serde_json::{Value, json};

mod common;
use common::{Rotating, keys, policy};

const SHARED: &str = "Read ./data/people.json (name, email), keep the people whose email domain appears more than once, and write them to ./out/shared.json";

fn plan() -> Value {
    json!({"steps":[
        {"op":"read","detail":"./data/people.json (name, email)","evidence":"Read ./data/people.json (name, email)"},
        {"op":"compute","detail":"the people whose email domain appears more than once","evidence":"keep the people whose email domain appears more than once","computation":{"present":false}}],
      "effects":[{"verb":"write","target":"./out/shared.json","policy":"automatic","evidence":"write them to ./out/shared.json"}],
      "obligations":[],"constraints":[],"unknowns":[],
      "regions":[{"text":"Read ./data/people.json (name, email),","role":"operation"},
                 {"text":"keep the people whose email domain appears more than once,","role":"operation"},
                 {"text":"and write them to ./out/shared.json","role":"effect"}],
      "approval_bypass":{"present":false,"evidence":""}})
}

const PROGRAM: &str =
    "(.records | group_by(.email | split(\"@\")[1]) | map(select(length > 1)) | add // [])";

fn transform(jq: &str, expected: &Value, columns: &[&str]) -> Value {
    json!({"jq": jq, "columns_read": columns,
           "example_input": [{"name":"a","email":"a@x.org"},{"name":"b","email":"b@x.org"},{"name":"c","email":"c@y.org"}],
           "expected_output": expected})
}

fn shared_two() -> Value {
    json!([{"name":"a","email":"a@x.org"},{"name":"b","email":"b@x.org"}])
}

fn transforms(out: &nika_compile::CompileOutcome) -> Value {
    out.provenance.decision.as_ref().unwrap()["transforms"].clone()
}

#[tokio::test]
async fn a_verified_program_runs_as_the_compute_task_and_replays_with_zero_calls() {
    let provider = Rotating::new(vec![
        plan().to_string(),
        transform(PROGRAM, &shared_two(), &["email"]).to_string(),
    ]);
    let req = CompileRequest::create(SHARED).with_authoring_policy(policy());
    let out = compile_with_provider(&req, &provider).await.unwrap();
    assert_eq!(out.status, CompileStatus::Ready, "{out:#?}");
    assert!(keys(&out).is_empty(), "{out:#?}");
    let candidate = out.candidate.as_deref().expect("a candidate");
    assert!(candidate.contains("group_by(.email"), "{candidate}");
    assert!(!candidate.contains("rule_expression"), "{candidate}");
    assert!(candidate.contains("compute_guard"), "{candidate}");
    assert_eq!(
        out.provenance.authoring.as_ref().unwrap().calls,
        2,
        "{out:#?}"
    );
    assert_eq!(transforms(&out)[0]["accepted"], true, "{out:#?}");
    let record = out.provenance.plan.clone().unwrap();
    assert_eq!(record["rules"][0]["program"]["jq"], PROGRAM, "{record:#}");
    let replayed = compile(&CompileRequest::create(SHARED).with_plan(record)).unwrap();
    assert_eq!(replayed.status, CompileStatus::Ready, "{replayed:#?}");
    assert_eq!(replayed.candidate, out.candidate);
    assert!(replayed.provenance.authoring.is_none());
}

#[tokio::test]
async fn a_program_the_seat_cannot_predict_is_refused_and_the_question_stays() {
    // The seat expects an output its own program does not return on its own example.
    let lying = transform(
        PROGRAM,
        &json!([{"name":"c","email":"c@y.org"}]),
        &["email"],
    );
    let provider = Rotating::new(vec![plan().to_string(), lying.to_string()]);
    let req = CompileRequest::create(SHARED).with_authoring_policy(policy());
    let out = compile_with_provider(&req, &provider).await.unwrap();
    assert_ne!(out.status, CompileStatus::Ready, "{out:#?}");
    assert!(keys(&out).contains(&"const.rule_expression"), "{out:#?}");
    let verdict = transforms(&out)[0].clone();
    assert_eq!(verdict["accepted"], false, "{verdict:#}");
    assert!(
        verdict["why"]
            .as_str()
            .unwrap()
            .contains("does not return the output"),
        "{verdict:#}"
    );
}

#[tokio::test]
async fn a_program_that_reads_an_undeclared_column_or_an_invented_literal_is_refused() {
    let undeclared = transform(PROGRAM, &shared_two(), &["name"]);
    let provider = Rotating::new(vec![plan().to_string(), undeclared.to_string()]);
    let req = CompileRequest::create(SHARED).with_authoring_policy(policy());
    let out = compile_with_provider(&req, &provider).await.unwrap();
    assert!(
        transforms(&out)[0]["why"]
            .as_str()
            .unwrap()
            .contains("a column it did not declare"),
        "{out:#?}"
    );
    let invented = transform(
        "[.records[] | select(.email | endswith(\"corp.com\"))]",
        &json!([]),
        &["email"],
    );
    let provider = Rotating::new(vec![plan().to_string(), invented.to_string()]);
    let out = compile_with_provider(&req, &provider).await.unwrap();
    assert!(
        transforms(&out)[0]["why"]
            .as_str()
            .unwrap()
            .contains("`corp.com` is not in the request"),
        "{out:#?}"
    );
}

#[tokio::test]
async fn observed_keys_override_a_programs_self_consistent_example() {
    let provider = Rotating::new(vec![
        plan().to_string(),
        transform(PROGRAM, &shared_two(), &["email"]).to_string(),
    ]);
    let req = CompileRequest::create(SHARED)
        .with_authoring_policy(policy())
        .with_knowledge(json!({"observed":[{"path":"./data/people.json",
            "state":"observed", "columns":["id", "address"],
            "common_columns":["id", "address"], "complete":false}]}));
    let out = compile_with_provider(&req, &provider).await.unwrap();
    assert_ne!(out.status, CompileStatus::Ready, "{out:#?}");
    assert_eq!(transforms(&out)[0]["accepted"], false);
    assert!(
        transforms(&out)[0]["why"]
            .as_str()
            .unwrap()
            .contains("observed fields")
    );
    let question = out
        .questions
        .iter()
        .find(|q| q.key == "const.rule_field_1")
        .unwrap();
    assert_eq!(question.answer_type, nika_compile::QuestionType::Choice);
    assert_eq!(
        question
            .options
            .iter()
            .map(|o| o.key.as_str())
            .collect::<Vec<_>>(),
        ["id", "address"]
    );
}

#[tokio::test]
async fn the_existing_verified_transform_keeps_working_with_matching_observations() {
    let provider = Rotating::new(vec![
        plan().to_string(),
        transform(PROGRAM, &shared_two(), &["email"]).to_string(),
    ]);
    let req = CompileRequest::create(SHARED)
        .with_authoring_policy(policy())
        .with_knowledge(json!({"observed":[{"path":"./data/people.json",
            "state":"observed", "columns":["name", "email"],
            "common_columns":["name", "email"], "complete":false}]}));
    let out = compile_with_provider(&req, &provider).await.unwrap();
    assert_eq!(out.status, CompileStatus::Ready, "{out:#?}");
    assert_eq!(transforms(&out)[0]["accepted"], true);
}

/// Deterministic replay records an answer but never calls a provider or asks for jq.
#[tokio::test]
async fn an_observed_field_answer_cannot_regenerate_a_rejected_program_on_plan_replay() {
    let provider = Rotating::new(vec![
        plan().to_string(),
        transform(PROGRAM, &shared_two(), &["email"]).to_string(),
    ]);
    let req = CompileRequest::create(SHARED)
        .with_authoring_policy(policy())
        .with_knowledge(json!({"observed":[{"path":"./data/people.json",
            "state":"observed", "columns":["id", "address"],
            "common_columns":["id", "address"], "complete":false}]}));
    let first = compile_with_provider(&req, &provider).await.unwrap();
    assert_ne!(first.status, CompileStatus::Ready, "{first:#?}");
    assert!(keys(&first).contains(&"const.rule_field_1"));
    let answer = CompileRequest::create(SHARED)
        .with_plan(first.provenance.plan.unwrap())
        .answer("const.rule_field_1", "\"address\"");
    let replayed = compile(&answer).unwrap();
    assert_ne!(replayed.status, CompileStatus::Ready, "{replayed:#?}");
    assert!(
        !keys(&replayed).contains(&"const.rule_expression"),
        "{replayed:#?}"
    );
    assert_eq!(
        replayed.provenance.plan.as_ref().unwrap()["pending_transform"]["fields"][0]["answer"],
        "address"
    );
    assert!(replayed.provenance.authoring.is_none());
}

fn observed_address() -> Value {
    json!({"observed":[{"path":"./data/people.json", "state":"observed",
        "columns":["id", "address"], "common_columns":["id", "address"],
        "complete":false, "peek_sha256":"synthetic-observation-a"}]})
}
async fn pending_fields() -> nika_compile::CompileOutcome {
    let provider = Rotating::new(vec![
        plan().to_string(),
        transform(PROGRAM, &shared_two(), &["email"]).to_string(),
    ]);
    let req = CompileRequest::create(SHARED)
        .with_authoring_policy(policy())
        .with_knowledge(observed_address());
    let out = compile_with_provider(&req, &provider).await.unwrap();
    assert_ne!(out.status, CompileStatus::Ready, "{out:#?}");
    assert!(
        out.provenance
            .plan
            .as_ref()
            .unwrap()
            .get("pending_transform")
            .is_some()
    );
    assert!(!keys(&out).contains(&"const.rule_expression"));
    out
}
fn addressed_program() -> Value {
    json!({"jq":"(.records | group_by(.address | split(\"@\")[1]) | map(select(length > 1)) | add // [])",
        "columns_read":["address"],
        "example_input":[{"id":1,"address":"a@x.org"},{"id":2,"address":"b@x.org"},{"id":3,"address":"c@y.org"}],
        "expected_output":[{"id":1,"address":"a@x.org"},{"id":2,"address":"b@x.org"}]})
}
fn answered(record: Value) -> CompileRequest {
    CompileRequest::create(SHARED)
        .with_plan(record)
        .with_authoring_policy(policy())
        .answer("const.rule_field_1", "\"address\"")
}
#[tokio::test]
async fn a_field_answer_regenerates_once_and_the_verified_program_replays_without_a_provider() {
    let first = pending_fields().await;
    let provider = Rotating::new(vec![addressed_program().to_string()]);
    let out = compile_with_provider(&answered(first.provenance.plan.unwrap()), &provider)
        .await
        .unwrap();
    assert_eq!(out.status, CompileStatus::Ready, "{out:#?}");
    assert_eq!(provider.calls.load(std::sync::atomic::Ordering::SeqCst), 1);
    assert_eq!(out.provenance.authoring.as_ref().unwrap().calls, 1);
    assert!(!keys(&out).contains(&"const.rule_expression"));
    assert!(
        out.candidate
            .as_ref()
            .unwrap()
            .contains("group_by(.address")
    );
    let record = out.provenance.plan.clone().unwrap();
    assert!(record.get("pending_transform").is_none());
    assert!(record.get("verified_transform").is_some());
    let replay = compile(&CompileRequest::create(SHARED).with_plan(record)).unwrap();
    assert_eq!(replay.status, CompileStatus::Ready, "{replay:#?}");
    assert_eq!(replay.candidate, out.candidate);
    assert!(replay.provenance.authoring.is_none());
}
#[tokio::test]
async fn provider_failure_keeps_the_answer_and_spends_a_durable_bounded_attempt() {
    let first = pending_fields().await;
    let out = compile_with_provider(
        &answered(first.provenance.plan.unwrap()),
        &nika_compile_cognition::NoProvider,
    )
    .await
    .unwrap();
    assert_ne!(out.status, CompileStatus::Ready, "{out:#?}");
    assert!(out.candidate.is_none());
    assert!(
        out.diagnostics
            .iter()
            .any(|d| d.target == "authoring_provider")
    );
    assert!(!keys(&out).contains(&"const.rule_expression"));
    let record = out.provenance.plan.unwrap();
    assert_eq!(record["pending_transform"]["attempts"], 1);
    assert_eq!(
        record["pending_transform"]["fields"][0]["answer"],
        "address"
    );
    let provider = Rotating::new(vec![addressed_program().to_string()]);
    // The next explicit round needs no repeated field answer.
    let req = CompileRequest::create(SHARED)
        .with_plan(record)
        .with_authoring_policy(policy());
    let retried = compile_with_provider(&req, &provider).await.unwrap();
    assert_eq!(retried.status, CompileStatus::Ready, "{retried:#?}");
    assert_eq!(provider.calls.load(std::sync::atomic::Ordering::SeqCst), 1);
}
#[tokio::test]
async fn unoffered_or_invalid_field_answers_never_call_the_provider() {
    let first = pending_fields().await;
    let provider = Rotating::new(vec![addressed_program().to_string()]);
    for answer in ["\"missing\"", "\"ADDRESS\"", "42", "not-json"] {
        let req = CompileRequest::create(SHARED)
            .with_plan(first.provenance.plan.clone().unwrap())
            .with_authoring_policy(policy())
            .answer("const.rule_field_1", answer);
        let out = compile_with_provider(&req, &provider).await.unwrap();
        assert_ne!(out.status, CompileStatus::Ready, "{out:#?}");
        let q = out
            .questions
            .iter()
            .find(|q| q.key == "const.rule_field_1")
            .unwrap();
        assert_eq!(q.answer_type, nika_compile::QuestionType::Choice);
        assert_eq!(
            q.options.iter().map(|o| o.key.as_str()).collect::<Vec<_>>(),
            ["id", "address"]
        );
        assert!(!keys(&out).contains(&"const.rule_expression"));
    }
    assert_eq!(provider.calls.load(std::sync::atomic::Ordering::SeqCst), 0);
}
#[tokio::test]
async fn changed_source_or_intent_invalidates_pending_choices_without_a_call() {
    let first = pending_fields().await;
    let record = first.provenance.plan.unwrap();
    let provider = Rotating::new(vec![addressed_program().to_string()]);
    let mut changed = observed_address();
    changed["observed"][0]["peek_sha256"] = json!("synthetic-observation-b");
    for world in [
        changed,
        json!({"observed":[]}),
        json!({"observed":[{"path":"./data/people.json", "state":"absent"}]}),
    ] {
        let out = compile_with_provider(&answered(record.clone()).with_knowledge(world), &provider)
            .await
            .unwrap();
        assert_ne!(out.status, CompileStatus::Ready, "{out:#?}");
        assert!(out.provenance.plan.is_none());
        assert!(out.questions.is_empty());
        assert!(
            out.diagnostics
                .iter()
                .any(|d| d.target == "pending_transform")
        );
    }
    for intent in [format!("{SHARED} Keep an audit."), "hello".to_owned()] {
        let req = CompileRequest::create(intent)
            .with_plan(record.clone())
            .with_authoring_policy(policy())
            .answer("const.rule_field_1", "\"address\"");
        let out = compile_with_provider(&req, &provider).await.unwrap();
        assert_ne!(out.status, CompileStatus::Ready, "{out:#?}");
        assert!(out.provenance.plan.is_none());
    }
    let req = answered(record).answer(
        "intent.clarification",
        serde_json::to_string(SHARED).unwrap(),
    );
    let out = compile_with_provider(&req, &provider).await.unwrap();
    assert_ne!(out.status, CompileStatus::Ready, "{out:#?}");
    assert!(out.provenance.plan.is_none());
    assert_eq!(provider.calls.load(std::sync::atomic::Ordering::SeqCst), 0);
}
#[tokio::test]
async fn invalid_regeneration_is_not_admitted_and_attempts_cannot_loop() {
    let first = pending_fields().await;
    // Still uses the rejected field: it cannot pass the observed-key verifier.
    let provider = Rotating::new(vec![
        transform(PROGRAM, &shared_two(), &["email"]).to_string(),
    ]);
    let mut req = answered(first.provenance.plan.unwrap());
    for attempt in 1..=3 {
        let out = compile_with_provider(&req, &provider).await.unwrap();
        assert_ne!(out.status, CompileStatus::Ready, "{out:#?}");
        assert!(out.candidate.is_none());
        assert!(!keys(&out).contains(&"const.rule_expression"));
        let record = out.provenance.plan.unwrap();
        assert_eq!(record["pending_transform"]["attempts"], attempt.min(2));
        req = CompileRequest::create(SHARED)
            .with_plan(record)
            .with_authoring_policy(policy());
    }
    assert_eq!(provider.calls.load(std::sync::atomic::Ordering::SeqCst), 2);
}
#[tokio::test]
async fn regeneration_neither_saves_nor_runs_the_authored_workflow() {
    let dir = tempfile::tempdir().unwrap();
    let source = dir
        .path()
        .join("people.json")
        .to_string_lossy()
        .into_owned();
    let output = dir
        .path()
        .join("shared.json")
        .to_string_lossy()
        .into_owned();
    let replace_paths = |text: String| {
        text.replace("./data/people.json", &source)
            .replace("./out/shared.json", &output)
    };
    let intent = replace_paths(SHARED.to_owned());
    let mut world = observed_address();
    world["observed"][0]["path"] = json!(source);
    let provider = Rotating::new(vec![
        replace_paths(plan().to_string()),
        transform(PROGRAM, &shared_two(), &["email"]).to_string(),
    ]);
    let initial = compile_with_provider(
        &CompileRequest::create(&intent)
            .with_authoring_policy(policy())
            .with_knowledge(world),
        &provider,
    )
    .await
    .unwrap();
    let regen = Rotating::new(vec![addressed_program().to_string()]);
    let req = CompileRequest::create(intent)
        .with_plan(initial.provenance.plan.unwrap())
        .with_authoring_policy(policy())
        .answer("const.rule_field_1", "\"address\"");
    let out = compile_with_provider(&req, &regen).await.unwrap();
    assert_eq!(out.status, CompileStatus::Ready, "{out:#?}");
    assert!(!std::path::Path::new(&source).exists());
    assert!(!std::path::Path::new(&output).exists());
    assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), 0);
}

#[tokio::test]
async fn verified_field_receipts_cannot_survive_a_source_change_or_a_new_answer() {
    let first = pending_fields().await;
    let provider = Rotating::new(vec![addressed_program().to_string()]);
    let ready = compile_with_provider(&answered(first.provenance.plan.unwrap()), &provider)
        .await
        .unwrap();
    assert_eq!(ready.status, CompileStatus::Ready, "{ready:#?}");
    let record = ready.provenance.plan.unwrap();
    let changed_answer = compile(
        &CompileRequest::create(SHARED)
            .with_plan(record.clone())
            .answer("const.rule_field_1", "\"id\""),
    )
    .unwrap();
    assert_ne!(changed_answer.status, CompileStatus::Ready);
    assert!(changed_answer.candidate.is_none());
    let mut world = observed_address();
    world["observed"][0]["peek_sha256"] = json!("synthetic-observation-new");
    let changed_source = compile(
        &CompileRequest::create(SHARED)
            .with_plan(record)
            .with_knowledge(world),
    )
    .unwrap();
    assert_ne!(changed_source.status, CompileStatus::Ready);
    assert!(changed_source.candidate.is_none());
}

#[tokio::test]
async fn edits_unknown_pending_versions_and_changed_plans_do_not_reuse_a_choice() {
    let first = pending_fields().await;
    let record = first.provenance.plan.unwrap();
    let edit = CompileRequest::edit("nika: sample\ntasks: {}\n", "Change the destination.")
        .with_plan(record.clone())
        .with_authoring_policy(policy());
    let provider = Rotating::new(vec![addressed_program().to_string()]);
    let out = compile_with_provider(&edit, &provider).await.unwrap();
    assert!(out.candidate.is_none());
    assert!(
        out.diagnostics
            .iter()
            .any(|d| d.target == "pending_transform")
    );
    let mut unknown = record.clone();
    unknown["pending_transform"]["schema"] = json!("nika/pending-transform@999");
    let mut changed_snapshot = record.clone();
    changed_snapshot["observed_world"]["observed"][0]["peek_sha256"] =
        json!("changed-recorded-source");
    let mut changed_plan = record;
    changed_plan["constraints"] = json!(["new constraint"]);
    for record in [unknown, changed_plan, changed_snapshot] {
        let out = compile_with_provider(&answered(record), &provider)
            .await
            .unwrap();
        assert_ne!(out.status, CompileStatus::Ready);
        assert!(out.provenance.plan.is_none());
        assert!(out.candidate.is_none());
    }
    assert_eq!(provider.calls.load(std::sync::atomic::Ordering::SeqCst), 0);
}

struct InspectRegeneration;
impl nika_kernel::ai::provider::ProviderInferDyn for InspectRegeneration {
    async fn infer(
        &self,
        request: nika_kernel::ai::provider::InferRequest,
    ) -> Result<nika_kernel::ai::provider::InferResponse, nika_kernel::ai::provider::ProviderError>
    {
        use nika_kernel::ai::provider::{ContentBlock, InferResponse, StopReason, TokenUsage};
        assert_eq!(request.max_tokens, Some(1024));
        assert_eq!(request.timeout, Some(std::time::Duration::from_secs(2)));
        assert!(request.tools.is_empty());
        let context = request
            .messages
            .iter()
            .flat_map(|m| &m.content)
            .find_map(|b| match b {
                ContentBlock::Text { text } => serde_json::from_str::<Value>(text).ok(),
                _ => None,
            })
            .unwrap();
        assert_eq!(context["request"], SHARED);
        assert_eq!(context["field_choices"][0]["name"], "email");
        assert_eq!(context["field_choices"][0]["answer"], "address");
        assert_eq!(context["columns"], json!(["id", "address"]));
        assert_eq!(context["source"]["peek_sha256"], "synthetic-observation-a");
        Ok(InferResponse::new(
            vec![ContentBlock::Text {
                text: addressed_program().to_string(),
            }],
            TokenUsage::new(10, 5),
            StopReason::EndTurn,
        ))
    }
}
#[tokio::test]
async fn regeneration_sends_original_intent_and_answer_as_data_under_the_existing_bounds() {
    let initial = pending_fields().await;
    let out = compile_with_provider(
        &answered(initial.provenance.plan.unwrap()),
        &InspectRegeneration,
    )
    .await
    .unwrap();
    assert_eq!(out.status, CompileStatus::Ready, "{out:#?}");
}
#[tokio::test]
async fn having_a_provider_without_an_authoring_policy_does_not_authorize_regeneration() {
    let initial = pending_fields().await;
    let mut req = answered(initial.provenance.plan.unwrap());
    req.authoring = None;
    let provider = Rotating::new(vec![addressed_program().to_string()]);
    let out = compile_with_provider(&req, &provider).await.unwrap();
    assert_ne!(out.status, CompileStatus::Ready, "{out:#?}");
    assert!(out.candidate.is_none());
    assert!(!keys(&out).contains(&"const.rule_expression"));
    assert_eq!(provider.calls.load(std::sync::atomic::Ordering::SeqCst), 0);
}
