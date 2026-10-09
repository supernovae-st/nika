// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

#![allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)]

use std::io::{Read, Write};
use std::net::TcpStream;
use std::sync::{Arc, Mutex};

use nika_session_change::tools::{SessionTools, ToolCall, ToolDef, ToolReply};
use serde_json::{Value, json};

use super::{ToolServer, bridge, dispatch};

/// A session with two tools that records every call it receives.
#[derive(Default)]
struct Recorder {
    calls: Mutex<Vec<ToolCall>>,
}

impl SessionTools for Recorder {
    fn tools(&self) -> Vec<ToolDef> {
        vec![
            ToolDef::new(
                "candidate_read",
                "Read the candidate.",
                json!({"type": "object"}),
                true,
            ),
            ToolDef::new("ask", "Ask the person.", json!({"type": "object"}), false),
        ]
    }

    fn call(&self, call: ToolCall) -> ToolReply {
        let reply = match call.name.as_str() {
            "candidate_read" => ToolReply::ok(format!("revision 3 · {}", call.arguments)),
            _ => ToolReply::ends_turn("asked: which folder?"),
        };
        self.calls.lock().expect("calls").push(call);
        reply
    }
}

fn request(id: u64, method: &str, params: &Value) -> Value {
    json!({"jsonrpc": "2.0", "id": id, "method": method, "params": params})
}

#[test]
fn initialize_names_the_nika_server_with_tools_only() {
    let tools = Recorder::default();
    let asked = request(1, "initialize", &json!({"protocolVersion": "2025-06-18"}));
    let reply = dispatch(&tools, &asked).expect("a request is answered");
    assert_eq!(reply["result"]["protocolVersion"], "2025-06-18");
    assert_eq!(reply["result"]["serverInfo"]["name"], "nika");
    assert_eq!(reply["result"]["capabilities"], json!({"tools": {}}));
    let cold = request(2, "initialize", &json!({"protocolVersion": "1999-01-01"}));
    let reply = dispatch(&tools, &cold).expect("answered");
    assert_eq!(
        reply["result"]["protocolVersion"],
        crate::protocol::PROTOCOL_VERSION
    );
}

#[test]
fn the_session_definitions_are_listed_as_mcp_tools() {
    let tools = Recorder::default();
    let reply = dispatch(&tools, &request(1, "tools/list", &json!({}))).expect("answered");
    let listed = reply["result"]["tools"].as_array().expect("a tool array");
    let expected = json!({"name": "candidate_read", "description": "Read the candidate.",
        "inputSchema": {"type": "object"}, "annotations": {"readOnlyHint": true}});
    assert_eq!(listed.len(), 2);
    assert_eq!(listed[0], expected);
    assert_eq!(listed[1]["annotations"]["readOnlyHint"], false);
}

/// The call reaches the session with its arguments and the tool-use id Claude Code names in
/// `_meta`; the reply is text, and a reply that ends the turn is returned like any other.
#[test]
fn a_listed_call_reaches_the_session_with_its_tool_use_id() {
    let tools = Recorder::default();
    let params = json!({"name": "candidate_read", "arguments": {"window": 2},
        "_meta": {"claudecode/toolUseId": "toolu_01"}});
    let reply = dispatch(&tools, &request(7, "tools/call", &params)).expect("answered");
    let expected = json!({"content": [{"type": "text", "text": "revision 3 · {\"window\":2}"}],
        "isError": false});
    assert_eq!(
        (reply["id"].clone(), reply["result"].clone()),
        (json!(7), expected)
    );
    let ask = json!({"name": "ask", "arguments": {}});
    let reply = dispatch(&tools, &request(8, "tools/call", &ask)).expect("answered");
    assert_eq!(
        reply["result"]["content"][0]["text"],
        "asked: which folder?"
    );
    assert_eq!(reply["result"]["isError"], false);
    let calls = tools.calls.lock().expect("calls").clone();
    let first = ToolCall::new("candidate_read", json!({"window": 2})).with_meta("toolu_01");
    assert_eq!(calls, vec![first, ToolCall::new("ask", json!({}))]);
}

/// A name the session does not list is a tool error the agent reads, and never reaches the
/// session; a malformed call is a protocol error.
#[test]
fn an_unlisted_tool_never_reaches_the_session() {
    let tools = Recorder::default();
    let params = json!({"name": "exec", "arguments": {"command": ["rm", "-rf", "/"]}});
    let reply = dispatch(&tools, &request(3, "tools/call", &params)).expect("answered");
    assert_eq!(reply["result"]["isError"], true);
    let text = reply["result"]["content"][0]["text"]
        .as_str()
        .expect("text");
    assert!(text.contains("`exec`"), "{text}");
    assert!(tools.calls.lock().expect("calls").is_empty());
    let reply = dispatch(&tools, &request(4, "tools/call", &json!({}))).expect("answered");
    assert_eq!(reply["error"]["code"], -32602);
}

#[test]
fn notifications_batches_and_unknown_methods_keep_the_wire_honest() {
    let tools = Recorder::default();
    let note = json!({"jsonrpc": "2.0", "method": "notifications/initialized"});
    assert_eq!(dispatch(&tools, &note), None);
    let batch = json!([request(1, "ping", &json!({}))]);
    let reply = dispatch(&tools, &batch).expect("a batch is refused, never silent");
    assert_eq!(
        (reply["error"]["code"].clone(), reply["id"].clone()),
        (json!(-32600), Value::Null)
    );
    let reply = dispatch(&tools, &json!({"jsonrpc": "2.0", "id": 5})).expect("answered");
    assert_eq!(reply["error"]["code"], -32600);
    let reply = dispatch(&tools, &request(6, "prompts/list", &json!({}))).expect("answered");
    assert_eq!(reply["error"]["code"], -32601);
    let reply = dispatch(&tools, &request(9, "ping", &json!({}))).expect("answered");
    assert_eq!(reply["result"], json!({}));
}

/// Each server mints its own 256-bit bearer, and no debug rendering shows it.
#[test]
fn each_server_mints_its_own_bearer_and_never_shows_it() {
    let first = ToolServer::start(Arc::new(Recorder::default())).expect("binds");
    let second = ToolServer::start(Arc::new(Recorder::default())).expect("binds");
    for server in [&first, &second] {
        let bearer = server.bearer();
        assert_eq!(bearer.len(), 64);
        assert!(
            bearer
                .chars()
                .all(|c| c.is_ascii_hexdigit() && !c.is_ascii_uppercase())
        );
        assert!(!format!("{server:?}").contains(bearer));
        let url = server.url().expect("an address");
        assert!(
            url.starts_with("http://127.0.0.1:") && url.ends_with("/mcp"),
            "{url}"
        );
    }
    assert_ne!(first.bearer(), second.bearer());
    assert_eq!(first.tool_names(), vec!["candidate_read", "ask"]);
}

fn post(url: &str, bearer: Option<&str>, body: &str) -> String {
    let authority = url.trim_start_matches("http://").trim_end_matches("/mcp");
    let mut stream = TcpStream::connect(authority).expect("connects");
    let auth = bearer
        .map(|b| format!("Authorization: Bearer {b}\r\n"))
        .unwrap_or_default();
    write!(
        stream,
        "POST /mcp HTTP/1.1\r\nHost: {authority}\r\n{auth}Content-Length: {}\r\n\r\n{body}",
        body.len()
    )
    .expect("sends");
    let mut reply = String::new();
    stream.read_to_string(&mut reply).expect("reads");
    reply
}

/// The real socket: a request without the bearer is refused before the session sees it, the
/// bearer's request is answered, and closing ends the serving loop.
#[test]
fn the_server_requires_its_bearer_and_stops_when_closed() {
    let tools = Arc::new(Recorder::default());
    let server = ToolServer::start(Arc::clone(&tools) as Arc<dyn SessionTools>).expect("binds");
    let (url, bearer) = (server.url().expect("url"), server.bearer().to_owned());
    let closer = server.closer().expect("a closer");
    std::thread::scope(|scope| {
        let serving = scope.spawn(|| server.serve());
        let call = r#"{"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":"ask"}}"#;
        assert!(post(&url, None, call).starts_with("HTTP/1.1 401"));
        assert!(post(&url, Some("0".repeat(64).as_str()), call).starts_with("HTTP/1.1 401"));
        assert!(tools.calls.lock().expect("calls").is_empty());
        let answered = post(&url, Some(&bearer), call);
        assert!(answered.starts_with("HTTP/1.1 200"), "{answered}");
        assert!(answered.contains("asked: which folder?"), "{answered}");
        closer.close();
        serving
            .join()
            .expect("the serving loop returns once closed");
    });
    assert_eq!(tools.calls.lock().expect("calls").len(), 1);
}

/// A dropped closer stops the server too: a session that forgets to close leaks no loop.
#[test]
fn dropping_the_closer_ends_the_serving_loop() {
    let server = ToolServer::start(Arc::new(Recorder::default())).expect("binds");
    let closer = server.closer().expect("a closer");
    std::thread::scope(|scope| {
        let serving = scope.spawn(|| server.serve());
        drop(closer);
        serving.join().expect("returns");
    });
}

/// The stdio bridge relays each line to the server with the bearer: a request's reply comes
/// back as one line, a notification gets none, a blank line is skipped and a malformed line
/// gets the server's parse error.
#[test]
fn the_bridge_relays_a_stdio_session_to_the_server() {
    let server = ToolServer::start(Arc::new(Recorder::default())).expect("binds");
    let (url, bearer) = (server.url().expect("url"), server.bearer().to_owned());
    let closer = server.closer().expect("a closer");
    let input = concat!(
        r#"{"jsonrpc":"2.0","id":1,"method":"tools/list"}"#,
        "\n\n",
        r#"{"jsonrpc":"2.0","method":"notifications/initialized"}"#,
        "\nnot json\n",
        r#"{"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":"ask"}}"#,
        "\n"
    );
    let mut output = Vec::new();
    std::thread::scope(|scope| {
        let serving = scope.spawn(|| server.serve());
        bridge(&url, &bearer, input.as_bytes(), &mut output).expect("relays");
        let refused = bridge(
            &url,
            "wrong",
            &br#"{"jsonrpc":"2.0","id":3,"method":"ping"}"#[..],
            Vec::new(),
        );
        assert!(refused.is_ok(), "a refusal is answered, not fatal");
        closer.close();
        serving.join().expect("returns");
    });
    let lines: Vec<Value> = String::from_utf8(output)
        .expect("utf8")
        .lines()
        .map(|line| serde_json::from_str(line).expect("one JSON reply per line"))
        .collect();
    assert_eq!(lines.len(), 3, "{lines:?}");
    assert_eq!(lines[0]["result"]["tools"][1]["name"], "ask");
    assert_eq!(lines[1]["error"]["code"], -32700);
    assert_eq!(
        lines[2]["result"]["content"][0]["text"],
        "asked: which folder?"
    );
}

#[test]
fn the_bridge_reaches_only_a_loopback_endpoint() {
    for url in [
        "https://127.0.0.1:1/mcp",
        "http://example.com:80/mcp",
        "127.0.0.1:1",
    ] {
        let refused = bridge(url, "x", &b"{}\n"[..], Vec::new());
        assert!(refused.is_err(), "{url}");
    }
}
