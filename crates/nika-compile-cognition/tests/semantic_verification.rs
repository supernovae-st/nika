// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>
//! What a candidate a model wrote must be before it is READY, against the whole request (R4
//! A11). The seats here are injected doubles, named as such: they script what a provider
//! answers, never what the compiler decides. Business outcomes run the emitted program on rows
//! stated before the run, with the jaq crates and the capability filter the runtime installs.
#![allow(clippy::unwrap_used, clippy::expect_used)]
use jaq_core::load::{Arena, File, Loader};
use jaq_core::{Compiler, Ctx, Vars, data as jaq_data};
use jaq_json::{Val, read};
use nika_compile::{AuthoringPolicy, CompileOutcome, CompileRequest, CompileStatus, HotPolicy};
use nika_compile_cognition::compile_with_provider;
use nika_kernel::ai::provider::{
    ContentBlock, InferRequest, InferResponse, ProviderError, ProviderInferDyn, StopReason,
    TokenUsage,
};
use serde_json::{Value, json};
use std::sync::Mutex;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

/// An injected seat that answers its calls in order and keeps the last message each call sent
/// (a double: it scripts a provider, and optionally the admission layer's call ceiling or a
/// provider failure).
struct Scripted {
    answers: Vec<String>,
    calls: AtomicUsize,
    asked: Mutex<Vec<String>>,
    /// The first call the double's ceiling refuses before any transport; none by default.
    refused_from: usize,
    /// The first call the double's provider fails with no answer; none by default.
    failed_from: usize,
}

impl Scripted {
    fn new(answers: Vec<String>) -> Self {
        Self {
            answers,
            calls: AtomicUsize::new(0),
            asked: Mutex::new(Vec::new()),
            refused_from: usize::MAX,
            failed_from: usize::MAX,
        }
    }
    /// The same double behind a ceiling of `ceiling` calls: each later call is refused as the
    /// admission layer refuses it, locally, before any provider request.
    fn ceiling(answers: Vec<String>, ceiling: usize) -> Self {
        Self {
            refused_from: ceiling,
            ..Self::new(answers)
        }
    }
    /// The same double whose provider fails each call from `from` on with no answer, as a
    /// provider may before or after any transport (the receipt cannot tell which).
    fn failing(answers: Vec<String>, from: usize) -> Self {
        Self {
            failed_from: from,
            ..Self::new(answers)
        }
    }
    fn calls(&self) -> usize {
        self.calls.load(Ordering::SeqCst)
    }
    /// The JSON state the seat read in its call `at` (the call's last message).
    fn asked(&self, at: usize) -> Value {
        serde_json::from_str(&self.asked.lock().unwrap()[at]).unwrap()
    }
}

impl ProviderInferDyn for Scripted {
    async fn infer(&self, request: InferRequest) -> Result<InferResponse, ProviderError> {
        let last = request
            .messages
            .last()
            .and_then(|message| {
                message.content.iter().find_map(|block| match block {
                    ContentBlock::Text { text } => Some(text.clone()),
                    _ => None,
                })
            })
            .unwrap_or_default();
        self.asked.lock().unwrap().push(last);
        let at = self.calls.fetch_add(1, Ordering::SeqCst);
        if at >= self.refused_from {
            return Err(ProviderError::AdmissionDenied {
                reason: format!(
                    "the authoring call ceiling of {} calls is reached",
                    self.refused_from
                ),
            });
        }
        if at >= self.failed_from {
            return Err(ProviderError::Other {
                reason: "the provider failed with no answer".to_owned(),
            });
        }
        let text = self.answers[at.min(self.answers.len() - 1)].clone();
        Ok(InferResponse::new(
            vec![ContentBlock::Text { text }],
            TokenUsage::new(1, 1),
            StopReason::EndTurn,
        ))
    }
}

/// The one value `program` emits over `input`, with the jaq definitions and natives the
/// runtime's capability filter installs (the programs here use none of its corrections).
fn run(program: &str, input: &Value) -> Value {
    let defs = jaq_core::defs()
        .chain(jaq_std::defs().filter(|d| nika_cap::install_jq_definition(d.name)))
        .chain(jaq_json::defs());
    let funs = jaq_core::funs()
        .chain(jaq_std::funs())
        .chain(jaq_json::funs())
        .filter(|f| nika_cap::install_jq_native(f.0));
    let arena = Arena::default();
    let file = File {
        code: program,
        path: (),
    };
    let modules = Loader::new(defs)
        .load(&arena, file)
        .expect("the program parses");
    let filter = Compiler::default()
        .with_funs(funs)
        .with_global_vars(std::iter::once(nika_cap::JQ_RUN_START_VAR))
        .compile(modules)
        .expect("the program compiles");
    let vars = Vars::new(std::iter::once(Val::from(1_700_000_000_isize)));
    let ctx = Ctx::<jaq_data::JustLut<Val>>::new(&filter.lut, vars);
    let val = read::parse_single(&serde_json::to_vec(input).unwrap()).unwrap();
    let out: Vec<Val> = filter
        .id
        .run((ctx, val))
        .map(|r| r.expect("runs"))
        .collect();
    assert_eq!(out.len(), 1, "one value");
    serde_json::from_str(&out[0].to_string()).unwrap()
}

/// The live DEV request (B16's composed `DeepSeek` run, 2026-09-29): its computation clause
/// and its write, over the source as observed.
const SUM: (&str, &str) = (
    "sum qty over the rows where status is shipped",
    "write the sum to ./out/result.json",
);

fn intent((stated, write): (&str, &str)) -> String {
    format!("read ./data/input.csv, {stated}, {write}")
}

/// The plan the seat proposed: a computation the typed stages do not state (treatment B).
fn plan((stated, write): (&str, &str)) -> Value {
    json!({
        "steps": [
            {"op": "read", "detail": "./data/input.csv", "evidence": "read ./data/input.csv"},
            {"op": "compute", "detail": stated, "evidence": stated, "computation": {"present": false}}
        ],
        "effects": [{"verb": "write", "target": "./out/result.json", "policy": "automatic", "evidence": write}],
        "obligations": [], "constraints": [], "unknowns": [],
        "regions": [
            {"text": "read ./data/input.csv,", "role": "operation"},
            {"text": format!("{stated},"), "role": "operation"},
            {"text": write, "role": "effect"}
        ],
        "approval_bypass": {"present": false, "evidence": ""}
    })
}

/// A transform answer reading `status` and the quantity column named `qty`, with the seat's own
/// example (a shipped 40 beside a pending 15).
fn program_over(jq: &str, qty: &str) -> String {
    let example = json!([
        {"id": "a1", "status": "shipped", qty: "40"},
        {"id": "a2", "status": "pending", qty: "15"}
    ]);
    json!({"jq": jq, "columns_read": ["status", qty], "example_input": example, "expected_output": 40})
        .to_string()
}

/// A transform answer over the source's own `qty` column.
fn program(jq: &str) -> String {
    program_over(jq, "qty")
}

/// The request of `clause` compiled by `seat` under a policy granting `repairs` repair rounds.
async fn compiled_as(seat: &Scripted, clause: (&str, &str), repairs: u32) -> CompileOutcome {
    let observed = json!({"observed": [{"path": "./data/input.csv", "state": "observed", "complete": false, "kind": "csv", "columns": ["id", "item", "status", "qty"]}]});
    let policy =
        AuthoringPolicy::new("mock/authoring", 1024, Duration::from_secs(2)).with_repairs(repairs);
    let request = CompileRequest::create(intent(clause))
        .with_knowledge(observed)
        .with_hot_policy(HotPolicy::Off)
        .with_authoring_policy(policy);
    compile_with_provider(&request, seat).await.unwrap()
}

/// The live request under the default policy (three repair rounds).
async fn compiled(seat: &Scripted) -> CompileOutcome {
    compiled_as(seat, SUM, 3).await
}

/// The compute program of a READY candidate.
fn compute(out: &CompileOutcome) -> String {
    assert_eq!(out.status, CompileStatus::Ready, "{out:#?}");
    let doc: Value = serde_yaml_bw::from_str(out.candidate.as_deref().unwrap()).unwrap();
    doc["tasks"]["compute"]["invoke"]["args"]["expression"]
        .as_str()
        .unwrap()
        .to_owned()
}

/// The roles of the calls the receipt journals, in call order.
fn roles(out: &CompileOutcome) -> Vec<String> {
    let receipt = out.provenance.authoring.as_ref().unwrap();
    receipt
        .context
        .iter()
        .filter_map(|c| c["call"].as_str().map(str::to_owned))
        .collect()
}

fn rows(pairs: &[(&str, &str)]) -> Value {
    let records: Vec<Value> = pairs
        .iter()
        .map(|(status, qty)| json!({"status": status, "qty": qty}))
        .collect();
    json!({ "records": records })
}

/// B16's preregistered fixtures (the columns the program reads): the main rows sum to 70, the
/// adverse rows to 107.
const MAIN: [(&str, &str); 5] = [
    ("shipped", "40"),
    ("pending", "15"),
    ("shipped", "25"),
    ("cancelled", "60"),
    ("shipped", "5"),
];
const ADVERSE: [(&str, &str); 6] = [
    ("pending", "40"),
    ("shipped", "15"),
    ("shipped", "25"),
    ("shipped", "60"),
    ("cancelled", "5"),
    ("shipped", "7"),
];
/// B16's zero-row fixtures: no shipped row among the rows, and a header with no row at all.
/// The oracle of both is the number 0.
const NO_MATCHING_ROWS: [(&str, &str); 2] = [("pending", "40"), ("cancelled", "25")];
const HEADER_ONLY: [(&str, &str); 0] = [];

/// The whole-output oracle on the four fixtures: 70, 107, and 0 where no row is kept.
fn exact_on_every_fixture(expression: &str) {
    assert_eq!(run(expression, &rows(&MAIN)), json!(70), "{expression}");
    assert_eq!(run(expression, &rows(&ADVERSE)), json!(107), "{expression}");
    assert_eq!(
        run(expression, &rows(&NO_MATCHING_ROWS)),
        json!(0),
        "{expression}"
    );
    assert_eq!(
        run(expression, &rows(&HEADER_ONLY)),
        json!(0),
        "{expression}"
    );
}

/// The live counterexample: the program B16's composed `DeepSeek` run generated, exact on its two
/// fixtures and null over no shipped row, where the write failed at run.
const GENERATED: &str = ".records | map(select(.status == \"shipped\") | .qty | tonumber) | add";
/// The kept quantities, before any aggregate.
const KEPT: &str = ".records | map(select(.status == \"shipped\") | .qty | tonumber)";

/// The unchanged generated program over B16's zero-row fixtures (R4 A11): null, which the write
/// cannot take. This is the defect before any verifier, on the data it failed on.
#[test]
fn the_generated_program_is_null_where_no_row_is_kept() {
    assert_eq!(run(GENERATED, &rows(&MAIN)), json!(70));
    assert_eq!(run(GENERATED, &rows(&ADVERSE)), json!(107));
    assert_eq!(run(GENERATED, &rows(&NO_MATCHING_ROWS)), Value::Null);
    assert_eq!(run(GENERATED, &rows(&HEADER_ONLY)), Value::Null);
}

/// The generated empty-sum program is repaired from its concrete defect (R4 A11, fixed plan).
/// It was READY and the run failed on a source with no shipped row; the seat now reads its own
/// program and the sum's identity refusal, returns `add // 0`, and the workflow is exact on the
/// four fixtures. The human is never asked for what the verifier states.
#[tokio::test]
async fn a_program_null_on_an_empty_source_is_repaired_from_its_defect() {
    let repaired = format!("{GENERATED} // 0");
    let seat = Scripted::new(vec![
        plan(SUM).to_string(),
        program(GENERATED),
        program(&repaired),
    ]);
    let out = compiled(&seat).await;
    let expression = compute(&out);
    assert!(expression.ends_with(&repaired), "{expression}");
    assert_eq!(seat.calls(), 3);
    assert_eq!(roles(&out), ["plan", "transform", "transform_repair"]);
    let verifier = &seat.asked(2)["verifier"];
    assert_eq!(verifier["your_program"], json!(GENERATED));
    let refused = verifier["refused"].as_str().unwrap();
    assert!(refused.contains("the number 0"), "{refused}");
    let attempt = &out.provenance.decision.as_ref().unwrap()["transform_repairs"][0];
    assert_eq!(attempt["call"], json!("answered"));
    exact_on_every_fixture(&expression);
}

/// A repair the call ceiling refuses was requested, never sent (R4 A11, a labelled negative):
/// the double refuses the third call before any transport, as the admission layer does. The
/// finding and the attempt say so, the defect stays named, and nothing is READY.
#[tokio::test]
async fn a_repair_the_call_ceiling_refuses_is_requested_never_sent() {
    let seat = Scripted::ceiling(vec![plan(SUM).to_string(), program(GENERATED)], 2);
    let out = compiled(&seat).await;
    assert_eq!(out.status, CompileStatus::Incomplete, "{out:#?}");
    assert_eq!(roles(&out), ["plan", "transform", "transform_repair"]);
    let receipt = out.provenance.authoring.as_ref().unwrap();
    let refused = &receipt.context[2]["result"]["failure_kind"];
    assert_eq!(refused, &json!("admission_refused"));
    let decision = out.provenance.decision.as_ref().unwrap();
    assert_eq!(
        decision["transform_repairs"][0]["call"],
        json!("admission_refused")
    );
    let why = decision["transforms"][0]["why"].as_str().unwrap();
    assert!(why.contains("the number 0"), "{why}");
    assert!(why.contains("before any transport"), "{why}");
    let told: Vec<&str> = out
        .diagnostics
        .iter()
        .filter(|d| d.target == "authoring_transform")
        .map(|d| d.message.as_str())
        .collect();
    assert!(
        told.iter()
            .any(|m| m.contains("refused its call before any transport")),
        "{told:?}"
    );
    assert!(!told.iter().any(|m| m.contains("was sent")), "{told:?}");
}

/// A repair whose provider call failed is told as failed, its transport unobserved, never as
/// sent (R4 A11, a labelled negative): the double's provider fails the third call with no
/// answer. The attempt keeps the receipt's failure kind and nothing is READY.
#[tokio::test]
async fn a_repair_whose_provider_call_failed_is_never_told_as_sent() {
    let seat = Scripted::failing(vec![plan(SUM).to_string(), program(GENERATED)], 2);
    let out = compiled(&seat).await;
    assert_eq!(out.status, CompileStatus::Incomplete, "{out:#?}");
    let receipt = out.provenance.authoring.as_ref().unwrap();
    let failed = &receipt.context[2]["result"]["failure_kind"];
    assert_eq!(failed, &json!("provider_error"));
    let decision = out.provenance.decision.as_ref().unwrap();
    assert_eq!(
        decision["transform_repairs"][0]["call"],
        json!("provider_error")
    );
    let told: Vec<&str> = out
        .diagnostics
        .iter()
        .filter(|d| d.target == "authoring_transform")
        .map(|d| d.message.as_str())
        .collect();
    assert!(
        told.iter()
            .any(|m| m.contains("its provider call failed, its transport unobserved")),
        "{told:?}"
    );
    assert!(!told.iter().any(|m| m.contains("sent")), "{told:?}");
}

/// A policy granting no repair is obeyed (R4 A11): the refused program buys no second
/// transform call, and the request stays INCOMPLETE with the refusal recorded.
#[tokio::test]
async fn no_repair_is_bought_when_the_policy_grants_none() {
    let seat = Scripted::new(vec![plan(SUM).to_string(), program(GENERATED)]);
    let out = compiled_as(&seat, SUM, 0).await;
    assert_eq!(seat.calls(), 2);
    assert_eq!(roles(&out), ["plan", "transform"]);
    assert_eq!(out.status, CompileStatus::Incomplete, "{out:#?}");
    let transforms = &out.provenance.decision.as_ref().unwrap()["transforms"];
    assert_eq!(transforms[0]["accepted"], json!(false));
    let why = transforms[0]["why"].as_str().unwrap();
    assert!(why.contains("the number 0"), "{why}");
}

/// A safe refusal is no sum (R4 A11, labelled negative): a program stopping with an error where
/// no row is kept is refused for a sum, whose value there is 0. Repaired from once, a seat that
/// keeps the error leaves the request INCOMPLETE, never READY.
#[tokio::test]
async fn an_explicit_error_is_no_sum_of_no_row() {
    let error = format!("{KEPT} | if length == 0 then error else add end");
    let seat = Scripted::new(vec![
        plan(SUM).to_string(),
        program(&error),
        program(&error),
    ]);
    let out = compiled(&seat).await;
    assert_eq!(seat.calls(), 3);
    assert_eq!(roles(&out), ["plan", "transform", "transform_repair"]);
    assert_eq!(out.status, CompileStatus::Incomplete, "{out:#?}");
}

/// Equivalent sums are admitted as proposed (R4 A11): each holds the identity of the sum and
/// is exact on the four fixtures, and no repair call is spent on it.
#[tokio::test]
async fn equivalent_sums_are_admitted_as_proposed() {
    for sum in [
        format!("{GENERATED} // 0"),
        "reduce (.records[] | select(.status == \"shipped\") | .qty | tonumber) as $q (0; . + $q)"
            .to_owned(),
        "[.records[] | select(.status == \"shipped\") | .qty | tonumber] | add // 0".to_owned(),
    ] {
        let seat = Scripted::new(vec![plan(SUM).to_string(), program(&sum)]);
        let out = compiled(&seat).await;
        let expression = compute(&out);
        assert!(expression.ends_with(&sum), "{sum}");
        assert_eq!(seat.calls(), 2, "{sum}");
        exact_on_every_fixture(&expression);
    }
}

/// The floor of every clause, end to end (R4 A11): a minimum of no row has no value, so the sum's
/// identity does not apply, yet a program returning null there is refused and repaired from that
/// defect. The stated error it returns instead is admitted; 0 is never required of a minimum.
#[tokio::test]
async fn a_minimum_null_on_no_kept_row_is_repaired_to_a_stated_error() {
    let minimum = (
        "minimum qty over the rows where status is shipped",
        "write the minimum to ./out/result.json",
    );
    let nulled = format!("{KEPT} | min");
    let error = format!("{KEPT} | if length == 0 then error else min end");
    let seat = Scripted::new(vec![
        plan(minimum).to_string(),
        program(&nulled),
        program(&error),
    ]);
    let out = compiled_as(&seat, minimum, 3).await;
    let expression = compute(&out);
    assert!(expression.ends_with(&error), "{expression}");
    assert_eq!(roles(&out), ["plan", "transform", "transform_repair"]);
    let verifier = &seat.asked(2)["verifier"];
    assert_eq!(verifier["your_program"], json!(nulled));
    let refused = verifier["refused"].as_str().unwrap();
    assert!(refused.contains("cannot write"), "{refused}");
    assert_eq!(run(&expression, &rows(&MAIN)), json!(5));
    assert_eq!(run(&expression, &rows(&ADVERSE)), json!(7));
}

/// The first round of the field-answer path: the seat's program reads `quantity`, a field the
/// observed source does not carry, so the computation waits on the human's field choice. The
/// record and the question's key.
async fn pending_on_a_field() -> (Value, String) {
    let unread = "[.records[] | select(.status == \"shipped\") | .quantity | tonumber] | add // 0";
    let seat = Scripted::new(vec![
        plan(SUM).to_string(),
        program_over(unread, "quantity"),
    ]);
    let out = compiled(&seat).await;
    let record = out.provenance.plan.clone().unwrap();
    assert!(record.get("pending_transform").is_some(), "{out:#?}");
    let key = out
        .questions
        .iter()
        .find(|q| q.key.starts_with("const.rule_field"))
        .map(|q| q.key.clone())
        .unwrap();
    (record, key)
}

/// The answer round: the field answered `qty`, the regeneration call, and the repair its policy
/// may buy.
async fn answered(record: Value, key: &str, seat: &Scripted, repairs: u32) -> CompileOutcome {
    let policy =
        AuthoringPolicy::new("mock/authoring", 1024, Duration::from_secs(2)).with_repairs(repairs);
    let request = CompileRequest::create(intent(SUM))
        .with_plan(record)
        .with_authoring_policy(policy)
        .answer(key, "\"qty\"");
    compile_with_provider(&request, seat).await.unwrap()
}

/// The field-answer path holds the same laws and buys the same repair (R4 A11, parity): the
/// program regenerated once the human answered `qty` is null where no row is kept, is refused by
/// the sum's identity, and is repaired from that defect within the answer round's allowance.
#[tokio::test]
async fn a_regenerated_program_is_held_to_the_same_laws_and_repair() {
    let (record, key) = pending_on_a_field().await;
    let repaired = format!("{GENERATED} // 0");
    let seat = Scripted::new(vec![program(GENERATED), program(&repaired)]);
    let out = answered(record, &key, &seat, 3).await;
    let expression = compute(&out);
    assert!(expression.ends_with(&repaired), "{expression}");
    assert_eq!(seat.calls(), 2);
    assert_eq!(roles(&out), ["transform", "transform_repair"]);
    let verifier = &seat.asked(1)["verifier"];
    assert_eq!(verifier["your_program"], json!(GENERATED));
    let refused = verifier["refused"].as_str().unwrap();
    assert!(refused.contains("the number 0"), "{refused}");
    exact_on_every_fixture(&expression);
}

/// With no repair granted, the regenerated program's refusal stands: one call, never READY.
#[tokio::test]
async fn a_regenerated_program_buys_no_repair_when_none_is_granted() {
    let (record, key) = pending_on_a_field().await;
    let seat = Scripted::new(vec![program(GENERATED)]);
    let out = answered(record, &key, &seat, 0).await;
    assert_eq!(seat.calls(), 1);
    assert_ne!(out.status, CompileStatus::Ready, "{out:#?}");
    let regeneration = &out.provenance.decision.as_ref().unwrap()["transform_regeneration"];
    assert_eq!(regeneration["accepted"], json!(false));
    let why = regeneration["why"].as_str().unwrap();
    assert!(why.contains("the number 0"), "{why}");
}
