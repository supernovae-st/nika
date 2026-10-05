//! Pure native request projection. A native option cannot weaken the context guards.

use nika_kernel::ai::provider::{
    ContentBlock, InferRequest, Message, ProviderError, ResponseFormat, Role, ToolChoice,
};
use serde_json::{Value, json};

use super::{ToolNameMap, refused};

/// Per-invocation capacity, not a daemon setting or an assertion of model/hardware support.
const DEFAULT_CONTEXT: u64 = 65_536;

pub(super) fn endpoint(base: &str) -> Result<String, ProviderError> {
    let mut url = url::Url::parse(base).map_err(|_| refused("invalid endpoint"))?;
    if !matches!(url.scheme(), "http" | "https")
        || !url.username().is_empty()
        || url.password().is_some()
        || url.query().is_some()
        || url.fragment().is_some()
    {
        return Err(refused(
            "endpoint must be an HTTP(S) address without userinfo, query or fragment",
        ));
    }
    let path = url.path().trim_end_matches('/');
    let native = if path.ends_with("/api/chat") {
        path.to_owned()
    } else if let Some(prefix) = path
        .strip_suffix("/v1/chat/completions")
        .or_else(|| path.strip_suffix("/v1"))
    {
        format!("{prefix}/api/chat")
    } else if path.is_empty() {
        "/api/chat".to_owned()
    } else {
        return Err(refused(
            "endpoint path is neither chat completions nor native /api/chat",
        ));
    };
    url.set_path(&native);
    Ok(url.into())
}

pub(super) fn body(
    model: &str,
    req: &InferRequest,
    stream: bool,
    names: &ToolNameMap,
) -> Result<Value, ProviderError> {
    if req.thinking_budget.is_some() {
        return Err(refused(
            "a numeric thinking budget is not supported; nothing was sent",
        ));
    }
    if !matches!(req.tool_choice, ToolChoice::Auto | ToolChoice::None) {
        return Err(refused(
            "required/specific tool choice is not supported; nothing was sent",
        ));
    }
    if req
        .extra
        .params
        .keys()
        .any(|k| !matches!(k.as_str(), "options" | "think" | "keep_alive"))
    {
        return Err(refused(
            "unsupported extra parameter; native context/identity guards cannot be overridden",
        ));
    }
    let mut messages = Vec::new();
    for message in &req.messages {
        push_message(&mut messages, message, names)?;
    }
    let mut body = json!({"model":model,"messages":messages,"stream":stream,
        "truncate":false,"shift":false,"options":options(req)?});
    if !req.tools.is_empty() && !matches!(req.tool_choice, ToolChoice::None) {
        body["tools"] = req
            .tools
            .iter()
            .map(|t| {
                json!({"type":"function","function":{
            "name":names.to_wire(&t.name),"description":t.description,"parameters":t.parameters}})
            })
            .collect();
    }
    match &req.response_format {
        ResponseFormat::Text => {}
        ResponseFormat::Json => body["format"] = json!("json"),
        ResponseFormat::JsonSchema(schema) => body["format"] = schema.clone(),
        _ => return Err(refused("response format is not supported")),
    }
    for key in ["think", "keep_alive"] {
        if let Some(value) = req.extra.params.get(key) {
            body[key] = value.clone();
        }
    }
    Ok(body)
}

fn options(req: &InferRequest) -> Result<Value, ProviderError> {
    let mut options = match req.extra.params.get("options") {
        None => serde_json::Map::new(),
        Some(Value::Object(value)) => value.clone(),
        Some(_) => return Err(refused("options must be an object")),
    };
    let context = match options.get("num_ctx") {
        None => DEFAULT_CONTEXT,
        Some(value) => value
            .as_u64()
            .filter(|n| (1..=1_048_576).contains(n))
            .ok_or_else(|| refused("options.num_ctx must be in 1..=1048576"))?,
    };
    let output = u64::from(req.max_tokens.unwrap_or(4096));
    if output == 0 || output >= context {
        return Err(refused(
            "output token bound must be positive and below options.num_ctx; no prompt was sent or trimmed",
        ));
    }
    for (key, value) in [("num_predict", json!(output)), ("num_ctx", json!(context))] {
        if options.get(key).is_some_and(|v| v != &value) {
            return Err(refused(
                "native options conflict with the request's token bound",
            ));
        }
        options.insert(key.into(), value);
    }
    if let Some(temperature) = req.temperature {
        options.insert("temperature".into(), json!(temperature));
    }
    if !req.stop_sequences.is_empty() {
        options.insert("stop".into(), json!(req.stop_sequences));
    }
    if let Some(seed) = req.replay_seed {
        options.insert("seed".into(), json!(seed));
    }
    Ok(Value::Object(options))
}

fn push_message(
    out: &mut Vec<Value>,
    message: &Message,
    names: &ToolNameMap,
) -> Result<(), ProviderError> {
    let role = match message.role {
        Role::System => "system",
        Role::Assistant => "assistant",
        Role::User => "user",
        Role::Tool => "tool",
        _ => return Err(refused("message role is not supported")),
    };
    let mut text = Vec::new();
    let mut thinking = Vec::new();
    let mut images = Vec::new();
    let mut calls = Vec::new();
    for block in &message.content {
        match block {
            ContentBlock::Text { text: value } => text.push(value.as_str()),
            ContentBlock::Thinking { text: value } => thinking.push(value.as_str()),
            ContentBlock::Image { source, detail } => {
                if detail.as_deref().is_some_and(|v| v != "auto") {
                    return Err(refused("explicit image detail has no native equivalent"));
                }
                let image = super::super::parse_data_image(source)
                    .ok_or_else(|| refused("native images require inline base64 bytes; remote image URLs are not fetched"))?;
                images.push(image.data);
            }
            ContentBlock::ToolUse { id, name, input } => calls.push(json!({"id":id,"function":{
                "name":names.to_wire(name),"arguments":input}})),
            ContentBlock::ToolResult {
                tool_use_id,
                content,
                ..
            } => out.push(json!({
                "role":"tool","tool_call_id":tool_use_id,"content":content})),
            _ => return Err(refused("content block has no native representation")),
        }
    }
    if !text.is_empty() || !thinking.is_empty() || !images.is_empty() || !calls.is_empty() {
        let mut value = json!({"role":role,"content":text.join("\n")});
        if !thinking.is_empty() {
            value["thinking"] = json!(thinking.join("\n"));
        }
        if !images.is_empty() {
            value["images"] = json!(images);
        }
        if !calls.is_empty() {
            value["tool_calls"] = json!(calls);
        }
        out.push(value);
    }
    Ok(())
}
