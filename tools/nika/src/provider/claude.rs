//! Claude provider using Anthropic API

use super::{
    ChatResponse, ContentBlock, Message, MessageContent, MessageRole, Provider, StopReason,
    ToolCall, ToolDefinition, Usage, CLAUDE_DEFAULT_MODEL,
};
use crate::util::{CONNECT_TIMEOUT, INFER_TIMEOUT};
use anyhow::{Context, Result};
use async_trait::async_trait;
use reqwest::Client;
use serde_json::{json, Value};

pub struct ClaudeProvider {
    api_key: String,
    client: Client,
}

impl ClaudeProvider {
    pub fn new() -> Result<Self> {
        let api_key = std::env::var("ANTHROPIC_API_KEY").context("ANTHROPIC_API_KEY not set")?;

        let client = Client::builder()
            .timeout(INFER_TIMEOUT)
            .connect_timeout(CONNECT_TIMEOUT)
            .user_agent("nika-cli/0.1")
            .build()
            .context("Failed to build HTTP client")?;

        Ok(Self { api_key, client })
    }

    /// Resolve model aliases to full Anthropic model IDs
    fn resolve_model<'a>(&self, model: &'a str) -> &'a str {
        // Sonnet variants
        if model.eq_ignore_ascii_case("claude-sonnet-4-5")
            || model.eq_ignore_ascii_case("claude-sonnet")
            || model.eq_ignore_ascii_case("sonnet")
        {
            "claude-sonnet-4-20250514"
        }
        // Opus variants
        else if model.eq_ignore_ascii_case("claude-opus-4")
            || model.eq_ignore_ascii_case("claude-opus")
            || model.eq_ignore_ascii_case("opus")
        {
            "claude-opus-4-20250514"
        }
        // Haiku variants
        else if model.eq_ignore_ascii_case("claude-haiku") || model.eq_ignore_ascii_case("haiku")
        {
            "claude-3-5-haiku-20241022"
        }
        // Pass through if already a full model ID (case-insensitive prefix check)
        else if model
            .get(..7)
            .is_some_and(|s| s.eq_ignore_ascii_case("claude-"))
        {
            model
        }
        // Default
        else {
            "claude-sonnet-4-20250514"
        }
    }
}

#[async_trait]
impl Provider for ClaudeProvider {
    fn default_model(&self) -> &str {
        CLAUDE_DEFAULT_MODEL
    }

    fn name(&self) -> &str {
        "claude"
    }

    async fn infer(&self, prompt: &str, model: &str) -> Result<String> {
        let resolved_model = self.resolve_model(model);

        let response = self
            .client
            .post("https://api.anthropic.com/v1/messages")
            .header("x-api-key", &self.api_key)
            .header("anthropic-version", "2023-06-01")
            .header("content-type", "application/json")
            .json(&json!({
                "model": resolved_model,
                "max_tokens": 4096,
                "messages": [
                    { "role": "user", "content": prompt }
                ]
            }))
            .send()
            .await
            .context("Failed to send request to Claude API")?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            anyhow::bail!("Claude API error {}: {}", status, body);
        }

        let json: Value = response.json().await?;
        let text = json["content"][0]["text"]
            .as_str()
            .context("Invalid response format from Claude API")?;

        Ok(text.to_string())
    }

    async fn chat(
        &self,
        messages: &[Message],
        tools: Option<&[ToolDefinition]>,
        model: &str,
    ) -> Result<ChatResponse> {
        let resolved_model = self.resolve_model(model);

        // Convert messages to Claude API format
        let api_messages: Vec<Value> = messages
            .iter()
            .filter(|m| m.role != MessageRole::System)
            .map(|m| self.message_to_api(m))
            .collect();

        // Extract system message if present
        let system_prompt: Option<String> = messages.iter().find_map(|m| {
            if m.role == MessageRole::System {
                m.content.as_text()
            } else {
                None
            }
        });

        // Build request body
        let mut body = json!({
            "model": resolved_model,
            "max_tokens": 4096,
            "messages": api_messages,
        });

        if let Some(system) = system_prompt {
            body["system"] = json!(system);
        }

        if let Some(tool_defs) = tools {
            if !tool_defs.is_empty() {
                let api_tools: Vec<Value> = tool_defs
                    .iter()
                    .map(|t| {
                        json!({
                            "name": t.name,
                            "description": t.description,
                            "input_schema": t.input_schema,
                        })
                    })
                    .collect();
                body["tools"] = json!(api_tools);
            }
        }

        let response = self
            .client
            .post("https://api.anthropic.com/v1/messages")
            .header("x-api-key", &self.api_key)
            .header("anthropic-version", "2023-06-01")
            .header("content-type", "application/json")
            .json(&body)
            .send()
            .await
            .context("Failed to send chat request to Claude API")?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            anyhow::bail!("Claude API error {}: {}", status, body);
        }

        let json: Value = response.json().await?;
        self.parse_response(&json)
    }
}

impl ClaudeProvider {
    /// Convert a Message to Claude API format
    fn message_to_api(&self, msg: &Message) -> Value {
        let role = match msg.role {
            MessageRole::User => "user",
            MessageRole::Assistant => "assistant",
            MessageRole::Tool => "user", // Tool results go as user messages
            MessageRole::System => "user", // Should be filtered out
        };

        let content = match &msg.content {
            MessageContent::Text(text) => {
                if msg.role == MessageRole::Tool {
                    // Tool result format
                    json!([{
                        "type": "tool_result",
                        "tool_use_id": msg.tool_call_id.as_deref().unwrap_or("unknown"),
                        "content": text,
                    }])
                } else {
                    json!(text)
                }
            }
            MessageContent::Blocks(blocks) => {
                let api_blocks: Vec<Value> = blocks
                    .iter()
                    .map(|b| match b {
                        ContentBlock::Text { text } => json!({
                            "type": "text",
                            "text": text,
                        }),
                        ContentBlock::ToolUse { id, name, input } => json!({
                            "type": "tool_use",
                            "id": id,
                            "name": name,
                            "input": input,
                        }),
                        ContentBlock::ToolResult {
                            tool_use_id,
                            content,
                            is_error,
                        } => {
                            let mut result = json!({
                                "type": "tool_result",
                                "tool_use_id": tool_use_id,
                                "content": content,
                            });
                            if let Some(true) = is_error {
                                result["is_error"] = json!(true);
                            }
                            result
                        }
                    })
                    .collect();
                json!(api_blocks)
            }
        };

        json!({
            "role": role,
            "content": content,
        })
    }

    /// Parse Claude API response into ChatResponse
    fn parse_response(&self, json: &Value) -> Result<ChatResponse> {
        // Parse content blocks
        let content_blocks = json["content"]
            .as_array()
            .context("Missing 'content' array in response")?;

        let mut blocks: Vec<ContentBlock> = Vec::new();
        let mut tool_calls: Vec<ToolCall> = Vec::new();

        for block in content_blocks {
            let block_type = block["type"].as_str().unwrap_or("text");
            match block_type {
                "text" => {
                    let text = block["text"].as_str().unwrap_or("").to_string();
                    blocks.push(ContentBlock::Text { text });
                }
                "tool_use" => {
                    let id = block["id"].as_str().unwrap_or("").to_string();
                    let name = block["name"].as_str().unwrap_or("").to_string();
                    let input = block["input"].clone();

                    blocks.push(ContentBlock::ToolUse {
                        id: id.clone(),
                        name: name.clone(),
                        input: input.clone(),
                    });
                    tool_calls.push(ToolCall {
                        id,
                        name,
                        arguments: input,
                    });
                }
                _ => {}
            }
        }

        // Determine content format
        let content = if blocks.len() == 1 {
            if let ContentBlock::Text { text } = &blocks[0] {
                MessageContent::Text(text.clone())
            } else {
                MessageContent::Blocks(blocks)
            }
        } else {
            MessageContent::Blocks(blocks)
        };

        // Parse stop reason
        let stop_reason = match json["stop_reason"].as_str() {
            Some("end_turn") => StopReason::EndTurn,
            Some("tool_use") => StopReason::ToolUse,
            Some("max_tokens") => StopReason::MaxTokens,
            Some("stop_sequence") => StopReason::StopSequence,
            _ => StopReason::Unknown,
        };

        // Parse usage
        let usage = Usage {
            input_tokens: json["usage"]["input_tokens"].as_u64().unwrap_or(0) as u32,
            output_tokens: json["usage"]["output_tokens"].as_u64().unwrap_or(0) as u32,
            cache_creation_input_tokens: json["usage"]["cache_creation_input_tokens"]
                .as_u64()
                .unwrap_or(0) as u32,
            cache_read_input_tokens: json["usage"]["cache_read_input_tokens"]
                .as_u64()
                .unwrap_or(0) as u32,
        };

        Ok(ChatResponse {
            content,
            tool_calls,
            stop_reason,
            usage,
        })
    }
}
