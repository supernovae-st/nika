//! Provider Types for LLM Communication
//!
//! This module defines the core types used for communicating with LLM providers.
//! Supports tool calling, multi-turn conversations, and usage tracking.
//!
//! **DEPRECATED (v0.3.1)**: These types are deprecated in favor of rig-core types.
//! The [`RigAgentLoop`](crate::runtime::RigAgentLoop) uses rig-core's native types directly.
//!
//! ## Migration Path
//! | Old Type | New (rig-core) |
//! |----------|----------------|
//! | `Message` | Use rig's message types |
//! | `ToolCall` | Use rig's `ToolCall` |
//! | `ToolDefinition` | Use `NikaMcpToolDef` from `provider::rig` |
//!
//! These types remain for backward compatibility with executor.rs and agent_loop.rs.

use serde::{Deserialize, Serialize};

// ============================================================================
// Message Types
// ============================================================================

/// A message in a conversation with an LLM
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Message {
    /// Role of the message sender
    pub role: MessageRole,
    /// Content of the message
    pub content: MessageContent,
    /// Tool call ID (for tool results only)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_call_id: Option<String>,
}

impl Message {
    /// Create a user message with text content
    pub fn user(text: impl Into<String>) -> Self {
        Self {
            role: MessageRole::User,
            content: MessageContent::Text(text.into()),
            tool_call_id: None,
        }
    }

    /// Create an assistant message with text content
    pub fn assistant(text: impl Into<String>) -> Self {
        Self {
            role: MessageRole::Assistant,
            content: MessageContent::Text(text.into()),
            tool_call_id: None,
        }
    }

    /// Create a tool result message
    pub fn tool_result(tool_call_id: impl Into<String>, result: impl Into<String>) -> Self {
        Self {
            role: MessageRole::Tool,
            content: MessageContent::Text(result.into()),
            tool_call_id: Some(tool_call_id.into()),
        }
    }

    /// Create a system message
    pub fn system(text: impl Into<String>) -> Self {
        Self {
            role: MessageRole::System,
            content: MessageContent::Text(text.into()),
            tool_call_id: None,
        }
    }

    /// Create an assistant message with content blocks
    pub fn assistant_blocks(blocks: Vec<ContentBlock>) -> Self {
        Self {
            role: MessageRole::Assistant,
            content: MessageContent::Blocks(blocks),
            tool_call_id: None,
        }
    }
}

/// Role of a message sender
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "lowercase")]
pub enum MessageRole {
    /// User message
    User,
    /// Assistant (LLM) response
    Assistant,
    /// Tool execution result
    Tool,
    /// System prompt
    System,
}

/// Content of a message - either simple text or content blocks
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(untagged)]
pub enum MessageContent {
    /// Simple text content
    Text(String),
    /// Array of content blocks (for tool use/results)
    Blocks(Vec<ContentBlock>),
}

impl MessageContent {
    /// Extract text content, joining multiple text blocks if needed
    pub fn as_text(&self) -> Option<String> {
        match self {
            MessageContent::Text(s) => Some(s.clone()),
            MessageContent::Blocks(blocks) => {
                let texts: Vec<&str> = blocks
                    .iter()
                    .filter_map(|b| match b {
                        ContentBlock::Text { text } => Some(text.as_str()),
                        _ => None,
                    })
                    .collect();
                if texts.is_empty() {
                    None
                } else {
                    Some(texts.join("\n"))
                }
            }
        }
    }
}

/// A block of content in a message
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ContentBlock {
    /// Text content
    Text { text: String },
    /// Tool use request from assistant
    ToolUse {
        id: String,
        name: String,
        input: serde_json::Value,
    },
    /// Tool execution result
    ToolResult {
        tool_use_id: String,
        content: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        is_error: Option<bool>,
    },
}

// ============================================================================
// Tool Definition Types
// ============================================================================

/// Definition of a tool available to the LLM
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ToolDefinition {
    /// Name of the tool
    pub name: String,
    /// Description of what the tool does
    pub description: String,
    /// JSON Schema for the tool's input parameters
    pub input_schema: serde_json::Value,
}

impl ToolDefinition {
    /// Create a new tool definition
    pub fn new(
        name: impl Into<String>,
        description: impl Into<String>,
        input_schema: serde_json::Value,
    ) -> Self {
        Self {
            name: name.into(),
            description: description.into(),
            input_schema,
        }
    }
}

/// A tool call requested by the LLM
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ToolCall {
    /// Unique ID of this tool call
    pub id: String,
    /// Name of the tool to invoke
    pub name: String,
    /// Arguments to pass to the tool (JSON)
    pub arguments: serde_json::Value,
}

// ============================================================================
// Response Types
// ============================================================================

/// Response from an LLM chat completion
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ChatResponse {
    /// Content of the response
    pub content: MessageContent,
    /// Tool calls requested by the LLM
    #[serde(default)]
    pub tool_calls: Vec<ToolCall>,
    /// Reason the response ended
    pub stop_reason: StopReason,
    /// Token usage statistics
    pub usage: Usage,
}

impl ChatResponse {
    /// Check if the response contains tool calls
    pub fn has_tool_calls(&self) -> bool {
        !self.tool_calls.is_empty()
    }

    /// Get the text content of the response
    pub fn text(&self) -> Option<String> {
        self.content.as_text()
    }
}

/// Reason the LLM stopped generating
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum StopReason {
    /// Natural end of response
    #[default]
    EndTurn,
    /// Tool use requested
    ToolUse,
    /// Maximum tokens reached
    MaxTokens,
    /// Stop sequence encountered
    StopSequence,
    /// Unknown or unrecognized reason
    Unknown,
}

// ============================================================================
// Usage and Cost Types
// ============================================================================

/// Token usage statistics for a request
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct Usage {
    /// Number of input tokens
    pub input_tokens: u32,
    /// Number of output tokens
    pub output_tokens: u32,
    /// Tokens used for cache creation (Claude extended thinking)
    #[serde(default)]
    pub cache_creation_input_tokens: u32,
    /// Tokens read from cache (Claude prompt caching)
    #[serde(default)]
    pub cache_read_input_tokens: u32,
}

impl Usage {
    /// Create a new Usage with basic token counts
    pub fn new(input_tokens: u32, output_tokens: u32) -> Self {
        Self {
            input_tokens,
            output_tokens,
            cache_creation_input_tokens: 0,
            cache_read_input_tokens: 0,
        }
    }

    /// Total tokens used
    pub fn total_tokens(&self) -> u32 {
        self.input_tokens + self.output_tokens
    }

    /// Estimate cost in USD for Claude models
    ///
    /// Pricing per million tokens (as of 2025):
    /// - Sonnet 4: $3 input, $15 output
    /// - Opus 4: $15 input, $75 output
    /// - Haiku 3.5: $0.80 input, $4 output
    pub fn estimate_cost_usd(&self, model: &str) -> f64 {
        let (input_price, output_price) = Self::get_pricing(model);

        let input_cost = (self.input_tokens as f64 / 1_000_000.0) * input_price;
        let output_cost = (self.output_tokens as f64 / 1_000_000.0) * output_price;

        // Cache tokens are charged at 25% of base rate for reads, 125% for writes
        let cache_read_cost =
            (self.cache_read_input_tokens as f64 / 1_000_000.0) * input_price * 0.25;
        let cache_write_cost =
            (self.cache_creation_input_tokens as f64 / 1_000_000.0) * input_price * 1.25;

        input_cost + output_cost + cache_read_cost + cache_write_cost
    }

    /// Get pricing per million tokens (input, output)
    fn get_pricing(model: &str) -> (f64, f64) {
        let model_lower = model.to_lowercase();

        // Claude Opus 4
        if model_lower.contains("opus") {
            return (15.0, 75.0);
        }

        // Claude Haiku
        if model_lower.contains("haiku") {
            return (0.80, 4.0);
        }

        // Claude Sonnet (default for Claude models)
        if model_lower.contains("sonnet") || model_lower.contains("claude") {
            return (3.0, 15.0);
        }

        // OpenAI GPT-4o
        if model_lower.contains("gpt-4o") && !model_lower.contains("mini") {
            return (2.50, 10.0);
        }

        // OpenAI GPT-4o-mini
        if model_lower.contains("gpt-4o-mini") {
            return (0.15, 0.60);
        }

        // Default to Sonnet pricing
        (3.0, 15.0)
    }
}

impl std::ops::Add for Usage {
    type Output = Self;

    fn add(self, other: Self) -> Self {
        Self {
            input_tokens: self.input_tokens + other.input_tokens,
            output_tokens: self.output_tokens + other.output_tokens,
            cache_creation_input_tokens: self.cache_creation_input_tokens
                + other.cache_creation_input_tokens,
            cache_read_input_tokens: self.cache_read_input_tokens + other.cache_read_input_tokens,
        }
    }
}

impl std::ops::AddAssign for Usage {
    fn add_assign(&mut self, other: Self) {
        self.input_tokens += other.input_tokens;
        self.output_tokens += other.output_tokens;
        self.cache_creation_input_tokens += other.cache_creation_input_tokens;
        self.cache_read_input_tokens += other.cache_read_input_tokens;
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use pretty_assertions::assert_eq;
    use serde_json::json;

    // ------------------------------------------------------------------------
    // Message Tests
    // ------------------------------------------------------------------------

    #[test]
    fn test_message_user_creates_user_role() {
        let msg = Message::user("Hello");
        assert_eq!(msg.role, MessageRole::User);
        assert_eq!(msg.content, MessageContent::Text("Hello".to_string()));
        assert_eq!(msg.tool_call_id, None);
    }

    #[test]
    fn test_message_assistant_creates_assistant_role() {
        let msg = Message::assistant("Hi there");
        assert_eq!(msg.role, MessageRole::Assistant);
        assert_eq!(msg.content, MessageContent::Text("Hi there".to_string()));
    }

    #[test]
    fn test_message_tool_result_has_tool_call_id() {
        let msg = Message::tool_result("call_123", "Result data");
        assert_eq!(msg.role, MessageRole::Tool);
        assert_eq!(msg.tool_call_id, Some("call_123".to_string()));
        assert_eq!(msg.content, MessageContent::Text("Result data".to_string()));
    }

    #[test]
    fn test_message_system_creates_system_role() {
        let msg = Message::system("You are a helpful assistant");
        assert_eq!(msg.role, MessageRole::System);
    }

    #[test]
    fn test_message_assistant_blocks_creates_blocks_content() {
        let blocks = vec![
            ContentBlock::Text {
                text: "Let me help".to_string(),
            },
            ContentBlock::ToolUse {
                id: "tool_1".to_string(),
                name: "search".to_string(),
                input: json!({"query": "rust"}),
            },
        ];
        let msg = Message::assistant_blocks(blocks.clone());
        assert_eq!(msg.role, MessageRole::Assistant);
        assert_eq!(msg.content, MessageContent::Blocks(blocks));
    }

    // ------------------------------------------------------------------------
    // MessageContent Tests
    // ------------------------------------------------------------------------

    #[test]
    fn test_message_content_as_text_from_text() {
        let content = MessageContent::Text("Hello".to_string());
        assert_eq!(content.as_text(), Some("Hello".to_string()));
    }

    #[test]
    fn test_message_content_as_text_from_blocks() {
        let content = MessageContent::Blocks(vec![
            ContentBlock::Text {
                text: "First".to_string(),
            },
            ContentBlock::ToolUse {
                id: "1".to_string(),
                name: "test".to_string(),
                input: json!({}),
            },
            ContentBlock::Text {
                text: "Second".to_string(),
            },
        ]);
        assert_eq!(content.as_text(), Some("First\nSecond".to_string()));
    }

    #[test]
    fn test_message_content_as_text_empty_blocks() {
        let content = MessageContent::Blocks(vec![ContentBlock::ToolUse {
            id: "1".to_string(),
            name: "test".to_string(),
            input: json!({}),
        }]);
        assert_eq!(content.as_text(), None);
    }

    // ------------------------------------------------------------------------
    // MessageRole Serialization Tests
    // ------------------------------------------------------------------------

    #[test]
    fn test_message_role_serializes_lowercase() {
        assert_eq!(
            serde_json::to_string(&MessageRole::User).unwrap(),
            "\"user\""
        );
        assert_eq!(
            serde_json::to_string(&MessageRole::Assistant).unwrap(),
            "\"assistant\""
        );
        assert_eq!(
            serde_json::to_string(&MessageRole::Tool).unwrap(),
            "\"tool\""
        );
        assert_eq!(
            serde_json::to_string(&MessageRole::System).unwrap(),
            "\"system\""
        );
    }

    #[test]
    fn test_message_role_deserializes_lowercase() {
        assert_eq!(
            serde_json::from_str::<MessageRole>("\"user\"").unwrap(),
            MessageRole::User
        );
        assert_eq!(
            serde_json::from_str::<MessageRole>("\"assistant\"").unwrap(),
            MessageRole::Assistant
        );
    }

    // ------------------------------------------------------------------------
    // ContentBlock Tests
    // ------------------------------------------------------------------------

    #[test]
    fn test_content_block_text_serialization() {
        let block = ContentBlock::Text {
            text: "Hello".to_string(),
        };
        let json = serde_json::to_value(&block).unwrap();
        assert_eq!(json, json!({"type": "text", "text": "Hello"}));
    }

    #[test]
    fn test_content_block_tool_use_serialization() {
        let block = ContentBlock::ToolUse {
            id: "call_1".to_string(),
            name: "search".to_string(),
            input: json!({"query": "rust"}),
        };
        let json = serde_json::to_value(&block).unwrap();
        assert_eq!(
            json,
            json!({
                "type": "tool_use",
                "id": "call_1",
                "name": "search",
                "input": {"query": "rust"}
            })
        );
    }

    #[test]
    fn test_content_block_tool_result_serialization() {
        let block = ContentBlock::ToolResult {
            tool_use_id: "call_1".to_string(),
            content: "Result".to_string(),
            is_error: None,
        };
        let json = serde_json::to_value(&block).unwrap();
        assert_eq!(
            json,
            json!({
                "type": "tool_result",
                "tool_use_id": "call_1",
                "content": "Result"
            })
        );
    }

    #[test]
    fn test_content_block_tool_result_with_error() {
        let block = ContentBlock::ToolResult {
            tool_use_id: "call_1".to_string(),
            content: "Error: not found".to_string(),
            is_error: Some(true),
        };
        let json = serde_json::to_value(&block).unwrap();
        assert_eq!(json["is_error"], json!(true));
    }

    // ------------------------------------------------------------------------
    // ToolDefinition Tests
    // ------------------------------------------------------------------------

    #[test]
    fn test_tool_definition_new() {
        let schema = json!({
            "type": "object",
            "properties": {
                "query": {"type": "string"}
            },
            "required": ["query"]
        });
        let tool = ToolDefinition::new("search", "Search the web", schema.clone());
        assert_eq!(tool.name, "search");
        assert_eq!(tool.description, "Search the web");
        assert_eq!(tool.input_schema, schema);
    }

    #[test]
    fn test_tool_definition_serialization() {
        let tool = ToolDefinition::new("calculator", "Perform math", json!({"type": "object"}));
        let json = serde_json::to_value(&tool).unwrap();
        assert_eq!(json["name"], "calculator");
        assert_eq!(json["description"], "Perform math");
    }

    // ------------------------------------------------------------------------
    // ToolCall Tests
    // ------------------------------------------------------------------------

    #[test]
    fn test_tool_call_serialization() {
        let call = ToolCall {
            id: "call_abc".to_string(),
            name: "get_weather".to_string(),
            arguments: json!({"city": "Paris"}),
        };
        let json = serde_json::to_value(&call).unwrap();
        assert_eq!(json["id"], "call_abc");
        assert_eq!(json["name"], "get_weather");
        assert_eq!(json["arguments"]["city"], "Paris");
    }

    // ------------------------------------------------------------------------
    // ChatResponse Tests
    // ------------------------------------------------------------------------

    #[test]
    fn test_chat_response_has_tool_calls() {
        let response = ChatResponse {
            content: MessageContent::Text("Let me search".to_string()),
            tool_calls: vec![ToolCall {
                id: "1".to_string(),
                name: "search".to_string(),
                arguments: json!({}),
            }],
            stop_reason: StopReason::ToolUse,
            usage: Usage::new(100, 50),
        };
        assert!(response.has_tool_calls());
    }

    #[test]
    fn test_chat_response_no_tool_calls() {
        let response = ChatResponse {
            content: MessageContent::Text("Hello".to_string()),
            tool_calls: vec![],
            stop_reason: StopReason::EndTurn,
            usage: Usage::new(100, 50),
        };
        assert!(!response.has_tool_calls());
    }

    #[test]
    fn test_chat_response_text() {
        let response = ChatResponse {
            content: MessageContent::Text("Result".to_string()),
            tool_calls: vec![],
            stop_reason: StopReason::EndTurn,
            usage: Usage::new(10, 20),
        };
        assert_eq!(response.text(), Some("Result".to_string()));
    }

    // ------------------------------------------------------------------------
    // StopReason Tests
    // ------------------------------------------------------------------------

    #[test]
    fn test_stop_reason_default() {
        let reason: StopReason = Default::default();
        assert_eq!(reason, StopReason::EndTurn);
    }

    #[test]
    fn test_stop_reason_serialization() {
        assert_eq!(
            serde_json::to_string(&StopReason::EndTurn).unwrap(),
            "\"end_turn\""
        );
        assert_eq!(
            serde_json::to_string(&StopReason::ToolUse).unwrap(),
            "\"tool_use\""
        );
        assert_eq!(
            serde_json::to_string(&StopReason::MaxTokens).unwrap(),
            "\"max_tokens\""
        );
    }

    // ------------------------------------------------------------------------
    // Usage Tests
    // ------------------------------------------------------------------------

    #[test]
    fn test_usage_new() {
        let usage = Usage::new(100, 50);
        assert_eq!(usage.input_tokens, 100);
        assert_eq!(usage.output_tokens, 50);
        assert_eq!(usage.cache_creation_input_tokens, 0);
        assert_eq!(usage.cache_read_input_tokens, 0);
    }

    #[test]
    fn test_usage_total_tokens() {
        let usage = Usage::new(100, 50);
        assert_eq!(usage.total_tokens(), 150);
    }

    #[test]
    fn test_usage_add() {
        let u1 = Usage::new(100, 50);
        let u2 = Usage::new(200, 100);
        let total = u1 + u2;
        assert_eq!(total.input_tokens, 300);
        assert_eq!(total.output_tokens, 150);
    }

    #[test]
    fn test_usage_add_assign() {
        let mut usage = Usage::new(100, 50);
        usage += Usage::new(50, 25);
        assert_eq!(usage.input_tokens, 150);
        assert_eq!(usage.output_tokens, 75);
    }

    #[test]
    fn test_usage_estimate_cost_sonnet() {
        // 1M input + 1M output = $3 + $15 = $18
        let usage = Usage::new(1_000_000, 1_000_000);
        let cost = usage.estimate_cost_usd("claude-sonnet-4");
        assert!((cost - 18.0).abs() < 0.001);
    }

    #[test]
    fn test_usage_estimate_cost_opus() {
        // 1M input + 1M output = $15 + $75 = $90
        let usage = Usage::new(1_000_000, 1_000_000);
        let cost = usage.estimate_cost_usd("claude-opus-4");
        assert!((cost - 90.0).abs() < 0.001);
    }

    #[test]
    fn test_usage_estimate_cost_haiku() {
        // 1M input + 1M output = $0.80 + $4 = $4.80
        let usage = Usage::new(1_000_000, 1_000_000);
        let cost = usage.estimate_cost_usd("claude-3-5-haiku");
        assert!((cost - 4.80).abs() < 0.001);
    }

    #[test]
    fn test_usage_estimate_cost_gpt4o() {
        // 1M input + 1M output = $2.50 + $10 = $12.50
        let usage = Usage::new(1_000_000, 1_000_000);
        let cost = usage.estimate_cost_usd("gpt-4o");
        assert!((cost - 12.50).abs() < 0.001);
    }

    #[test]
    fn test_usage_estimate_cost_gpt4o_mini() {
        // 1M input + 1M output = $0.15 + $0.60 = $0.75
        let usage = Usage::new(1_000_000, 1_000_000);
        let cost = usage.estimate_cost_usd("gpt-4o-mini");
        assert!((cost - 0.75).abs() < 0.001);
    }

    #[test]
    fn test_usage_estimate_cost_with_cache() {
        let usage = Usage {
            input_tokens: 1_000_000,
            output_tokens: 1_000_000,
            cache_creation_input_tokens: 100_000, // 10% write
            cache_read_input_tokens: 500_000,     // 50% read
        };
        // Base: $3 + $15 = $18
        // Cache read: 0.5M * $3 * 0.25 = $0.375
        // Cache write: 0.1M * $3 * 1.25 = $0.375
        // Total: $18.75
        let cost = usage.estimate_cost_usd("claude-sonnet-4");
        assert!((cost - 18.75).abs() < 0.001);
    }

    #[test]
    fn test_usage_default() {
        let usage: Usage = Default::default();
        assert_eq!(usage.input_tokens, 0);
        assert_eq!(usage.output_tokens, 0);
        assert_eq!(usage.total_tokens(), 0);
    }

    // ------------------------------------------------------------------------
    // Full Message Serialization Round-trip
    // ------------------------------------------------------------------------

    #[test]
    fn test_message_serialization_roundtrip() {
        let msg = Message::user("Test message");
        let json = serde_json::to_string(&msg).unwrap();
        let parsed: Message = serde_json::from_str(&json).unwrap();
        assert_eq!(msg, parsed);
    }

    #[test]
    fn test_message_tool_result_skips_none_tool_call_id() {
        let msg = Message::user("Hello");
        let json = serde_json::to_value(&msg).unwrap();
        assert!(json.get("tool_call_id").is_none());
    }
}
