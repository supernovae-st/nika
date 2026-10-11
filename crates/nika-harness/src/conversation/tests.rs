// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The conversation profile's pure halves: the audited options, the one server `session/new`
//! mounts, the transport chosen by capability, which tool is Nika's, the permission answers and
//! the records.

use serde_json::{Value, json};

use super::*;
use crate::authoring::acp;
use crate::wire::PermissionOptionIn;

const BEARER: &str = "0f1e2d3c4b5a69788796a5b4c3d2e1f00f1e2d3c4b5a69788796a5b4c3d2e1f0";

fn offer() -> ToolOffer {
    let tools = vec!["candidate_read".to_owned(), "check".to_owned()];
    ToolOffer::new("nika", tools).with_http("http://127.0.0.1:4242/mcp", BEARER)
}

fn bridge() -> ToolOffer {
    let args = vec!["mcp".to_owned(), "--session".to_owned()];
    let env = vec![("NIKA_MCP_TOKEN".to_owned(), BEARER.to_owned())];
    offer().with_stdio("nika", args, env)
}

/// The audited contract: a conversation sends the one-shot's closed surface with no turn
/// limit, and the one-shot profile itself is unchanged.
#[test]
fn the_conversation_profile_is_the_one_shot_surface_without_a_turn_limit() {
    let shown = json!({"type": "adaptive", "display": "summarized"});
    let conversation = json!({"claudeCode": {"options": {
        "tools": [], "mcpServers": {}, "strictMcpConfig": true,
        "settingSources": [], "plugins": [], "skills": [], "agents": {},
        "allowDangerouslySkipPermissions": false, "persistSession": false, "thinking": shown}}});
    assert_eq!(acp::conversation_profile(), conversation);
    let mut one_shot = conversation;
    one_shot["claudeCode"]["options"]["maxTurns"] = json!(1);
    assert_eq!(
        acp::profile(),
        one_shot,
        "the one-shot profile is unchanged"
    );
}

/// `session/new` mounts exactly Nika's server, over the chosen transport, for Claude Code;
/// Codex mounts it from its configuration, so its `session/new` names none.
#[test]
fn session_new_mounts_exactly_nikas_server() {
    let header = json!({"name": "Authorization", "value": format!("Bearer {BEARER}")});
    let http = json!({"type": "http", "name": "nika", "url": "http://127.0.0.1:4242/mcp",
        "headers": [header]});
    assert_eq!(
        mounts(&offer(), Transport::Http, Profile::ClaudeCode),
        vec![http]
    );
    let stdio = json!({"name": "nika", "command": "nika", "args": ["mcp", "--session"],
        "env": [{"name": "NIKA_MCP_TOKEN", "value": BEARER}]});
    assert_eq!(
        mounts(&bridge(), Transport::Stdio, Profile::ClaudeCode),
        vec![stdio]
    );
    assert_eq!(
        mounts(&offer(), Transport::Http, Profile::Codex),
        Vec::<Value>::new()
    );
}

/// Codex's configuration mounts the server with its bearer in an environment variable it
/// names: the bearer never rides the configuration a read-back prints.
#[test]
fn codex_reads_the_bearer_from_its_environment_never_its_configuration() {
    let (server, env) = codex_mount(&offer(), Transport::Http);
    let expected = json!({"url": "http://127.0.0.1:4242/mcp",
        "bearer_token_env_var": "NIKA_MCP_BEARER"});
    assert_eq!(server, expected);
    assert!(!server.to_string().contains(BEARER));
    assert_eq!(env, vec![("NIKA_MCP_BEARER".to_owned(), BEARER.to_owned())]);
    let (server, env) = codex_mount(&bridge(), Transport::Stdio);
    let expected = json!({"command": "nika", "args": ["mcp", "--session"],
        "env": {"NIKA_MCP_TOKEN": BEARER}});
    assert_eq!((server, env), (expected, Vec::new()));
}

/// HTTP when the agent advertises it and Nika offers it, the stdio bridge when offered
/// otherwise, and a refusal naming the missing capability when neither fits: never the
/// one-shot path.
#[test]
fn the_transport_follows_the_advertised_capability_and_never_falls_back() {
    let http_only = offer();
    let stdio_only = ToolOffer {
        http: None,
        ..bridge()
    };
    let cases = [
        (true, &http_only, Some(Transport::Http)),
        (true, &bridge(), Some(Transport::Http)),
        (false, &bridge(), Some(Transport::Stdio)),
        (true, &stdio_only, Some(Transport::Stdio)),
        (false, &stdio_only, Some(Transport::Stdio)),
        (false, &http_only, None),
    ];
    for (http, offered, expected) in cases {
        assert_eq!(
            choose(http, offered).ok(),
            expected,
            "http {http}: {offered:?}"
        );
    }
    let Err(HarnessError::Refused { reason }) = choose(false, &http_only) else {
        panic!("an agent without HTTP MCP is refused");
    };
    assert!(reason.contains("`mcpCapabilities.http`"), "{reason}");
    assert!(reason.contains("no fallback"), "{reason}");
    let nothing = ToolOffer::new("nika", Vec::new());
    let Err(HarnessError::Refused { reason }) = choose(true, &nothing) else {
        panic!("no offered server opens nothing");
    };
    assert!(
        reason.contains("no Nika tool server was offered"),
        "{reason}"
    );
    let init = json!({"agentCapabilities": {"mcpCapabilities": {"http": true, "sse": true}}});
    assert!(advertises_http(&init));
    assert!(!advertises_http(
        &json!({"agentCapabilities": {"mcpCapabilities": {}}})
    ));
    assert!(!advertises_http(
        &json!({"agentCapabilities": {"mcpCapabilities": {"http": "1"}}})
    ));
}

/// A call is Nika's only when it names one of the offered tools on Nika's server, by the name
/// Claude Code gives it (`mcp__nika__<tool>`); the server Claude Code names beside it must
/// agree.
#[test]
fn only_an_offered_tool_on_nikas_server_is_nikas() {
    let meta = |tool: &str| json!({"_meta": {"claudeCode": {"toolName": tool}}});
    let ours = |call: &Value| nika_tool(call, &offer());
    assert_eq!(
        ours(&meta("mcp__nika__candidate_read")).as_deref(),
        Some("candidate_read")
    );
    assert_eq!(
        ours(&json!({"name": "mcp__nika__check"})).as_deref(),
        Some("check")
    );
    let served_elsewhere = json!({"_meta": {"claudeCode": {"toolName": "mcp__nika__check",
        "mcpServer": {"name": "other", "source": "dynamic"}}}});
    let named_server = json!({"_meta": {"claudeCode": {"toolName": "mcp__nika__check",
        "mcpServer": {"name": "nika", "source": "dynamic"}}}});
    assert_eq!(ours(&named_server).as_deref(), Some("check"));
    for foreign in [
        meta("Bash"),
        meta("mcp__nika__trial"),
        meta("mcp__nikax__check"),
        meta("mcp__other__check"),
        meta("mcp__nika__check extra"),
        meta("mcp__nika__"),
        json!({"title": "mcp__nika__check"}),
        served_elsewhere,
    ] {
        assert_eq!(ours(&foreign), None, "{foreign}");
    }
    assert_eq!(asked_tool(&meta("Bash")).as_deref(), Some("Bash"));
    assert_eq!(
        asked_tool(&meta("rm -rf /")),
        None,
        "free text is never recorded"
    );
}

/// codex-acp 1.13.1's announcement of an MCP call (`createMcpToolCallUpdate`): the server and
/// tool Codex 0.156.1 routed it to.
fn codex_call(server: &str, tool: &str) -> Value {
    json!({"sessionUpdate": "tool_call", "toolCallId": "call_1", "kind": "execute",
        "title": format!("mcp.{server}.{tool}"), "status": "in_progress",
        "rawInput": {"server": server, "tool": tool, "arguments": {}},
        "_meta": {"is_mcp_tool_call": true}})
}

/// codex-acp 1.13.1's ask about an MCP call it could not tie to its announcement
/// (`buildMcpPermissionRequest`): the server and Codex's own question.
fn codex_question(server: &str, question: &str) -> Value {
    json!({"toolCallId": format!("elicitation:s-1:{server}:1"), "kind": "execute",
        "status": "pending", "title": "MCP tool call approval",
        "content": [{"type": "content", "content": {"type": "text", "text": question}}],
        "rawInput": {"serverName": server, "description": question,
            "schema": {"type": "object", "properties": {}}}})
}

/// codex-acp names an MCP call by the server and tool Codex routed it to, never by a name: the
/// call is Nika's only when that server is Nika's and the tool one it offers. A call that gives
/// a name is judged by the name alone.
#[test]
fn codex_acp_names_a_call_by_its_server_and_tool() {
    let ours = |call: &Value| nika_tool(call, &offer());
    assert_eq!(ours(&codex_call("nika", "check")).as_deref(), Some("check"));
    let asked = codex_question("nika", "Allow the nika MCP server to run tool \"check\"?");
    assert_eq!(ours(&asked).as_deref(), Some("check"));
    let mut unmarked = codex_call("nika", "check");
    unmarked["_meta"] = json!({});
    let mut named = codex_call("nika", "check");
    named["name"] = json!("Bash");
    for foreign in [
        codex_call("nika", "trial"),
        codex_call("other", "check"),
        codex_call("nikax", "check"),
        unmarked,
        named,
        codex_question("other", "Allow the other MCP server to run tool \"check\"?"),
        codex_question("nika", "Allow the other MCP server to run tool \"check\"?"),
        codex_question(
            "nika",
            "Allow the nika MCP server to run tool \"check\" now?",
        ),
        json!({"title": "MCP tool call approval", "rawInput": {"serverName": "nika"}}),
        json!({"toolCallId": "call_1", "kind": "execute", "status": "pending"}),
    ] {
        assert_eq!(ours(&foreign), None, "{foreign}");
    }
    assert!(names_tool(&codex_call("other", "check")) && names_tool(&json!({"name": "Bash"})));
    let completed = json!({"toolCallId": "call_1", "status": "completed",
        "rawInput": {"server": "nika", "tool": "check"}});
    assert!(
        !names_tool(&completed),
        "a status names no tool: {completed}"
    );
}

/// An ask refers to a call the agent announced only when it says nothing but the call's id, its
/// kind and its status, as codex-acp asks about the MCP call it announced; anything more is
/// judged by what it says.
#[test]
fn only_an_ask_by_id_alone_refers_to_an_announced_call() {
    let bare = json!({"toolCallId": "call_1", "kind": "execute", "status": "pending"});
    assert_eq!(referenced(&bare), Some("call_1"));
    assert_eq!(referenced(&json!({"toolCallId": "call_1"})), Some("call_1"));
    for said in [
        json!({"toolCallId": "call_1", "kind": "execute", "status": "pending", "title": "Run"}),
        json!({"toolCallId": "call_1", "_meta": {"claudeCode": {"toolName": "Bash"}}}),
        json!({"toolCallId": "call_1", "rawInput": {}}),
        json!({"kind": "execute", "status": "pending"}),
        json!({"toolCallId": 7}),
        json!("call_1"),
    ] {
        assert_eq!(referenced(&said), None, "{said}");
    }
}

fn options(kinds: &[(&str, &str)]) -> Vec<PermissionOptionIn> {
    let listed: Vec<Value> = (kinds.iter())
        .map(|(id, kind)| json!({"optionId": id, "kind": kind, "name": id}))
        .collect();
    serde_json::from_value(json!(listed)).expect("options")
}

/// Allowed is `allow_once`, never `allow_always`; denied is `reject_once`, so the agent goes
/// on without the tool; an option that is not offered cancels (fail-closed).
#[test]
fn permissions_are_answered_once_and_never_remembered() {
    let offered = options(&[
        ("allow-with-updates", "allow_always"),
        ("allow-once", "allow_once"),
        ("reject", "reject_once"),
        ("reject-always", "reject_always"),
    ]);
    let outcome = |allowed: bool, offered: &[PermissionOptionIn]| {
        serde_json::to_value(answer(allowed, offered)).expect("json")["outcome"].clone()
    };
    let selected = |id: &str| json!({"outcome": "selected", "optionId": id});
    assert_eq!(outcome(true, &offered), selected("allow-once"));
    assert_eq!(outcome(false, &offered), selected("reject"));
    let cancelled = json!({"outcome": "cancelled"});
    let always = options(&[("allow-with-updates", "allow_always")]);
    assert_eq!(outcome(true, &always), cancelled);
    assert_eq!(outcome(false, &always), cancelled);
}

/// The opening record says what was chosen and why, in closed facts: never the endpoint,
/// the bearer or adapter text.
#[test]
fn the_opening_record_tells_the_choice_and_never_the_secret() {
    let init = json!({"agentCapabilities": {"mcpCapabilities": {"http": true, "sse": false},
        "loadSession": true}});
    let selection = HarnessSelection::default();
    let record = opening(
        &init,
        &bridge(),
        Transport::Http,
        Profile::ClaudeCode,
        (&selection, None),
    );
    let expected = json!({"chosen": "http", "mounted_by": "session/new",
        "advertised": {"http": true, "sse": false}, "offered": ["http", "stdio"]});
    assert_eq!(record["transport"], expected);
    assert_eq!(record["server"], json!({"name": "nika", "tools": 2}));
    assert_eq!(
        (record["profile"].clone(), record["adapter_version"].clone()),
        (json!("conversation"), json!("0.81.1"))
    );
    assert_eq!(record["load_session"], true);
    let text = record.to_string();
    assert!(!text.contains(BEARER) && !text.contains("4242"), "{text}");
    let codex = opening(
        &json!({}),
        &offer(),
        Transport::Http,
        Profile::Codex,
        (&selection, None),
    );
    assert_eq!(codex["transport"]["advertised"], Value::Null);
    assert_eq!(
        codex["transport"]["mounted_by"],
        "codex configuration, read back at spawn"
    );
    assert_eq!(codex["adapter_version"], "1.13.1");
    assert!(
        !format!("{:?}", bridge()).contains(BEARER),
        "debug never shows the bearer"
    );
}

#[test]
fn each_stop_reason_has_one_closed_word() {
    let cases = [
        (Some("end_turn"), TurnEnd::EndTurn, "end_turn"),
        (Some("max_tokens"), TurnEnd::MaxTokens, "max_tokens"),
        (
            Some("max_turn_requests"),
            TurnEnd::MaxTurnRequests,
            "max_turn_requests",
        ),
        (Some("refusal"), TurnEnd::Refusal, "refusal"),
        (Some("cancelled"), TurnEnd::Cancelled, "cancelled"),
        (Some("paused"), TurnEnd::Other, "other"),
        (None, TurnEnd::Other, "other"),
    ];
    for (stop, end, word) in cases {
        assert_eq!((TurnEnd::of(stop), TurnEnd::of(stop).as_str()), (end, word));
    }
    assert_eq!(
        (Transport::Http.as_str(), Transport::Stdio.as_str()),
        ("http", "stdio")
    );
}
