// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! MCP over JSON-RPC 2.0 — the PURE message dispatcher.
//!
//! [`handle`] maps one incoming message to its reply (or `None` for a
//! notification). It is side-effect-free: tool execution ([`crate::tools`]) is
//! itself pure static analysis, so the whole server is a function — the stdio
//! pump in [`crate::run_stdio`] is the only I/O. That keeps the protocol
//! unit-testable end-to-end without spawning a process or a pipe.

use serde_json::{Value, json};

use crate::tools;

/// The MCP protocol revision this server speaks. (The client sends its own in
/// `initialize`; we answer with ours — a client negotiates down if it must.)
pub(crate) const PROTOCOL_VERSION: &str = "2025-06-18";
/// The advertised server name (`serverInfo.name`).
pub(crate) const SERVER_NAME: &str = "nika";

/// Dispatch one parsed JSON-RPC 2.0 message.
///
/// Returns `Some(reply)` for a request (a message carrying an `id`), `None` for
/// a notification (no `id` · JSON-RPC forbids replying). Unknown methods get a
/// `-32601` error; a `tools/call` for an unknown tool is a successful reply
/// with `isError: true` (a TOOL error, not a PROTOCOL error · per MCP).
#[must_use]
pub fn handle(msg: &Value) -> Option<Value> {
    // A notification has no `id` — never reply (JSON-RPC 2.0 §4.1).
    let id = msg.get("id")?;
    let method = msg
        .get("method")
        .and_then(Value::as_str)
        .unwrap_or_default();
    Some(match method {
        "initialize" => ok(id, initialize_result()),
        "tools/list" => ok(id, json!({ "tools": tools::catalog() })),
        "tools/call" => tools_call(id, msg.get("params")),
        "ping" => ok(id, json!({})),
        other => err(id, -32601, &format!("method not found: {other}")),
    })
}

/// The `initialize` result — protocol version · the tools capability · identity.
fn initialize_result() -> Value {
    json!({
        "protocolVersion": PROTOCOL_VERSION,
        "capabilities": { "tools": {} },
        "serverInfo": { "name": SERVER_NAME, "version": env!("CARGO_PKG_VERSION") },
    })
}

/// `tools/call` → run the named tool over its `arguments`. A missing/!object
/// params block is a `-32602` protocol error; a tool failure is a normal reply
/// with `isError: true` so the model SEES the error and can adapt (MCP law).
fn tools_call(id: &Value, params: Option<&Value>) -> Value {
    let Some(params) = params else {
        return err(
            id,
            -32602,
            "invalid params: `tools/call` requires {name, arguments}",
        );
    };
    let name = params
        .get("name")
        .and_then(Value::as_str)
        .unwrap_or_default();
    let args = params
        .get("arguments")
        .cloned()
        .unwrap_or_else(|| json!({}));
    match tools::execute(name, &args) {
        Ok(text) => ok(id, tool_content(&text, false)),
        Err(text) => ok(id, tool_content(&text, true)),
    }
}

/// An MCP tool result: one text content block + the `isError` flag.
fn tool_content(text: &str, is_error: bool) -> Value {
    json!({ "content": [{ "type": "text", "text": text }], "isError": is_error })
}

/// A JSON-RPC 2.0 success reply.
fn ok(id: &Value, result: Value) -> Value {
    reply(id, "result", result)
}

/// A JSON-RPC 2.0 error reply.
fn err(id: &Value, code: i64, message: &str) -> Value {
    reply(id, "error", json!({ "code": code, "message": message }))
}

/// Build the JSON-RPC 2.0 envelope · borrows `id` (it lives in the request),
/// MOVES `payload` into the `result`/`error` slot (consumed · `json!` would
/// only borrow it, so we insert directly — no needless clone).
fn reply(id: &Value, slot: &'static str, payload: Value) -> Value {
    let mut m = serde_json::Map::with_capacity(3);
    m.insert("jsonrpc".to_owned(), Value::from("2.0"));
    m.insert("id".to_owned(), id.clone());
    m.insert(slot.to_owned(), payload);
    Value::Object(m)
}

#[cfg(test)]
#[allow(clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    #[test]
    fn initialize_advertises_tools_and_identity() {
        let resp =
            handle(&json!({ "jsonrpc": "2.0", "id": 1, "method": "initialize", "params": {} }))
                .expect("a request gets a reply");
        assert_eq!(resp["id"], 1);
        assert_eq!(resp["result"]["protocolVersion"], PROTOCOL_VERSION);
        assert_eq!(resp["result"]["serverInfo"]["name"], "nika");
        assert!(resp["result"]["capabilities"]["tools"].is_object());
    }

    #[test]
    fn a_notification_gets_no_reply() {
        // `notifications/initialized` carries no id → silence (JSON-RPC 2.0).
        assert!(
            handle(&json!({ "jsonrpc": "2.0", "method": "notifications/initialized" })).is_none()
        );
    }

    #[test]
    fn tools_list_returns_the_catalog() {
        let resp =
            handle(&json!({ "jsonrpc": "2.0", "id": 2, "method": "tools/list" })).expect("reply");
        let tools = resp["result"]["tools"].as_array().expect("array");
        assert_eq!(tools.len(), 2);
    }

    #[test]
    fn tools_call_runs_the_tool() {
        let wf = "nika: v1\nworkflow: t\ntasks:\n  - id: a\n    exec: { command: \"echo hi\" }\n";
        let resp = handle(&json!({
            "jsonrpc": "2.0", "id": 3, "method": "tools/call",
            "params": { "name": "nika_check", "arguments": { "workflow": wf } }
        }))
        .expect("reply");
        assert_eq!(resp["result"]["isError"], false);
        assert!(
            resp["result"]["content"][0]["text"]
                .as_str()
                .expect("text")
                .contains("clean")
        );
    }

    #[test]
    fn tools_call_tool_error_is_iserror_not_protocol_error() {
        let resp = handle(&json!({
            "jsonrpc": "2.0", "id": 4, "method": "tools/call",
            "params": { "name": "nika_explain", "arguments": { "code": "NIKA-GHOST-999" } }
        }))
        .expect("reply");
        // A tool failure is a SUCCESSFUL reply with isError — the model sees it.
        assert!(resp.get("error").is_none(), "not a protocol error");
        assert_eq!(resp["result"]["isError"], true);
    }

    #[test]
    fn unknown_method_is_method_not_found() {
        let resp =
            handle(&json!({ "jsonrpc": "2.0", "id": 5, "method": "frobnicate" })).expect("reply");
        assert_eq!(resp["error"]["code"], -32601);
    }

    #[test]
    fn tools_call_without_params_is_invalid_params() {
        let resp =
            handle(&json!({ "jsonrpc": "2.0", "id": 6, "method": "tools/call" })).expect("reply");
        assert_eq!(resp["error"]["code"], -32602);
    }
}
