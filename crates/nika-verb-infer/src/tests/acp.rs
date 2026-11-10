// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! A declared `run.access.protocol: acp` `infer:` on the verb: the route's
//! ACP one-shot carries the model and native effort to a scripted
//! claude-agent-acp and the receipt keeps the four facts apart; every
//! refusal (an unqualified route, a control the session cannot apply)
//! happens before any session opens. A `schema:` is validated by Nika, each
//! repair a new one-shot session; `vision:` files ride as ACP image blocks
//! only to an agent that advertises image prompts. A declared service tier
//! rides the Codex one-shot through the adapter's own speed option.

use std::future::Future;
use std::pin::Pin;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

use nika_error::traits::NikaErrorCode;
use nika_kernel::ai::harness::{DynAgentBackend, HarnessError, HarnessEventStream, HarnessRequest};
use nika_providers::ProvidersConfig;
use nika_types::access::{AccessFallback, AccessProtocol, AccessRequirement, ServiceTier};
use serde_json::{Value, json};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};

use super::*;

type Reader = BufReader<tokio::io::ReadHalf<tokio::io::DuplexStream>>;
type Writer = tokio::io::WriteHalf<tokio::io::DuplexStream>;

async fn read(peer: &mut Reader) -> Option<Value> {
    let mut line = String::new();
    let size = tokio::time::timeout(std::time::Duration::from_secs(5), peer.read_line(&mut line))
        .await
        .expect("scripted dialogue must settle")
        .expect("read peer");
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

/// claude-agent-acp 0.81.1: `max` is offered only once `opus` is selected.
fn claude(model: &str, effort: &str) -> Value {
    let efforts: &[&str] = if model == "opus" {
        &["default", "low", "medium", "high", "max"]
    } else {
        &["default", "low", "medium", "high"]
    };
    json!([
        select("model", "model", model, &["default", "opus", "sonnet"]),
        select("effort", "thought_level", effort, efforts)
    ])
}

/// What one scripted session answers, and whether its `initialize` advertises image prompts.
#[derive(Clone, Copy)]
struct Turn {
    answer: &'static str,
    images: bool,
}

async fn claude_peer(mut r: Reader, mut w: Writer, seen: Arc<Mutex<Vec<Value>>>, turn: Turn) {
    let mut model = "default".to_owned();
    while let Some(request) = read(&mut r).await {
        seen.lock().expect("seen").push(request.clone());
        let result = match request["method"].as_str() {
            Some("initialize") if turn.images => json!({"protocolVersion":1,"agentInfo":
                {"name":"@agentclientprotocol/claude-agent-acp","version":"0.81.1"},
                "agentCapabilities":{"promptCapabilities":{"image":true,"embeddedContext":true}}}),
            Some("initialize") => json!({"protocolVersion":1,"agentInfo":
                {"name":"@agentclientprotocol/claude-agent-acp","version":"0.81.1"}}),
            Some("session/new") => {
                json!({"sessionId":"s-infer","configOptions":claude("default","default")})
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
                    "sessionId":"s-infer","update":{"sessionUpdate":"agent_message_chunk",
                    "content":{"type":"text","text":turn.answer}}}});
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

/// The lent transport: the REAL completion-profile client over a duplex;
/// `opened` counts sessions (zero proves a refusal before any spawn).
struct Scripted {
    /// The route whose completion profile this transport drives.
    seat: &'static str,
    opened: AtomicUsize,
    seen: Arc<Mutex<Vec<Value>>>,
    /// Session n answers `answers[n]` (the last one repeats).
    answers: Vec<&'static str>,
    images: bool,
}

impl DynAgentBackend for Scripted {
    fn run_agent_boxed(
        &self,
        request: HarnessRequest,
    ) -> Pin<Box<dyn Future<Output = Result<HarnessEventStream, HarnessError>> + Send + '_>> {
        let n = self.opened.fetch_add(1, Ordering::SeqCst);
        let answer = self
            .answers
            .get(n)
            .or_else(|| self.answers.last())
            .copied()
            .unwrap_or("three invariants");
        let turn = Turn {
            answer,
            images: self.images,
        };
        let (ours, theirs) = tokio::io::duplex(64 * 1024);
        let (client_read, client_write) = tokio::io::split(ours);
        let (peer_read, peer_write) = tokio::io::split(theirs);
        tokio::spawn(claude_peer(
            BufReader::new(peer_read),
            peer_write,
            Arc::clone(&self.seen),
            turn,
        ));
        // The lent transport drives the profile of the route it stands for.
        let one_shot = nika_harness::meet_acp_one_shot(self.seat).expect("attested");
        Box::pin(async move { Ok(one_shot.drive(client_read, client_write, request)) })
    }
}

/// A transport whose adapter is gone.
struct Absent;

impl DynAgentBackend for Absent {
    fn run_agent_boxed(
        &self,
        _: HarnessRequest,
    ) -> Pin<Box<dyn Future<Output = Result<HarnessEventStream, HarnessError>> + Send + '_>> {
        Box::pin(async {
            Err(HarnessError::Unavailable {
                reason: "claude-agent-acp is not on PATH".to_owned(),
            })
        })
    }
}

fn verb(transport: Arc<dyn DynAgentBackend>) -> InferVerb {
    let registry = Arc::new(ProviderRegistry::without_http(ProvidersConfig::default()));
    InferVerb::new(registry, "anthropic/opus").with_acp_transport(transport)
}

fn scripted() -> Arc<Scripted> {
    scripted_as("claude-code")
}

/// A scripted claude-agent-acp peer behind the completion profile of `seat`.
fn scripted_as(seat: &'static str) -> Arc<Scripted> {
    scripted_answering(seat, &["three invariants"], false)
}

/// A scripted peer whose sessions answer `answers` in turn.
fn scripted_answering(seat: &'static str, answers: &[&'static str], images: bool) -> Arc<Scripted> {
    Arc::new(Scripted {
        seat,
        opened: AtomicUsize::new(0),
        seen: Arc::new(Mutex::new(Vec::new())),
        answers: answers.to_vec(),
        images,
    })
}

/// The `session/prompt` requests the peer saw, in order.
fn prompts(transport: &Scripted) -> Vec<Value> {
    let seen = transport.seen.lock().expect("seen").clone();
    seen.into_iter()
        .filter(|r| r["method"] == "session/prompt")
        .collect()
}

fn declared(via: &str, effort: &str) -> AccessRequirement {
    AccessRequirement::new()
        .with_via(Some(via.to_owned()))
        .with_protocol(Some(AccessProtocol::Acp))
        .with_fallback(Some(AccessFallback::None))
        .with_effort(Some(effort.to_owned()))
}

fn ask(via: &str, effort: &str) -> InferInput {
    InferInput::new("summarise the invariants").with_requirement(Some(&declared(via, effort)))
}

#[tokio::test]
async fn a_declared_acp_infer_rides_the_one_shot_and_keeps_its_receipt() {
    let transport = scripted();
    let out = verb(transport.clone())
        .run_on_harness("claude-code", ask("claude-code", "max"))
        .await
        .expect("answered over ACP");
    assert_eq!(out.output, json!("three invariants"));
    assert_eq!(out.requested_model, "anthropic/opus");
    assert_eq!(
        out.observed_model, None,
        "a read-back configuration is not a responder"
    );
    assert_eq!(transport.opened.load(Ordering::SeqCst), 1);
    let seen = transport.seen.lock().expect("seen").clone();
    let methods: Vec<&str> = seen.iter().filter_map(|r| r["method"].as_str()).collect();
    assert_eq!(
        methods,
        [
            "initialize",
            "session/new",
            "session/set_config_option",
            "session/set_config_option",
            "session/prompt"
        ]
    );
    assert_eq!(seen[3]["params"]["configId"], "effort");
    assert_eq!(seen[3]["params"]["value"], "max");
    let receipt = out.selection.expect("a declared run carries its receipt");
    assert_eq!(
        receipt.to_json(),
        json!({
            "schema": "nika/access-selection@1",
            "protocol": "acp",
            "model": {"requested": "anthropic/opus", "option": "model", "transmitted": "opus",
                "configured": "opus", "configured_source": "confirmed_selection"},
            "effort": {"requested": "max", "option": "effort", "transmitted": "max",
                "configured": "max", "configured_source": "confirmed_selection"},
            "responder": {"model": null, "evidence": "unknown"}
        })
    );
}

#[tokio::test]
async fn a_route_without_a_profile_refuses_before_any_session() {
    let transport = scripted();
    let err = verb(transport.clone())
        .run_on_harness("gemini-cli", ask("gemini-cli", "high"))
        .await
        .expect_err("no audited profile");
    let text = err.to_string();
    assert!(
        matches!(err, VerbInferError::HarnessAccess { .. })
            && text.contains("no audited ACP one-shot completion profile")
            && text.contains("nothing was sent"),
        "{text}"
    );
    assert_eq!(transport.opened.load(Ordering::SeqCst), 0);
}

/// The Codex route rides ITS profile: a session answering as another
/// adapter (here the scripted claude-agent-acp) is refused at initialize,
/// typed (NIKA-1805), before any prompt.
#[tokio::test]
async fn the_codex_route_admits_only_codex_acp() {
    let transport = scripted_as("codex");
    let err = verb(transport.clone())
        .run_on_harness("codex", ask("codex", "high"))
        .await
        .expect_err("another adapter answered");
    assert_eq!(err.nika_code().to_string(), "NIKA-1805", "{err}");
    assert!(
        err.to_string()
            .contains("requires the audited @agentclientprotocol/codex-acp 1.13.1 profile"),
        "{err}"
    );
    let seen = transport.seen.lock().expect("seen").clone();
    assert!(
        seen.iter().all(|r| r["method"] != "session/prompt"),
        "zero prompts: {seen:?}"
    );
}

/// D2 · the profile enforces no schema, so Nika does (spec 02 conjunct 3): the prompt carries
/// the schema, an answer that misses it is never the output, and the repair is a NEW one-shot
/// session that receives the earlier answer and the errors as text.
#[tokio::test]
async fn a_schema_task_is_validated_by_nika_and_repaired_in_a_new_session() {
    let transport = scripted_answering(
        "claude-code",
        &["Here you go: a = yes", "{\"a\": \"yes\"}"],
        false,
    );
    let mut input = ask("claude-code", "max");
    input.schema = Some(json!({"type":"object","required":["a"],
        "properties":{"a":{"type":"string"}}}));
    let out = verb(transport.clone())
        .run_on_harness("claude-code", input)
        .await
        .expect("the repair validates");
    assert_eq!(
        out.output,
        json!({"a": "yes"}),
        "the validated object is the output"
    );
    assert_eq!(
        transport.opened.load(Ordering::SeqCst),
        2,
        "one repair, one new session"
    );
    let prompts = prompts(&transport);
    assert_eq!(prompts.len(), 2);
    let first = prompts[0]["params"]["prompt"][0]["text"]
        .as_str()
        .expect("text");
    assert!(
        first.starts_with("summarise the invariants")
            && first.contains("Reply with ONLY a JSON value that satisfies this JSON Schema")
            && first.contains("\"required\":[\"a\"]"),
        "the schema rides the prompt: {first}"
    );
    let repair = prompts[1]["params"]["prompt"][0]["text"]
        .as_str()
        .expect("text");
    assert!(
        repair.contains("Your previous reply was:\nHere you go: a = yes")
            && repair.contains("did not satisfy the required output schema"),
        "the new session receives the earlier answer and the errors: {repair}"
    );
}

#[tokio::test]
async fn a_schema_task_that_never_validates_fails_infer_002_after_its_repairs() {
    let transport = scripted_answering("claude-code", &["no json here"], false);
    let mut input = ask("claude-code", "max");
    input.schema = Some(json!({"type":"object","required":["a"]}));
    let registry = Arc::new(ProviderRegistry::without_http(ProvidersConfig::default()));
    let err = InferVerb::new(registry, "anthropic/opus")
        .with_acp_transport(transport.clone())
        .with_schema_retry_budget(1)
        .run_on_harness("claude-code", input)
        .await
        .expect_err("never validates");
    assert!(
        matches!(&err, VerbInferError::SchemaValidation { attempts: 2, .. }),
        "{err:?}"
    );
    assert_eq!(err.spec_code(), "NIKA-INFER-002");
    assert_eq!(
        transport.opened.load(Ordering::SeqCst),
        2,
        "the first turn and exactly the budgeted repair"
    );
}

/// D2 · `vision:` files ride as ACP image blocks after the text, to an agent whose
/// `initialize` advertises image prompts.
#[tokio::test]
async fn a_vision_file_rides_as_an_image_block_to_an_agent_that_reads_images() {
    let transport = scripted_answering("claude-code", &["a blue logo"], true);
    let mut input = ask("claude-code", "max");
    input.vision = vec![VisionPart::loaded(
        "./logo.png",
        b"\x89PNG\r\n\x1a\nIHDR-bytes".to_vec(),
    )];
    let out = verb(transport.clone())
        .run_on_harness("claude-code", input)
        .await
        .expect("answered with the image");
    assert_eq!(out.output, json!("a blue logo"));
    let prompts = prompts(&transport);
    assert_eq!(
        prompts[0]["params"]["prompt"][1],
        json!({"type":"image","mimeType":"image/png",
            "data": "iVBORw0KGgpJSERSLWJ5dGVz"})
    );
}

#[tokio::test]
async fn a_vision_file_refuses_before_the_session_when_the_agent_reads_no_images() {
    let transport = scripted_answering("claude-code", &["unseen"], false);
    let mut input = ask("claude-code", "max");
    input.vision = vec![VisionPart::loaded(
        "./logo.png",
        b"\x89PNG\r\n\x1a\nIHDR-bytes".to_vec(),
    )];
    let err = verb(transport.clone())
        .run_on_harness("claude-code", input)
        .await
        .expect_err("no image prompts advertised");
    assert!(
        err.to_string().contains("does not advertise image prompts"),
        "{err}"
    );
    let seen = transport.seen.lock().expect("seen").clone();
    let methods: Vec<&str> = seen.iter().filter_map(|r| r["method"].as_str()).collect();
    assert_eq!(
        methods,
        ["initialize"],
        "judged on initialize, before any session"
    );
}

#[tokio::test]
async fn a_vision_url_refuses_before_any_session() {
    let transport = scripted_answering("claude-code", &["unseen"], true);
    let mut input = ask("claude-code", "max");
    input.vision = vec![VisionPart::url("https://example.com/logo.png")];
    let err = verb(transport.clone())
        .run_on_harness("claude-code", input)
        .await
        .expect_err("Nika never fetches it");
    assert!(
        matches!(&err, VerbInferError::InvalidParam { param: "vision", detail }
            if detail.contains("vision URL") && detail.contains("nothing was sent")),
        "{err:?}"
    );
    assert_eq!(transport.opened.load(Ordering::SeqCst), 0);
}

#[tokio::test]
async fn a_control_the_session_cannot_apply_refuses_before_any_session() {
    let transport = scripted();
    let mut input = ask("claude-code", "max");
    input.temperature = Some(0.2);
    input.max_tokens = Some(400);
    let err = verb(transport.clone())
        .run_on_harness("claude-code", input)
        .await
        .expect_err("never dropped");
    match &err {
        VerbInferError::InvalidParam { param, detail } => {
            assert_eq!(*param, "temperature");
            assert!(detail.contains("temperature · max_tokens"), "{detail}");
            assert!(detail.contains("nothing was sent"), "{detail}");
        }
        other => panic!("expected the parameter refusal: {other:?}"),
    }
    assert_eq!(transport.opened.load(Ordering::SeqCst), 0);
}

/// codex-acp 1.13.1's configuration for a model that supports a fast tier: `mode`, `model`,
/// `reasoning_effort` and the `fast-mode` speed option (`model_config`, `off` before `on`).
fn codex(mode: &str, model: &str, effort: &str, fast: &str) -> Value {
    json!([
        select(
            "mode",
            "mode",
            mode,
            &["read-only", "agent", "agent-full-access"]
        ),
        select("model", "model", model, &["gpt-6-astra", "gpt-6-luna"]),
        select(
            "reasoning_effort",
            "thought_level",
            effort,
            &["low", "medium", "high"]
        ),
        select("fast-mode", "model_config", fast, &["off", "on"])
    ])
}

/// A scripted codex-acp 1.13.1: every setter is applied and answered with the complete state.
async fn codex_peer(mut r: Reader, mut w: Writer, seen: Arc<Mutex<Vec<Value>>>) {
    let (mut mode, mut model, mut effort, mut fast) = (
        "agent".to_owned(),
        "gpt-6-astra".to_owned(),
        "medium".to_owned(),
        "off".to_owned(),
    );
    while let Some(request) = read(&mut r).await {
        seen.lock().expect("seen").push(request.clone());
        let value = request["params"]["value"]
            .as_str()
            .unwrap_or_default()
            .to_owned();
        let result = match request["method"].as_str() {
            Some("initialize") => json!({"protocolVersion":1,"agentInfo":
                {"name":"@agentclientprotocol/codex-acp","title":"Codex","version":"1.13.1"}}),
            Some("session/new") => {
                json!({"sessionId":"s-infer","configOptions":codex(&mode, &model, &effort, &fast)})
            }
            Some("session/set_config_option") => {
                match request["params"]["configId"].as_str() {
                    Some("model") => model = value,
                    Some("reasoning_effort") => effort = value,
                    Some("fast-mode") => fast = value,
                    Some("mode") => mode = value,
                    other => panic!("unexpected option {other:?}"),
                }
                json!({"configOptions":codex(&mode, &model, &effort, &fast)})
            }
            Some("session/prompt") => {
                let chunk = json!({"jsonrpc":"2.0","method":"session/update","params":{
                    "sessionId":"s-infer","update":{"sessionUpdate":"agent_message_chunk",
                    "content":{"type":"text","text":"OK"}}}});
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

/// The lent transport of the Codex route: its REAL completion-profile client over a duplex.
#[derive(Default)]
struct ScriptedCodex {
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
            BufReader::new(peer_read),
            peer_write,
            Arc::clone(&self.seen),
        ));
        let one_shot = nika_harness::meet_acp_one_shot("codex").expect("attested");
        Box::pin(async move { Ok(one_shot.drive(client_read, client_write, request)) })
    }
}

/// One `infer:` on the Codex route over ACP under `requirement`, with what its peer read.
async fn on_codex(
    requirement: &AccessRequirement,
) -> (Result<HarnessInferOutput, VerbInferError>, Vec<Value>) {
    let transport = Arc::new(ScriptedCodex::default());
    let registry = Arc::new(ProviderRegistry::without_http(ProvidersConfig::default()));
    let verb: InferVerb =
        InferVerb::new(registry, "openai/gpt-6-luna").with_acp_transport(transport.clone());
    let out = verb
        .run_on_harness(
            "codex",
            InferInput::new("answer OK").with_requirement(Some(requirement)),
        )
        .await;
    let seen = transport.seen.lock().expect("seen").clone();
    (out, seen)
}

/// The `(configId, value)` of every selection the peer read, in order.
fn selections(seen: &[Value]) -> Vec<(String, String)> {
    seen.iter()
        .filter(|r| r["method"] == "session/set_config_option")
        .map(|r| {
            let text = |key: &str| r["params"][key].as_str().unwrap_or_default().to_owned();
            (text("configId"), text("value"))
        })
        .collect()
}

/// A declared service tier rides the Codex ACP one-shot: the file's word becomes the
/// adapter's own `fast-mode` value after the effort and before the `read-only` mode, one
/// prompt follows, and the receipt keeps the tier's facts apart beside the model and effort.
#[tokio::test]
async fn a_declared_service_tier_rides_the_codex_one_shot_and_keeps_its_receipt() {
    for (tier, native) in [(ServiceTier::Fast, "on"), (ServiceTier::Standard, "off")] {
        let requirement = declared("codex", "low").with_service_tier(Some(tier));
        let (out, seen) = on_codex(&requirement).await;
        let out = out.expect("answered over ACP");
        assert_eq!(out.output, json!("OK"), "{tier}");
        let own = |pair: (&str, &str)| (pair.0.to_owned(), pair.1.to_owned());
        assert_eq!(
            selections(&seen),
            [
                own(("model", "gpt-6-luna")),
                own(("reasoning_effort", "low")),
                own(("fast-mode", native)),
                own(("mode", "read-only")),
            ],
            "{tier}"
        );
        let prompts = seen.iter().filter(|r| r["method"] == "session/prompt");
        assert_eq!(prompts.count(), 1, "{tier}");
        let receipt = out.selection.expect("a declared run carries its receipt");
        assert_eq!(
            receipt.to_json(),
            json!({
                "schema": "nika/access-selection@1",
                "protocol": "acp",
                "model": {"requested": "openai/gpt-6-luna", "option": "model",
                    "transmitted": "gpt-6-luna", "configured": "gpt-6-luna",
                    "configured_source": "confirmed_selection"},
                "effort": {"requested": "low", "option": "reasoning_effort",
                    "transmitted": "low", "configured": "low",
                    "configured_source": "confirmed_selection"},
                "service_tier": {"requested": tier.as_str(), "option": "fast-mode",
                    "offered": ["off", "on"], "transmitted": native, "configured": native,
                    "configured_source": "confirmed_selection",
                    "turn": {"value": null, "evidence": "unknown"},
                    "effective": {"value": null, "evidence": "unknown"}},
                "responder": {"model": null, "evidence": "unknown"}
            }),
            "{tier}"
        );
    }
}

/// Without the key the same route sends no speed selection and its receipt holds no tier.
#[tokio::test]
async fn without_a_declared_tier_the_codex_one_shot_selects_no_speed() {
    let (out, seen) = on_codex(&declared("codex", "low")).await;
    let out = out.expect("answered over ACP");
    assert!(
        selections(&seen).iter().all(|(id, _)| id != "fast-mode"),
        "{seen:?}"
    );
    let receipt = out.selection.expect("receipt").to_json();
    assert!(receipt.get("service_tier").is_none(), "{receipt}");
}

/// The adapter gone at the call: the task fails with the harness's own
/// typed class (NIKA-1803), never another transport.
#[tokio::test]
async fn an_absent_adapter_fails_typed_with_no_substitute() {
    let err = verb(Arc::new(Absent))
        .run_on_harness("claude-code", ask("claude-code", "max"))
        .await
        .expect_err("unavailable");
    assert!(
        matches!(
            &err,
            VerbInferError::Harness {
                source: HarnessError::Unavailable { .. }
            }
        ),
        "{err:?}"
    );
    assert_eq!(err.nika_code().to_string(), "NIKA-1803");
    assert!(!err.is_transient());
}
