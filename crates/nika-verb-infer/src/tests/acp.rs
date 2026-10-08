// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! A declared `run.access.protocol: acp` `infer:` on the verb: the route's
//! ACP one-shot carries the model and native effort to a scripted
//! claude-agent-acp and the receipt keeps the four facts apart; every
//! refusal (an unqualified route, a schema, a control the session cannot
//! apply) happens before any session opens.

use std::future::Future;
use std::pin::Pin;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

use nika_error::traits::NikaErrorCode;
use nika_kernel::ai::harness::{DynAgentBackend, HarnessError, HarnessEventStream, HarnessRequest};
use nika_providers::ProvidersConfig;
use nika_types::access::{AccessFallback, AccessProtocol, AccessRequirement};
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

async fn claude_peer(mut r: Reader, mut w: Writer, seen: Arc<Mutex<Vec<Value>>>) {
    let mut model = "default".to_owned();
    while let Some(request) = read(&mut r).await {
        seen.lock().expect("seen").push(request.clone());
        let result = match request["method"].as_str() {
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
                    "content":{"type":"text","text":"three invariants"}}}});
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
    opened: AtomicUsize,
    seen: Arc<Mutex<Vec<Value>>>,
}

impl DynAgentBackend for Scripted {
    fn run_agent_boxed(
        &self,
        request: HarnessRequest,
    ) -> Pin<Box<dyn Future<Output = Result<HarnessEventStream, HarnessError>> + Send + '_>> {
        self.opened.fetch_add(1, Ordering::SeqCst);
        let (ours, theirs) = tokio::io::duplex(64 * 1024);
        let (client_read, client_write) = tokio::io::split(ours);
        let (peer_read, peer_write) = tokio::io::split(theirs);
        tokio::spawn(claude_peer(
            BufReader::new(peer_read),
            peer_write,
            Arc::clone(&self.seen),
        ));
        Box::pin(async move {
            Ok(nika_harness::drive_one_shot(
                client_read,
                client_write,
                request,
            ))
        })
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
    Arc::new(Scripted {
        opened: AtomicUsize::new(0),
        seen: Arc::new(Mutex::new(Vec::new())),
    })
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
async fn codex_acp_infer_refuses_before_any_session() {
    let transport = scripted();
    let err = verb(transport.clone())
        .run_on_harness("codex", ask("codex", "high"))
        .await
        .expect_err("not qualified");
    let text = err.to_string();
    assert!(
        matches!(err, VerbInferError::HarnessAccess { .. })
            && text.contains("no qualified tool-free ACP one-shot profile yet")
            && text.contains("nothing was sent"),
        "{text}"
    );
    assert_eq!(transport.opened.load(Ordering::SeqCst), 0);
}

#[tokio::test]
async fn a_schema_task_refuses_before_any_session() {
    let transport = scripted();
    let mut input = ask("claude-code", "max");
    input.schema = Some(json!({"type":"object","properties":{"a":{"type":"string"}}}));
    let err = verb(transport.clone())
        .run_on_harness("claude-code", input)
        .await
        .expect_err("text only");
    assert!(
        err.to_string().contains("failed structured_output"),
        "{err}"
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
