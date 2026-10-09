// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The conversation relay end to end: the harness opens an ACP conversation with a real child
//! process (a python3 agent speaking ACP as claude-agent-acp 0.81.1 does), the agent mounts the
//! tool server `session/new` names, and calls Nika's tool over MCP with the bearer it was given;
//! the session's answer comes back into the agent's reply. A foreign tool is rejected, and the
//! second prompt rides the same agent session.

#![allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)]

use std::sync::{Arc, Mutex};
use std::time::Duration;

use nika_harness::{
    ConversationSetup, HarnessAdapter, SpawnedHarness, ToolOffer, Transport, TurnEnd, TurnEvent,
    TurnStream,
};
use nika_mcp::conversation::{SERVER_NAME, ToolServer};
use nika_session_change::tools::{SessionTools, ToolCall, ToolDef, ToolReply};
use serde_json::{Value, json};

/// The agent: it reads the server `session/new` mounts, initializes it, asks permission for
/// Nika's tool and calls it with its own tool-use id, then asks for a tool that is not Nika's.
const AGENT: &str = r#"
import json, sys, urllib.request

def send(obj):
    sys.stdout.write(json.dumps(obj) + "\n")
    sys.stdout.flush()

def recv():
    line = sys.stdin.readline()
    return json.loads(line) if line else None

def update(update):
    send({"jsonrpc": "2.0", "method": "session/update",
          "params": {"sessionId": "s-relay", "update": update}})

def ask(id, tool):
    options = [{"optionId": "allow-once", "kind": "allow_once", "name": "Yes"},
               {"optionId": "allow-with-updates", "kind": "allow_always", "name": "Always"},
               {"optionId": "reject", "kind": "reject_once", "name": "No"}]
    call = {"toolCallId": id, "status": "pending", "_meta": {"claudeCode": {"toolName": tool}}}
    update(dict(call, sessionUpdate="tool_call"))
    send({"jsonrpc": "2.0", "id": id, "method": "session/request_permission",
          "params": {"sessionId": "s-relay", "toolCall": call, "options": options}})
    answer = recv()
    assert answer["id"] == id, answer
    return answer["result"]["outcome"]

opener = urllib.request.build_opener(urllib.request.ProxyHandler({}))

def mcp(server, message):
    headers = {h["name"]: h["value"] for h in server["headers"]}
    headers.update({"Content-Type": "application/json",
                    "Accept": "application/json, text/event-stream"})
    body = json.dumps(message).encode()
    request = urllib.request.Request(server["url"], data=body, headers=headers, method="POST")
    with opener.open(request, timeout=20) as response:
        return json.loads(response.read())

init = recv()
send({"jsonrpc": "2.0", "id": init["id"], "result": {"protocolVersion": 1,
      "agentInfo": {"name": "@agentclientprotocol/claude-agent-acp", "version": "0.81.1"},
      "agentCapabilities": {"loadSession": True,
                            "mcpCapabilities": {"http": True, "sse": True}}}})
new = recv()
servers = new["params"]["mcpServers"]
assert len(servers) == 1 and servers[0]["type"] == "http", servers
server = servers[0]
send({"jsonrpc": "2.0", "id": new["id"], "result": {"sessionId": "s-relay"}})

first = recv()
asked = first["params"]["prompt"][0]["text"]
hello = mcp(server, {"jsonrpc": "2.0", "id": 1, "method": "initialize",
                     "params": {"protocolVersion": "2025-06-18", "capabilities": {},
                                "clientInfo": {"name": "fake-agent", "version": "0"}}})
assert hello["result"]["serverInfo"]["name"] == "nika", hello
listed = mcp(server, {"jsonrpc": "2.0", "id": 2, "method": "tools/list"})
names = [tool["name"] for tool in listed["result"]["tools"]]
assert ask("toolu_relay", "mcp__nika__" + names[0]) == {"outcome": "selected",
                                                        "optionId": "allow-once"}
called = mcp(server, {"jsonrpc": "2.0", "id": 3, "method": "tools/call",
                      "params": {"name": names[0], "arguments": {"window": 3},
                                 "_meta": {"claudecode/toolUseId": "toolu_relay"}}})
result = called["result"]["content"][0]["text"]
update({"sessionUpdate": "tool_call_update", "toolCallId": "toolu_relay", "status": "completed"})
update({"sessionUpdate": "agent_message_chunk", "content": {"type": "text", "text": result}})
assert ask("toolu_bash", "Bash") == {"outcome": "selected", "optionId": "reject"}
update({"sessionUpdate": "tool_call_update", "toolCallId": "toolu_bash", "status": "failed"})
send({"jsonrpc": "2.0", "id": first["id"], "result": {"stopReason": "end_turn"}})

second = recv()
assert second["params"]["sessionId"] == "s-relay", second
update({"sessionUpdate": "agent_message_chunk",
        "content": {"type": "text", "text": "You first asked: " + asked}})
send({"jsonrpc": "2.0", "id": second["id"], "result": {"stopReason": "end_turn"}})
assert recv() is None
"#;

/// A session serving one read-only tool, recording every call it receives.
#[derive(Default)]
struct Session {
    calls: Mutex<Vec<ToolCall>>,
}

impl SessionTools for Session {
    fn tools(&self) -> Vec<ToolDef> {
        let schema = json!({"type": "object", "properties": {"window": {"type": "integer"}}});
        vec![ToolDef::new(
            "candidate_read",
            "Read the candidate.",
            schema,
            true,
        )]
    }

    fn call(&self, call: ToolCall) -> ToolReply {
        let reply = ToolReply::ok(format!("revision 7 · {}", call.arguments));
        self.calls.lock().expect("calls").push(call);
        reply
    }
}

/// The agent script's directory, removed with the test.
struct Scratch(std::path::PathBuf);

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

async fn beats(mut turn: TurnStream) -> Vec<TurnEvent> {
    let mut seen = Vec::new();
    while let Some(beat) = turn.next_beat().await {
        seen.push(beat.expect("the turn goes through"));
    }
    seen
}

fn ended(beat: Option<&TurnEvent>) -> (TurnEnd, Value) {
    let Some(TurnEvent::Ended { end, record }) = beat else {
        panic!("a turn ends with its record: {beat:?}");
    };
    (*end, record.clone())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_conversation_relays_nikas_tool_through_the_real_server() {
    let session = Arc::new(Session::default());
    let server = ToolServer::start(Arc::clone(&session) as Arc<dyn SessionTools>).expect("binds");
    let offer = ToolOffer::new(SERVER_NAME, server.tool_names())
        .with_http(server.url().expect("url"), server.bearer());
    let closer = server.closer().expect("a closer");
    let serving = tokio::task::spawn_blocking(move || server.serve());

    let dir = std::env::temp_dir().join(format!("nika-mcp-relay-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("a scratch for the agent script");
    let scratch = Scratch(dir);
    let script = scratch.0.join("agent.py");
    std::fs::write(&script, AGENT).expect("the script is written");
    let adapter = HarnessAdapter::new("claude-code", "python3")
        .expect("an adapter id")
        .with_args(vec![script.to_string_lossy().into_owned()]);
    let setup = ConversationSetup::new().with_allowance(Duration::from_secs(30));
    let conversation = SpawnedHarness::new(adapter)
        .converse(setup, offer)
        .await
        .expect("the conversation opens");
    assert_eq!(conversation.transport(), Transport::Http);

    let first = beats(
        conversation
            .prompt("Show me the candidate.")
            .expect("a turn"),
    )
    .await;
    let answer = TurnEvent::Answer {
        text: "revision 7 · {\"window\":3}".to_owned(),
    };
    assert!(first.contains(&answer), "{first:?}");
    let decisions: Vec<(Option<String>, bool)> = (first.iter())
        .filter_map(|beat| match beat {
            TurnEvent::Permission {
                nika_tool, allowed, ..
            } => Some((nika_tool.clone(), *allowed)),
            _ => None,
        })
        .collect();
    let expected = vec![(Some("candidate_read".to_owned()), true), (None, false)];
    assert_eq!(decisions, expected);
    let (end, record) = ended(first.last());
    assert_eq!(end, TurnEnd::EndTurn);
    assert_eq!(
        record["permissions"],
        json!({"allowed": 1, "denied": ["Bash"]})
    );
    let relayed = ToolCall::new("candidate_read", json!({"window": 3})).with_meta("toolu_relay");
    assert_eq!(*session.calls.lock().expect("calls"), vec![relayed]);

    let second = beats(
        conversation
            .prompt("What did I ask first?")
            .expect("a turn"),
    )
    .await;
    let recalled = "You first asked: Show me the candidate.".to_owned();
    assert_eq!(second.first(), Some(&TurnEvent::Answer { text: recalled }));
    assert_eq!(ended(second.last()).0, TurnEnd::EndTurn);

    drop(conversation);
    closer.close();
    serving.await.expect("the server stops once closed");
    assert_eq!(
        session.calls.lock().expect("calls").len(),
        1,
        "one call, one relay"
    );
}
