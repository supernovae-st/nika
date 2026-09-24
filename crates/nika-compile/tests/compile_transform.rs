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
