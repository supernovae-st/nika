// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Ollama's native chat dialect, through the same injected HTTP effect.
//! The compatibility endpoint ignores native context guards. Never trim a request,
//! slide its context, change its model, or retry it through the compatibility API.

mod request;
mod stream;
#[cfg(test)]
mod tests;

use std::collections::BTreeSet;

use bytes::Bytes;
use nika_kernel::ai::provider::{
    ContentBlock, InferEventStream, InferRequest, InferResponse, ProviderError, StopReason,
    TokenUsage,
};
use nika_kernel::http::{HttpPostDyn, HttpRequest};
use serde_json::Value;

use super::{ToolNameMap, gen_ai_system, map_http_err, status_error};
use crate::registry::ResolvedProvider;

fn refused(message: &str) -> ProviderError {
    ProviderError::Other {
        reason: format!("Ollama native chat: {message}"),
    }
}

pub(super) async fn infer<H: HttpPostDyn + Send + Sync + 'static>(
    rp: &ResolvedProvider<H>,
    req: InferRequest,
    sent: &mut bool,
    route: &mut Option<crate::retry::BillingRoute>,
    call: &mut Option<nika_types::cost::InferenceCall>,
) -> Result<InferResponse, ProviderError> {
    let names = ToolNameMap::from_tools(&req.tools);
    let http_req = build_request(rp, &req, false, &names)?;
    let http = rp
        .http
        .as_ref()
        .ok_or_else(|| refused("missing HTTP effect"))?;
    let endpoint = http_req.url.clone();
    *call = Some(nika_types::cost::InferenceCall::new());
    if let Some(call) = call {
        call.requested_endpoint = crate::retry::BillingRoute::new(
            rp.profile.id.into(),
            rp.wire_model.clone(),
            endpoint.clone(),
        )
        .map(|r| r.endpoint);
    }
    *sent = true;
    crate::dispatch_journal::sent(call.as_ref());
    let response = http.post(http_req).await.map_err(|e| map_http_err(&e))?;
    *route = crate::retry::BillingRoute::new(
        rp.profile.id.into(),
        rp.wire_model.clone(),
        response.final_url.clone(),
    );
    crate::retry::record(call, route.as_ref(), None, None);
    if response.final_url != endpoint {
        return Err(refused("transport endpoint changed; result not accepted"));
    }
    if !(200..300).contains(&response.status) {
        return Err(status_error(
            response.status,
            &response.body,
            response.headers.get("retry-after").map(String::as_str),
            &rp.wire_model,
        ));
    }
    let value = parse(&response.body)?;
    if value.get("done").and_then(Value::as_bool) != Some(true) {
        return Err(refused(
            "buffered response has no terminal done=true; no complete answer",
        ));
    }
    let content = contents(&value, &names, &mut BTreeSet::new())?;
    let has_tools = content
        .iter()
        .any(|b| matches!(b, ContentBlock::ToolUse { .. }));
    let usage = usage(&value);
    let reported = usage.is_some();
    let mut result =
        InferResponse::new(content, usage.unwrap_or_default(), stop(&value, has_tools));
    result.usage_reported = reported;
    result.finish_reason_raw = value
        .get("done_reason")
        .and_then(Value::as_str)
        .map(str::to_owned);
    result.gen_ai.system = gen_ai_system(rp.profile.id);
    result.gen_ai.response_model = value
        .get("model")
        .and_then(Value::as_str)
        .map(str::to_owned);
    crate::retry::record(call, route.as_ref(), Some(&result), None);
    Ok(result)
}

pub(super) async fn infer_stream<H: HttpPostDyn + Send + Sync + 'static>(
    rp: &ResolvedProvider<H>,
    req: InferRequest,
) -> Result<InferEventStream, ProviderError> {
    let names = ToolNameMap::from_tools(&req.tools);
    let request = build_request(rp, &req, true, &names)?;
    let endpoint = request.url.clone();
    let response = rp
        .http
        .as_ref()
        .ok_or_else(|| refused("missing HTTP effect"))?
        .send_streaming(request)
        .await
        .map_err(|e| map_http_err(&e))?;
    if response.final_url != endpoint {
        return Err(refused("stream endpoint changed; result not accepted"));
    }
    if !(200..300).contains(&response.status) {
        return Err(super::stream_status_error(response, &rp.wire_model).await);
    }
    Ok(Box::pin(stream::NativeStream::new(response.body, names)))
}

fn build_request(
    rp: &ResolvedProvider<impl Sized>,
    req: &InferRequest,
    stream: bool,
    names: &ToolNameMap,
) -> Result<HttpRequest, ProviderError> {
    if let Some(account) = &rp.admission {
        return Err(
            account.refuse("Ollama native chat has no qualified catalog admission settlement")
        );
    }
    super::reasoning::unsupported(req, rp.profile.id, &rp.wire_model)?;
    let body = request::body(&rp.wire_model, req, stream, names)?;
    let mut request = HttpRequest::post(request::endpoint(&rp.base_url)?);
    request
        .headers
        .insert("content-type".into(), "application/json".into());
    if let Some(key) = &rp.key {
        request
            .headers
            .insert("authorization".into(), format!("Bearer {}", key.expose()));
    }
    request.body = Some(Bytes::from(
        serde_json::to_vec(&body).map_err(|_| refused("request serialization failed"))?,
    ));
    request.timeout = super::transport_deadline(req, stream);
    request.follow_redirects = false;
    Ok(request)
}

fn parse(bytes: &[u8]) -> Result<Value, ProviderError> {
    let value = super::bounded_json::parse(bytes)
        .map_err(|_| refused("invalid or duplicate JSON fields"))?;
    if value.get("error").is_some() {
        return Err(refused(
            "server refused generation; check context capacity and local server logs; no complete answer",
        ));
    }
    if !value.is_object() || value.get("done").and_then(Value::as_bool).is_none() {
        return Err(refused("response lacks the native completion marker"));
    }
    Ok(value)
}

fn usage(value: &Value) -> Option<TokenUsage> {
    Some(TokenUsage::new(
        value.get("prompt_eval_count")?.as_u64()?,
        value.get("eval_count")?.as_u64()?,
    ))
}

fn stop(value: &Value, has_tools: bool) -> StopReason {
    match value.get("done_reason").and_then(Value::as_str) {
        Some("length") => StopReason::MaxTokens,
        Some("stop") if has_tools => StopReason::ToolUse,
        Some("stop") => StopReason::EndTurn,
        reason => StopReason::Unknown(reason.unwrap_or("missing").to_owned()),
    }
}

fn contents(
    value: &Value,
    names: &ToolNameMap,
    tool_ids: &mut BTreeSet<String>,
) -> Result<Vec<ContentBlock>, ProviderError> {
    let Some(message) = value.get("message").and_then(Value::as_object) else {
        return Err(refused("response lacks a message object"));
    };
    let mut result = Vec::new();
    for key in ["thinking", "content"] {
        if let Some(value) = message.get(key) {
            let text = value
                .as_str()
                .ok_or_else(|| refused("message content is not text"))?;
            if !text.is_empty() {
                result.push(if key == "thinking" {
                    ContentBlock::Thinking { text: text.into() }
                } else {
                    ContentBlock::Text { text: text.into() }
                });
            }
        }
    }
    if let Some(calls) = message.get("tool_calls") {
        for call in calls
            .as_array()
            .ok_or_else(|| refused("tool calls are not an array"))?
        {
            let name = call
                .pointer("/function/name")
                .and_then(Value::as_str)
                .filter(|s| !s.is_empty())
                .ok_or_else(|| refused("tool call has no name"))?;
            let input = call
                .pointer("/function/arguments")
                .filter(|v| v.is_object())
                .ok_or_else(|| refused("tool arguments are not an object"))?;
            let id = call
                .get("id")
                .and_then(Value::as_str)
                .filter(|s| !s.is_empty())
                .map_or_else(|| format!("ollama-call-{}", tool_ids.len()), str::to_owned);
            if !tool_ids.insert(id.clone()) {
                return Err(refused("duplicate tool call identity"));
            }
            result.push(ContentBlock::ToolUse {
                id,
                name: names.to_canonical(name),
                input: input.clone(),
            });
        }
    }
    Ok(result)
}
