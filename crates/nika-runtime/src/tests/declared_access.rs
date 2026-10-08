// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The authored `run.access` + `run.reasoning.effort` from the FILE to the
//! wire: parser → the declared plan (no flag) → dispatch → the agent verb
//! → the real ACP client driver against a scripted codex-acp peer that
//! asserts every configuration payload. The terminal frames carry the
//! authored requirement and the call's receipt; the API provider beside
//! the seat is never dialed; an unoffered effort refuses with ZERO
//! prompts; an `infer:` over the codex ACP route is admitted at the gate
//! (its completion profile is attested); and a changed effort changes the
//! resume identity.

use std::future::Future;
use std::pin::Pin;
use std::sync::{Arc, Mutex};

use nika_kernel::ai::harness::{DynAgentBackend, HarnessError, HarnessEventStream, HarnessRequest};
use nika_kernel_mock::{
    MockClock, MockProvider, MockShell, MockToolDefinitionProvider, MockToolExecutor,
};
use nika_providers::probe::{ExecutionLocus, ProviderProbe, ProviderReadiness};
use nika_providers::{ExecutionAccessPlan, ModelNeed, VerbNeeds};
use nika_types::access::AccessClass;
use nika_verb_agent::AgentVerb;
use nika_verb_exec::ExecVerb;
use nika_verb_infer::InferVerb;
use nika_verb_invoke::InvokeVerb;
use serde_json::{Value, json};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};

use super::*;

const MODEL: &str = "openai/gpt-6-astra";

fn workflow(effort: &str, verb: &str) -> RawWorkflow {
    let body = match verb {
        "agent" => "    agent:\n      prompt: \"summarise the invariants\"\n",
        _ => "    infer:\n      prompt: \"summarise the invariants\"\n",
    };
    let yaml = format!(
        "nika: declared-access\nmodel: {MODEL}\npermits: {{}}\nrun:\n  access:\n    via: codex\n    \
         protocol: acp\n    fallback: none\n  reasoning:\n    effort: {effort}\ntasks:\n  ask:\n{body}"
    );
    nika_schema::parse(
        &yaml,
        nika_schema::FileId::new(0),
        nika_schema::ParseMode::Strict,
    )
    .expect("the ratified shape parses")
}

/// This machine: an `openai` key READY and a signed-in codex seat — the key
/// must never be substituted for the file's route.
fn probes() -> Vec<ProviderProbe> {
    let api = ProviderProbe::new(
        "openai",
        true,
        true,
        "OPENAI_API_KEY",
        false,
        ProviderReadiness::new(
            true,
            true,
            None,
            None,
            true,
            ExecutionLocus::Cloud,
            AccessClass::Api,
        ),
        "https://api.openai.com",
    );
    let codex = ProviderProbe::new(
        "codex",
        false,
        true,
        "",
        false,
        ProviderReadiness::new(
            true,
            true,
            None,
            None,
            false,
            ExecutionLocus::Loopback,
            AccessClass::Harness,
        ),
        "",
    )
    .with_serves(vec!["openai".to_owned()]);
    vec![api, codex]
}

/// The plan the CLI door resolves for this file with NO `--access` flag.
fn plan(wf: &RawWorkflow, infer: bool) -> ExecutionAccessPlan {
    let requirement = wf
        .run
        .as_ref()
        .and_then(|run| run.value.access_requirement())
        .expect("declared");
    let needs = [ModelNeed::new(MODEL, infer, !infer)];
    nika_providers::resolve_execution_plan_declared(
        &needs,
        &probes(),
        None,
        VerbNeeds::new(infer, !infer),
        Some(&requirement),
    )
}

type Peer = (
    BufReader<tokio::io::ReadHalf<tokio::io::DuplexStream>>,
    tokio::io::WriteHalf<tokio::io::DuplexStream>,
);

async fn read(peer: &mut Peer) -> Option<Value> {
    let mut line = String::new();
    let n = tokio::time::timeout(
        std::time::Duration::from_secs(5),
        peer.0.read_line(&mut line),
    )
    .await
    .expect("dialogue settles")
    .expect("read");
    (n != 0).then(|| serde_json::from_str(&line).expect("json"))
}

async fn write(peer: &mut Peer, message: &Value) {
    let mut bytes = serde_json::to_vec(message).expect("json");
    bytes.push(b'\n');
    peer.1.write_all(&bytes).await.expect("write");
}

async fn reply(peer: &mut Peer, request: &Value, result: Value) {
    write(
        peer,
        &json!({"jsonrpc":"2.0","id":request["id"],"result":result}),
    )
    .await;
}

fn config(model: &str, effort: &str, efforts: &[&str]) -> Value {
    let select = |id: &str, category: &str, current: &str, values: &[&str]| {
        json!({"id":id,"name":id,"category":category,"type":"select","currentValue":current,
            "options":values.iter().map(|v| json!({"value":v,"name":v})).collect::<Vec<_>>()})
    };
    json!([
        select("model", "model", model, &["gpt-5.5", "gpt-6-astra"]),
        select("reasoning_effort", "thought_level", effort, efforts)
    ])
}

/// A scripted codex-acp: the default model offers low/medium; selecting
/// `gpt-6-astra` refreshes the effort set to `refreshed`. Every request
/// the client sends is recorded (method + params).
async fn codex_peer(
    mut peer: Peer,
    refreshed: &'static [&'static str],
    seen: Arc<Mutex<Vec<Value>>>,
) {
    let mut next = read(&mut peer).await;
    while let Some(request) = next {
        seen.lock()
            .expect("seen")
            .push(json!({"method":request["method"],"params":request["params"]}));
        match request["method"].as_str() {
            Some("initialize") => reply(&mut peer, &request, json!({"protocolVersion":1})).await,
            Some("session/new") => {
                let result = json!({"sessionId":"s-run","configOptions":config("gpt-5.5", "medium", &["low", "medium"])});
                reply(&mut peer, &request, result).await;
            }
            Some("session/set_config_option") if request["params"]["configId"] == "model" => {
                let model = request["params"]["value"].as_str().expect("value");
                let options = json!({"configOptions":config(model, "medium", refreshed)});
                reply(&mut peer, &request, options).await;
            }
            Some("session/set_config_option") => {
                let effort = request["params"]["value"].as_str().expect("value");
                let options = json!({"configOptions":config("gpt-6-astra", effort, refreshed)});
                reply(&mut peer, &request, options).await;
            }
            Some("session/prompt") => {
                write(
                    &mut peer,
                    &json!({"jsonrpc":"2.0","method":"session/update","params":{
                    "sessionId":"s-run","update":{"sessionUpdate":"agent_message_chunk",
                    "content":{"type":"text","text":"three invariants"}}}}),
                )
                .await;
                reply(&mut peer, &request, json!({"stopReason":"end_turn"})).await;
            }
            other => panic!("unexpected client request {other:?}"),
        }
        next = read(&mut peer).await;
    }
}

/// The seat backend: each delegated run drives the REAL client over a
/// duplex against the scripted peer.
struct ScriptedCodex {
    refreshed: &'static [&'static str],
    seen: Arc<Mutex<Vec<Value>>>,
}

impl DynAgentBackend for ScriptedCodex {
    fn run_agent_boxed(
        &self,
        request: HarnessRequest,
    ) -> Pin<Box<dyn Future<Output = Result<HarnessEventStream, HarnessError>> + Send + '_>> {
        let (ours, theirs) = tokio::io::duplex(64 * 1024);
        let (client_read, client_write) = tokio::io::split(ours);
        let (peer_read, peer_write) = tokio::io::split(theirs);
        tokio::spawn(codex_peer(
            (BufReader::new(peer_read), peer_write),
            self.refreshed,
            Arc::clone(&self.seen),
        ));
        Box::pin(async move { Ok(nika_harness::drive(client_read, client_write, request)) })
    }
}

type SeatRuntime = Runtime<
    MockShell,
    MockToolExecutor,
    nika_providers::NoHttp,
    MockProvider,
    MockToolDefinitionProvider,
    MockClock,
>;

fn runtime(api: Arc<MockProvider>) -> SeatRuntime {
    let invoke = Arc::new(InvokeVerb::new(Arc::new(MockToolExecutor::new())));
    Runtime::new(
        ExecVerb::new(Arc::new(MockShell::new())),
        Arc::clone(&invoke),
        InferVerb::new(
            Arc::new(nika_providers::ProviderRegistry::without_http(
                nika_providers::ProvidersConfig::new(),
            )),
            MODEL,
        ),
        AgentVerb::new(
            api,
            invoke,
            Arc::new(MockToolDefinitionProvider::new()),
            MODEL,
        ),
        MockClock::new(),
        RuntimeConfig::default(),
    )
}

fn text<'a>(event: &'a Event, key: &str) -> Option<&'a str> {
    event
        .fields
        .iter()
        .find(|kv| kv.key == key)
        .and_then(|kv| match &kv.value {
            FieldValue::String(value) => Some(value.as_str()),
            _ => None,
        })
}

async fn run(
    effort: &str,
    refreshed: &'static [&'static str],
) -> (RunOutcome, Vec<Event>, Vec<Value>, usize) {
    let wf = workflow(effort, "agent");
    let report = nika_check::check(&wf);
    assert!(report.is_clean(), "{report:?}");
    let plan = plan(&wf, false);
    assert!(plan.is_admitted(), "{:?}", plan.pin_refusal);
    assert_eq!(
        plan.seat.as_deref(),
        Some("codex"),
        "the file alone seats codex"
    );
    let api = Arc::new(MockProvider::new("mock").enqueue_text("never asked"));
    let seen = Arc::new(Mutex::new(Vec::new()));
    let backend = Arc::new(ScriptedCodex {
        refreshed,
        seen: Arc::clone(&seen),
    });
    let runtime = runtime(Arc::clone(&api))
        .with_access_plan(plan)
        .with_harness_backend(backend, "codex".into())
        .expect("seated");
    let mut sink = VecSink::new();
    let outcome = runtime
        .run(&wf, &report, &mut DeterministicStamper::new(), &mut sink)
        .await
        .expect("the run settles");
    let seen = seen.lock().expect("seen").clone();
    (
        outcome,
        sink.into_events(),
        seen,
        api.captured_requests().len(),
    )
}

#[tokio::test]
async fn the_file_route_model_and_effort_reach_the_session_before_the_prompt() {
    let (outcome, events, seen, api_calls) = run("xhigh", &["medium", "high", "xhigh"]).await;
    assert!(outcome.ok, "{events:?}");
    let methods: Vec<&str> = seen.iter().filter_map(|r| r["method"].as_str()).collect();
    assert_eq!(
        methods,
        [
            "initialize",
            "session/new",
            "session/set_config_option",
            "session/set_config_option",
            "session/prompt"
        ],
        "configure, read back, then exactly one prompt"
    );
    assert_eq!(
        seen[2]["params"],
        json!({"sessionId":"s-run","configId":"model","value":"gpt-6-astra"})
    );
    assert_eq!(
        seen[3]["params"],
        json!({"sessionId":"s-run","configId":"reasoning_effort","value":"xhigh"})
    );
    assert_eq!(
        api_calls, 0,
        "the ready API key beside the seat is never dialed"
    );
    let done = events
        .iter()
        .find(|e| e.kind == EventKind::TaskCompleted)
        .expect("completed");
    assert_eq!(text(done, "access"), Some("harness"));
    assert_eq!(text(done, "access_id"), Some("codex"));
    let requirement: Value =
        serde_json::from_str(text(done, "access_requirement").expect("requirement")).expect("json");
    assert_eq!(
        requirement,
        json!({"via":"codex","protocol":"acp","fallback":"none","effort":"xhigh"})
    );
    let selection: Value =
        serde_json::from_str(text(done, "access_selection").expect("receipt")).expect("json");
    assert_eq!(
        selection,
        json!({
            "schema": "nika/access-selection@1",
            "protocol": "acp",
            "model": {"requested": MODEL, "option": "model", "transmitted": "gpt-6-astra",
                "configured": "gpt-6-astra", "configured_source": "confirmed_selection"},
            "effort": {"requested": "xhigh", "option": "reasoning_effort", "transmitted": "xhigh",
                "configured": "xhigh", "configured_source": "confirmed_selection"},
            "responder": {"model": null, "evidence": "unknown"}
        })
    );
    let opening = events.first().expect("an opening frame");
    let boot_requirement = events
        .iter()
        .find_map(|e| text(e, "access_requirement").filter(|_| e.kind != EventKind::TaskCompleted));
    assert!(
        boot_requirement.is_some(),
        "the prologue records the authored selection: {opening:?}"
    );
}

#[tokio::test]
async fn an_effort_the_selected_model_does_not_offer_refuses_with_zero_prompts() {
    let (outcome, events, seen, api_calls) = run("xhigh", &["low", "medium", "high"]).await;
    assert!(!outcome.ok);
    assert!(
        !seen.iter().any(|r| r["method"] == "session/prompt"),
        "zero prompts: {seen:?}"
    );
    assert_eq!(api_calls, 0, "no substitute route");
    let failed = events
        .iter()
        .find(|e| e.kind == EventKind::TaskFailed)
        .expect("task_failed");
    let detail = text(failed, "detail").expect("detail");
    assert!(
        detail.contains("NIKA-1805")
            && detail.contains("`xhigh`")
            && detail.contains("low · medium · high"),
        "{detail}"
    );
}

/// The infer role over the codex ACP route is admitted at the gate: its
/// completion profile is attested (the turn itself is proven by the
/// harness's scripted Codex peers and the live qualification).
#[test]
fn an_infer_task_over_the_codex_acp_route_is_admitted_at_the_gate() {
    let wf = workflow("high", "infer");
    let plan = plan(&wf, true);
    assert!(plan.is_admitted(), "{:?}", plan.pin_refusal);
    assert_eq!(plan.seat.as_deref(), Some("codex"));
}

#[test]
fn a_changed_effort_changes_the_resume_identity() {
    let stamp_of = |effort: &str| {
        let wf = workflow(effort, "agent");
        let runtime =
            runtime(Arc::new(MockProvider::new("mock"))).with_access_plan(plan(&wf, false));
        let ctx = runtime.resume_context(&wf, &BTreeMap::new());
        let task = &wf.tasks[0].value;
        resume::stamp(
            task,
            &BTreeMap::new(),
            &BTreeMap::new(),
            &BTreeMap::new(),
            &ctx,
        )
        .expect("eligible")
        .def_hash
    };
    assert_eq!(stamp_of("high"), stamp_of("high"), "stable");
    assert_ne!(
        stamp_of("high"),
        stamp_of("low"),
        "a changed effort never reuses the old output"
    );
}

/// A seat whose adapter is gone at the call.
struct GoneSeat;

impl DynAgentBackend for GoneSeat {
    fn run_agent_boxed(
        &self,
        _request: HarnessRequest,
    ) -> Pin<Box<dyn Future<Output = Result<HarnessEventStream, HarnessError>> + Send + '_>> {
        Box::pin(async {
            Err(HarnessError::Unavailable {
                reason: "codex-acp exited before initialize".into(),
            })
        })
    }
}

/// `fallback: none` survives a run-time liveness failure: the task fails
/// with the seat's typed code, and the READY API key is never dialed.
#[tokio::test]
async fn a_seat_lost_at_the_call_fails_typed_and_never_falls_back() {
    let wf = workflow("high", "agent");
    let report = nika_check::check(&wf);
    let api = Arc::new(MockProvider::new("mock").enqueue_text("never asked"));
    let runtime = runtime(Arc::clone(&api))
        .with_access_plan(plan(&wf, false))
        .with_harness_backend(Arc::new(GoneSeat), "codex".into())
        .expect("seated");
    let mut sink = VecSink::new();
    let outcome = runtime
        .run(&wf, &report, &mut DeterministicStamper::new(), &mut sink)
        .await
        .expect("the run settles");
    assert!(!outcome.ok);
    let events = sink.into_events();
    let failed = events
        .iter()
        .find(|e| e.kind == EventKind::TaskFailed)
        .expect("task_failed");
    let detail = text(failed, "detail").expect("detail");
    assert!(detail.starts_with("NIKA-1803"), "{detail}");
    assert_eq!(text(failed, "access_id"), Some("codex"));
    assert!(api.captured_requests().is_empty(), "no substitute route");
}

/// An explicit `--access` that contradicts the file refuses at the gate:
/// zero events, zero spend, the file field named.
#[tokio::test]
async fn a_contradicting_flag_refuses_at_the_gate() {
    let wf = workflow("high", "agent");
    let report = nika_check::check(&wf);
    let requirement = wf.run.as_ref().and_then(|r| r.value.access_requirement());
    let plan = nika_providers::resolve_execution_plan_declared(
        &[ModelNeed::new(MODEL, false, true)],
        &probes(),
        Some("openai"),
        VerbNeeds::new(false, true),
        requirement.as_ref(),
    );
    let api = Arc::new(MockProvider::new("mock").enqueue_text("never asked"));
    let runtime = runtime(Arc::clone(&api)).with_access_plan(plan);
    let mut sink = VecSink::new();
    let err = runtime
        .run(&wf, &report, &mut DeterministicStamper::new(), &mut sink)
        .await
        .expect_err("refused before the prologue");
    assert!(
        matches!(err, RuntimeError::AccessPinUnsatisfied { .. }),
        "{err:?}"
    );
    assert!(err.to_string().contains("run.access.via: codex"), "{err}");
    assert!(sink.events().is_empty());
    assert!(api.captured_requests().is_empty());
}

/// A model RENDERED from an input has no static lane, yet it rides the
/// declared seat with its whole receipt: the exact route id, the authored
/// requirement and the call's read-back with the responder unknown.
#[tokio::test]
async fn a_rendered_model_on_the_declared_seat_keeps_its_receipt() {
    let yaml = "nika: declared-access\ninputs:\n  m: { type: string, required: true }\npermits: {}\n\
                run:\n  access: { via: codex, protocol: acp, fallback: none }\n  reasoning: { \
                effort: xhigh }\ntasks:\n  ask:\n    agent:\n      prompt: \"summarise the \
                invariants\"\n      model: \"${{ inputs.m }}\"\n";
    let wf = nika_schema::parse(
        yaml,
        nika_schema::FileId::new(0),
        nika_schema::ParseMode::Strict,
    )
    .expect("parses");
    let report = nika_check::check(&wf);
    let requirement = wf.run.as_ref().and_then(|r| r.value.access_requirement());
    let plan = nika_providers::resolve_execution_plan_declared(
        &[],
        &probes(),
        None,
        VerbNeeds::new(false, true),
        requirement.as_ref(),
    );
    assert!(plan.is_admitted() && plan.lanes.is_empty(), "{plan:?}");
    let api = Arc::new(MockProvider::new("mock").enqueue_text("never asked"));
    let seen = Arc::new(Mutex::new(Vec::new()));
    let backend = Arc::new(ScriptedCodex {
        refreshed: &["medium", "high", "xhigh"],
        seen: Arc::clone(&seen),
    });
    let runtime = runtime(Arc::clone(&api))
        .with_access_plan(plan)
        .with_harness_backend(backend, "codex".into())
        .expect("seated")
        .with_var_overrides([("m".to_owned(), Value::from(MODEL))].into_iter().collect());
    let mut sink = VecSink::new();
    let outcome = runtime
        .run(&wf, &report, &mut DeterministicStamper::new(), &mut sink)
        .await
        .expect("the run settles");
    let events = sink.into_events();
    assert!(outcome.ok, "{events:?}");
    assert!(
        api.captured_requests().is_empty(),
        "the API key is never dialed"
    );
    let prompts = seen
        .lock()
        .expect("seen")
        .iter()
        .filter(|r| r["method"] == "session/prompt")
        .count();
    assert_eq!(prompts, 1);
    let done = events
        .iter()
        .find(|e| e.kind == EventKind::TaskCompleted)
        .expect("completed");
    assert_eq!(
        (text(done, "access"), text(done, "access_id")),
        (Some("harness"), Some("codex"))
    );
    let requirement: Value =
        serde_json::from_str(text(done, "access_requirement").expect("requirement")).expect("json");
    assert_eq!(
        requirement,
        json!({"via":"codex","protocol":"acp","fallback":"none","effort":"xhigh"})
    );
    let selection: Value =
        serde_json::from_str(text(done, "access_selection").expect("receipt")).expect("json");
    assert_eq!(selection["effort"]["configured"], "xhigh");
    assert_eq!(
        selection["effort"]["configured_source"],
        "confirmed_selection"
    );
    assert_eq!(selection["model"]["transmitted"], "gpt-6-astra");
    assert_eq!(
        selection["responder"],
        json!({"model": null, "evidence": "unknown"})
    );
}
