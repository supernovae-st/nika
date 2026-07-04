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
        obj.insert("max_tokens".to_owned(), json!(mt));
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
    match &req.response_format {
        ResponseFormat::Json => {
            obj.insert(
                "response_format".to_owned(),
                json!({ "type": "json_object" }),
            );
        }
        ResponseFormat::JsonSchema(schema) => {
            // OpenAI's strict mode rejects (HTTP 400) any schema whose
            // object nodes omit `additionalProperties:false` or whose
            // `required` does not list every property. Normalize on that
            // path only — the OpenAI-compatible peers + local servers take
            // the author's schema as written (gemini has its own adapter).
            let schema = if provider_id == "openai" {
                normalize_strict_schema(schema)
            } else {
                schema.clone()
            };
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

/// Recursively rewrite a JSON Schema into `OpenAI` strict-mode shape.
///
/// On every `"type":"object"` node it (1) injects
/// `additionalProperties:false` and (2) sets `required` to ALL declared
/// property keys — a field the author left out of `required` is made
/// *nullable* (`"type":[T,"null"]`, `OpenAI`'s documented optional
/// workaround) rather than dropped, so optionality survives. On every node
/// it also rewrites `oneOf` → `anyOf` (`OpenAI` strict rejects `oneOf` with
/// HTTP 400 "'oneOf' is not permitted"; a well-formed `anyOf` is accepted
/// by both `OpenAI` and gemini). The walk descends into `properties.*`,
/// `items` (single + tuple/`prefixItems`), every `$defs`/`definitions`
/// entry, and the `anyOf`/`allOf`/`oneOf` branches.
fn normalize_strict_schema(schema: &Value) -> Value {
    let mut out = schema.clone();
    normalize_node(&mut out);
    out
}

/// In-place strict-mode rewrite of one schema node and its descendants.
fn normalize_node(node: &mut Value) {
    let Some(obj) = node.as_object_mut() else {
        return;
    };
    descend_subschemas(obj);
    rename_oneof_to_anyof(obj);
    simplify_unsupported(obj);
    if obj.get("type").and_then(Value::as_str) == Some("object") {
        tighten_object(obj);
    }
}

/// Rewrite two keywords `OpenAI` strict does not accept:
/// - `const: X` → `enum: [X]` (`X`'s JSON type is preserved verbatim;
///   `const` 400s "must have a type" / "not permitted", `enum` is
///   supported and a one-member `enum` is the equivalent constraint).
/// - `uniqueItems` → stripped (array-validation-only; not expressible in
///   the structured-output dialect, so it is dropped at the wire — the
///   model is still steered by the prompt + item schema).
fn simplify_unsupported(obj: &mut serde_json::Map<String, Value>) {
    if let Some(c) = obj.remove("const") {
        obj.entry("enum").or_insert_with(|| Value::Array(vec![c]));
    }
    obj.remove("uniqueItems");
}

/// `oneOf` → `anyOf` (`OpenAI` strict permits `anyOf`, not `oneOf`). Runs
/// after [`descend_subschemas`], so the branches are already normalized;
/// this only swaps the keyword. If the node already carries an `anyOf`, the
/// `oneOf` branches are appended to it (both forms must hold under
/// JSON-Schema, and `anyOf`'s "at least one" is the accepted superset of
/// `oneOf`'s "exactly one") — either way no `oneOf` survives.
fn rename_oneof_to_anyof(obj: &mut serde_json::Map<String, Value>) {
    let Some(Value::Array(one_of)) = obj.remove("oneOf") else {
        return;
    };
    match obj.get_mut("anyOf").and_then(Value::as_array_mut) {
        Some(any_of) => any_of.extend(one_of),
        None => {
            obj.insert("anyOf".to_owned(), Value::Array(one_of));
        }
    }
}

/// Recurse into every child that is itself a schema (the composition +
/// container keywords). `properties` optionality is handled by the parent
/// in [`tighten_object`]; here we only recurse to normalize nested objects.
fn descend_subschemas(obj: &mut serde_json::Map<String, Value>) {
    if let Some(props) = obj.get_mut("properties").and_then(Value::as_object_mut) {
        for prop in props.values_mut() {
            normalize_node(prop);
        }
    }
    // `items` is a single subschema (array element) or a tuple array;
    // `prefixItems` (2020-12) is always a tuple array.
    for key in ["items", "prefixItems", "additionalItems"] {
        match obj.get_mut(key) {
            Some(Value::Array(items)) => items.iter_mut().for_each(normalize_node),
            Some(child) => normalize_node(child),
            None => {}
        }
    }
    for key in ["$defs", "definitions"] {
        if let Some(defs) = obj.get_mut(key).and_then(Value::as_object_mut) {
            for def in defs.values_mut() {
                normalize_node(def);
            }
        }
    }
    for key in ["anyOf", "allOf", "oneOf"] {
        if let Some(branches) = obj.get_mut(key).and_then(Value::as_array_mut) {
            branches.iter_mut().for_each(normalize_node);
        }
    }
}

/// Apply the two object-node invariants: `additionalProperties:false` and
/// `required` = all property keys (optionals made nullable first).
fn tighten_object(obj: &mut serde_json::Map<String, Value>) {
    let all_keys: Vec<String> = obj
        .get("properties")
        .and_then(Value::as_object)
        .map(|p| p.keys().cloned().collect())
        .unwrap_or_default();

    // Keys the author already marked required keep their type as written;
    // the rest become nullable so they can stay in `required` (OpenAI's
    // optional shape) without forcing the model to invent a value.
    let already: std::collections::BTreeSet<String> = obj
        .get("required")
        .and_then(Value::as_array)
        .map(|r| {
            r.iter()
                .filter_map(|v| v.as_str().map(str::to_owned))
                .collect()
        })
        .unwrap_or_default();
    if let Some(props) = obj.get_mut("properties").and_then(Value::as_object_mut) {
        for (name, prop) in props.iter_mut() {
            if !already.contains(name) {
                make_nullable(prop);
            }
        }
    }

    obj.insert("additionalProperties".to_owned(), Value::Bool(false));
    obj.insert(
        "required".to_owned(),
        Value::Array(all_keys.into_iter().map(Value::String).collect()),
    );
}

/// Widen a schema node's `type` to include `"null"` (idempotent). A scalar
/// `"type":"string"` becomes `["string","null"]`; an existing array gains
/// `"null"` only if absent; a typeless node (e.g. a `$ref`/`anyOf` form) is
/// left untouched — there is no `type` to widen.
fn make_nullable(prop: &mut Value) {
    let Some(obj) = prop.as_object_mut() else {
        return;
    };
    match obj.get_mut("type") {
        Some(Value::String(t)) => {
            let widened = vec![
                Value::String(std::mem::take(t)),
                Value::String("null".to_owned()),
            ];
            obj.insert("type".to_owned(), Value::Array(widened));
        }
        Some(Value::Array(types)) => {
            if !types.iter().any(|v| v.as_str() == Some("null")) {
                types.push(Value::String("null".to_owned()));
            }
        }
        _ => {}
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

    #[test]
    fn tools_and_response_format_shape() {
        let mut r = req(vec![Message::text(Role::User, "calcule")]);
        r.tools = vec![ToolDef::new("add", "adds", json!({"type":"object"}))];
        r.tool_choice = ToolChoice::Specific("add".into());
        r.response_format = ResponseFormat::JsonSchema(json!({"type":"object"}));
        // groq is an OpenAI-compatible peer: the author's schema is taken
        // verbatim (only `openai` runs the strict-mode normalizer below).
        let body = body_of("m", &r, false, true, "groq").expect("body");
        assert_eq!(body["tools"][0]["type"], "function");
        assert_eq!(body["tools"][0]["function"]["name"], "add");
        assert_eq!(body["tool_choice"]["function"]["name"], "add");
        assert_eq!(body["response_format"]["type"], "json_schema");
        assert_eq!(body["response_format"]["json_schema"]["strict"], true);
        assert_eq!(
            body["response_format"]["json_schema"]["schema"],
            json!({"type":"object"}),
            "non-openai schema passes through unmodified"
        );
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
