// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>
//! A fresh CREATE through the compile door reaches the complete document (R5 · C13): the FIRST
//! authoring call of a new intent asks for the whole document in the whole language, under every
//! policy that permits a native door, with no plan or sketch round to fail first. The document
//! is judged, settled, asked and bound as any candidate, and a refusal is repaired in the same
//! talk.
//!
//! SCRIPTED hermetic doubles only: `Author` answers fixed documents in order and records the
//! schema of every authoring call; the verifier's whole-request judge is approved
//! (`common::Judged`). The documents are pack examples or local fixtures, and the strict parser
//! and Check judge them again here. A scripted answer is not a model's: these tests prove the
//! door, its laws and its records, never a model's ability to write such a document.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

use common::Judged;
use nika_compile::surface::{literal_projection, sha256};
use nika_compile::{AuthoringPolicy, CompileOutcome, CompileRequest, CompileStatus, NativeMode};
use nika_compile_cognition::{Cognition, compile_with_cognition_composed};
use nika_kernel::ai::provider::{
    ContentBlock, InferRequest, InferResponse, ProviderError, ProviderInferDyn, ResponseFormat,
    Role, StopReason, TokenUsage,
};
use serde_json::{Value, json};
use std::sync::Mutex;
use std::time::Duration;

const CONFIG_INTENT: &str = include_str!("fixtures/document_create/config-values.intent.txt");
const RICH_INTENT: &str = include_str!("fixtures/document_create/rich-control.intent.txt");
const RICH: &str = include_str!("fixtures/document_create/rich-control.nika");
const CHILD_INTENT: &str = include_str!("fixtures/document_create/child-pipeline.intent.txt");
const AUDIT_CONFIG_INTENT: &str =
    include_str!("fixtures/document_create/config-values.audit-intent.txt");
const WEBHOOK_INTENT: &str = include_str!("fixtures/document_create/digest-webhook.intent.txt");
const STALE_INTENT: &str = include_str!("fixtures/document_create/stale-filter.intent.txt");
const STALE_BUILT: &str = include_str!("fixtures/document_create/stale-construction.nika");

/// The roles of the doors a complete-document creation must not need first.
const RETIRED_FIRST: &[&str] = &[
    "plan",
    "repair",
    "sketch",
    "sketch-repair",
    "fill",
    "fill-repair",
    "source-recovery",
    "source-recovery-repair",
];

/// One authoring call as the scripted author received it.
struct Call {
    schema: Vec<String>,
    system: String,
    last: String,
}

/// A scripted author: the documents it answers, in order, and every authoring call it received.
/// A call past the script fails as a provider would.
struct Author {
    answers: Vec<String>,
    calls: Mutex<Vec<Call>>,
}

impl Author {
    fn new(answers: Vec<String>) -> Self {
        Self {
            answers,
            calls: Mutex::new(Vec::new()),
        }
    }

    fn schema(&self, at: usize) -> Vec<String> {
        self.calls.lock().unwrap()[at].schema.clone()
    }

    fn count(&self) -> usize {
        self.calls.lock().unwrap().len()
    }
}

fn text(content: &[ContentBlock]) -> String {
    (content.iter())
        .filter_map(|block| match block {
            ContentBlock::Text { text } => Some(text.as_str()),
            _ => None,
        })
        .collect()
}

impl ProviderInferDyn for Author {
    async fn infer(&self, request: InferRequest) -> Result<InferResponse, ProviderError> {
        let mut schema: Vec<String> = match &request.response_format {
            ResponseFormat::JsonSchema(schema) => (schema["properties"].as_object())
                .map(|properties| properties.keys().cloned().collect())
                .unwrap_or_default(),
            _ => Vec::new(),
        };
        schema.sort();
        let system = (request.messages.iter())
            .find(|m| matches!(m.role, Role::System))
            .map_or_else(String::new, |m| text(&m.content));
        let last = request
            .messages
            .last()
            .map_or_else(String::new, |m| text(&m.content));
        let at = {
            let mut calls = self.calls.lock().unwrap();
            calls.push(Call {
                schema,
                system,
                last,
            });
            calls.len() - 1
        };
        match self.answers.get(at) {
            Some(answer) => Ok(InferResponse::new(
                vec![ContentBlock::Text {
                    text: answer.clone(),
                }],
                TokenUsage::new(300, 200),
                StopReason::EndTurn,
            )),
            None => Err(ProviderError::Other {
                reason: "the scripted author has no further answer".to_owned(),
            }),
        }
    }
}

/// The document door's answer: the whole source, operations over it and the business questions.
fn answer(source: &str, operations: &Value, questions: &Value) -> String {
    json!({"candidate": source, "candidate_lines": [], "operations": operations,
        "questions": questions, "gaps": [], "notes": "scripted"})
    .to_string()
}

fn written(source: &str) -> String {
    answer(source, &json!([]), &json!([]))
}

fn policy(native: NativeMode) -> AuthoringPolicy {
    AuthoringPolicy::new("mock/authoring", 8192, Duration::from_secs(5)).with_native(native)
}

async fn create(request: &CompileRequest, author: &Author) -> CompileOutcome {
    let judged = Judged::approving(author);
    let cognition = Cognition {
        provider: Some(&judged),
        seat: None,
    };
    compile_with_cognition_composed(request, cognition, None, None)
        .await
        .unwrap()
}

/// The roles of every call the outcome's receipt journals, in order.
fn roles(out: &CompileOutcome) -> Vec<String> {
    (out.provenance.authoring.as_ref())
        .map(|receipt| {
            (receipt.context.iter())
                .filter_map(|call| call["call"].as_str().map(str::to_owned))
                .collect()
        })
        .unwrap_or_default()
}

fn decision(out: &CompileOutcome) -> &Value {
    out.provenance.decision.as_ref().expect("a decision record")
}

/// The first authoring call is the document door's, and no retired door was asked before it.
fn door_first(out: &CompileOutcome, author: &Author) {
    let schema = author.schema(0);
    for key in ["candidate", "candidate_lines", "operations", "questions"] {
        assert!(schema.iter().any(|k| k == key), "{key}: {schema:?}");
    }
    assert!(
        !schema.iter().any(|k| k == "tasks" || k == "steps"),
        "{schema:?}"
    );
    let roles = roles(out);
    assert_eq!(
        roles.first().map(String::as_str),
        Some("document"),
        "{roles:?}"
    );
    assert!(
        !roles.iter().any(|r| RETIRED_FIRST.contains(&r.as_str())),
        "{roles:?}"
    );
    let agenda = &decision(out)["agenda"];
    assert_eq!(agenda[0]["action"], "compose: document", "{agenda:#}");
}

/// The candidate is a workflow the strict parser reads and pure Check passes.
fn checked(candidate: &str) -> Value {
    let wf = nika_compile::parse(candidate).expect("the candidate parses strictly");
    let report = nika_check::check(&wf);
    assert!(report.is_clean(), "{:#?}", report.findings);
    literal_projection(candidate).expect("a literal document")
}

#[tokio::test]
async fn a_fresh_intent_reaches_the_complete_document_at_its_first_authoring_call() {
    let document = nika_pack::example("08-config-values").expect("the pack ships 08");
    let author = Author::new(vec![written(document)]);
    let request =
        CompileRequest::create(CONFIG_INTENT).with_authoring_policy(policy(NativeMode::Escalate));
    let out = create(&request, &author).await;
    door_first(&out, &author);
    let turns: Vec<String> = author
        .calls
        .lock()
        .unwrap()
        .iter()
        .map(|c| c.last.clone())
        .collect();
    assert_eq!(
        author.count(),
        1,
        "one authoring call, no repair: {turns:#?}"
    );
    assert_eq!(out.status, CompileStatus::Ready, "{:#?}", out.diagnostics);
    // The author's bytes are the candidate, comments and presentation included.
    let candidate = out.candidate.as_deref().expect("a candidate");
    assert_eq!(candidate, document);
    let doc = checked(candidate);
    let inputs = &doc["inputs"];
    assert_eq!(inputs["team"]["type"], "string");
    assert_eq!(
        (
            inputs["team"]["required"].clone(),
            inputs["team"]["default"].clone()
        ),
        (json!(true), json!("atelier"))
    );
    assert_eq!(
        (
            inputs["region"]["required"].clone(),
            inputs["region"]["default"].clone()
        ),
        (json!(false), json!("eu"))
    );
    assert_eq!(
        (
            inputs["verbose"]["type"].clone(),
            inputs["verbose"]["default"].clone()
        ),
        (json!("bool"), json!(false))
    );
    assert_eq!(doc["const"]["window_days"], 7, "a constant, never an input");
    assert_eq!(
        doc["tasks"]["debug_dump"]["when"],
        "${{ inputs.verbose == true }}"
    );
    assert_eq!(
        doc["tasks"]["debug_dump"]["with"]["report"],
        "${{ tasks.report.output }}"
    );
    assert_eq!(
        doc["outputs"]["report"],
        json!({"value": "${{ tasks.report.output }}", "description": "The shaped report · title carries team + region + window"})
    );
    // The author read the whole language and the door's own instruction.
    let calls = author.calls.lock().unwrap();
    assert!(
        calls[0].system.contains("# The document door"),
        "the door's instruction"
    );
    assert!(calls[0].system.contains("The language's complete schema"));
    assert!(
        calls[0].system.contains("\"max_parallel\""),
        "the Spec schema rides the call"
    );
    drop(calls);
    // The record says who wrote what: the whole document, no component, no preservation claimed.
    let created = &decision(&out)["document_create"];
    assert_eq!(created["mode"], "written", "{created:#}");
    assert!(
        created["preservation"]
            .as_str()
            .unwrap()
            .starts_with("none claimed")
    );
    assert_eq!(created["reuse"]["expanded"], 0);
    assert_eq!(
        decision(&out)["forensic"]["door"]["reason"],
        "complete_document_door"
    );
    // The settled document is bound to its final bytes under the request they answer.
    let plan = out.provenance.plan.as_ref().expect("a plan");
    assert_eq!(plan["strategy"], "native");
    let bound = &plan["document"];
    assert_eq!(bound["version"], 1);
    assert_eq!(bound["candidate_sha256"], json!(sha256(candidate)));
    assert_eq!(bound["request"], CONFIG_INTENT);
    assert_eq!(bound["base_sha256"], Value::Null);
    assert!(plan.get("source_revision").is_none(), "{plan:#}");
}

#[tokio::test]
async fn control_flow_the_sketch_cannot_state_is_composed_whole() {
    let author = Author::new(vec![written(RICH)]);
    let request =
        CompileRequest::create(RICH_INTENT).with_authoring_policy(policy(NativeMode::Escalate));
    let out = create(&request, &author).await;
    door_first(&out, &author);
    let turns: Vec<String> = author
        .calls
        .lock()
        .unwrap()
        .iter()
        .map(|c| c.last.clone())
        .collect();
    assert_eq!(
        out.status,
        CompileStatus::Ready,
        "{:#?} {turns:#?}",
        out.diagnostics
    );
    let candidate = out.candidate.as_deref().expect("a candidate");
    assert_eq!(
        candidate, RICH,
        "every byte kept: comments, quoting styles and the block"
    );
    let doc = checked(candidate);
    let fetch = &doc["tasks"]["fetch"];
    assert_eq!(
        fetch["retry"],
        json!({"max_attempts": 3, "backoff_ms": 250, "jitter": false})
    );
    assert_eq!(
        (fetch["timeout"].clone(), fetch["extract"].clone()),
        (json!("30s"), json!({"count": "length"}))
    );
    let report = &doc["tasks"]["report"];
    assert_eq!(report["after"], json!({"fetch": "failure"}));
    assert_eq!(report["when"], "${{ inputs.verbose == true }}");
    assert_eq!(
        report["for_each"],
        json!({"items": ["a", "b"], "max_parallel": 2, "fail_fast": false})
    );
    assert_eq!(
        (report["group"].clone(), report["on_error"].clone()),
        (json!("reports"), json!({"recover": "none"}))
    );
    assert_eq!(
        doc["tasks"]["persist"]["after"],
        json!({"report": "terminal"})
    );
    assert_eq!(doc["run"], json!({"entropy": "none"}));
    assert_eq!(doc["const"]["label"], "it's late");
    assert_eq!(
        doc["const"]["limits"],
        json!({"type": "integer", "value": 3})
    );
    assert_eq!(doc["outputs"]["total"], "${{ tasks.fetch.count }}");
}

/// A native child call the request names by its path, in the request's own words: the path law
/// takes the exact `invoke.workflow` target as realizing the named path (no `permits.fs` entry is
/// invented for it) and the author's document is accepted at its first call. A source-only
/// compile reads no child file, so the call stays a named, unjudged dependency of the preview
/// (`finish`: never READY on a child no reader judged), with nothing bound as settled bytes.
#[tokio::test]
async fn a_named_native_child_call_is_accepted_whole_and_its_unread_child_is_named() {
    let parent = nika_pack::example("10-compose-pipeline").expect("the pack ships 10");
    let author = Author::new(vec![written(parent)]);
    let request =
        CompileRequest::create(CHILD_INTENT).with_authoring_policy(policy(NativeMode::Escalate));
    let out = create(&request, &author).await;
    door_first(&out, &author);
    let turns: Vec<String> = author
        .calls
        .lock()
        .unwrap()
        .iter()
        .map(|c| c.last.clone())
        .collect();
    assert_eq!(
        author.count(),
        1,
        "the path law accepts the target, no repair: {turns:#?}"
    );
    // The document the author wrote is the preview, byte for byte: a native child call.
    let candidate = out
        .candidate
        .as_deref()
        .expect("the accepted document is previewed");
    assert_eq!(candidate, parent);
    let wf = nika_compile::parse(candidate).expect("parses");
    assert!(nika_check::check(&wf).is_clean());
    let call = (wf.tasks.iter())
        .find(|t| t.value.id.value == "call")
        .expect("the call task");
    let nika_schema::raw::RawAction::Invoke(invoke) = &call.value.action else {
        panic!("the call is an invoke");
    };
    let nika_schema::raw::RawInvokeTarget::Workflow(target) = &invoke.target else {
        panic!("a native child workflow, never a tool imitation");
    };
    assert_eq!(target.value, "./10-compose-child.nika");
    let doc = literal_projection(candidate).unwrap();
    assert_eq!(
        doc["tasks"]["call"]["invoke"]["args"]["topic"],
        "${{ inputs.topic }}"
    );
    assert_eq!(
        doc["tasks"]["call"]["returns"],
        json!({"object": {"summary": "string"}})
    );
    assert_eq!(
        doc["tasks"]["wrap"]["with"]["summary"],
        "${{ tasks.call.output.summary }}"
    );
    assert!(
        doc["permits"]["fs"].is_null(),
        "no file permit stands in for the child call"
    );
    // No law refused it: the only hold is the child no reader judged here, named on its task.
    assert!(
        !out.diagnostics
            .iter()
            .any(|d| d.message.contains("UNREALIZED PATH"))
    );
    assert_ne!(out.status, CompileStatus::Ready);
    let named = |d: &&nika_compile::CompileDiagnostic| {
        d.target == "call" && d.message.contains("cannot resolve this child workflow")
    };
    assert!(
        out.diagnostics.iter().any(|d| named(&d)),
        "{:#?}",
        out.diagnostics
    );
    let plan = out.provenance.plan.as_ref().expect("the door's record");
    assert_eq!(plan["document_create"]["mode"], "written");
    assert!(
        plan.get("document").is_none(),
        "an unjudged child binds no settled bytes"
    );
}

/// The audit's own wording, « Create a new workflow named config-values », asks to author the
/// workflow: the request to author is not an effect its tasks must carry. The original words
/// reach a READY whole document at the first call, with no repair, no endpoint and no
/// clarification manufactured from the framing, bound under those same words.
#[tokio::test]
async fn the_authoring_request_itself_needs_no_program_effect_or_clarification() {
    let document = nika_pack::example("08-config-values").expect("the pack ships 08");
    let author = Author::new(vec![written(document)]);
    let request = CompileRequest::create(AUDIT_CONFIG_INTENT)
        .with_authoring_policy(policy(NativeMode::Escalate));
    let out = create(&request, &author).await;
    door_first(&out, &author);
    assert_eq!(author.count(), 1, "the framing needs no repair");
    assert_eq!(out.status, CompileStatus::Ready, "{:#?}", out.diagnostics);
    assert_eq!(out.questions.len(), 0, "{:#?}", out.questions);
    assert_eq!(out.candidate.as_deref(), Some(document));
    let doc = checked(out.candidate.as_deref().expect("a candidate"));
    assert_eq!(doc["nika"], "config-values");
    assert_eq!(doc["tasks"].as_object().map(serde_json::Map::len), Some(2));
    let bound = &out.provenance.plan.as_ref().expect("a plan")["document"];
    assert_eq!(bound["candidate_sha256"], json!(sha256(document)));
    assert_eq!(bound["request"], AUDIT_CONFIG_INTENT);
}

/// A webhook the request names without its address, sent with an invented host (refused), then
/// with the placeholder the laws ask for.
fn digest(url: &str, http: &str, endpoint: &str) -> String {
    format!(
        r#"nika: digest-sender
inputs:
  team: {{ type: string, required: true }}
const:
  window_days: 7{endpoint}
permits:
  tools: ["nika:jq", "nika:fetch"]
  net: {{ http: [{http}] }}
tasks:
  report:
    invoke: {{ tool: "nika:jq", args: {{ input: {{ team: "${{{{ inputs.team }}}}", window_days: "${{{{ const.window_days }}}}" }}, expression: "." }} }}
  send:
    with: {{ payload: "${{{{ tasks.report.output }}}}" }}
    invoke: {{ tool: "nika:fetch", args: {{ url: "{url}", method: POST, headers: {{ content-type: application/json }}, body: "${{{{ with.payload }}}}" }} }}
"#
    )
}

#[tokio::test]
async fn an_open_endpoint_becomes_a_question_and_the_answer_round_binds_the_document() {
    let invented = digest(
        "https://hooks.example.com/digest",
        "\"hooks.example.com\"",
        "",
    );
    let placeholder = digest(
        "${{ const.webhook_endpoint }}",
        "",
        "\n  webhook_endpoint: \"\"",
    );
    let asked = json!([{"key": "const.webhook_endpoint", "label": "The team webhook's HTTPS address",
        "answer_type": "literal", "why": "the request names the webhook without its address"}]);
    let author = Author::new(vec![
        written(&invented),
        answer(&placeholder, &json!([]), &asked),
    ]);
    let request =
        CompileRequest::create(WEBHOOK_INTENT).with_authoring_policy(policy(NativeMode::Escalate));
    let out = create(&request, &author).await;
    door_first(&out, &author);
    // The invented host was refused by the laws and the refusal went back to the author.
    assert_eq!(author.count(), 2, "{:?}", roles(&out));
    let repair = author.calls.lock().unwrap()[1].last.clone();
    assert!(
        repair.contains("hooks.example.com"),
        "the repair names the host: {repair}"
    );
    assert_eq!(roles(&out)[..2], ["document", "document-repair"]);
    // The repaired document keeps every requested construct and asks for the address.
    let keys: Vec<&str> = out.questions.iter().map(|q| q.key.as_str()).collect();
    assert!(keys.contains(&"const.webhook_endpoint"), "{keys:?}");
    assert_ne!(out.status, CompileStatus::Ready);
    let plan = out
        .provenance
        .plan
        .clone()
        .expect("the continuation record");
    assert_eq!(plan["strategy"], "native");
    assert!(
        plan.get("document").is_none(),
        "nothing is bound before the answer"
    );
    let kept = literal_projection(plan["source"].as_str().unwrap()).unwrap();
    assert_eq!(
        kept["inputs"]["team"],
        json!({"type": "string", "required": true})
    );
    assert_eq!(kept["const"]["window_days"], 7);
    assert!(
        !plan["source"]
            .as_str()
            .unwrap()
            .contains("hooks.example.com")
    );
    // The answer round replays the record with no authoring call and binds the final bytes.
    let answered = request
        .clone()
        .answer(
            "const.webhook_endpoint",
            "\"https://hooks.example.org/digest\"",
        )
        .with_plan(plan);
    let done = create(&answered, &author).await;
    assert_eq!(author.count(), 2, "no author call in the answer round");
    assert_eq!(done.status, CompileStatus::Ready, "{:#?}", done.diagnostics);
    let candidate = done.candidate.as_deref().unwrap();
    let doc = checked(candidate);
    assert_eq!(
        doc["const"]["webhook_endpoint"],
        "https://hooks.example.org/digest"
    );
    assert_eq!(doc["permits"]["net"]["http"], json!(["hooks.example.org"]));
    let bound = &done.provenance.plan.as_ref().unwrap()["document"];
    assert_eq!(
        bound["candidate_sha256"],
        json!(sha256(candidate)),
        "{bound:#}"
    );
    assert_eq!(bound["request"], WEBHOOK_INTENT);
    // The answer round never ran the door: its decision is restated on the answered bytes.
    let decided = &decision(&done)["document_create"];
    assert_eq!(
        decided["candidate_sha256"], bound["candidate_sha256"],
        "{decided:#}"
    );
    assert_eq!(decided["mode"], "written");
}

#[tokio::test]
async fn the_policy_names_the_door_and_only_off_and_sketch_keep_their_own() {
    let probe = |native| async move {
        let author = Author::new(vec![written("nika: probe\n")]);
        let request = CompileRequest::create(common::INTENT).with_authoring_policy(policy(native));
        let _ = create(&request, &author).await;
        author.schema(0)
    };
    for native in [NativeMode::Escalate, NativeMode::Only] {
        let schema = probe(native).await;
        assert!(
            schema.iter().any(|k| k == "operations"),
            "{native:?}: {schema:?}"
        );
    }
    let sketch = probe(NativeMode::Sketch).await;
    assert!(sketch.iter().any(|k| k == "tasks"), "{sketch:?}");
    let plan = probe(NativeMode::Off).await;
    assert!(plan.iter().any(|k| k == "steps"), "{plan:?}");
}

#[tokio::test]
async fn a_missing_component_leaves_a_new_construction_with_every_clause() {
    let envelope = "nika: stale-tickets-report\npermits:\n  fs: { read: [\"./in/tickets.json\"], write: [\"./out/report.json\"] }\n  tools: [\"nika:read\", \"nika:jq\", \"nika:write\"]\n";
    let compose = json!({"op": "compose", "component": "block:stale-filter-report",
        "version": "fixture-document-r1", "bindings_json": "{\"const.max_age_hours\": 48}"});
    let author = Author::new(vec![
        answer(envelope, &json!([compose]), &json!([])),
        written(STALE_BUILT),
    ]);
    let request =
        CompileRequest::create(STALE_INTENT).with_authoring_policy(policy(NativeMode::Escalate));
    let out = create(&request, &author).await;
    door_first(&out, &author);
    // No catalogue was lent: the compose is refused by name, and the author builds the work.
    let repair = author.calls.lock().unwrap()[1].last.clone();
    assert!(repair.contains("no component catalogue"), "{repair}");
    assert_eq!(out.status, CompileStatus::Ready, "{:#?}", out.diagnostics);
    let candidate = out.candidate.as_deref().unwrap();
    assert_eq!(candidate, STALE_BUILT);
    let doc = checked(candidate);
    assert_eq!(
        doc["tasks"]["write_report"]["invoke"]["args"]["path"],
        "./out/report.json"
    );
    assert_eq!(doc["outputs"]["stale"], "${{ tasks.stale.output }}");
    // Nothing claims a reuse that did not happen; the refused call stays on the receipt.
    let created = &decision(&out)["document_create"];
    assert_eq!(
        (
            created["mode"].clone(),
            created["reuse"]["expanded"].clone()
        ),
        (json!("written"), json!(0))
    );
    assert_eq!(created["components"], json!([]));
    let receipt = out.provenance.authoring.as_ref().unwrap();
    assert_eq!(
        receipt.context[0]["result"]["input_tokens"], 300,
        "the refused call is paid"
    );
}

const WEEKLY_INTENT: &str =
    "Every Monday morning, read ./in/tickets.json and log how many records it holds.";

/// A weekly count: the schedule stays beside the file, never in it.
const WEEKLY: &str = r#"nika: weekly-ticket-count
permits:
  fs: { read: ["./in/tickets.json"] }
  tools: ["nika:read", "nika:jq", "nika:log"]
tasks:
  read_tickets:
    invoke: { tool: "nika:read", args: { path: "./in/tickets.json" } }
  count:
    with: { raw: "${{ tasks.read_tickets.output }}" }
    invoke: { tool: "nika:jq", args: { input: "${{ with.raw }}", expression: "fromjson | length" } }
  announce:
    with: { total: "${{ tasks.count.output }}" }
    invoke: { tool: "nika:log", args: { level: info, message: "tickets: ${{ with.total }}" } }
"#;

/// A schedule the request states rides beside the candidate: its timezone, missed-run and
/// overlap bindings are optional questions that never block READY. The document is bound to its
/// final bytes all the same, so a Save keeps its identity while a binding is still open; the
/// native record stays the continuation an answer to one of them replays, with no author call.
#[tokio::test]
async fn a_ready_document_with_only_optional_schedule_questions_is_bound_and_still_answerable() {
    let author = Author::new(vec![written(WEEKLY)]);
    let request =
        CompileRequest::create(WEEKLY_INTENT).with_authoring_policy(policy(NativeMode::Escalate));
    let out = create(&request, &author).await;
    door_first(&out, &author);
    assert_eq!(out.status, CompileStatus::Ready, "{:#?}", out.diagnostics);
    let trigger = out
        .requested_trigger
        .as_ref()
        .expect("the schedule is recorded");
    assert_eq!(trigger.kind, nika_compile::TriggerKind::Schedule);
    assert!(
        !out.questions.is_empty(),
        "the schedule's bindings are asked"
    );
    assert!(
        out.questions.iter().all(|q| !q.mandatory),
        "{:#?}",
        out.questions
    );
    let candidate = out.candidate.clone().expect("a candidate");
    assert_eq!(candidate, WEEKLY);
    let plan = out.provenance.plan.clone().expect("a plan");
    assert_eq!(plan["strategy"], "native", "the continuation is kept");
    assert_eq!(
        plan["document"]["candidate_sha256"],
        json!(sha256(&candidate))
    );
    // An optional answer later: the same creation's record replays, nothing is re-authored.
    let answered = request
        .clone()
        .answer("trigger.timezone", "\"Europe/Paris\"")
        .with_plan(plan);
    let again = create(&answered, &author).await;
    assert_eq!(author.count(), 1, "no author call in the answer round");
    assert_eq!(
        again.status,
        CompileStatus::Ready,
        "{:#?}",
        again.diagnostics
    );
    let zone = again
        .requested_trigger
        .as_ref()
        .and_then(|t| t.timezone.clone());
    assert_eq!(zone.as_deref(), Some("Europe/Paris"));
    assert_eq!(
        again.candidate.as_deref(),
        Some(WEEKLY),
        "the bytes are unchanged"
    );
    let bound = &again.provenance.plan.as_ref().expect("a plan")["document"];
    assert_eq!(
        bound["candidate_sha256"],
        json!(sha256(WEEKLY)),
        "{bound:#}"
    );
    assert_eq!(
        decision(&again)["document_create"]["candidate_sha256"],
        json!(sha256(WEEKLY))
    );
}

/// The record as every transport carries it: the machine document (`outcome_document`, what the
/// CLI's JSON output and Serve emit). READY holds `plan.document` beside the door's two
/// sections, every digest naming the final bytes; a mandatory-question continuation holds both
/// sections, no candidate and no `plan.document`. A SCRIPTED author through the in-process
/// compile entry, never a launched door or a model.
#[tokio::test]
async fn the_machine_document_carries_the_settled_record_and_a_continuation_none() {
    let document = nika_pack::example("08-config-values").expect("the pack ships 08");
    let author = Author::new(vec![written(document)]);
    let request =
        CompileRequest::create(CONFIG_INTENT).with_authoring_policy(policy(NativeMode::Escalate));
    let ready = nika_compile::outcome_document(&create(&request, &author).await);
    assert_eq!(ready["status"], "ready", "{ready:#}");
    let candidate = ready["candidate"].as_str().expect("the candidate");
    let bound = &ready["provenance"]["plan"]["document"];
    let expected = json!({"version": 1, "candidate_sha256": sha256(candidate),
        "request": CONFIG_INTENT, "base_sha256": null, "mode": "written", "components": []});
    assert_eq!(*bound, expected, "exactly the agreed members");
    let section = &ready["provenance"]["plan"]["document_create"];
    assert_eq!(
        (section["route"].as_str(), section["resolved"].as_str()),
        (Some("native: document"), Some(CONFIG_INTENT))
    );
    let decided = &ready["provenance"]["decision"]["document_create"];
    assert_eq!(
        decided["candidate_sha256"], bound["candidate_sha256"],
        "one final digest"
    );
    assert_eq!(
        (
            decided["operations"].as_u64(),
            decided["base_sha256"].is_null()
        ),
        (Some(0), true)
    );
    assert_eq!(decided["reuse"]["expanded"], 0);
    // A webhook whose address the request leaves open: the continuation, nothing settled.
    let placeholder = digest(
        "${{ const.webhook_endpoint }}",
        "",
        "\n  webhook_endpoint: \"\"",
    );
    let asked = json!([{"key": "const.webhook_endpoint", "label": "The team webhook's HTTPS address",
        "answer_type": "literal", "why": "the request names the webhook without its address"}]);
    let author = Author::new(vec![answer(&placeholder, &json!([]), &asked)]);
    let request =
        CompileRequest::create(WEBHOOK_INTENT).with_authoring_policy(policy(NativeMode::Escalate));
    let held = nika_compile::outcome_document(&create(&request, &author).await);
    assert_eq!(
        (held["status"].as_str(), held["candidate"].is_null()),
        (Some("incomplete"), true),
        "{held:#}"
    );
    let asked: Vec<(&str, bool)> = (held["questions"].as_array().into_iter().flatten())
        .map(|q| {
            (
                q["key"].as_str().unwrap_or_default(),
                q["mandatory"] == true,
            )
        })
        .collect();
    assert!(
        asked.contains(&("const.webhook_endpoint", true)),
        "{asked:?}"
    );
    let plan = &held["provenance"]["plan"];
    assert!(plan.get("document").is_none(), "{plan:#}");
    assert_eq!(
        (
            plan["strategy"].as_str(),
            plan["document_create"]["resolved"].as_str()
        ),
        (Some("native"), Some(WEBHOOK_INTENT))
    );
    let decided = &held["provenance"]["decision"]["document_create"];
    assert!(decided["candidate_sha256"].is_null(), "{decided:#}");
}

// ── A document the judge finds lacking: the unoffered preview, its attempts and its repairs ───

/// The option each verifier question offers when it approves: the whole request, a clause or
/// part, an observed run, the extra-operation question and the task question.
const APPROVALS: [&str; 5] = [
    "faithful",
    "carried",
    "consistent",
    "only_requested",
    "no_task",
];

/// A judge double over the scripted author: a candidate whose bytes carry `marker` is refused
/// and its defect located (the whole request `unfaithful`, each part `missing`, a task question
/// with the first task offered, else `omitted`); any other candidate is approved. A scripted
/// verdict proves the door's exits, never a model's judgment.
struct Localizing<'a> {
    author: &'a Author,
    marker: &'static str,
}

impl ProviderInferDyn for Localizing<'_> {
    async fn infer(&self, request: InferRequest) -> Result<InferResponse, ProviderError> {
        let ResponseFormat::JsonSchema(schema) = &request.response_format else {
            return self.author.infer(request).await;
        };
        let keys = (schema["properties"]["choice"]["enum"].as_array())
            .cloned()
            .unwrap_or_default();
        let Some(approval) = APPROVALS.iter().find(|a| keys.iter().any(|k| k == *a)) else {
            return self.author.infer(request).await;
        };
        let shown: String = (request.messages.iter())
            .map(|m| text(&m.content))
            .collect();
        let offered = |key: &str| keys.iter().find(|k| *k == key);
        let task = |k: &&Value| k.as_str().is_some_and(|k| k.starts_with("task-"));
        let refusal = (offered("unfaithful").or_else(|| offered("missing")))
            .or_else(|| keys.iter().find(task))
            .or_else(|| offered("omitted"))
            .and_then(Value::as_str);
        let key = match refusal {
            Some(refused) if shown.contains(self.marker) => refused,
            _ => approval,
        };
        Ok(InferResponse::new(
            vec![ContentBlock::Text {
                text: json!({"choice": key}).to_string(),
            }],
            TokenUsage::new(1, 1),
            StopReason::EndTurn,
        ))
    }
}

/// The rich-control document with its first comment marked: the same workflow, other bytes.
fn drafted(mark: &str) -> String {
    let marked = format!("# {mark}: header comment kept byte for byte");
    RICH.replacen("# header comment kept byte for byte", &marked, 1)
}

/// The rich-control request under `escalate` with `repairs`, judged by [`Localizing`].
async fn judged(author: &Author, repairs: u32) -> CompileOutcome {
    let judge = Localizing {
        author,
        marker: "draft-",
    };
    let cognition = Cognition {
        provider: Some(&judge),
        seat: None,
    };
    let policy = policy(NativeMode::Escalate).with_repairs(repairs);
    let request = CompileRequest::create(RICH_INTENT).with_authoring_policy(policy);
    compile_with_cognition_composed(&request, cognition, None, None)
        .await
        .unwrap()
}

fn route(out: &CompileOutcome) -> Vec<String> {
    (decision(out)["route"].as_array().into_iter().flatten())
        .filter_map(|step| step.as_str().map(str::to_owned))
        .collect()
}

fn verdicts(out: &CompileOutcome) -> Vec<Value> {
    (decision(out)["semantic_verification"].as_array())
        .cloned()
        .unwrap_or_default()
}

/// The judge's draft as COLD kept it: INCOMPLETE, shown with its Check preview, never offered,
/// its replayable record, questions and boundary dropped, and the held finding said.
fn assert_kept_preview(out: &CompileOutcome, shown: &str) {
    assert_eq!(
        out.status,
        CompileStatus::Incomplete,
        "{:#?}",
        out.diagnostics
    );
    assert_eq!(
        out.candidate.as_deref(),
        Some(shown),
        "the draft stays shown"
    );
    assert!(out.check_preview.is_some(), "its Check preview stays");
    assert!(out.provenance.plan.is_none(), "no replay of doubted bytes");
    assert!(out.questions.is_empty() && out.requested_boundary.is_none());
    assert!(
        (out.diagnostics.iter()).any(|d| d.target == "verify_held"),
        "{:#?}",
        out.diagnostics
    );
}

#[tokio::test]
async fn a_document_the_judge_finds_lacking_past_its_rounds_stays_the_unoffered_preview() {
    let first = drafted("draft-one");
    let author = Author::new(vec![written(&first)]);
    let out = judged(&author, 0).await;
    assert_eq!(author.count(), 1);
    assert_kept_preview(&out, &first);
    let route = route(&out);
    for step in ["verify: not ready", "verify: doubted, not replayable"] {
        assert!(route.iter().any(|s| s == step), "{step}: {route:?}");
    }
    let verdicts = verdicts(&out);
    assert_eq!(verdicts.len(), 1, "{verdicts:#?}");
    assert!(
        (verdicts[0]["defects"].as_array()).is_some_and(|d| !d.is_empty()),
        "a located defect: {verdicts:#?}"
    );
}

#[tokio::test]
async fn the_same_document_again_is_the_second_attempt_and_no_progress() {
    let first = drafted("draft-one");
    let author = Author::new(vec![written(&first), written(&first)]);
    let out = judged(&author, 2).await;
    assert_eq!(author.count(), 2);
    let verdicts = verdicts(&out);
    assert_eq!(verdicts.len(), 2, "{verdicts:#?}");
    let attempts = (
        verdicts[0]["attempt"].clone(),
        verdicts[1]["attempt"].clone(),
    );
    assert_eq!(attempts, (json!(0), json!(1)), "{verdicts:#?}");
    assert_eq!(verdicts[1]["same_bytes_as"], 0, "{verdicts:#?}");
    let route = route(&out);
    for step in ["verify: repair 1", "native: no progress"] {
        assert!(route.iter().any(|s| s == step), "{step}: {route:?}");
    }
    assert_kept_preview(&out, &first);
}

#[tokio::test]
async fn a_repair_from_the_judge_is_named_and_the_repaired_document_is_ready() {
    let author = Author::new(vec![written(&drafted("draft-one")), written(RICH)]);
    let out = judged(&author, 1).await;
    assert_eq!(out.status, CompileStatus::Ready, "{:#?}", out.diagnostics);
    assert_eq!(out.candidate.as_deref(), Some(RICH));
    assert_eq!(roles(&out)[..1], ["document"], "{:?}", roles(&out));
    assert!(roles(&out).iter().any(|r| r == "document-repair"));
    let route = route(&out);
    assert!(route.iter().any(|s| s == "verify: repair 1"), "{route:?}");
    let verdicts = verdicts(&out);
    assert_eq!(verdicts.len(), 2, "{verdicts:#?}");
    assert_eq!(verdicts[1]["attempt"], 1, "{verdicts:#?}");
}

#[tokio::test]
async fn the_same_defects_in_new_bytes_end_the_door_with_the_preview_kept() {
    let second = drafted("draft-two");
    let author = Author::new(vec![written(&drafted("draft-one")), written(&second)]);
    let out = judged(&author, 3).await;
    assert_eq!(author.count(), 2, "no reopening after the same defects");
    assert_kept_preview(&out, &second);
    let route = route(&out);
    assert!(
        route.iter().any(|s| s == "native: no progress"),
        "{route:?}"
    );
}

// ── An UNJUDGED document: withdrawn, its record kept for a later round (pinned follow-up) ─────

/// A judge double whose every verifier call fails; authoring calls go to the scripted author.
struct Unreachable<'a> {
    author: &'a Author,
}

impl ProviderInferDyn for Unreachable<'_> {
    async fn infer(&self, request: InferRequest) -> Result<InferResponse, ProviderError> {
        let ResponseFormat::JsonSchema(schema) = &request.response_format else {
            return self.author.infer(request).await;
        };
        let keys = (schema["properties"]["choice"]["enum"].as_array())
            .cloned()
            .unwrap_or_default();
        if APPROVALS.iter().any(|a| keys.iter().any(|k| k == *a)) {
            return Err(ProviderError::Other {
                reason: "the judge is unreachable".to_owned(),
            });
        }
        self.author.infer(request).await
    }
}

/// Pinned on purpose, a follow-up and not a decision: a document whose judge never answered is
/// withdrawn, its replayable record kept with `verify_resume`, the signal a host resumes on, so
/// a later round asks the judge again on the same bytes. COLD showed such a draft as the
/// preview; whether the document door keeps it visible too is an open product question, and
/// this witness turns red when that contract changes.
#[tokio::test]
async fn an_unjudged_document_is_withdrawn_with_its_record_kept_for_a_later_round() {
    let author = Author::new(vec![written(RICH)]);
    let judge = Unreachable { author: &author };
    let cognition = Cognition {
        provider: Some(&judge),
        seat: None,
    };
    let request =
        CompileRequest::create(RICH_INTENT).with_authoring_policy(policy(NativeMode::Escalate));
    let out = compile_with_cognition_composed(&request, cognition, None, None)
        .await
        .unwrap();
    assert_eq!(author.count(), 1);
    assert_eq!(
        out.status,
        CompileStatus::Incomplete,
        "{:#?}",
        out.diagnostics
    );
    assert!(out.candidate.is_none(), "withdrawn: the open follow-up");
    let record = out.provenance.plan.as_ref().expect("the record kept");
    assert_eq!(record["source"], RICH, "the bytes a later round replays");
    assert!(
        (out.diagnostics.iter()).any(|d| d.target == "verify_resume"),
        "{:#?}",
        out.diagnostics
    );
    let route = route(&out);
    assert!(
        route.iter().any(|s| s == "verify: unjudged, record kept"),
        "{route:?}"
    );
}
