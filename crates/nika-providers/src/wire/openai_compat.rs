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
    let mut sent = false;
    infer_tracked(rp, request, &mut sent).await
}

/// Same wire, with an exact dispatched/not-dispatched observation for the
/// bounded registry (no retry after an unknown charge).
pub(crate) async fn infer_tracked<H>(
    rp: &ResolvedProvider<H>,
    request: InferRequest,
    sent: &mut bool,
) -> Result<InferResponse, ProviderError>
where
    H: HttpPostDyn + Send + Sync + 'static,
{
    // Tool names carry Nika's `:`/`/` separators; the OpenAI function-calling
    // API rejects them (NIKA-463). The same map sanitizes on the way out and
    // restores the canonical id on the way back (the verb never sees the
    // wire form).
    let names = ToolNameMap::from_tools(&request.tools);
    let mut http_req = build_request(rp, &request, false, &names)?;
    let bytes = http_req.body.as_ref().map_or(0, bytes::Bytes::len);
    let http = rp.http.as_ref().ok_or_else(wiring_bug)?;
    if let Some(account) = &rp.admission
        && !http.supports_single_attempt()
    {
        return Err(account.refuse("HTTP effect has no single-attempt guarantee"));
    }
    let mut attempt = super::admission::reserve(rp, &request, bytes)?;
    if attempt.is_some() {
        http_req.follow_redirects = false;
    }
    if let Some(a) = &mut attempt {
        a.sent()?;
    }
    *sent = true;
    let resp = http.post(http_req).await.map_err(|e| map_http_err(&e))?;
    if let Some(account) = &rp.admission
        && resp.final_url != rp.base_url
    {
        return Err(account.refuse("transport endpoint changed; charge unknown"));
    }
    if !(200..300).contains(&resp.status) {
        return Err(status_error(
            resp.status,
            &resp.body,
            resp.headers.get("retry-after").map(String::as_str),
            &rp.wire_model,
        ));
    }
    let response = parse_response(rp, &resp.body, &names)?;
    if let Some(a) = &mut attempt {
        a.settle(&response)?;
    }
    Ok(response)
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
    // The seat's structured-output level is a catalog fact: a `json_schema`
    // promise the seat cannot honour is reshaped BEFORE the body is built
    // (`wire::json_mode` · DeepSeek refuses it with a 400 at the door).
    let json_mode = nika_catalog::model_capabilities(rp.profile.id, &rp.wire_model).json_mode;
    let req = super::json_mode::shape(req, json_mode);
    let body = request_body(
        &rp.wire_model,
        &req,
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
    http_req.timeout = super::transport_deadline(&rp.profile, &req, stream);
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

/// Route the budget through shared model capabilities; compatible peers retain
/// their established `max_tokens` contract.
fn token_budget_key(provider_id: &str, model: &str) -> &'static str {
    if provider_id == "openai"
        && nika_catalog::model_capabilities(provider_id, model).token_limit_param
            == nika_catalog::TokenLimitParam::MaxCompletionTokens
    {
        "max_completion_tokens"
    } else {
        "max_tokens"
    }
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
    let parsed = if rp.admission.is_some() {
        super::bounded_json::parse(body)
    } else {
        serde_json::from_slice(body)
    };
    let v: Value = parsed.map_err(|e| ProviderError::Other {
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
    // Keep the billed response intact; verbs stop before consuming its output.
    if msg
        .and_then(|m| m.get("refusal"))
        .and_then(Value::as_str)
        .is_some_and(|refusal| !refusal.is_empty())
    {
        resp.stop_reason = StopReason::ContentFilter;
    }
    // The budget law (R3-F1): an omitting backend gets an UNREPORTED
    // mark, not a fabricated zero the budgets would trust — and an EMPTY
    // usage object carries no signal, same class as the omission.
    resp.usage_reported = v.pointer("/usage").is_some_and(|u| {
        !u.is_null()
            && (u.pointer("/prompt_tokens").is_some() || u.pointer("/completion_tokens").is_some())
    });
    if super::admission::complete_usage(rp.profile.id, &v) {
        resp.usage_completeness = nika_kernel::ai::provider::UsageCompleteness::Complete;
    }
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
/// stream translator read the same `usage` object through it (the
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
            // The SAME parse as the non-stream door : a streamed
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
mod tests;
