// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The ACP wire, hand-rolled (spec §2bis Lane A) — JSON-RPC 2.0 over
//! newline-delimited JSON lines, wire protocol v1.
//!
//! Every shape here was read off the OFFICIAL schema source
//! (`agent-client-protocol-schema` 1.5.0 · verified 2026-08-05):
//! structs `camelCase` · `StopReason` `snake_case` · `SessionUpdate`
//! internally tagged `sessionUpdate` (`snake_case` variants) · permission
//! outcome tagged `outcome` · protocol version = the integer `1`. The
//! reader side is LENIENT on purpose (unknown fields ignored — the
//! forward-compat posture the trace reader already takes); the writer
//! side emits exactly the narrow waist the Client role needs:
//! `initialize` · `session/new` · `session/prompt` · `session/cancel` ·
//! the `session/request_permission` response.

use serde::{Deserialize, Serialize};
use serde_json::Value;

/// The six wire method names (the Client role's narrow waist) — the
/// exact strings the official schema pins.
pub const METHOD_INITIALIZE: &str = "initialize";
/// `session/new`.
pub const METHOD_SESSION_NEW: &str = "session/new";
/// `session/prompt`.
pub const METHOD_SESSION_PROMPT: &str = "session/prompt";
/// `session/cancel` (notification).
pub const METHOD_SESSION_CANCEL: &str = "session/cancel";
/// `session/update` (notification, agent → client).
pub const METHOD_SESSION_UPDATE: &str = "session/update";
/// `session/request_permission` (request, agent → client).
pub const METHOD_REQUEST_PERMISSION: &str = "session/request_permission";

/// The wire protocol version this client speaks — the INTEGER `1`
/// (schema `ProtocolVersion::V1`). A response naming another version
/// refuses (the schema-diff gate's runtime twin).
pub const PROTOCOL_V1: u16 = 1;

// ─── Envelope ───────────────────────────────────────────────────────

/// Render one outgoing REQUEST line (JSON-RPC 2.0 · numeric id).
///
/// # Errors
///
/// Serialization of `params` failed (a caller bug — the params types
/// below always serialize).
pub fn request_line(id: u64, method: &str, params: &impl Serialize) -> Result<String, WireError> {
    let frame = serde_json::json!({
        "jsonrpc": "2.0",
        "id": id,
        "method": method,
        "params": params,
    });
    serde_json::to_string(&frame).map_err(WireError::from)
}

/// Render one outgoing NOTIFICATION line (no id · fire and forget).
///
/// # Errors
///
/// Serialization of `params` failed.
pub fn notification_line(method: &str, params: &impl Serialize) -> Result<String, WireError> {
    let frame = serde_json::json!({
        "jsonrpc": "2.0",
        "method": method,
        "params": params,
    });
    serde_json::to_string(&frame).map_err(WireError::from)
}

/// Render one outgoing RESPONSE line (answering an agent → client
/// request · the permission bridge's half).
///
/// # Errors
///
/// Serialization of `result` failed.
pub fn response_line(id: &Value, result: &impl Serialize) -> Result<String, WireError> {
    let frame = serde_json::json!({
        "jsonrpc": "2.0",
        "id": id,
        "result": result,
    });
    serde_json::to_string(&frame).map_err(WireError::from)
}

/// One parsed incoming line, classified by envelope shape.
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub enum Incoming {
    /// A response to one of OUR requests (`id` + `result`).
    Response {
        /// The correlated request id.
        id: u64,
        /// The raw result payload.
        result: Value,
    },
    /// A response carrying a JSON-RPC error object.
    ErrorResponse {
        /// The correlated request id.
        id: u64,
        /// The error's `message` (code folded in for the teaching line).
        message: String,
    },
    /// A request FROM the agent (`id` + `method` — permission asks).
    Request {
        /// The agent's request id, echoed verbatim in our response
        /// (kept as raw JSON: the peer may use non-numeric ids).
        id: Value,
        /// The method name.
        method: String,
        /// The raw params.
        params: Value,
    },
    /// A notification FROM the agent (no id — session updates).
    Notification {
        /// The method name.
        method: String,
        /// The raw params.
        params: Value,
    },
}

/// Classify one incoming line.
///
/// # Errors
///
/// The line is not valid JSON, or its envelope fits no JSON-RPC shape.
pub fn parse_line(line: &str) -> Result<Incoming, WireError> {
    let v: Value = serde_json::from_str(line)?;
    let obj = v.as_object().ok_or_else(|| WireError::Envelope {
        why: "line is not a JSON object".to_owned(),
    })?;
    let id = obj.get("id");
    let method = obj.get("method").and_then(Value::as_str);
    match (id, method) {
        (Some(id), Some(m)) => Ok(Incoming::Request {
            id: id.clone(),
            method: m.to_owned(),
            params: obj.get("params").cloned().unwrap_or(Value::Null),
        }),
        (None, Some(m)) => Ok(Incoming::Notification {
            method: m.to_owned(),
            params: obj.get("params").cloned().unwrap_or(Value::Null),
        }),
        (Some(id), None) => {
            let id = id.as_u64().ok_or_else(|| WireError::Envelope {
                why: "response id is not the numeric id this client sent".to_owned(),
            })?;
            if let Some(err) = obj.get("error") {
                let code = err.get("code").and_then(Value::as_i64).unwrap_or_default();
                let msg = err
                    .get("message")
                    .and_then(Value::as_str)
                    .unwrap_or("unnamed error");
                Ok(Incoming::ErrorResponse {
                    id,
                    message: format!("{msg} (jsonrpc {code})"),
                })
            } else {
                Ok(Incoming::Response {
                    id,
                    result: obj.get("result").cloned().unwrap_or(Value::Null),
                })
            }
        }
        (None, None) => Err(WireError::Envelope {
            why: "neither request, notification nor response".to_owned(),
        }),
    }
}

// ─── Outgoing params (writer side · exactly what we send) ───────────

/// `initialize` params.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InitializeParams {
    /// The wire version we speak (the integer `1`).
    pub protocol_version: u16,
    /// Client capabilities — the empty object: this client offers no
    /// fs/terminal caps in B3.1 (they arrive with the sandbox backing).
    pub client_capabilities: Value,
}

/// `session/new` params.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NewSessionParams {
    /// The session's working directory (absolute).
    pub cwd: std::path::PathBuf,
    /// MCP servers the agent should mount — empty in B3.1 (the
    /// nika-mcp injection is a P3 rider).
    pub mcp_servers: Vec<Value>,
}

/// One text content block (the baseline every agent MUST support).
#[derive(Debug, Serialize)]
pub struct TextBlock {
    /// The tag — always `"text"`.
    #[serde(rename = "type")]
    pub kind: &'static str,
    /// The text.
    pub text: String,
}

/// `session/prompt` params.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PromptParams {
    /// The session this prompt belongs to.
    pub session_id: String,
    /// The user message as content blocks (text-only in B3.1).
    pub prompt: Vec<TextBlock>,
}

/// `session/cancel` params.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CancelParams {
    /// The session to cancel.
    pub session_id: String,
}

/// The `session/request_permission` RESPONSE result — outcome tagged
/// `outcome`, `snake_case` (schema-pinned).
#[derive(Debug, Serialize)]
pub struct PermissionResult {
    /// The outcome object.
    pub outcome: Value,
}

impl PermissionResult {
    /// The selected-option outcome.
    #[must_use]
    pub fn selected(option_id: &str) -> Self {
        Self {
            outcome: serde_json::json!({ "outcome": "selected", "optionId": option_id }),
        }
    }

    /// The cancelled outcome — the fail-closed word (a denied or
    /// unanswerable ask never selects anything).
    #[must_use]
    pub fn cancelled() -> Self {
        Self {
            outcome: serde_json::json!({ "outcome": "cancelled" }),
        }
    }
}

// ─── Incoming payloads (reader side · lenient) ──────────────────────

/// `initialize` result — only what we judge.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InitializeResult {
    /// The version the agent settled on.
    pub protocol_version: u16,
}

/// `session/new` result.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NewSessionResult {
    /// The agent-assigned session id.
    pub session_id: String,
}

/// `session/prompt` result.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PromptResult {
    /// Why the turn ended (schema `snake_case`: `end_turn` · `refusal` ·
    /// `cancelled` · `max_tokens` · `max_turn_requests` · future).
    pub stop_reason: String,
}

/// `session/update` params — the update rides RAW; the client routes
/// on its `sessionUpdate` tag and reads only the chunk text (every
/// other update kind is observed-and-passed in B3.1).
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionUpdateParams {
    /// The session the update belongs to.
    pub session_id: String,
    /// The tagged update object.
    pub update: Value,
}

/// One permission option, lenient.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PermissionOptionIn {
    /// The id our response selects.
    pub option_id: String,
    /// The kind hint (schema `snake_case`: `allow_once` · `allow_always`
    /// · `reject_once` · `reject_always`).
    pub kind: String,
}

/// `session/request_permission` params, lenient.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PermissionRequestIn {
    /// The session asking.
    pub session_id: String,
    /// The options offered.
    #[serde(default)]
    pub options: Vec<PermissionOptionIn>,
    /// The tool call this ask is about (raw · its `title` becomes the
    /// question when present).
    #[serde(default)]
    pub tool_call: Value,
}

/// The agent-message chunk text of an update, when it is one — `None`
/// for every other update kind (observed, never an error).
#[must_use]
pub fn agent_chunk_text(update: &Value) -> Option<String> {
    if update.get("sessionUpdate").and_then(Value::as_str) != Some("agent_message_chunk") {
        return None;
    }
    let content = update.get("content")?;
    if content.get("type").and_then(Value::as_str) != Some("text") {
        return None;
    }
    content
        .get("text")
        .and_then(Value::as_str)
        .map(str::to_owned)
}

// ─── Wire error ─────────────────────────────────────────────────────

/// A wire-level defect (invalid JSON · unclassifiable envelope).
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum WireError {
    /// The line is not valid JSON.
    #[error("wire: invalid JSON: {0}")]
    Json(#[from] serde_json::Error),
    /// The JSON fits no JSON-RPC envelope shape.
    #[error("wire: bad envelope: {why}")]
    Envelope {
        /// What was wrong.
        why: String,
    },
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn request_lines_speak_jsonrpc_two_with_camel_case_params() {
        let line = request_line(
            1,
            METHOD_INITIALIZE,
            &InitializeParams {
                protocol_version: PROTOCOL_V1,
                client_capabilities: serde_json::json!({}),
            },
        )
        .expect("serializes");
        let v: Value = serde_json::from_str(&line).expect("valid json");
        assert_eq!(v["jsonrpc"], "2.0");
        assert_eq!(v["id"], 1);
        assert_eq!(v["method"], "initialize");
        // The schema pins camelCase + the INTEGER version.
        assert_eq!(v["params"]["protocolVersion"], 1);
        assert!(v["params"]["clientCapabilities"].is_object());
        assert!(!line.contains('\n'), "one line, no embedded newline");
    }

    #[test]
    fn prompt_params_carry_the_text_block_baseline() {
        let line = request_line(
            3,
            METHOD_SESSION_PROMPT,
            &PromptParams {
                session_id: "s-1".into(),
                prompt: vec![TextBlock {
                    kind: "text",
                    text: "hello".into(),
                }],
            },
        )
        .expect("serializes");
        let v: Value = serde_json::from_str(&line).expect("valid json");
        assert_eq!(v["params"]["sessionId"], "s-1");
        assert_eq!(v["params"]["prompt"][0]["type"], "text");
        assert_eq!(v["params"]["prompt"][0]["text"], "hello");
    }

    #[test]
    fn incoming_classification_covers_the_three_shapes() {
        let resp =
            parse_line(r#"{"jsonrpc":"2.0","id":2,"result":{"sessionId":"s-1"}}"#).expect("parses");
        assert!(matches!(resp, Incoming::Response { id: 2, .. }));

        let req = parse_line(
            r#"{"jsonrpc":"2.0","id":"agent-7","method":"session/request_permission","params":{}}"#,
        )
        .expect("parses");
        let Incoming::Request { id, method, .. } = req else {
            panic!("a line with id+method is a request");
        };
        assert_eq!(id, Value::String("agent-7".into()));
        assert_eq!(method, METHOD_REQUEST_PERMISSION);

        let notif = parse_line(r#"{"jsonrpc":"2.0","method":"session/update","params":{}}"#)
            .expect("parses");
        assert!(matches!(notif, Incoming::Notification { .. }));

        let err =
            parse_line(r#"{"jsonrpc":"2.0","id":3,"error":{"code":-32600,"message":"nope"}}"#)
                .expect("parses");
        let Incoming::ErrorResponse { id: 3, message } = err else {
            panic!("an id+error line is an error response");
        };
        assert!(message.contains("nope"), "{message}");
        assert!(message.contains("-32600"), "{message}");
    }

    #[test]
    fn envelope_refuses_the_shapeless() {
        assert!(parse_line("not json").is_err());
        assert!(parse_line(r#"{"jsonrpc":"2.0"}"#).is_err());
        assert!(parse_line("[1,2,3]").is_err());
    }

    #[test]
    fn agent_chunk_text_reads_only_its_kind() {
        let chunk = serde_json::json!({
            "sessionUpdate": "agent_message_chunk",
            "content": { "type": "text", "text": "hi" }
        });
        assert_eq!(agent_chunk_text(&chunk).as_deref(), Some("hi"));
        // Every other kind is observed-and-passed, never an error.
        let thought = serde_json::json!({
            "sessionUpdate": "agent_thought_chunk",
            "content": { "type": "text", "text": "thinking" }
        });
        assert_eq!(agent_chunk_text(&thought), None);
        let non_text = serde_json::json!({
            "sessionUpdate": "agent_message_chunk",
            "content": { "type": "image" }
        });
        assert_eq!(agent_chunk_text(&non_text), None);
    }

    #[test]
    fn permission_outcomes_speak_the_tagged_wire() {
        let sel = serde_json::to_value(PermissionResult::selected("opt-1")).expect("json");
        assert_eq!(sel["outcome"]["outcome"], "selected");
        assert_eq!(sel["outcome"]["optionId"], "opt-1");
        let can = serde_json::to_value(PermissionResult::cancelled()).expect("json");
        assert_eq!(can["outcome"]["outcome"], "cancelled");
    }

    #[test]
    fn lenient_readers_ignore_unknown_fields() {
        let r: NewSessionResult =
            serde_json::from_str(r#"{"sessionId":"s-9","modes":{"x":1},"futureField":true}"#)
                .expect("lenient");
        assert_eq!(r.session_id, "s-9");
        let p: PermissionRequestIn = serde_json::from_str(
            r#"{"sessionId":"s-9","options":[{"optionId":"a","kind":"allow_once","name":"Allow"}]}"#,
        )
        .expect("lenient");
        assert_eq!(p.options.len(), 1);
        assert_eq!(p.options[0].kind, "allow_once");
    }
}
