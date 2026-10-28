// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! ONE declaration, both roles: `via: claude-code, protocol: acp` with a
//! native effort serves an `infer:` AND an `agent:` task over ACP, each
//! against its own scripted claude-agent-acp. The `infer:` session opens
//! under the audited completion profile (its options sent with
//! `session/new`), the `agent:` session under the agent's own; both apply
//! the model and the effort and read them back before exactly one prompt;
//! the ready API key is never dialed; each terminal carries the authored
//! requirement and its own call's receipt. A `schema:` infer rides the same
//! one-shot: the profile's own level is text, so Nika carries the schema in
//! the prompt and validates the answer (spec 02, conjunct 3).

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

/// claude-agent-acp 0.81.1: `max` is offered only once opus is selected; its mode option opens
/// on the user's `permissions.defaultMode` (`auto` here, a classifier that approves for the
/// client), which an agent session must leave for `default` before its prompt (NIK-266).
fn claude(model: &str, effort: &str, mode: &str) -> Value {
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
        select("effort", "thought_level", effort, efforts),
        select(
            "mode",
            "mode",
            mode,
            &["default", "acceptEdits", "plan", "auto"]
        )
    ])
}

async fn claude_peer(
    mut r: Reader,
    mut w: Writer,
    answer: &'static str,
    seen: Arc<Mutex<Vec<Value>>>,
    images: bool,
) {
    let (mut model, mut effort, mut mode) = (
        "default".to_owned(),
        "default".to_owned(),
        "auto".to_owned(),
    );
    while let Some(request) = read(&mut r).await {
        seen.lock().expect("seen").push(request.clone());
        let result = match request["method"].as_str() {
            Some("initialize") if images => json!({"protocolVersion":1,"agentInfo":
                {"name":"@agentclientprotocol/claude-agent-acp","version":"0.81.1"},
                "agentCapabilities":{"promptCapabilities":{"image":true}}}),
            Some("initialize") => json!({"protocolVersion":1,"agentInfo":
                {"name":"@agentclientprotocol/claude-agent-acp","version":"0.81.1"}}),
            Some("session/new") => {
                json!({"sessionId":"s-claude","configOptions":claude(&model, &effort, &mode)})
            }
            Some("session/set_config_option") => {
                let value = request["params"]["value"]
                    .as_str()
                    .expect("value")
                    .to_owned();
                match request["params"]["configId"].as_str() {
                    Some("model") => model = value,
                    Some("effort") => effort = value,
                    Some("mode") => mode = value,
                    other => panic!("unexpected option {other:?}"),
                }
                json!({"configOptions":claude(&model, &effort, &mode)})
            }
            Some("session/prompt") => {
                let asked = request["params"]["prompt"][0]["text"]
                    .as_str()
                    .unwrap_or("");
                // Under a schema the scripted model answers in JSON, fenced like a real one.
                let answer = if asked.contains("satisfies this JSON Schema") {
                    "```json\n{\"a\": \"three invariants\"}\n```"
                } else {
                    answer
                };
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
    /// Whether `initialize` advertises image prompts (claude-agent-acp does).
    images: bool,
    opened: AtomicUsize,
    seen: Arc<Mutex<Vec<Value>>>,
}

impl Claude {
    fn new(one_shot: bool) -> Arc<Self> {
        Arc::new(Self {
            one_shot,
            images: false,
            opened: AtomicUsize::new(0),
            seen: Arc::new(Mutex::new(Vec::new())),
        })
    }

    /// The one-shot transport of an agent that reads images.
    fn reading_images() -> Arc<Self> {
        Arc::new(Self {
            one_shot: true,
            images: true,
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
            self.images,
        ));
        let one_shot = self.one_shot;
        Box::pin(async move {
            Ok(if one_shot {
                nika_harness::meet_acp_one_shot("claude-code")
                    .expect("attested")
                    .drive(client_read, client_write, request)
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
    // The agent session is also confined: `auto` is left for `default`, read back (NIK-266).
    let confined = [
        "initialize",
        "session/new",
        "session/set_config_option",
        "session/set_config_option",
        "session/set_config_option",
        "session/prompt",
    ];
    assert_eq!(methods(&agent), confined, "agent: {agent:?}");
    assert_eq!(
        agent[4]["params"],
        json!({"sessionId":"s-claude","configId":"mode","value":"default"})
    );
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
        agent[5]["params"]["prompt"][0]["text"]
            .as_str()
            .is_some_and(|prompt| prompt.contains("three invariants")),
        "the agent read the infer answer: {agent:?}"
    );
}

/// D2 · `schema:` on the seat: one one-shot session, the schema in its prompt, the answer
/// validated by Nika and handed downstream as the object.
#[tokio::test]
async fn a_schema_infer_over_acp_is_validated_by_nika_on_one_session() {
    let (outcome, events, one_shot, seat, api_calls) = run(&workflow(SCHEMA)).await;
    assert!(outcome.ok, "{events:?}");
    assert_eq!(
        one_shot.opened.load(Ordering::SeqCst),
        1,
        "a valid answer needs no repair"
    );
    assert_eq!(api_calls, 0, "no substitute route");
    let prompt = one_shot
        .seen()
        .into_iter()
        .find(|r| r["method"] == "session/prompt")
        .expect("one prompt");
    assert!(
        prompt["params"]["prompt"][0]["text"]
            .as_str()
            .is_some_and(|t| t.contains("Reply with ONLY a JSON value")),
        "{prompt}"
    );
    assert!(
        seat.seen()
            .iter()
            .filter(|r| r["method"] == "session/prompt")
            .any(|r| r["params"]["prompt"][0]["text"]
                .as_str()
                .is_some_and(|t| t.contains("{\"a\":\"three invariants\"}"))),
        "the agent read the validated object, not the fenced text"
    );
}

/// The run of `wf` over ACP on the REAL builtin plane rooted at `root` (the production
/// `BuiltinDispatcher` over `AnchoredFs`, bounded by the workflow's own `permits.fs`), the
/// input `pic` bound to `pic`.
async fn run_on_plane(
    wf: &RawWorkflow,
    root: &std::path::Path,
    pic: &str,
) -> (RunOutcome, Vec<Event>, Arc<Claude>, usize) {
    let report = nika_check::check(wf);
    assert!(report.is_clean(), "{report:?}");
    let plan = plan(wf);
    assert_eq!(plan.seat.as_deref(), Some("claude-code"));
    let plane = Arc::new(
        nika_builtin::BuiltinDispatcher::new(
            Arc::new(nika_fs::AnchoredFs::new(root.to_path_buf())),
            Arc::new(nika_kernel_mock::MockHttp::new()),
            Arc::new(MockClock::new()),
            Arc::new(nika_builtin::NullEmitter::default()),
            Arc::new(nika_builtin::NonInteractive::default()),
            Arc::new(nika_builtin::NoWorkflow::default()),
        )
        .with_fs_boundary(crate::compose::fs_boundary_of(wf)),
    );
    let invoke = Arc::new(InvokeVerb::new(plane));
    let api = Arc::new(MockProvider::new("mock").enqueue_text("never asked"));
    let one_shot = Claude::reading_images();
    let runtime = Runtime::new(
        ExecVerb::new(Arc::new(MockShell::new())),
        Arc::clone(&invoke),
        InferVerb::new(
            Arc::new(nika_providers::ProviderRegistry::without_http(
                nika_providers::ProvidersConfig::new(),
            )),
            MODEL,
        )
        .with_acp_transport(Arc::clone(&one_shot) as Arc<dyn DynAgentBackend>),
        AgentVerb::new(
            Arc::clone(&api),
            invoke,
            Arc::new(MockToolDefinitionProvider::new()),
            MODEL,
        ),
        MockClock::new(),
        RuntimeConfig::default(),
    )
    .with_var_overrides(std::iter::once(("pic".to_owned(), Value::from(pic))).collect())
    .with_access_plan(plan);
    let mut sink = VecSink::new();
    let outcome = runtime
        .run(wf, &report, &mut DeterministicStamper::new(), &mut sink)
        .await
        .expect("the run settles");
    let api_calls = api.captured_requests().len();
    (outcome, sink.into_events(), one_shot, api_calls)
}

/// Spec 02 §Vision inputs on the ACP route: the run reads a `vision:` file through the
/// workflow's boundary before any session opens. Inside the grant its bytes ride the prompt
/// as an image block; outside it the task fails `NIKA-SEC-004`, a denied fs frame is
/// witnessed, and no session ever opens, so nothing reaches the agent.
#[tokio::test]
async fn a_vision_file_over_acp_is_read_inside_the_boundary_before_any_session() {
    let root = std::env::temp_dir().join(format!("nika-acp-vision-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(root.join("diagrams")).expect("dir");
    std::fs::create_dir_all(root.join("secrets")).expect("dir");
    let root = root.canonicalize().expect("canonical root");
    let flow = b"\x89PNG\r\n\x1a\nflow".to_vec();
    std::fs::write(root.join("diagrams/flow.png"), &flow).expect("flow");
    std::fs::write(root.join("secrets/key.png"), b"\x89PNG\r\n\x1a\nsecret").expect("key");
    let yaml = format!(
        "nika: acp-vision\nmodel: {MODEL}\npermits:\n  fs: {{ read: [\"./diagrams/**\"] }}\n\
         inputs:\n  pic: {{ type: string, default: \"./diagrams/flow.png\" }}\nrun:\n  access:\n    \
         via: claude-code\n    protocol: acp\n    fallback: none\n  reasoning:\n    effort: max\n\
         tasks:\n  draft:\n    infer:\n      prompt: \"describe the flow\"\n      vision:\n        \
         - {{ source: file, path: \"${{{{ inputs.pic }}}}\" }}\n"
    );
    let wf = nika_schema::parse(
        &yaml,
        nika_schema::FileId::new(0),
        nika_schema::ParseMode::Strict,
    )
    .expect("parses");

    let (outcome, events, one_shot, api_calls) =
        run_on_plane(&wf, &root, "./diagrams/flow.png").await;
    assert!(outcome.ok, "{events:?}");
    assert_eq!(api_calls, 0, "the ready API key is never dialed");
    let prompt = one_shot
        .seen()
        .into_iter()
        .find(|r| r["method"] == "session/prompt")
        .expect("one prompt");
    assert_eq!(
        prompt["params"]["prompt"][1],
        json!({"type":"image","mimeType":"image/png",
            "data": base64::Engine::encode(&base64::engine::general_purpose::STANDARD, &flow)})
    );

    let secret = root.join("secrets/key.png").to_string_lossy().into_owned();
    let (outcome, events, one_shot, api_calls) = run_on_plane(&wf, &root, &secret).await;
    assert!(!outcome.ok);
    let failed = terminal(&events, EventKind::TaskFailed, "draft");
    assert!(
        text(failed, "outcome").is_some_and(|o| o.contains("NIKA-SEC-004")),
        "{failed:?}"
    );
    assert_eq!(
        one_shot.opened.load(Ordering::SeqCst),
        0,
        "no session ever opened: the file was refused before the agent"
    );
    assert_eq!(api_calls, 0);
    assert!(
        events.iter().any(|e| e.kind == EventKind::PermitChecked
            && text(e, "plane") == Some("fs")
            && text(e, "decision") == Some("deny")),
        "the refusal is witnessed: {events:?}"
    );
    let _ = std::fs::remove_dir_all(&root);
}
