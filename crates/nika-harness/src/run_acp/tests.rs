// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The Run `infer:` one-shot over ACP against scripted claude-agent-acp
//! peers: the exact profile, the selection and its read-back before ONE
//! prompt, every refusal proven with zero prompts, and a tool beat or a
//! model moved mid-turn refusing the whole answer.

use std::future::Future;
use std::sync::{Arc, Mutex};

use nika_kernel::ai::harness::ModelProvenance;
use serde_json::{Value, json};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};

use super::*;
use crate::authoring::acp::profile;

type Reader = BufReader<tokio::io::ReadHalf<tokio::io::DuplexStream>>;
type Writer = tokio::io::WriteHalf<tokio::io::DuplexStream>;

/// One line from the client, `None` at its EOF (the client gave up).
async fn read(peer: &mut Reader) -> Option<Value> {
    let mut line = String::new();
    let size = tokio::time::timeout(Duration::from_secs(5), peer.read_line(&mut line))
        .await
        .expect("scripted dialogue must settle")
        .expect("read peer");
    (size != 0).then(|| serde_json::from_str(&line).expect("JSON request"))
}

async fn send(peer: &mut Writer, message: &Value) {
    let mut bytes = serde_json::to_vec(message).expect("JSON");
    bytes.push(b'\n');
    // The client may already have refused and closed its end.
    let _ = peer.write_all(&bytes).await;
    let _ = peer.flush().await;
}

async fn reply(peer: &mut Writer, request: &Value, result: Value) {
    send(
        peer,
        &json!({"jsonrpc":"2.0","id":request["id"],"result":result}),
    )
    .await;
}

async fn update(peer: &mut Writer, update: Value) {
    send(
        peer,
        &json!({"jsonrpc":"2.0","method":"session/update",
            "params":{"sessionId":"s-infer","update":update}}),
    )
    .await;
}

fn select(id: &str, category: &str, current: &str, values: &[&str]) -> Value {
    json!({"id":id,"name":id,"category":category,"type":"select","currentValue":current,
        "options":values.iter().map(|v| json!({"value":v,"name":v})).collect::<Vec<_>>()})
}

/// The claude-agent-acp 0.81.1 shape: `model` plus `effort` (category
/// `thought_level`) whose values belong to the CURRENT model.
fn claude(model: &str, effort: &str) -> Value {
    let efforts: &[&str] = if model == "opus" {
        &["default", "low", "medium", "high", "xhigh", "max"]
    } else {
        &["default", "low", "medium", "high"]
    };
    json!([
        select("model", "model", model, &["default", "opus", "sonnet"]),
        select("effort", "thought_level", effort, efforts)
    ])
}

/// How the scripted turn ends after the prompt.
#[derive(Debug, Clone, Copy)]
enum Ending {
    /// One text chunk, then `end_turn`.
    Answer,
    /// A tool beat (the profile forbids it), then `end_turn`.
    ToolCall,
    /// The agent moves its model to `sonnet`, answers, ends the turn.
    MovesModel,
    /// Never answers the prompt.
    Silent,
}

/// A scripted claude-agent-acp 0.81.1 recording every request it reads.
async fn claude_peer(mut r: Reader, mut w: Writer, ending: Ending, seen: Arc<Mutex<Vec<Value>>>) {
    let mut model = "default".to_owned();
    while let Some(request) = read(&mut r).await {
        seen.lock()
            .expect("seen")
            .push(json!({"method":request["method"],"params":request["params"]}));
        match request["method"].as_str() {
            Some("initialize") => {
                let identity = json!({"protocolVersion":1,"agentInfo":
                    {"name":"@agentclientprotocol/claude-agent-acp","version":"0.81.1"}});
                reply(&mut w, &request, identity).await;
            }
            Some("session/new") => {
                let config = claude("default", "default");
                reply(
                    &mut w,
                    &request,
                    json!({"sessionId":"s-infer","configOptions":config}),
                )
                .await;
            }
            Some("session/set_config_option") if request["params"]["configId"] == "model" => {
                request["params"]["value"]
                    .as_str()
                    .expect("value")
                    .clone_into(&mut model);
                let config = claude(&model, "default");
                reply(&mut w, &request, json!({"configOptions":config})).await;
            }
            Some("session/set_config_option") => {
                let effort = request["params"]["value"].as_str().expect("value");
                let config = claude(&model, effort);
                reply(&mut w, &request, json!({"configOptions":config})).await;
            }
            Some("session/prompt") => match ending {
                Ending::Silent => {}
                Ending::Answer | Ending::ToolCall | Ending::MovesModel => {
                    if matches!(ending, Ending::ToolCall) {
                        update(
                            &mut w,
                            json!({"sessionUpdate":"tool_call","toolCallId":"t1",
                            "title":"Read secrets.txt","kind":"read","status":"pending"}),
                        )
                        .await;
                    }
                    if matches!(ending, Ending::MovesModel) {
                        let moved = claude("sonnet", "high");
                        update(
                            &mut w,
                            json!({"sessionUpdate":"config_option_update","configOptions":moved}),
                        )
                        .await;
                    }
                    let chunk = json!({"sessionUpdate":"agent_message_chunk",
                        "content":{"type":"text","text":"three invariants"}});
                    update(&mut w, chunk).await;
                    reply(&mut w, &request, json!({"stopReason":"end_turn"})).await;
                }
            },
            other => panic!("unexpected client request {other:?}"),
        }
    }
}

/// A lent transport: every one-shot drives the REAL completion-profile
/// client over a duplex against the scripted peer.
struct Scripted {
    ending: Ending,
    seen: Arc<Mutex<Vec<Value>>>,
}

impl DynAgentBackend for Scripted {
    fn run_agent_boxed(
        &self,
        request: HarnessRequest,
    ) -> Pin<Box<dyn Future<Output = Result<HarnessEventStream, HarnessError>> + Send + '_>> {
        let (ours, theirs) = tokio::io::duplex(64 * 1024);
        let (client_read, client_write) = tokio::io::split(ours);
        let (peer_read, peer_write) = tokio::io::split(theirs);
        tokio::spawn(claude_peer(
            BufReader::new(peer_read),
            peer_write,
            self.ending,
            Arc::clone(&self.seen),
        ));
        Box::pin(async move { Ok(drive_one_shot(client_read, client_write, request)) })
    }
}

fn ask(effort: &str) -> HarnessRequest {
    HarnessRequest::new("summarise the invariants", "/never/the/session/root")
        .with_system("be brief")
        .with_requested_model("anthropic/opus")
        .with_requested_effort(Some(effort.to_owned()))
}

/// Run one scripted one-shot; returns the result and every request seen.
async fn one_shot(
    ending: Ending,
    request: HarnessRequest,
    timeout: Duration,
) -> (Result<HarnessOutcome, HarnessError>, Vec<Value>) {
    let seen = Arc::new(Mutex::new(Vec::new()));
    let transport = Scripted {
        ending,
        seen: Arc::clone(&seen),
    };
    let claude = meet_acp_one_shot("claude-code").expect("attested");
    let result = claude.run_over(&transport, request, Some(timeout)).await;
    let seen = seen.lock().expect("seen").clone();
    (result, seen)
}

fn methods(seen: &[Value]) -> Vec<&str> {
    seen.iter().filter_map(|r| r["method"].as_str()).collect()
}

#[test]
fn only_claude_code_carries_an_attested_acp_one_shot() {
    let claude = meet_acp_one_shot("claude-code").expect("attested");
    assert_eq!(claude.seat, "claude-code");
    let proof = claude.attestation;
    assert!(proof.single_turn && proof.no_implicit_tools && proof.model_identity_observable);
    assert_eq!(proof.structured_output, StructuredOutputGrade::Text);
    let codex = meet_acp_one_shot("codex")
        .expect_err("not qualified")
        .witness;
    for named in [
        "CODEX_CONFIG",
        "shell_tool",
        "apply_patch",
        "MCP",
        "cancellation",
    ] {
        assert!(codex.contains(named), "{named}: {codex}");
    }
    assert!(
        !codex.contains("exposes no empty-tools"),
        "the corrected witness never claims the avenue is absent: {codex}"
    );
    for seat in ["kimi-code", "gemini-cli"] {
        let refused = meet_acp_one_shot(seat).expect_err(seat).witness;
        assert!(refused.contains(seat), "{refused}");
    }
}

#[test]
fn a_structured_need_is_refused_before_any_spawn() {
    let claude = meet_acp_one_shot("claude-code").expect("attested");
    assert!(claude.grade(StructuredOutputGrade::Text).is_ok());
    for need in [
        StructuredOutputGrade::Json,
        StructuredOutputGrade::JsonSchema,
    ] {
        let witness = claude.grade(need).expect_err("text only").witness;
        assert!(witness.contains("failed structured_output"), "{witness}");
        assert!(witness.contains(need.as_str()), "{witness}");
        assert!(witness.contains("`claude-code` over ACP"), "{witness}");
    }
}

/// The exact wire: the audited options with `session/new`, a fresh scratch
/// root, the model then the effort the REFRESHED options offer (`max`
/// exists only once `opus` is selected), both read back, then one prompt.
#[tokio::test]
async fn the_profile_selection_and_read_back_precede_one_prompt() {
    let (result, seen) = one_shot(Ending::Answer, ask("max"), Duration::from_secs(5)).await;
    let outcome = result.expect("answered");
    assert_eq!(outcome.output, "three invariants");
    assert_eq!(
        methods(&seen),
        [
            "initialize",
            "session/new",
            "session/set_config_option",
            "session/set_config_option",
            "session/prompt"
        ]
    );
    assert_eq!(seen[1]["params"]["_meta"], profile());
    assert_eq!(seen[1]["params"]["mcpServers"], json!([]));
    let root = seen[1]["params"]["cwd"].as_str().expect("cwd").to_owned();
    assert_ne!(
        root, "/never/the/session/root",
        "a fresh scratch, never a caller root"
    );
    assert!(
        !std::path::Path::new(&root).exists(),
        "the scratch dies with the call"
    );
    assert_eq!(
        seen[2]["params"],
        json!({"sessionId":"s-infer","configId":"model","value":"opus"})
    );
    assert_eq!(
        seen[3]["params"],
        json!({"sessionId":"s-infer","configId":"effort","value":"max"})
    );
    assert_eq!(
        seen[4]["params"]["prompt"],
        json!([{"type":"text","text":"be brief\n\nsummarise the invariants"}])
    );
    let selection = &outcome.selection;
    assert_eq!(selection.model_option.as_deref(), Some("model"));
    assert_eq!(selection.transmitted_model.as_deref(), Some("opus"));
    assert_eq!(selection.effort_option.as_deref(), Some("effort"));
    assert_eq!(selection.transmitted_effort.as_deref(), Some("max"));
    assert_eq!(selection.configured_effort.as_deref(), Some("max"));
    assert_eq!(
        selection.configured_effort_source,
        Some(ModelProvenance::ConfirmedSelection)
    );
    assert!(selection.changed_mid_turn.is_empty());
    assert_eq!(outcome.observed_model.as_deref(), Some("opus"));
    assert_eq!(
        outcome.observed_model_source,
        Some(ModelProvenance::ConfirmedSelection),
        "a read-back configuration, never a responder"
    );
}

#[tokio::test]
async fn an_effort_the_selected_model_does_not_offer_refuses_with_zero_prompts() {
    let (result, seen) = one_shot(Ending::Answer, ask("ultra"), Duration::from_secs(5)).await;
    match result {
        Err(HarnessError::Selection { reason }) => {
            assert!(reason.contains("`ultra`"), "{reason}");
            assert!(
                reason.contains("default · low · medium · high · xhigh · max"),
                "{reason}"
            );
        }
        other => panic!("an unoffered effort must refuse: {other:?}"),
    }
    assert!(!methods(&seen).contains(&"session/prompt"), "{seen:?}");
}

#[tokio::test]
async fn a_tool_beat_refuses_the_whole_answer() {
    let (result, seen) = one_shot(Ending::ToolCall, ask("high"), Duration::from_secs(5)).await;
    match result {
        Err(HarnessError::Refused { reason }) => {
            assert_eq!(
                reason,
                "ACP infer emitted a tool, media or unsupported event; no answer accepted"
            );
        }
        other => panic!("a tool beat must refuse: {other:?}"),
    }
    assert_eq!(
        methods(&seen)
            .iter()
            .filter(|m| **m == "session/prompt")
            .count(),
        1
    );
}

/// An explicit selection is exact: an agent that moves the model during
/// the turn has not answered under it.
#[tokio::test]
async fn a_model_moved_mid_turn_refuses_the_answer() {
    let (result, _) = one_shot(Ending::MovesModel, ask("high"), Duration::from_secs(5)).await;
    match result {
        Err(HarnessError::Selection { reason }) => {
            assert!(reason.contains("model=sonnet"), "{reason}");
            assert!(
                reason.contains("an explicit selection is exact"),
                "{reason}"
            );
        }
        other => panic!("a moved model must refuse: {other:?}"),
    }
}

#[tokio::test]
async fn a_silent_turn_ends_at_the_deadline() {
    let (result, seen) = one_shot(Ending::Silent, ask("high"), Duration::from_millis(200)).await;
    match result {
        Err(HarnessError::Refused { reason }) => {
            assert_eq!(reason, "ACP infer timed out; no answer accepted");
        }
        other => panic!("a silent turn must end: {other:?}"),
    }
    assert!(methods(&seen).contains(&"session/prompt"));
}
