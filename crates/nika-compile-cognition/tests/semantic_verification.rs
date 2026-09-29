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
use nika_compile::surface::{Binding, Disposition, Judgment};
use nika_compile::{AuthoringPolicy, CompileOutcome, CompileRequest, CompileStatus, HotPolicy};
use nika_compile_cognition::compile_with_provider;
use nika_compile_reader::plan::Plan;
use nika_compile_reader::shape::promote_stated_rules;
use nika_kernel::ai::provider::{
    ContentBlock, InferRequest, InferResponse, ProviderError, ProviderInferDyn, ResponseFormat,
    StopReason, TokenUsage,
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
    /// The system message of each call.
    systems: Mutex<Vec<String>>,
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
            systems: Mutex::new(Vec::new()),
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
    /// The text of the last message the seat read in its call `at`.
    fn said(&self, at: usize) -> String {
        self.asked.lock().unwrap()[at].clone()
    }
    /// The system message the seat read in its call `at`.
    fn system(&self, at: usize) -> String {
        self.systems.lock().unwrap()[at].clone()
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
        self.systems.lock().unwrap().push(first_text(&request));
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

/// A named judge double over a scripted seat (R4 A11): each verifier question (a closed choice
/// offering `faithful` or `carried`) is answered by `verdict` from the keys it offers and kept;
/// every other call goes to the scripted seat unchanged. A test names the verdict it opts into.
struct Judging<'a> {
    inner: &'a Scripted,
    verdict: fn(&[String]) -> &'static str,
    judged: Mutex<Vec<Vec<String>>>,
    states: Mutex<Vec<Value>>,
    systems: Mutex<Vec<String>>,
}

impl<'a> Judging<'a> {
    fn new(inner: &'a Scripted, verdict: fn(&[String]) -> &'static str) -> Self {
        Self {
            inner,
            verdict,
            judged: Mutex::new(Vec::new()),
            states: Mutex::new(Vec::new()),
            systems: Mutex::new(Vec::new()),
        }
    }
    /// The instructions each verifier question gave the judge (its system message).
    fn systems(&self) -> Vec<String> {
        self.systems.lock().unwrap().clone()
    }
    /// The state each verifier question showed the judge (the JSON of its STATE section).
    fn states(&self) -> Vec<Value> {
        self.states.lock().unwrap().clone()
    }
    /// The keys of each verifier question judged, in order.
    fn judged(&self) -> Vec<Vec<String>> {
        self.judged.lock().unwrap().clone()
    }
}

/// The text of a request's first message (its system instructions).
fn first_text(request: &InferRequest) -> String {
    request
        .messages
        .first()
        .and_then(|message| {
            message.content.iter().find_map(|block| match block {
                ContentBlock::Text { text } => Some(text.clone()),
                _ => None,
            })
        })
        .unwrap_or_default()
}

/// The approving verdict: the whole request faithful, each clause carried.
fn approve(keys: &[String]) -> &'static str {
    if keys.iter().any(|k| k == "faithful") {
        "faithful"
    } else {
        "carried"
    }
}

/// The verdict of a judge that finds every clause missing and the request unfaithful.
fn refuse(keys: &[String]) -> &'static str {
    if keys.iter().any(|k| k == "unfaithful") {
        "unfaithful"
    } else if keys.iter().any(|k| k == "missing") {
        "missing"
    } else {
        "another_part"
    }
}

/// The verdict of a judge that finds every clause missing and the request unfaithful, and locates
/// the last part the localization lists (R4 A11, E36).
fn refuse_last_part(keys: &[String]) -> &'static str {
    let last = keys.iter().filter(|k| k.starts_with("part-")).next_back();
    match last.map(String::as_str) {
        Some("part-0") => "part-0",
        Some("part-1") => "part-1",
        Some("part-2") => "part-2",
        Some("part-3") => "part-3",
        Some(_) => "another_part",
        None => refuse(keys),
    }
}

/// The verdict of a judge that abstains on every question.
fn abstain(_: &[String]) -> &'static str {
    "none"
}

impl ProviderInferDyn for Judging<'_> {
    async fn infer(&self, request: InferRequest) -> Result<InferResponse, ProviderError> {
        let keys: Vec<String> = match &request.response_format {
            ResponseFormat::JsonSchema(schema) => schema["properties"]["choice"]["enum"]
                .as_array()
                .map(|keys| {
                    keys.iter()
                        .filter_map(|k| k.as_str().map(str::to_owned))
                        .collect()
                })
                .unwrap_or_default(),
            _ => Vec::new(),
        };
        let verifier = keys
            .iter()
            .any(|k| k == "faithful" || k == "carried" || k == "another_part");
        if !verifier {
            return self.inner.infer(request).await;
        }
        let choice = (self.verdict)(&keys);
        self.judged.lock().unwrap().push(keys);
        self.systems.lock().unwrap().push(first_text(&request));
        let said = request
            .messages
            .last()
            .and_then(|message| {
                message.content.iter().find_map(|block| match block {
                    ContentBlock::Text { text } => Some(text.clone()),
                    _ => None,
                })
            })
            .unwrap_or_default();
        let state = said
            .strip_prefix("STATE:\n")
            .and_then(|rest| rest.split("\n\nOPTIONS:").next())
            .and_then(|json| serde_json::from_str(json).ok())
            .unwrap_or(Value::Null);
        self.states.lock().unwrap().push(state);
        Ok(InferResponse::new(
            vec![ContentBlock::Text {
                text: json!({"choice": choice}).to_string(),
            }],
            TokenUsage::new(3, 1),
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
async fn compiled_as<P: ProviderInferDyn>(
    seat: &P,
    clause: (&str, &str),
    repairs: u32,
) -> CompileOutcome {
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
async fn compiled<P: ProviderInferDyn>(seat: &P) -> CompileOutcome {
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

/// The roles of the authoring calls the receipt journals, the judge's apart (R4 A11).
fn authored(out: &CompileOutcome) -> Vec<String> {
    roles(out)
        .into_iter()
        .filter(|role| !role.starts_with("judge_"))
        .collect()
}

/// The roles of the judge's calls the receipt journals, in call order (R4 A11).
fn judged(out: &CompileOutcome) -> Vec<String> {
    roles(out)
        .into_iter()
        .filter(|role| role.starts_with("judge_"))
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
    // Judged by the explicit approving double (R4 A11): this test reads the emitted program.
    let judge = Judging::new(&seat, approve);
    let out = compiled(&judge).await;
    let expression = compute(&out);
    assert!(expression.ends_with(&repaired), "{expression}");
    assert_eq!(seat.calls(), 3);
    assert_eq!(authored(&out), ["plan", "transform", "transform_repair"]);
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
        // Judged by the explicit approving double (R4 A11): this test reads the emitted program.
        let judge = Judging::new(&seat, approve);
        let out = compiled(&judge).await;
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
    // Judged by the explicit approving double (R4 A11): this test reads the emitted program.
    let judge = Judging::new(&seat, approve);
    let out = compiled_as(&judge, minimum, 3).await;
    let expression = compute(&out);
    assert!(expression.ends_with(&error), "{expression}");
    assert_eq!(authored(&out), ["plan", "transform", "transform_repair"]);
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
async fn answered<P: ProviderInferDyn>(
    record: Value,
    key: &str,
    seat: &P,
    repairs: u32,
) -> CompileOutcome {
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
    // Judged by the explicit approving double (R4 A11): this test reads the emitted program.
    let judge = Judging::new(&seat, approve);
    let out = answered(record, &key, &judge, 3).await;
    let expression = compute(&out);
    assert!(expression.ends_with(&repaired), "{expression}");
    assert_eq!(seat.calls(), 2);
    assert_eq!(authored(&out), ["transform", "transform_repair"]);
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

/// The first candidate of a seat's plan is judged whole on the full state (R4 A11): each
/// question shows the judge the request as compiled, its answers, the observed world and the
/// candidate's own bytes; the clauses no law reads from the bytes come first, the whole request
/// last. Approved, it is READY, and the judge's calls ride the receipt with the others.
#[tokio::test]
async fn the_first_cold_candidate_is_judged_whole_on_the_full_state() {
    let sum = format!("{GENERATED} // 0");
    let seat = Scripted::new(vec![plan(SUM).to_string(), program(&sum)]);
    let judge = Judging::new(&seat, approve);
    let out = compiled(&judge).await;
    assert_eq!(out.status, CompileStatus::Ready, "{out:#?}");
    let candidate = out.candidate.clone().unwrap();
    let asked = judged(&out);
    assert_eq!(
        asked.last().map(String::as_str),
        Some("judge_request"),
        "{asked:?}"
    );
    assert_eq!(asked.len(), judge.judged().len());
    for state in judge.states() {
        assert_eq!(state["request"], json!(intent(SUM)), "{state:#}");
        assert_eq!(state["candidate_nika"], json!(candidate), "{state:#}");
        assert_eq!(state["observed"]["observed"][0]["columns"][3], json!("qty"));
        assert!(state["answers"].is_object(), "{state:#}");
    }
    let receipt = out.provenance.authoring.as_ref().unwrap();
    assert_eq!(receipt.calls as usize, 2 + asked.len());
    let usage = &out.provenance.decision.as_ref().unwrap()["semantic_verification"][0]["usage"];
    assert_eq!(usage["calls"], json!(asked.len()), "{usage:#}");
    assert_eq!(usage["complete"], json!(true), "{usage:#}");
}

/// The tools a candidate reaches by the checker's own capability inference over its parsed
/// workflow, each with its whole contract section read from the embedded stdlib page itself (an
/// oracle independent of the verifier).
fn reached(candidate: &str) -> Vec<(String, String)> {
    let workflow = nika_compile::parse(candidate).unwrap();
    let tools = nika_check::infer_permits(&workflow)
        .permits
        .tools
        .unwrap_or_default();
    let page = nika_pack::doc("stdlib/builtins-v0.1.md").unwrap();
    tools
        .into_iter()
        .filter_map(|tool| {
            let heading = format!("### `{tool}`");
            let rest = &page[page.find(&heading)?..];
            let tail = &rest[heading.len()..];
            let end = [tail.find("\n### "), tail.find("\n## ")]
                .into_iter()
                .flatten()
                .min()
                .map_or(rest.len(), |at| at + heading.len());
            Some((tool, rest[..end].trim().to_owned()))
        })
        .collect()
}

/// Every verifier question and the repair are grounded in one compiler-owned reference kept apart
/// from the untrusted state (R4 A11, E36): the engine's output conventions (the written-total law,
/// a requested shape overriding it), the language section of the engine card and the WHOLE
/// contract of every tool the candidate reaches by the checker's capability inference over the
/// parsed workflow (the write contract past two thousand characters included, never cut). The
/// state keeps the request, the world and the candidate. The verdict records the digest and size
/// of the exact reference text sent, each piece's receipt and the engine identity; each judge
/// call and the repair journal those receipts, and the repair carries the same bytes.
#[tokio::test]
async fn the_judge_and_the_repair_read_one_grounded_reference() {
    let sum = format!("{GENERATED} // 0");
    let seat = Scripted::new(vec![
        plan(SUM).to_string(),
        program(&sum),
        plan(SUM).to_string(),
        program(&sum),
    ]);
    let judge = Judging::new(&seat, refuse);
    let out = compiled_as(&judge, SUM, 1).await;
    assert_eq!(authored(&out), ["plan", "transform", "repair", "transform"]);
    let digest = nika_compile::surface::sha256;
    let record = &out.provenance.decision.as_ref().unwrap()["semantic_verification"][0];
    let reference = &record["reference"];
    let systems = judge.systems();
    let first = systems.first().cloned().unwrap_or_default();
    let start = first.find("REFERENCE").unwrap_or(first.len());
    let bytes = usize::try_from(reference["bytes"].as_u64().unwrap_or(0)).unwrap();
    let carried = first.get(start..start + bytes).unwrap_or_default();
    assert!(!carried.is_empty(), "{first}");
    assert_eq!(reference["sha256"], json!(digest(carried)), "{reference:#}");
    let candidate = judge.states()[0]["candidate_nika"]
        .as_str()
        .unwrap()
        .to_owned();
    let contracts = reached(&candidate);
    let write = contracts.iter().find(|(tool, _)| tool == "nika:write");
    assert!(
        write.is_some_and(|(_, section)| section.len() > 2_000),
        "{contracts:?}"
    );
    let conventions = include_str!("../assets/native_output_conventions.md");
    assert!(carried.contains("untrusted"), "{carried}");
    assert!(carried.contains(conventions), "{carried}");
    assert!(
        carried.contains("« only the number » is the value alone"),
        "{carried}"
    );
    assert!(carried.contains("# The language in one page"), "{carried}");
    assert!(
        carried.contains("`-002` (`overwrite: false` and the path exists)"),
        "{carried}"
    );
    for (tool, section) in &contracts {
        assert!(carried.contains(section.as_str()), "{tool}: {carried}");
    }
    for system in systems.iter().chain([&seat.system(2)]) {
        assert!(system.contains(carried), "{system}");
        assert!(!system.contains(&candidate), "{system}");
    }
    for state in judge.states() {
        assert!(state.get("reference").is_none(), "{state:#}");
    }
    let pieces = reference["references"]
        .as_array()
        .cloned()
        .unwrap_or_default();
    for (tool, section) in &contracts {
        let piece = pieces.iter().find(|p| p["id"] == json!(tool));
        let sha = piece.map(|p| p["sha256"].clone());
        assert_eq!(sha, Some(json!(digest(section))), "{tool}: {reference:#}");
    }
    let identity = &reference["identity"];
    assert_eq!(identity["conventions_sha256"], json!(digest(conventions)));
    assert!(
        pieces
            .iter()
            .any(|p| p["sha256"] == identity["conventions_sha256"])
    );
    let journal = &out.provenance.authoring.as_ref().unwrap().context;
    let repair = journal.iter().position(|e| e["call"] == json!("repair"));
    let first_attempt = &journal[..=repair.unwrap()];
    let grounded: Vec<&Value> = first_attempt
        .iter()
        .filter(|e| {
            e["call"] == json!("repair")
                || e["call"].as_str().is_some_and(|c| c.starts_with("judge_"))
        })
        .collect();
    assert!(grounded.len() >= 2, "{journal:#?}");
    for entry in grounded {
        assert_eq!(entry["references"], reference["references"], "{entry:#}");
    }
    let repaired = first_attempt.last().unwrap();
    assert_eq!(
        repaired["instruction_sha256"],
        json!(digest(&seat.system(2)))
    );
}

/// A part the judge finds missing is repaired from with the state the judge read (R4 A11): the
/// repair call carries the request, its answers, the observed world and the candidate's bytes;
/// the repaired plan's computation goes through the transform seat again with the judge's
/// defects; a judge that still finds it missing leaves the request INCOMPLETE naming it, with
/// no question to the human.
#[tokio::test]
async fn a_part_the_judge_finds_missing_is_repaired_from_the_whole_state_then_named() {
    let sum = format!("{GENERATED} // 0");
    let seat = Scripted::new(vec![
        plan(SUM).to_string(),
        program(&sum),
        plan(SUM).to_string(),
        program(&sum),
    ]);
    let judge = Judging::new(&seat, refuse);
    let out = compiled_as(&judge, SUM, 1).await;
    assert_eq!(out.status, CompileStatus::Incomplete, "{out:#?}");
    assert_eq!(authored(&out), ["plan", "transform", "repair", "transform"]);
    let repair = seat.said(2);
    assert!(repair.starts_with("VERIFIER:"), "{repair}");
    for part in [
        intent(SUM).as_str(),
        "candidate_nika",
        "observed",
        "qty",
        "answers",
    ] {
        assert!(repair.contains(part), "{part}: {repair}");
    }
    let resynthesized = seat.asked(3);
    let defects = resynthesized["verifier_defects"].as_array().cloned();
    assert!(defects.is_some_and(|d| !d.is_empty()), "{resynthesized:#}");
    let named: Vec<&str> = out
        .diagnostics
        .iter()
        .filter(|d| d.target == "semantic_verification")
        .map(|d| d.message.as_str())
        .collect();
    assert!(
        named.iter().any(|m| m.contains("does not carry")),
        "{named:?}"
    );
    assert!(
        out.questions
            .iter()
            .all(|q| q.key != "intent.clarification"),
        "{out:#?}"
    );
}

/// The judge and the repair read the request the human first stated (R4 A11): when the
/// preserved original, the submitted text and the clarification that replaced it all differ,
/// every verifier question and the repair show the effective request and the preserved
/// original, the very text the binding holds, never the replaced submission; with no preserved
/// original, the submitted text is the first statement, for both.
#[tokio::test]
async fn the_judge_and_the_repair_read_the_preserved_original() {
    let sum = format!("{GENERATED} // 0");
    let original = "read ./data/input.csv and total the shipped quantities";
    let submitted = "read ./data/input.csv and sum qty where status is shipped";
    let effective = intent(SUM);
    for preserved in [Some(original), None] {
        let seat = Scripted::new(vec![
            plan(SUM).to_string(),
            program(&sum),
            plan(SUM).to_string(),
            program(&sum),
        ]);
        let judge = Judging::new(&seat, refuse);
        let observed = json!({"observed": [{"path": "./data/input.csv", "state": "observed", "complete": false, "kind": "csv", "columns": ["id", "item", "status", "qty"]}]});
        let policy =
            AuthoringPolicy::new("mock/authoring", 1024, Duration::from_secs(2)).with_repairs(1);
        let mut request = CompileRequest::create(submitted)
            .with_knowledge(observed)
            .with_hot_policy(HotPolicy::Off)
            .with_authoring_policy(policy);
        if let Some(first) = preserved {
            request = request.with_original_intent(first);
        }
        request
            .answers
            .insert("intent.clarification".into(), json!(effective).to_string());
        let out = compile_with_provider(&request, &judge).await.unwrap();
        let calls = authored(&out);
        assert_eq!(
            calls,
            ["plan", "transform", "repair", "transform"],
            "{out:#?}"
        );
        let repair = seat.said(2);
        let (_, shown) = repair.split_once("the candidate's bytes):\n").unwrap();
        let repaired: Value = serde_json::from_str(shown).unwrap();
        let states = judge.states();
        assert!(!states.is_empty());
        let first = preserved.unwrap_or(submitted);
        let bound = Binding::of(&effective, &request, &Plan::default(), "");
        assert_eq!(bound.original, Some(nika_compile::surface::sha256(first)));
        for state in states.iter().chain([&repaired]) {
            assert_eq!(state["request"], json!(effective), "{state:#}");
            assert_eq!(state["original_request"], json!(first), "{state:#}");
        }
    }
}

/// The part a judge locates reaches the repair as the request's own intact phrase (R4 A11, E36):
/// « write the sum to ./out/result.json » is never cut at the dots of its path into « write the
/// sum to », so the repair reads the defect it must correct, target included.
#[tokio::test]
async fn a_located_part_reaches_the_repair_whole() {
    let sum = format!("{GENERATED} // 0");
    let seat = Scripted::new(vec![
        plan(SUM).to_string(),
        program(&sum),
        plan(SUM).to_string(),
        program(&sum),
    ]);
    let judge = Judging::new(&seat, refuse_last_part);
    let out = compiled_as(&judge, SUM, 1).await;
    assert_eq!(authored(&out), ["plan", "transform", "repair", "transform"]);
    let repair = seat.said(2);
    assert!(repair.contains(&format!("\n- {}\n", SUM.1)), "{repair}");
}

/// The engine's output conventions state the written-total law the compiler emits (R4 A11,
/// E36: a judge held « write the sum to ./out/result.json » against the object the compiler
/// writes). Measured on emitted candidates: a total over every row goes to a structured file as
/// the compute's object and to a prose file as its value alone; an explicitly requested object is
/// that object, and an explicitly requested bare number is never silently wrapped (the
/// deterministic compile leaves it unread). The conventions say so, a requested shape
/// overriding, with no other wrapper, key or field.
#[test]
fn the_output_conventions_state_the_written_total_law() {
    let observed = json!({"observed": [{"path": "./data/input.csv", "state": "observed", "complete": false, "kind": "csv", "columns": ["id", "item", "status", "qty"]}]});
    let compile = |text: String| {
        nika_compile::compile(&CompileRequest::create(text).with_knowledge(observed.clone()))
            .unwrap()
    };
    let written = |write: &str| {
        let out = compile(format!("read ./data/input.csv, the total of qty, {write}"));
        assert_eq!(out.status, CompileStatus::Ready, "{write}: {out:#?}");
        let doc: Value = serde_yaml_bw::from_str(out.candidate.as_deref().unwrap()).unwrap();
        doc["tasks"]["write_output"]["with"]["content"]
            .as_str()
            .unwrap()
            .to_owned()
    };
    let object = "${{ tasks.compute.output }}";
    assert_eq!(written("write it to ./out/result.json"), object);
    assert_eq!(
        written("write it to ./out/result.md"),
        "${{ tasks.compute.output.total }}"
    );
    let requested = "write it as an object with a total field to ./out/result.json";
    assert_eq!(written(requested), object);
    let bare = compile(
        "read ./data/input.csv, the total of qty, write only the number to ./out/result.json"
            .to_owned(),
    );
    assert_ne!(bare.status, CompileStatus::Ready, "{bare:#?}");
    let conventions = include_str!("../assets/native_output_conventions.md");
    for statement in [
        "A total over every row is written as the engine's compute returns it",
        "A total the engine types (a named total)",
        "to a structured file (json, csv, yaml, toml), the object with one field per named total",
        "to a prose file (md, txt or any other destination), the value alone when it is the only total",
        "several totals keep the object",
        "A shape the request names overrides both",
        "Add no other wrapper, key or field",
    ] {
        assert!(conventions.contains(statement), "{statement}");
    }
    assert!(!conventions.contains("add no wrapper, key or field the request did not ask for"));
}

/// A computation the engine does not type is written as the value the jq program a seat
/// synthesized returns, to a json and to a prose file alike (R4 A11, E36): measured on the
/// emitted COLD candidates (the write carries the compute's own output, never a named total nor
/// a wrapper) and on that program's value over B16's rows (the number 70, not an object). The
/// conventions state it beside the typed law.
#[tokio::test]
async fn a_synthesized_computation_is_written_as_its_program_returns_it() {
    let sum = format!("{GENERATED} // 0");
    for write in [
        "write the sum to ./out/result.json",
        "write the sum to ./out/result.md",
    ] {
        let clause = (SUM.0, write);
        let mut shaped = plan(clause);
        shaped["effects"][0]["target"] = json!(write.rsplit(' ').next().unwrap());
        let seat = Scripted::new(vec![shaped.to_string(), program(&sum)]);
        let judge = Judging::new(&seat, approve);
        let out = compiled_as(&judge, clause, 1).await;
        let expression = compute(&out);
        assert_eq!(expression, sum, "{write}");
        let doc: Value = serde_yaml_bw::from_str(out.candidate.as_deref().unwrap()).unwrap();
        let content = &doc["tasks"]["write_output"]["with"]["content"];
        assert_eq!(content, &json!("${{ tasks.compute.output }}"), "{write}");
        assert_eq!(run(&expression, &rows(&MAIN)), json!(70), "{write}");
    }
    let conventions = include_str!("../assets/native_output_conventions.md");
    assert!(conventions.contains(
        "A computation the engine does not type is a jq program a seat synthesizes: the value that program returns, written as it is to a json or a prose file"
    ));
    assert!(!conventions.contains("written as it is to any destination"));
}

/// To a csv, yaml or toml file the value of a synthesized program is not written as it is: the
/// emitted COLD candidate converts it first (R4 A11, E36, B22's counterexample), a `nika:convert`
/// stage from json to the destination's format reading the compute's own output and feeding the
/// write, so the convert's accepted input shapes apply (no scalar conversion is promised). The
/// conventions say so.
#[tokio::test]
async fn a_synthesized_computation_to_csv_yaml_or_toml_passes_through_convert() {
    let sum = format!("{GENERATED} // 0");
    for format in ["csv", "yaml", "toml"] {
        let write = format!("write the sum to ./out/result.{format}");
        let clause = (SUM.0, write.as_str());
        let mut shaped = plan(clause);
        shaped["effects"][0]["target"] = json!(format!("./out/result.{format}"));
        let seat = Scripted::new(vec![shaped.to_string(), program(&sum)]);
        let judge = Judging::new(&seat, approve);
        let out = compiled_as(&judge, clause, 1).await;
        assert_eq!(compute(&out), sum, "{write}");
        let doc: Value = serde_yaml_bw::from_str(out.candidate.as_deref().unwrap()).unwrap();
        let content = doc["tasks"]["write_output"]["with"]["content"]
            .as_str()
            .unwrap_or_default()
            .to_owned();
        let stage = content
            .strip_prefix("${{ tasks.")
            .and_then(|rest| rest.strip_suffix(".output }}"))
            .unwrap_or_default();
        let converts = &doc["tasks"][stage];
        assert_eq!(
            converts["invoke"]["tool"],
            json!("nika:convert"),
            "{write}: {doc:#}"
        );
        assert_eq!(converts["invoke"]["args"]["from"], json!("json"), "{write}");
        assert_eq!(converts["invoke"]["args"]["to"], json!(format), "{write}");
        let data = &converts["with"]["data"];
        assert_eq!(data, &json!("${{ tasks.compute.output }}"), "{write}");
    }
    let conventions = include_str!("../assets/native_output_conventions.md");
    assert!(conventions.contains(
        "to a csv, yaml or toml file it first passes through `nika:convert` from json, whose accepted input shapes apply"
    ));
}

/// A judge that abstains settles nothing (R4 A11): no defect to repair from, no repair call,
/// the request INCOMPLETE naming what the judge could not settle.
#[tokio::test]
async fn an_abstaining_judge_keeps_the_request_incomplete() {
    let sum = format!("{GENERATED} // 0");
    let seat = Scripted::new(vec![plan(SUM).to_string(), program(&sum)]);
    let judge = Judging::new(&seat, abstain);
    let out = compiled(&judge).await;
    assert_eq!(out.status, CompileStatus::Incomplete, "{out:#?}");
    assert_eq!(authored(&out), ["plan", "transform"]);
    assert!(!judge.judged().is_empty());
    assert!(
        out.diagnostics
            .iter()
            .any(|d| d.target == "semantic_verification" && d.message.contains("could not settle")),
        "{out:#?}"
    );
}

/// B21 A2's wording over `shipped` (R4 A11, B21 T3): « as one line » after the synthesized rule's
/// clause is a cardinality no element of the workflow carries, so the core emits no candidate and
/// names the duty itself; no judge is asked.
const LINES: (&str, &str) = (
    "for the rows where status is shipped, return each item with its status as one line",
    "write the lines to ./out/result.json",
);

/// The seat's label program for [`LINES`] over its own example rows.
fn label() -> String {
    let example = json!([
        {"id": "a1", "item": "x", "status": "shipped", "qty": "40"},
        {"id": "a2", "item": "y", "status": "pending", "qty": "15"}
    ]);
    let jq = ".records | map(select(.status == \"shipped\") | .item + \": \" + .status)";
    json!({"jq": jq, "columns_read": ["status", "item"], "example_input": example, "expected_output": ["x: shipped"]})
        .to_string()
}

/// The verifier's findings on an outcome.
fn verifier_said(out: &CompileOutcome) -> Vec<String> {
    out.diagnostics
        .iter()
        .filter(|d| d.target == "semantic_verification")
        .map(|d| d.message.clone())
        .collect()
}

/// A duty the core names is told as the core's (R4 A11, B21 T3): with no candidate and no judge
/// call, no finding says a judge compared anything. It names the core, the duty and its kind,
/// and says no judge was asked; the clarification the core asks stays the next action.
#[tokio::test]
async fn a_duty_the_core_names_is_told_as_the_core_s_and_keeps_its_question() {
    let seat = Scripted::new(vec![plan(LINES).to_string(), label()]);
    let out = compiled_as(&seat, LINES, 0).await;
    assert_eq!(out.status, CompileStatus::Incomplete, "{out:#?}");
    assert_eq!(roles(&out), ["plan", "transform"], "{out:#?}");
    let told = verifier_said(&out);
    let named = told.iter().any(|m| {
        m.contains("The core named") && m.contains("no judge was asked") && m.contains(LINES.0)
    });
    assert!(named, "{told:?}");
    assert!(told.iter().all(|m| !m.contains("compared")), "{told:?}");
    assert!(
        out.questions
            .iter()
            .any(|q| q.key == "intent.clarification"),
        "{out:#?}"
    );
}

/// A repair from a duty the core names is never told a judge compared the workflow (R4 A11, B21
/// T3): it names the duty and says no judge compared anything. The repaired plan still leaves
/// the duty uncarried: INCOMPLETE, told as the core's, with its question.
#[tokio::test]
async fn a_repair_from_a_duty_the_core_names_is_not_told_a_judge_compared() {
    let seat = Scripted::new(vec![
        plan(LINES).to_string(),
        label(),
        plan(LINES).to_string(),
        label(),
    ]);
    let out = compiled_as(&seat, LINES, 1).await;
    assert_eq!(
        authored(&out),
        ["plan", "transform", "repair", "transform"],
        "{out:#?}"
    );
    let repair = seat.said(2);
    assert!(repair.starts_with("VERIFIER:"), "{repair}");
    assert!(!repair.contains("compared"), "{repair}");
    assert!(repair.contains("no judge"), "{repair}");
    assert!(repair.contains(&format!("\n- {}", LINES.0)), "{repair}");
    assert_eq!(out.status, CompileStatus::Incomplete, "{out:#?}");
    assert!(verifier_said(&out).iter().all(|m| !m.contains("compared")));
    assert!(
        out.questions
            .iter()
            .any(|q| q.key == "intent.clarification"),
        "{out:#?}"
    );
}

/// The READY record of the live request and its candidate, judged in its first compile by the
/// explicit approving double.
async fn judged_record() -> (Value, String) {
    let sum = format!("{GENERATED} // 0");
    let seat = Scripted::new(vec![plan(SUM).to_string(), program(&sum)]);
    let judge = Judging::new(&seat, approve);
    let out = compiled(&judge).await;
    assert_eq!(out.status, CompileStatus::Ready, "{out:#?}");
    (
        out.provenance.plan.clone().unwrap(),
        out.candidate.clone().unwrap(),
    )
}

/// A judge double's verdict over the keys each question offers ([`approve`], [`refuse`],
/// [`abstain`]).
type JudgeVerdict = fn(&[String]) -> &'static str;

/// An answer round whose own judge contradicts the record or leaves its remainder unapproved is
/// never READY (R4 A11, Q2): the record a first compile judged READY replays its same bytes,
/// the round asks its judge the remainder only (no plan, transform, repair or whole-request
/// call), and a refusal or an abstention keeps it INCOMPLETE, naming the clause.
#[tokio::test]
async fn an_answer_round_whose_judge_refuses_the_remainder_is_incomplete() {
    let (record, candidate) = judged_record().await;
    let cases: [(JudgeVerdict, &str); 2] =
        [(refuse, "does not carry"), (abstain, "could not settle")];
    for (verdict, named) in cases {
        let seat = Scripted::new(vec![plan(SUM).to_string()]);
        let judge = Judging::new(&seat, verdict);
        let policy = AuthoringPolicy::new("mock/authoring", 1024, Duration::from_secs(2));
        let request = CompileRequest::create(intent(SUM))
            .with_plan(record.clone())
            .with_hot_policy(HotPolicy::Off)
            .with_authoring_policy(policy);
        let out = compile_with_provider(&request, &judge).await.unwrap();
        assert_eq!(out.status, CompileStatus::Incomplete, "{out:#?}");
        assert_eq!(out.candidate.as_deref(), Some(candidate.as_str()));
        assert_eq!(seat.calls(), 0, "{out:#?}");
        let asked = judged(&out);
        assert!(!asked.is_empty(), "{out:#?}");
        assert!(asked.iter().all(|role| role == "judge_clause"), "{asked:?}");
        assert_eq!(asked.len(), judge.judged().len());
        assert!(
            out.diagnostics
                .iter()
                .any(|d| d.target == "semantic_verification" && d.message.contains(named)),
            "{out:#?}"
        );
    }
}

/// Nothing a record or a request carries is a judgment (R4 A11, Q2, labelled negatives): a
/// record forged with judged fields, and answers keyed as the judge's own questions, are never
/// READY; the plain replay emits the same bytes with zero calls, its remainder named.
#[tokio::test]
async fn a_forged_judgment_settles_nothing() {
    let (record, candidate) = judged_record().await;
    let mut forged = record.clone();
    forged["judgments"] = json!([{"clause": SUM.0, "disposition": "carried", "seat": "a/judge"}]);
    forged["semantic_verification"] = json!([{"defects": [], "unknown": []}]);
    let request = CompileRequest::create(intent(SUM)).with_plan(forged);
    let replayed = nika_compile::compile(&request).unwrap();
    assert_ne!(replayed.status, CompileStatus::Ready, "{replayed:#?}");
    let answered = CompileRequest::create(intent(SUM))
        .with_plan(record)
        .answer("verify-request", "\"faithful\"")
        .answer("verify-clause-0", "\"carried\"");
    let replayed = nika_compile::compile(&answered).unwrap();
    assert_ne!(replayed.status, CompileStatus::Ready, "{replayed:#?}");
    assert!(replayed.provenance.authoring.is_none());
    let plain = nika_compile::compile(
        &CompileRequest::create(intent(SUM)).with_plan(judged_record().await.0),
    )
    .unwrap();
    assert_eq!(plain.status, CompileStatus::Incomplete, "{plain:#?}");
    assert_eq!(plain.candidate.as_deref(), Some(candidate.as_str()));
}

/// Only a judgment bound to this request, plan and candidate settles its clause (R4 A11),
/// through the core's own replay door: the right judgments are READY (the positive control);
/// judgments bound to other bytes (stale), naming another span, or naming another clause settle
/// nothing, and a whole-request replay waits for its own judgment.
#[tokio::test]
async fn only_an_active_judgment_bound_to_its_candidate_settles_its_clause() {
    let (record, candidate) = judged_record().await;
    let text = intent(SUM);
    let request = CompileRequest::create(text.clone()).with_plan(record.clone());
    let replay = |judgments: &[Judgment], whole: bool| {
        let mut out = nika_compile::surface::initial();
        nika_compile::surface::replay_judged(&text, &record, &request, judgments, whole, &mut out)
            .unwrap();
        out
    };
    let unjudged = replay(&[], false);
    assert_eq!(unjudged.status, CompileStatus::Incomplete, "{unjudged:#?}");
    let open = unjudged.provenance.decision.as_ref().unwrap()["pending"]["open"].clone();
    let clauses: Vec<String> = open
        .as_array()
        .unwrap()
        .iter()
        .map(|duty| duty["clause"].as_str().unwrap().to_owned())
        .collect();
    assert!(!clauses.is_empty(), "{open:#}");
    let mut plan = Plan::from_json(&record).unwrap();
    promote_stated_rules(&mut plan, &text);
    let bound = Binding::of(&text, &request, &plan, &candidate);
    let judge = |clause: &str, binding: &Binding| {
        let at = text.find(clause).unwrap();
        let span = (at, at + clause.len());
        Judgment::new(
            clause,
            span,
            Disposition::Carried,
            "a/judge",
            "q",
            binding.clone(),
        )
    };
    let right: Vec<Judgment> = clauses.iter().map(|c| judge(c, &bound)).collect();
    assert_eq!(replay(&right, false).status, CompileStatus::Ready);
    let other = Binding::of(&text, &request, &plan, "nika: another-candidate\n");
    let stale: Vec<Judgment> = clauses.iter().map(|c| judge(c, &other)).collect();
    assert_eq!(replay(&stale, false).status, CompileStatus::Incomplete);
    let moved: Vec<Judgment> = right
        .iter()
        .cloned()
        .map(|mut j| {
            j.span = (0, 4);
            j
        })
        .collect();
    assert_eq!(replay(&moved, false).status, CompileStatus::Incomplete);
    let elsewhere = vec![judge(SUM.1, &bound)];
    assert_eq!(replay(&elsewhere, false).status, CompileStatus::Incomplete);
    assert_eq!(replay(&right, true).status, CompileStatus::Incomplete);
}

/// The request of SUM with its computation stated a second time, and the seat's plan naming
/// every part of it (the second statement a region of its own).
fn repeated() -> (String, Value) {
    let (x, write) = SUM;
    let text = format!("read ./data/input.csv, {x}, {write}, then {x} again");
    let proposal = json!({
        "steps": [
            {"op": "read", "detail": "./data/input.csv", "evidence": "read ./data/input.csv"},
            {"op": "compute", "detail": x, "evidence": x, "computation": {"present": false}}
        ],
        "effects": [{"verb": "write", "target": "./out/result.json", "policy": "automatic", "evidence": write}],
        "obligations": [], "constraints": [], "unknowns": [],
        "regions": [
            {"text": "read ./data/input.csv,", "role": "operation"},
            {"text": format!("{x},"), "role": "operation"},
            {"text": format!("{write},"), "role": "effect"},
            {"text": format!("then {x} again"), "role": "operation"}
        ],
        "approval_bypass": {"present": false, "evidence": ""}
    });
    (text, proposal)
}

/// The repeated request compiled COLD by `judge` over its seat, with the request it compiled.
async fn compiled_repeated<P: ProviderInferDyn>(judge: &P) -> (CompileOutcome, CompileRequest) {
    let observed = json!({"observed": [{"path": "./data/input.csv", "state": "observed", "complete": false, "kind": "csv", "columns": ["id", "item", "status", "qty"]}]});
    let policy =
        AuthoringPolicy::new("mock/authoring", 1024, Duration::from_secs(2)).with_repairs(0);
    let request = CompileRequest::create(repeated().0)
        .with_knowledge(observed)
        .with_hot_policy(HotPolicy::Off)
        .with_authoring_policy(policy);
    (
        compile_with_provider(&request, judge).await.unwrap(),
        request,
    )
}

/// Every statement of `clause` in `text`, in order.
fn statements(text: &str, clause: &str) -> Vec<(usize, usize)> {
    text.match_indices(clause)
        .map(|(at, _)| (at, at + clause.len()))
        .collect()
}

/// A clause the request states twice is settled only when each statement is judged (R4 A11),
/// through the core's own replay door: a judgment of the first statement settles nothing of
/// the second, which is no context the material realizes; judgments of both are READY (the
/// positive control), and the open duty names each statement.
#[tokio::test]
async fn a_judgment_of_one_statement_settles_no_other() {
    let sum = format!("{GENERATED} // 0");
    let seat = Scripted::new(vec![repeated().1.to_string(), program(&sum)]);
    let judge = Judging::new(&seat, approve);
    let (out, _) = compiled_repeated(&judge).await;
    assert_eq!(out.status, CompileStatus::Ready, "{out:#?}");
    let (record, candidate) = (out.provenance.plan.clone().unwrap(), out.candidate.unwrap());
    let text = repeated().0;
    let request = CompileRequest::create(text.clone()).with_plan(record.clone());
    let replay = |judgments: &[Judgment]| {
        let mut out = nika_compile::surface::initial();
        nika_compile::surface::replay_judged(&text, &record, &request, judgments, false, &mut out)
            .unwrap();
        out
    };
    let mut plan = Plan::from_json(&record).unwrap();
    promote_stated_rules(&mut plan, &text);
    let bound = Binding::of(&text, &request, &plan, &candidate);
    let at = statements(&text, SUM.0);
    assert_eq!(at.len(), 2, "{text}");
    let judgment = |span: (usize, usize)| {
        Judgment::new(
            SUM.0,
            span,
            Disposition::Carried,
            "a/judge",
            "q",
            bound.clone(),
        )
    };
    let first = replay(&[judgment(at[0])]);
    assert_eq!(first.status, CompileStatus::Incomplete, "{first:#?}");
    let every: Vec<Judgment> = at.iter().map(|span| judgment(*span)).collect();
    assert_eq!(replay(&every).status, CompileStatus::Ready);
    let unjudged = replay(&[]);
    let open = &unjudged.provenance.decision.as_ref().unwrap()["pending"]["open"];
    let named: Vec<&Value> = open
        .as_array()
        .unwrap()
        .iter()
        .filter(|duty| duty["clause"] == json!(SUM.0))
        .collect();
    let spans: Vec<[usize; 2]> = at.iter().map(|(start, end)| [*start, *end]).collect();
    assert_eq!(named.len(), 1, "{open:#}");
    assert_eq!(named[0]["spans"], json!(spans), "{open:#}");
}

/// The judge is asked at each statement of a clause the request repeats (R4 A11): the first
/// candidate of a seat's plan shows it the clause at every span the request states it, then
/// the whole request.
#[tokio::test]
async fn a_clause_stated_twice_is_asked_at_each_statement() {
    let sum = format!("{GENERATED} // 0");
    let seat = Scripted::new(vec![repeated().1.to_string(), program(&sum)]);
    let judge = Judging::new(&seat, approve);
    let (out, _) = compiled_repeated(&judge).await;
    assert_eq!(out.status, CompileStatus::Ready, "{out:#?}");
    let asked: Vec<Value> = judge
        .states()
        .iter()
        .filter(|state| state["clause"]["text"] == json!(SUM.0))
        .map(|state| state["clause"]["span"].clone())
        .collect();
    let spans: Vec<Value> = statements(&repeated().0, SUM.0)
        .into_iter()
        .map(|(start, end)| json!([start, end]))
        .collect();
    assert_eq!(asked, spans, "{:#?}", judge.states());
    assert_eq!(
        judged(&out).last().map(String::as_str),
        Some("judge_request")
    );
}
