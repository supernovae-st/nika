// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>
//! An answer round replays the plan the previous round produced: the same candidate, the
//! same questions, zero authoring calls. A model's plan (COLD, WARM) is READY only once the
//! round's own judge carries the whole request over the replayed bytes; the reader's own plan
//! asks no judge. A plan that does not parse, is not anchored in the intent or still carries
//! unknowns is a finding, never a candidate.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
use nika_compile::{
    AuthoringCognition, AuthoringPolicy, CompileRequest, CompileStatus, DiagnosticKind, Strategy,
    compile, intent_sha256, outcome_document,
};
use nika_compile_cognition::{
    Cognition, compile_with_cognition, compile_with_provider,
    decide::{ChoiceAnswer, ChoiceFuture, ChoiceQuestion, DecisionSeat},
};
use nika_kernel::ai::provider::{
    ContentBlock, InferRequest, InferResponse, ProviderError, ProviderInferDyn, StopReason,
    TokenUsage,
};
use serde_json::{Value, json};
use std::{
    sync::atomic::{AtomicU32, Ordering},
    time::Duration,
};

/// The answer rounds of a COLD record, kept beside this file to bound its size.
#[path = "compile_replay/cold.rs"]
mod cold;
mod common;

/// A clause the deterministic reader cannot consume ("harmonise le ton") forces COLD.
const COLD_INTENT: &str = "Pour chaque demande, consulte le client, classe le problème, puis harmonise le ton de la réponse. Demande un accord humain avant le remboursement.";
/// Every clause explicit: the deterministic reader admits it without any seat.
const HOT_INTENT: &str = "Read ./notes/brief.md, summarize it in three bullets, and write the summary to ./out/summary.md";

struct Provider {
    text: String,
    calls: AtomicU32,
}
impl Provider {
    fn new(plan: &Value) -> Self {
        Self {
            text: plan.to_string(),
            calls: AtomicU32::new(0),
        }
    }
}
impl ProviderInferDyn for Provider {
    async fn infer(&self, _: InferRequest) -> Result<InferResponse, ProviderError> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        Ok(InferResponse::new(
            vec![ContentBlock::Text {
                text: self.text.clone(),
            }],
            TokenUsage::new(120, 90),
            StopReason::EndTurn,
        ))
    }
}

/// A seat that must never be asked during a replay.
struct Seat(AtomicU32);
impl DecisionSeat for Seat {
    fn name(&self) -> &'static str {
        "double/seat"
    }
    fn choose<'a>(&'a self, question: &'a ChoiceQuestion) -> ChoiceFuture<'a> {
        self.0.fetch_add(1, Ordering::SeqCst);
        let key = question.options[0].key.clone();
        Box::pin(async move { Ok(ChoiceAnswer::new(key, "double/seat")) })
    }
}

fn proposal() -> Value {
    json!({"steps":[{"op":"lookup","detail":"le client","evidence":"consulte le client"},{"op":"classify","detail":"le problème","evidence":"classe le problème"},{"op":"draft","detail":"la réponse","evidence":"harmonise le ton de la réponse"}],
           "effects":[{"verb":"refund","target":"le remboursement","policy":"human_first","evidence":"Demande un accord humain avant le remboursement"}],
           "obligations":[],"constraints":[],"unknowns":[]})
}
fn policy() -> AuthoringPolicy {
    AuthoringPolicy::new("mock/authoring", 1024, Duration::from_secs(2))
}
fn answered(request: CompileRequest) -> CompileRequest {
    request
        .answer("model", r#""mock/echo""#)
        .answer("const.customer_directory", r#""customers.json""#)
        .answer(
            "const.refund_policy",
            r#"{"cap":100,"currency":"EUR","criteria":"unused purchase within 14 days"}"#,
        )
        .answer(
            "const.refund_endpoint",
            r#""https://refund.example.invalid/refunds""#,
        )
}
fn keys(out: &nika_compile::CompileOutcome) -> Vec<&str> {
    out.questions.iter().map(|q| q.key.as_str()).collect()
}
fn route(out: &nika_compile::CompileOutcome) -> Value {
    outcome_document(out)["provenance"]["decision"]["route"].clone()
}

#[test]
fn a_recorded_hot_plan_replays_identically() {
    let fresh = compile(&CompileRequest::create(HOT_INTENT)).unwrap();
    assert_eq!(fresh.provenance.strategy, Some(Strategy::Hot), "{fresh:#?}");
    let plan = fresh.provenance.plan.clone().unwrap();
    assert_eq!(plan["strategy"], "hot");
    assert_eq!(
        outcome_document(&fresh)["provenance"]["decision"]["intent_sha256"],
        intent_sha256(HOT_INTENT)
    );
    let replayed = compile(&CompileRequest::create(HOT_INTENT).with_plan(plan.clone())).unwrap();
    assert_eq!(replayed.status, fresh.status);
    assert_eq!(replayed.candidate, fresh.candidate);
    assert_eq!(keys(&replayed), keys(&fresh));
    assert_eq!(replayed.provenance.strategy, Some(Strategy::Hot));
    assert_eq!(route(&replayed), json!(["replayed plan"]));
    let ready = compile(
        &CompileRequest::create(HOT_INTENT)
            .with_plan(plan)
            .answer("model", r#""mock/echo""#),
    )
    .unwrap();
    assert_eq!(ready.status, CompileStatus::Ready, "{ready:#?}");
    let expected =
        compile(&CompileRequest::create(HOT_INTENT).answer("model", r#""mock/echo""#)).unwrap();
    assert_eq!(ready.candidate, expected.candidate);
}

#[test]
fn a_malformed_or_unanchored_plan_is_a_finding_never_a_candidate() {
    let good = compile(&CompileRequest::create(HOT_INTENT))
        .unwrap()
        .provenance
        .plan
        .unwrap();
    let mut unanchored = good.clone();
    unanchored["operations"][0]["evidence"] = json!("evidence the request never wrote");
    let mut unknown_op = good.clone();
    unknown_op["operations"][0]["op"] = json!("teleport");
    let mut unknown_role = good.clone();
    unknown_role["bindings"] = json!([{"role":"invented","literal":"x"}]);
    let mut bad_policy = good.clone();
    bad_policy["effects"][0]["policy"] = json!("whenever");
    let mut bad_obligation = good.clone();
    bad_obligation["obligations"] = json!([{"kind":"retry_bound","value":null,"evidence":"Read"}]);
    for plan in [
        json!("not an object"),
        json!({}),
        json!({"operations":"nope"}),
        json!({"operations":[{"op":"read"}],"effects":[],"obligations":[],"bindings":[],"constraints":[],"unknowns":[],"trigger":null}),
        unanchored,
        unknown_op,
        unknown_role,
        bad_policy,
        bad_obligation,
    ] {
        let out = compile(&CompileRequest::create(HOT_INTENT).with_plan(plan.clone())).unwrap();
        assert_eq!(out.status, CompileStatus::Incomplete, "{plan}");
        assert!(out.candidate.is_none(), "{plan}: {out:#?}");
        assert!(
            out.diagnostics
                .iter()
                .any(|d| { d.kind == DiagnosticKind::Unknown && d.target == "recorded_plan" }),
            "{plan}: {out:#?}"
        );
        assert_eq!(route(&out), json!(["replayed plan"]), "{plan}");
    }
}

#[test]
fn a_recorded_plan_with_unknowns_is_never_assembled() {
    let mut plan = compile(&CompileRequest::create(HOT_INTENT))
        .unwrap()
        .provenance
        .plan
        .unwrap();
    plan["unknowns"] = json!(["Use a previous approval for a different amount"]);
    let out = compile(
        &CompileRequest::create(HOT_INTENT)
            .with_plan(plan)
            .answer("model", r#""mock/echo""#),
    )
    .unwrap();
    assert_eq!(out.status, CompileStatus::Incomplete);
    assert!(out.candidate.is_none(), "{out:#?}");
    assert!(keys(&out).contains(&"intent.clarification"), "{out:#?}");
}

#[test]
fn the_plan_record_round_trips_every_element() {
    // Bindings, constraints, categories, obligations with a value and a trigger all survive.
    let intent = "Read ./notes/brief.md, classify it as urgent or routine, and write the verdict to ./out/verdict.md";
    let fresh = compile(&CompileRequest::create(intent)).unwrap();
    let plan = fresh.provenance.plan.clone().expect("plan");
    let replayed = compile(&CompileRequest::create(intent).with_plan(plan.clone())).unwrap();
    assert_eq!(replayed.provenance.plan, Some(plan.clone()));
    assert_eq!(replayed.candidate, fresh.candidate);
    let mut with_extras = plan;
    with_extras["obligations"] = json!([{"kind":"retry_bound","value":3,"evidence":"Read"},{"kind":"dedup","value":null,"evidence":"classify"}]);
    with_extras["constraints"] = json!(["never infer the priority"]);
    with_extras["trigger"] = json!("every morning");
    with_extras["bindings"]
        .as_array_mut()
        .unwrap()
        .push(json!({"role":"timezone","literal":"Europe/Paris"}));
    let out = compile(&CompileRequest::create(intent).with_plan(with_extras.clone())).unwrap();
    let recorded = out.provenance.plan.expect("plan");
    for key in [
        "operations",
        "effects",
        "obligations",
        "bindings",
        "constraints",
        "unknowns",
        "trigger",
    ] {
        assert_eq!(recorded[key], with_extras[key], "{key}");
    }
}

#[test]
fn intent_sha_folds_apostrophes_and_is_hex() {
    let straight = intent_sha256("Lis l'avis, puis résume-le");
    let curly = intent_sha256("Lis l’avis, puis résume-le");
    assert_eq!(straight, curly);
    assert_eq!(straight.len(), 64);
    assert!(straight.chars().all(|c| c.is_ascii_hexdigit()));
    assert_ne!(straight, intent_sha256("Lis l'avis"));
}

#[test]
fn a_replayed_document_keeps_the_generation_one_shape() {
    let plan = compile(&CompileRequest::create(HOT_INTENT))
        .unwrap()
        .provenance
        .plan
        .unwrap();
    let out = compile(&CompileRequest::create(HOT_INTENT).with_plan(plan)).unwrap();
    let document = outcome_document(&out);
    let keys: Vec<_> = document.as_object().unwrap().keys().cloned().collect();
    assert_eq!(
        keys,
        [
            "candidate",
            "check_preview",
            "compile_version",
            "diagnostics",
            "provenance",
            "questions",
            "requested_boundary",
            "requested_trigger",
            "status"
        ]
    );
    let provenance: Vec<_> = document["provenance"]
        .as_object()
        .unwrap()
        .keys()
        .filter(|k| !matches!(k.as_str(), "strategy" | "plan" | "decision"))
        .cloned()
        .collect();
    // `suggested_file` joined the provenance additively (a file name for whoever saves the
    // candidate); the generation stays one.
    assert_eq!(
        provenance,
        [
            "cognition",
            "compiler_version",
            "skeleton",
            "spec_pin",
            "suggested_file"
        ]
    );
}

// ── Slice C: an accepted sketch replays its own graph and fills, never a stored source ────────
//
// Synthetic sentinels; scripted providers are mechanics evidence only. Reassembly is not
// semantic satisfaction: a replayed candidate stays pending on its whole request until a
// judgment made in that round settles it, and that judgment is a counted call.

const COPIES: &str = "Copie ./alpha.txt dans ./out/alpha.txt et ./beta.txt dans ./out/beta.txt. Demande mon accord une seule fois avant les deux écritures. Nomme les résultats alpha et beta.";
/// A summary needs a model: the record's candidate asks for one (`model`).
const SUMMARY: &str =
    "Lis ./notes.md, résume-le en trois points et écris le résumé dans ./out/resume.md.";

fn copy_task(id: &str, tool: &str, extra: &Value) -> Value {
    let mut task = json!({"id": id, "verb": "invoke", "tool": tool, "purpose": id});
    for (key, value) in extra.as_object().unwrap() {
        task[key] = value.clone();
    }
    task
}

/// The accepted graph: two reads, one approval, two gated writes, two named results.
fn copies_sketch() -> Value {
    let write = |id: &str, path: &str, from: &str| {
        copy_task(
            id,
            "nika:write",
            &json!({"writes": [path], "with": [{"name": "text", "from": from}], "gated_by": "approve"}),
        )
    };
    json!({"name": "two-copies", "tasks": [
        copy_task("read_alpha", "nika:read", &json!({"reads": ["./alpha.txt"]})),
        copy_task("read_beta", "nika:read", &json!({"reads": ["./beta.txt"]})),
        copy_task("approve", "nika:prompt", &json!({})),
        write("write_alpha", "./out/alpha.txt", "read_alpha"),
        write("write_beta", "./out/beta.txt", "read_beta"),
    ], "outputs": [{"name": "alpha", "from": "write_alpha"}, {"name": "beta", "from": "write_beta"}]})
}

fn copies_fills() -> Value {
    json!([{"task": "approve", "field": "args.message", "value": "Écrire les deux copies ?"}])
}

fn summary_sketch() -> Value {
    json!({"name": "resume", "tasks": [
        copy_task("read_notes", "nika:read", &json!({"reads": ["./notes.md"]})),
        {"id": "summarize", "verb": "infer", "purpose": "trois points",
         "with": [{"name": "notes", "from": "read_notes"}]},
        copy_task("write_summary", "nika:write",
            &json!({"writes": ["./out/resume.md"], "with": [{"name": "text", "from": "summarize"}]})),
    ]})
}

fn summary_fills() -> Value {
    json!([{"task": "summarize", "field": "prompt", "value": "Résume en trois points : {{notes}}"}])
}

fn policy_c(native: nika_compile::NativeMode, repairs: u32) -> AuthoringPolicy {
    AuthoringPolicy::new("mock/authoring", 4096, Duration::from_secs(2))
        .with_native(native)
        .with_repairs(repairs)
}

fn answer_of(sketch: &Value, gaps: &[&str]) -> String {
    let mut answer = sketch.clone();
    answer["questions"] = json!([]);
    answer["gaps"] = json!(gaps);
    answer["notes"] = json!("graph");
    answer.to_string()
}

fn fills_of(fills: &Value) -> String {
    json!({"fills": fills, "notes": "fills"}).to_string()
}

/// The sketch door's answers in order, then an approving judge of the whole request.
async fn authored_with(
    intent: &str,
    replies: Vec<String>,
    repairs: u32,
) -> (nika_compile::CompileOutcome, u32) {
    let provider = common::Rotating::new(replies);
    let judged = common::Judged::approving(&provider);
    let request = CompileRequest::create(intent)
        .with_authoring_policy(policy_c(nika_compile::NativeMode::Sketch, repairs));
    let out = compile_with_provider(&request, &judged).await.unwrap();
    (out, provider.calls.load(Ordering::SeqCst))
}

async fn authored() -> nika_compile::CompileOutcome {
    let replies = vec![answer_of(&copies_sketch(), &[]), fills_of(&copies_fills())];
    authored_with(COPIES, replies, 0).await.0
}

/// A provider that must never be asked: every replay that refuses or reconstructs makes no call.
struct Never(AtomicU32);
impl ProviderInferDyn for Never {
    async fn infer(&self, _: InferRequest) -> Result<InferResponse, ProviderError> {
        self.0.fetch_add(1, Ordering::SeqCst);
        Err(ProviderError::Other {
            reason: "no call is expected".to_owned(),
        })
    }
}

/// The cognition replay of `record` for `request` under a policy, through a provider that must
/// not be asked: the outcome and the calls it made.
async fn replay_with_cognition(
    request: CompileRequest,
    record: Value,
) -> (nika_compile::CompileOutcome, u32) {
    let never = Never(AtomicU32::new(0));
    let request = request
        .with_authoring_policy(policy_c(nika_compile::NativeMode::Sketch, 0))
        .with_plan(record);
    let out = compile_with_provider(&request, &never).await.unwrap();
    (out, never.0.load(Ordering::SeqCst))
}

/// Whether the route names judgments the host supplied (their origin, not their acceptance).
fn host_judged(out: &nika_compile::CompileOutcome) -> bool {
    (out.provenance.decision.as_ref()).is_some_and(|d| {
        d["route"]
            .to_string()
            .contains("judgments supplied by the host")
    })
}

fn refused_replay(out: &nika_compile::CompileOutcome) -> bool {
    out.candidate.is_none()
        && out.diagnostics.iter().any(|d| {
            d.target == "recorded_plan"
                && d.message
                    .starts_with("The recorded sketch cannot be replayed")
        })
}

/// The bytes a record's graph and fills emit, the way any forger could compute them.
fn emitted_bytes(record: &Value) -> String {
    use nika_compile_fidelity::sketch as ir;
    let sketch = ir::Sketch::from_json(&record["sketch"]).unwrap();
    let fills = ir::fills_from_json(&json!({"fills": record["fills"]})).unwrap();
    serde_yaml_bw::to_string(&ir::complete_document(&sketch, &fills).unwrap()).unwrap()
}

/// Re-sign a mutated record so that every digest it carries is consistent again: a hostile
/// record a forger can always produce. Its candidate must still never become authority.
fn forged(mut record: Value, request: &CompileRequest) -> Value {
    let source = emitted_bytes(&record);
    record["assembly_sha256"] = json!(nika_compile::surface::sha256(&source));
    let view = json!({"source": source, "questions": record["settlement"]["questions"],
                      "gaps": record["settlement"]["gaps"], "trigger": record["settlement"]["trigger"]});
    let mut out = nika_compile::surface::initial();
    nika_compile::surface::native_apply(&view, request, &mut out);
    record["final"]["candidate_sha256"] =
        json!(out.candidate.as_deref().map(nika_compile::surface::sha256));
    record
}

#[tokio::test]
async fn an_accepted_sketch_replays_its_graph_and_fills_and_never_its_stored_source() {
    let out = authored().await;
    assert_eq!(out.status, CompileStatus::Ready, "{:?}", out.diagnostics);
    let candidate = out.candidate.clone().unwrap();
    let mut record = out.provenance.plan.clone().unwrap();
    // A stored source is an observation at most: rewriting it must not reach the emission.
    let hostile = candidate.replace("./out/beta.txt", "./out/elsewhere.txt");
    assert_ne!(hostile, candidate);
    record["source"] = json!(hostile);
    let replayed = compile(&CompileRequest::create(COPIES).with_plan(record)).unwrap();
    assert_eq!(
        replayed.candidate.as_deref(),
        Some(candidate.as_str()),
        "the replay emits the accepted graph and fills: {:?}",
        replayed.diagnostics
    );
    // Reconstruction is not judgment: zero calls, the whole request still pending.
    assert_eq!(replayed.status, CompileStatus::Incomplete);
    let open = &replayed.provenance.decision.as_ref().unwrap()["pending"]["open"];
    assert_eq!(open.as_array().map(Vec::len), Some(1), "{open}");
}

#[tokio::test]
async fn the_record_keeps_the_request_basis_and_the_accepted_pair_not_a_legacy_source() {
    let out = authored().await;
    let record = out.provenance.plan.clone().unwrap();
    assert_ne!(
        record.get("strategy").and_then(Value::as_str),
        Some("native"),
        "a semantic record must not read as a legacy native-source record"
    );
    let mut keys: Vec<&str> = record
        .as_object()
        .unwrap()
        .keys()
        .map(String::as_str)
        .collect();
    keys.sort_unstable();
    assert_eq!(
        keys,
        [
            "assembly_sha256",
            "basis",
            "fills",
            "final",
            "intent_sha256",
            "lowering",
            "semantic_record",
            "settlement",
            "sketch",
            "source",
            "source_is"
        ],
        "a closed record: no judgment, journal or plan vocabulary"
    );
    assert_eq!(
        record["sketch"],
        copies_sketch(),
        "the accepted graph, as accepted"
    );
    assert_eq!(
        record["fills"],
        copies_fills(),
        "the accepted fills, exactly"
    );
    assert_eq!(
        record["basis"]["caller"]["input"], COPIES,
        "the caller's own words"
    );
    let read = &record["basis"]["read"];
    assert_eq!(read["effective"], COPIES);
    let seen = nika_compile_reader::lexicon::read(COPIES).seen;
    assert_eq!(
        read["seen"],
        json!(seen),
        "every clause occurrence, in order"
    );
    let request = CompileRequest::create(COPIES);
    assert_eq!(
        *read,
        nika_compile::surface::semantic::request_basis(COPIES, &request),
        "the basis is the request's own reading, recomputable without any seat"
    );
}

/// Every mutation the table refuses: of the caller, the request, the answers, the world, the
/// graph, the fills, the versions, the identities and the closed format.
#[allow(clippy::too_many_lines)] // one table: one row per mutation, each a single refusal case
fn mutations(record: &Value) -> Vec<(&'static str, CompileRequest, Value)> {
    let request = CompileRequest::create(COPIES);
    let mutate = |f: &dyn Fn(&mut Value)| {
        let mut r = record.clone();
        f(&mut r);
        r
    };
    vec![
        (
            "caller words",
            request.clone(),
            mutate(&|r| r["basis"]["caller"]["input"] = json!("Copie ./alpha.txt.")),
        ),
        (
            "effective words",
            request.clone(),
            mutate(&|r| r["basis"]["read"]["effective"] = json!("Copie ./alpha.txt.")),
        ),
        (
            "caller initial answers",
            request.clone(),
            mutate(&|r| r["basis"]["caller"]["answers"] = json!({"model": "\"x/y\""})),
        ),
        (
            "bound answers",
            request.clone(),
            mutate(&|r| r["final"]["answers"] = json!({"model": "\"x/y\""})),
        ),
        (
            "observed world",
            request.clone().with_knowledge(json!({"observed": [
                {"path": "./alpha.txt", "state": "absent"}
            ]})),
            record.clone(),
        ),
        (
            "request words",
            CompileRequest::create("Copie ./alpha.txt dans ./out/alpha.txt."),
            record.clone(),
        ),
        (
            "replacement request",
            request.clone().answer(
                "intent.clarification",
                r#""Copie ./alpha.txt dans ./out/x.txt.""#,
            ),
            record.clone(),
        ),
        (
            "an edge",
            request.clone(),
            mutate(&|r| r["sketch"]["tasks"][3]["with"][0]["from"] = json!("read_beta")),
        ),
        (
            "an output",
            request.clone(),
            mutate(&|r| r["sketch"]["outputs"][1]["from"] = json!("write_alpha")),
        ),
        (
            "a control",
            request.clone(),
            mutate(&|r| r["sketch"]["tasks"][0]["max_turns"] = json!(3)),
        ),
        (
            "a fill",
            request.clone(),
            mutate(&|r| r["fills"][0]["value"] = json!("Écrire ailleurs ?")),
        ),
        (
            "the record version",
            request.clone(),
            mutate(&|r| r["semantic_record"] = json!(2)),
        ),
        (
            "the lowering version",
            request.clone(),
            mutate(&|r| r["lowering"] = json!(2)),
        ),
        (
            "the assembly identity",
            request.clone(),
            mutate(&|r| r["assembly_sha256"] = json!("0".repeat(64))),
        ),
        (
            "the final identity",
            request.clone(),
            mutate(&|r| r["final"]["candidate_sha256"] = json!("0".repeat(64))),
        ),
        (
            "an unknown record key",
            request.clone(),
            mutate(&|r| r["zz_unknown_key"] = json!(1)),
        ),
        (
            "a legacy native marker",
            request.clone(),
            mutate(&|r| r["strategy"] = json!("native")),
        ),
        (
            "an unknown sketch root key",
            request.clone(),
            mutate(&|r| r["sketch"]["zz_unknown_key"] = json!(1)),
        ),
        (
            "an unknown question key",
            request.clone(),
            mutate(&|r| r["settlement"]["questions"] = json!([{"key": "k", "zz_unknown_key": 1}])),
        ),
    ]
}

#[tokio::test]
async fn every_mutation_of_the_request_or_the_record_refuses_without_source_or_model() {
    let record = authored().await.provenance.plan.unwrap();
    let cases = mutations(&record);
    for (label, request, record) in cases {
        let (out, calls) = replay_with_cognition(request.clone(), record.clone()).await;
        assert_eq!(calls, 0, "{label}: no model is asked");
        assert!(
            refused_replay(&out),
            "{label}: {:?} {:?}",
            out.status,
            out.diagnostics
        );
        let text = format!("{:?}", out.diagnostics);
        assert!(
            !text.contains("zz_unknown_key") && !text.contains("ailleurs"),
            "{label}: {text}"
        );
        // The core replay refuses the same mutations of the record (the caller's own words are
        // judged at the cognition entry).
        if !label.starts_with("caller") && label != "replacement request" {
            let core = compile(&request.with_plan(record)).unwrap();
            assert!(
                refused_replay(&core),
                "core {label}: {:?}",
                core.diagnostics
            );
        }
    }
}

#[tokio::test]
async fn no_relevant_world_facts_replay_but_still_need_a_fresh_judgment() {
    let out = authored().await;
    let record = out.provenance.plan.unwrap();
    for world in [
        json!({"observed": []}),
        json!({"observed": [{"path": "./unrelated.txt", "state": "absent"}]}),
    ] {
        let request = CompileRequest::create(COPIES).with_knowledge(world);
        assert_eq!(
            nika_compile::surface::semantic::request_basis(COPIES, &request),
            record["basis"]["read"]
        );
        let core = compile(&request.clone().with_plan(record.clone())).unwrap();
        assert_eq!(core.candidate, out.candidate);
        assert_eq!(core.status, CompileStatus::Incomplete);
        let (replayed, calls, judgments) = judged_replay(request, record.clone()).await;
        assert_eq!(replayed.candidate, out.candidate);
        assert_eq!(replayed.status, CompileStatus::Ready);
        assert_eq!((calls, judgments), (0, 1));
    }
}

#[tokio::test]
async fn a_self_consistent_forged_record_is_reassembled_but_never_authority() {
    let record = authored().await.provenance.plan.unwrap();
    let request = CompileRequest::create(COPIES);
    let mut hostile = record.clone();
    hostile["fills"][0]["value"] = json!("Écrire les deux copies maintenant ?");
    let hostile = forged(hostile, &request);
    // The core reassembles exactly what the forged pair states, and nothing is READY on it.
    let core = compile(&request.clone().with_plan(hostile.clone())).unwrap();
    assert!(core.candidate.as_deref().unwrap().contains("maintenant"));
    assert_eq!(
        core.status,
        CompileStatus::Incomplete,
        "{:?}",
        core.diagnostics
    );
    assert!(
        core.check_preview.is_some(),
        "Check still judges the rebuilt bytes"
    );
    // The cognition replay asks the round's judge, counted calls: a refusing judge (the request
    // unfaithful, each of its three parts, asked alone, missing, an operation no task performs)
    // keeps it from READY whatever the record's digests say.
    let choices = std::iter::once("unfaithful").chain(["missing", "omitted"].repeat(3));
    let refusing =
        common::Rotating::new(choices.map(|c| json!({"choice": c}).to_string()).collect());
    let request = request
        .with_authoring_policy(policy_c(nika_compile::NativeMode::Sketch, 0))
        .with_plan(hostile);
    let out = compile_with_provider(&request, &refusing).await.unwrap();
    let calls = refusing.calls.load(Ordering::SeqCst);
    assert_eq!(calls, 7, "the judgment is real calls");
    assert_ne!(out.status, CompileStatus::Ready, "{:?}", out.diagnostics);
    let decision = out.provenance.decision.as_ref().unwrap();
    let parts = [
        "Copie ./alpha.txt dans ./out/alpha.txt et ./beta.txt dans ./out/beta.txt",
        "Demande mon accord une seule fois avant les deux écritures",
        "Nomme les résultats alpha et beta",
    ];
    let verified = &decision["semantic_verification"][0];
    let omitted = "the judge finds no task performing it";
    let notes: Vec<Value> = (parts.iter())
        .map(|part| json!({"defect": part, "note": omitted}))
        .collect();
    let found = (&verified["defects"], &verified["notes"]);
    assert_eq!(found, (&json!(parts), &json!(notes)), "{decision:#}");
    // The judge doubted the whole request of the forged bytes: no record replays them to it.
    assert!(out.provenance.plan.is_none(), "{:?}", out.diagnostics);
    let route = decision["route"].to_string();
    assert!(route.contains("verify: doubted, not replayable"), "{route}");
}

#[tokio::test]
async fn an_effectful_agent_tool_forged_into_a_record_is_refused_before_any_judgment() {
    let record = authored().await.provenance.plan.unwrap();
    let request = CompileRequest::create(COPIES);
    let mut hostile = record.clone();
    hostile["sketch"]["tasks"][0] = json!({"id": "read_alpha", "verb": "agent", "purpose": "read",
        "tools": ["nika:write"], "reads": ["./alpha.txt"]});
    hostile["fills"] = json!([
        {"task": "approve", "field": "args.message", "value": "Écrire les deux copies ?"},
        {"task": "read_alpha", "field": "prompt", "value": "Copie ./alpha.txt"}
    ]);
    let hostile = forged(hostile, &request);
    let (out, calls) = replay_with_cognition(request, hostile).await;
    assert_eq!(
        calls, 0,
        "no judgment is asked of a graph the reach laws refuse"
    );
    assert!(out.candidate.is_none(), "{:?}", out.diagnostics);
    assert!(
        out.diagnostics
            .iter()
            .any(|d| d.message.contains("reach laws")),
        "{:?}",
        out.diagnostics
    );
    let shown = format!("{:?}", out.diagnostics);
    assert!(
        !shown.contains("nika:write") && !shown.contains("read_alpha"),
        "a refusal names no record value: {shown}"
    );
}

#[tokio::test]
async fn the_same_answers_reconstruct_with_zero_calls_and_ready_takes_a_counted_judgment() {
    let out = authored().await;
    let record = out.provenance.plan.clone().unwrap();
    let request = CompileRequest::create(COPIES);
    let core = compile(&request.clone().with_plan(record.clone())).unwrap();
    assert_eq!(core.candidate, out.candidate, "the same bytes");
    assert_eq!(
        core.status,
        CompileStatus::Incomplete,
        "reconstruction is not judgment"
    );
    assert!(!host_judged(&core), "no judgment was supplied");
    assert_eq!(
        core.provenance.plan.as_ref().unwrap()["final"],
        record["final"]
    );
    let provider = common::Rotating::new(vec![]);
    let judged = common::Judged::approving(&provider);
    let replay = request
        .with_authoring_policy(policy_c(nika_compile::NativeMode::Sketch, 0))
        .with_plan(record);
    let judged_out = compile_with_provider(&replay, &judged).await.unwrap();
    assert_eq!(
        judged_out.status,
        CompileStatus::Ready,
        "{:?}",
        judged_out.diagnostics
    );
    assert_eq!(judged_out.candidate, out.candidate);
    assert_eq!(
        provider.calls.load(Ordering::SeqCst),
        0,
        "no authoring call"
    );
    assert_eq!(
        judged.judged.load(Ordering::SeqCst),
        1,
        "one counted whole-request judgment"
    );
    assert!(
        host_judged(&judged_out),
        "READY only with the bound judgment supplied"
    );
}

#[tokio::test]
async fn a_new_named_answer_binds_anew_and_keeps_the_frozen_basis() {
    let replies = vec![
        answer_of(&summary_sketch(), &[]),
        fills_of(&summary_fills()),
    ];
    let (out, _) = authored_with(SUMMARY, replies, 0).await;
    let keys: Vec<&str> = out.questions.iter().map(|q| q.key.as_str()).collect();
    assert_eq!(keys, ["model"], "{:?}", out.diagnostics);
    let record = out.provenance.plan.clone().unwrap();
    assert_eq!(record["final"]["candidate_sha256"], Value::Null);
    let answered =
        CompileRequest::create(SUMMARY).answer("model", r#""mistral/mistral-small-latest""#);
    let first = compile(&answered.clone().with_plan(record.clone())).unwrap();
    let source = first
        .candidate
        .clone()
        .expect("the answered question emits the candidate");
    assert!(source.contains("mistral/mistral-small-latest"));
    let next = first.provenance.plan.clone().unwrap();
    assert_eq!(
        next["basis"], record["basis"],
        "the frozen basis is unchanged"
    );
    assert_eq!(
        next["final"]["answers"]["model"],
        r#""mistral/mistral-small-latest""#
    );
    assert_eq!(
        next["final"]["candidate_sha256"],
        json!(nika_compile::surface::sha256(&source))
    );
    // The same answers on the new binding reproduce the same bytes.
    let again = compile(&answered.clone().with_plan(next.clone())).unwrap();
    assert_eq!(again.candidate.as_deref(), Some(source.as_str()));
    // A changed bound answer, or an answer to no question, refuses.
    let changed = CompileRequest::create(SUMMARY).answer("model", r#""openai/gpt-5.2""#);
    assert!(refused_replay(
        &compile(&changed.with_plan(next.clone())).unwrap()
    ));
    let unasked = answered.answer("permits.net", r#""example.invalid""#);
    assert!(refused_replay(&compile(&unasked.with_plan(next)).unwrap()));
}

#[tokio::test]
async fn the_basis_survives_a_refused_fill_and_its_repair_with_every_occurrence() {
    let intent = format!("{COPIES} Nomme les résultats alpha et beta.");
    let ghost = json!([{"task": "ghost", "field": "args.message", "value": "?"}]);
    let replies = vec![
        answer_of(&copies_sketch(), &[]),
        fills_of(&ghost),
        fills_of(&copies_fills()),
    ];
    let (repaired, calls) = authored_with(&intent, replies, 1).await;
    assert_eq!(
        calls, 3,
        "sketch, refused fill, accepted repair: every call kept"
    );
    let direct = vec![answer_of(&copies_sketch(), &[]), fills_of(&copies_fills())];
    let (direct, _) = authored_with(&intent, direct, 1).await;
    let basis = &repaired.provenance.plan.as_ref().unwrap()["basis"]["read"];
    assert_eq!(
        *basis,
        direct.provenance.plan.as_ref().unwrap()["basis"]["read"]
    );
    let seen = nika_compile_reader::lexicon::read(&intent).seen;
    let repeated = seen
        .iter()
        .filter(|c| c.contains("Nomme les résultats"))
        .count();
    assert_eq!(
        repeated, 2,
        "the reader keeps the repeated clause: {seen:?}"
    );
    assert_eq!(
        basis["seen"],
        json!(seen),
        "and so does the basis, in order"
    );
}

#[tokio::test]
async fn a_gap_stays_an_open_duty_whatever_its_answer() {
    let clause = "harmonise le ton";
    let intent = format!("{COPIES} Puis {clause}.");
    let replies = vec![
        answer_of(&copies_sketch(), &[clause]),
        fills_of(&copies_fills()),
    ];
    let (out, _) = authored_with(&intent, replies, 0).await;
    let record = out.provenance.plan.clone().expect("a record");
    assert_eq!(record["settlement"]["gaps"], json!([clause]));
    let dropped = CompileRequest::create(intent.as_str()).answer("gap.1", r#""drop""#);
    let replayed = compile(&dropped.with_plan(record)).unwrap();
    assert_ne!(
        replayed.status,
        CompileStatus::Ready,
        "{:?}",
        replayed.diagnostics
    );
    assert!(
        replayed
            .diagnostics
            .iter()
            .any(|d| d.message.contains("stays an open duty")),
        "{:?}",
        replayed.diagnostics
    );
}

/// The sketch door for `request` (its policy set here), answered in order, approving judge.
async fn authored_request(
    request: CompileRequest,
    replies: Vec<String>,
) -> nika_compile::CompileOutcome {
    let provider = common::Rotating::new(replies);
    let judged = common::Judged::approving(&provider);
    let request = request.with_authoring_policy(policy_c(nika_compile::NativeMode::Sketch, 0));
    compile_with_provider(&request, &judged).await.unwrap()
}

/// The cognition replay of `record` under an approving judge: the outcome, authoring calls and
/// judgments.
async fn judged_replay(
    request: CompileRequest,
    record: Value,
) -> (nika_compile::CompileOutcome, u32, u32) {
    let provider = common::Rotating::new(vec![]);
    let judged = common::Judged::approving(&provider);
    let request = request
        .with_authoring_policy(policy_c(nika_compile::NativeMode::Sketch, 0))
        .with_plan(record);
    let out = compile_with_provider(&request, &judged).await.unwrap();
    let calls = provider.calls.load(Ordering::SeqCst);
    (out, calls, judged.judged.load(Ordering::SeqCst))
}

fn with_money(intent: &str) -> CompileRequest {
    let spans: Vec<_> = nika_compile::money::directives(intent)
        .unwrap()
        .found
        .into_iter()
        .map(|d| d.span)
        .collect();
    assert!(!spans.is_empty(), "the fixture states money: {intent}");
    CompileRequest::create(intent).with_admitted_money(spans)
}

#[tokio::test]
async fn the_caller_basis_is_read_before_money_is_blanked_and_binds_the_replay() {
    let stated = format!("{COPIES} Budget 2 USD.");
    let replies = vec![answer_of(&copies_sketch(), &[]), fills_of(&copies_fills())];
    let out = authored_request(with_money(&stated), replies).await;
    let record = out.provenance.plan.clone().expect("a record");
    let basis = &record["basis"];
    assert_eq!(
        basis["caller"]["input"],
        stated.as_str(),
        "the caller's exact words"
    );
    assert!(!basis["caller"]["money"].as_array().unwrap().is_empty());
    assert!(
        !basis["read"]["effective"]
            .as_str()
            .unwrap()
            .contains("2 USD"),
        "the door read the request with its money blanked"
    );
    let (same, calls, judged) = judged_replay(with_money(&stated), record.clone()).await;
    assert!(!refused_replay(&same), "{:?}", same.diagnostics);
    assert_eq!((calls, judged), (0, 1));
    let other = format!("{COPIES} Budget 5 USD.");
    let (changed, calls, judged) = judged_replay(with_money(&other), record.clone()).await;
    assert!(refused_replay(&changed), "{:?}", changed.diagnostics);
    assert_eq!(
        (calls, judged),
        (0, 0),
        "a changed budget is another caller basis"
    );
    let (unadmitted, ..) = judged_replay(CompileRequest::create(stated.as_str()), record).await;
    assert!(refused_replay(&unadmitted), "{:?}", unadmitted.diagnostics);
}

#[tokio::test]
async fn a_clarification_is_part_of_the_caller_basis_and_a_new_one_is_a_new_basis() {
    let clarified = |text: &str| {
        CompileRequest::create("Fais les deux copies.")
            .answer("intent.clarification", serde_json::to_string(text).unwrap())
    };
    let replies = vec![answer_of(&copies_sketch(), &[]), fills_of(&copies_fills())];
    let out = authored_request(clarified(COPIES), replies).await;
    let record = out.provenance.plan.clone().expect("a record");
    assert_eq!(record["basis"]["read"]["effective"], COPIES);
    assert_eq!(record["basis"]["caller"]["input"], "Fais les deux copies.");
    let (same, ..) = judged_replay(clarified(COPIES), record.clone()).await;
    assert!(!refused_replay(&same), "{:?}", same.diagnostics);
    let other = COPIES.replace("./out/beta.txt", "./out/gamma.txt");
    let (replaced, calls, judged) = judged_replay(clarified(&other), record.clone()).await;
    assert!(refused_replay(&replaced), "{:?}", replaced.diagnostics);
    assert_eq!((calls, judged), (0, 0));
    let (dropped, ..) =
        judged_replay(CompileRequest::create("Fais les deux copies."), record).await;
    assert!(refused_replay(&dropped), "{:?}", dropped.diagnostics);
}

#[tokio::test]
async fn an_unbound_gap_text_in_a_record_is_never_repeated_and_its_duty_stays_open() {
    let clause = "harmonise le ton";
    let intent = format!("{COPIES} Puis {clause}.");
    let replies = vec![
        answer_of(&copies_sketch(), &[clause]),
        fills_of(&copies_fills()),
    ];
    let (out, _) = authored_with(&intent, replies, 0).await;
    let mut record = out.provenance.plan.clone().expect("a record");
    // No identity binds a gap's words: a record can carry any text there.
    record["settlement"]["gaps"][0] = json!("zz-gap-sentinel");
    let replayed = compile(&CompileRequest::create(intent.as_str()).with_plan(record)).unwrap();
    let shown = format!("{:?} {:?}", replayed.diagnostics, replayed.questions);
    assert!(!shown.contains("zz-gap-sentinel"), "{shown}");
    assert_ne!(replayed.status, CompileStatus::Ready);
    assert!(
        replayed.questions.iter().any(|q| q.key == "gap.1"),
        "the duty stays open at its position: {shown}"
    );
}

/// A hermetic rehearsal host: it keeps what it was asked to rehearse and runs nothing.
struct Room {
    asked: std::sync::Mutex<Vec<(String, Vec<String>)>>,
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
        use nika_compile_cognition::rehearse::{
            Attempt, EffectCounts, Observation, Refusal, Rehearsal, RehearsalReport,
        };
        Box::pin(async move {
            self.asked
                .lock()
                .unwrap()
                .push((candidate.to_owned(), inputs.to_vec()));
            RehearsalReport::new(
                Rehearsal::NotRun {
                    reason: "synthetic room: nothing runs".into(),
                },
                Attempt::NeverAttempted,
                EffectCounts::none(),
                nika_compile::surface::sha256(candidate),
            )
            .with_observation(Observation::refused(Refusal::Effect))
        })
    }
}

#[tokio::test]
async fn a_rehearsal_host_reads_the_semantic_record_through_its_rebuild() {
    let provider = common::Rotating::new(vec![
        answer_of(&copies_sketch(), &[]),
        fills_of(&copies_fills()),
    ]);
    let judged = common::Judged::approving(&provider);
    let room = Room {
        asked: std::sync::Mutex::new(Vec::new()),
    };
    let request = CompileRequest::create(COPIES)
        .with_authoring_policy(policy_c(nika_compile::NativeMode::Sketch, 0));
    let cognition = Cognition {
        seat: None,
        provider: Some(&judged),
    };
    let out =
        nika_compile_cognition::compile_with_cognition_rehearsed(&request, cognition, Some(&room))
            .await
            .unwrap();
    let shown = format!("{:?}", out.diagnostics);
    assert!(
        !shown.contains("no current answer record")
            && !shown.contains("do not bind these candidate paths"),
        "the record's answered paths are read from its rebuild: {shown}"
    );
    let asked = room.asked.lock().unwrap();
    assert_eq!(asked.len(), 1, "the room was asked once: {shown}");
    assert_eq!(Some(&asked[0].0), out.candidate.as_ref());
}

#[tokio::test]
async fn two_successive_answer_rounds_keep_the_initial_caller_answers() {
    let clarified = |request: CompileRequest| {
        request.answer(
            "intent.clarification",
            serde_json::to_string(SUMMARY).unwrap(),
        )
    };
    let first = CompileRequest::create("Fais le résumé.");
    let replies = vec![
        answer_of(&summary_sketch(), &[]),
        fills_of(&summary_fills()),
    ];
    let out = authored_request(clarified(first.clone()), replies).await;
    let record = out.provenance.plan.clone().expect("a record");
    let initial = record["basis"]["caller"].clone();
    assert_eq!(
        initial["answers"].as_object().map(serde_json::Map::len),
        Some(1),
        "{initial}"
    );
    let model =
        |request: CompileRequest| request.answer("model", r#""mistral/mistral-small-latest""#);
    let (second, ..) = judged_replay(model(clarified(first.clone())), record).await;
    let next = second.provenance.plan.clone().expect("the round's record");
    assert_eq!(
        next["basis"]["caller"], initial,
        "A0 is kept, never the round's answers"
    );
    assert_eq!(
        next["final"]["answers"]["model"],
        r#""mistral/mistral-small-latest""#
    );
    let (third, calls, _) = judged_replay(model(clarified(first)), next.clone()).await;
    assert_eq!(calls, 0);
    assert_eq!(
        third.candidate, second.candidate,
        "the same answers, the same bytes"
    );
    assert_eq!(
        third.provenance.plan.as_ref().unwrap()["basis"]["caller"],
        initial
    );
}

// ── QUAL20 P2a: a record's questions are exactly the rebuilt candidate's open placeholders ────

/// The approval message names a value the request leaves open: the sketch declares the
/// placeholder `const.recipient` and the seat asks for it.
async fn authored_with_placeholder() -> nika_compile::CompileOutcome {
    let mut answer: Value = serde_json::from_str(&answer_of(&copies_sketch(), &[])).unwrap();
    answer["questions"] = json!([{"key": "const.recipient", "label": "Pour qui ?", "answer_type": "text", "why": "le destinataire"}]);
    let fills = json!([{"task": "approve", "field": "args.message",
                        "value": "Écrire les deux copies pour ${{ const.recipient }} ?"}]);
    authored_with(COPIES, vec![answer.to_string(), fills_of(&fills)], 0)
        .await
        .0
}

#[tokio::test]
async fn questions_that_are_not_exactly_the_open_placeholders_refuse_the_record() {
    let out = authored_with_placeholder().await;
    let record = out.provenance.plan.clone().expect("a record");
    let asked: Vec<&str> = out.questions.iter().map(|q| q.key.as_str()).collect();
    assert_eq!(asked, ["const.recipient"], "{:?}", out.diagnostics);
    let request = CompileRequest::create(COPIES);
    let question = |key: &str| json!({"key": key, "label": "?", "answer_type": "text", "why": "?"});
    let cases = [
        (
            "an extra question",
            json!([question("const.recipient"), question("const.amount")]),
        ),
        ("an omitted question", json!([])),
        (
            "a duplicate question",
            json!([question("const.recipient"), question("const.recipient")]),
        ),
        (
            "a question on no open placeholder",
            json!([question("const.copies")]),
        ),
        (
            "a question on a non-const path",
            json!([question("permits.net")]),
        ),
    ];
    for (label, questions) in cases {
        let mut hostile = record.clone();
        hostile["settlement"]["questions"] = questions;
        let hostile = forged(hostile, &request);
        let answered = request.clone().answer("const.recipient", r#""Ada""#);
        for request in [request.clone(), answered] {
            let out = compile(&request.with_plan(hostile.clone())).unwrap();
            assert!(
                refused_replay(&out),
                "{label}: {:?} {:?}",
                out.status,
                out.diagnostics
            );
        }
    }
}

#[tokio::test]
async fn a_recorded_question_text_is_never_repeated_on_replay() {
    let out = authored_with_placeholder().await;
    let mut record = out.provenance.plan.clone().expect("a record");
    record["settlement"]["questions"][0]["label"] = json!("zz-label-sentinel");
    record["settlement"]["questions"][0]["why"] = json!("zz-why-sentinel");
    let replayed = compile(&CompileRequest::create(COPIES).with_plan(record)).unwrap();
    let shown = format!("{:?} {:?}", replayed.questions, replayed.diagnostics);
    assert!(
        !shown.contains("zz-label-sentinel") && !shown.contains("zz-why-sentinel"),
        "{shown}"
    );
    assert_eq!(
        replayed
            .questions
            .iter()
            .map(|q| q.key.as_str())
            .collect::<Vec<_>>(),
        ["const.recipient"],
        "the open placeholder is still asked: {shown}"
    );
}

// ── QUAL20 P2c: a semantic record replays only through a compile of the raw request ───────────

#[tokio::test]
async fn the_public_replay_doors_refuse_a_semantic_record() {
    let record = authored().await.provenance.plan.unwrap();
    let request = CompileRequest::create(COPIES);
    let mut out = nika_compile::surface::initial();
    nika_compile::surface::replay(COPIES, &record, &request, &mut out).unwrap();
    assert!(refused_replay(&out), "{:?}", out.diagnostics);
    let mut judged = nika_compile::surface::initial();
    nika_compile::surface::replay_judged(COPIES, &record, &request, &[], false, &mut judged)
        .unwrap();
    assert!(refused_replay(&judged), "{:?}", judged.diagnostics);
    // The guarded door replays it (a non-semantic record keeps its public replay: the legacy
    // suites above).
    let core = nika_compile::compile_judged(&request.with_plan(record), &[]).unwrap();
    assert!(core.candidate.is_some(), "{:?}", core.diagnostics);
}

#[tokio::test]
async fn a_judgment_bound_to_another_candidate_or_request_settles_nothing() {
    use nika_compile::surface::{Binding, Disposition, Judgment};
    let out = authored().await;
    let record = out.provenance.plan.clone().unwrap();
    let request = CompileRequest::create(COPIES).with_plan(record);
    let plan = nika_compile_reader::lexicon::read(COPIES).plan;
    let whole = (0, COPIES.len());
    let forged = |intent: &str, candidate: &str| {
        let binding = Binding::of(intent, &CompileRequest::create(intent), &plan, candidate);
        Judgment::new(
            intent,
            whole,
            Disposition::Carried,
            "test",
            "verify-request",
            binding,
        )
    };
    for (label, judgment) in [
        (
            "another candidate",
            forged(COPIES, "nika: other\\ntasks: {}\\n"),
        ),
        (
            "another request",
            forged(
                "Copie ./alpha.txt dans ./out/x.txt.",
                out.candidate.as_deref().unwrap(),
            ),
        ),
    ] {
        let replayed = nika_compile::compile_judged(&request, &[judgment]).unwrap();
        assert_eq!(
            replayed.status,
            CompileStatus::Incomplete,
            "{label}: {:?}",
            replayed.diagnostics
        );
        assert_eq!(
            replayed.candidate, out.candidate,
            "{label}: the bytes are rebuilt, not judged"
        );
        assert!(
            host_judged(&replayed),
            "{label}: the supplied origin is named"
        );
    }
}

#[tokio::test]
async fn typographic_apostrophes_and_a_clarification_replay_through_one_raw_request() {
    let words = "Copie ./alpha.txt dans ./out/alpha.txt et ./beta.txt dans ./out/beta.txt, dans l’ordre. Demande mon accord une seule fois avant les deux écritures. Nomme les résultats alpha et beta.";
    let clarified = CompileRequest::create("Fais les copies.").answer(
        "intent.clarification",
        serde_json::to_string(words).unwrap(),
    );
    let replies = vec![answer_of(&copies_sketch(), &[]), fills_of(&copies_fills())];
    let out = authored_request(clarified.clone(), replies).await;
    let record = (out.provenance.plan.clone())
        .unwrap_or_else(|| panic!("a record: {:?} {:?}", out.status, out.diagnostics));
    let core = compile(&clarified.clone().with_plan(record.clone())).unwrap();
    assert!(!refused_replay(&core), "{:?}", core.diagnostics);
    assert_eq!(
        core.candidate, out.candidate,
        "the same bytes from the raw request alone"
    );
    let (judged, calls, judgments) = judged_replay(clarified, record).await;
    assert_eq!((calls, judgments), (0, 1), "{:?}", judged.diagnostics);
    assert_eq!(
        judged.status,
        CompileStatus::Ready,
        "{:?}",
        judged.diagnostics
    );
}

#[tokio::test]
async fn admitted_money_with_an_identical_clarification_keeps_its_record_and_replays() {
    // One derivation of the money of a request: identical clarification words keep the host's
    // admission and are read blanked, by the seats' door and by the core replay alike.
    let stated = format!("{COPIES} Budget 2 USD.");
    let clarified = with_money(&stated).answer(
        "intent.clarification",
        serde_json::to_string(&stated).unwrap(),
    );
    let replies = vec![answer_of(&copies_sketch(), &[]), fills_of(&copies_fills())];
    let out = authored_request(clarified.clone(), replies).await;
    let record = (out.provenance.plan.clone())
        .unwrap_or_else(|| panic!("a record: {:?} {:?}", out.status, out.diagnostics));
    assert!(
        !record["basis"]["read"]["effective"]
            .as_str()
            .unwrap()
            .contains("2 USD"),
        "the clarification was read blanked"
    );
    let core = compile(&clarified.clone().with_plan(record.clone())).unwrap();
    assert!(!refused_replay(&core), "{:?}", core.diagnostics);
    assert_eq!(core.candidate, out.candidate);
    let (judged, calls, judgments) = judged_replay(clarified, record).await;
    assert_eq!((calls, judgments), (0, 1), "{:?}", judged.diagnostics);
    assert_eq!(
        judged.status,
        CompileStatus::Ready,
        "{:?}",
        judged.diagnostics
    );
}

#[tokio::test]
async fn changed_clarification_words_discard_the_admission_and_still_replay() {
    let stated = format!("{COPIES} Budget 2 USD.");
    let clarified = with_money(&stated).answer(
        "intent.clarification",
        serde_json::to_string(COPIES).unwrap(),
    );
    let replies = vec![answer_of(&copies_sketch(), &[]), fills_of(&copies_fills())];
    let out = authored_request(clarified.clone(), replies).await;
    let record = (out.provenance.plan.clone())
        .unwrap_or_else(|| panic!("a record: {:?} {:?}", out.status, out.diagnostics));
    assert_eq!(record["basis"]["read"]["effective"], COPIES);
    let core = compile(&clarified.with_plan(record)).unwrap();
    assert!(!refused_replay(&core), "{:?}", core.diagnostics);
    assert_eq!(core.candidate, out.candidate);
}

#[tokio::test]
async fn stated_money_in_a_clarification_still_closes_every_seat() {
    // A door that states money re-reads the replacement's own directives; its ceiling cannot
    // bind an unpriced seat, so no request is sent and no record exists.
    let stated = format!("{COPIES} Budget 2 USD.");
    let provider = common::Rotating::new(vec![answer_of(&copies_sketch(), &[])]);
    let request = CompileRequest::create("Fais les copies.")
        .with_stated_money()
        .answer(
            "intent.clarification",
            serde_json::to_string(&stated).unwrap(),
        )
        .with_authoring_policy(policy_c(nika_compile::NativeMode::Sketch, 0));
    let out = compile_with_provider(&request, &provider).await.unwrap();
    assert_eq!(
        provider.calls.load(Ordering::SeqCst),
        0,
        "{:?}",
        out.diagnostics
    );
    assert!(
        (out.provenance.plan.as_ref()).is_none_or(|p| p.get("semantic_record").is_none()),
        "no sketch was asked, so no semantic record exists"
    );
    assert!(
        out.diagnostics
            .iter()
            .any(|d| d.target == "authoring_money"),
        "{:?}",
        out.diagnostics
    );
}
