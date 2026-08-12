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

use crate::prompts;
use crate::tools;

/// The MCP protocol revision this server prefers (its own latest).
///
/// 2026-07-28 (FINAL of the RC locked 2026-05-21) is a STATELESS-core
/// revision: over HTTP the handshake + `Mcp-Session-Id` disappear in favor
/// of per-request `Mcp-Method`/`Mcp-Name` headers — over STDIO (this
/// server's only transport) nothing structural changes: a 2026-07-28
/// client may simply START with `tools/list` (no `initialize` first), which
/// this dispatcher has always accepted (every method is handled statelessly ·
/// no session object exists to require). Tool inputSchemas are full JSON
/// Schema 2020-12 (`oneOf`/`$ref` allowed — ours already conform as plain
/// object schemas). The 2025-era `-32002` code is not emitted here (unknown
/// method = `-32601` · malformed = `-32600` · both unchanged).
pub(crate) const PROTOCOL_VERSION: &str = "2026-07-28";
/// The revisions this server interoperates with — `initialize` ECHOES the
/// client's requested version when it is one of these (spec lifecycle · version
/// negotiation MUST · « respond with the same version »), else answers with our
/// latest and the client decides. All four drive the identical tools/list +
/// tools/call surface; batching (a 2024-11-05 / 2025-03-26 feature) is handled
/// by [`dispatch`]; statelessness (2026-07-28) is the dispatcher's native
/// shape. SUPPORTED stays additive — no client ever breaks (no flag-day).
pub(crate) const SUPPORTED: [&str; 5] = [
    "2026-07-28",
    "2025-11-25",
    "2025-06-18",
    "2025-03-26",
    "2024-11-05",
];
/// The advertised server name (`serverInfo.name`).
pub(crate) const SERVER_NAME: &str = "nika";

/// Dispatch one INCOMING stdio message — a single request/notification (a JSON
/// object) OR a JSON-RPC 2.0 BATCH (an array · the 2024-11-05 / 2025-03-26
/// revs). A batch returns an array of the non-empty replies, `None` when every
/// member was a notification (JSON-RPC 2.0 §6 · a batch of only notifications
/// gets no response).
#[must_use]
pub(crate) fn dispatch(msg: &Value) -> Option<Value> {
    if let Some(batch) = msg.as_array() {
        // An empty batch is itself an Invalid Request — a single `-32600`
        // reply, not silence (JSON-RPC 2.0 §6 · else a batching client hangs).
        if batch.is_empty() {
            return Some(err(&Value::Null, -32600, "invalid request: empty batch"));
        }
        let replies: Vec<Value> = batch.iter().filter_map(handle).collect();
        return (!replies.is_empty()).then_some(Value::Array(replies));
    }
    handle(msg)
}

/// Dispatch one single JSON-RPC 2.0 message.
///
/// Returns `Some(reply)` for a request (a message carrying an `id`), `None` for
/// a notification (no `id` · JSON-RPC forbids replying). Unknown methods get a
/// `-32601` error; a `tools/call` for an unknown tool is a successful reply
/// with `isError: true` (a TOOL error, not a PROTOCOL error · per MCP).
#[must_use]
pub(crate) fn handle(msg: &Value) -> Option<Value> {
    // A notification has no `id` — never reply (JSON-RPC 2.0 §4.1).
    let id = msg.get("id")?;
    // `method` MUST be a present string (JSON-RPC 2.0 §4) — its absence or a
    // non-string value is a malformed request (`-32600` Invalid Request), NOT
    // an unknown method (`-32601`); a strict client branches on the code.
    let Some(method) = msg.get("method").and_then(Value::as_str) else {
        return Some(err(
            id,
            -32600,
            "invalid request: `method` must be a string",
        ));
    };
    Some(match method {
        "initialize" => ok(id, initialize_result(msg.get("params"))),
        "tools/list" => ok(id, json!({ "tools": tools::catalog() })),
        "tools/call" => tools_call(id, msg.get("params")),
        "prompts/list" => ok(id, json!({ "prompts": prompts::catalog() })),
        "prompts/get" => prompts_get(id, msg.get("params")),
        "ping" => ok(id, json!({})),
        other => err(id, -32601, &format!("method not found: {other}")),
    })
}

/// The `initialize` result — the negotiated protocol version · the tools
/// and prompts capabilities · identity. Version negotiation (spec lifecycle · MUST): echo the
/// client's requested version when supported, else answer with ours.
fn initialize_result(params: Option<&Value>) -> Value {
    let version = params
        .and_then(|p| p.get("protocolVersion"))
        .and_then(Value::as_str)
        .filter(|v| SUPPORTED.contains(v))
        .unwrap_or(PROTOCOL_VERSION);
    json!({
        "protocolVersion": version,
        "capabilities": { "tools": {}, "prompts": {} },
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
    let name = params.get("name").and_then(Value::as_str).unwrap_or("");
    let args = params
        .get("arguments")
        .cloned()
        .unwrap_or_else(|| json!({}));
    match tools::execute(name, &args) {
        Ok(text) => ok(id, tool_content(&text, false)),
        Err(text) => ok(id, tool_content(&text, true)),
    }
}

/// `prompts/get` → the named kit command, rendered with its argument.
/// Prompts have NO `isError` channel (unlike `tools/call`): an unknown
/// name or a missing required argument is a real `-32602`, never a
/// content block a model would read back as instructions.
fn prompts_get(id: &Value, params: Option<&Value>) -> Value {
    let Some(params) = params else {
        return err(
            id,
            -32602,
            "invalid params: `prompts/get` requires {name, arguments}",
        );
    };
    let Some(name) = params.get("name").and_then(Value::as_str) else {
        return err(
            id,
            -32602,
            "invalid params: `prompts/get` requires a `name`",
        );
    };
    match prompts::render(name, params.get("arguments")) {
        Ok(result) => ok(id, result),
        Err(message) => err(id, -32602, &message),
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
    use proptest::prelude::*;

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
    fn negotiates_2026_07_28_by_echo() {
        // The FINAL rev (RC locked 2026-05-21) is in SUPPORTED — a client
        // requesting it gets it echoed back (spec lifecycle MUST).
        let resp = handle(&json!({ "jsonrpc": "2.0", "id": 9, "method": "initialize",
            "params": { "protocolVersion": "2026-07-28" } }))
        .expect("reply");
        assert_eq!(resp["result"]["protocolVersion"], "2026-07-28");
    }

    #[test]
    fn stateless_client_may_start_without_initialize() {
        // 2026-07-28 stateless core: a client may open with tools/list —
        // no handshake first. This dispatcher has always been stateless;
        // this test PINS that property against a future session object.
        let resp =
            handle(&json!({ "jsonrpc": "2.0", "id": 1, "method": "tools/list" })).expect("reply");
        assert!(
            resp["result"]["tools"]
                .as_array()
                .is_some_and(|t| !t.is_empty()),
            "tools/list answers cold · no initialize required"
        );
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
        // 3 validate (check · inspect · explain) + 6 learn (schema ·
        // examples · template · canon · catalog · tools).
        assert_eq!(tools.len(), 9);
    }

    #[test]
    fn tools_call_runs_the_tool() {
        let wf = "nika: t\npermits: { exec: [\"echo\"] }\ntasks:\n  a:\n    exec: { command: [\"echo\", \"hi\"] }\n";
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
    fn tools_call_dirty_check_is_iserror_so_the_repair_loop_triggers() {
        // A workflow with findings comes back isError:true (the model
        // sees the findings AND a repair-on-error harness fires) — the
        // MCP lane must agree with the CLI's exit-2-on-dirty.
        let wf = "nika: t\ntasks:\n  a:\n    after:\n      ghost: success\n    exec: { command: [\"x\"] }\n";
        let resp = handle(&json!({
            "jsonrpc": "2.0", "id": 7, "method": "tools/call",
            "params": { "name": "nika_check", "arguments": { "workflow": wf } }
        }))
        .expect("reply");
        assert!(resp.get("error").is_none(), "not a protocol error");
        assert_eq!(
            resp["result"]["isError"], true,
            "dirty check signals isError"
        );
        assert!(
            resp["result"]["content"][0]["text"]
                .as_str()
                .expect("text")
                .contains("findings"),
            "the findings still ride the content so the model repairs"
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
    fn a_request_without_a_method_is_invalid_request() {
        // Absent `method` on a request → -32600 Invalid Request, NOT -32601 (a
        // strict MCP client branches on the code · JSON-RPC 2.0 §4).
        let resp = handle(&json!({ "jsonrpc": "2.0", "id": 7 })).expect("reply");
        assert_eq!(resp["error"]["code"], -32600);
    }

    #[test]
    fn a_non_string_method_is_invalid_request() {
        // `method` present but not a string → -32600, not "method not found: ".
        let resp = handle(&json!({ "jsonrpc": "2.0", "id": 8, "method": 42 })).expect("reply");
        assert_eq!(resp["error"]["code"], -32600);
    }

    #[test]
    fn an_empty_batch_is_invalid_request() {
        // JSON-RPC 2.0 §6 — an empty array batch gets a single -32600 reply,
        // never silence (a batching client would otherwise hang).
        let resp = dispatch(&json!([])).expect("a single error reply, not silence");
        assert_eq!(resp["error"]["code"], -32600);
        assert!(
            resp["id"].is_null(),
            "the id is null when it cannot be determined"
        );
    }

    #[test]
    fn tools_call_without_params_is_invalid_params() {
        let resp =
            handle(&json!({ "jsonrpc": "2.0", "id": 6, "method": "tools/call" })).expect("reply");
        assert_eq!(resp["error"]["code"], -32602);
    }

    #[test]
    fn initialize_echoes_a_supported_client_version() {
        // Spec lifecycle MUST: a client on an older supported rev gets THAT rev
        // back (not our latest) — so it connects instead of disconnecting.
        for v in ["2024-11-05", "2025-03-26", "2025-06-18", "2025-11-25"] {
            let resp = handle(&json!({
                "jsonrpc": "2.0", "id": 1, "method": "initialize",
                "params": { "protocolVersion": v }
            }))
            .expect("reply");
            assert_eq!(
                resp["result"]["protocolVersion"], v,
                "echo the client's {v}"
            );
        }
    }

    #[test]
    fn initialize_falls_back_to_ours_on_an_unknown_version() {
        let resp = handle(&json!({
            "jsonrpc": "2.0", "id": 1, "method": "initialize",
            "params": { "protocolVersion": "1999-01-01" }
        }))
        .expect("reply");
        assert_eq!(resp["result"]["protocolVersion"], PROTOCOL_VERSION);
    }

    #[test]
    fn a_batch_returns_an_array_of_replies() {
        // JSON-RPC 2.0 batch (the 2024/2025-03 revs) — an array in, an array of
        // the non-notification replies out.
        let batch = json!([
            { "jsonrpc": "2.0", "id": 1, "method": "ping" },
            { "jsonrpc": "2.0", "method": "notifications/initialized" },
            { "jsonrpc": "2.0", "id": 2, "method": "tools/list" }
        ]);
        let resp = dispatch(&batch).expect("a batch with requests replies");
        let arr = resp.as_array().expect("array reply");
        assert_eq!(
            arr.len(),
            2,
            "two requests → two replies · the notification is silent"
        );
        assert_eq!(arr[0]["id"], 1);
        assert_eq!(arr[1]["id"], 2);
    }

    #[test]
    fn a_batch_of_only_notifications_is_silent() {
        let batch = json!([{ "jsonrpc": "2.0", "method": "notifications/initialized" }]);
        assert!(
            dispatch(&batch).is_none(),
            "no reply when every member is a notification"
        );
    }

    proptest! {
        /// Robustness over untrusted client input: for ANY method name and id, a
        /// request gets a well-formed JSON-RPC 2.0 reply — `id` echoed, EXACTLY
        /// one of result/error, and never a panic (the MCP stdio surface accepts
        /// whatever an arbitrary connecting client sends).
        #[test]
        fn any_request_is_a_wellformed_reply(method in "\\PC{0,32}", id in 0u64..100_000) {
            let reply = handle(&json!({ "jsonrpc": "2.0", "id": id, "method": method }))
                .expect("a message carrying an id always replies");
            prop_assert_eq!(&reply["jsonrpc"], &json!("2.0"));
            prop_assert_eq!(&reply["id"], &json!(id));
            prop_assert!(
                reply.get("result").is_some() ^ reply.get("error").is_some(),
                "exactly one of result/error: {}",
                reply
            );
        }

        /// For ANY method, a message with no `id` is silent (JSON-RPC 2.0 §4.1).
        #[test]
        fn any_notification_is_silent(method in "\\PC{0,32}") {
            let reply = handle(&json!({ "jsonrpc": "2.0", "method": method }));
            prop_assert!(reply.is_none(), "a message with no id must be silent: {:?}", reply);
        }
    }
}
