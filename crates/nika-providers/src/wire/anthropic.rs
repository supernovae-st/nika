// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Anthropic Messages API wire adapter.
//!
//! Request/response mapping is pure (testable without I/O); transport rides
//! the injected kernel http effect. Streaming maps the Messages SSE event
//! family onto the kernel `InferEvent` stream.

use std::collections::BTreeMap;

use bytes::Bytes;
use nika_kernel::ai::provider::{
    ContentBlock, InferEvent, InferEventStream, InferRequest, InferResponse, ProviderError, Role,
    StopReason, ToolChoice,
};
use nika_kernel::ai::provider::{ResponseFormat, TokenUsage};
use nika_kernel::http::{HttpPostDyn, HttpRequest};
use serde_json::{Value, json};

use super::{
    EventMapper, SseEventStream, ToolNameMap, gen_ai_system, map_http_err, status_error, str_at,
    u64_at,
};
use crate::registry::ResolvedProvider;

const ANTHROPIC_VERSION: &str = "2023-06-01";
const DEFAULT_MAX_TOKENS: u32 = 4096;

/// Single-shot inference.
pub(crate) async fn infer<H>(
    rp: &ResolvedProvider<H>,
    request: InferRequest,
) -> Result<InferResponse, ProviderError>
where
    H: HttpPostDyn + Send + Sync + 'static,
{
    // Anthropic's function-calling rejects tool names outside
    // `^[a-zA-Z0-9_-]{1,64}$` (NIKA-463) — Nika's `nika:read` / `mcp:git/diff`
    // ids must be sanitized on send and restored on parse (the verb only
    // ever sees the canonical colon form).
    let names = ToolNameMap::from_tools(&request.tools);
    let http_req = build_request(rp, &request, false, &names)?;
    let http = rp.http.as_ref().ok_or_else(wiring_bug)?;
    let resp = http.post(http_req).await.map_err(|e| map_http_err(&e))?;
    if !(200..300).contains(&resp.status) {
        return Err(status_error(
            resp.status,
            &resp.body,
            resp.headers.get("retry-after").map(String::as_str),
            &rp.wire_model,
        ));
    }
    parse_response(rp, &resp.body, &names)
}

/// Streaming inference.
pub(crate) async fn infer_stream<H>(
    rp: &ResolvedProvider<H>,
    request: InferRequest,
) -> Result<InferEventStream, ProviderError>
where
    H: HttpPostDyn + Send + Sync + 'static,
{
    let names = ToolNameMap::from_tools(&request.tools);
    let http_req = build_request(rp, &request, true, &names)?;
    let http = rp.http.as_ref().ok_or_else(wiring_bug)?;
    let resp = http
        .send_streaming(http_req)
        .await
        .map_err(|e| map_http_err(&e))?;
    if !(200..300).contains(&resp.status) {
        return Err(super::stream_status_error(resp, &rp.wire_model).await);
    }
    Ok(Box::pin(SseEventStream::new(
        resp.body,
        AnthropicMapper::new(names),
    )))
}

fn wiring_bug() -> ProviderError {
    ProviderError::Other {
        reason: "anthropic wire reached without an http effect (registry bug)".to_owned(),
    }
}

/// Build the HTTP request (headers + JSON body).
fn build_request(
    rp: &ResolvedProvider<impl Sized>,
    req: &InferRequest,
    stream: bool,
    names: &ToolNameMap,
) -> Result<HttpRequest, ProviderError> {
    let key = rp.key.as_ref().ok_or_else(|| ProviderError::AuthFailed {
        reason: "anthropic requires an API key".to_owned(),
    })?;
    let body = request_body(&rp.wire_model, req, stream, names)?;
    let bytes = serde_json::to_vec(&body).map_err(|e| ProviderError::Other {
        reason: format!("request serialization failed: {e}"),
    })?;

    let mut headers = BTreeMap::new();
    headers.insert("x-api-key".to_owned(), key.expose().to_owned());
    headers.insert("anthropic-version".to_owned(), ANTHROPIC_VERSION.to_owned());
    headers.insert("content-type".to_owned(), "application/json".to_owned());

    let mut http_req = HttpRequest::post(rp.base_url.clone());
    http_req.headers = headers;
    http_req.body = Some(Bytes::from(bytes));
    // The task `timeout:` governs the transport deadline (F1) — see
    // `wire::transport_deadline` for the buffered-vs-streaming split.
    http_req.timeout = super::transport_deadline(&rp.profile, req, stream);
    Ok(http_req)
}

/// The JSON body (pure — the unit-testable core).
fn request_body(
    model: &str,
    req: &InferRequest,
    stream: bool,
    names: &ToolNameMap,
) -> Result<Value, ProviderError> {
    if !matches!(req.response_format, ResponseFormat::Text) {
        return Err(ProviderError::Other {
            reason: "anthropic does not support response_format at v0.1 · \
                     use an openai-compat provider or prompt-level JSON"
                .to_owned(),
        });
    }

    let mut system = String::new();
    let mut messages = Vec::new();
    for m in &req.messages {
        if matches!(m.role, Role::System) {
            for block in &m.content {
                if let ContentBlock::Text { text } = block {
                    if !system.is_empty() {
                        system.push('\n');
                    }
                    system.push_str(text);
                }
            }
            continue;
        }
        let role = if matches!(m.role, Role::Assistant) {
            "assistant"
        } else {
            "user"
        };
        let mut content = Vec::new();
        for block in &m.content {
            if let Some(v) = content_block(block, names)? {
                content.push(v);
            }
        }
        if !content.is_empty() {
            messages.push(json!({ "role": role, "content": content }));
        }
    }

    let mut body = json!({
        "model": model,
        "max_tokens": req.max_tokens.unwrap_or(DEFAULT_MAX_TOKENS),
        "messages": messages,
        "stream": stream,
    });
    let obj = body.as_object_mut().ok_or_else(|| ProviderError::Other {
        reason: "request body must be an object".to_owned(),
    })?;
    if !system.is_empty() {
        obj.insert("system".to_owned(), Value::String(system));
    }
    if let Some(t) = req.temperature {
        obj.insert("temperature".to_owned(), json!(t));
    }
    if !req.stop_sequences.is_empty() {
        obj.insert("stop_sequences".to_owned(), json!(req.stop_sequences));
    }
    if !req.tools.is_empty() {
        push_tools(obj, req, names);
    }
    if let Some(budget) = req.thinking_budget {
        obj.insert(
            "thinking".to_owned(),
            json!({ "type": "enabled", "budget_tokens": budget }),
        );
    }
    // Provider extras never override the structural keys this adapter set
    // (model · messages · stream · tools · …) — first-write-wins.
    for (k, v) in &req.extra.params {
        if !obj.contains_key(k) {
            obj.insert(k.clone(), v.clone());
        }
    }
    Ok(body)
}

/// Insert `tools` + `tool_choice`. Each tool name is sanitized to the
/// wire-legal charset (`^[a-zA-Z0-9_-]{1,64}$` · NIKA-463); the model echoes
/// the sanitized name in `tool_use`, restored to the canonical id on parse.
fn push_tools(obj: &mut serde_json::Map<String, Value>, req: &InferRequest, names: &ToolNameMap) {
    let tools: Vec<Value> = req
        .tools
        .iter()
        .map(|t| {
            json!({
                "name": names.to_wire(&t.name),
                "description": t.description,
                "input_schema": t.parameters,
            })
        })
        .collect();
    obj.insert("tools".to_owned(), Value::Array(tools));
    let choice = match &req.tool_choice {
        ToolChoice::Required => json!({ "type": "any" }),
        ToolChoice::None => json!({ "type": "none" }),
        ToolChoice::Specific(name) => json!({ "type": "tool", "name": names.to_wire(name) }),
        ToolChoice::Auto | _ => json!({ "type": "auto" }),
    };
    obj.insert("tool_choice".to_owned(), choice);
}

/// Kernel content block → Messages API block. `Ok(None)` = skipped
/// (thinking blocks are model-internal; they are not resent). A re-sent
/// `ToolUse` (the assistant's prior call, stored with its canonical id) is
/// re-sanitized so its `name` matches the registered tool.
fn content_block(
    block: &ContentBlock,
    names: &ToolNameMap,
) -> Result<Option<Value>, ProviderError> {
    Ok(match block {
        ContentBlock::Text { text } => Some(json!({ "type": "text", "text": text })),
        ContentBlock::Image { source, .. } => {
            if source.starts_with("http://") || source.starts_with("https://") {
                Some(json!({ "type": "image", "source": { "type": "url", "url": source } }))
            } else {
                return Err(ProviderError::Other {
                    reason: "image source must be a URL at v0.1 (CAS sources land with nika-media)"
                        .to_owned(),
                });
            }
        }
        ContentBlock::ToolUse { id, name, input } => Some(json!({
            "type": "tool_use", "id": id, "name": names.to_wire(name), "input": input,
        })),
        ContentBlock::ToolResult {
            tool_use_id,
            content,
            is_error,
        } => Some(json!({
            "type": "tool_result", "tool_use_id": tool_use_id,
            "content": content, "is_error": is_error,
        })),
        ContentBlock::Thinking { .. } | _ => None,
    })
}

/// Parse a 2xx Messages API response.
fn parse_response(
    rp: &ResolvedProvider<impl Sized>,
    body: &[u8],
    names: &ToolNameMap,
) -> Result<InferResponse, ProviderError> {
    let v: Value = serde_json::from_slice(body).map_err(|e| ProviderError::Other {
        reason: format!("anthropic response is not JSON: {e}"),
    })?;

    let mut content = Vec::new();
    for block in v
        .pointer("/content")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
    {
        match block.pointer("/type").and_then(Value::as_str) {
            Some("text") => {
                if let Some(text) = block.pointer("/text").and_then(Value::as_str) {
                    content.push(ContentBlock::Text {
                        text: text.to_owned(),
                    });
                }
            }
            Some("tool_use") => content.push(ContentBlock::ToolUse {
                id: str_at(block, "/id"),
                name: names.to_canonical(&str_at(block, "/name")),
                input: block.pointer("/input").cloned().unwrap_or(Value::Null),
            }),
            Some("thinking") => {
                if let Some(text) = block.pointer("/thinking").and_then(Value::as_str) {
                    content.push(ContentBlock::Thinking {
                        text: text.to_owned(),
                    });
                }
            }
            _ => {}
        }
    }

    let mut usage = TokenUsage::new(
        u64_at(&v, "/usage/input_tokens"),
        u64_at(&v, "/usage/output_tokens"),
    );
    usage.cache_read_tokens = v
        .pointer("/usage/cache_read_input_tokens")
        .and_then(Value::as_u64);
    usage.cache_creation_tokens = v
        .pointer("/usage/cache_creation_input_tokens")
        .and_then(Value::as_u64);

    let raw_stop = v.pointer("/stop_reason").and_then(Value::as_str);
    let mut resp = InferResponse::new(content, usage, map_stop(raw_stop));
    resp.request_id = v
        .pointer("/id")
        .and_then(Value::as_str)
        .map(ToOwned::to_owned);
    resp.finish_reason_raw = raw_stop.map(ToOwned::to_owned);
    resp.cached_tokens = v
        .pointer("/usage/cache_read_input_tokens")
        .and_then(Value::as_u64)
        .and_then(|n| u32::try_from(n).ok());
    resp.gen_ai.system = gen_ai_system(rp.profile.id);
    resp.gen_ai.response_id = resp.request_id.clone();
    resp.gen_ai.response_model = v
        .pointer("/model")
        .and_then(Value::as_str)
        .map(ToOwned::to_owned);
    Ok(resp)
}

fn map_stop(raw: Option<&str>) -> StopReason {
    match raw {
        Some("end_turn") => StopReason::EndTurn,
        Some("max_tokens") => StopReason::MaxTokens,
        Some("stop_sequence") => StopReason::StopSequence,
        Some("tool_use") => StopReason::ToolUse,
        Some(other) => StopReason::Unknown(other.to_owned()),
        None => StopReason::Unknown("missing-stop-reason".to_owned()),
    }
}

/// Messages SSE → `InferEvent` translator.
#[derive(Default)]
struct AnthropicMapper {
    request_id: Option<String>,
    input_tokens: u64,
    stop_reason: Option<String>,
    /// `content_block` index → tool id (for `input_json_delta` routing).
    tools: BTreeMap<u64, String>,
    /// sanitized↔canonical tool-name map (restores the canonical id the
    /// model echoes back in its sanitized form · NIKA-463).
    names: ToolNameMap,
    done_sent: bool,
}

impl AnthropicMapper {
    fn new(names: ToolNameMap) -> Self {
        Self {
            names,
            ..Self::default()
        }
    }

    fn done(&mut self) -> InferEvent {
        self.done_sent = true;
        InferEvent::Done {
            stop_reason: map_stop(self.stop_reason.as_deref()),
            request_id: self.request_id.clone(),
            finish_reason_raw: self.stop_reason.clone(),
        }
    }
}

impl EventMapper for AnthropicMapper {
    fn map(&mut self, payload: &str) -> Vec<Result<InferEvent, ProviderError>> {
        let Ok(v) = serde_json::from_str::<Value>(payload) else {
            return Vec::new(); // tolerate keepalive noise
        };
        let mut out = Vec::new();
        match v.pointer("/type").and_then(Value::as_str) {
            Some("message_start") => {
                self.request_id = v
                    .pointer("/message/id")
                    .and_then(Value::as_str)
                    .map(ToOwned::to_owned);
                self.input_tokens = u64_at(&v, "/message/usage/input_tokens");
            }
            Some("content_block_start") => {
                if v.pointer("/content_block/type").and_then(Value::as_str) == Some("tool_use") {
                    let id = str_at(&v, "/content_block/id");
                    let index = u64_at(&v, "/index");
                    self.tools.insert(index, id.clone());
                    out.push(Ok(InferEvent::ToolUseStart {
                        id,
                        name: self.names.to_canonical(&str_at(&v, "/content_block/name")),
                    }));
                }
            }
            Some("content_block_delta") => match v.pointer("/delta/type").and_then(Value::as_str) {
                Some("text_delta") => out.push(Ok(InferEvent::Delta {
                    text: str_at(&v, "/delta/text"),
                })),
                Some("input_json_delta") => {
                    let index = u64_at(&v, "/index");
                    out.push(Ok(InferEvent::ToolUseDelta {
                        id: self.tools.get(&index).cloned().unwrap_or_default(),
                        partial_json: str_at(&v, "/delta/partial_json"),
                    }));
                }
                Some("thinking_delta") => out.push(Ok(InferEvent::Thinking {
                    text: str_at(&v, "/delta/thinking"),
                })),
                _ => {}
            },
            Some("message_delta") => {
                if let Some(s) = v.pointer("/delta/stop_reason").and_then(Value::as_str) {
                    self.stop_reason = Some(s.to_owned());
                }
                out.push(Ok(InferEvent::Usage(TokenUsage::new(
                    self.input_tokens,
                    u64_at(&v, "/usage/output_tokens"),
                ))));
            }
            Some("message_stop") => out.push(Ok(self.done())),
            Some("error") => {
                // In-band wire error ends the logical stream: mark done so
                // EOF does not append a synthetic Done after the Err (the
                // terminal contract is « ends with Err OR with Done »).
                self.done_sent = true;
                // `overloaded_error` (529-class) and `api_error` (500) are
                // transient per is_transient()'s 5xx rule; anything else
                // maps non-transient.
                let status = match v.pointer("/error/type").and_then(Value::as_str) {
                    Some("overloaded_error") => 529,
                    Some("api_error") => 500,
                    _ => 400,
                };
                out.push(Err(ProviderError::Api {
                    status,
                    message: str_at(&v, "/error/message"),
                }));
            }
            _ => {}
        }
        out
    }

    fn finish(&mut self) -> Vec<Result<InferEvent, ProviderError>> {
        if self.done_sent {
            Vec::new()
        } else {
            vec![Ok(self.done())]
        }
    }
}

#[cfg(test)]
mod tests {
    use nika_kernel::ai::provider::{Message, ToolDef};

    use super::*;
    use crate::test_support::{FakeHttp, collect, resolved_with};

    fn req(messages: Vec<Message>) -> InferRequest {
        InferRequest::new("claude-sonnet-4-20250514", messages)
    }

    /// `request_body` with the tool-name map derived from the request's own
    /// tools — exactly how `infer`/`infer_stream` build it in production.
    fn body_of(model: &str, r: &InferRequest, stream: bool) -> Result<Value, ProviderError> {
        let names = ToolNameMap::from_tools(&r.tools);
        request_body(model, r, stream, &names)
    }

    #[tokio::test]
    async fn infer_shapes_request_and_parses_response() {
        let fake = FakeHttp::with_json(
            200,
            r#"{"id":"msg_1","model":"claude-sonnet-4-20250514",
                "content":[{"type":"text","text":"bonjour"}],
                "stop_reason":"end_turn",
                "usage":{"input_tokens":12,"output_tokens":5,"cache_read_input_tokens":3}}"#,
        );
        let rp = resolved_with(&fake, "anthropic", "sk-ant-test");

        let mut request = req(vec![
            Message::text(Role::System, "tu es bref"),
            Message::text(Role::User, "salut"),
        ]);
        request.max_tokens = Some(64);
        request.temperature = Some(0.2);

        let resp = infer(&rp, request).await.expect("infer ok");

        // Response mapping.
        assert!(matches!(resp.stop_reason, StopReason::EndTurn));
        assert_eq!(resp.usage.input_tokens, 12);
        assert_eq!(resp.usage.output_tokens, 5);
        assert_eq!(resp.cached_tokens, Some(3));
        assert_eq!(resp.request_id.as_deref(), Some("msg_1"));
        assert_eq!(
            resp.gen_ai.system,
            nika_kernel::genai::GenAiSystem::Anthropic
        );
        assert_eq!(
            resp.gen_ai.response_model.as_deref(),
            Some("claude-sonnet-4-20250514")
        );
        match &resp.content[0] {
            ContentBlock::Text { text } => assert_eq!(text, "bonjour"),
            other => panic!("expected text, got {other:?}"),
        }

        // Request shape.
        let sent = fake.captured();
        assert_eq!(sent.len(), 1);
        assert_eq!(sent[0].url, "https://api.anthropic.com/v1/messages");
        assert_eq!(sent[0].headers.get("x-api-key").unwrap(), "sk-ant-test");
        assert_eq!(
            sent[0].headers.get("anthropic-version").unwrap(),
            ANTHROPIC_VERSION
        );
        let body: Value =
            serde_json::from_slice(sent[0].body.as_ref().expect("has body")).expect("json");
        assert_eq!(body["model"], "claude-sonnet-4-20250514");
        assert_eq!(body["max_tokens"], 64);
        assert_eq!(body["system"], "tu es bref");
        assert_eq!(body["stream"], false);
        assert_eq!(body["messages"][0]["role"], "user");
        assert_eq!(body["messages"][0]["content"][0]["text"], "salut");
    }

    #[test]
    fn tools_and_thinking_shape() {
        let mut r = req(vec![Message::text(Role::User, "calcule")]);
        r.tools = vec![ToolDef::new(
            "add",
            "additionne",
            serde_json::json!({"type":"object"}),
        )];
        r.tool_choice = ToolChoice::Required;
        r.thinking_budget = Some(2048);
        let body = body_of("m", &r, true).expect("body");
        assert_eq!(body["tools"][0]["name"], "add");
        assert!(body["tools"][0]["input_schema"].is_object());
        assert_eq!(body["tool_choice"]["type"], "any");
        assert_eq!(body["thinking"]["budget_tokens"], 2048);
        assert_eq!(body["stream"], true);
    }

    #[test]
    fn tool_round_trip_blocks_shape() {
        let messages = vec![
            Message::new(
                Role::Assistant,
                vec![ContentBlock::ToolUse {
                    id: "tu_1".into(),
                    name: "add".into(),
                    input: serde_json::json!({"a":1}),
                }],
            ),
            Message::new(
                Role::User,
                vec![ContentBlock::ToolResult {
                    tool_use_id: "tu_1".into(),
                    content: "2".into(),
                    is_error: false,
                }],
            ),
        ];
        let body = body_of("m", &req(messages), false).expect("body");
        assert_eq!(body["messages"][0]["content"][0]["type"], "tool_use");
        assert_eq!(body["messages"][1]["content"][0]["type"], "tool_result");
        assert_eq!(body["messages"][1]["content"][0]["tool_use_id"], "tu_1");
    }

    #[test]
    fn response_format_is_an_honest_error() {
        let mut r = req(vec![Message::text(Role::User, "json")]);
        r.response_format = ResponseFormat::Json;
        let err = body_of("m", &r, false).unwrap_err();
        assert!(err.to_string().contains("response_format"));
    }

    #[tokio::test]
    async fn auth_error_maps_from_status() {
        let fake = FakeHttp::with_json(401, r#"{"error":{"message":"invalid x-api-key"}}"#);
        let rp = resolved_with(&fake, "anthropic", "sk-ant-bad");
        let err = infer(&rp, req(vec![Message::text(Role::User, "x")]))
            .await
            .unwrap_err();
        assert!(matches!(err, ProviderError::AuthFailed { .. }), "{err}");
    }

    #[tokio::test]
    async fn stream_maps_the_event_family() {
        let sse = concat!(
            "event: message_start\n",
            "data: {\"type\":\"message_start\",\"message\":{\"id\":\"msg_s\",\"usage\":{\"input_tokens\":7}}}\n\n",
            "data: {\"type\":\"content_block_delta\",\"index\":0,\"delta\":{\"type\":\"text_delta\",\"text\":\"bon\"}}\n\n",
            "data: {\"type\":\"content_block_delta\",\"index\":0,\"delta\":{\"type\":\"text_delta\",\"text\":\"jour\"}}\n\n",
            "data: {\"type\":\"message_delta\",\"delta\":{\"stop_reason\":\"end_turn\"},\"usage\":{\"output_tokens\":2}}\n\n",
            "data: {\"type\":\"message_stop\"}\n\n",
        );
        let fake = FakeHttp::with_stream(200, sse, 7);
        let rp = resolved_with(&fake, "anthropic", "sk-ant-test");
        let stream = infer_stream(&rp, req(vec![Message::text(Role::User, "salut")]))
            .await
            .expect("stream opens");
        let events = collect(stream).await;

        let texts: Vec<&str> = events
            .iter()
            .filter_map(|e| match e {
                Ok(InferEvent::Delta { text }) => Some(text.as_str()),
                _ => None,
            })
            .collect();
        assert_eq!(texts, vec!["bon", "jour"]);
        let done = events.last().expect("has events");
        match done {
            Ok(InferEvent::Done {
                stop_reason,
                request_id,
                ..
            }) => {
                assert!(matches!(stop_reason, StopReason::EndTurn));
                assert_eq!(request_id.as_deref(), Some("msg_s"));
            }
            other => panic!("expected Done, got {other:?}"),
        }
        let usage_seen = events
            .iter()
            .any(|e| matches!(e, Ok(InferEvent::Usage(u)) if u.input_tokens == 7 && u.output_tokens == 2));
        assert!(usage_seen, "usage emitted from message_delta");
    }

    #[test]
    fn no_system_no_tools_keys_absent_and_extras_first_write_wins() {
        let mut r = req(vec![Message::text(Role::User, "hi")]);
        r.extra
            .params
            .insert("custom_field".into(), serde_json::json!(7));
        r.extra
            .params
            .insert("model".into(), serde_json::json!("evil-override"));
        let body = body_of("real-model", &r, false).expect("body");
        assert!(body.get("system").is_none(), "no system message → no key");
        assert!(body.get("tools").is_none(), "no tools → no key");
        assert!(body.get("stop_sequences").is_none(), "empty stops → no key");
        let mut r2 = req(vec![Message::text(Role::User, "hi")]);
        r2.stop_sequences = vec!["END".to_owned()];
        let body2 = body_of("m", &r2, false).expect("body");
        assert_eq!(body2["stop_sequences"][0], "END", "stops forwarded");
        assert_eq!(body["custom_field"], 7, "extras pass through");
        assert_eq!(body["model"], "real-model", "structural keys win");
    }

    #[test]
    fn image_blocks_http_and_https_accepted_cas_rejected() {
        let names = ToolNameMap::default();
        let ok = content_block(
            &ContentBlock::Image {
                source: "http://example.com/i.png".into(),
                detail: None,
            },
            &names,
        )
        .expect("http URL accepted")
        .expect("emitted");
        assert_eq!(ok["type"], "image");
        let ok2 = content_block(
            &ContentBlock::Image {
                source: "https://example.com/i.png".into(),
                detail: None,
            },
            &names,
        )
        .expect("https URL accepted");
        assert!(ok2.is_some());
        assert!(
            content_block(
                &ContentBlock::Image {
                    source: "blake3:abc".into(),
                    detail: None,
                },
                &names,
            )
            .is_err(),
            "CAS source rejected at v0.1"
        );
    }

    #[test]
    fn parse_response_maps_tool_use_and_thinking_blocks() {
        let fake = FakeHttp::with_json(200, "{}");
        let rp = resolved_with(&fake, "anthropic", "sk-ant-t");
        let body = br#"{"id":"m1","model":"claude-x","stop_reason":"tool_use",
            "content":[{"type":"text","text":"t"},
                       {"type":"tool_use","id":"tu1","name":"add","input":{"a":1}},
                       {"type":"thinking","thinking":"hmm"}],
            "usage":{"input_tokens":1,"output_tokens":2}}"#;
        let resp = parse_response(&rp, body, &ToolNameMap::default()).expect("parse");
        assert_eq!(resp.content.len(), 3);
        assert!(matches!(&resp.content[1],
            ContentBlock::ToolUse { id, name, .. } if id == "tu1" && name == "add"));
        assert!(matches!(&resp.content[2],
            ContentBlock::Thinking { text } if text == "hmm"));
    }

    #[test]
    fn mapper_message_stop_thinking_and_error_arms() {
        // message_stop emits Done immediately (not just at EOF).
        let mut m = AnthropicMapper::new(ToolNameMap::default());
        let out = m.map(r#"{"type":"message_stop"}"#);
        assert!(matches!(out.first(), Some(Ok(InferEvent::Done { .. }))));
        assert!(m.finish().is_empty(), "done only once");

        // thinking_delta maps to Thinking.
        let mut m = AnthropicMapper::new(ToolNameMap::default());
        let out = m.map(
            r#"{"type":"content_block_delta","index":0,"delta":{"type":"thinking_delta","thinking":"th"}}"#,
        );
        assert!(matches!(out.first(),
            Some(Ok(InferEvent::Thinking { text })) if text == "th"));

        // error arm: overloaded → 529 transient · api_error → 500 transient ·
        // other → 400 non-transient · and no synthetic Done after the Err.
        for (etype, status, transient) in [
            ("overloaded_error", 529, true),
            ("api_error", 500, true),
            ("invalid_request_error", 400, false),
        ] {
            let mut m = AnthropicMapper::new(ToolNameMap::default());
            let payload =
                format!(r#"{{"type":"error","error":{{"type":"{etype}","message":"x"}}}}"#);
            let out = m.map(&payload);
            match out.first() {
                Some(Err(e @ ProviderError::Api { status: s, .. })) => {
                    assert_eq!(*s, status, "{etype}");
                    assert_eq!(e.is_transient(), transient, "{etype}");
                }
                other => panic!("expected Err(Api), got {other:?}"),
            }
            assert!(m.finish().is_empty(), "no Done after wire error");
        }
    }

    #[test]
    fn tool_stream_routes_deltas_by_index_and_finish_closes() {
        let mut m = AnthropicMapper::new(ToolNameMap::default());
        m.map(r#"{"type":"content_block_start","index":1,"content_block":{"type":"tool_use","id":"tu_9","name":"add"}}"#);
        let deltas = m.map(
            r#"{"type":"content_block_delta","index":1,"delta":{"type":"input_json_delta","partial_json":"{\"a\":"}}"#,
        );
        match deltas.first() {
            Some(Ok(InferEvent::ToolUseDelta { id, partial_json })) => {
                assert_eq!(id, "tu_9");
                assert_eq!(partial_json, "{\"a\":");
            }
            other => panic!("expected ToolUseDelta, got {other:?}"),
        }
        // No message_stop seen → finish() synthesizes Done.
        let tail = m.finish();
        assert!(matches!(tail.first(), Some(Ok(InferEvent::Done { .. }))));
        assert!(m.finish().is_empty(), "done only once");
    }

    // ── BUG#5 · tool-name sanitization on the anthropic wire (NIKA-463) ──

    #[test]
    fn tool_names_with_colons_are_sanitized_on_send() {
        let mut r = req(vec![Message::text(Role::User, "go")]);
        r.tools = vec![
            ToolDef::new("nika:read", "", serde_json::json!({"type":"object"})),
            ToolDef::new("mcp:git/diff", "", serde_json::json!({"type":"object"})),
        ];
        r.tool_choice = ToolChoice::Specific("mcp:git/diff".into());
        let body = body_of("m", &r, false).expect("body");
        assert_eq!(body["tools"][0]["name"], "nika_read");
        assert_eq!(body["tools"][1]["name"], "mcp_git_diff");
        assert_eq!(body["tool_choice"]["name"], "mcp_git_diff");
    }

    #[test]
    fn resent_tool_use_name_is_sanitized() {
        let mut r = req(vec![Message::new(
            Role::Assistant,
            vec![ContentBlock::ToolUse {
                id: "tu_1".into(),
                name: "nika:read".into(),
                input: serde_json::json!({"path": "x"}),
            }],
        )]);
        r.tools = vec![ToolDef::new(
            "nika:read",
            "",
            serde_json::json!({"type":"object"}),
        )];
        let body = body_of("m", &r, false).expect("body");
        assert_eq!(body["messages"][0]["content"][0]["name"], "nika_read");
    }

    #[test]
    fn parse_reverse_maps_tool_name_to_canonical() {
        let fake = FakeHttp::with_json(200, "{}");
        let rp = resolved_with(&fake, "anthropic", "sk-ant-t");
        let names = ToolNameMap::from_tools(&[ToolDef::new(
            "mcp:git/diff",
            "",
            serde_json::json!({"type":"object"}),
        )]);
        let body = br#"{"id":"m1","model":"claude-x","stop_reason":"tool_use",
            "content":[{"type":"tool_use","id":"tu1","name":"mcp_git_diff","input":{}}],
            "usage":{"input_tokens":1,"output_tokens":1}}"#;
        let resp = parse_response(&rp, body, &names).expect("parse");
        match &resp.content[0] {
            ContentBlock::ToolUse { name, .. } => assert_eq!(name, "mcp:git/diff"),
            other => panic!("expected tool_use, got {other:?}"),
        }
    }

    #[test]
    fn stream_reverse_maps_tool_use_start_name() {
        let names = ToolNameMap::from_tools(&[ToolDef::new(
            "nika:read",
            "",
            serde_json::json!({"type":"object"}),
        )]);
        let mut m = AnthropicMapper::new(names);
        let out = m.map(
            r#"{"type":"content_block_start","index":0,"content_block":{"type":"tool_use","id":"tu_1","name":"nika_read"}}"#,
        );
        assert!(
            matches!(out.first(),
                Some(Ok(InferEvent::ToolUseStart { name, .. })) if name == "nika:read"),
            "ToolUseStart name reverse-mapped to canonical: {out:?}"
        );
    }
}
