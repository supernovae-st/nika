//! Provider abstraction layer
//!
//! ## Provider Strategy (v0.4)
//!
//! Nika uses [rig-core](https://github.com/0xPlaygrounds/rig) for LLM providers.
//!
//! | Component | Implementation |
//! |-----------|----------------|
//! | `agent:` verb | [`RigAgentLoop`](crate::runtime::RigAgentLoop) + rig-core |
//! | `infer:` verb | [`RigProvider`](rig::RigProvider) + rig-core |
//! | Tool calling | [`NikaMcpTool`](rig::NikaMcpTool) (rig `ToolDyn`) |
//!
//! ## Modules
//!
//! - [`rig`] - rig-core integration (`RigProvider`, `NikaMcpTool`)
//!
//! ## Example
//!
//! ```rust,ignore
//! use nika::runtime::RigAgentLoop;
//! use nika::ast::AgentParams;
//! use nika::event::EventLog;
//!
//! let params = AgentParams {
//!     prompt: "Generate a landing page".to_string(),
//!     mcp: vec!["novanet".to_string()],
//!     max_turns: Some(5),
//!     ..Default::default()
//! };
//! let mut agent = RigAgentLoop::new("task-1".into(), params, EventLog::new(), mcp_clients)?;
//! let result = agent.run_claude().await?;
//! ```

pub mod rig;

// ═══════════════════════════════════════════════════════════════════════════
// DEPRECATED: Legacy Provider types (v0.4)
//
// These types are kept for compatibility with resilience/provider.rs.
// They will be removed in a future version.
// ═══════════════════════════════════════════════════════════════════════════

use anyhow::Result;
use async_trait::async_trait;
use serde::{Deserialize, Serialize};

/// Default models per provider
#[deprecated(since = "0.4.0", note = "Use rig-core model constants instead")]
pub const CLAUDE_DEFAULT_MODEL: &str = "claude-sonnet-4-5";
#[deprecated(since = "0.4.0", note = "Use rig-core model constants instead")]
pub const OPENAI_DEFAULT_MODEL: &str = "gpt-4o";

/// Message role in conversation
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[deprecated(since = "0.4.0", note = "Use rig-core types instead")]
pub enum MessageRole {
    User,
    Assistant,
    System,
}

/// Content of a message
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[deprecated(since = "0.4.0", note = "Use rig-core types instead")]
pub enum MessageContent {
    Text(String),
    ToolResult { tool_use_id: String, content: String },
}

/// A message in a conversation
#[derive(Debug, Clone, Serialize, Deserialize)]
#[deprecated(since = "0.4.0", note = "Use rig-core types instead")]
#[allow(deprecated)]
pub struct Message {
    pub role: MessageRole,
    pub content: MessageContent,
}

/// Tool definition for function calling
#[derive(Debug, Clone, Serialize, Deserialize)]
#[deprecated(since = "0.4.0", note = "Use rig-core types instead")]
pub struct ToolDefinition {
    pub name: String,
    pub description: String,
    pub input_schema: serde_json::Value,
}

/// Tool call from LLM
#[derive(Debug, Clone, Serialize, Deserialize)]
#[deprecated(since = "0.4.0", note = "Use rig-core types instead")]
pub struct ToolCall {
    pub id: String,
    pub name: String,
    pub input: serde_json::Value,
}

/// Stop reason for completion
#[derive(Debug, Clone, PartialEq, Eq)]
#[deprecated(since = "0.4.0", note = "Use rig-core types instead")]
pub enum StopReason {
    EndTurn,
    ToolUse,
    MaxTokens,
    StopSequence,
}

/// Token usage tracking
#[derive(Debug, Clone, Default)]
#[deprecated(since = "0.4.0", note = "Use rig-core types instead")]
pub struct Usage {
    pub input_tokens: u32,
    pub output_tokens: u32,
}

#[allow(deprecated)]
impl Usage {
    pub fn new(input: u32, output: u32) -> Self {
        Self {
            input_tokens: input,
            output_tokens: output,
        }
    }
}

/// Chat response from provider
#[derive(Debug, Clone)]
#[deprecated(since = "0.4.0", note = "Use rig-core types instead")]
#[allow(deprecated)]
pub struct ChatResponse {
    pub content: MessageContent,
    pub tool_calls: Vec<ToolCall>,
    pub stop_reason: StopReason,
    pub usage: Usage,
}

/// LLM provider abstraction for inference operations
///
/// DEPRECATED: Use `RigAgentLoop` for agent execution and `RigProvider` for inference.
#[deprecated(since = "0.4.0", note = "Use RigAgentLoop or RigProvider instead")]
#[async_trait]
#[allow(deprecated)]
pub trait Provider: Send + Sync {
    /// Execute a prompt and return the response (simple, no tools)
    async fn infer(&self, prompt: &str, model: &str) -> Result<String>;

    /// Chat with tool support for multi-turn conversations
    async fn chat(
        &self,
        messages: &[Message],
        tools: Option<&[ToolDefinition]>,
        model: &str,
    ) -> Result<ChatResponse>;

    /// Default model for this provider
    fn default_model(&self) -> &str;

    /// Get the provider name (e.g., "claude", "openai", "mock")
    fn name(&self) -> &str;

    /// Get the current model identifier
    fn model(&self) -> &str {
        self.default_model()
    }
}

/// Mock provider for testing
#[derive(Default)]
#[deprecated(since = "0.4.0", note = "Use RigAgentLoop.run_mock() instead")]
pub struct MockProvider;

#[allow(deprecated)]
#[async_trait]
impl Provider for MockProvider {
    async fn infer(&self, _prompt: &str, _model: &str) -> Result<String> {
        Ok("Mock response".to_string())
    }

    async fn chat(
        &self,
        _messages: &[Message],
        _tools: Option<&[ToolDefinition]>,
        _model: &str,
    ) -> Result<ChatResponse> {
        Ok(ChatResponse {
            content: MessageContent::Text("Mock response".to_string()),
            tool_calls: vec![],
            stop_reason: StopReason::EndTurn,
            usage: Usage::new(10, 10),
        })
    }

    fn default_model(&self) -> &str {
        "mock-v1"
    }

    fn name(&self) -> &str {
        "mock"
    }
}
