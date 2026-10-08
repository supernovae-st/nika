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
        let claude = meet_acp_one_shot("claude-code").expect("attested");
        Box::pin(async move { Ok(claude.drive(client_read, client_write, request)) })
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
fn claude_code_and_codex_carry_attested_acp_one_shots() {
    let claude = meet_acp_one_shot("claude-code").expect("attested");
    assert_eq!(claude.seat, "claude-code");
    let proof = claude.attestation;
    assert!(proof.single_turn && proof.no_implicit_tools && proof.model_identity_observable);
    assert_eq!(proof.structured_output, StructuredOutputGrade::Text);
    let codex = meet_acp_one_shot("codex").expect("attested");
    assert_eq!(codex.seat, "codex");
    assert_eq!(
        codex.attestation.structured_output,
        StructuredOutputGrade::Text
    );
    assert!(
        codex
            .attestation
            .proof
            .contains("RESIDUE: apply_patch stays callable"),
        "the Codex contract names its residue: {}",
        codex.attestation.proof
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
                "ACP infer emitted a tool, media or unsupported event (`tool_call` · kind `read`); no \
                 answer accepted"
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

/// How the scripted codex-acp treats the mode selection.
#[derive(Debug, Clone, Copy)]
enum Mode {
    /// Applies it and answers the complete configuration.
    Confirms,
    /// Answers as if nothing changed.
    Ignores,
    /// Advertises no mode option at all.
    Absent,
}

/// codex-acp 1.13.1's shape: `model`, `reasoning_effort` (`thought_level`)
/// whose values belong to the CURRENT model, and `mode`.
fn codex(model: &str, effort: &str, mode: Option<&str>) -> Value {
    let efforts: &[&str] = if model == "gpt-6-astra" {
        &["low", "medium", "high", "xhigh", "max", "ultra"]
    } else {
        &["low", "medium", "high", "xhigh", "max"]
    };
    let mut options = vec![
        select("model", "model", model, &["gpt-6-astra", "gpt-6-luna"]),
        select("reasoning_effort", "thought_level", effort, efforts),
    ];
    if let Some(mode) = mode {
        options.push(select(
            "mode",
            "mode",
            mode,
            &["read-only", "agent", "agent-full-access"],
        ));
    }
    Value::Array(options)
}

/// A scripted codex-acp 1.13.1 recording every request it reads.
async fn codex_peer(mut r: Reader, mut w: Writer, mode: Mode, seen: Arc<Mutex<Vec<Value>>>) {
    let advertised = |current: &str| match mode {
        Mode::Absent => None,
        Mode::Confirms | Mode::Ignores => Some(current.to_owned()),
    };
    let (mut model, mut effort, mut current) = (
        "gpt-6-astra".to_owned(),
        "medium".to_owned(),
        "agent".to_owned(),
    );
    while let Some(request) = read(&mut r).await {
        seen.lock()
            .expect("seen")
            .push(json!({"method":request["method"],"params":request["params"]}));
        let config_id = request["params"]["configId"].as_str().unwrap_or_default();
        let value = request["params"]["value"].as_str().unwrap_or_default();
        let result = match request["method"].as_str() {
            Some("initialize") => json!({"protocolVersion":1,"agentInfo":
                {"name":"@agentclientprotocol/codex-acp","title":"Codex","version":"1.13.1"}}),
            Some("session/new") => json!({"sessionId":"s-codex",
                "configOptions":codex(&model, &effort, advertised(&current).as_deref())}),
            Some("session/set_config_option") => {
                match config_id {
                    "model" => value.clone_into(&mut model),
                    "reasoning_effort" => value.clone_into(&mut effort),
                    _ if matches!(mode, Mode::Confirms) => value.clone_into(&mut current),
                    _ => {}
                }
                json!({"configOptions":codex(&model, &effort, advertised(&current).as_deref())})
            }
            Some("session/prompt") => {
                let chunk = json!({"sessionUpdate":"agent_message_chunk",
                    "content":{"type":"text","text":"OK"}});
                send(
                    &mut w,
                    &json!({"jsonrpc":"2.0","method":"session/update",
                        "params":{"sessionId":"s-codex","update":chunk}}),
                )
                .await;
                json!({"stopReason":"end_turn"})
            }
            other => panic!("unexpected client request {other:?}"),
        };
        reply(&mut w, &request, result).await;
    }
}

/// A lent transport driving the Codex completion profile over a duplex.
struct ScriptedCodex {
    mode: Mode,
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
            self.mode,
            Arc::clone(&self.seen),
        ));
        let codex = meet_acp_one_shot("codex").expect("attested");
        Box::pin(async move { Ok(codex.drive(client_read, client_write, request)) })
    }
}

async fn codex_one_shot(
    mode: Mode,
    effort: &str,
) -> (Result<HarnessOutcome, HarnessError>, Vec<Value>) {
    let seen = Arc::new(Mutex::new(Vec::new()));
    let transport = ScriptedCodex {
        mode,
        seen: Arc::clone(&seen),
    };
    let request = HarnessRequest::new("answer OK", "/never/the/session/root")
        .with_requested_model("openai/gpt-6-luna")
        .with_requested_effort(Some(effort.to_owned()));
    let codex = meet_acp_one_shot("codex").expect("attested");
    let result = codex
        .run_over(&transport, request, Some(Duration::from_secs(5)))
        .await;
    let seen = seen.lock().expect("seen").clone();
    (result, seen)
}

/// The Codex profile's wire: no SDK options with `session/new` (its tool
/// surface is closed by the configuration proven at spawn), then the model,
/// the effort the REFRESHED options offer, and the `read-only` mode, each
/// applied and read back, before exactly one prompt.
#[tokio::test]
async fn the_codex_profile_selects_model_effort_and_read_only_before_one_prompt() {
    let (result, seen) = codex_one_shot(Mode::Confirms, "max").await;
    let outcome = result.expect("answered");
    assert_eq!(outcome.output, "OK");
    assert_eq!(
        methods(&seen),
        [
            "initialize",
            "session/new",
            "session/set_config_option",
            "session/set_config_option",
            "session/set_config_option",
            "session/prompt"
        ]
    );
    assert!(seen[1]["params"].get("_meta").is_none(), "{:?}", seen[1]);
    assert_eq!(
        seen[2]["params"],
        json!({"sessionId":"s-codex","configId":"model","value":"gpt-6-luna"})
    );
    assert_eq!(
        seen[3]["params"],
        json!({"sessionId":"s-codex","configId":"reasoning_effort","value":"max"})
    );
    assert_eq!(
        seen[4]["params"],
        json!({"sessionId":"s-codex","configId":"mode","value":"read-only"})
    );
    let selection = &outcome.selection;
    assert_eq!(selection.transmitted_model.as_deref(), Some("gpt-6-luna"));
    assert_eq!(selection.configured_effort.as_deref(), Some("max"));
    assert_eq!(selection.effort_option.as_deref(), Some("reasoning_effort"));
}

/// `ultra` exists for the session default (astra) only: judged on the
/// options refreshed by selecting luna, it refuses before any prompt.
#[tokio::test]
async fn a_codex_effort_only_the_previous_model_offered_refuses_with_zero_prompts() {
    let (result, seen) = codex_one_shot(Mode::Confirms, "ultra").await;
    match result {
        Err(HarnessError::Selection { reason }) => {
            assert!(reason.contains("`ultra`"), "{reason}");
            assert!(
                reason.contains("low · medium · high · xhigh · max"),
                "{reason}"
            );
        }
        other => panic!("an unoffered effort must refuse: {other:?}"),
    }
    assert!(!methods(&seen).contains(&"session/prompt"), "{seen:?}");
}

#[tokio::test]
async fn a_mode_the_session_does_not_confirm_refuses_with_zero_prompts() {
    let (result, seen) = codex_one_shot(Mode::Ignores, "max").await;
    match result {
        Err(HarnessError::Refused { reason }) => {
            assert!(
                reason.contains("did not confirm mode `read-only`"),
                "{reason}"
            );
            assert!(reason.contains("`agent`"), "{reason}");
        }
        other => panic!("an unconfirmed mode must refuse: {other:?}"),
    }
    assert!(!methods(&seen).contains(&"session/prompt"), "{seen:?}");
}

#[tokio::test]
async fn a_session_without_a_mode_option_refuses_with_zero_prompts() {
    let (result, seen) = codex_one_shot(Mode::Absent, "max").await;
    match result {
        Err(HarnessError::Refused { reason }) => {
            assert!(reason.contains("advertises no mode option"), "{reason}");
        }
        other => panic!("a missing mode option must refuse: {other:?}"),
    }
    assert!(!methods(&seen).contains(&"session/prompt"), "{seen:?}");
}

/// Each profile admits its own adapter only: the Codex one-shot refuses
/// a claude-agent-acp identity before `session/new`.
#[tokio::test]
async fn the_codex_profile_admits_only_codex_acp() {
    struct ClaudeAsCodex;
    impl DynAgentBackend for ClaudeAsCodex {
        fn run_agent_boxed(
            &self,
            request: HarnessRequest,
        ) -> Pin<Box<dyn Future<Output = Result<HarnessEventStream, HarnessError>> + Send + '_>>
        {
            let (ours, theirs) = tokio::io::duplex(64 * 1024);
            let (client_read, client_write) = tokio::io::split(ours);
            let (peer_read, peer_write) = tokio::io::split(theirs);
            tokio::spawn(claude_peer(
                BufReader::new(peer_read),
                peer_write,
                Ending::Answer,
                Arc::new(Mutex::new(Vec::new())),
            ));
            let codex = meet_acp_one_shot("codex").expect("attested");
            Box::pin(async move { Ok(codex.drive(client_read, client_write, request)) })
        }
    }
    let codex = meet_acp_one_shot("codex").expect("attested");
    let refused = codex
        .run_over(&ClaudeAsCodex, ask("high"), Some(Duration::from_secs(5)))
        .await
        .expect_err("another adapter");
    assert!(
        refused
            .to_string()
            .contains("requires the audited @agentclientprotocol/codex-acp 1.13.1 profile"),
        "{refused}"
    );
}
