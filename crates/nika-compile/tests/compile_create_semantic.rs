// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>
//! Fresh CREATE is semantic at the shared entry: the model proposes a private plan or a sketch
//! and its fills, the compiler writes the source. A recording transport answers each call by the
//! schema it was asked under and refuses every whole-source schema (`candidate`,
//! `candidate_lines`), counting the attempt. The expected duties, paths, gates and results are
//! built here, never read from a proposal. Scripted providers and an approving judge: no network,
//! no key, and no claim that a scripted judgment qualifies a model.
#![allow(clippy::unwrap_used, clippy::expect_used)]
use nika_compile::{
    AuthoringKnowledge, AuthoringPolicy, CompileOutcome, CompileRequest, CompileStatus,
    DiagnosticKind, KnowledgeReference, NativeMode,
};
use nika_compile_cognition::{
    Cognition, NoProvider, compile_with_cognition, compile_with_provider,
};
use nika_kernel::ai::provider::{
    ContentBlock, InferRequest, InferResponse, ProviderError, ProviderInferDyn, ResponseFormat,
    StopReason, TokenUsage,
};
use serde_json::{Value, json};
use std::{sync::Mutex, time::Duration};

mod common;
use common::{INTENT, plan};

const PACK_DIGEST: &str = "6a1cfeed6a1cfeed6a1cfeed6a1cfeed6a1cfeed6a1cfeed6a1cfeed6a1cfeed";
const PACK_SENTINEL: &str = "pack-sentinel-31d7: a reply drafted from the classified issue";

/// Two sources, two gated destinations, two named results (slice B's composed request).
const COPIES: &str = "Copie ./alpha.txt dans ./out/alpha.txt et ./beta.txt dans ./out/beta.txt. Demande mon accord une seule fois avant les deux écritures. Nomme les résultats alpha et beta.";
/// The independent oracle of `COPIES`: each source, the write that must carry it, its result.
const BRANCHES: [(&str, &str, &str, &str); 2] = [
    ("./alpha.txt", "write_alpha", "./out/alpha.txt", "alpha"),
    ("./beta.txt", "write_beta", "./out/beta.txt", "beta"),
];

/// What the transport received, by the schema of each call: `plan`, `sketch`, `fills`, `judge`,
/// or `source` for a whole-source schema it refused.
struct Semantic {
    plans: Vec<String>,
    sketches: Vec<String>,
    fills: Vec<String>,
    seen: Mutex<Vec<&'static str>>,
}

impl Semantic {
    fn new(plans: &[String], sketches: &[String], fills: &[String]) -> Self {
        Self {
            plans: plans.to_vec(),
            sketches: sketches.to_vec(),
            fills: fills.to_vec(),
            seen: Mutex::new(Vec::new()),
        }
    }
    fn seen(&self) -> Vec<&'static str> {
        self.seen.lock().unwrap().clone()
    }
    fn count(&self, kind: &str) -> usize {
        self.seen().iter().filter(|k| **k == kind).count()
    }
}

fn reply(text: &str) -> InferResponse {
    InferResponse::new(
        vec![ContentBlock::Text {
            text: text.to_owned(),
        }],
        TokenUsage::new(100, 50),
        StopReason::EndTurn,
    )
}

fn nth(script: &[String], index: usize) -> &str {
    &script[index.min(script.len().saturating_sub(1))]
}

impl ProviderInferDyn for Semantic {
    async fn infer(&self, request: InferRequest) -> Result<InferResponse, ProviderError> {
        let schema = match &request.response_format {
            ResponseFormat::JsonSchema(schema) => schema.clone(),
            _ => Value::Null,
        };
        let has = |key: &str| schema["properties"].get(key).is_some();
        let kind = if has("candidate") || has("candidate_lines") {
            "source"
        } else if has("choice") {
            "judge"
        } else if has("steps") {
            "plan"
        } else if has("tasks") {
            "sketch"
        } else if has("fills") {
            "fills"
        } else {
            "other"
        };
        let index = {
            let mut seen = self.seen.lock().unwrap();
            seen.push(kind);
            seen.iter().filter(|k| **k == kind).count() - 1
        };
        match kind {
            "source" => Err(ProviderError::Other {
                reason: "the test transport refuses a whole-source schema".to_owned(),
            }),
            "judge" => {
                let keys = schema["properties"]["choice"]["enum"].to_string();
                let key = if keys.contains("\"faithful\"") {
                    "faithful"
                } else {
                    "carried"
                };
                Ok(reply(&json!({"choice": key}).to_string()))
            }
            "plan" => Ok(reply(nth(&self.plans, index))),
            "sketch" => Ok(reply(nth(&self.sketches, index))),
            "fills" => Ok(reply(nth(&self.fills, index))),
            _ => Err(ProviderError::Other {
                reason: format!("an unexpected schema: {schema}"),
            }),
        }
    }
}

fn policy(native: NativeMode, repairs: u32) -> AuthoringPolicy {
    AuthoringPolicy::new("mock/authoring", 4096, Duration::from_secs(2))
        .with_native(native)
        .with_repairs(repairs)
}

/// The support request answered as the COLD suites answer it.
fn answered(native: NativeMode) -> CompileRequest {
    CompileRequest::create(INTENT)
        .with_authoring_policy(policy(native, 3))
        .answer("model", r#""mock/echo""#)
        .answer("const.customer_directory", r#""customers.json""#)
        .answer("const.refund_policy", r#"{"cap":100,"currency":"EUR"}"#)
        .answer(
            "const.refund_endpoint",
            r#""https://refund.example.invalid/refunds""#,
        )
}

fn pack() -> AuthoringKnowledge {
    AuthoringKnowledge {
        identity: json!({"door": {"pack_sha256": PACK_DIGEST}}),
        references: vec![KnowledgeReference {
            id: "pattern:customer-reply".into(),
            kind: "pattern".into(),
            text: PACK_SENTINEL.into(),
        }],
        ..AuthoringKnowledge::default()
    }
}

fn world() -> Value {
    common::observed(&[("./customers.json", &["id", "email"])])
}

fn roles(out: &CompileOutcome) -> Vec<String> {
    (out.provenance.authoring.as_ref())
        .map(|r| {
            r.context
                .iter()
                .map(|c| c["call"].as_str().unwrap_or_default().to_owned())
                .collect()
        })
        .unwrap_or_default()
}

fn door(out: &CompileOutcome) -> Value {
    out.provenance.decision.as_ref().unwrap()["forensic"]["door"].clone()
}

fn route(out: &CompileOutcome) -> String {
    out.provenance
        .decision
        .as_ref()
        .map_or_else(String::new, |d| d["route"].to_string())
}

fn copy_task(id: &str, tool: &str, extra: &Value) -> Value {
    let mut task = json!({"id": id, "verb": "invoke", "tool": tool, "purpose": id});
    for (key, value) in extra.as_object().unwrap() {
        task[key] = value.clone();
    }
    task
}

fn copies_sketch() -> String {
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
    ], "outputs": [{"name": "alpha", "from": "write_alpha"}, {"name": "beta", "from": "write_beta"}],
       "questions": [], "gaps": [], "notes": "graph"})
    .to_string()
}

fn copies_fills() -> String {
    json!({"fills": [{"task": "approve", "field": "args.message", "value": "Écrire les deux copies ?"}],
           "notes": "fills"})
    .to_string()
}

/// Every branch of the independent oracle is realized in the emitted source: its read, its write
/// of that read behind the one approval, its named result.
fn assert_branches(candidate: &str) {
    let doc: Value = serde_yaml_bw::from_str(candidate).unwrap();
    for (source, write, destination, result) in BRANCHES {
        let task = &doc["tasks"][write];
        assert_eq!(task["invoke"]["args"]["path"], destination, "{doc:#}");
        let reader = (task["with"]["text"].as_str().unwrap())
            .trim_start_matches("${{ tasks.")
            .trim_end_matches(".output }}");
        assert_eq!(doc["tasks"][reader]["invoke"]["args"]["path"], source);
        assert_eq!(task["when"], "${{ with.approved == true }}", "{write}");
        assert_eq!(
            doc["outputs"][result],
            format!("${{{{ tasks.{write}.output }}}}")
        );
    }
    assert_eq!(
        doc["outputs"].as_object().map(serde_json::Map::len),
        Some(2)
    );
}

/// The migration a retired source-only CREATE names: no request, no candidate, both semantic doors.
fn only_refused(out: &CompileOutcome) -> bool {
    out.candidate.is_none()
        && out.status == CompileStatus::Refused
        && out.diagnostics.iter().any(|d| {
            d.target == "authoring_policy"
                && d.kind == DiagnosticKind::Refused
                && d.message.contains("native: only")
                && d.message.contains("escalate")
                && d.message.contains("sketch")
        })
}

// ── Escalate: the Plan reads the admitted Foundry; knowledge never selects source ─────────────

#[tokio::test]
async fn escalate_plans_with_the_admitted_foundry_and_never_asks_for_source() {
    let provider = Semantic::new(&[plan().to_string()], &[], &[]);
    let request = answered(NativeMode::Escalate)
        .with_authoring_knowledge(pack())
        .with_knowledge(world());
    let out = compile_with_provider(&request, &provider).await.unwrap();
    assert_eq!(provider.count("source"), 0, "{:?}", provider.seen());
    assert_eq!(
        provider.seen().first().copied(),
        Some("plan"),
        "{:?}",
        provider.seen()
    );
    assert_eq!(out.status, CompileStatus::Ready, "{out:#?}");
    assert_eq!(door(&out)["source_owner"], "deterministic_assembler");
    let receipt = out.provenance.authoring.as_ref().unwrap();
    let opening = receipt
        .context
        .iter()
        .find(|c| c["call"] == "plan")
        .unwrap();
    assert_eq!(
        opening["semantic_context"]["pack_sha256"], PACK_DIGEST,
        "{opening:#}"
    );
    let candidate = out.candidate.as_deref().unwrap();
    assert!(
        !candidate.contains(PACK_SENTINEL),
        "a reference is never authority"
    );
    // The same request, policy and judge with knowledge off: the same route and the same source.
    let bare = Semantic::new(&[plan().to_string()], &[], &[]);
    let off = compile_with_provider(
        &answered(NativeMode::Escalate).with_knowledge(world()),
        &bare,
    )
    .await
    .unwrap();
    assert_eq!(
        off.candidate, out.candidate,
        "knowledge never selects the route"
    );
    assert_eq!(bare.seen(), provider.seen());
    assert_eq!(roles(&off), roles(&out));
}

/// A plan that ends without a candidate escalates to the sketch door with the same request, its
/// spend kept first: the composed graph keeps both branches, the gate and the named results.
#[tokio::test]
async fn a_plan_without_a_candidate_escalates_to_the_sketch_door_and_keeps_both_branches() {
    let provider = Semantic::new(
        &["no plan here".to_owned()],
        &[copies_sketch()],
        &[copies_fills()],
    );
    let request =
        CompileRequest::create(COPIES).with_authoring_policy(policy(NativeMode::Escalate, 3));
    let out = compile_with_provider(&request, &provider).await.unwrap();
    assert_eq!(provider.count("source"), 0, "{:?}", provider.seen());
    assert_eq!(
        &provider.seen()[..3],
        ["plan", "sketch", "fills"],
        "{:?}",
        provider.seen()
    );
    assert_eq!(out.status, CompileStatus::Ready, "{out:#?}");
    assert_branches(out.candidate.as_deref().unwrap());
    assert_eq!(
        &roles(&out)[..3],
        ["plan", "sketch", "fill"],
        "the paid plan stays first"
    );
    assert_eq!(
        out.provenance.authoring.as_ref().unwrap().calls as usize,
        provider.seen().len(),
        "every call is journaled"
    );
    assert_eq!(door(&out)["name"], "sketch", "{:#}", door(&out));
    assert_eq!(
        door(&out)["source_owner"],
        "compiler_from_model_sketch_and_fills"
    );
    assert!(route(&out).contains("sketch"), "{}", route(&out));
}

/// The sketch door after a plan takes one request more than the native door did, within the
/// same bound: with no repair left, the escalation is named and no request is sent.
#[tokio::test]
async fn an_escalation_without_a_repair_allowance_is_named_and_sends_nothing_more() {
    let provider = Semantic::new(
        &["no plan here".to_owned()],
        &[copies_sketch()],
        &[copies_fills()],
    );
    let request =
        CompileRequest::create(COPIES).with_authoring_policy(policy(NativeMode::Escalate, 0));
    let out = compile_with_provider(&request, &provider).await.unwrap();
    assert_eq!(provider.seen(), ["plan"], "only the plan was sent");
    assert!(out.candidate.is_none());
    let told: Vec<&str> = out.diagnostics.iter().map(|d| d.message.as_str()).collect();
    assert!(
        told.iter()
            .any(|m| m.contains("sketch door") && m.contains("repair allowance (0)")),
        "{told:?}"
    );
}

/// A source-shaped answer is never a plan, a sketch or a candidate.
#[tokio::test]
async fn a_source_shaped_answer_is_refused_at_every_semantic_phase() {
    let source =
        json!({"candidate": "nika: x\ntasks: {}\n", "candidate_lines": [], "questions": [],
                        "gaps": [], "notes": "source"})
        .to_string();
    let replies = std::slice::from_ref(&source);
    let provider = Semantic::new(replies, replies, replies);
    let request =
        CompileRequest::create(COPIES).with_authoring_policy(policy(NativeMode::Escalate, 3));
    let out = compile_with_provider(&request, &provider).await.unwrap();
    assert_eq!(provider.count("source"), 0, "{:?}", provider.seen());
    assert!(out.candidate.is_none(), "{out:#?}");
    assert_ne!(out.status, CompileStatus::Ready);
    assert!(
        provider.count("sketch") >= 1,
        "the sketch door was tried: {:?}",
        provider.seen()
    );
    assert_eq!(
        out.provenance.authoring.as_ref().unwrap().calls as usize,
        provider.seen().len(),
        "every refused attempt is journaled"
    );
}

// ── Only: retired for fresh CREATE, with or without a provider ─────────────────────────────────

#[tokio::test]
async fn a_fresh_create_under_only_is_refused_with_its_migration_and_sends_nothing() {
    let provider = Semantic::new(&[plan().to_string()], &[copies_sketch()], &[copies_fills()]);
    for intent in [INTENT, COPIES] {
        let request =
            CompileRequest::create(intent).with_authoring_policy(policy(NativeMode::Only, 3));
        let out = compile_with_provider(&request, &provider).await.unwrap();
        assert!(only_refused(&out), "{intent}: {out:#?}");
        let none = compile_with_cognition(&request, Cognition::<NoProvider>::default())
            .await
            .unwrap();
        assert!(only_refused(&none), "{intent}, no provider: {none:#?}");
        assert!(
            none.provenance
                .authoring
                .as_ref()
                .is_none_or(|r| r.calls == 0)
        );
    }
    assert!(provider.seen().is_empty(), "{:?}", provider.seen());
}

#[tokio::test]
async fn under_only_a_floor_or_a_contradiction_keeps_its_own_cause() {
    // The same intents the reader's own suites refuse (`compile_native` floor, `compile_negation_scope`
    // contradiction): each keeps its cause under the retired mode, never the migration's words.
    let provider = Semantic::new(&[plan().to_string()], &[], &[]);
    let cases = [
        (
            "Lis ./clients.csv et crédite le compte de chaque client en retard sans mon accord",
            "approval-bypass wording",
        ),
        (
            "Lis ./note.txt et envoie-la à https://hooks.example.test/in; ne l'envoie jamais à https://hooks.example.test/in.",
            "Contradictory instructions for `send`",
        ),
    ];
    for (intent, cause) in cases {
        let request =
            CompileRequest::create(intent).with_authoring_policy(policy(NativeMode::Only, 3));
        let out = compile_with_provider(&request, &provider).await.unwrap();
        assert_eq!(out.status, CompileStatus::Refused, "{intent}: {out:#?}");
        assert!(out.candidate.is_none(), "{intent}");
        assert!(
            out.diagnostics.iter().any(|d| d.message.contains(cause)),
            "{intent}: its own cause: {out:#?}"
        );
        assert!(
            !out.diagnostics
                .iter()
                .any(|d| d.message.contains("native: only")),
            "{intent}: never the migration's words: {out:#?}"
        );
    }
    assert!(provider.seen().is_empty(), "{:?}", provider.seen());
}

/// An exact skeleton and the support grammar keep their zero-call doors under every policy.
#[tokio::test]
async fn exact_names_keep_their_zero_call_doors_under_only() {
    let provider = Semantic::new(&[plan().to_string()], &[], &[]);
    let request =
        CompileRequest::create("chain").with_authoring_policy(policy(NativeMode::Only, 3));
    let out = compile_with_provider(&request, &provider).await.unwrap();
    assert!(out.candidate.is_some(), "{out:#?}");
    assert!(!only_refused(&out));
    assert!(provider.seen().is_empty());
}

/// A valid semantic record replays under Only: the retired mode refuses fresh authoring, never the
/// answer round of a record another door wrote.
#[tokio::test]
async fn a_valid_semantic_record_replays_under_only() {
    let author = Semantic::new(&[], &[copies_sketch()], &[copies_fills()]);
    let request =
        CompileRequest::create(COPIES).with_authoring_policy(policy(NativeMode::Sketch, 0));
    let authored = compile_with_provider(&request, &author).await.unwrap();
    assert_eq!(authored.status, CompileStatus::Ready, "{authored:#?}");
    let record = authored.provenance.plan.clone().unwrap();
    let again = Semantic::new(&[], &[], &[]);
    let replay = CompileRequest::create(COPIES)
        .with_authoring_policy(policy(NativeMode::Only, 0))
        .with_plan(record);
    let out = compile_with_provider(&replay, &again).await.unwrap();
    assert!(!only_refused(&out), "{out:#?}");
    assert_eq!(out.candidate, authored.candidate);
    assert!(
        again.seen().iter().all(|k| *k == "judge"),
        "a replay regenerates nothing: {:?}",
        again.seen()
    );
}

// ── Sketch and Off keep their own laws ─────────────────────────────────────────────────────────

#[tokio::test]
async fn explicit_sketch_and_off_keep_their_doors() {
    let sketch = Semantic::new(&[], &[copies_sketch()], &[copies_fills()]);
    let request =
        CompileRequest::create(COPIES).with_authoring_policy(policy(NativeMode::Sketch, 0));
    let out = compile_with_provider(&request, &sketch).await.unwrap();
    assert_eq!(&sketch.seen()[..2], ["sketch", "fills"]);
    assert_branches(out.candidate.as_deref().unwrap());
    // Off: a plan that ends without a candidate stays the plan round's outcome; no other door.
    let off = Semantic::new(
        &["no plan here".to_owned()],
        &[copies_sketch()],
        &[copies_fills()],
    );
    let request = CompileRequest::create(COPIES).with_authoring_policy(policy(NativeMode::Off, 3));
    let out = compile_with_provider(&request, &off).await.unwrap();
    assert_eq!(off.count("plan"), off.seen().len(), "{:?}", off.seen());
    assert!(off.count("plan") >= 1);
    assert!(out.candidate.is_none());
}

// ── A join of two parsed tables (capability witness) ───────────────────────────────────────────

/// Two stated tables parsed, joined on a key and summed per region: the jq program must receive
/// BOTH tables. The sketch binds a jq task's input from its edges; a lost second edge would make
/// the program read a table it never received.
#[tokio::test]
async fn a_join_reads_both_parsed_tables() {
    let intent = "Read ./orders.csv and ./customers.csv, join them on customer_id, sum amount_cents per region and write the totals to ./out/result.json.";
    let edge = |name: &str, from: &str| json!({"name": name, "from": from});
    let graph = json!({"name": "regional-totals", "tasks": [
        copy_task("read_orders", "nika:read", &json!({"reads": ["./orders.csv"]})),
        copy_task("read_customers", "nika:read", &json!({"reads": ["./customers.csv"]})),
        copy_task("parse_orders", "nika:convert", &json!({"with": [edge("document", "read_orders")]})),
        copy_task("parse_customers", "nika:convert", &json!({"with": [edge("document", "read_customers")]})),
        copy_task("totals", "nika:jq", &json!({"with": [edge("orders", "parse_orders"), edge("customers", "parse_customers")]})),
        copy_task("write_totals", "nika:write", &json!({"writes": ["./out/result.json"], "with": [edge("content", "totals")]})),
    ], "outputs": [{"name": "totals", "from": "totals"}], "questions": [], "gaps": [], "notes": "join"});
    let fills = json!({"fills": [
        {"task": "parse_orders", "field": "args", "value": {"from": "csv", "to": "json"}},
        {"task": "parse_customers", "field": "args", "value": {"from": "csv", "to": "json"}},
        {"task": "totals", "field": "expression", "value": "(.customers | map({key: .customer_id, value: .region}) | from_entries) as $region | .orders | group_by($region[.customer_id]) | map({region: $region[.[0].customer_id], total_cents: (map(.amount_cents | tonumber) | add)})"}
    ], "notes": "three holes"});
    let provider = Semantic::new(&[], &[graph.to_string()], &[fills.to_string()]);
    let request =
        CompileRequest::create(intent).with_authoring_policy(policy(NativeMode::Sketch, 0));
    let out = compile_with_provider(&request, &provider).await.unwrap();
    assert_eq!(out.status, CompileStatus::Ready, "{out:#?}");
    let doc: Value = serde_yaml_bw::from_str(out.candidate.as_deref().unwrap()).unwrap();
    let input = doc["tasks"]["totals"]["invoke"]["args"]["input"].to_string();
    assert!(
        input.contains("orders") && input.contains("customers"),
        "the join's input carries both tables: {input}"
    );
}
