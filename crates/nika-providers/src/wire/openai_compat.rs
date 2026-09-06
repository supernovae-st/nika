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

use super::openai_schema::normalize_strict_schema;
use super::{EventMapper, SseEventStream, ToolNameMap, gen_ai_system, map_http_err, status_error};
use crate::registry::ResolvedProvider;

/// Single-shot inference.
pub(crate) async fn infer<H>(
    rp: &ResolvedProvider<H>,
    request: InferRequest,
) -> Result<InferResponse, ProviderError>
where
    H: HttpPostDyn + Send + Sync + 'static,
{
    // Tool names carry Nika's `:`/`/` separators; the OpenAI function-calling
    // API rejects them (NIKA-463). The same map sanitizes on the way out and
    // restores the canonical id on the way back (the verb never sees the
    // wire form).
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
        CompatMapper::new(names),
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
    names: &ToolNameMap,
) -> Result<HttpRequest, ProviderError> {
    // stream_options is an OpenAI-cloud extension; the 5 keyless local
    // servers (older llama.cpp/LocalAI builds) may 400 on unknown fields,
    // so it is gated to key-bearing (cloud) profiles. The profile id picks
    // the strict-schema normalization (openai's structured-output dialect).
    let body = request_body(
        &rp.wire_model,
        req,
        stream,
        rp.profile.requires_key,
        rp.profile.id,
        names,
    )?;
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
    if rp.profile.id == "openrouter" {
        // OpenRouter app attribution (their optional-but-recommended pair):
        // calls show up as `Nika` on openrouter.ai/rankings instead of an
        // anonymous key. openrouter-only — peers may 400 on surprise headers.
        headers.insert("http-referer".to_owned(), "https://nika.sh".to_owned());
        headers.insert("x-title".to_owned(), "Nika".to_owned());
    }

    let mut http_req = HttpRequest::post(rp.base_url.clone());
    http_req.headers = headers;
    http_req.body = Some(Bytes::from(bytes));
    // The task `timeout:` governs the transport deadline (F1) — buffered
    // calls always get one (per-provider default when undeclared · local
    // servers get minutes, not the 30s cloud default); streaming carries
    // only an explicit budget (the idle-read guard reaps stalls).
    http_req.timeout = super::transport_deadline(&rp.profile, req, stream);
    Ok(http_req)
}

/// The JSON body (pure — the unit-testable core).
///
/// `provider_id` selects the strict-schema dialect: only `openai` rejects
/// (HTTP 400) a `strict:true` JSON Schema whose objects omit
/// `additionalProperties:false` or under-specify `required`, so the
/// normalization is gated to it (the OpenAI-compatible cloud peers and the
/// local servers take the author's schema verbatim).
fn request_body(
    model: &str,
    req: &InferRequest,
    stream: bool,
    cloud_extensions: bool,
    provider_id: &str,
    names: &ToolNameMap,
) -> Result<Value, ProviderError> {
    let mut messages = Vec::new();
    for m in &req.messages {
        push_message(&mut messages, m, names)?;
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
        obj.insert(token_budget_key(provider_id, model).to_owned(), json!(mt));
    }
    if !req.stop_sequences.is_empty() {
        obj.insert("stop".to_owned(), json!(req.stop_sequences));
    }
    if !req.tools.is_empty() {
        // Each function name is sanitized to the wire-legal charset; the
        // model echoes the sanitized name in its tool_calls, which the
        // parse paths reverse-map back to the canonical id.
        let tools: Vec<Value> = req
            .tools
            .iter()
            .map(|t| {
                json!({ "type": "function", "function": {
                    "name": names.to_wire(&t.name),
                    "description": t.description,
                    "parameters": t.parameters,
                }})
            })
            .collect();
        obj.insert("tools".to_owned(), Value::Array(tools));
        let choice = match &req.tool_choice {
            ToolChoice::Required => json!("required"),
            ToolChoice::None => json!("none"),
            ToolChoice::Specific(name) => {
                json!({ "type": "function", "function": { "name": names.to_wire(name) } })
            }
            ToolChoice::Auto | _ => json!("auto"),
        };
        obj.insert("tool_choice".to_owned(), choice);
    }
    if let Some(rf) = response_format_value(&req.response_format, provider_id) {
        obj.insert("response_format".to_owned(), rf);
    }
    // Provider extras never override the structural keys this adapter set
    // (model · messages · stream · tools · …) — first-write-wins. The two
    // token-budget spellings are ONE logical key: when the adapter already
    // routed the budget (`max_completion_tokens` for gpt-5/o-series), a raw
    // extras `max_tokens` must not ride alongside it — OpenAI rejects a body
    // carrying both.
    for (k, v) in &req.extra.params {
        let sibling_present = budget_key_sibling(k).is_some_and(|alt| obj.contains_key(alt));
        if !obj.contains_key(k) && !sibling_present {
            obj.insert(k.clone(), v.clone());
        }
    }
    Ok(body)
}

/// The other spelling of the token-budget key, if `k` is one of the pair.
fn budget_key_sibling(k: &str) -> Option<&'static str> {
    match k {
        "max_tokens" => Some("max_completion_tokens"),
        "max_completion_tokens" => Some("max_tokens"),
        _ => None,
    }
}

/// `OpenAI` keeps legacy Chat Completions models on `max_tokens`, while GPT-5
/// and o-series reject it and require `max_completion_tokens`.
fn token_budget_key(provider_id: &str, model: &str) -> &'static str {
    if provider_id == "openai" && openai_uses_max_completion_tokens(model) {
        "max_completion_tokens"
    } else {
        "max_tokens"
    }
}

fn openai_uses_max_completion_tokens(model: &str) -> bool {
    let model = model.to_ascii_lowercase();
    ["gpt-5", "o1", "o3", "o4"]
        .iter()
        .any(|prefix| model.starts_with(prefix))
}

/// Build the `response_format` value for one request, or `None` for plain
/// text. Split out of `request_body` so that function stays under the
/// 100-LOC cap and the structured-output dialect lives in one place.
///
/// `strict:true` is an OpenAI-specific CLAIM that every object node carries
/// `additionalProperties:false` and lists all properties as `required` —
/// which ONLY `normalize_strict_schema` guarantees, and only for `openai`.
/// Sending it to a compat peer alongside the author's raw schema is a false
/// claim: a server honoring strict semantics would 400, while every
/// documented local wire (ollama · llama.cpp · vLLM · LM Studio) enforces on
/// the PRESENCE of `json_schema` and needs no strict flag (verified live
/// 2026-07-07: ollama/qwen enforces with and without it). So `strict` travels
/// ONLY where the schema was normalized to earn it.
fn response_format_value(format: &ResponseFormat, provider_id: &str) -> Option<Value> {
    match format {
        ResponseFormat::Json => Some(json!({ "type": "json_object" })),
        ResponseFormat::JsonSchema(schema) => {
            let is_openai = provider_id == "openai";
            let schema = if is_openai {
                normalize_strict_schema(schema)
            } else {
                schema.clone()
            };
            let mut json_schema = json!({ "name": "response", "schema": schema });
            if is_openai {
                json_schema["strict"] = json!(true);
            }
            Some(json!({ "type": "json_schema", "json_schema": json_schema }))
        }
        ResponseFormat::Text | _ => None,
    }
}

/// Kernel message → Chat Completions message(s). `ToolResult` blocks become
/// their own `role:"tool"` messages (the dialect's shape). A re-sent
/// `ToolUse` (the assistant's prior call, stored with its canonical id) is
/// re-sanitized so its `function.name` matches the registered tool.
fn push_message(
    out: &mut Vec<Value>,
    m: &nika_kernel::ai::provider::Message,
    names: &ToolNameMap,
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
                if !super::image_source_is_url(source) {
                    return Err(ProviderError::Other {
                        reason:
                            "image source must be an http(s) or data: URL at v0.1 (CAS sources land with nika-media)"
                                .to_owned(),
                    });
                }
                has_image = true;
                parts.push(json!({ "type": "image_url", "image_url": { "url": source } }));
            }
            ContentBlock::ToolUse { id, name, input } => {
                tool_calls.push(json!({ "id": id, "type": "function", "function": {
                    "name": names.to_wire(name), "arguments": input.to_string(),
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
    names: &ToolNameMap,
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
            name: names.to_canonical(&super::str_at(tc, "/function/name")),
            input,
        });
    }

    let usage = v.pointer("/usage").map(usage_from).unwrap_or_default();

    let raw_finish = v
        .pointer("/choices/0/finish_reason")
        .and_then(Value::as_str);
    let mut resp = InferResponse::new(content, usage, map_finish(raw_finish));
    // The budget law (R3-F1): an omitting backend gets an UNREPORTED
    // mark, not a fabricated zero the budgets would trust — and an EMPTY
    // usage object carries no signal, same class as the omission.
    resp.usage_reported = v.pointer("/usage").is_some_and(|u| {
        !u.is_null()
            && (u.pointer("/prompt_tokens").is_some() || u.pointer("/completion_tokens").is_some())
    });
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

/// The ONE OpenAI-compatible usage parse — the non-stream door and the
/// stream translator read the same `usage` object through it (B19: the
/// stream used to build a bare `TokenUsage::new(prompt, completion)` and
/// drop both details subsets, so a streamed cached prompt would price at
/// the full input rate).
///
/// `OTel` `gen_ai` semantics hold by construction: `prompt_tokens`
/// already INCLUDES the cached subset, so `input_tokens` needs no fold —
/// only the meter rides. Missing meters stay `None` ("not reported",
/// never a zero the budgets would trust).
///
/// `DeepSeek` reports its prompt cache at the TOP of the usage object
/// (`prompt_cache_hit_tokens` / `prompt_cache_miss_tokens`) instead of
/// under `prompt_tokens_details`; both spellings land in
/// `cache_read_tokens`, so `usd_for_split` prices the hit portion at the
/// catalog's cache-read rate instead of the full input rate.
fn usage_from(u: &Value) -> TokenUsage {
    let at = |key: &str| u.pointer(key).and_then(Value::as_u64);
    let mut usage = TokenUsage::new(
        at("/prompt_tokens").unwrap_or_default(),
        at("/completion_tokens").unwrap_or_default(),
    );
    usage.cache_read_tokens =
        at("/prompt_tokens_details/cached_tokens").or_else(|| at("/prompt_cache_hit_tokens"));
    usage.reasoning_tokens = at("/completion_tokens_details/reasoning_tokens");
    usage.total_tokens = at("/total_tokens");
    usage
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
    /// sanitized↔canonical tool-name map (restores the canonical id the
    /// model echoes back in its sanitized form · NIKA-463).
    names: ToolNameMap,
    done_sent: bool,
}

impl CompatMapper {
    fn new(names: ToolNameMap) -> Self {
        Self {
            names,
            ..Self::default()
        }
    }

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
                    name: self
                        .names
                        .to_canonical(&super::str_at(tc, "/function/name")),
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
            // The SAME parse as the non-stream door (B19): a streamed
            // cached prompt must not price at the full input rate the
            // day a verb streams. `stream_options.include_usage` is
            // already requested above.
            out.push(Ok(InferEvent::Usage(usage_from(u))));
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

    /// `request_body` with the tool-name map derived from the request's own
    /// tools — exactly how `infer`/`infer_stream` build it in production.
    fn body_of(
        model: &str,
        r: &InferRequest,
        stream: bool,
        cloud: bool,
        provider: &str,
    ) -> Result<Value, ProviderError> {
        let names = ToolNameMap::from_tools(&r.tools);
        request_body(model, r, stream, cloud, provider, &names)
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
        // output == completion_tokens verbatim (the openai wire does NOT
        // fold any sub-count into output — reasoning rides its own field;
        // the gemini wire's thoughts-fold is provider-specific, not here).
        assert_eq!(resp.usage.output_tokens, 3);
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

    #[tokio::test]
    async fn openrouter_sends_app_attribution_headers() {
        let fake = FakeHttp::with_json(
            200,
            r#"{"choices":[{"message":{"content":"ok"},"finish_reason":"stop"}],"usage":{}}"#,
        );
        let rp = resolved_with(&fake, "openrouter", "sk-or-test");
        let _ = infer(&rp, req(vec![Message::text(Role::User, "x")]))
            .await
            .expect("ok");
        let sent = fake.captured();
        assert_eq!(
            sent[0].headers.get("http-referer").map(String::as_str),
            Some("https://nika.sh"),
            "openrouter app attribution · referer"
        );
        assert_eq!(
            sent[0].headers.get("x-title").map(String::as_str),
            Some("Nika"),
            "openrouter app attribution · title"
        );
    }

    #[tokio::test]
    async fn attribution_headers_stay_openrouter_only() {
        let fake = FakeHttp::with_json(
            200,
            r#"{"choices":[{"message":{"content":"ok"},"finish_reason":"stop"}],"usage":{}}"#,
        );
        let rp = resolved_with(&fake, "groq", "gsk-test");
        let _ = infer(&rp, req(vec![Message::text(Role::User, "x")]))
            .await
            .expect("ok");
        let sent = fake.captured();
        assert!(
            !sent[0].headers.contains_key("http-referer")
                && !sent[0].headers.contains_key("x-title"),
            "attribution is an openrouter contract, not an openai-compat one"
        );
    }

    #[test]
    fn tools_and_response_format_shape() {
        let mut r = req(vec![Message::text(Role::User, "calcule")]);
        r.tools = vec![ToolDef::new("add", "adds", json!({"type":"object"}))];
        r.tool_choice = ToolChoice::Specific("add".into());
        r.response_format = ResponseFormat::JsonSchema(json!({"type":"object"}));
        // groq is an OpenAI-compatible peer: the author's schema is taken
        // verbatim (only `openai` runs the strict-mode normalizer below) AND
        // `strict` is NOT claimed — the peer enforces on `json_schema` presence
        // and a strict claim over an un-normalized schema is a false contract.
        let body = body_of("m", &r, false, true, "groq").expect("body");
        assert_eq!(body["tools"][0]["type"], "function");
        assert_eq!(body["tools"][0]["function"]["name"], "add");
        assert_eq!(body["tool_choice"]["function"]["name"], "add");
        assert_eq!(body["response_format"]["type"], "json_schema");
        assert!(
            body["response_format"]["json_schema"]
                .get("strict")
                .is_none(),
            "a compat peer must not carry the OpenAI-only strict claim"
        );
        assert_eq!(
            body["response_format"]["json_schema"]["schema"],
            json!({"type":"object"}),
            "non-openai schema passes through unmodified"
        );
    }

    #[test]
    fn openai_alone_carries_the_strict_claim() {
        // The strict flag rides ONLY the openai path — where the schema is also
        // normalized to actually satisfy strict mode (additionalProperties +
        // required). Sibling to `tools_and_response_format_shape` (the peer
        // case): together they pin strict to the one provider that earns it.
        let mut r = req(vec![Message::text(Role::User, "x")]);
        r.response_format = ResponseFormat::JsonSchema(json!({
            "type": "object",
            "properties": { "a": { "type": "string" } },
            "required": ["a"]
        }));
        let body = body_of("m", &r, false, true, "openai").expect("body");
        assert_eq!(
            body["response_format"]["json_schema"]["strict"], true,
            "openai claims strict"
        );
        assert_eq!(
            body["response_format"]["json_schema"]["schema"]["additionalProperties"], false,
            "openai schema is normalized to satisfy the strict claim"
        );
    }

    #[test]
    fn openai_gpt5_uses_max_completion_tokens() {
        let mut r = req(vec![Message::text(Role::User, "hi")]);
        r.max_tokens = Some(64);
        let body = body_of("gpt-5.2", &r, false, true, "openai").expect("body");
        assert_eq!(body["max_completion_tokens"], 64);
        assert!(
            body.get("max_tokens").is_none(),
            "gpt-5 rejects max_tokens on the OpenAI wire"
        );
    }

    #[test]
    fn non_openai_compat_and_legacy_openai_keep_max_tokens() {
        let mut r = req(vec![Message::text(Role::User, "hi")]);
        r.max_tokens = Some(64);

        let legacy = body_of("gpt-4o-mini", &r, false, true, "openai").expect("legacy body");
        assert_eq!(legacy["max_tokens"], 64);
        assert!(legacy.get("max_completion_tokens").is_none());

        let peer = body_of("gpt-5.2", &r, false, true, "groq").expect("peer body");
        assert_eq!(peer["max_tokens"], 64);
        assert!(peer.get("max_completion_tokens").is_none());
    }

    #[test]
    fn extras_max_tokens_never_rides_alongside_the_routed_budget_key() {
        // A raw extras `max_tokens` (the escape hatch's legacy spelling) must
        // not join a body whose budget was already routed to
        // `max_completion_tokens` — OpenAI rejects a request carrying both.
        let mut r = req(vec![Message::text(Role::User, "hi")]);
        r.max_tokens = Some(64);
        r.extra
            .params
            .insert("max_tokens".to_owned(), serde_json::json!(128));
        let body = body_of("gpt-5.2", &r, false, true, "openai").expect("body");
        assert_eq!(body["max_completion_tokens"], 64, "structural key wins");
        assert!(
            body.get("max_tokens").is_none(),
            "the sibling spelling must not ride alongside: {body}"
        );

        // The hatch stays verbatim when nothing structural was routed: an
        // extras-only budget passes through untouched (caller owns the wire).
        let mut hatch = req(vec![Message::text(Role::User, "hi")]);
        hatch
            .extra
            .params
            .insert("max_completion_tokens".to_owned(), serde_json::json!(99));
        let hatch_body = body_of("gpt-5.2", &hatch, false, true, "openai").expect("body");
        assert_eq!(hatch_body["max_completion_tokens"], 99);
        assert!(hatch_body.get("max_tokens").is_none());
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
        let body = body_of("m", &req(messages), false, true, "openai").expect("body");
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
        let resp = parse_response(&rp, body, &ToolNameMap::default()).expect("parse");
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
        let body = body_of("m", &req(messages), false, true, "openai").expect("body");
        assert_eq!(body["messages"][0]["role"], "system");
        assert_eq!(body["messages"][1]["role"], "user");
        assert_eq!(body["messages"][2]["role"], "assistant");
        assert!(body.get("stop").is_none(), "no stop_sequences → no key");
    }

    #[test]
    fn stream_options_gated_to_cloud_and_stream_only() {
        let r = req(vec![Message::text(Role::User, "x")]);
        let cloud_nostream = body_of("m", &r, false, true, "openai").expect("body");
        assert!(
            cloud_nostream.get("stream_options").is_none(),
            "non-stream → no stream_options"
        );
        let local_stream = body_of("m", &r, true, false, "ollama").expect("body");
        assert!(
            local_stream.get("stream_options").is_none(),
            "local profile → no stream_options"
        );
        let cloud_stream = body_of("m", &r, true, true, "openai").expect("body");
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
        let body = body_of("real", &r, false, true, "openai").expect("body");
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
        let body = body_of("m", &req(vec![msg]), false, true, "openai").expect("body");
        let parts = body["messages"][0]["content"]
            .as_array()
            .expect("multimodal parts array");
        assert!(parts.iter().any(|p| p["type"] == "image_url"));

        let data = Message::new(
            Role::User,
            vec![ContentBlock::Image {
                source: "data:image/png;base64,QUJD".into(),
                detail: None,
            }],
        );
        let data_body = body_of("m", &req(vec![data]), false, true, "openai").expect("data URL");
        let data_parts = data_body["messages"][0]["content"]
            .as_array()
            .expect("multimodal parts array");
        assert!(
            data_parts.iter().any(|p| {
                p["type"] == "image_url" && p["image_url"]["url"] == "data:image/png;base64,QUJD"
            }),
            "data: URLs ride as image_url: {data_parts:?}"
        );

        let bad = Message::new(
            Role::User,
            vec![ContentBlock::Image {
                source: "blake3:abc".into(),
                detail: None,
            }],
        );
        assert!(body_of("m", &req(vec![bad]), false, true, "openai").is_err());
    }

    #[test]
    fn mapper_tool_args_emitted_and_finish_synthesizes_done() {
        let mut m = CompatMapper::new(ToolNameMap::default());
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

    // ── BUG#4 · OpenAI strict-mode schema normalization (Gate 2 parity) ──

    /// Pull the wire `schema` out of an openai `JsonSchema` request body.
    fn strict_schema(input: Value) -> Value {
        let mut r = req(vec![Message::text(Role::User, "x")]);
        r.response_format = ResponseFormat::JsonSchema(input);
        let body = body_of("m", &r, false, true, "openai").expect("body");
        body["response_format"]["json_schema"]["schema"].clone()
    }

    #[test]
    fn strict_flat_object_gets_additional_props_and_all_required() {
        let out = strict_schema(json!({
            "type": "object",
            "properties": { "name": {"type": "string"}, "age": {"type": "integer"} }
        }));
        assert_eq!(out["additionalProperties"], false);
        let req: Vec<&str> = out["required"]
            .as_array()
            .expect("required array")
            .iter()
            .map(|v| v.as_str().expect("str"))
            .collect();
        // both keys required (no author `required` → all become nullable +
        // required, OpenAI's optional shape).
        assert!(req.contains(&"name") && req.contains(&"age"), "{req:?}");
        assert_eq!(out["properties"]["name"]["type"], json!(["string", "null"]));
        assert_eq!(out["properties"]["age"]["type"], json!(["integer", "null"]));
    }

    #[test]
    fn strict_required_field_keeps_its_scalar_type() {
        let out = strict_schema(json!({
            "type": "object",
            "properties": { "id": {"type": "string"}, "note": {"type": "string"} },
            "required": ["id"]
        }));
        // `id` was author-required → type untouched; `note` was optional →
        // nullable; both end up in `required` (the strict invariant).
        assert_eq!(out["properties"]["id"]["type"], "string");
        assert_eq!(out["properties"]["note"]["type"], json!(["string", "null"]));
        let mut req: Vec<&str> = out["required"]
            .as_array()
            .expect("req")
            .iter()
            .map(|v| v.as_str().expect("str"))
            .collect();
        req.sort_unstable();
        assert_eq!(req, vec!["id", "note"]);
    }

    #[test]
    fn strict_recurses_into_nested_objects() {
        let out = strict_schema(json!({
            "type": "object",
            "properties": {
                "address": {
                    "type": "object",
                    "properties": { "city": {"type": "string"} }
                }
            },
            "required": ["address"]
        }));
        let inner = &out["properties"]["address"];
        assert_eq!(
            inner["additionalProperties"], false,
            "inner object tightened"
        );
        assert_eq!(inner["required"], json!(["city"]));
        assert_eq!(
            inner["properties"]["city"]["type"],
            json!(["string", "null"])
        );
    }

    #[test]
    fn strict_recurses_into_array_of_objects() {
        let out = strict_schema(json!({
            "type": "object",
            "properties": {
                "items": {
                    "type": "array",
                    "items": {
                        "type": "object",
                        "properties": { "sku": {"type": "string"} }
                    }
                }
            },
            "required": ["items"]
        }));
        let elem = &out["properties"]["items"]["items"];
        assert_eq!(
            elem["additionalProperties"], false,
            "array element tightened"
        );
        assert_eq!(elem["required"], json!(["sku"]));
    }

    #[test]
    fn strict_recurses_into_defs() {
        let out = strict_schema(json!({
            "type": "object",
            "properties": { "node": {"$ref": "#/$defs/Node"} },
            "required": ["node"],
            "$defs": {
                "Node": {
                    "type": "object",
                    "properties": { "label": {"type": "string"} }
                }
            }
        }));
        let node = &out["$defs"]["Node"];
        assert_eq!(node["additionalProperties"], false, "$defs entry tightened");
        assert_eq!(node["required"], json!(["label"]));
    }

    #[test]
    fn strict_make_nullable_is_idempotent_on_existing_union() {
        // An author who already wrote `["string","null"]` must not gain a
        // second `"null"`.
        let out = strict_schema(json!({
            "type": "object",
            "properties": { "maybe": {"type": ["string", "null"]} }
        }));
        assert_eq!(
            out["properties"]["maybe"]["type"],
            json!(["string", "null"])
        );
    }

    #[test]
    fn strict_only_fires_for_openai_not_for_peers() {
        // The exact same nested schema reaches groq verbatim (no injected
        // `additionalProperties`, no widened types) — the regression guard
        // for "do not mutate non-openai providers".
        let nested = json!({
            "type": "object",
            "properties": {
                "inner": { "type": "object", "properties": { "x": {"type": "string"} } }
            }
        });
        let mut r = req(vec![Message::text(Role::User, "x")]);
        r.response_format = ResponseFormat::JsonSchema(nested.clone());
        let body = body_of("m", &r, false, true, "groq").expect("body");
        assert_eq!(
            body["response_format"]["json_schema"]["schema"], nested,
            "peer schema is byte-for-byte the author's"
        );
    }

    // ── BUG#7 · oneOf → anyOf for openai strict (Gate 2 parity) ──

    #[test]
    fn strict_rewrites_oneof_to_anyof_in_a_property() {
        // gemini accepts oneOf; openai 400s "'oneOf' is not permitted". The
        // property's oneOf must reach the wire as anyOf, with no oneOf left.
        let out = strict_schema(json!({
            "type": "object",
            "required": ["result"],
            "properties": {
                "result": {
                    "oneOf": [
                        {"type": "object", "required": ["card_last4"],
                         "properties": {"card_last4": {"type": "string"}}},
                        {"type": "object", "required": ["bank_account"],
                         "properties": {"bank_account": {"type": "string"}}}
                    ]
                }
            }
        }));
        let result = &out["properties"]["result"];
        assert!(result.get("oneOf").is_none(), "no oneOf survives: {result}");
        let branches = result["anyOf"].as_array().expect("anyOf array");
        assert_eq!(branches.len(), 2, "both branches preserved");
        // the branches were normalized too: each object got tightened.
        assert_eq!(branches[0]["additionalProperties"], false);
        assert_eq!(branches[0]["required"], json!(["card_last4"]));
    }

    #[test]
    fn strict_rewrites_nested_oneof_recursively() {
        // a oneOf nested inside a oneOf branch is also rewritten.
        let out = strict_schema(json!({
            "type": "object",
            "required": ["x"],
            "properties": {
                "x": {
                    "oneOf": [
                        {"oneOf": [
                            {"type": "string"},
                            {"type": "integer"}
                        ]},
                        {"type": "boolean"}
                    ]
                }
            }
        }));
        let x = &out["properties"]["x"];
        assert!(x.get("oneOf").is_none());
        let inner = &x["anyOf"][0];
        assert!(
            inner.get("oneOf").is_none(),
            "nested oneOf rewritten: {inner}"
        );
        assert!(inner["anyOf"].is_array());
    }

    #[test]
    fn strict_merges_oneof_into_existing_anyof() {
        // pathological author: both anyOf and oneOf on one node. The oneOf
        // branches fold into anyOf so no oneOf reaches the wire.
        let out = strict_schema(json!({
            "anyOf": [{"type": "string"}],
            "oneOf": [{"type": "integer"}, {"type": "boolean"}]
        }));
        assert!(out.get("oneOf").is_none(), "oneOf gone");
        assert_eq!(
            out["anyOf"].as_array().expect("anyOf").len(),
            3,
            "1 original anyOf + 2 folded oneOf branches"
        );
    }

    // ── const + uniqueItems · openai strict (Gate 2 parity) ──

    #[test]
    fn strict_rewrites_const_to_single_member_enum_preserving_type() {
        // `const` 400s on openai strict; rewrite to a one-member `enum`.
        // The integer const keeps its JSON number type (not stringified).
        let out = strict_schema(json!({
            "type": "object",
            "required": ["version", "name"],
            "properties": {
                "version": {"const": 1},
                "name": {"type": "string"}
            }
        }));
        let version = &out["properties"]["version"];
        assert!(version.get("const").is_none(), "no const survives");
        assert_eq!(version["enum"], json!([1]), "const → one-member enum");
        assert!(
            version["enum"][0].is_number(),
            "integer const stays a number, not a string"
        );
    }

    #[test]
    fn strict_strips_unique_items() {
        // `uniqueItems` is array-only validation, not in the dialect → drop.
        let out = strict_schema(json!({
            "type": "object",
            "required": ["fruits"],
            "properties": {
                "fruits": {
                    "type": "array",
                    "uniqueItems": true,
                    "items": {"type": "string"}
                }
            }
        }));
        let fruits = &out["properties"]["fruits"];
        assert!(fruits.get("uniqueItems").is_none(), "uniqueItems stripped");
        // the rest of the array schema is intact.
        assert_eq!(fruits["type"], "array");
        assert_eq!(fruits["items"]["type"], "string");
    }

    // ── negation/conditional strip + allOf flatten (live-verified 400s · 2026-07-08) ──

    #[test]
    fn strict_strips_negation_and_conditional_family() {
        // `not` / `if`/`then`/`else` / `dependentRequired` all 400 at
        // request time ("Unsupported keywords") — stripped at the wire,
        // held by the verb's local validation.
        let out = strict_schema(json!({
            "type": "object",
            "required": ["x"],
            "properties": {
                "x": {"type": "string", "not": {"enum": ["forbidden"]}}
            },
            "if": { "properties": { "x": { "enum": ["a"] } } },
            "then": { "required": ["x"] },
            "dependentRequired": { "x": ["y"] }
        }));
        assert!(out["properties"]["x"].get("not").is_none(), "{out}");
        assert!(out.get("if").is_none());
        assert!(out.get("then").is_none());
        assert!(out.get("dependentRequired").is_none());
    }

    #[test]
    fn strict_flattens_single_branch_allof() {
        // "'allOf' is not permitted" — a single branch inlines into the
        // node and the merged object still earns the strict invariants.
        let out = strict_schema(json!({
            "type": "object",
            "required": ["p"],
            "properties": {
                "p": { "allOf": [{
                    "type": "object",
                    "required": ["name"],
                    "properties": { "name": {"type": "string"} }
                }]}
            }
        }));
        let p = &out["properties"]["p"];
        assert!(p.get("allOf").is_none(), "no allOf survives: {p}");
        assert_eq!(p["type"], "object", "branch keys inlined");
        assert_eq!(p["properties"]["name"]["type"], "string");
        assert_eq!(p["additionalProperties"], false, "merged node tightened");
    }

    #[test]
    fn strict_flattens_multi_branch_allof_with_unions() {
        // A conjunction of two object branches: properties union +
        // required union — the wire shape covers every branch's keys and
        // the local validator still enforces the authored conjunction.
        let out = strict_schema(json!({
            "type": "object",
            "required": ["p"],
            "properties": {
                "p": { "allOf": [
                    {"type": "object", "required": ["a"],
                     "properties": {"a": {"type": "string"}}},
                    {"type": "object", "required": ["b"],
                     "properties": {"b": {"type": "integer"}}}
                ]}
            }
        }));
        let p = &out["properties"]["p"];
        assert!(p.get("allOf").is_none());
        assert!(p["properties"]["a"].is_object() && p["properties"]["b"].is_object());
        let req = p["required"].as_array().expect("required union");
        assert!(
            req.contains(&json!("a")) && req.contains(&json!("b")),
            "conjunction requires both branches' keys: {req:?}"
        );
    }

    #[test]
    fn peer_keeps_allof_and_not_verbatim() {
        // The strip is an OPENAI-dialect rewrite — a compat peer (groq ·
        // local servers) still receives the author's composition as
        // written.
        let schema = json!({
            "type": "object",
            "properties": { "p": { "allOf": [{"type": "object"}], "not": {"enum": [1]} } }
        });
        let mut r = req(vec![Message::text(Role::User, "x")]);
        r.response_format = ResponseFormat::JsonSchema(schema.clone());
        let body = body_of("m", &r, false, true, "groq").expect("body");
        assert_eq!(
            body["response_format"]["json_schema"]["schema"], schema,
            "peer schema byte-for-byte"
        );
    }

    // ── BUG#5 · tool-name sanitization on the openai-compat wire (NIKA-463) ──

    #[test]
    fn tool_names_with_colons_are_sanitized_on_send() {
        let mut r = req(vec![Message::text(Role::User, "go")]);
        r.tools = vec![
            ToolDef::new("nika:read", "", json!({"type":"object"})),
            ToolDef::new("mcp:git/diff", "", json!({"type":"object"})),
        ];
        r.tool_choice = ToolChoice::Specific("nika:read".into());
        let body = body_of("m", &r, false, true, "openai").expect("body");
        // the colon/slash names reach the wire in the legal charset
        assert_eq!(body["tools"][0]["function"]["name"], "nika_read");
        assert_eq!(body["tools"][1]["function"]["name"], "mcp_git_diff");
        // tool_choice's Specific name is sanitized identically
        assert_eq!(body["tool_choice"]["function"]["name"], "nika_read");
    }

    #[test]
    fn resent_tool_use_name_is_sanitized() {
        // The assistant's prior ToolUse (stored canonical) must serialize
        // its function.name back to the sanitized form on the next turn.
        let mut r = req(vec![Message::new(
            Role::Assistant,
            vec![ContentBlock::ToolUse {
                id: "call_1".into(),
                name: "nika:read".into(),
                input: json!({"path": "x"}),
            }],
        )]);
        r.tools = vec![ToolDef::new("nika:read", "", json!({"type":"object"}))];
        let body = body_of("m", &r, false, true, "openai").expect("body");
        assert_eq!(
            body["messages"][0]["tool_calls"][0]["function"]["name"],
            "nika_read"
        );
    }

    #[test]
    fn parse_reverse_maps_tool_name_to_canonical() {
        // The model echoes the SANITIZED name; the parse path restores the
        // canonical id the verb dispatches on.
        let fake = FakeHttp::with_json(200, "{}");
        let rp = resolved_with(&fake, "openai", "sk-test");
        let names =
            ToolNameMap::from_tools(&[ToolDef::new("mcp:git/diff", "", json!({"type":"object"}))]);
        let body = br#"{"id":"cc_3",
            "choices":[{"message":{"content":null,
                "tool_calls":[{"id":"c1","function":{"name":"mcp_git_diff","arguments":"{}"}}]},
                "finish_reason":"tool_calls"}],
            "usage":{}}"#;
        let resp = parse_response(&rp, body, &names).expect("parse");
        match &resp.content[0] {
            ContentBlock::ToolUse { name, .. } => assert_eq!(name, "mcp:git/diff"),
            other => panic!("expected tool_use, got {other:?}"),
        }
    }

    #[test]
    fn stream_reverse_maps_tool_use_start_name() {
        let names =
            ToolNameMap::from_tools(&[ToolDef::new("nika:done", "", json!({"type":"object"}))]);
        let mut m = CompatMapper::new(names);
        let out = m.map(
            r#"{"choices":[{"delta":{"tool_calls":[{"index":0,"id":"c1","function":{"name":"nika_done","arguments":""}}]},"finish_reason":null}]}"#,
        );
        assert!(
            out.iter().any(|e| matches!(e,
                Ok(InferEvent::ToolUseStart { name, .. }) if name == "nika:done")),
            "ToolUseStart name reverse-mapped to canonical: {out:?}"
        );
    }

    #[tokio::test]
    async fn end_to_end_round_trips_a_colon_tool_name() {
        // A colon tool goes out sanitized; the model's sanitized echo comes
        // back as the canonical id — the whole NIKA-463 loop in one test.
        let fake = FakeHttp::with_json(
            200,
            r#"{"id":"cc_e2e","model":"gpt-4o-mini",
                "choices":[{"message":{"content":null,
                    "tool_calls":[{"id":"c1","function":{"name":"nika_read","arguments":"{\"path\":\"f\"}"}}]},
                    "finish_reason":"tool_calls"}],
                "usage":{"prompt_tokens":1,"completion_tokens":1}}"#,
        );
        let rp = resolved_with(&fake, "openai", "sk-test");
        let mut request = req(vec![Message::text(Role::User, "read f")]);
        request.tools = vec![ToolDef::new("nika:read", "reads", json!({"type":"object"}))];
        let resp = infer(&rp, request).await.expect("infer ok");

        // sent: sanitized name on the wire
        let sent = fake.captured();
        let body: Value = serde_json::from_slice(sent[0].body.as_ref().unwrap()).unwrap();
        assert_eq!(body["tools"][0]["function"]["name"], "nika_read");
        // received: canonical id restored for the executor
        match &resp.content[0] {
            ContentBlock::ToolUse { name, .. } => assert_eq!(name, "nika:read"),
            other => panic!("expected tool_use, got {other:?}"),
        }
    }
}
