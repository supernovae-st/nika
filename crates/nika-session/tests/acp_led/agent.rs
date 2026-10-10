// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The agent: `claude-agent-acp` as a python3 script speaking ACP as claude-agent-acp 0.81.1
//! does, found on PATH through the registry's real `claude-code` row (its handshake probe
//! included). It mounts the one MCP server `session/new` names, with the bearer it was given,
//! and plays its scenario (`scenario.json` beside it): one list of steps per prompt — a call of
//! one of Nika's tools after asking its permission, a foreign tool whose permission it asks,
//! words, or a wait for `session/cancel`. Everything it sees is noted under `observed/`.

/// The script, installed as `bin/claude-agent-acp`.
pub(crate) const AGENT: &str = r#"#!/usr/bin/env python3
import json, os, sys, urllib.request

HERE = os.path.dirname(os.path.abspath(__file__))
OBSERVED = os.path.join(os.path.dirname(HERE), "observed")
with open(os.path.join(HERE, "scenario.json")) as scenario:
    TURNS = json.load(scenario)

def send(obj):
    sys.stdout.write(json.dumps(obj) + "\n")
    sys.stdout.flush()

def recv():
    line = sys.stdin.readline()
    return json.loads(line) if line else None

def note(name, value):
    with open(os.path.join(OBSERVED, name), "a") as out:
        out.write(json.dumps(value) + "\n")

init = recv()
if init is None:
    sys.exit(0)
send({"jsonrpc": "2.0", "id": init["id"], "result": {"protocolVersion": 1,
      "agentInfo": {"name": "@agentclientprotocol/claude-agent-acp", "title": "Claude Agent",
                    "version": "0.81.1"},
      "agentCapabilities": {"loadSession": True,
                            "mcpCapabilities": {"http": True, "sse": True}}}})
new = recv()
if new is None:
    sys.exit(0)
note("spawned", os.getpid())
servers = new["params"]["mcpServers"]
assert len(servers) == 1 and servers[0]["type"] == "http" and servers[0]["name"] == "nika", servers
server = servers[0]
send({"jsonrpc": "2.0", "id": new["id"], "result": {"sessionId": "s-led"}})

opener = urllib.request.build_opener(urllib.request.ProxyHandler({}))
sent = [0]

def mcp(method, params=None):
    sent[0] += 1
    headers = {h["name"]: h["value"] for h in server["headers"]}
    headers.update({"Content-Type": "application/json",
                    "Accept": "application/json, text/event-stream"})
    body = {"jsonrpc": "2.0", "id": sent[0], "method": method}
    if params is not None:
        body["params"] = params
    request = urllib.request.Request(server["url"], data=json.dumps(body).encode(),
                                     headers=headers, method="POST")
    with opener.open(request, timeout=60) as response:
        return json.loads(response.read())

def update(update):
    send({"jsonrpc": "2.0", "method": "session/update",
          "params": {"sessionId": "s-led", "update": update}})

def permission(id, tool):
    options = [{"optionId": "allow-once", "kind": "allow_once", "name": "Yes"},
               {"optionId": "allow-always", "kind": "allow_always", "name": "Always"},
               {"optionId": "reject", "kind": "reject_once", "name": "No"}]
    call = {"toolCallId": id, "status": "pending", "_meta": {"claudeCode": {"toolName": tool}}}
    update(dict(call, sessionUpdate="tool_call"))
    send({"jsonrpc": "2.0", "id": id, "method": "session/request_permission",
          "params": {"sessionId": "s-led", "toolCall": call, "options": options}})
    answer = recv()
    assert answer["id"] == id, answer
    return answer["result"]["outcome"]

mcp("initialize", {"protocolVersion": "2025-06-18", "capabilities": {},
                   "clientInfo": {"name": "fake-claude", "version": "0"}})
note("tools", [tool["name"] for tool in mcp("tools/list")["result"]["tools"]])

for turn in TURNS:
    prompt = recv()
    if prompt is None:
        break
    note("prompts", prompt["params"]["prompt"][0]["text"])
    stop = "end_turn"
    for step in turn:
        if "call" in step:
            outcome = permission(step["id"], "mcp__nika__" + step["call"])
            if outcome.get("optionId") != "allow-once":
                note("refused", step["call"])
                continue
            called = mcp("tools/call", {"name": step["call"], "arguments": step["args"],
                                        "_meta": {"claudecode/toolUseId": step["id"]}})
            result = called["result"]
            note("replies", {"call": step["call"], "text": result["content"][0]["text"],
                             "is_error": result.get("isError", False)})
            update({"sessionUpdate": "tool_call_update", "toolCallId": step["id"],
                    "status": "completed"})
        elif "foreign" in step:
            note("foreign", permission(step["id"], step["foreign"]))
            update({"sessionUpdate": "tool_call_update", "toolCallId": step["id"],
                    "status": "failed"})
        elif "say" in step:
            update({"sessionUpdate": "agent_message_chunk",
                    "content": {"type": "text", "text": step["say"]}})
        elif "await_cancel" in step:
            update({"sessionUpdate": "agent_thought_chunk",
                    "content": {"type": "text", "text": "Thinking it over."}})
            open(os.path.join(OBSERVED, "waiting"), "w").close()
            cancel = recv()
            note("cancels", cancel["method"])
            stop = "cancelled"
    send({"jsonrpc": "2.0", "id": prompt["id"], "result": {"stopReason": stop}})

line = recv()
while line is not None:
    note("after", line.get("method"))
    line = recv()
"#;
