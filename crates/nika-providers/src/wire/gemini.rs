// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Google Gemini `generateContent` wire adapter (s8.6 — closes 14/14).
//!
//! Shape differences vs the other two wires, all absorbed here:
//! - the profile `base_url` is a **stem** (`…/v1beta`) — the model and the
//!   method are path segments (`/models/{model}:generateContent` ·
//!   `:streamGenerateContent?alt=sse` for SSE);
//! - auth is the `x-goog-api-key` header (not Bearer);
//! - roles are `user`/`model` (no `assistant`), system prompts ride a
//!   dedicated `systemInstruction` field;
//! - tool calls carry **no id** — ids are synthesized (`{name}-{n}`) and
//!   tool results are name-keyed (`functionResponse`), the name recovered
//!   by looking back through the message history;
//! - streaming chunks are full `GenerateContentResponse` objects and the
//!   stream has **no terminal sentinel** — EOF closes it (the shared
//!   `EventMapper::finish` synthesizes the `Done`).

use std::collections::BTreeMap;

use bytes::Bytes;
use nika_kernel::ai::provider::{
    ContentBlock, InferEvent, InferEventStream, InferRequest, InferResponse, Message,
    ProviderError, ResponseFormat, Role, StopReason, TokenUsage, ToolChoice,
};
use nika_kernel::http::{HttpPostDyn, HttpRequest};
use serde_json::{Value, json};

use super::{EventMapper, SseEventStream, gen_ai_system, map_http_err, status_error, str_at};
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
        GeminiMapper::default(),
    )))
}

fn wiring_bug() -> ProviderError {
    ProviderError::Other {
        reason: "gemini wire reached without an http effect (registry bug)".to_owned(),
    }
}

/// Endpoint from the stem: `{base}/models/{model}:method[?alt=sse]`.
fn endpoint(base_url: &str, model: &str, stream: bool) -> String {
    let base = base_url.trim_end_matches('/');
    if stream {
        format!("{base}/models/{model}:streamGenerateContent?alt=sse")
    } else {
        format!("{base}/models/{model}:generateContent")
    }
}

/// Build the HTTP request (x-goog-api-key + JSON body).
fn build_request(
    rp: &ResolvedProvider<impl Sized>,
    req: &InferRequest,
    stream: bool,
) -> Result<HttpRequest, ProviderError> {
    let key = rp.key.as_ref().ok_or_else(|| ProviderError::AuthFailed {
        reason: "gemini requires an API key".to_owned(),
    })?;
    let body = request_body(req)?;
    let bytes = serde_json::to_vec(&body).map_err(|e| ProviderError::Other {
        reason: format!("request serialization failed: {e}"),
    })?;

    let mut headers = BTreeMap::new();
    headers.insert("x-goog-api-key".to_owned(), key.expose().to_owned());
    headers.insert("content-type".to_owned(), "application/json".to_owned());

    let mut http_req = HttpRequest::post(endpoint(&rp.base_url, &rp.wire_model, stream));
    http_req.headers = headers;
    http_req.body = Some(Bytes::from(bytes));
    Ok(http_req)
}

/// The JSON body (pure — the unit-testable core). The model is a URL path
/// segment, not a body field.
fn request_body(req: &InferRequest) -> Result<Value, ProviderError> {
    let mut system = String::new();
    let mut contents = Vec::new();
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
        push_content(&mut contents, m, &req.messages)?;
    }

    let mut body = json!({ "contents": contents });
    let obj = body.as_object_mut().ok_or_else(|| ProviderError::Other {
        reason: "request body must be an object".to_owned(),
    })?;
    if !system.is_empty() {
        obj.insert(
            "systemInstruction".to_owned(),
            json!({ "parts": [{ "text": system }] }),
        );
    }

    let mut gen_config = serde_json::Map::new();
    if let Some(t) = req.temperature {
        gen_config.insert("temperature".to_owned(), json!(t));
    }
    if let Some(mt) = req.max_tokens {
        gen_config.insert("maxOutputTokens".to_owned(), json!(mt));
    }
    if !req.stop_sequences.is_empty() {
        gen_config.insert("stopSequences".to_owned(), json!(req.stop_sequences));
    }
    if let Some(budget) = req.thinking_budget {
        gen_config.insert(
            "thinkingConfig".to_owned(),
            json!({ "thinkingBudget": budget }),
        );
    }
    match &req.response_format {
        ResponseFormat::Json => {
            gen_config.insert("responseMimeType".to_owned(), json!("application/json"));
        }
        ResponseFormat::JsonSchema(schema) => {
            gen_config.insert("responseMimeType".to_owned(), json!("application/json"));
            // Gemini takes an OpenAPI-style subset; simple JSON Schemas pass
            // through — complex keywords ($ref · oneOf) are the caller's risk.
            gen_config.insert("responseSchema".to_owned(), schema.clone());
        }
        ResponseFormat::Text | _ => {}
    }
    if !gen_config.is_empty() {
        obj.insert("generationConfig".to_owned(), Value::Object(gen_config));
    }

    if !req.tools.is_empty() {
        let decls: Vec<Value> = req
            .tools
            .iter()
            .map(|t| {
                json!({
                    "name": t.name,
                    "description": t.description,
                    "parameters": t.parameters,
                })
            })
            .collect();
        obj.insert(
            "tools".to_owned(),
            json!([{ "functionDeclarations": decls }]),
        );
        let mode = match &req.tool_choice {
            ToolChoice::Required => json!({ "mode": "ANY" }),
            ToolChoice::None => json!({ "mode": "NONE" }),
            ToolChoice::Specific(name) => {
                json!({ "mode": "ANY", "allowedFunctionNames": [name] })
            }
            _ => json!({ "mode": "AUTO" }),
        };
        obj.insert(
            "toolConfig".to_owned(),
            json!({ "functionCallingConfig": mode }),
        );
    }

    // Provider extras never override the structural keys this adapter set —
    // first-write-wins (same contract as the other wires).
    for (k, v) in &req.extra.params {
        if !obj.contains_key(k) {
            obj.insert(k.clone(), v.clone());
        }
    }
    Ok(body)
}

/// One kernel message → one `contents[]` entry. Tool results become
/// `functionResponse` parts (name recovered from the prior `ToolUse`).
fn push_content(
    out: &mut Vec<Value>,
    m: &Message,
    history: &[Message],
) -> Result<(), ProviderError> {
    let role = if matches!(m.role, Role::Assistant) {
        "model"
    } else {
        "user"
    };
    let mut parts = Vec::new();
    for block in &m.content {
        match block {
            ContentBlock::Text { text } => parts.push(json!({ "text": text })),
            ContentBlock::ToolUse { name, input, .. } => {
                parts.push(json!({ "functionCall": { "name": name, "args": input } }));
            }
            ContentBlock::ToolResult {
                tool_use_id,
                content,
                is_error,
            } => {
                let name = tool_name_for(history, tool_use_id);
                let response = if *is_error {
                    json!({ "error": content })
                } else {
                    json!({ "output": content })
                };
                parts.push(json!({
                    "functionResponse": { "name": name, "response": response }
                }));
            }
            ContentBlock::Image { .. } => {
                return Err(ProviderError::Other {
                    reason: "gemini image input lands with nika-media (inlineData) · \
                             use anthropic/… or openai/… for URL images at v0.1"
                        .to_owned(),
                });
            }
            _ => {}
        }
    }
    if !parts.is_empty() {
        out.push(json!({ "role": role, "parts": parts }));
    }
    Ok(())
}

/// Recover a tool name from the `ToolUse` block carrying this id (Gemini
/// `functionResponse` is name-keyed · ids are ours, synthesized).
fn tool_name_for(history: &[Message], tool_use_id: &str) -> String {
    for m in history {
        for block in &m.content {
            if let ContentBlock::ToolUse { id, name, .. } = block
                && id == tool_use_id
            {
                return name.clone();
            }
        }
    }
    // Fall back to the id minus our `-N` suffix (synthesized shape).
    match tool_use_id.rsplit_once('-') {
        Some((name, n)) if n.chars().all(|c| c.is_ascii_digit()) => name.to_owned(),
        _ => tool_use_id.to_owned(),
    }
}

/// Parse a 2xx `GenerateContentResponse`.
fn parse_response(
    rp: &ResolvedProvider<impl Sized>,
    body: &[u8],
) -> Result<InferResponse, ProviderError> {
    let v: Value = serde_json::from_slice(body).map_err(|e| ProviderError::Other {
        reason: format!("gemini response is not JSON: {e}"),
    })?;

    let mut content = Vec::new();
    let mut call_n = 0u32;
    for part in v
        .pointer("/candidates/0/content/parts")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
    {
        collect_part(part, &mut content, &mut call_n);
    }

    let usage = usage_from(&v);
    let raw_finish = v
        .pointer("/candidates/0/finishReason")
        .and_then(Value::as_str);
    let has_call = content
        .iter()
        .any(|b| matches!(b, ContentBlock::ToolUse { .. }));
    let mut resp = InferResponse::new(content, usage, map_finish(raw_finish, has_call));
    resp.request_id = v
        .pointer("/responseId")
        .and_then(Value::as_str)
        .map(ToOwned::to_owned);
    resp.finish_reason_raw = raw_finish.map(ToOwned::to_owned);
    resp.gen_ai.system = gen_ai_system(rp.profile.id);
    resp.gen_ai.response_id = resp.request_id.clone();
    resp.gen_ai.response_model = v
        .pointer("/modelVersion")
        .and_then(Value::as_str)
        .map(ToOwned::to_owned)
        .or_else(|| Some(rp.wire_model.clone()));
    Ok(resp)
}

/// One response part → kernel content block(s).
fn collect_part(part: &Value, content: &mut Vec<ContentBlock>, call_n: &mut u32) {
    if let Some(text) = part.pointer("/text").and_then(Value::as_str) {
        if part.pointer("/thought").and_then(Value::as_bool) == Some(true) {
            content.push(ContentBlock::Thinking {
                text: text.to_owned(),
            });
        } else if !text.is_empty() {
            content.push(ContentBlock::Text {
                text: text.to_owned(),
            });
        }
    }
    if let Some(fc) = part.pointer("/functionCall") {
        let name = str_at(fc, "/name");
        content.push(ContentBlock::ToolUse {
            id: format!("{name}-{n}", n = *call_n),
            name,
            input: fc.pointer("/args").cloned().unwrap_or(Value::Null),
        });
        *call_n += 1;
    }
}

fn usage_from(v: &Value) -> TokenUsage {
    let mut usage = TokenUsage::new(
        v.pointer("/usageMetadata/promptTokenCount")
            .and_then(Value::as_u64)
            .unwrap_or_default(),
        v.pointer("/usageMetadata/candidatesTokenCount")
            .and_then(Value::as_u64)
            .unwrap_or_default(),
    );
    usage.thinking_tokens = v
        .pointer("/usageMetadata/thoughtsTokenCount")
        .and_then(Value::as_u64);
    usage.cache_read_tokens = v
        .pointer("/usageMetadata/cachedContentTokenCount")
        .and_then(Value::as_u64);
    usage.total_tokens = v
        .pointer("/usageMetadata/totalTokenCount")
        .and_then(Value::as_u64);
    usage
}

fn map_finish(raw: Option<&str>, has_function_call: bool) -> StopReason {
    match raw {
        Some("STOP") if has_function_call => StopReason::ToolUse,
        Some("STOP") => StopReason::EndTurn,
        Some("MAX_TOKENS") => StopReason::MaxTokens,
        Some("SAFETY" | "PROHIBITED_CONTENT" | "BLOCKLIST") => StopReason::ContentFilter,
        Some(other) => StopReason::Unknown(other.to_owned()),
        None => StopReason::Unknown("missing-finish-reason".to_owned()),
    }
}

/// `streamGenerateContent?alt=sse` → `InferEvent` translator. Each SSE
/// payload is a full `GenerateContentResponse` chunk; there is no terminal
/// sentinel — EOF triggers `finish()`.
#[derive(Default)]
struct GeminiMapper {
    request_id: Option<String>,
    finish: Option<String>,
    saw_function_call: bool,
    usage_sent: bool,
    call_n: u32,
    done_sent: bool,
}

impl GeminiMapper {
    fn done(&mut self) -> InferEvent {
        self.done_sent = true;
        InferEvent::Done {
            stop_reason: map_finish(self.finish.as_deref(), self.saw_function_call),
            request_id: self.request_id.clone(),
            finish_reason_raw: self.finish.clone(),
        }
    }
}

impl EventMapper for GeminiMapper {
    fn map(&mut self, payload: &str) -> Vec<Result<InferEvent, ProviderError>> {
        let Ok(v) = serde_json::from_str::<Value>(payload) else {
            return Vec::new();
        };
        if self.done_sent {
            return Vec::new();
        }
        let mut out = Vec::new();
        if self.request_id.is_none() {
            self.request_id = v
                .pointer("/responseId")
                .and_then(Value::as_str)
                .map(ToOwned::to_owned);
        }
        if let Some(err) = v.pointer("/error") {
            self.done_sent = true;
            let status = err.pointer("/code").and_then(Value::as_u64).unwrap_or(400);
            out.push(Err(ProviderError::Api {
                status: u16::try_from(status).unwrap_or(400),
                message: str_at(err, "/message"),
            }));
            return out;
        }
        for part in v
            .pointer("/candidates/0/content/parts")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
        {
            if let Some(text) = part.pointer("/text").and_then(Value::as_str) {
                let is_thought = part.pointer("/thought").and_then(Value::as_bool) == Some(true);
                if is_thought {
                    out.push(Ok(InferEvent::Thinking {
                        text: text.to_owned(),
                    }));
                } else if !text.is_empty() {
                    out.push(Ok(InferEvent::Delta {
                        text: text.to_owned(),
                    }));
                }
            }
            if let Some(fc) = part.pointer("/functionCall") {
                let name = str_at(fc, "/name");
                let id = format!("{name}-{n}", n = self.call_n);
                self.call_n += 1;
                self.saw_function_call = true;
                out.push(Ok(InferEvent::ToolUseStart {
                    id: id.clone(),
                    name,
                }));
                let args = fc.pointer("/args").cloned().unwrap_or(Value::Null);
                out.push(Ok(InferEvent::ToolUseDelta {
                    id,
                    partial_json: args.to_string(),
                }));
            }
        }
        if let Some(f) = v
            .pointer("/candidates/0/finishReason")
            .and_then(Value::as_str)
        {
            self.finish = Some(f.to_owned());
            if !self.usage_sent && v.pointer("/usageMetadata").is_some() {
                self.usage_sent = true;
                out.push(Ok(InferEvent::Usage(usage_from(&v))));
            }
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
    use nika_kernel::ai::provider::ToolDef;

    use super::*;
    use crate::test_support::{FakeHttp, collect, resolved_with};

    fn req(messages: Vec<Message>) -> InferRequest {
        InferRequest::new("gemini-2.5-flash", messages)
    }

    #[tokio::test]
    async fn infer_shapes_url_auth_and_parses() {
        let fake = FakeHttp::with_json(
            200,
            r#"{"responseId":"g_1","modelVersion":"gemini-2.5-flash",
                "candidates":[{"content":{"role":"model","parts":[{"text":"bonjour"}]},
                               "finishReason":"STOP"}],
                "usageMetadata":{"promptTokenCount":7,"candidatesTokenCount":3,
                                 "totalTokenCount":10}}"#,
        );
        let rp = resolved_with(&fake, "gemini/flash-25", "g-key");
        let mut r = req(vec![
            Message::text(Role::System, "sois bref"),
            Message::text(Role::User, "salut"),
        ]);
        r.temperature = Some(0.5);
        r.max_tokens = Some(128);

        let resp = infer(&rp, r).await.expect("infer ok");

        assert!(matches!(resp.stop_reason, StopReason::EndTurn));
        assert_eq!(resp.usage.input_tokens, 7);
        assert_eq!(resp.usage.total_tokens, Some(10));
        assert_eq!(resp.request_id.as_deref(), Some("g_1"));
        assert_eq!(
            resp.gen_ai.response_model.as_deref(),
            Some("gemini-2.5-flash")
        );
        assert_eq!(resp.gen_ai.system, nika_kernel::genai::GenAiSystem::Google);

        let sent = fake.captured();
        // nickname flash-25 resolved through the catalog row to the wire id.
        assert!(
            sent[0].url.starts_with(
                "https://generativelanguage.googleapis.com/v1beta/models/gemini-2.5-flash"
            ),
            "stem + model path: {}",
            sent[0].url
        );
        assert!(sent[0].url.ends_with(":generateContent"));
        assert_eq!(sent[0].headers.get("x-goog-api-key").unwrap(), "g-key");
        assert!(!sent[0].headers.contains_key("authorization"));
        let body: Value = serde_json::from_slice(sent[0].body.as_ref().unwrap()).unwrap();
        assert_eq!(body["systemInstruction"]["parts"][0]["text"], "sois bref");
        assert_eq!(body["contents"][0]["role"], "user");
        assert_eq!(body["contents"][0]["parts"][0]["text"], "salut");
        assert_eq!(body["generationConfig"]["temperature"], 0.5);
        assert_eq!(body["generationConfig"]["maxOutputTokens"], 128);
        assert!(body.get("model").is_none(), "model is a path segment");
    }

    #[test]
    fn tools_schema_and_choice_modes() {
        let mut r = req(vec![Message::text(Role::User, "calcule")]);
        r.tools = vec![ToolDef::new("add", "adds", json!({"type":"object"}))];
        r.tool_choice = ToolChoice::Specific("add".into());
        r.response_format = ResponseFormat::JsonSchema(json!({"type":"object"}));
        let body = request_body(&r).expect("body");

        // choice-mode table (kills the per-arm deletions)
        let mut r2 = req(vec![Message::text(Role::User, "x")]);
        r2.tools = vec![ToolDef::new("add", "adds", json!({"type":"object"}))];
        r2.tool_choice = ToolChoice::Required;
        let b2 = request_body(&r2).expect("body");
        assert_eq!(b2["toolConfig"]["functionCallingConfig"]["mode"], "ANY");
        assert!(
            b2["toolConfig"]["functionCallingConfig"]
                .get("allowedFunctionNames")
                .is_none(),
            "Required has no allow-list"
        );
        r2.tool_choice = ToolChoice::None;
        let b3 = request_body(&r2).expect("body");
        assert_eq!(b3["toolConfig"]["functionCallingConfig"]["mode"], "NONE");
        r2.tool_choice = ToolChoice::Auto;
        let b4 = request_body(&r2).expect("body");
        assert_eq!(b4["toolConfig"]["functionCallingConfig"]["mode"], "AUTO");
        assert_eq!(body["tools"][0]["functionDeclarations"][0]["name"], "add");
        assert_eq!(body["toolConfig"]["functionCallingConfig"]["mode"], "ANY");
        assert_eq!(
            body["toolConfig"]["functionCallingConfig"]["allowedFunctionNames"][0],
            "add"
        );
        assert_eq!(
            body["generationConfig"]["responseMimeType"],
            "application/json"
        );
        assert!(body["generationConfig"]["responseSchema"].is_object());
    }

    #[test]
    fn extras_first_write_wins_and_custom_passes() {
        let mut r = req(vec![Message::text(Role::User, "x")]);
        r.extra.params.insert("custom_field".into(), json!(7));
        r.extra
            .params
            .insert("contents".into(), json!("evil-override"));
        let body = request_body(&r).expect("body");
        assert_eq!(body["custom_field"], 7, "extras pass through");
        assert!(body["contents"].is_array(), "structural keys win");
    }

    #[test]
    fn finish_chunk_without_usage_metadata_emits_no_usage() {
        let mut m = GeminiMapper::default();
        let out = m.map(r#"{"candidates":[{"content":{"parts":[]},"finishReason":"STOP"}]}"#);
        assert!(
            !out.iter().any(|e| matches!(e, Ok(InferEvent::Usage(_)))),
            "no usageMetadata → no Usage event: {out:?}"
        );
    }

    #[test]
    fn body_key_presence_tracks_inputs() {
        // no system → no systemInstruction · no config → no generationConfig
        let body = request_body(&req(vec![Message::text(Role::User, "x")])).expect("body");
        assert!(body.get("systemInstruction").is_none());
        assert!(body.get("generationConfig").is_none());
        // system present → key present (kills the two `delete !` mutants)
        let body2 = request_body(&req(vec![
            Message::text(Role::System, "s"),
            Message::text(Role::User, "x"),
        ]))
        .expect("body");
        assert_eq!(body2["systemInstruction"]["parts"][0]["text"], "s");
    }

    #[test]
    fn tool_name_fallback_strips_only_numeric_suffix() {
        // id not found in history → fallback path
        assert_eq!(tool_name_for(&[], "my-tool-3"), "my-tool");
        assert_eq!(
            tool_name_for(&[], "my-tool-x"),
            "my-tool-x",
            "non-numeric suffix kept"
        );
        assert_eq!(tool_name_for(&[], "plain"), "plain");
        // history lookup matches the exact id, not another
        let history = vec![Message::new(
            Role::Assistant,
            vec![
                ContentBlock::ToolUse {
                    id: "add-0".into(),
                    name: "add".into(),
                    input: json!({}),
                },
                ContentBlock::ToolUse {
                    id: "mul-1".into(),
                    name: "mul".into(),
                    input: json!({}),
                },
            ],
        )];
        assert_eq!(tool_name_for(&history, "mul-1"), "mul");
    }

    #[test]
    fn tool_round_trip_recovers_the_name_from_history() {
        let messages = vec![
            Message::new(
                Role::Assistant,
                vec![ContentBlock::ToolUse {
                    id: "add-0".into(),
                    name: "add".into(),
                    input: json!({"a":1}),
                }],
            ),
            Message::new(
                Role::User,
                vec![ContentBlock::ToolResult {
                    tool_use_id: "add-0".into(),
                    content: "2".into(),
                    is_error: false,
                }],
            ),
        ];
        let body = request_body(&req(messages)).expect("body");
        assert_eq!(body["contents"][0]["role"], "model");
        assert_eq!(
            body["contents"][0]["parts"][0]["functionCall"]["name"],
            "add"
        );
        assert_eq!(
            body["contents"][1]["parts"][0]["functionResponse"]["name"],
            "add"
        );
        assert_eq!(
            body["contents"][1]["parts"][0]["functionResponse"]["response"]["output"],
            "2"
        );
    }

    #[test]
    fn image_is_an_honest_error_and_finish_maps() {
        let bad = Message::new(
            Role::User,
            vec![ContentBlock::Image {
                source: "https://example.com/i.png".into(),
                detail: None,
            }],
        );
        assert!(request_body(&req(vec![bad])).is_err());

        assert!(matches!(
            map_finish(Some("STOP"), false),
            StopReason::EndTurn
        ));
        assert!(matches!(
            map_finish(Some("STOP"), true),
            StopReason::ToolUse
        ));
        assert!(matches!(
            map_finish(Some("MAX_TOKENS"), false),
            StopReason::MaxTokens
        ));
        assert!(matches!(
            map_finish(Some("SAFETY"), false),
            StopReason::ContentFilter
        ));
        assert!(matches!(
            map_finish(Some("RECITATION"), false),
            StopReason::Unknown(_)
        ));
    }

    #[test]
    fn parse_maps_function_call_with_synthesized_ids() {
        let fake = FakeHttp::with_json(200, "{}");
        let rp = resolved_with(&fake, "gemini/flash-25", "g-key");
        let body = br#"{"candidates":[{"content":{"parts":[
            {"functionCall":{"name":"add","args":{"a":1}}},
            {"functionCall":{"name":"add","args":{"a":2}}}]},
            "finishReason":"STOP"}],
            "usageMetadata":{"promptTokenCount":1,"candidatesTokenCount":2}}"#;
        let resp = parse_response(&rp, body).expect("parse");
        assert!(matches!(resp.stop_reason, StopReason::ToolUse));
        match (&resp.content[0], &resp.content[1]) {
            (ContentBlock::ToolUse { id: id0, .. }, ContentBlock::ToolUse { id: id1, .. }) => {
                assert_eq!(id0, "add-0");
                assert_eq!(id1, "add-1");
            }
            other => panic!("expected two tool_use blocks, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn stream_maps_chunks_usage_and_eof_done() {
        let sse = concat!(
            "data: {\"responseId\":\"g_s\",\"candidates\":[{\"content\":{\"parts\":[{\"text\":\"bon\"}]}}]}\n\n",
            "data: {\"candidates\":[{\"content\":{\"parts\":[{\"text\":\"soir\"}]},\"finishReason\":\"STOP\"}],\"usageMetadata\":{\"promptTokenCount\":4,\"candidatesTokenCount\":2}}\n\n",
        );
        let fake = FakeHttp::with_stream(200, sse, 13);
        let rp = resolved_with(&fake, "gemini/flash-25", "g-key");
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
        let usage_count = events
            .iter()
            .filter(|e| matches!(e, Ok(InferEvent::Usage(u)) if u.input_tokens == 4))
            .count();
        assert_eq!(
            usage_count, 1,
            "usage emitted exactly once on the finish chunk"
        );
        match events.last() {
            Some(Ok(InferEvent::Done {
                stop_reason,
                request_id,
                ..
            })) => {
                assert!(matches!(stop_reason, StopReason::EndTurn));
                assert_eq!(request_id.as_deref(), Some("g_s"));
            }
            other => panic!("expected EOF-synthesized Done, got {other:?}"),
        }
        // stream URL shape
        let sent = fake.captured();
        assert!(
            sent[0].url.ends_with(":streamGenerateContent?alt=sse"),
            "{}",
            sent[0].url
        );
    }

    #[test]
    fn stream_function_call_and_in_band_error() {
        let mut m = GeminiMapper::default();
        let out = m.map(
            r#"{"candidates":[{"content":{"parts":[{"functionCall":{"name":"add","args":{"a":1}}},{"functionCall":{"name":"mul","args":{"b":2}}}]}}]}"#,
        );
        assert!(matches!(out.first(),
            Some(Ok(InferEvent::ToolUseStart { id, name })) if id == "add-0" && name == "add"));
        // call counter INCREMENTS (kills += → *=): second call is mul-1.
        assert!(
            out.iter().any(|e| matches!(e,
                Ok(InferEvent::ToolUseStart { id, .. }) if id == "mul-1")),
            "second call gets index 1: {out:?}"
        );
        assert!(matches!(&out[1],
            Ok(InferEvent::ToolUseDelta { partial_json, .. }) if partial_json.contains("\"a\":1")));
        // the last chunk carries finishReason STOP → Done maps to ToolUse.
        m.map(r#"{"candidates":[{"content":{"parts":[]},"finishReason":"STOP"}]}"#);
        let tail = m.finish();
        match tail.first() {
            Some(Ok(InferEvent::Done { stop_reason, .. })) => {
                assert!(matches!(stop_reason, StopReason::ToolUse));
            }
            other => panic!("expected Done, got {other:?}"),
        }

        // in-band error chunk → typed Err, no synthetic Done after.
        let mut m = GeminiMapper::default();
        let out = m.map(r#"{"error":{"code":429,"message":"quota"}}"#);
        assert!(matches!(
            out.first(),
            Some(Err(ProviderError::Api { status: 429, .. }))
        ));
        assert!(m.finish().is_empty(), "no Done after wire error");
    }

    #[test]
    fn thinking_parts_map_and_thought_tokens_counted() {
        let fake = FakeHttp::with_json(200, "{}");
        let rp = resolved_with(&fake, "gemini/flash-25", "g-key");
        let body = br#"{"candidates":[{"content":{"parts":[
            {"text":"hmm","thought":true},{"text":"answer"}]},
            "finishReason":"STOP"}],
            "usageMetadata":{"promptTokenCount":1,"candidatesTokenCount":2,
                             "thoughtsTokenCount":5}}"#;
        let resp = parse_response(&rp, body).expect("parse");
        assert!(matches!(&resp.content[0],
            ContentBlock::Thinking { text } if text == "hmm"));
        assert!(matches!(&resp.content[1],
            ContentBlock::Text { text } if text == "answer"));
        assert_eq!(resp.usage.thinking_tokens, Some(5));
    }
}
