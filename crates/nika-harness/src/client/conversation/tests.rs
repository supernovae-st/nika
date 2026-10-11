// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Scripted conversations over a duplex pipe: the agent side is a script that asserts every
//! frame the client writes and answers as claude-agent-acp 0.81.1 does.

use std::path::PathBuf;
use std::time::Duration;

use nika_kernel::ai::harness::HarnessError;
use serde_json::{Value, json};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader, DuplexStream, ReadHalf, WriteHalf};

use super::{Opening, open};
use crate::authoring::acp::{self, Profile};
use crate::conversation::{
    ConversationSetup, ToolOffer, Transport, TurnEnd, TurnEvent, TurnStream,
};

const BEARER: &str = "5ec2e75ec2e75ec2e75ec2e75ec2e75ec2e75ec2e75ec2e75ec2e75ec2e75ec2";

/// The agent's end of the pipe.
struct Agent {
    reader: BufReader<ReadHalf<DuplexStream>>,
    writer: WriteHalf<DuplexStream>,
}

impl Agent {
    /// The client's next message, or `None` once the client closed the pipe.
    async fn read(&mut self) -> Option<Value> {
        let mut line = String::new();
        let read = self
            .reader
            .read_line(&mut line)
            .await
            .expect("the agent reads");
        (read > 0).then(|| serde_json::from_str(line.trim_end()).expect("one JSON message a line"))
    }

    async fn expect(&mut self, method: &str) -> Value {
        let message = self.read().await.expect("the client writes");
        assert_eq!(message["method"], method, "{message}");
        message
    }

    async fn send(&mut self, message: &Value) {
        let line = format!("{message}\n");
        self.writer
            .write_all(line.as_bytes())
            .await
            .expect("the agent writes");
    }

    async fn reply(&mut self, id: &Value, result: Value) {
        self.send(&json!({"jsonrpc": "2.0", "id": id, "result": result}))
            .await;
    }

    async fn update(&mut self, update: Value) {
        let params = json!({"sessionId": "s-1", "update": update});
        let note = json!({"jsonrpc": "2.0", "method": "session/update", "params": params});
        self.send(&note).await;
    }

    /// Ask a permission as claude-agent-acp does for `tool_call`, and read the outcome.
    async fn ask(&mut self, id: u64, tool_call: Value) -> Value {
        let options = json!([
            {"optionId": "allow-once", "kind": "allow_once", "name": "Yes"},
            {"optionId": "allow-with-updates", "kind": "allow_always", "name": "Yes, always"},
            {"optionId": "reject", "kind": "reject_once", "name": "No"}
        ]);
        let params = json!({"sessionId": "s-1", "toolCall": tool_call, "options": options});
        let asked = json!({"jsonrpc": "2.0", "id": id,
            "method": "session/request_permission", "params": params});
        self.send(&asked).await;
        let answer = self.read().await.expect("the client answers");
        assert_eq!(answer["id"], id, "{answer}");
        answer["result"]["outcome"].clone()
    }

    /// Ask a permission as codex-acp 1.13.1 does, with `options` and the ask's own `_meta`, and
    /// read the outcome.
    async fn ask_codex(&mut self, id: u64, tool_call: Value, options: Value, meta: Value) -> Value {
        let params = json!({"sessionId": "s-1", "toolCall": tool_call, "options": options,
            "_meta": meta});
        let asked = json!({"jsonrpc": "2.0", "id": id,
            "method": "session/request_permission", "params": params});
        self.send(&asked).await;
        let answer = self.read().await.expect("the client answers");
        assert_eq!(answer["id"], id, "{answer}");
        answer["result"]["outcome"].clone()
    }
}

fn pipe() -> (Agent, ReadHalf<DuplexStream>, WriteHalf<DuplexStream>) {
    let (client, agent) = tokio::io::duplex(1 << 16);
    let (client_read, client_write) = tokio::io::split(client);
    let (agent_read, agent_write) = tokio::io::split(agent);
    let agent = Agent {
        reader: BufReader::new(agent_read),
        writer: agent_write,
    };
    (agent, client_read, client_write)
}

fn offer() -> ToolOffer {
    let tools = vec!["candidate_read".to_owned(), "check".to_owned()];
    ToolOffer::new("nika", tools).with_http("http://127.0.0.1:4242/mcp", BEARER)
}

fn opening(offer: ToolOffer, setup: ConversationSetup) -> Opening {
    Opening {
        setup,
        offer,
        profile: Profile::ClaudeCode,
        cwd: PathBuf::from("/scratch/conversation"),
    }
}

fn initialized(http: bool, version: &str) -> Value {
    json!({"protocolVersion": 1,
        "agentInfo": {"name": "@agentclientprotocol/claude-agent-acp", "version": version},
        "agentCapabilities": {"loadSession": true, "mcpCapabilities": {"http": http, "sse": http}}})
}

/// `initialize` and `session/new`, answered; the client's `session/new` params.
async fn handshake(agent: &mut Agent, init: Value) -> Value {
    let hello = agent.expect("initialize").await;
    agent.reply(&hello["id"], init).await;
    let new = agent.expect("session/new").await;
    agent.reply(&new["id"], json!({"sessionId": "s-1"})).await;
    new["params"].clone()
}

/// Every beat of a turn until it ends; an error is a test failure.
async fn beats(mut turn: TurnStream) -> Vec<TurnEvent> {
    let mut seen = Vec::new();
    while let Some(beat) = turn.next_beat().await {
        seen.push(beat.expect("the turn goes through"));
    }
    seen
}

fn text(update: &str, text: &str) -> Value {
    json!({"sessionUpdate": update, "content": {"type": "text", "text": text}})
}

fn tool(update: &str, id: &str, status: &str, tool: Option<&str>) -> Value {
    let mut frame = json!({"sessionUpdate": update, "toolCallId": id, "status": status});
    if let Some(tool) = tool {
        frame["_meta"] = json!({"claudeCode": {"toolName": tool}});
    }
    frame
}

fn ended(beat: Option<&TurnEvent>) -> (TurnEnd, Value) {
    let Some(TurnEvent::Ended { end, record }) = beat else {
        panic!("a turn ends with its record: {beat:?}");
    };
    (*end, record.clone())
}

/// The agent side of the acceptance fixture: two turns on one session (the second recalls the
/// first, the agent keeping the history), one Nika tool asked and allowed, one foreign tool
/// asked and rejected, then a third turn stopped by exactly one `session/cancel`. Its result is
/// the client's `session/new` params.
async fn acceptance_agent(mut agent: Agent) -> Value {
    let new = handshake(&mut agent, initialized(true, "0.81.1")).await;
    let first = agent.expect("session/prompt").await;
    assert_eq!(first["params"]["sessionId"], "s-1");
    let said = first["params"]["prompt"][0]["text"]
        .as_str()
        .expect("text")
        .to_owned();
    agent
        .update(text("agent_thought_chunk", "Reading the candidate."))
        .await;
    let read_call = tool(
        "tool_call",
        "toolu_1",
        "pending",
        Some("mcp__nika__candidate_read"),
    );
    agent.update(read_call.clone()).await;
    let allowed = agent.ask(900, read_call).await;
    assert_eq!(
        allowed,
        json!({"outcome": "selected", "optionId": "allow-once"})
    );
    agent
        .update(tool("tool_call_update", "toolu_1", "completed", None))
        .await;
    agent
        .update(text(
            "agent_message_chunk",
            "The candidate is at revision 3.",
        ))
        .await;
    let bash = tool("tool_call", "toolu_2", "pending", Some("Bash"));
    agent.update(bash.clone()).await;
    let refused = agent.ask(901, bash).await;
    assert_eq!(
        refused,
        json!({"outcome": "selected", "optionId": "reject"})
    );
    agent
        .update(tool("tool_call_update", "toolu_2", "failed", None))
        .await;
    agent
        .reply(&first["id"], json!({"stopReason": "end_turn"}))
        .await;

    let second = agent.expect("session/prompt").await;
    assert_eq!(second["params"]["sessionId"], "s-1", "the same session");
    assert_ne!(second["id"], first["id"]);
    let recalled = format!("You first asked: {said}");
    agent.update(text("agent_message_chunk", &recalled)).await;
    agent
        .reply(&second["id"], json!({"stopReason": "end_turn"}))
        .await;

    let third = agent.expect("session/prompt").await;
    agent
        .update(text("agent_thought_chunk", "Listing folders."))
        .await;
    let cancel = agent.expect("session/cancel").await;
    assert_eq!(cancel["params"], json!({"sessionId": "s-1"}));
    assert!(cancel.get("id").is_none(), "a notification: {cancel}");
    agent
        .reply(&third["id"], json!({"stopReason": "cancelled"}))
        .await;
    assert_eq!(
        agent.read().await,
        None,
        "one cancel, then the handle closed the pipe"
    );
    new
}

/// The first turn's beats before its end, as the acceptance agent sends them.
fn first_turn_beats() -> [TurnEvent; 8] {
    let call = |id: &str, status: &str, nika: Option<&str>| TurnEvent::Tool {
        tool_call_id: id.to_owned(),
        status: Some(status.to_owned()),
        nika_tool: nika.map(str::to_owned),
    };
    [
        TurnEvent::Thought {
            text: "Reading the candidate.".to_owned(),
        },
        call("toolu_1", "pending", Some("candidate_read")),
        TurnEvent::Permission {
            tool: Some("mcp__nika__candidate_read".to_owned()),
            nika_tool: Some("candidate_read".to_owned()),
            allowed: true,
        },
        call("toolu_1", "completed", None),
        TurnEvent::Answer {
            text: "The candidate is at revision 3.".to_owned(),
        },
        call("toolu_2", "pending", None),
        TurnEvent::Permission {
            tool: Some("Bash".to_owned()),
            nika_tool: None,
            allowed: false,
        },
        call("toolu_2", "failed", None),
    ]
}

/// The first turn's record: its permissions decided and recorded, no Stop, and the activity of
/// its frames, tool frames counted as expected activity.
fn assert_first_record(record: &Value) {
    assert_eq!(record["turn"], 1);
    assert_eq!(
        record["permissions"],
        json!({"allowed": 1, "denied": ["Bash"]})
    );
    assert_eq!(
        record["stop"],
        json!({"requested": 0, "cancel_sent": false})
    );
    let frames = json!({"answer": 1, "thought": 1, "usage": 0, "status": 0, "other_update": 4,
        "client_request": 2, "prompt_result": 1, "prompt_error": 0});
    assert_eq!(record["activity"]["session"]["frames"], frames);
    assert_eq!(
        record["activity"]["session"]["last_update"]["kind"],
        "tool_update"
    );
    assert_eq!(record["activity"]["ended_by"], "completed");
}

/// Every beat of a turn, errors included.
async fn all_beats(mut turn: TurnStream) -> Vec<Result<TurnEvent, HarnessError>> {
    let mut seen = Vec::new();
    while let Some(beat) = turn.next_beat().await {
        seen.push(beat);
    }
    seen
}

/// The acceptance fixture: two prompts on one session (the agent keeps the history), one Nika
/// tool allowed once and its result relayed into the answer, a foreign tool rejected and
/// recorded, a prompt sent mid-turn refused, and Stop sending `session/cancel` exactly once.
#[tokio::test]
async fn one_session_serves_every_turn_with_nikas_tools_alone() {
    let (agent, read, write) = pipe();
    let script = tokio::spawn(acceptance_agent(agent));
    let setup = ConversationSetup::new();
    let conversation = open(read, write, opening(offer(), setup), ())
        .await
        .expect("opens");
    assert_eq!(conversation.transport(), Transport::Http);
    assert_eq!(conversation.record()["transport"]["chosen"], "http");

    let turn = conversation.prompt("Find the latest invoices.");
    let first = beats(turn.expect("a turn")).await;
    let expected = first_turn_beats();
    assert_eq!(first.len(), expected.len() + 1, "{first:?}");
    assert_eq!(first[..expected.len()], expected[..]);
    let (end, record) = ended(first.last());
    assert_eq!(end, TurnEnd::EndTurn);
    assert_first_record(&record);

    let second = conversation.prompt("Which one did I ask for first?");
    let early = conversation.prompt("Too soon");
    let refused = all_beats(early.expect("refused on its own stream")).await;
    let busy = |beat: &Result<TurnEvent, HarnessError>| match beat {
        Err(HarnessError::Session { reason }) => reason.contains("already in flight"),
        _ => false,
    };
    assert!(refused.len() == 1 && busy(&refused[0]), "{refused:?}");
    let second = beats(second.expect("a turn")).await;
    let recalled = "You first asked: Find the latest invoices.".to_owned();
    assert_eq!(second[0], TurnEvent::Answer { text: recalled });
    let (end, record) = ended(second.get(1));
    assert_eq!((end, record["turn"].clone()), (TurnEnd::EndTurn, json!(2)));

    let mut third = conversation.prompt("Go on.").expect("a turn");
    let thought = third.next_beat().await.expect("a beat").expect("a thought");
    let listing = TurnEvent::Thought {
        text: "Listing folders.".to_owned(),
    };
    assert_eq!(thought, listing);
    conversation.stop();
    conversation.stop();
    let rest = beats(third).await;
    let (end, record) = ended(rest.first());
    assert_eq!((rest.len(), end), (1, TurnEnd::Cancelled));
    assert_eq!(record["stop"], json!({"requested": 2, "cancel_sent": true}));
    assert_eq!(record["end"], "cancelled");
    drop(conversation);

    let new = script.await.expect("the script ends");
    let header = json!({"name": "Authorization", "value": format!("Bearer {BEARER}")});
    let mounted = json!([{"type": "http", "name": "nika", "url": "http://127.0.0.1:4242/mcp",
        "headers": [header]}]);
    assert_eq!(new["mcpServers"], mounted, "exactly Nika's server");
    assert_eq!(new["_meta"], acp::conversation_profile());
    assert_eq!(new["cwd"], "/scratch/conversation");
}

/// No offered endpoint is one the agent can mount: refused after `initialize`, before any
/// session, and nothing falls back.
#[tokio::test]
async fn an_agent_without_http_mcp_is_refused_before_any_session() {
    let (mut agent, read, write) = pipe();
    let script = tokio::spawn(async move {
        let hello = agent.expect("initialize").await;
        agent
            .reply(&hello["id"], initialized(false, "0.81.1"))
            .await;
        agent.read().await
    });
    let opened = open(read, write, opening(offer(), ConversationSetup::new()), ()).await;
    let Err(HarnessError::Refused { reason }) = opened else {
        panic!("refused: {opened:?}");
    };
    assert!(reason.contains("`mcpCapabilities.http`"), "{reason}");
    assert_eq!(
        script.await.expect("the script ends"),
        None,
        "no session/new was sent"
    );
}

/// The capability decides, and the record says so: an agent without HTTP mounts the offered
/// stdio bridge.
#[tokio::test]
async fn an_agent_without_http_mcp_mounts_the_offered_stdio_bridge() {
    let (mut agent, read, write) = pipe();
    let script =
        tokio::spawn(async move { handshake(&mut agent, initialized(false, "0.81.1")).await });
    let bridge = offer().with_stdio("nika", vec!["mcp".to_owned()], Vec::new());
    let conversation = open(read, write, opening(bridge, ConversationSetup::new()), ())
        .await
        .expect("opens");
    assert_eq!(conversation.transport(), Transport::Stdio);
    let transport = &conversation.record()["transport"];
    assert_eq!(transport["chosen"], "stdio");
    assert_eq!(
        transport["advertised"],
        json!({"http": false, "sse": false})
    );
    let new = script.await.expect("the script ends");
    let stdio = json!([{"name": "nika", "command": "nika", "args": ["mcp"], "env": []}]);
    assert_eq!(new["mcpServers"], stdio);
}

/// The audited identity is admitted exactly: another adapter version opens nothing.
#[tokio::test]
async fn an_unaudited_adapter_version_is_refused_before_any_session() {
    let (mut agent, read, write) = pipe();
    let script = tokio::spawn(async move {
        let hello = agent.expect("initialize").await;
        agent.reply(&hello["id"], initialized(true, "0.82.0")).await;
        agent.read().await
    });
    let opened = open(read, write, opening(offer(), ConversationSetup::new()), ()).await;
    let Err(HarnessError::Refused { reason }) = opened else {
        panic!("refused: {opened:?}");
    };
    assert!(
        reason.contains("ACP conversation requires the audited"),
        "{reason}"
    );
    assert_eq!(script.await.expect("the script ends"), None);
}

/// An open tool call holds the silence allowance (its time is Nika's own); a turn silent for a
/// whole allowance with none open ends the conversation, and later prompts are refused.
#[tokio::test(start_paused = true)]
async fn an_open_tool_call_holds_the_silence_allowance() {
    let (mut agent, read, write) = pipe();
    let script = tokio::spawn(async move {
        handshake(&mut agent, initialized(true, "0.81.1")).await;
        let first = agent.expect("session/prompt").await;
        let call = tool("tool_call", "toolu_9", "pending", Some("mcp__nika__check"));
        agent.update(call.clone()).await;
        let allowed = agent.ask(910, call).await;
        assert_eq!(allowed["optionId"], "allow-once");
        tokio::time::sleep(Duration::from_secs(10)).await;
        agent
            .update(tool("tool_call_update", "toolu_9", "completed", None))
            .await;
        agent.update(text("agent_message_chunk", "Clean.")).await;
        agent
            .reply(&first["id"], json!({"stopReason": "end_turn"}))
            .await;
        agent.expect("session/prompt").await;
        agent.read().await
    });
    let setup = ConversationSetup::new().with_allowance(Duration::from_secs(1));
    let conversation = open(read, write, opening(offer(), setup), ())
        .await
        .expect("opens");
    let first = beats(conversation.prompt("Check it.").expect("a turn")).await;
    assert_eq!(
        first.get(3),
        Some(&TurnEvent::Answer {
            text: "Clean.".to_owned()
        })
    );
    assert_eq!(ended(first.last()).0, TurnEnd::EndTurn);
    let mut second = conversation.prompt("And now?").expect("a turn");
    let silent = second.next_beat().await.expect("the turn fails");
    let Err(HarnessError::Session { reason }) = silent else {
        panic!("a silent turn fails: {silent:?}");
    };
    assert!(reason.contains("silence allowance"), "{reason}");
    assert_eq!(
        script.await.expect("the script ends"),
        None,
        "the conversation ended"
    );
    let later = conversation.prompt("Hello?");
    assert!(
        matches!(later, Err(HarnessError::Session { .. })),
        "{later:?}"
    );
}

fn codex_offer() -> ToolOffer {
    let tools = ["candidate_read", "candidate_write", "knowledge"].map(str::to_owned);
    ToolOffer::new("nika", tools.to_vec()).with_http("http://127.0.0.1:4242/mcp", BEARER)
}

/// codex-acp 1.13.1's `mode` option (`AgentMode`); the profile applies `read-only`.
fn codex_modes(current: &str) -> Value {
    let modes = ["read-only", "agent", "agent-full-access"];
    let options: Vec<Value> = (modes.iter())
        .map(|mode| json!({"value": mode, "name": mode}))
        .collect();
    json!([{"id": "mode", "name": "Mode", "category": "mode", "type": "select",
        "currentValue": current, "options": options}])
}

/// `initialize`, `session/new` (no server: Codex mounts Nika's from its configuration) and the
/// `read-only` mode, answered as codex-acp 1.13.1 does.
async fn codex_handshake(agent: &mut Agent) {
    let hello = agent.expect("initialize").await;
    let info = json!({"name": "@agentclientprotocol/codex-acp", "title": "Codex",
        "version": "1.13.1"});
    let init = json!({"protocolVersion": 1, "agentInfo": info,
        "agentCapabilities": {"loadSession": true}});
    agent.reply(&hello["id"], init).await;
    let new = agent.expect("session/new").await;
    assert_eq!(new["params"]["mcpServers"], json!([]), "{new}");
    let configured = json!({"sessionId": "s-1", "configOptions": codex_modes("agent")});
    agent.reply(&new["id"], configured).await;
    let mode = agent.expect("session/set_config_option").await;
    assert_eq!(mode["params"]["value"], "read-only", "{mode}");
    let applied = json!({"configOptions": codex_modes("read-only")});
    agent.reply(&mode["id"], applied).await;
}

/// codex-acp 1.13.1 announcing the MCP call Codex 0.156.1 routed (`createMcpToolCallUpdate`).
fn codex_call(id: &str, tool: &str) -> Value {
    json!({"sessionUpdate": "tool_call", "toolCallId": id, "kind": "execute",
        "title": format!("mcp.nika.{tool}"), "status": "in_progress",
        "rawInput": {"server": "nika", "tool": tool, "arguments": {"source": "workflow: draft"}},
        "_meta": {"is_mcp_tool_call": true}})
}

/// codex-acp 1.13.1 updating a call: running once allowed, then completed or failed with its
/// input and output (`completeItemEvent`, no `_meta`).
fn codex_status(id: &str, tool: &str, status: &str) -> Value {
    let mut update = json!({"sessionUpdate": "tool_call_update", "toolCallId": id,
        "status": status});
    if status != "in_progress" {
        let arguments = json!({"source": "workflow: draft"});
        update["rawInput"] = json!({"server": "nika", "tool": tool, "arguments": arguments});
        update["rawOutput"] = json!({"result": {"content": []}, "error": null});
    }
    update
}

/// The options codex-acp 1.13.1 offers for Codex's approval of an MCP call, Codex asking with
/// `persist: ["session", "always"]` (`buildMcpPermissionOptions`).
fn codex_mcp_options() -> Value {
    let option = |id: &str, name: &str, kind: &str, said: &str| {
        json!({"optionId": id, "name": name, "kind": kind,
            "_meta": {"permission": {"version": 1, "description": said}}})
    };
    json!([
        option(
            "allow_once",
            "Allow",
            "allow_once",
            "Run the tool and continue."
        ),
        option(
            "allow_session",
            "Allow for this session",
            "allow_always",
            "Run the tool and remember this choice for this session."
        ),
        option(
            "allow_always",
            "Always allow",
            "allow_always",
            "Run the tool and remember this choice for future tool calls."
        ),
        option("cancel", "Cancel", "reject_once", "Cancel this tool call")
    ])
}

/// The agent side of one codex-acp 1.13.1 turn, Codex 0.156.1 behind it: a read-only tool runs
/// unasked; before any other MCP call codex-acp asks about the call it announced by its id
/// alone, or in Codex's own words when two calls are open on the server; a command is asked as
/// a command (`commandToolCall`).
async fn codex_agent(mut agent: Agent) {
    codex_handshake(&mut agent).await;
    let turn = agent.expect("session/prompt").await;
    let by_id = |id: &str| json!({"toolCallId": id, "kind": "execute", "status": "pending"});
    let approval = || json!({"is_mcp_tool_approval": true});
    let allow_once = json!({"outcome": "selected", "optionId": "allow_once"});
    let cancel = json!({"outcome": "selected", "optionId": "cancel"});

    agent
        .update(codex_call("call_read", "candidate_read"))
        .await;
    let read = codex_status("call_read", "candidate_read", "completed");
    agent.update(read).await;
    let never_announced = by_id("call_unknown");
    let asked = (agent.ask_codex(900, never_announced, codex_mcp_options(), approval())).await;
    assert_eq!(asked, cancel, "an id never announced names nothing");

    agent
        .update(codex_call("call_write", "candidate_write"))
        .await;
    let asked = (agent.ask_codex(901, by_id("call_write"), codex_mcp_options(), approval())).await;
    assert_eq!(
        asked, allow_once,
        "Nika's own candidate_write, asked by its id"
    );
    let running = codex_status("call_write", "candidate_write", "in_progress");
    agent.update(running).await;
    let written = codex_status("call_write", "candidate_write", "completed");
    agent.update(written).await;
    let asked = (agent.ask_codex(902, by_id("call_write"), codex_mcp_options(), approval())).await;
    assert_eq!(asked, cancel, "a completed call is no longer open");

    agent.update(codex_call("call_know", "knowledge")).await;
    agent
        .update(codex_call("call_write_2", "candidate_write"))
        .await;
    let question = "Allow the nika MCP server to run tool \"candidate_write\"?";
    let said = json!([{"type": "content", "content": {"type": "text", "text": question}}]);
    let standalone = json!({"toolCallId": "elicitation:s-1:nika:1", "kind": "execute",
        "status": "pending", "title": "MCP tool call approval", "content": said,
        "rawInput": {"serverName": "nika", "description": question,
            "schema": {"type": "object", "properties": {}}}});
    let asked = (agent.ask_codex(903, standalone, codex_mcp_options(), approval())).await;
    assert_eq!(
        asked, allow_once,
        "Nika's own candidate_write, in Codex's words"
    );
    let finalized = json!({"sessionUpdate": "tool_call_update",
        "toolCallId": "elicitation:s-1:nika:1", "status": "completed",
        "title": "MCP tool call approval", "content": said, "rawOutput": {"action": "accept"}});
    agent.update(finalized).await;
    let written = codex_status("call_write_2", "candidate_write", "completed");
    agent.update(written).await;
    let known = codex_status("call_know", "knowledge", "completed");
    agent.update(known).await;

    agent.update(codex_call("call_trial", "trial")).await;
    let asked = (agent.ask_codex(904, by_id("call_trial"), codex_mcp_options(), approval())).await;
    assert_eq!(asked, cancel, "a tool Nika does not offer");
    agent
        .update(codex_status("call_trial", "trial", "failed"))
        .await;

    let command = json!({"toolCallId": "call_exec", "name": "exec_command", "kind": "execute",
        "status": "pending", "title": "Run command",
        "rawInput": {"command": "rm -rf build", "cwd": "/scratch/conversation"}});
    let decisions = json!([
        {"optionId": "allow_once", "name": "Yes, proceed", "kind": "allow_once"},
        {"optionId": "decline", "name": "No, continue without running it", "kind": "reject_once"},
        {"optionId": "cancel", "name": "No, and tell Codex what to do differently",
            "kind": "reject_once"}
    ]);
    let run = json!({"permission": {"version": 1, "title": "Run command?"}});
    let asked = agent.ask_codex(905, command, decisions, run).await;
    assert_eq!(asked, json!({"outcome": "selected", "optionId": "decline"}));

    agent
        .update(text("agent_message_chunk", "The draft is saved."))
        .await;
    agent
        .reply(&turn["id"], json!({"stopReason": "end_turn"}))
        .await;
    assert_eq!(agent.read().await, None, "the handle closed the pipe");
}

/// The tool frames and decisions of a turn, as the turn told them.
type Told<'a> = (
    Vec<(&'a str, &'a str, Option<&'a str>)>,
    Vec<(Option<&'a str>, Option<&'a str>, bool)>,
);

fn told(beats: &[TurnEvent]) -> Told<'_> {
    let (mut tools, mut decided) = (Vec::new(), Vec::new());
    for beat in beats {
        match beat {
            TurnEvent::Tool {
                tool_call_id,
                status,
                nika_tool,
            } => tools.push((
                tool_call_id.as_str(),
                status.as_deref().unwrap_or_default(),
                nika_tool.as_deref(),
            )),
            TurnEvent::Permission {
                tool,
                nika_tool,
                allowed,
            } => decided.push((tool.as_deref(), nika_tool.as_deref(), *allowed)),
            _ => {}
        }
    }
    (tools, decided)
}

/// codex-acp 1.13.1 never names the MCP call it asks about: it announces the call with the
/// server and tool Codex routed it to, then asks by the call's id alone (or in Codex's words).
/// Nika's own `candidate_write` is allowed once either way; an id never announced or no longer
/// open, a tool Nika does not offer and a command are rejected; each decision is told to the
/// turn and recorded.
#[tokio::test]
async fn codex_acp_asks_by_call_id_and_nikas_own_tool_is_allowed_once() {
    let (agent, read, write) = pipe();
    let script = tokio::spawn(codex_agent(agent));
    let codex = Opening {
        setup: ConversationSetup::new(),
        offer: codex_offer(),
        profile: Profile::Codex,
        cwd: PathBuf::from("/scratch/conversation"),
    };
    let conversation = open(read, write, codex, ()).await.expect("opens");
    assert_eq!(conversation.record()["adapter_version"], "1.13.1");
    let turn = conversation.prompt("Write the invoice workflow.");
    let seen = beats(turn.expect("a turn")).await;
    let (tools, decided) = told(&seen);
    let ours = Some("candidate_write");
    assert_eq!(
        decided,
        [
            (None, None, false),
            (None, ours, true),
            (None, None, false),
            (None, ours, true),
            (None, None, false),
            (Some("exec_command"), None, false),
        ]
    );
    assert_eq!(
        tools,
        [
            ("call_read", "in_progress", Some("candidate_read")),
            ("call_read", "completed", None),
            ("call_write", "in_progress", ours),
            ("call_write", "in_progress", None),
            ("call_write", "completed", None),
            ("call_know", "in_progress", Some("knowledge")),
            ("call_write_2", "in_progress", ours),
            ("elicitation:s-1:nika:1", "completed", None),
            ("call_write_2", "completed", None),
            ("call_know", "completed", None),
            ("call_trial", "in_progress", None),
            ("call_trial", "failed", None),
        ]
    );
    assert_eq!(
        seen.get(seen.len() - 2),
        Some(&TurnEvent::Answer {
            text: "The draft is saved.".to_owned()
        })
    );
    let (end, record) = ended(seen.last());
    assert_eq!(end, TurnEnd::EndTurn);
    let denied = ["(unnamed)", "(unnamed)", "(unnamed)", "exec_command"];
    assert_eq!(
        record["permissions"],
        json!({"allowed": 2, "denied": denied})
    );
    drop(conversation);
    script.await.expect("the script ends");
}
