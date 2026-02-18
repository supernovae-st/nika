//! OpenAI provider using OpenAI API

use super::{
    ChatResponse, ContentBlock, Message, MessageContent, MessageRole, Provider, StopReason,
    ToolCall, ToolDefinition, Usage, OPENAI_DEFAULT_MODEL,
};
use crate::util::{CONNECT_TIMEOUT, INFER_TIMEOUT};
use anyhow::{Context, Result};
use async_trait::async_trait;
use reqwest::Client;
use serde_json::{json, Value};

pub struct OpenAIProvider {
    api_key: String,
    client: Client,
}

impl OpenAIProvider {
    pub fn new() -> Result<Self> {
        let api_key = std::env::var("OPENAI_API_KEY").context("OPENAI_API_KEY not set")?;

        let client = Client::builder()
            .timeout(INFER_TIMEOUT)
            .connect_timeout(CONNECT_TIMEOUT)
            .user_agent("nika-cli/0.1")
            .build()
            .context("Failed to build HTTP client")?;

        Ok(Self { api_key, client })
    }

    /// Map model names to valid OpenAI models
    fn resolve_model(&self, model: &str) -> &'static str {
        // Direct OpenAI models - zero allocation exact matches
        if model.eq_ignore_ascii_case("gpt-4o") {
            return "gpt-4o";
        }
        if model.eq_ignore_ascii_case("gpt-4o-mini") {
            return "gpt-4o-mini";
        }
        if model.eq_ignore_ascii_case("gpt-4-turbo") {
            return "gpt-4-turbo";
        }
        if model.eq_ignore_ascii_case("gpt-3.5-turbo") {
            return "gpt-3.5-turbo";
        }
        if model.eq_ignore_ascii_case("o1") {
            return "o1";
        }
        if model.eq_ignore_ascii_case("o1-mini") {
            return "o1-mini";
        }
        if model.eq_ignore_ascii_case("o1-preview") {
            return "o1-preview";
        }

        // Claude model fallback - single allocation for contains checks
        let model_lower = model.to_ascii_lowercase();

        // Claude Haiku → GPT-4o-mini (fast/cheap)
        if model_lower.contains("haiku") {
            return "gpt-4o-mini";
        }

        // Claude Sonnet/Opus → GPT-4o
        if model_lower.contains("sonnet")
            || model_lower.contains("opus")
            || model_lower.contains("claude")
        {
            return "gpt-4o";
        }

        // Default to gpt-4o
        "gpt-4o"
    }
}

#[async_trait]
impl Provider for OpenAIProvider {
    fn default_model(&self) -> &str {
        OPENAI_DEFAULT_MODEL
    }

    fn name(&self) -> &str {
        "openai"
    }

    async fn infer(&self, prompt: &str, model: &str) -> Result<String> {
        let resolved_model = self.resolve_model(model);

        let response = self
            .client
            .post("https://api.openai.com/v1/chat/completions")
            .header("Authorization", format!("Bearer {}", self.api_key))
            .header("Content-Type", "application/json")
            .json(&json!({
                "model": resolved_model,
                "messages": [
                    { "role": "user", "content": prompt }
                ]
            }))
            .send()
            .await
            .context("Failed to send request to OpenAI API")?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            anyhow::bail!("OpenAI API error {}: {}", status, body);
        }

        let json: Value = response.json().await?;
        let text = json["choices"][0]["message"]["content"]
            .as_str()
            .context("Invalid response format from OpenAI API")?;

        Ok(text.to_string())
    }

    async fn chat(
        &self,
        messages: &[Message],
        tools: Option<&[ToolDefinition]>,
        model: &str,
    ) -> Result<ChatResponse> {
        let resolved_model = self.resolve_model(model);

        // Convert messages to OpenAI API format
        let api_messages: Vec<Value> = messages.iter().map(|m| self.message_to_api(m)).collect();

        // Build request body
        let mut body = json!({
            "model": resolved_model,
            "messages": api_messages,
        });

        if let Some(tool_defs) = tools {
            if !tool_defs.is_empty() {
                let api_tools: Vec<Value> = tool_defs
                    .iter()
                    .map(|t| {
                        json!({
                            "type": "function",
                            "function": {
                                "name": t.name,
                                "description": t.description,
                                "parameters": t.input_schema,
                            }
                        })
                    })
                    .collect();
                body["tools"] = json!(api_tools);
            }
        }

        let response = self
            .client
            .post("https://api.openai.com/v1/chat/completions")
            .header("Authorization", format!("Bearer {}", self.api_key))
            .header("Content-Type", "application/json")
            .json(&body)
            .send()
            .await
            .context("Failed to send chat request to OpenAI API")?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            anyhow::bail!("OpenAI API error {}: {}", status, body);
        }

        let json: Value = response.json().await?;
        self.parse_response(&json)
    }
}

impl OpenAIProvider {
    /// Convert a Message to OpenAI API format
    fn message_to_api(&self, msg: &Message) -> Value {
        let role = match msg.role {
            MessageRole::User => "user",
            MessageRole::Assistant => "assistant",
            MessageRole::Tool => "tool",
            MessageRole::System => "system",
        };

        match &msg.content {
            MessageContent::Text(text) => {
                let mut message = json!({
                    "role": role,
                    "content": text,
                });
                if let Some(tool_call_id) = &msg.tool_call_id {
                    message["tool_call_id"] = json!(tool_call_id);
                }
                message
            }
            MessageContent::Blocks(blocks) => {
                // For assistant messages with tool calls
                if msg.role == MessageRole::Assistant {
                    let mut tool_calls: Vec<Value> = Vec::new();
                    let mut text_content = String::new();

                    for block in blocks {
                        match block {
                            ContentBlock::Text { text } => {
                                text_content.push_str(text);
                            }
                            ContentBlock::ToolUse { id, name, input } => {
                                tool_calls.push(json!({
                                    "id": id,
                                    "type": "function",
                                    "function": {
                                        "name": name,
                                        "arguments": input.to_string(),
                                    }
                                }));
                            }
                            _ => {}
                        }
                    }

                    let mut message = json!({
                        "role": "assistant",
                    });
                    if !text_content.is_empty() {
                        message["content"] = json!(text_content);
                    }
                    if !tool_calls.is_empty() {
                        message["tool_calls"] = json!(tool_calls);
                    }
                    message
                } else {
                    // Extract text from blocks for other roles
                    let text: String = blocks
                        .iter()
                        .filter_map(|b| match b {
                            ContentBlock::Text { text } => Some(text.as_str()),
                            _ => None,
                        })
                        .collect::<Vec<_>>()
                        .join("\n");
                    json!({
                        "role": role,
                        "content": text,
                    })
                }
            }
        }
    }

    /// Parse OpenAI API response into ChatResponse
    fn parse_response(&self, json: &Value) -> Result<ChatResponse> {
        let choice = &json["choices"][0];
        let message = &choice["message"];

        // Parse content
        let content_text = message["content"].as_str().map(|s| s.to_string());
        let mut tool_calls: Vec<ToolCall> = Vec::new();

        // Parse tool calls if present
        if let Some(api_tool_calls) = message["tool_calls"].as_array() {
            for tc in api_tool_calls {
                let id = tc["id"].as_str().unwrap_or("").to_string();
                let name = tc["function"]["name"].as_str().unwrap_or("").to_string();
                let arguments_str = tc["function"]["arguments"].as_str().unwrap_or("{}");
                let arguments: Value = serde_json::from_str(arguments_str).unwrap_or(json!({}));

                tool_calls.push(ToolCall {
                    id,
                    name,
                    arguments,
                });
            }
        }

        // Build content
        let content = if tool_calls.is_empty() {
            MessageContent::Text(content_text.unwrap_or_default())
        } else {
            let mut blocks: Vec<ContentBlock> = Vec::new();
            if let Some(text) = content_text {
                if !text.is_empty() {
                    blocks.push(ContentBlock::Text { text });
                }
            }
            for tc in &tool_calls {
                blocks.push(ContentBlock::ToolUse {
                    id: tc.id.clone(),
                    name: tc.name.clone(),
                    input: tc.arguments.clone(),
                });
            }
            if blocks.len() == 1 {
                if let ContentBlock::Text { text } = &blocks[0] {
                    MessageContent::Text(text.clone())
                } else {
                    MessageContent::Blocks(blocks)
                }
            } else {
                MessageContent::Blocks(blocks)
            }
        };

        // Parse finish reason
        let stop_reason = match choice["finish_reason"].as_str() {
            Some("stop") => StopReason::EndTurn,
            Some("tool_calls") => StopReason::ToolUse,
            Some("length") => StopReason::MaxTokens,
            _ => StopReason::Unknown,
        };

        // Parse usage
        let usage = Usage {
            input_tokens: json["usage"]["prompt_tokens"].as_u64().unwrap_or(0) as u32,
            output_tokens: json["usage"]["completion_tokens"].as_u64().unwrap_or(0) as u32,
            cache_creation_input_tokens: 0,
            cache_read_input_tokens: 0,
        };

        Ok(ChatResponse {
            content,
            tool_calls,
            stop_reason,
            usage,
        })
    }
}
