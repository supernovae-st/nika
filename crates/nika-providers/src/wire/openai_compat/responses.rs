// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! `OpenAI`'s Responses door, for the function tools Chat Completions refuses on `OpenAI`'s current
//! models. `OpenAI` documents that « Starting with GPT-5.4, Chat Completions does not support tool
//! calling with `reasoning_effort` values other than `none` », and its current models do not
//! default to `none`: measured live on 2026-10-16, any function tool on `/v1/chat/completions`
//! is refused (HTTP 400) for gpt-6-astra, gpt-6-sol, gpt-6.1-sol, gpt-6-luna and gpt-5.6-sol,
//! and gpt-6-astra and gpt-6.1-sol take no `none` at all. A request with function tools to
//! `OpenAI`'s own endpoint therefore rides `/v1/responses`: the conversation becomes input items,
//! the tools are internally tagged, the reply's output items become the same content blocks.
//!
//! Stateless: `store: false`, and every `reasoning` item the reply carries (with its
//! `encrypted_content`) is kept as opaque reasoning state and sent back verbatim, in place, on
//! the next request of the conversation.

use std::collections::BTreeMap;

use bytes::Bytes;
use nika_kernel::ai::provider::{
    ContentBlock, InferRequest, InferResponse, ProviderError, ResponseFormat, Role, StopReason,
    TokenUsage, ToolChoice,
};
use nika_kernel::http::HttpRequest;
use serde_json::{Map, Value, json};

use super::super::openai_schema::normalize_strict_schema;
use super::super::{ToolNameMap, gen_ai_system};
use crate::registry::ResolvedProvider;

/// The wire of the reasoning state this door keeps and sends back.
pub(super) const WIRE: &str = "openai-responses";

/// Whether `req` rides the Responses door: function tools, to `OpenAI`'s own Chat Completions
/// endpoint (a configured gateway keeps the dialect it serves).
pub(super) fn wanted(rp: &ResolvedProvider<impl Sized>, req: &InferRequest) -> bool {
    rp.profile.id == "openai"
        && rp.base_url == rp.profile.base_url
        && !req.tools.is_empty()
        && endpoint(&rp.base_url).is_some()
}

/// The Responses endpoint beside a Chat Completions one.
#[must_use]
pub(crate) fn endpoint(chat: &str) -> Option<String> {
    chat.strip_suffix("/chat/completions")
        .map(|stem| format!("{stem}/responses"))
}

/// The HTTP request: `OpenAI`'s bearer key and the Responses body.
pub(super) fn build_request(
    rp: &ResolvedProvider<impl Sized>,
    req: &InferRequest,
    names: &ToolNameMap,
) -> Result<HttpRequest, ProviderError> {
    let json_mode = nika_catalog::model_capabilities(rp.profile.id, &rp.wire_model).json_mode;
    let req = super::super::json_mode::shape(req, json_mode);
    let mut body = request_body(&rp.wire_model, &req, names)?;
    effort(&mut body, &req, rp)?;
    let bytes = serde_json::to_vec(&body).map_err(|e| ProviderError::Other {
        reason: format!("request serialization failed: {e}"),
    })?;
    let url = endpoint(&rp.base_url).ok_or_else(|| ProviderError::Other {
        reason: "the OpenAI endpoint names no Responses route".to_owned(),
    })?;
    let mut headers = BTreeMap::new();
    headers.insert("content-type".to_owned(), "application/json".to_owned());
    if let Some(key) = &rp.key {
        headers.insert(
            "authorization".to_owned(),
            format!("Bearer {}", key.expose()),
        );
    }
    let mut http_req = HttpRequest::post(url);
    http_req.headers = headers;
    http_req.body = Some(Bytes::from(bytes));
    http_req.timeout = super::super::transport_deadline(&req, false);
    Ok(http_req)
}

/// An explicit reasoning effort, qualified as the Chat Completions door qualifies it (the exact
/// model documents that level, on the direct route), written as Responses spells it.
fn effort(
    body: &mut Value,
    req: &InferRequest,
    rp: &ResolvedProvider<impl Sized>,
) -> Result<(), ProviderError> {
    let mut chat = json!({});
    super::super::reasoning::apply(&mut chat, req, rp.profile.id, &rp.wire_model, &rp.base_url)?;
    if let Some(word) = chat.get("reasoning_effort").cloned()
        && let Some(object) = body.as_object_mut()
    {
        object.insert("reasoning".to_owned(), json!({ "effort": word }));
    }
    Ok(())
}

/// The Responses body (pure, the unit-testable core).
pub(super) fn request_body(
    model: &str,
    req: &InferRequest,
    names: &ToolNameMap,
) -> Result<Value, ProviderError> {
    if !req.stop_sequences.is_empty() {
        return Err(ProviderError::Other {
            reason: "the OpenAI Responses API takes no stop sequences; nothing was sent".to_owned(),
        });
    }
    let mut instructions = Vec::new();
    let mut input = Vec::new();
    for m in &req.messages {
        push_items(&mut input, &mut instructions, m, names)?;
    }
    let mut body = json!({
        "model": model,
        "input": input,
        "store": false,
        "include": ["reasoning.encrypted_content"],
    });
    let obj = body.as_object_mut().ok_or_else(|| ProviderError::Other {
        reason: "request body must be an object".to_owned(),
    })?;
    if !instructions.is_empty() {
        obj.insert("instructions".to_owned(), json!(instructions.join("\n\n")));
    }
    if let Some(t) = req.temperature {
        obj.insert("temperature".to_owned(), json!(t));
    }
    if let Some(mt) = req.max_tokens {
        obj.insert("max_output_tokens".to_owned(), json!(mt));
    }
    let tools: Vec<Value> = (req.tools.iter())
        .map(|t| {
            json!({
                "type": "function",
                "name": names.to_wire(&t.name),
                "description": t.description,
                "parameters": t.parameters,
                // Chat Completions' functions are non-strict; Responses tries strict unless told.
                "strict": false,
            })
        })
        .collect();
    obj.insert("tools".to_owned(), Value::Array(tools));
    let choice = match &req.tool_choice {
        ToolChoice::Required => json!("required"),
        ToolChoice::None => json!("none"),
        ToolChoice::Specific(name) => json!({ "type": "function", "name": names.to_wire(name) }),
        ToolChoice::Auto | _ => json!("auto"),
    };
    obj.insert("tool_choice".to_owned(), choice);
    if let Some(format) = text_format(&req.response_format) {
        obj.insert("text".to_owned(), json!({ "format": format }));
    }
    if let Some(key) = &req.cache_key
        && !req.extra.params.contains_key("prompt_cache_key")
    {
        obj.insert("prompt_cache_key".to_owned(), json!(key));
    }
    // Provider extras never override the structural keys this door set, and a Chat Completions
    // token budget has no place here (the budget is `max_output_tokens`).
    for (k, v) in &req.extra.params {
        if !obj.contains_key(k) && !matches!(k.as_str(), "max_tokens" | "max_completion_tokens") {
            obj.insert(k.clone(), v.clone());
        }
    }
    Ok(body)
}

/// Structured output as Responses spells it (`text.format`), strict as on Chat Completions.
fn text_format(format: &ResponseFormat) -> Option<Value> {
    match format {
        ResponseFormat::Json => Some(json!({ "type": "json_object" })),
        ResponseFormat::JsonSchema(schema) => Some(json!({
            "type": "json_schema",
            "name": "response",
            "schema": normalize_strict_schema(schema),
            "strict": true,
        })),
        ResponseFormat::Text | _ => None,
    }
}

/// One kernel message as input items: system text joins the instructions; a user message is one
/// message item after the function outputs it carries; an assistant message replays its
/// reasoning state, its text and its function calls in their order.
fn push_items(
    input: &mut Vec<Value>,
    instructions: &mut Vec<String>,
    m: &nika_kernel::ai::provider::Message,
    names: &ToolNameMap,
) -> Result<(), ProviderError> {
    match m.role {
        Role::System => {
            for block in &m.content {
                if let ContentBlock::Text { text } = block {
                    instructions.push(text.clone());
                }
            }
        }
        Role::Assistant => {
            for block in &m.content {
                match block {
                    ContentBlock::Reasoning { wire, state } if wire == WIRE => {
                        input.push(state.clone());
                    }
                    ContentBlock::Text { text } if !text.is_empty() => {
                        input.push(json!({ "role": "assistant", "content": text }));
                    }
                    // Without its item id: a call replayed by id asks its reasoning item too.
                    ContentBlock::ToolUse {
                        id,
                        name,
                        input: args,
                    } => input.push(json!({
                        "type": "function_call",
                        "call_id": id,
                        "name": names.to_wire(name),
                        "arguments": args.to_string(),
                    })),
                    _ => {}
                }
            }
        }
        _ => {
            let mut parts = Vec::new();
            for block in &m.content {
                match block {
                    ContentBlock::Text { text } => {
                        parts.push(json!({ "type": "input_text", "text": text }));
                    }
                    ContentBlock::Image { source, .. } => {
                        if !super::super::image_source_is_url(source) {
                            return Err(ProviderError::Other {
                                reason: "image source must be an http(s) or data: URL".to_owned(),
                            });
                        }
                        parts.push(json!({ "type": "input_image", "image_url": source }));
                    }
                    ContentBlock::ToolResult {
                        tool_use_id,
                        content,
                        ..
                    } => input.push(json!({
                        "type": "function_call_output",
                        "call_id": tool_use_id,
                        "output": content,
                    })),
                    _ => {}
                }
            }
            if !parts.is_empty() {
                input.push(json!({ "role": "user", "content": parts }));
            }
        }
    }
    Ok(())
}

/// Parse a 2xx Responses reply.
pub(super) fn parse_response(
    rp: &ResolvedProvider<impl Sized>,
    body: &[u8],
    names: &ToolNameMap,
) -> Result<InferResponse, ProviderError> {
    let v: Value = serde_json::from_slice(body).map_err(|e| ProviderError::Other {
        reason: format!("openai responses reply is not JSON: {e}"),
    })?;
    let mut content = Vec::new();
    let mut refused = false;
    for item in (v.get("output").and_then(Value::as_array))
        .into_iter()
        .flatten()
    {
        match item.get("type").and_then(Value::as_str) {
            Some("reasoning") => {
                for summary in (item.get("summary").and_then(Value::as_array))
                    .into_iter()
                    .flatten()
                {
                    if let Some(text) = summary.get("text").and_then(Value::as_str)
                        && !text.is_empty()
                    {
                        content.push(ContentBlock::Thinking {
                            text: text.to_owned(),
                        });
                    }
                }
                content.push(super::super::reasoning(WIRE, item.clone()));
            }
            Some("message") => {
                for part in (item.get("content").and_then(Value::as_array))
                    .into_iter()
                    .flatten()
                {
                    match part.get("type").and_then(Value::as_str) {
                        Some("output_text") => {
                            if let Some(text) = part.get("text").and_then(Value::as_str)
                                && !text.is_empty()
                            {
                                content.push(ContentBlock::Text {
                                    text: text.to_owned(),
                                });
                            }
                        }
                        Some("refusal") => refused = true,
                        _ => {}
                    }
                }
            }
            Some("function_call") => {
                let args = item
                    .get("arguments")
                    .and_then(Value::as_str)
                    .unwrap_or("{}");
                let input =
                    serde_json::from_str(args).unwrap_or_else(|_| Value::String(args.to_owned()));
                content.push(ContentBlock::ToolUse {
                    id: str_of(item, "call_id"),
                    name: names.to_canonical(&str_of(item, "name")),
                    input,
                });
            }
            _ => {}
        }
    }
    let reported = v.get("usage").and_then(usage_from);
    let usage_reported = reported.is_some();
    let incomplete = (v.pointer("/incomplete_details/reason")).and_then(Value::as_str);
    let called = content
        .iter()
        .any(|b| matches!(b, ContentBlock::ToolUse { .. }));
    let stop = match (incomplete, called) {
        (Some("max_output_tokens"), _) => StopReason::MaxTokens,
        (Some("content_filter"), _) => StopReason::ContentFilter,
        (Some(other), _) => StopReason::Unknown(other.to_owned()),
        (None, true) => StopReason::ToolUse,
        (None, false) => StopReason::EndTurn,
    };
    let mut resp = InferResponse::new(content, reported.unwrap_or_default(), stop);
    if refused {
        resp.stop_reason = StopReason::ContentFilter;
    }
    resp.usage_reported = usage_reported;
    if complete_usage(&v) {
        resp.usage_completeness = nika_kernel::ai::provider::UsageCompleteness::Complete;
    }
    resp.request_id = v.get("id").and_then(Value::as_str).map(ToOwned::to_owned);
    resp.finish_reason_raw = incomplete
        .or_else(|| v.get("status").and_then(Value::as_str))
        .map(ToOwned::to_owned);
    resp.gen_ai.system = gen_ai_system(rp.profile.id);
    resp.gen_ai.response_id = resp.request_id.clone();
    resp.gen_ai.response_model = v
        .get("model")
        .and_then(Value::as_str)
        .map(ToOwned::to_owned);
    Ok(resp)
}

fn str_of(item: &Value, key: &str) -> String {
    item.get(key)
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_owned()
}

/// The Responses `usage` object as the kernel's split: the input with its cache read and write,
/// the output with its reasoning inside it.
fn usage_from(u: &Value) -> Option<TokenUsage> {
    let at = |pointer: &str| u.pointer(pointer).and_then(Value::as_u64);
    let mut usage = TokenUsage::new(at("/input_tokens")?, at("/output_tokens")?);
    usage.cache_read_tokens = at("/input_tokens_details/cached_tokens");
    usage.cache_write_tokens = at("/input_tokens_details/cache_write_tokens");
    usage.reasoning_tokens = at("/output_tokens_details/reasoning_tokens");
    usage.total_tokens = at("/total_tokens");
    Some(usage)
}

/// Responses' usage law (the reply of 2026-10-16, measured live): the input and the output add
/// up to `total_tokens`; the cache read and write ride within the input, the reasoning within
/// the output; the default service tier served it (flex and priority bill other rates); no
/// server tool spent anything (`tool_usage`: every count 0); every key is known.
#[must_use]
pub(crate) fn complete_usage(v: &Value) -> bool {
    let Some(u) = v.get("usage").and_then(Value::as_object) else {
        return false;
    };
    let at = |k: &str| u.get(k).and_then(Value::as_u64);
    let (Some(input), Some(output), Some(total)) =
        (at("input_tokens"), at("output_tokens"), at("total_tokens"))
    else {
        return false;
    };
    input > 0
        && input.checked_add(output) == Some(total)
        && within(
            u.get("input_tokens_details"),
            &["cached_tokens", "cache_write_tokens"],
            input,
        )
        && within(
            u.get("output_tokens_details"),
            &["reasoning_tokens"],
            output,
        )
        && u.keys().all(|k| {
            matches!(
                k.as_str(),
                "input_tokens"
                    | "input_tokens_details"
                    | "output_tokens"
                    | "output_tokens_details"
                    | "total_tokens"
            )
        })
        && matches!(
            v.get("service_tier").and_then(Value::as_str),
            None | Some("default")
        )
        && v.get("tool_usage").is_none_or(all_zero)
}

/// A details object whose counts are among `keys` and sum within `bound`; absent reads as none.
fn within(details: Option<&Value>, keys: &[&str], bound: u64) -> bool {
    let details: &Map<String, Value> = match details {
        None | Some(Value::Null) => return true,
        Some(Value::Object(details)) => details,
        Some(_) => return false,
    };
    let mut sum = 0_u64;
    for (k, n) in details {
        let Some(n) = n.as_u64() else {
            return false;
        };
        if !keys.contains(&k.as_str()) {
            return false;
        }
        let Some(next) = sum.checked_add(n) else {
            return false;
        };
        sum = next;
    }
    sum <= bound
}

/// Every number in `value` is 0 (a null or an empty object counts nothing).
fn all_zero(value: &Value) -> bool {
    match value {
        Value::Null => true,
        Value::Number(n) => n.as_u64() == Some(0),
        Value::Object(map) => map.values().all(all_zero),
        Value::Array(items) => items.iter().all(all_zero),
        _ => false,
    }
}

#[cfg(test)]
mod tests;
