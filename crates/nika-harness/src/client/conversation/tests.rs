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
