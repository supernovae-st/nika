// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>
//! Fresh CREATE at the shared entry: under `escalate` and `only` the first authoring call asks
//! for the complete document (the document door, R5 · C13); the private plan (`off`) and the
//! sketch with its fills (`sketch`) stay explicit doors whose source the compiler writes. A
//! recording transport answers each call by the schema it was asked under (`document` for the
//! document door's answer, which carries `operations`) and refuses the retired whole-source
//! schema (`candidate` or `candidate_lines` without `operations`), counting the attempt. The
//! expected duties, paths, gates and results are built here, never read from a proposal; a
//! scripted complete document is one an explicit door emitted. Scripted providers and an
//! approving judge: no network, no key, and no claim that a scripted judgment qualifies a model.
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

/// What the transport received, by the schema of each call: `document`, `plan`, `sketch`,
/// `fills`, `judge`, or `source` for the retired whole-source schema it refused. A document call
/// with no scripted document fails as a provider would.
struct Semantic {
    documents: Vec<String>,
    plans: Vec<String>,
    sketches: Vec<String>,
    fills: Vec<String>,
    seen: Mutex<Vec<&'static str>>,
}

impl Semantic {
    fn new(plans: &[String], sketches: &[String], fills: &[String]) -> Self {
        Self {
            documents: Vec::new(),
            plans: plans.to_vec(),
            sketches: sketches.to_vec(),
            fills: fills.to_vec(),
            seen: Mutex::new(Vec::new()),
        }
    }
    fn with_documents(mut self, documents: &[String]) -> Self {
        self.documents = documents.to_vec();
        self
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
        let kind = if has("operations") {
            "document"
        } else if has("candidate") || has("candidate_lines") {
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
            "document" if self.documents.is_empty() => Err(ProviderError::Other {
                reason: "no document is scripted for this call".to_owned(),
            }),
            "document" => Ok(reply(nth(&self.documents, index))),
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

/// The refusal a retired source-only CREATE once named (no request, no candidate, both semantic
/// doors): `only` now opens the document door, and no route may name this again.
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

// ── The admitted Foundry reaches the first call; knowledge never selects the route ────────────

#[tokio::test]
async fn the_admitted_foundry_reaches_the_first_call_and_never_selects_the_route() {
    // Off: the private plan reads the admitted Foundry; the compiler writes the source.
    let provider = Semantic::new(&[plan().to_string()], &[], &[]);
    let request = answered(NativeMode::Off)
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
    let written = out.candidate.clone().unwrap();
    assert!(
        !written.contains(PACK_SENTINEL),
        "a reference is never authority"
    );
    // Escalate: the document door's first call carries the same pack (the scripted author
    // answers the document the plan's assembler wrote); no call asks for the retired schema.
    let documents = [common::document_answer(&written)];
    let author = Semantic::new(&[], &[], &[]).with_documents(&documents);
    let request = answered(NativeMode::Escalate)
        .with_authoring_knowledge(pack())
        .with_knowledge(world());
    let created = compile_with_provider(&request, &author).await.unwrap();
    assert_eq!(author.count("source"), 0, "{:?}", author.seen());
    assert_eq!(
        author.seen().first().copied(),
        Some("document"),
        "{:?}",
        author.seen()
    );
    assert_eq!(created.status, CompileStatus::Ready, "{created:#?}");
    assert_eq!(door(&created)["source_owner"], "model");
    let first = &created.provenance.authoring.as_ref().unwrap().context[0];
    assert!(
        (first["references"].as_array().unwrap().iter())
            .any(|r| r["id"] == "pattern:customer-reply"),
        "{first:#}"
    );
    let candidate = created.candidate.as_deref().unwrap();
    assert!(
        !candidate.contains(PACK_SENTINEL),
        "a reference is never authority"
    );
    // The same requests, policies and judge with knowledge off: the same routes and sources.
    for (native, kept, seen) in [
        (NativeMode::Off, &out, provider.seen()),
        (NativeMode::Escalate, &created, author.seen()),
    ] {
        let bare = Semantic::new(&[plan().to_string()], &[], &[]).with_documents(&documents);
        let off = compile_with_provider(&answered(native).with_knowledge(world()), &bare)
            .await
            .unwrap();
        assert_eq!(
            off.candidate, kept.candidate,
            "{native:?}: knowledge never selects the route"
        );
        assert_eq!(bare.seen(), seen, "{native:?}");
        assert_eq!(roles(&off), roles(kept), "{native:?}");
    }
}

/// A composed request (two copies behind one approval, two named results) reaches the complete
/// document at the first call under `escalate`: both branches, the gate and the named results
/// kept, every call journaled, the document door named. The scripted document is the one the
/// explicit sketch door emits from the same graph and fills.
#[tokio::test]
async fn a_composed_request_reaches_the_complete_document_with_both_branches() {
    let author = Semantic::new(&[], &[copies_sketch()], &[copies_fills()]);
    let request =
        CompileRequest::create(COPIES).with_authoring_policy(policy(NativeMode::Sketch, 3));
    let drawn = compile_with_provider(&request, &author).await.unwrap();
    assert_eq!(drawn.status, CompileStatus::Ready, "{drawn:#?}");
    assert_eq!(
        door(&drawn)["source_owner"],
        "compiler_from_model_sketch_and_fills"
    );
    let written = drawn.candidate.unwrap();
    assert_branches(&written);
    let provider =
        Semantic::new(&[], &[], &[]).with_documents(&[common::document_answer(&written)]);
    let request =
        CompileRequest::create(COPIES).with_authoring_policy(policy(NativeMode::Escalate, 3));
    let out = compile_with_provider(&request, &provider).await.unwrap();
    assert_eq!(provider.count("source"), 0, "{:?}", provider.seen());
    assert_eq!(
        provider.seen().first().copied(),
        Some("document"),
        "{:?}",
        provider.seen()
    );
    assert!(
        !(provider.seen().iter()).any(|k| ["plan", "sketch", "fills"].contains(k)),
        "{:?}",
        provider.seen()
    );
    assert_eq!(out.status, CompileStatus::Ready, "{out:#?}");
    assert_branches(out.candidate.as_deref().unwrap());
    assert_eq!(roles(&out)[0], "document", "{:?}", roles(&out));
    assert_eq!(
        out.provenance.authoring.as_ref().unwrap().calls as usize,
        provider.seen().len(),
        "every call is journaled"
    );
    assert_eq!(door(&out)["name"], "native_source", "{:#}", door(&out));
    assert_eq!(door(&out)["reason"], "complete_document_door");
    assert_eq!(door(&out)["source_owner"], "model");
    assert!(route(&out).contains("native: document"), "{}", route(&out));
}

/// Under `escalate` with no repair allowance, a document the laws refuse ends the door: the
/// refusal is named and nothing more is sent.
#[tokio::test]
async fn a_refused_document_without_a_repair_allowance_is_named_and_sends_nothing_more() {
    let provider = Semantic::new(&[], &[], &[])
        .with_documents(&[common::document_answer("nika: x\ntasks: {}\n")]);
    let request =
        CompileRequest::create(COPIES).with_authoring_policy(policy(NativeMode::Escalate, 0));
    let out = compile_with_provider(&request, &provider).await.unwrap();
    assert_eq!(provider.seen(), ["document"], "only the document was sent");
    assert!(out.candidate.is_none());
    assert_ne!(out.status, CompileStatus::Ready);
    let told: Vec<&str> = out.diagnostics.iter().map(|d| d.message.as_str()).collect();
    assert!(
        (told.iter()).any(|m| m.contains("recorded 1 round(s)") && m.contains("UNREALIZED PATH")),
        "the one refused round is named: {told:?}"
    );
}

/// A source-shaped answer is never a plan, a sketch or fills; at the document door it is the
/// document it says, and the laws refuse that empty workflow.
#[tokio::test]
async fn a_source_shaped_answer_is_refused_at_every_semantic_phase() {
    let source =
        json!({"candidate": "nika: x\ntasks: {}\n", "candidate_lines": [], "questions": [],
                        "gaps": [], "notes": "source"})
        .to_string();
    let replies = std::slice::from_ref(&source);
    for (native, phase) in [
        (NativeMode::Off, "plan"),
        (NativeMode::Sketch, "sketch"),
        (NativeMode::Escalate, "document"),
    ] {
        let provider = Semantic::new(replies, replies, replies).with_documents(replies);
        let request = CompileRequest::create(COPIES).with_authoring_policy(policy(native, 3));
        let out = compile_with_provider(&request, &provider).await.unwrap();
        assert_eq!(
            provider.count("source"),
            0,
            "{native:?}: {:?}",
            provider.seen()
        );
        assert!(out.candidate.is_none(), "{native:?}: {out:#?}");
        assert_ne!(out.status, CompileStatus::Ready, "{native:?}");
        assert!(
            provider.count(phase) >= 1,
            "{native:?}: the {phase} phase was tried: {:?}",
            provider.seen()
        );
        assert_eq!(
            out.provenance.authoring.as_ref().unwrap().calls as usize,
            provider.seen().len(),
            "{native:?}: every refused attempt is journaled"
        );
    }
}

// ── Only: the document door straight, with or without a provider ──────────────────────────────

/// `only` names the document door before the reader: a fresh CREATE's first authoring call is
/// the complete document, never a plan, a sketch or the retired whole-source schema; with no
/// provider nothing is sent.
#[tokio::test]
async fn a_fresh_create_under_only_goes_straight_to_the_document_door() {
    for intent in [INTENT, COPIES] {
        let provider = Semantic::new(&[plan().to_string()], &[copies_sketch()], &[copies_fills()]);
        let request =
            CompileRequest::create(intent).with_authoring_policy(policy(NativeMode::Only, 3));
        let out = compile_with_provider(&request, &provider).await.unwrap();
        assert_eq!(provider.seen(), ["document"], "{intent}");
        assert!(!only_refused(&out), "{intent}: {out:#?}");
        assert!(
            route(&out).contains("native: document"),
            "{intent}: {}",
            route(&out)
        );
        assert!(
            !route(&out).contains("hot"),
            "{intent}: no reading first: {}",
            route(&out)
        );
        let none = compile_with_cognition(&request, Cognition::<NoProvider>::default())
            .await
            .unwrap();
        assert!(!only_refused(&none), "{intent}, no provider: {none:#?}");
        assert!(
            none.provenance
                .authoring
                .as_ref()
                .is_none_or(|r| r.calls == 0)
        );
    }
}

#[tokio::test]
async fn under_only_a_floor_or_a_contradiction_keeps_its_own_cause() {
    // The same intents the reader's own suites refuse (`compile_native` floor, `compile_negation_scope`
    // contradiction): each keeps its cause under `only`, before any door, never the retired words.
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

/// A valid semantic record replays under Only: `only` opens the document door for fresh
/// authoring, never for the answer round of a record another door wrote.
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
