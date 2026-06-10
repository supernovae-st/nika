// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! OpenAI-compatible Chat Completions wire adapter.
//!
//! One adapter, twelve profiles: openai · deepseek · mistral · xai · groq ·
//! openrouter (cloud) and ollama · lmstudio · llamacpp · localai · vllm
//! (local · keyless · loopback). The request body sticks to the most
//! conservative compatible subset so the local servers accept it verbatim.

use std::collections::BTreeMap;

use bytes::Bytes;
use nika_kernel::ai::provider::{
    ContentBlock, InferEvent, InferEventStream, InferRequest, InferResponse, ProviderError,
    ResponseFormat, Role, StopReason, TokenUsage, ToolChoice,
};
use nika_kernel::http::{HttpPostDyn, HttpRequest};
use serde_json::{Value, json};

use super::{EventMapper, SseEventStream, gen_ai_system, map_http_err, status_error};
use crate::registry::ResolvedProvider;

/// Single-shot inference.
pub(crate) async fn infer<H>(
    rp: &ResolvedProvider<H>,
    request: InferRequest,
) -> Result<InferResponse, ProviderError>
where
    H: HttpPostDyn + Send + Sync + 'static,
{
    let http_req = build_request(rp, &request, false)?;
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
    parse_response(rp, &resp.body)
}

/// Streaming inference.
pub(crate) async fn infer_stream<H>(
    rp: &ResolvedProvider<H>,
    request: InferRequest,
) -> Result<InferEventStream, ProviderError>
where
    H: HttpPostDyn + Send + Sync + 'static,
{
    let http_req = build_request(rp, &request, true)?;
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
        CompatMapper::default(),
    )))
}

fn wiring_bug() -> ProviderError {
    ProviderError::Other {
        reason: "openai-compat wire reached without an http effect (registry bug)".to_owned(),
    }
}

/// Build the HTTP request (Bearer auth when a key exists — locals are keyless).
fn build_request(
    rp: &ResolvedProvider<impl Sized>,
    req: &InferRequest,
    stream: bool,
) -> Result<HttpRequest, ProviderError> {
    // stream_options is an OpenAI-cloud extension; the 5 keyless local
    // servers (older llama.cpp/LocalAI builds) may 400 on unknown fields,
    // so it is gated to key-bearing (cloud) profiles.
    let body = request_body(&rp.wire_model, req, stream, rp.profile.requires_key)?;
    let bytes = serde_json::to_vec(&body).map_err(|e| ProviderError::Other {
        reason: format!("request serialization failed: {e}"),
    })?;

    let mut headers = BTreeMap::new();
    headers.insert("content-type".to_owned(), "application/json".to_owned());
    if let Some(key) = &rp.key {
        headers.insert(
            "authorization".to_owned(),
            format!("Bearer {}", key.expose()),
        );
    }

    let mut http_req = HttpRequest::post(rp.base_url.clone());
    http_req.headers = headers;
    http_req.body = Some(Bytes::from(bytes));
    Ok(http_req)
}

/// The JSON body (pure — the unit-testable core).
fn request_body(
    model: &str,
    req: &InferRequest,
    stream: bool,
    cloud_extensions: bool,
) -> Result<Value, ProviderError> {
    let mut messages = Vec::new();
    for m in &req.messages {
        push_message(&mut messages, m)?;
    }

    let mut body = json!({ "model": model, "messages": messages, "stream": stream });
    let obj = body.as_object_mut().ok_or_else(|| ProviderError::Other {
        reason: "request body must be an object".to_owned(),
    })?;
    if stream && cloud_extensions {
        obj.insert(
            "stream_options".to_owned(),
            json!({ "include_usage": true }),
        );
    }
    if let Some(t) = req.temperature {
        obj.insert("temperature".to_owned(), json!(t));
    }
    if let Some(mt) = req.max_tokens {
        obj.insert("max_tokens".to_owned(), json!(mt));
    }
    if !req.stop_sequences.is_empty() {
        obj.insert("stop".to_owned(), json!(req.stop_sequences));
    }
    if !req.tools.is_empty() {
        let tools: Vec<Value> = req
            .tools
            .iter()
            .map(|t| {
                json!({ "type": "function", "function": {
                    "name": t.name, "description": t.description, "parameters": t.parameters,
                }})
            })
            .collect();
        obj.insert("tools".to_owned(), Value::Array(tools));
        let choice = match &req.tool_choice {
            ToolChoice::Required => json!("required"),
            ToolChoice::None => json!("none"),
            ToolChoice::Specific(name) => {
                json!({ "type": "function", "function": { "name": name } })
            }
            ToolChoice::Auto | _ => json!("auto"),
        };
        obj.insert("tool_choice".to_owned(), choice);
    }
    match &req.response_format {
        ResponseFormat::Json => {
            obj.insert(
                "response_format".to_owned(),
                json!({ "type": "json_object" }),
            );
        }
        ResponseFormat::JsonSchema(schema) => {
            obj.insert(
                "response_format".to_owned(),
                json!({ "type": "json_schema", "json_schema": {
                    "name": "response", "schema": schema, "strict": true,
                }}),
            );
        }
        ResponseFormat::Text | _ => {}
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

/// Kernel message → Chat Completions message(s). `ToolResult` blocks become
/// their own `role:"tool"` messages (the dialect's shape).
fn push_message(
    out: &mut Vec<Value>,
    m: &nika_kernel::ai::provider::Message,
) -> Result<(), ProviderError> {
    let role = match m.role {
        Role::System => "system",
        Role::Assistant => "assistant",
        _ => "user",
    };

    let mut texts = Vec::new();
    let mut parts = Vec::new();
    let mut tool_calls = Vec::new();
    let mut has_image = false;

    for block in &m.content {
        match block {
            ContentBlock::Text { text } => {
                texts.push(text.clone());
                parts.push(json!({ "type": "text", "text": text }));
            }
            ContentBlock::Image { source, .. } => {
                if !(source.starts_with("http://") || source.starts_with("https://")) {
                    return Err(ProviderError::Other {
                        reason:
                            "image source must be a URL at v0.1 (CAS sources land with nika-media)"
                                .to_owned(),
                    });
                }
                has_image = true;
                parts.push(json!({ "type": "image_url", "image_url": { "url": source } }));
            }
            ContentBlock::ToolUse { id, name, input } => {
                tool_calls.push(json!({ "id": id, "type": "function", "function": {
                    "name": name, "arguments": input.to_string(),
                }}));
            }
            ContentBlock::ToolResult {
                tool_use_id,
                content,
                ..
            } => out.push(json!({
                "role": "tool", "tool_call_id": tool_use_id, "content": content,
            })),
            _ => {}
        }
    }

    if !tool_calls.is_empty() {
        let content = if texts.is_empty() {
            Value::Null
        } else {
            Value::String(texts.join("\n"))
        };
        out.push(json!({ "role": role, "content": content, "tool_calls": tool_calls }));
    } else if has_image {
        out.push(json!({ "role": role, "content": parts }));
    } else if !texts.is_empty() {
        out.push(json!({ "role": role, "content": texts.join("\n") }));
    }
    Ok(())
}

/// Parse a 2xx Chat Completions response.
fn parse_response(
    rp: &ResolvedProvider<impl Sized>,
    body: &[u8],
) -> Result<InferResponse, ProviderError> {
    let v: Value = serde_json::from_slice(body).map_err(|e| ProviderError::Other {
        reason: format!("openai-compat response is not JSON: {e}"),
    })?;
    let msg = v.pointer("/choices/0/message");

    let mut content = Vec::new();
    if let Some(text) = msg
        .and_then(|m| m.pointer("/content"))
        .and_then(Value::as_str)
        && !text.is_empty()
    {
        content.push(ContentBlock::Text {
            text: text.to_owned(),
        });
    }
    for tc in msg
        .and_then(|m| m.pointer("/tool_calls"))
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
    {
        let args = tc
            .pointer("/function/arguments")
            .and_then(Value::as_str)
            .unwrap_or("{}");
        let input = serde_json::from_str(args).unwrap_or_else(|_| Value::String(args.to_owned()));
        content.push(ContentBlock::ToolUse {
            id: super::str_at(tc, "/id"),
            name: super::str_at(tc, "/function/name"),
            input,
        });
    }

    let mut usage = TokenUsage::new(
        v.pointer("/usage/prompt_tokens")
            .and_then(Value::as_u64)
            .unwrap_or_default(),
        v.pointer("/usage/completion_tokens")
            .and_then(Value::as_u64)
            .unwrap_or_default(),
    );
    usage.cache_read_tokens = v
        .pointer("/usage/prompt_tokens_details/cached_tokens")
        .and_then(Value::as_u64);
    usage.reasoning_tokens = v
        .pointer("/usage/completion_tokens_details/reasoning_tokens")
        .and_then(Value::as_u64);

    let raw_finish = v
        .pointer("/choices/0/finish_reason")
        .and_then(Value::as_str);
    let mut resp = InferResponse::new(content, usage, map_finish(raw_finish));
    resp.request_id = v
        .pointer("/id")
        .and_then(Value::as_str)
        .map(ToOwned::to_owned);
    resp.finish_reason_raw = raw_finish.map(ToOwned::to_owned);
    resp.gen_ai.system = gen_ai_system(rp.profile.id);
    resp.gen_ai.response_id = resp.request_id.clone();
    resp.gen_ai.response_model = v
        .pointer("/model")
        .and_then(Value::as_str)
        .map(ToOwned::to_owned);
    Ok(resp)
}

fn map_finish(raw: Option<&str>) -> StopReason {
    match raw {
        Some("stop") => StopReason::EndTurn,
        Some("length") => StopReason::MaxTokens,
        Some("tool_calls") => StopReason::ToolUse,
        Some("content_filter") => StopReason::ContentFilter,
        Some(other) => StopReason::Unknown(other.to_owned()),
        None => StopReason::Unknown("missing-finish-reason".to_owned()),
    }
}

/// Chat Completions SSE → `InferEvent` translator.
#[derive(Default)]
struct CompatMapper {
    request_id: Option<String>,
    finish: Option<String>,
    /// delta index → tool call id.
    tools: BTreeMap<u64, String>,
    done_sent: bool,
}

impl CompatMapper {
    fn done(&mut self) -> InferEvent {
        self.done_sent = true;
        InferEvent::Done {
            stop_reason: map_finish(self.finish.as_deref()),
            request_id: self.request_id.clone(),
            finish_reason_raw: self.finish.clone(),
        }
    }
}

impl EventMapper for CompatMapper {
    fn map(&mut self, payload: &str) -> Vec<Result<InferEvent, ProviderError>> {
        if payload.trim() == "[DONE]" {
            return if self.done_sent {
                Vec::new()
            } else {
                vec![Ok(self.done())]
            };
        }
        let Ok(v) = serde_json::from_str::<Value>(payload) else {
            return Vec::new();
        };
        // Anything after [DONE] would violate « Done is terminal ».
        if self.done_sent {
            return Vec::new();
        }
        let mut out = Vec::new();
        if self.request_id.is_none() {
            self.request_id = v
                .pointer("/id")
                .and_then(Value::as_str)
                .map(ToOwned::to_owned);
        }
        if let Some(f) = v
            .pointer("/choices/0/finish_reason")
            .and_then(Value::as_str)
        {
            self.finish = Some(f.to_owned());
        }
        if let Some(text) = v
            .pointer("/choices/0/delta/content")
            .and_then(Value::as_str)
            && !text.is_empty()
        {
            out.push(Ok(InferEvent::Delta {
                text: text.to_owned(),
            }));
        }
        for tc in v
            .pointer("/choices/0/delta/tool_calls")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
        {
            let index = tc.pointer("/index").and_then(Value::as_u64).unwrap_or(0);
            if let Some(id) = tc.pointer("/id").and_then(Value::as_str) {
                self.tools.insert(index, id.to_owned());
                out.push(Ok(InferEvent::ToolUseStart {
                    id: id.to_owned(),
                    name: super::str_at(tc, "/function/name"),
                }));
            }
            if let Some(args) = tc.pointer("/function/arguments").and_then(Value::as_str)
                && !args.is_empty()
            {
                out.push(Ok(InferEvent::ToolUseDelta {
                    id: self.tools.get(&index).cloned().unwrap_or_default(),
                    partial_json: args.to_owned(),
                }));
            }
        }
        if let Some(u) = v.pointer("/usage")
            && !u.is_null()
        {
            out.push(Ok(InferEvent::Usage(TokenUsage::new(
                u.pointer("/prompt_tokens")
                    .and_then(Value::as_u64)
                    .unwrap_or_default(),
                u.pointer("/completion_tokens")
                    .and_then(Value::as_u64)
                    .unwrap_or_default(),
            ))));
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
        InferRequest::new("test-model", messages)
    }

    #[tokio::test]
    async fn infer_shapes_request_with_bearer_and_parses() {
        let fake = FakeHttp::with_json(
            200,
            r#"{"id":"cc_1","model":"llama-3.3-70b",
                "choices":[{"message":{"content":"salut"},"finish_reason":"stop"}],
                "usage":{"prompt_tokens":9,"completion_tokens":3}}"#,
        );
        let rp = resolved_with(&fake, "groq", "gsk-test");
        let resp = infer(&rp, req(vec![Message::text(Role::User, "hi")]))
            .await
            .expect("infer ok");

        assert!(matches!(resp.stop_reason, StopReason::EndTurn));
        assert_eq!(resp.usage.input_tokens, 9);
        assert_eq!(
            resp.gen_ai.system,
            nika_kernel::genai::GenAiSystem::OpenAiCompatible
        );

        let sent = fake.captured();
        assert_eq!(
            sent[0].url,
            "https://api.groq.com/openai/v1/chat/completions"
        );
        assert_eq!(
            sent[0].headers.get("authorization").unwrap(),
            "Bearer gsk-test"
        );
        let body: Value = serde_json::from_slice(sent[0].body.as_ref().unwrap()).unwrap();
        assert_eq!(body["messages"][0]["role"], "user");
        assert_eq!(body["messages"][0]["content"], "hi");
    }

    #[tokio::test]
    async fn local_profile_sends_no_auth_header() {
        let fake = FakeHttp::with_json(
            200,
            r#"{"choices":[{"message":{"content":"ok"},"finish_reason":"stop"}],"usage":{}}"#,
        );
        let rp = resolved_with(&fake, "ollama", "");
        let _ = infer(&rp, req(vec![Message::text(Role::User, "x")]))
            .await
            .expect("ok");
        let sent = fake.captured();
        assert!(
            !sent[0].headers.contains_key("authorization"),
            "keyless local call"
        );
        assert!(sent[0].url.starts_with("http://127.0.0.1:11434"));
    }

    #[test]
    fn tools_and_response_format_shape() {
        let mut r = req(vec![Message::text(Role::User, "calcule")]);
        r.tools = vec![ToolDef::new("add", "adds", json!({"type":"object"}))];
        r.tool_choice = ToolChoice::Specific("add".into());
        r.response_format = ResponseFormat::JsonSchema(json!({"type":"object"}));
        let body = request_body("m", &r, false, true).expect("body");
        assert_eq!(body["tools"][0]["type"], "function");
        assert_eq!(body["tools"][0]["function"]["name"], "add");
        assert_eq!(body["tool_choice"]["function"]["name"], "add");
        assert_eq!(body["response_format"]["type"], "json_schema");
        assert_eq!(body["response_format"]["json_schema"]["strict"], true);
    }

    #[test]
    fn tool_round_trip_uses_tool_role_messages() {
        let messages = vec![
            Message::new(
                Role::Assistant,
                vec![ContentBlock::ToolUse {
                    id: "call_1".into(),
                    name: "add".into(),
                    input: json!({"a":1}),
                }],
            ),
            Message::new(
                Role::User,
                vec![ContentBlock::ToolResult {
                    tool_use_id: "call_1".into(),
                    content: "2".into(),
                    is_error: false,
                }],
            ),
        ];
        let body = request_body("m", &req(messages), false, true).expect("body");
        assert_eq!(body["messages"][0]["tool_calls"][0]["id"], "call_1");
        assert_eq!(
            body["messages"][0]["tool_calls"][0]["function"]["arguments"],
            "{\"a\":1}"
        );
        assert_eq!(body["messages"][1]["role"], "tool");
        assert_eq!(body["messages"][1]["tool_call_id"], "call_1");
    }

    #[test]
    fn parse_maps_tool_calls_and_finish_reasons() {
        let fake = FakeHttp::with_json(200, "{}");
        let rp = resolved_with(&fake, "openai", "sk-test");
        let body = br#"{"id":"cc_2",
            "choices":[{"message":{"content":null,
                "tool_calls":[{"id":"call_9","function":{"name":"add","arguments":"{\"a\":1}"}}]},
                "finish_reason":"tool_calls"}],
            "usage":{"prompt_tokens":1,"completion_tokens":2}}"#;
        let resp = parse_response(&rp, body).expect("parse");
        assert!(matches!(resp.stop_reason, StopReason::ToolUse));
        match &resp.content[0] {
            ContentBlock::ToolUse { id, name, input } => {
                assert_eq!(id, "call_9");
                assert_eq!(name, "add");
                assert_eq!(input["a"], 1);
            }
            other => panic!("expected tool_use, got {other:?}"),
        }
        assert!(matches!(map_finish(Some("length")), StopReason::MaxTokens));
        assert!(matches!(
            map_finish(Some("content_filter")),
            StopReason::ContentFilter
        ));
    }

    #[test]
    fn roles_map_exactly_and_no_stop_key_when_empty() {
        let messages = vec![
            Message::text(Role::System, "sys"),
            Message::text(Role::User, "usr"),
            Message::text(Role::Assistant, "asst"),
        ];
        let body = request_body("m", &req(messages), false, true).expect("body");
        assert_eq!(body["messages"][0]["role"], "system");
        assert_eq!(body["messages"][1]["role"], "user");
        assert_eq!(body["messages"][2]["role"], "assistant");
        assert!(body.get("stop").is_none(), "no stop_sequences → no key");
    }

    #[test]
    fn stream_options_gated_to_cloud_and_stream_only() {
        let r = req(vec![Message::text(Role::User, "x")]);
        let cloud_nostream = request_body("m", &r, false, true).expect("body");
        assert!(
            cloud_nostream.get("stream_options").is_none(),
            "non-stream → no stream_options"
        );
        let local_stream = request_body("m", &r, true, false).expect("body");
        assert!(
            local_stream.get("stream_options").is_none(),
            "local profile → no stream_options"
        );
        let cloud_stream = request_body("m", &r, true, true).expect("body");
        assert_eq!(cloud_stream["stream_options"]["include_usage"], true);
    }

    #[test]
    fn extras_first_write_wins_and_custom_passes() {
        let mut r = req(vec![Message::text(Role::User, "x")]);
        r.extra
            .params
            .insert("custom_field".into(), serde_json::json!("v"));
        r.extra
            .params
            .insert("model".into(), serde_json::json!("evil"));
        let body = request_body("real", &r, false, true).expect("body");
        assert_eq!(body["custom_field"], "v");
        assert_eq!(body["model"], "real", "structural keys win");
    }

    #[test]
    fn image_parts_http_and_https_accepted_cas_rejected() {
        let msg = Message::new(
            Role::User,
            vec![
                ContentBlock::Text {
                    text: "look".into(),
                },
                ContentBlock::Image {
                    source: "http://example.com/i.png".into(),
                    detail: None,
                },
            ],
        );
        let body = request_body("m", &req(vec![msg]), false, true).expect("body");
        let parts = body["messages"][0]["content"]
            .as_array()
            .expect("multimodal parts array");
        assert!(parts.iter().any(|p| p["type"] == "image_url"));

        let bad = Message::new(
            Role::User,
            vec![ContentBlock::Image {
                source: "blake3:abc".into(),
                detail: None,
            }],
        );
        assert!(request_body("m", &req(vec![bad]), false, true).is_err());
    }

    #[test]
    fn mapper_tool_args_emitted_and_finish_synthesizes_done() {
        let mut m = CompatMapper::default();
        let out = m.map(
            r#"{"choices":[{"delta":{"tool_calls":[{"index":0,"id":"c1","function":{"name":"add","arguments":"{partial"}}]},"finish_reason":null}]}"#,
        );
        assert!(
            out.iter().any(|e| matches!(e,
                Ok(InferEvent::ToolUseDelta { partial_json, .. }) if partial_json == "{partial")),
            "non-empty args emit a delta: {out:?}"
        );
        // EOF without [DONE] → finish() synthesizes exactly one Done.
        let tail = m.finish();
        assert!(matches!(tail.first(), Some(Ok(InferEvent::Done { .. }))));
        assert!(m.finish().is_empty(), "done only once");
    }

    #[tokio::test]
    async fn stream_maps_deltas_done_and_usage() {
        let sse = concat!(
            "data: {\"id\":\"cc_s\",\"choices\":[{\"delta\":{\"content\":\"bon\"},\"finish_reason\":null}]}\n\n",
            "data: {\"choices\":[{\"delta\":{\"content\":\"soir\"},\"finish_reason\":null}]}\n\n",
            "data: {\"choices\":[{\"delta\":{},\"finish_reason\":\"stop\"}]}\n\n",
            "data: {\"choices\":[],\"usage\":{\"prompt_tokens\":4,\"completion_tokens\":2}}\n\n",
            "data: [DONE]\n\n",
        );
        let fake = FakeHttp::with_stream(200, sse, 11);
        let rp = resolved_with(&fake, "openai", "sk-test");
        let stream = infer_stream(&rp, req(vec![Message::text(Role::User, "x")]))
            .await
            .expect("opens");
        let events = collect(stream).await;

        let text: String = events
            .iter()
            .filter_map(|e| match e {
                Ok(InferEvent::Delta { text }) => Some(text.clone()),
                _ => None,
            })
            .collect();
        assert_eq!(text, "bonsoir");
        let usage_seen = events
            .iter()
            .any(|e| matches!(e, Ok(InferEvent::Usage(u)) if u.input_tokens == 4));
        assert!(usage_seen);
        match events.last() {
            Some(Ok(InferEvent::Done {
                stop_reason,
                request_id,
                ..
            })) => {
                assert!(matches!(stop_reason, StopReason::EndTurn));
                assert_eq!(request_id.as_deref(), Some("cc_s"));
            }
            other => panic!("expected Done last, got {other:?}"),
        }
    }
}
