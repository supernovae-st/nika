// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! ONE declaration, both roles: `via: claude-code, protocol: acp` with a
//! native effort serves an `infer:` AND an `agent:` task over ACP, each
//! against its own scripted claude-agent-acp. The `infer:` session opens
//! under the audited completion profile (its options sent with
//! `session/new`), the `agent:` session under the agent's own; both apply
//! the model and the effort and read them back before exactly one prompt;
//! the ready API key is never dialed; each terminal carries the authored
//! requirement and its own call's receipt. A `schema:` infer refuses with
//! ZERO sessions: the profile's output level is text.

use std::future::Future;
use std::pin::Pin;
use std::sync::atomic::{AtomicUsize, Ordering};
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

const MODEL: &str = "anthropic/claude-opus-5-5";

fn workflow(infer_body: &str) -> RawWorkflow {
    let yaml = format!(
        "nika: declared-acp-infer\nmodel: {MODEL}\npermits: {{}}\nrun:\n  access:\n    via: \
         claude-code\n    protocol: acp\n    fallback: none\n  reasoning:\n    effort: max\n\
         tasks:\n  draft:\n    infer:\n{infer_body}  review:\n    with: {{ draft: \"${{{{ \
         tasks.draft.output }}}}\" }}\n    agent:\n      prompt: \"tighten ${{{{ with.draft \
         }}}}\"\n"
    );
    nika_schema::parse(
        &yaml,
        nika_schema::FileId::new(0),
        nika_schema::ParseMode::Strict,
    )
    .expect("the ratified shape parses")
}

const PLAIN: &str = "      prompt: \"summarise the invariants\"\n";
const SCHEMA: &str = "      prompt: \"summarise the invariants\"\n      schema: { type: object, \
                      properties: { a: { type: string } } }\n";

/// This machine: an `anthropic` key READY beside a signed-in claude-code
/// seat — the key must never be substituted for the file's route.
fn probes() -> Vec<ProviderProbe> {
    let api = ProviderProbe::new(
        "anthropic",
        true,
        true,
        "ANTHROPIC_API_KEY",
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
        "https://api.anthropic.com",
    );
    let seat = ProviderProbe::new(
        "claude-code",
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
    .with_serves(vec!["anthropic".to_owned()]);
    vec![api, seat]
}

/// The plan the CLI door resolves for this file with NO flag.
fn plan(wf: &RawWorkflow) -> ExecutionAccessPlan {
    let requirement = wf
        .run
        .as_ref()
        .and_then(|run| run.value.access_requirement())
        .expect("declared");
    nika_providers::resolve_execution_plan_declared(
        &[ModelNeed::new(MODEL, true, true)],
        &probes(),
        None,
        VerbNeeds::new(true, true),
        Some(&requirement),
    )
}

type Reader = BufReader<tokio::io::ReadHalf<tokio::io::DuplexStream>>;
type Writer = tokio::io::WriteHalf<tokio::io::DuplexStream>;

async fn read(peer: &mut Reader) -> Option<Value> {
    let mut line = String::new();
    let size = tokio::time::timeout(std::time::Duration::from_secs(5), peer.read_line(&mut line))
        .await
        .ok()?
        .ok()?;
    (size != 0).then(|| serde_json::from_str(&line).expect("JSON request"))
}

async fn send(peer: &mut Writer, message: &Value) {
    let mut bytes = serde_json::to_vec(message).expect("JSON");
    bytes.push(b'\n');
    let _ = peer.write_all(&bytes).await;
    let _ = peer.flush().await;
}

fn select(id: &str, category: &str, current: &str, values: &[&str]) -> Value {
    json!({"id":id,"name":id,"category":category,"type":"select","currentValue":current,
        "options":values.iter().map(|v| json!({"value":v,"name":v})).collect::<Vec<_>>()})
}

/// claude-agent-acp 0.81.1: `max` is offered only once opus is selected.
fn claude(model: &str, effort: &str) -> Value {
    let efforts: &[&str] = if model == "claude-opus-5-5" {
        &["default", "low", "medium", "high", "xhigh", "max"]
    } else {
        &["default", "low", "medium", "high"]
    };
    json!([
        select(
            "model",
            "model",
            model,
            &["default", "claude-opus-5-5", "claude-sonnet-5"]
        ),
        select("effort", "thought_level", effort, efforts)
    ])
}

async fn claude_peer(
    mut r: Reader,
    mut w: Writer,
    answer: &'static str,
    seen: Arc<Mutex<Vec<Value>>>,
) {
    let mut model = "default".to_owned();
    while let Some(request) = read(&mut r).await {
        seen.lock().expect("seen").push(request.clone());
        let result = match request["method"].as_str() {
            Some("initialize") => json!({"protocolVersion":1,"agentInfo":
                {"name":"@agentclientprotocol/claude-agent-acp","version":"0.81.1"}}),
            Some("session/new") => {
                json!({"sessionId":"s-claude","configOptions":claude("default","default")})
            }
            Some("session/set_config_option") if request["params"]["configId"] == "model" => {
                request["params"]["value"]
                    .as_str()
                    .expect("value")
                    .clone_into(&mut model);
                json!({"configOptions":claude(&model, "default")})
            }
            Some("session/set_config_option") => {
                let effort = request["params"]["value"].as_str().expect("value");
                json!({"configOptions":claude(&model, effort)})
            }
            Some("session/prompt") => {
                let chunk = json!({"jsonrpc":"2.0","method":"session/update","params":{
                    "sessionId":"s-claude","update":{"sessionUpdate":"agent_message_chunk",
                    "content":{"type":"text","text":answer}}}});
                send(&mut w, &chunk).await;
                json!({"stopReason":"end_turn"})
            }
            other => panic!("unexpected client request {other:?}"),
        };
        send(
            &mut w,
            &json!({"jsonrpc":"2.0","id":request["id"],"result":result}),
        )
        .await;
    }
}

/// A scripted claude-code: `one_shot` drives the completion-profile client
/// (the `infer:` transport), otherwise the plain agent driver (the seat).
struct Claude {
    one_shot: bool,
    opened: AtomicUsize,
    seen: Arc<Mutex<Vec<Value>>>,
}

impl Claude {
    fn new(one_shot: bool) -> Arc<Self> {
        Arc::new(Self {
            one_shot,
            opened: AtomicUsize::new(0),
            seen: Arc::new(Mutex::new(Vec::new())),
        })
    }

    fn seen(&self) -> Vec<Value> {
        self.seen.lock().expect("seen").clone()
    }
}

impl DynAgentBackend for Claude {
    fn run_agent_boxed(
        &self,
        request: HarnessRequest,
    ) -> Pin<Box<dyn Future<Output = Result<HarnessEventStream, HarnessError>> + Send + '_>> {
        self.opened.fetch_add(1, Ordering::SeqCst);
        let (ours, theirs) = tokio::io::duplex(64 * 1024);
        let (client_read, client_write) = tokio::io::split(ours);
        let (peer_read, peer_write) = tokio::io::split(theirs);
        let answer = if self.one_shot {
            "three invariants"
        } else {
            "two tight invariants"
        };
        tokio::spawn(claude_peer(
            BufReader::new(peer_read),
            peer_write,
            answer,
            Arc::clone(&self.seen),
        ));
        let one_shot = self.one_shot;
        Box::pin(async move {
            Ok(if one_shot {
                nika_harness::drive_one_shot(client_read, client_write, request)
            } else {
                nika_harness::drive(client_read, client_write, request)
            })
        })
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

fn runtime(api: Arc<MockProvider>, one_shot: Arc<Claude>) -> SeatRuntime {
    let invoke = Arc::new(InvokeVerb::new(Arc::new(MockToolExecutor::new())));
    Runtime::new(
        ExecVerb::new(Arc::new(MockShell::new())),
        Arc::clone(&invoke),
        InferVerb::new(
            Arc::new(nika_providers::ProviderRegistry::without_http(
                nika_providers::ProvidersConfig::new(),
            )),
            MODEL,
        )
        .with_acp_transport(one_shot),
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

fn terminal<'a>(events: &'a [Event], kind: EventKind, task: &str) -> &'a Event {
    events
        .iter()
        .find(|e| e.kind == kind && text(e, "task") == Some(task))
        .unwrap_or_else(|| panic!("no {kind:?} for {task}: {events:?}"))
}

fn methods(seen: &[Value]) -> Vec<&str> {
    seen.iter().filter_map(|r| r["method"].as_str()).collect()
}

async fn run(wf: &RawWorkflow) -> (RunOutcome, Vec<Event>, Arc<Claude>, Arc<Claude>, usize) {
    let report = nika_check::check(wf);
    assert!(report.is_clean(), "{report:?}");
    let plan = plan(wf);
    assert!(plan.is_admitted(), "{:?}", plan.pin_refusal);
    assert_eq!(plan.seat.as_deref(), Some("claude-code"));
    let api = Arc::new(MockProvider::new("mock").enqueue_text("never asked"));
    let one_shot = Claude::new(true);
    let seat = Claude::new(false);
    let runtime = runtime(Arc::clone(&api), Arc::clone(&one_shot))
        .with_access_plan(plan)
        .with_harness_backend(
            Arc::clone(&seat) as Arc<dyn DynAgentBackend>,
            "claude-code".into(),
        )
        .expect("seated");
    let mut sink = VecSink::new();
    let outcome = runtime
        .run(wf, &report, &mut DeterministicStamper::new(), &mut sink)
        .await
        .expect("the run settles");
    let api_calls = api.captured_requests().len();
    (outcome, sink.into_events(), one_shot, seat, api_calls)
}

#[tokio::test]
async fn one_declaration_serves_infer_and_agent_over_acp() {
    let (outcome, events, one_shot, seat, api_calls) = run(&workflow(PLAIN)).await;
    assert!(outcome.ok, "{events:?}");
    assert_eq!(api_calls, 0, "the ready API key is never dialed");
    let configured = [
        "initialize",
        "session/new",
        "session/set_config_option",
        "session/set_config_option",
        "session/prompt",
    ];
    let infer = one_shot.seen();
    assert_eq!(methods(&infer), configured, "infer: {infer:?}");
    assert_eq!(
        infer[1]["params"]["_meta"]["claudeCode"]["options"]["tools"],
        json!([]),
        "the infer session opens under the completion profile"
    );
    assert_eq!(
        infer[1]["params"]["_meta"]["claudeCode"]["options"]["maxTurns"],
        1
    );
    let agent = seat.seen();
    assert_eq!(methods(&agent), configured, "agent: {agent:?}");
    assert!(
        agent[1]["params"].get("_meta").is_none(),
        "the agent session keeps its own loop"
    );
    for session in [&infer, &agent] {
        assert_eq!(
            session[2]["params"],
            json!({"sessionId":"s-claude","configId":"model","value":"claude-opus-5-5"})
        );
        assert_eq!(
            session[3]["params"],
            json!({"sessionId":"s-claude","configId":"effort","value":"max"})
        );
    }
    let requirement =
        json!({"via":"claude-code","protocol":"acp","fallback":"none","effort":"max"});
    for task in ["draft", "review"] {
        let done = terminal(&events, EventKind::TaskCompleted, task);
        assert_eq!(
            (text(done, "access"), text(done, "access_id")),
            (Some("harness"), Some("claude-code")),
            "{task}"
        );
        let authored: Value =
            serde_json::from_str(text(done, "access_requirement").expect("requirement"))
                .expect("json");
        assert_eq!(authored, requirement, "{task}");
        let receipt: Value =
            serde_json::from_str(text(done, "access_selection").expect("receipt")).expect("json");
        assert_eq!(
            receipt,
            json!({
                "schema": "nika/access-selection@1",
                "protocol": "acp",
                "model": {"requested": MODEL, "option": "model", "transmitted": "claude-opus-5-5",
                    "configured": "claude-opus-5-5", "configured_source": "confirmed_selection"},
                "effort": {"requested": "max", "option": "effort", "transmitted": "max",
                    "configured": "max", "configured_source": "confirmed_selection"},
                "responder": {"model": null, "evidence": "unknown"}
            }),
            "{task}"
        );
    }
    assert!(
        agent[4]["params"]["prompt"][0]["text"]
            .as_str()
            .is_some_and(|prompt| prompt.contains("three invariants")),
        "the agent read the infer answer: {agent:?}"
    );
}

#[tokio::test]
async fn a_schema_infer_over_acp_refuses_with_zero_sessions() {
    let (outcome, events, one_shot, seat, api_calls) = run(&workflow(SCHEMA)).await;
    assert!(!outcome.ok);
    assert_eq!(one_shot.opened.load(Ordering::SeqCst), 0, "zero sessions");
    assert_eq!(seat.opened.load(Ordering::SeqCst), 0, "the agent never ran");
    assert_eq!(api_calls, 0, "no substitute route");
    let failed = terminal(&events, EventKind::TaskFailed, "draft");
    let detail = text(failed, "detail").expect("detail");
    assert!(
        detail.contains("`claude-code` over ACP is not infer-grade for json_schema")
            && detail.contains("failed structured_output")
            && detail.contains("nothing was sent"),
        "{detail}"
    );
}
