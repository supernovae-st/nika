// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>
//! Observable compile semantics: which door answered, who authored the source, what each call
//! was shown and returned, and which evidence a candidate actually carries. Scripted providers
//! only; no network, no key.
//!
//! The `witness_*` tests describe CURRENT routing as the forensic record exposes it. Several
//! describe a known convergence defect (source requested where semantics were expected, HOT
//! skipped, fills outside the declared holes accepted). A witness that passes freezes the
//! behavior so a later routing change is visible; it never claims the defect fixed.
#![allow(clippy::unwrap_used, clippy::expect_used)]
use nika_compile::{
    AuthoringKnowledge, AuthoringPolicy, CompileOutcome, CompileRequest, KnowledgeReference,
    NativeMode, Strategy, outcome_document,
};
use nika_compile_cognition::compile_with_provider;
use nika_kernel::ai::provider::{
    ContentBlock, InferRequest, InferResponse, ProviderError, ProviderInferDyn, ResponseFormat,
    Role, StopReason, TokenUsage,
};
use serde_json::{Value, json};
use std::{sync::Mutex, time::Duration};

mod common;
use common::INTENT;

/// One scripted reply: a text with reported usage, a text without usage, or a provider failure.
enum Reply {
    Text(String),
    Unmetered(String),
    Fail,
    Refuse,
}

/// What one request asked for: the answer schema's property keys and every message's text.
struct Seen {
    schema: Vec<String>,
    system: String,
    user: String,
}

/// A provider answering its script in order (the last reply repeats) and keeping what each
/// request asked for.
struct Script {
    replies: Vec<Reply>,
    seen: Mutex<Vec<Seen>>,
}

impl Script {
    fn new(replies: Vec<Reply>) -> Self {
        Self {
            replies,
            seen: Mutex::new(Vec::new()),
        }
    }
    fn texts(texts: &[String]) -> Self {
        Self::new(texts.iter().cloned().map(Reply::Text).collect())
    }
    fn calls(&self) -> usize {
        self.seen.lock().unwrap().len()
    }
    /// The answer schema keys of every request, in call order.
    fn schemas(&self) -> Vec<Vec<String>> {
        self.seen
            .lock()
            .unwrap()
            .iter()
            .map(|s| s.schema.clone())
            .collect()
    }
}

fn text_of(request: &InferRequest, role: Role) -> String {
    request
        .messages
        .iter()
        .filter(|m| std::mem::discriminant(&m.role) == std::mem::discriminant(&role))
        .flat_map(|m| m.content.iter())
        .filter_map(|block| match block {
            ContentBlock::Text { text } => Some(text.as_str()),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join("\n")
}

impl ProviderInferDyn for Script {
    async fn infer(&self, request: InferRequest) -> Result<InferResponse, ProviderError> {
        let mut schema: Vec<String> = match &request.response_format {
            ResponseFormat::JsonSchema(schema) => schema["properties"]
                .as_object()
                .map(|p| p.keys().cloned().collect())
                .unwrap_or_default(),
            _ => Vec::new(),
        };
        schema.sort();
        let index = {
            let mut seen = self.seen.lock().unwrap();
            seen.push(Seen {
                schema,
                system: text_of(&request, Role::System),
                user: text_of(&request, Role::User),
            });
            seen.len() - 1
        };
        let reply = &self.replies[index.min(self.replies.len() - 1)];
        let respond = |text: &str| {
            InferResponse::new(
                vec![ContentBlock::Text {
                    text: text.to_owned(),
                }],
                TokenUsage::new(100, 50),
                StopReason::EndTurn,
            )
        };
        match reply {
            Reply::Text(text) => Ok(respond(text)),
            Reply::Unmetered(text) => {
                let mut response = respond(text);
                response.usage_reported = false;
                Ok(response)
            }
            Reply::Fail => Err(ProviderError::Other {
                reason: "scripted failure".to_owned(),
            }),
            Reply::Refuse => Err(ProviderError::AdmissionDenied {
                reason: "scripted local admission refusal".to_owned(),
            }),
        }
    }
}

fn policy(native: NativeMode, repairs: u32) -> AuthoringPolicy {
    AuthoringPolicy::new("mock/authoring", 4096, Duration::from_secs(2))
        .with_native(native)
        .with_repairs(repairs)
}

/// A source the native door answers with; the laws refuse it, which is not what is observed.
fn native_answer() -> String {
    json!({"candidate": "nika: x\ntasks: {}\n", "candidate_lines": [], "questions": [], "gaps": [], "notes": "n"}).to_string()
}

/// The answer schema of a door that asks the model for complete source.
fn source_schema() -> Vec<String> {
    ["candidate", "candidate_lines", "gaps", "notes", "questions"]
        .map(str::to_owned)
        .to_vec()
}

/// The answer schema of the private plan door.
fn plan_schema() -> Vec<String> {
    [
        "approval_bypass",
        "constraints",
        "effects",
        "obligations",
        "regions",
        "steps",
        "unknowns",
    ]
    .map(str::to_owned)
    .to_vec()
}

const SECRET: &str = "sk-forensic-canary-0123456789";

fn foundry() -> AuthoringKnowledge {
    AuthoringKnowledge {
        references: vec![KnowledgeReference {
            id: "pattern:customer-reply".into(),
            kind: "pattern".into(),
            text: format!("A reply drafted from the classified issue. Canary {SECRET}."),
        }],
        ..AuthoringKnowledge::default()
    }
}

fn route(out: &CompileOutcome) -> Vec<String> {
    out.provenance.decision.as_ref().unwrap()["route"]
        .as_array()
        .unwrap()
        .iter()
        .map(|s| s.as_str().unwrap().to_owned())
        .collect()
}

const REVISION_BASE: &str = "nika: copie\npermits:\n  tools: [\"nika:read\", \"nika:write\"]\n  fs:\n    read: [\"entree.txt\"]\n    write: [\"a.txt\"]\ntasks:\n  read_source:\n    invoke:\n      tool: \"nika:read\"\n      args:\n        path: \"entree.txt\"\n  write_dest:\n    with:\n      content: \"${{ tasks.read_source.output }}\"\n    invoke:\n      tool: \"nika:write\"\n      args:\n        path: \"a.txt\"\n        content: \"${{ with.content }}\"\n";

const HOT_INTENT: &str = "Read ./notes/brief.md, summarize it in three bullets, and write the summary to ./out/summary.md";

// ── Witnesses of current routing at the pinned base ───────────────────────────

#[tokio::test]
async fn witness_foundry_escalation_requests_candidate_source_and_the_plan_never_reads_it() {
    let provider = Script::texts(&[native_answer()]);
    let request = CompileRequest::create(INTENT)
        .with_authoring_policy(policy(NativeMode::Escalate, 0))
        .with_authoring_knowledge(foundry());
    let out = compile_with_provider(&request, &provider).await.unwrap();
    // The first and only generative call asks for complete source, never for a plan.
    assert!(provider.calls() >= 1, "{out:#?}");
    assert_eq!(provider.schemas()[0], source_schema());
    assert!(route(&out).contains(&"native: informed generation".to_owned()));
    assert_eq!(out.provenance.strategy, Some(Strategy::Native));
    // The same open request without attached knowledge opens with the plan, whose messages
    // carry only the instructions and the request: no attached reference reaches it.
    let plain = Script::texts(&[common::plan().to_string()]);
    let request = CompileRequest::create(INTENT).with_authoring_policy(policy(NativeMode::Off, 0));
    let _ = compile_with_provider(&request, &plain).await.unwrap();
    assert_eq!(plain.schemas()[0], plan_schema());
    let seen = plain.seen.lock().unwrap();
    assert_eq!(seen[0].user, INTENT);
    assert!(!seen[0].system.contains("pattern:customer-reply"));
}

#[tokio::test]
async fn witness_a_nonconstant_revision_requests_direct_source_even_under_sketch() {
    for mode in [NativeMode::Sketch, NativeMode::Escalate, NativeMode::Only] {
        let provider = Script::texts(&[native_answer()]);
        let request = CompileRequest::edit(
            REVISION_BASE,
            "Finalement, résume le texte avant de l'écrire.",
        )
        .with_original_intent("Copie entree.txt dans a.txt.")
        .with_authoring_policy(policy(mode, 0));
        let out = compile_with_provider(&request, &provider).await.unwrap();
        assert!(provider.calls() >= 1, "{mode:?}: {out:#?}");
        assert_eq!(provider.schemas()[0], source_schema(), "{mode:?}");
        assert!(
            route(&out)
                .iter()
                .any(|step| step.starts_with("edit: the constant door could not settle")),
            "{mode:?}: {:?}",
            route(&out)
        );
    }
}

#[tokio::test]
async fn witness_sketch_skips_the_zero_call_hot_reading() {
    // The same request settles HOT with zero calls under the default doors.
    let hot = Script::texts(&[native_answer()]);
    let out = compile_with_provider(
        &CompileRequest::create(HOT_INTENT).with_authoring_policy(policy(NativeMode::Escalate, 0)),
        &hot,
    )
    .await
    .unwrap();
    assert_eq!(out.provenance.strategy, Some(Strategy::Hot), "{out:#?}");
    assert_eq!(hot.calls(), 0);
    // Under Sketch the seat is called before HOT is ever tried.
    let sketch = Script::texts(&[
        json!({"name": "x", "tasks": [], "questions": [], "gaps": [], "notes": ""}).to_string(),
    ]);
    let out = compile_with_provider(
        &CompileRequest::create(HOT_INTENT).with_authoring_policy(policy(NativeMode::Sketch, 0)),
        &sketch,
    )
    .await
    .unwrap();
    assert!(sketch.calls() >= 1);
    assert_eq!(route(&out)[0], "native: sketch");
    assert!(!route(&out).iter().any(|step| step == "hot"));
}

const SKETCH_INTENT: &str =
    "Lis ./tickets.json, résume les tickets ouverts et écris le résumé dans ./out/recap.md";

/// The accepted sketch of the `compile_native` sketch suite: read, filter, summarize, write.
fn recap_sketch() -> Value {
    let task = |id: &str, verb: &str, tool: Option<&str>, extra: Value| {
        let mut t = json!({"id": id, "verb": verb, "purpose": id});
        if let Some(tool) = tool {
            t["tool"] = json!(tool);
        }
        for (k, v) in extra.as_object().unwrap() {
            t[k] = v.clone();
        }
        t
    };
    json!({"name": "recap-tickets", "tasks": [
        task("read_tickets", "invoke", Some("nika:read"), json!({"reads": ["./tickets.json"]})),
        task("open_only", "invoke", Some("nika:jq"), json!({"with": [{"name": "document", "from": "read_tickets"}]})),
        task("summarize", "infer", None, json!({"with": [{"name": "tickets", "from": "open_only"}]})),
        task("write_recap", "invoke", Some("nika:write"), json!({"writes": ["./out/recap.md"], "with": [{"name": "text", "from": "summarize"}]})),
    ], "questions": [], "gaps": [], "notes": "read → filter → summarize → write"})
}

fn valid_fills() -> Vec<Value> {
    vec![
        json!({"task": "open_only", "field": "expression", "value": "fromjson | map(select(.status == \"open\"))"}),
        json!({"task": "summarize", "field": "prompt", "value": "Résume ces tickets ouverts sans rien inventer: ${{ with.tickets }}"}),
    ]
}

fn native_rounds(out: &CompileOutcome) -> Vec<Value> {
    out.provenance.decision.as_ref().unwrap()["native"]["rounds"]
        .as_array()
        .cloned()
        .unwrap_or_default()
}

fn malformed_fills() -> Vec<(&'static str, Vec<Value>)> {
    let with = |extra: Value| {
        let mut fills = valid_fills();
        fills.push(extra);
        fills
    };
    vec![
        (
            "ghost_task",
            with(json!({"task": "ghost", "field": "prompt", "value": SECRET})),
        ),
        (
            "duplicate",
            with(json!({"task": "summarize", "field": "prompt", "value": "Invente des tickets."})),
        ),
        (
            "undeclared_field",
            with(json!({"task": "summarize", "field": "temperature", "value": 2})),
        ),
        (
            "wrong_type",
            vec![
                json!({"task": "open_only", "field": "expression", "value": 42}),
                valid_fills()[1].clone(),
            ],
        ),
        ("missing_required", vec![valid_fills()[1].clone()]),
        (
            "args_path_override",
            with(
                json!({"task": "write_recap", "field": "args.path", "value": "./out/elsewhere.md"}),
            ),
        ),
        (
            "args_object_override",
            with(
                json!({"task": "read_tickets", "field": "args", "value": {"path": "./out/recap.md"}}),
            ),
        ),
    ]
}

/// A declared fill as the journal keeps it: the closed form the document reads, the raw digest,
/// and how many keys the fill carried beside `task`, `field` and `value`.
fn consumed_fill(fill: &Value) -> Value {
    let ignored = fill
        .as_object()
        .unwrap()
        .keys()
        .filter(|k| !["task", "field", "value"].contains(&k.as_str()))
        .count();
    json!({"task": fill["task"], "field": fill["field"], "value": fill["value"],
        "sha256": sha(&fill.to_string()), "ignored_keys": ignored})
}

/// The recap sketch as the parse consumed it, every field it reads in closed form.
fn consumed_recap(raw: &Value) -> Value {
    let tasks: Vec<Value> = raw["tasks"]
        .as_array()
        .unwrap()
        .iter()
        .map(|t| {
            json!({
                "id": t["id"], "verb": t["verb"], "tool": t.get("tool"),
                "reads": t.get("reads").cloned().unwrap_or_else(|| json!([])),
                "writes": t.get("writes").cloned().unwrap_or_else(|| json!([])),
                "hosts": [], "after": [],
                "with": t.get("with").cloned().unwrap_or_else(|| json!([])),
                "gated_by": null, "for_each": null, "purpose": t["purpose"],
            })
        })
        .collect();
    json!({"name": raw["name"], "tasks": tasks})
}

fn forensic(out: &CompileOutcome) -> Value {
    out.provenance.decision.as_ref().unwrap()["forensic"].clone()
}

fn context(out: &CompileOutcome) -> Vec<Value> {
    out.provenance
        .authoring
        .as_ref()
        .map(|r| r.context.clone())
        .unwrap_or_default()
}

fn sha(text: &str) -> String {
    nika_compile::surface::sha256(text)
}

async fn sketch_with_fills(fills: Vec<Value>) -> (CompileOutcome, Script) {
    let provider = Script::texts(&[
        recap_sketch().to_string(),
        json!({"fills": fills, "notes": "fills"}).to_string(),
    ]);
    let request =
        CompileRequest::create(SKETCH_INTENT).with_authoring_policy(policy(NativeMode::Sketch, 0));
    let out = compile_with_provider(&request, &provider).await.unwrap();
    (out, provider)
}

/// Formerly the witness `witness_fills_outside_the_declared_holes_vanish_from_the_candidate_without_a_refusal`
/// (COMPILER-01, frozen at c15cf94: ghost/duplicate/undeclared fills accepted with the valid
/// bytes, `expression: 42` emitted, overrides refused only by Check). Since the sketch emission
/// integrity slice, each is refused by the fill laws before any document exists.
#[tokio::test]
async fn fills_outside_the_declared_holes_are_refused_before_emission() {
    for (name, fills) in malformed_fills() {
        let (out, provider) = sketch_with_fills(fills.clone()).await;
        let native = &out.provenance.decision.as_ref().unwrap()["native"];
        let rounds = native_rounds(&out);
        assert_eq!(provider.calls(), 2, "{name}: no call is added");
        assert_eq!(native["accepted"], false, "{name}");
        assert!(out.candidate.is_none(), "{name}");
        assert!(
            rounds[1].get("candidate_sha256").is_none(),
            "{name}: {:#}",
            rounds[1]
        );
        let kinds: Vec<&str> = rounds[1]["diagnostics"]
            .as_array()
            .unwrap()
            .iter()
            .map(|d| d["kind"].as_str().unwrap())
            .collect();
        assert!(
            !kinds.is_empty() && kinds.iter().all(|k| *k == "fill"),
            "{name}: {kinds:?}"
        );
        // No document was emitted from these fills: every fill is kept by digest only.
        let kept = rounds[1]["proposed_fills"].as_array().unwrap();
        assert_eq!(kept.len(), fills.len(), "{name}");
        for (fill, kept) in fills.iter().zip(kept) {
            assert_eq!(kept["withheld"], true, "{name}: {kept}");
            assert_eq!(kept["sha256"], sha(&fill.to_string()), "{name}");
            assert!(kept.get("value").is_none(), "{name}: {kept}");
        }
        assert!(
            !outcome_document(&out).to_string().contains(SECRET),
            "{name}"
        );
        let summary = forensic(&out);
        assert_eq!(summary["door"]["name"], "sketch", "{name}");
        assert_eq!(summary["door"]["source_owner"], "none", "{name}");
        assert_eq!(summary["evidence"]["behavioral_judge"]["state"], "not_run");
        assert_eq!(summary["evidence"]["satisfaction"], "UNKNOWN");
    }
}

// ── The forensic record ────────────────────────────────────────────────────────

#[tokio::test]
async fn a_cold_repair_keeps_both_exact_proposals_and_never_presents_the_attached_foundry() {
    // The first proposal cites evidence the request never wrote; the one repair call fixes it.
    let mut unanchored = common::plan();
    unanchored["steps"][0]["evidence"] = json!("consulte les clients fidèles");
    let replies = [unanchored.to_string(), common::plan().to_string()];
    let provider = Script::texts(&replies);
    let judged = common::Judged::approving(&provider);
    let request = CompileRequest::create(INTENT)
        .with_authoring_policy(policy(NativeMode::Off, 0))
        .with_authoring_knowledge(foundry());
    let out = compile_with_provider(&request, &judged).await.unwrap();
    let calls = context(&out);
    // No call is added: the receipt counts exactly the calls the provider served.
    let receipt = out.provenance.authoring.as_ref().unwrap();
    assert_eq!(receipt.calls as usize, provider.calls(), "{calls:#?}");
    assert_eq!(calls.len(), provider.calls());
    assert_eq!(calls[0]["call"], "plan");
    assert_eq!(calls[1]["call"], "repair");
    for (k, reply) in replies.iter().enumerate() {
        assert_eq!(calls[k]["response"]["sha256"], sha(reply), "{k}");
        assert_eq!(calls[k]["response"]["bytes"], reply.len(), "{k}");
        assert_eq!(calls[k]["proposed"]["decoded"], true, "{k}");
        assert_eq!(calls[k]["proposed"]["sha256"], sha(reply), "{k}");
        let exact: Value = serde_json::from_str(reply).unwrap();
        assert_eq!(calls[k]["proposed"]["object"], exact, "{k}");
        assert_eq!(calls[k]["references"], json!([]), "{k}");
    }
    let summary = forensic(&out);
    assert_eq!(summary["version"], 1);
    assert_eq!(summary["door"]["name"], "cold_plan", "{summary:#}");
    assert_eq!(summary["door"]["source_owner"], "deterministic_assembler");
    assert_eq!(summary["proposal"]["kind"], "plan");
    assert_eq!(summary["proposal"]["state"], "captured");
    assert_eq!(
        summary["doors_tried"]["cold_plan"]["called"],
        "cold: 1 sample(s)"
    );
    assert!(
        summary["doors_tried"]["hot"]["rejected"].is_string(),
        "{summary:#}"
    );
    // The attached reference was never carried by any call of this door.
    assert_eq!(summary["foundry"]["prepared"], json!([]));
    assert_eq!(summary["foundry"]["presented"], json!([]));
    assert_eq!(
        summary["foundry"]["attached_not_presented"],
        json!(["pattern:customer-reply"])
    );
    assert_eq!(
        summary["intent"]["original_sha256"],
        nika_compile::intent_sha256(INTENT)
    );
    assert_eq!(summary["intent"]["replaced_by_clarification"], false);
    // The composed plans are the universe this route built, kept where the composer wrote them.
    let universe = &summary["universe"];
    assert_eq!(universe["state"], "recorded", "{universe:#}");
    assert_eq!(universe["path"], "provenance.decision.candidates");
    let candidates = out.provenance.decision.as_ref().unwrap()["candidates"].clone();
    assert_eq!(universe["count"], candidates.as_array().unwrap().len());
    assert_eq!(
        universe["selected_candidate"],
        out.provenance.decision.as_ref().unwrap()["selected_candidate"]
    );
    // The real compile wire carries the record, versioned, without any reference text.
    let doc = outcome_document(&out);
    assert_eq!(doc["compile_version"], 2);
    assert_eq!(doc["provenance"]["decision"]["forensic"], summary);
    assert!(!doc.to_string().contains(SECRET));
}

#[tokio::test]
async fn a_refused_proposal_is_kept_by_digest_and_shape_never_by_its_text() {
    // An answer outside the closed plan shape that echoes a secret-like value.
    let refused =
        json!({"steps": [{"op": "lookup"}], "effects": [], "api_key": SECRET}).to_string();
    let provider = Script::texts(std::slice::from_ref(&refused));
    let request = CompileRequest::create(INTENT).with_authoring_policy(policy(NativeMode::Off, 0));
    let out = compile_with_provider(&request, &provider).await.unwrap();
    let calls = context(&out);
    assert_eq!(calls.len(), provider.calls());
    let proposed = &calls[0]["proposed"];
    assert_eq!(proposed["decoded"], false);
    assert_eq!(proposed["withheld"], true);
    assert_eq!(proposed["sha256"], sha(&refused));
    assert_eq!(proposed["bytes"], refused.len());
    assert!(proposed.get("object").is_none(), "{proposed}");
    assert_eq!(
        proposed["shape"],
        json!({"type": "object", "known_keys": ["effects", "steps"], "other_keys": 1})
    );
    assert!(!outcome_document(&out).to_string().contains(SECRET));
    assert!(out.candidate.is_none());
    let summary = forensic(&out);
    assert_eq!(summary["door"]["source_owner"], "none", "{summary:#}");
    assert_eq!(summary["evidence"]["candidate_sha256"], Value::Null);
}

#[tokio::test]
async fn failed_and_unmetered_calls_stay_in_the_journal_with_unknown_usage() {
    // A provider failure: the call is journaled, nothing was returned, usage is not complete.
    let failing = Script::new(vec![Reply::Fail]);
    let request = CompileRequest::create(INTENT).with_authoring_policy(policy(NativeMode::Off, 0));
    let out = compile_with_provider(&request, &failing).await.unwrap();
    let calls = context(&out);
    assert_eq!(calls.len(), 1);
    assert_eq!(failing.calls(), 1);
    assert_eq!(calls[0]["result"]["failure_kind"], "provider_error");
    assert_eq!(calls[0]["response"], Value::Null);
    assert!(calls[0].get("proposed").is_none());
    let summary = forensic(&out);
    assert_eq!(summary["calls"]["generative"]["failed"], 1);
    assert_eq!(summary["calls"]["generative"]["usage"], "incomplete");
    assert_eq!(summary["calls"]["generative"]["input_tokens"], Value::Null);
    // An answer without usage: the payload is kept, the tokens stay unknown, never zero.
    let unmetered = Script::new(vec![Reply::Unmetered(common::plan().to_string())]);
    let out = compile_with_provider(&request, &unmetered).await.unwrap();
    let receipt = out.provenance.authoring.as_ref().unwrap();
    assert_eq!(receipt.input_tokens, None);
    assert_eq!(receipt.output_tokens, None);
    let calls = context(&out);
    assert_eq!(calls[0]["result"]["usage_reported"], false);
    assert_eq!(calls[0]["proposed"]["object"], common::plan());
    let summary = forensic(&out);
    assert_eq!(summary["calls"]["generative"]["usage"], "incomplete");
    assert_eq!(summary["calls"]["generative"]["input_tokens"], Value::Null);
    assert_eq!(summary["calls"]["generative"]["output_tokens"], Value::Null);
}

#[tokio::test]
async fn source_direct_generation_records_what_it_was_shown_and_invents_no_plan() {
    let provider = Script::texts(&[native_answer()]);
    let request = CompileRequest::create(INTENT)
        .with_authoring_policy(policy(NativeMode::Escalate, 1))
        .with_authoring_knowledge(foundry());
    let out = compile_with_provider(&request, &provider).await.unwrap();
    let calls = context(&out);
    assert_eq!(calls.len(), provider.calls());
    let sent = out.provenance.decision.as_ref().unwrap()["native"]["references"].clone();
    assert!(
        sent.as_array()
            .unwrap()
            .iter()
            .any(|r| r["id"] == "pattern:customer-reply")
    );
    for call in calls
        .iter()
        .filter(|c| c["call"].as_str().unwrap().starts_with("native"))
    {
        // Each source call names the references its system message carried.
        assert_eq!(call["references"], sent, "{call:#}");
        assert!(
            call.get("proposed").is_none(),
            "no plan is read from source"
        );
    }
    // The canary reached the seat and is digested, never copied, into the record.
    assert!(provider.seen.lock().unwrap()[0].system.contains(SECRET));
    assert!(!outcome_document(&out).to_string().contains(SECRET));
    let summary = forensic(&out);
    assert_eq!(summary["door"]["name"], "native_source", "{summary:#}");
    assert_eq!(
        summary["door"]["reason"],
        "escalate_with_attached_foundry_references"
    );
    assert_eq!(summary["proposal"]["kind"], "source");
    assert_eq!(summary["proposal"]["state"], "NOT_CAPTURED");
    assert_eq!(summary["doors_tried"]["cold_plan"], "not_tried");
    assert_eq!(summary["universe"]["state"], "not_applicable");
    assert_eq!(summary["foundry"]["attached_not_presented"], json!([]));
    assert_eq!(
        summary["attempts"].as_array().unwrap().len(),
        native_rounds(&out).len()
    );
    let owner = summary["door"]["source_owner"].as_str().unwrap();
    let kept = out.candidate.is_some() || out.provenance.plan.is_some();
    assert_eq!(owner, if kept { "model" } else { "none" }, "{summary:#}");
}

#[tokio::test]
async fn an_accepted_sketch_keeps_its_exact_graph_and_fills_bound_to_the_candidate() {
    let (out, provider) = sketch_with_fills(valid_fills()).await;
    let rounds = native_rounds(&out);
    let sketch = recap_sketch();
    let exact = json!({"name": sketch["name"], "tasks": sketch["tasks"]});
    let kept = &rounds[0]["proposed_sketch"];
    assert_eq!(kept["sha256"], sha(&exact.to_string()));
    assert_eq!(kept["sha256"], rounds[0]["sketch_sha256"]);
    assert_eq!(kept["ignored_keys"], 0);
    let consumed = consumed_recap(&sketch);
    assert_eq!(kept["name"], consumed["name"]);
    assert_eq!(kept["tasks"], consumed["tasks"]);
    let fills: Vec<Value> = valid_fills().iter().map(consumed_fill).collect();
    assert_eq!(rounds[1]["proposed_fills"], json!(fills));
    assert_eq!(forensic(&out)["universe"]["state"], "not_applicable");
    let calls = context(&out);
    assert_eq!(calls.len(), provider.calls());
    assert_eq!(calls[0]["call"], "sketch");
    assert_eq!(calls[1]["call"], "fill");
    assert!(!calls[0]["references"].as_array().unwrap().is_empty());
    let summary = forensic(&out);
    assert_eq!(
        summary["door"]["source_owner"],
        "compiler_from_model_sketch_and_fills"
    );
    assert_eq!(summary["proposal"]["kind"], "sketch_and_fills");
    assert_eq!(summary["doors_tried"]["hot"], "not_tried");
    let attempts = summary["attempts"].as_array().unwrap();
    assert_eq!(attempts[0]["sketch_sha256"], rounds[0]["sketch_sha256"]);
    assert_eq!(
        attempts[1]["candidate_sha256"],
        rounds[1]["candidate_sha256"]
    );
    // Nothing judged the business behavior: no READY is presented as proof.
    assert_eq!(summary["evidence"]["semantic_judge"]["state"], "not_run");
    assert_eq!(summary["evidence"]["rehearsal"]["state"], "not_offered");
}

#[tokio::test]
async fn hot_names_the_elided_calls_and_a_seatless_compile_is_unchanged() {
    let provider = Script::texts(&[native_answer()]);
    let request =
        CompileRequest::create(HOT_INTENT).with_authoring_policy(policy(NativeMode::Escalate, 0));
    let out = compile_with_provider(&request, &provider).await.unwrap();
    assert_eq!(provider.calls(), 0);
    assert!(out.provenance.authoring.is_none());
    let summary = forensic(&out);
    assert_eq!(summary["door"]["name"], "hot", "{summary:#}");
    assert_eq!(summary["door"]["source_owner"], "deterministic_assembler");
    assert_eq!(summary["doors_tried"]["hot"], "admitted");
    assert_eq!(summary["calls"]["generative"]["count"], 0);
    assert_eq!(summary["calls"]["generative"]["usage"], "none");
    assert_eq!(summary["proposal"]["state"], "deterministic");
    assert_eq!(summary["universe"]["state"], "not_applicable");
    // Without a seat or provider the compile keeps its record exactly as before.
    let plain = nika_compile::compile(&CompileRequest::create(HOT_INTENT)).unwrap();
    assert!(
        plain
            .provenance
            .decision
            .as_ref()
            .unwrap()
            .get("forensic")
            .is_none()
    );
    let mut stripped = out.provenance.decision.clone().unwrap();
    stripped.as_object_mut().unwrap().remove("forensic");
    assert_eq!(Some(stripped), plain.provenance.decision);
    assert_eq!(out.candidate, plain.candidate);
    assert_eq!(out.questions, plain.questions);
}

#[tokio::test]
async fn a_nonconstant_revision_is_recorded_as_source_direct_with_its_base() {
    let provider = Script::texts(&[native_answer()]);
    let request = CompileRequest::edit(
        REVISION_BASE,
        "Finalement, résume le texte avant de l'écrire.",
    )
    .with_original_intent("Copie entree.txt dans a.txt.")
    .with_authoring_policy(policy(NativeMode::Sketch, 0));
    let out = compile_with_provider(&request, &provider).await.unwrap();
    let summary = forensic(&out);
    assert_eq!(summary["intent"]["kind"], "edit", "{summary:#}");
    assert_eq!(summary["intent"]["base_sha256"], sha(REVISION_BASE));
    assert_eq!(summary["intent"]["change"]["kind"], "text");
    assert_eq!(
        summary["intent"]["original_intent_sha256"],
        nika_compile::intent_sha256("Copie entree.txt dans a.txt.")
    );
    assert_eq!(summary["door"]["reason"], "nonconstant_revision");
    assert_eq!(summary["proposal"]["state"], "NOT_CAPTURED");
}

#[tokio::test]
async fn references_are_presented_only_by_an_answered_call() {
    let request = CompileRequest::create(INTENT)
        .with_authoring_policy(policy(NativeMode::Escalate, 0))
        .with_authoring_knowledge(foundry());
    for (reply, delivery) in [(Reply::Refuse, "not_sent"), (Reply::Fail, "unknown")] {
        let provider = Script::new(vec![reply]);
        let out = compile_with_provider(&request, &provider).await.unwrap();
        let calls = context(&out);
        assert_eq!(calls.len(), 1, "{delivery}: {calls:#?}");
        assert_eq!(calls[0]["call"], "native");
        assert_eq!(calls[0]["response"], Value::Null);
        let summary = forensic(&out);
        let foundry = &summary["foundry"];
        assert!(
            foundry["prepared"]
                .as_array()
                .unwrap()
                .contains(&json!("pattern:customer-reply")),
            "{foundry:#}"
        );
        assert_eq!(foundry["presented"], json!([]), "{delivery}");
        assert_eq!(
            foundry["attached_not_presented"],
            json!(["pattern:customer-reply"]),
            "{delivery}"
        );
        let unknown = foundry["delivery_unknown"].as_array().unwrap();
        assert_eq!(
            unknown.contains(&json!("pattern:customer-reply")),
            delivery == "unknown",
            "{delivery}: {foundry:#}"
        );
        assert_eq!(summary["calls"]["generative"]["delivery"][delivery], 1);
        assert_eq!(summary["calls"]["generative"]["failed"], 1);
    }
    // An answered call is the only confirmation the references reached a model.
    let answered = Script::texts(&[native_answer()]);
    let out = compile_with_provider(&request, &answered).await.unwrap();
    let summary = forensic(&out);
    assert!(
        summary["foundry"]["presented"]
            .as_array()
            .unwrap()
            .contains(&json!("pattern:customer-reply"))
    );
    assert_eq!(
        summary["calls"]["generative"]["delivery"]["answered"],
        answered.calls()
    );
}

#[tokio::test]
async fn a_sketch_the_floor_refuses_before_any_call_keeps_its_door() {
    let intent =
        "Lis ./clients.csv et crédite le compte de chaque client en retard sans mon accord";
    let provider = Script::texts(&[recap_sketch().to_string()]);
    let request =
        CompileRequest::create(intent).with_authoring_policy(policy(NativeMode::Sketch, 2));
    let out = compile_with_provider(&request, &provider).await.unwrap();
    assert_eq!(provider.calls(), 0);
    assert!(out.provenance.authoring.is_none());
    assert!(out.candidate.is_none());
    let summary = forensic(&out);
    assert_eq!(summary["door"]["name"], "sketch", "{summary:#}");
    assert_eq!(summary["door"]["reason"], "policy_sketch_before_hot");
    assert_eq!(summary["door"]["source_owner"], "none");
    assert_eq!(summary["calls"]["generative"]["count"], 0);
    assert_eq!(summary["evidence"]["status"], "refused");
}

#[tokio::test]
async fn a_refused_sketch_is_kept_by_digest_and_its_repair_exactly() {
    // Round 0 reads a file the request never states and carries a canary in a purpose.
    let mut refused = recap_sketch();
    refused["tasks"][0]["reads"] = json!(["./data/tickets.json"]);
    refused["tasks"][0]["purpose"] = json!(SECRET);
    let provider = Script::texts(&[
        refused.to_string(),
        recap_sketch().to_string(),
        json!({"fills": valid_fills(), "notes": "fills"}).to_string(),
    ]);
    let request =
        CompileRequest::create(SKETCH_INTENT).with_authoring_policy(policy(NativeMode::Sketch, 1));
    let out = compile_with_provider(&request, &provider).await.unwrap();
    let rounds = native_rounds(&out);
    assert_eq!(rounds.len(), 3, "{rounds:#?}");
    let record = json!({"name": refused["name"], "tasks": refused["tasks"]});
    let kept = &rounds[0]["proposed_sketch"];
    assert_eq!(kept["withheld"], true, "{kept}");
    assert_eq!(kept["sha256"], sha(&record.to_string()));
    assert_eq!(kept["sha256"], rounds[0]["sketch_sha256"]);
    assert!(!rounds[0]["diagnostics"].as_array().unwrap().is_empty());
    let accepted = consumed_recap(&recap_sketch());
    assert_eq!(rounds[1]["proposed_sketch"]["tasks"], accepted["tasks"]);
    assert_eq!(rounds[1]["proposed_sketch"]["ignored_keys"], 0);
    assert!(!outcome_document(&out).to_string().contains(SECRET));
    let calls = context(&out);
    assert_eq!(calls.len(), provider.calls());
    assert_eq!(calls[1]["call"], "sketch-repair");
}

/// Formerly `keys_the_compiler_ignores_in_accepted_fills_and_sketches_never_reach_the_record`
/// (COMPILER-01: an extra key beside a valid fill, task or edge was ignored and the candidate
/// emitted). The closed shapes now refuse them by their path; the canary still never reaches the
/// wire and no call is added.
#[tokio::test]
async fn keys_outside_the_closed_fill_and_sketch_shapes_are_refused_unechoed() {
    let mut fills = valid_fills();
    fills[1]["api_key"] = json!(SECRET);
    let (out, provider) = sketch_with_fills(fills.clone()).await;
    let rounds = native_rounds(&out);
    assert_eq!(provider.calls(), 2);
    assert!(out.candidate.is_none());
    assert!(
        rounds[1].get("candidate_sha256").is_none(),
        "{:#}",
        rounds[1]
    );
    assert!(
        rounds[1]["diagnostics"].to_string().contains("fills[1]"),
        "{:#}",
        rounds[1]
    );
    assert_eq!(rounds[1]["proposed_fills"][1]["withheld"], true);
    assert!(!outcome_document(&out).to_string().contains(SECRET));
    // A sketch whose task and edge carry keys outside the closed shapes is never filled.
    let mut sketch = recap_sketch();
    sketch["tasks"][1]["api_key"] = json!(SECRET);
    sketch["tasks"][1]["with"][0]["token"] = json!(SECRET);
    let provider = Script::texts(&[
        sketch.to_string(),
        json!({"fills": valid_fills(), "notes": "fills"}).to_string(),
    ]);
    let request =
        CompileRequest::create(SKETCH_INTENT).with_authoring_policy(policy(NativeMode::Sketch, 0));
    let out = compile_with_provider(&request, &provider).await.unwrap();
    let rounds = native_rounds(&out);
    assert_eq!(provider.calls(), 1);
    assert_eq!(rounds.len(), 1, "{rounds:#?}");
    assert!(rounds[0]["diagnostics"].to_string().contains("tasks[1]"));
    assert_eq!(rounds[0]["proposed_sketch"]["withheld"], true);
    assert!(!outcome_document(&out).to_string().contains(SECRET));
}
