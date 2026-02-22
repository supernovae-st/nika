# MVP 2: Agent Verb + Observability

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Enable agentic execution with MCP tool access and comprehensive observability through enhanced event logging and NDJSON trace files.

**Architecture:** Agent loop executes LLM turns with tool calling, emitting detailed events. Events are streamed to NDJSON files for debugging and TUI consumption.

**Tech Stack:** Rust, tokio, serde, tracing, uuid, xxhash

**Estimated Time:** 6-8 hours

**Prerequisites:** MVP 1 (Invoke Verb) completed

---

## Task 1: Add Provider Types for Tool Calling

**Files:**
- Create: `src/provider/types.rs`
- Modify: `src/provider/mod.rs`

### Step 1: Create types module

Create `src/provider/types.rs`:

```rust
//! Provider Types for LLM Communication
//!
//! Types for messages, tools, and responses in LLM conversations.

use serde::{Deserialize, Serialize};

/// A message in a conversation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Message {
    pub role: MessageRole,
    pub content: MessageContent,
    /// For tool result messages
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_call_id: Option<String>,
}

/// Message role
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum MessageRole {
    User,
    Assistant,
    Tool,
    System,
}

/// Message content (text or structured)
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum MessageContent {
    Text(String),
    Blocks(Vec<ContentBlock>),
}

impl MessageContent {
    pub fn as_text(&self) -> String {
        match self {
            Self::Text(s) => s.clone(),
            Self::Blocks(blocks) => blocks
                .iter()
                .filter_map(|b| match b {
                    ContentBlock::Text { text } => Some(text.clone()),
                    _ => None,
                })
                .collect::<Vec<_>>()
                .join("\n"),
        }
    }
}

/// Content block in a message
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ContentBlock {
    Text { text: String },
    ToolUse { id: String, name: String, input: serde_json::Value },
    ToolResult { tool_use_id: String, content: String },
}

impl Message {
    /// Create a user message
    pub fn user(content: impl Into<String>) -> Self {
        Self {
            role: MessageRole::User,
            content: MessageContent::Text(content.into()),
            tool_call_id: None,
        }
    }

    /// Create an assistant message
    pub fn assistant(content: impl Into<String>) -> Self {
        Self {
            role: MessageRole::Assistant,
            content: MessageContent::Text(content.into()),
            tool_call_id: None,
        }
    }

    /// Create a tool result message
    pub fn tool_result(tool_call_id: impl Into<String>, content: impl Into<String>) -> Self {
        Self {
            role: MessageRole::Tool,
            content: MessageContent::Text(content.into()),
            tool_call_id: Some(tool_call_id.into()),
        }
    }

    /// Create a system message
    pub fn system(content: impl Into<String>) -> Self {
        Self {
            role: MessageRole::System,
            content: MessageContent::Text(content.into()),
            tool_call_id: None,
        }
    }
}

/// Tool definition for LLM
#[derive(Debug, Clone, Serialize)]
pub struct ToolDefinition {
    pub name: String,
    pub description: String,
    pub input_schema: serde_json::Value,
}

/// A tool call from the LLM
#[derive(Debug, Clone, Deserialize)]
pub struct ToolCall {
    pub id: String,
    pub name: String,
    #[serde(alias = "input")]
    pub arguments: serde_json::Value,
}

/// Response from chat completion
#[derive(Debug, Clone)]
pub struct ChatResponse {
    /// Text content from the response
    pub content: String,
    /// Tool calls requested by the model
    pub tool_calls: Option<Vec<ToolCall>>,
    /// Stop reason
    pub stop_reason: StopReason,
    /// Usage statistics
    pub usage: Option<Usage>,
}

/// Why the model stopped generating
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StopReason {
    EndTurn,
    ToolUse,
    MaxTokens,
    StopSequence,
    Unknown,
}

/// Token usage statistics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Usage {
    pub input_tokens: u32,
    pub output_tokens: u32,
    #[serde(default)]
    pub cache_creation_input_tokens: u32,
    #[serde(default)]
    pub cache_read_input_tokens: u32,
}

impl Usage {
    /// Calculate total tokens
    pub fn total_tokens(&self) -> u32 {
        self.input_tokens + self.output_tokens
    }

    /// Estimate cost in USD (Claude pricing)
    pub fn estimate_cost_usd(&self, model: &str) -> f64 {
        let (input_price, output_price) = match model {
            m if m.contains("opus") => (0.015, 0.075),
            m if m.contains("sonnet") => (0.003, 0.015),
            m if m.contains("haiku") => (0.00025, 0.00125),
            _ => (0.003, 0.015), // Default to Sonnet pricing
        };

        let input_cost = (self.input_tokens as f64 / 1000.0) * input_price;
        let output_cost = (self.output_tokens as f64 / 1000.0) * output_price;
        let cache_read_discount = (self.cache_read_input_tokens as f64 / 1000.0) * input_price * 0.9;

        input_cost + output_cost - cache_read_discount
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_message_constructors() {
        let user = Message::user("Hello");
        assert_eq!(user.role, MessageRole::User);
        assert_eq!(user.content.as_text(), "Hello");

        let tool = Message::tool_result("call_123", "Result");
        assert_eq!(tool.role, MessageRole::Tool);
        assert_eq!(tool.tool_call_id, Some("call_123".to_string()));
    }

    #[test]
    fn test_usage_cost_estimation() {
        let usage = Usage {
            input_tokens: 1000,
            output_tokens: 500,
            cache_creation_input_tokens: 0,
            cache_read_input_tokens: 200,
        };

        let cost = usage.estimate_cost_usd("claude-sonnet-4");
        assert!(cost > 0.0);
        assert!(cost < 0.02); // Reasonable bound
    }
}
```

### Step 2: Export from mod.rs

Modify `src/provider/mod.rs`:

```rust
mod types;
pub use types::*;
```

### Step 3: Run tests

Run: `cd tools/nika && cargo test provider::types`
Expected: All tests pass

### Step 4: Commit

```bash
git add src/provider/types.rs
git commit -m "feat(provider): add types for tool calling

- Message with role and content
- ToolDefinition and ToolCall
- ChatResponse with usage stats
- Cost estimation for Claude models

Co-Authored-By: Claude <noreply@anthropic.com>"
```

---

## Task 2: Update LlmProvider Trait

**Files:**
- Modify: `src/provider/mod.rs`

### Step 1: Update trait definition

```rust
use async_trait::async_trait;
use crate::error::Result;
use crate::provider::types::*;

/// LLM Provider trait
#[async_trait]
pub trait LlmProvider: Send + Sync {
    /// Simple inference (no tools)
    async fn infer(&self, prompt: &str) -> Result<String>;

    /// Chat with tool support
    async fn chat(
        &self,
        messages: &[Message],
        tools: Option<&[ToolDefinition]>,
    ) -> Result<ChatResponse>;

    /// Get provider name
    fn name(&self) -> &str;

    /// Get current model
    fn model(&self) -> &str;
}
```

### Step 2: Commit

```bash
git add src/provider/mod.rs
git commit -m "feat(provider): add chat method to LlmProvider trait

- chat() for multi-turn conversations with tools
- name() and model() for identification

Co-Authored-By: Claude <noreply@anthropic.com>"
```

---

## Task 3: Implement Chat for Claude Provider

**Files:**
- Modify: `src/provider/claude.rs`
- Create: `tests/claude_chat_test.rs`

### Step 1: Write failing test

Create `tests/claude_chat_test.rs`:

```rust
//! Claude provider chat tests

use nika::provider::{Message, ToolDefinition, ChatResponse};

#[test]
fn test_tool_definition_serialization() {
    let tool = ToolDefinition {
        name: "novanet_generate".to_string(),
        description: "Generate content context".to_string(),
        input_schema: serde_json::json!({
            "type": "object",
            "properties": {
                "mode": {"type": "string"},
                "entity": {"type": "string"}
            },
            "required": ["mode"]
        }),
    };

    let json = serde_json::to_value(&tool).unwrap();
    assert_eq!(json["name"], "novanet_generate");
    assert!(json["input_schema"]["properties"].is_object());
}

#[test]
fn test_message_serialization_for_claude_api() {
    let messages = vec![
        Message::user("Hello"),
        Message::assistant("Hi there!"),
    ];

    let json = serde_json::to_value(&messages).unwrap();
    assert_eq!(json[0]["role"], "user");
    assert_eq!(json[1]["role"], "assistant");
}
```

### Step 2: Implement chat method in Claude provider

Update `src/provider/claude.rs`:

```rust
use crate::provider::types::*;

#[async_trait]
impl LlmProvider for ClaudeProvider {
    async fn infer(&self, prompt: &str) -> Result<String> {
        let messages = vec![Message::user(prompt)];
        let response = self.chat(&messages, None).await?;
        Ok(response.content)
    }

    async fn chat(
        &self,
        messages: &[Message],
        tools: Option<&[ToolDefinition]>,
    ) -> Result<ChatResponse> {
        let mut request = serde_json::json!({
            "model": self.model,
            "max_tokens": 4096,
            "messages": self.format_messages(messages)
        });

        if let Some(tools) = tools {
            request["tools"] = serde_json::to_value(
                tools.iter().map(|t| serde_json::json!({
                    "name": &t.name,
                    "description": &t.description,
                    "input_schema": &t.input_schema
                })).collect::<Vec<_>>()
            )?;
        }

        let response = self.client
            .post("https://api.anthropic.com/v1/messages")
            .header("x-api-key", &self.api_key)
            .header("anthropic-version", "2023-06-01")
            .header("content-type", "application/json")
            .json(&request)
            .send()
            .await
            .map_err(|e| NikaError::ProviderApiError {
                message: e.to_string()
            })?;

        let body: serde_json::Value = response.json().await
            .map_err(|e| NikaError::ProviderApiError {
                message: e.to_string()
            })?;

        self.parse_response(body)
    }

    fn name(&self) -> &str {
        "claude"
    }

    fn model(&self) -> &str {
        &self.model
    }
}

impl ClaudeProvider {
    fn format_messages(&self, messages: &[Message]) -> Vec<serde_json::Value> {
        messages.iter().map(|m| {
            let mut msg = serde_json::json!({
                "role": match m.role {
                    MessageRole::User => "user",
                    MessageRole::Assistant => "assistant",
                    MessageRole::Tool => "user", // Tool results go as user
                    MessageRole::System => "user",
                },
                "content": m.content.as_text()
            });

            // Handle tool results specially
            if m.role == MessageRole::Tool {
                if let Some(tool_id) = &m.tool_call_id {
                    msg["content"] = serde_json::json!([{
                        "type": "tool_result",
                        "tool_use_id": tool_id,
                        "content": m.content.as_text()
                    }]);
                }
            }

            msg
        }).collect()
    }

    fn parse_response(&self, body: serde_json::Value) -> Result<ChatResponse> {
        // Extract text content
        let content = body["content"]
            .as_array()
            .and_then(|arr| arr.iter()
                .find(|b| b["type"] == "text")
                .and_then(|b| b["text"].as_str()))
            .unwrap_or("")
            .to_string();

        // Extract tool calls
        let tool_calls = body["content"]
            .as_array()
            .map(|arr| arr.iter()
                .filter(|b| b["type"] == "tool_use")
                .map(|b| ToolCall {
                    id: b["id"].as_str().unwrap_or("").to_string(),
                    name: b["name"].as_str().unwrap_or("").to_string(),
                    arguments: b["input"].clone(),
                })
                .collect::<Vec<_>>())
            .filter(|v| !v.is_empty());

        // Parse stop reason
        let stop_reason = match body["stop_reason"].as_str() {
            Some("end_turn") => StopReason::EndTurn,
            Some("tool_use") => StopReason::ToolUse,
            Some("max_tokens") => StopReason::MaxTokens,
            Some("stop_sequence") => StopReason::StopSequence,
            _ => StopReason::Unknown,
        };

        // Parse usage
        let usage = body["usage"].as_object().map(|u| Usage {
            input_tokens: u["input_tokens"].as_u64().unwrap_or(0) as u32,
            output_tokens: u["output_tokens"].as_u64().unwrap_or(0) as u32,
            cache_creation_input_tokens: u["cache_creation_input_tokens"].as_u64().unwrap_or(0) as u32,
            cache_read_input_tokens: u["cache_read_input_tokens"].as_u64().unwrap_or(0) as u32,
        });

        Ok(ChatResponse {
            content,
            tool_calls,
            stop_reason,
            usage,
        })
    }
}
```

### Step 3: Run tests

Run: `cd tools/nika && cargo test claude`
Expected: All tests pass

### Step 4: Commit

```bash
git add src/provider/claude.rs tests/claude_chat_test.rs
git commit -m "feat(provider): implement chat with tools for Claude

- Multi-turn conversation support
- Tool definition formatting
- Response parsing with tool calls
- Usage statistics extraction

Co-Authored-By: Claude <noreply@anthropic.com>"
```

---

## Task 4: Create Agent Params

**Files:**
- Create: `src/ast/agent.rs`
- Modify: `src/ast/mod.rs`
- Create: `tests/agent_parse_test.rs`

### Step 1: Write failing test

Create `tests/agent_parse_test.rs`:

```rust
//! Agent verb parsing tests

use nika::ast::AgentParams;

#[test]
fn test_agent_params_full() {
    let yaml = r#"
prompt: |
  Generate native content for the homepage hero block.
  Use @entity:qr-code-generator for the main concept.
provider: claude
model: claude-sonnet-4
mcp:
  - novanet
max_turns: 10
stop_conditions:
  - "GENERATION_COMPLETE"
  - "VALIDATION_PASSED"
"#;

    let params: AgentParams = serde_yaml::from_str(yaml).unwrap();

    assert!(params.prompt.contains("homepage hero"));
    assert_eq!(params.provider, Some("claude".to_string()));
    assert_eq!(params.mcp, vec!["novanet"]);
    assert_eq!(params.max_turns, Some(10));
    assert_eq!(params.stop_conditions.len(), 2);
}

#[test]
fn test_agent_params_minimal() {
    let yaml = r#"
prompt: "Simple task"
"#;

    let params: AgentParams = serde_yaml::from_str(yaml).unwrap();

    assert_eq!(params.prompt, "Simple task");
    assert!(params.provider.is_none());
    assert!(params.mcp.is_empty());
    assert!(params.max_turns.is_none()); // Will use default
}

#[test]
fn test_agent_params_defaults() {
    let params = AgentParams::default();

    assert!(params.prompt.is_empty());
    assert_eq!(params.effective_max_turns(), 10); // Default
    assert!(params.mcp.is_empty());
}
```

### Step 2: Create AgentParams

Create `src/ast/agent.rs`:

```rust
//! Agent Action Parameters
//!
//! The `agent:` verb enables agentic execution with MCP tool access.
//!
//! # Example
//!
//! ```yaml
//! - id: generate
//!   agent:
//!     prompt: |
//!       Generate content for the hero block.
//!       Use novanet_generate for context.
//!     mcp:
//!       - novanet
//!     max_turns: 10
//!     stop_conditions:
//!       - "GENERATION_COMPLETE"
//! ```

use serde::Deserialize;

/// Default maximum turns for agent loop
const DEFAULT_MAX_TURNS: u32 = 10;

/// Parameters for the `agent:` verb
#[derive(Debug, Clone, Deserialize)]
pub struct AgentParams {
    /// System/user prompt for the agent
    pub prompt: String,

    /// LLM provider override (defaults to workflow provider)
    #[serde(default)]
    pub provider: Option<String>,

    /// Model override
    #[serde(default)]
    pub model: Option<String>,

    /// MCP servers the agent can access
    #[serde(default)]
    pub mcp: Vec<String>,

    /// Maximum agentic turns before stopping
    #[serde(default)]
    pub max_turns: Option<u32>,

    /// Conditions that trigger early stop (if output contains any)
    #[serde(default)]
    pub stop_conditions: Vec<String>,

    /// Scope preset (full, minimal, debug)
    #[serde(default)]
    pub scope: Option<String>,
}

impl Default for AgentParams {
    fn default() -> Self {
        Self {
            prompt: String::new(),
            provider: None,
            model: None,
            mcp: vec![],
            max_turns: None,
            stop_conditions: vec![],
            scope: None,
        }
    }
}

impl AgentParams {
    /// Get effective max turns (with default)
    pub fn effective_max_turns(&self) -> u32 {
        self.max_turns.unwrap_or(DEFAULT_MAX_TURNS)
    }

    /// Check if a response triggers a stop condition
    pub fn should_stop(&self, content: &str) -> bool {
        self.stop_conditions.iter().any(|cond| content.contains(cond))
    }

    /// Validate agent parameters
    pub fn validate(&self) -> Result<(), String> {
        if self.prompt.is_empty() {
            return Err("Agent prompt cannot be empty".to_string());
        }
        if let Some(max) = self.max_turns {
            if max == 0 {
                return Err("max_turns must be > 0".to_string());
            }
            if max > 100 {
                return Err("max_turns cannot exceed 100".to_string());
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_should_stop() {
        let params = AgentParams {
            prompt: "test".to_string(),
            stop_conditions: vec!["DONE".to_string(), "COMPLETE".to_string()],
            ..Default::default()
        };

        assert!(params.should_stop("Task is DONE"));
        assert!(params.should_stop("COMPLETE"));
        assert!(!params.should_stop("Still working..."));
    }

    #[test]
    fn test_validate() {
        let mut params = AgentParams::default();

        // Empty prompt
        assert!(params.validate().is_err());

        params.prompt = "test".to_string();
        assert!(params.validate().is_ok());

        // Zero max_turns
        params.max_turns = Some(0);
        assert!(params.validate().is_err());

        // Excessive max_turns
        params.max_turns = Some(101);
        assert!(params.validate().is_err());
    }
}
```

### Step 3: Export and add to TaskAction

Modify `src/ast/mod.rs`:

```rust
mod agent;
pub use agent::AgentParams;
```

Modify `src/ast/action.rs` to add Agent variant:

```rust
use crate::ast::{InvokeParams, AgentParams};

/// The 5 task action types (v0.2)
#[derive(Debug, Clone, Deserialize)]
#[serde(untagged)]
pub enum TaskAction {
    Infer { infer: InferParams },
    Exec { exec: ExecParams },
    Fetch { fetch: FetchParams },
    Invoke { invoke: InvokeParams },
    Agent { agent: AgentParams },
}
```

### Step 4: Run tests

Run: `cd tools/nika && cargo test agent`
Expected: All tests pass

### Step 5: Commit

```bash
git add src/ast/agent.rs tests/agent_parse_test.rs
git commit -m "feat(ast): add AgentParams for agent: verb

- Prompt with provider/model override
- MCP server list for tool access
- max_turns and stop_conditions
- Validation and defaults

Co-Authored-By: Claude <noreply@anthropic.com>"
```

---

## Task 5: Create Agent Loop

**Files:**
- Create: `src/runtime/agent_loop.rs`
- Modify: `src/runtime/mod.rs`
- Create: `tests/agent_loop_test.rs`

### Step 1: Write failing test

Create `tests/agent_loop_test.rs`:

```rust
//! Agent loop tests

use nika::ast::AgentParams;
use nika::runtime::AgentLoop;
use nika::mcp::McpClient;
use nika::event::EventLog;
use std::sync::Arc;
use std::collections::HashMap;

#[tokio::test]
async fn test_agent_loop_creation() {
    let params = AgentParams {
        prompt: "Test prompt".to_string(),
        mcp: vec!["novanet".to_string()],
        max_turns: Some(3),
        ..Default::default()
    };

    let event_log = EventLog::new();
    let mcp_clients = HashMap::new();

    let loop_instance = AgentLoop::new(
        "test_task".to_string(),
        params,
        event_log,
        mcp_clients,
    );

    assert!(loop_instance.is_ok());
}

#[tokio::test]
async fn test_agent_loop_stop_on_condition() {
    let params = AgentParams {
        prompt: "Test".to_string(),
        stop_conditions: vec!["DONE".to_string()],
        max_turns: Some(5),
        ..Default::default()
    };

    // With mock that returns "DONE" immediately
    // The loop should stop after 1 turn
}
```

### Step 2: Create agent loop module

Create `src/runtime/agent_loop.rs`:

```rust
//! Agent Loop - Agentic Execution Engine
//!
//! Executes multi-turn conversations with tool calling.

use crate::ast::AgentParams;
use crate::error::{NikaError, Result};
use crate::event::{EventKind, EventLog};
use crate::mcp::McpClient;
use crate::provider::types::*;
use crate::provider::LlmProvider;
use std::collections::HashMap;
use std::sync::Arc;

/// Agent loop for agentic execution
pub struct AgentLoop {
    task_id: String,
    params: AgentParams,
    event_log: EventLog,
    mcp_clients: HashMap<String, Arc<McpClient>>,
}

/// Result of an agent loop execution
#[derive(Debug)]
pub struct AgentLoopResult {
    pub status: AgentStatus,
    pub turns: u32,
    pub final_output: serde_json::Value,
    pub total_tokens: u32,
}

/// Agent completion status
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AgentStatus {
    /// Completed naturally (no more tool calls)
    NaturalCompletion,
    /// Stopped due to stop condition match
    StopConditionMet,
    /// Reached max_turns limit
    MaxTurnsReached,
    /// Error during execution
    Failed,
}

impl AgentLoop {
    /// Create a new agent loop
    pub fn new(
        task_id: String,
        params: AgentParams,
        event_log: EventLog,
        mcp_clients: HashMap<String, Arc<McpClient>>,
    ) -> Result<Self> {
        params.validate().map_err(|e| NikaError::ValidationError { reason: e })?;

        Ok(Self {
            task_id,
            params,
            event_log,
            mcp_clients,
        })
    }

    /// Run the agent loop
    pub async fn run(&self, provider: Arc<dyn LlmProvider>) -> Result<AgentLoopResult> {
        let max_turns = self.params.effective_max_turns();
        let mut conversation: Vec<Message> = vec![Message::user(&self.params.prompt)];
        let mut turn = 0u32;
        let mut total_tokens = 0u32;

        // Build tool definitions from MCP clients
        let tools = self.build_tool_definitions().await?;

        loop {
            // Emit turn started event
            self.event_log.emit(EventKind::AgentTurnStarted {
                task_id: self.task_id.clone().into(),
                turn_index: turn,
                message_count: conversation.len(),
            });

            // Check max turns
            if turn >= max_turns {
                self.event_log.emit(EventKind::AgentTurnCompleted {
                    task_id: self.task_id.clone().into(),
                    turn_index: turn,
                    status: "max_turns_reached".to_string(),
                });

                return Ok(AgentLoopResult {
                    status: AgentStatus::MaxTurnsReached,
                    turns: turn,
                    final_output: serde_json::Value::String(
                        conversation.last()
                            .map(|m| m.content.as_text())
                            .unwrap_or_default()
                    ),
                    total_tokens,
                });
            }

            // Call LLM
            let tools_ref = if tools.is_empty() { None } else { Some(tools.as_slice()) };
            let response = provider.chat(&conversation, tools_ref).await?;

            // Track tokens
            if let Some(usage) = &response.usage {
                total_tokens += usage.total_tokens();
            }

            // Add assistant response to conversation
            conversation.push(Message::assistant(&response.content));

            // Check stop conditions
            if self.params.should_stop(&response.content) {
                self.event_log.emit(EventKind::AgentTurnCompleted {
                    task_id: self.task_id.clone().into(),
                    turn_index: turn,
                    status: "stop_condition_met".to_string(),
                });

                return Ok(AgentLoopResult {
                    status: AgentStatus::StopConditionMet,
                    turns: turn + 1,
                    final_output: self.parse_output(&response.content),
                    total_tokens,
                });
            }

            // Process tool calls
            if let Some(tool_calls) = response.tool_calls {
                if tool_calls.is_empty() {
                    // No tool calls = natural completion
                    self.event_log.emit(EventKind::AgentTurnCompleted {
                        task_id: self.task_id.clone().into(),
                        turn_index: turn,
                        status: "natural_completion".to_string(),
                    });

                    return Ok(AgentLoopResult {
                        status: AgentStatus::NaturalCompletion,
                        turns: turn + 1,
                        final_output: self.parse_output(&response.content),
                        total_tokens,
                    });
                }

                // Execute each tool call
                for tool_call in tool_calls {
                    let result = self.execute_tool_call(&tool_call).await?;
                    conversation.push(Message::tool_result(&tool_call.id, &result));
                }
            } else {
                // No tool calls = natural completion
                self.event_log.emit(EventKind::AgentTurnCompleted {
                    task_id: self.task_id.clone().into(),
                    turn_index: turn,
                    status: "natural_completion".to_string(),
                });

                return Ok(AgentLoopResult {
                    status: AgentStatus::NaturalCompletion,
                    turns: turn + 1,
                    final_output: self.parse_output(&response.content),
                    total_tokens,
                });
            }

            self.event_log.emit(EventKind::AgentTurnCompleted {
                task_id: self.task_id.clone().into(),
                turn_index: turn,
                status: "continue".to_string(),
            });

            turn += 1;
        }
    }

    /// Build tool definitions from MCP clients
    async fn build_tool_definitions(&self) -> Result<Vec<ToolDefinition>> {
        let mut tools = Vec::new();

        for mcp_name in &self.params.mcp {
            let client = self.mcp_clients.get(mcp_name)
                .ok_or_else(|| NikaError::McpNotConnected { name: mcp_name.clone() })?;

            let mcp_tools = client.list_tools().await?;
            for tool in mcp_tools {
                tools.push(ToolDefinition {
                    // Prefix with MCP name for disambiguation
                    name: format!("{}_{}", mcp_name, tool.name),
                    description: tool.description.unwrap_or_default(),
                    input_schema: tool.input_schema,
                });
            }
        }

        Ok(tools)
    }

    /// Execute a tool call
    async fn execute_tool_call(&self, tool_call: &ToolCall) -> Result<String> {
        // Parse MCP name from tool name (format: "mcpname_toolname")
        let parts: Vec<&str> = tool_call.name.splitn(2, '_').collect();
        if parts.len() != 2 {
            return Err(NikaError::InvalidToolName { name: tool_call.name.clone() });
        }

        let mcp_name = parts[0];
        let tool_name = parts[1];

        // Emit tool call event
        self.event_log.emit(EventKind::McpToolCalled {
            task_id: self.task_id.clone().into(),
            tool: tool_call.name.clone(),
            params: tool_call.arguments.clone(),
        });

        let client = self.mcp_clients.get(mcp_name)
            .ok_or_else(|| NikaError::McpNotConnected { name: mcp_name.to_string() })?;

        let start = std::time::Instant::now();
        let result = client.call_tool(tool_name, tool_call.arguments.clone()).await?;
        let duration_ms = start.elapsed().as_millis() as u64;

        // Emit response event
        self.event_log.emit(EventKind::McpToolResponded {
            task_id: self.task_id.clone().into(),
            tool: tool_call.name.clone(),
            duration_ms,
            is_error: result.is_error(),
        });

        Ok(result.text())
    }

    /// Try to parse output as JSON, fallback to string
    fn parse_output(&self, content: &str) -> serde_json::Value {
        // Try to find JSON in content
        if let Some(start) = content.find('{') {
            if let Some(end) = content.rfind('}') {
                if let Ok(json) = serde_json::from_str(&content[start..=end]) {
                    return json;
                }
            }
        }
        serde_json::Value::String(content.to_string())
    }
}
```

### Step 3: Export agent loop

Modify `src/runtime/mod.rs`:

```rust
mod agent_loop;
pub use agent_loop::{AgentLoop, AgentLoopResult, AgentStatus};
```

### Step 4: Add agent events to EventLog

Add to `src/event/log.rs`:

```rust
// ═══════════════════════════════════════════
// AGENT EVENTS (NEW v0.2)
// ═══════════════════════════════════════════
AgentTurnStarted {
    task_id: Arc<str>,
    turn_index: u32,
    message_count: usize,
},
AgentTurnCompleted {
    task_id: Arc<str>,
    turn_index: u32,
    status: String,
},
```

### Step 5: Run tests

Run: `cd tools/nika && cargo test agent_loop`
Expected: All tests pass

### Step 6: Commit

```bash
git add src/runtime/agent_loop.rs tests/agent_loop_test.rs
git commit -m "feat(runtime): implement agent loop for agentic execution

- Multi-turn conversation with tool calling
- MCP tool integration
- Stop conditions and max_turns
- Event emission for observability

Co-Authored-By: Claude <noreply@anthropic.com>"
```

---

## Task 6: Add Agent Execution to Executor

**Files:**
- Modify: `src/runtime/executor.rs`

### Step 1: Add Agent case to execute_task

```rust
TaskAction::Agent { agent } => {
    agent.validate().map_err(|e| NikaError::ValidationError { reason: e })?;

    // Get provider
    let provider = self.get_provider(agent.provider.as_deref()).await?;

    // Get MCP clients for this agent
    let mut mcp_clients = HashMap::new();
    for mcp_name in &agent.mcp {
        let client = self.get_mcp_client(mcp_name).await?;
        mcp_clients.insert(mcp_name.clone(), client);
    }

    // Create and run agent loop
    let agent_loop = AgentLoop::new(
        task_id.clone(),
        agent.clone(),
        self.event_log.clone(),
        mcp_clients,
    )?;

    let result = agent_loop.run(provider).await?;

    // Log completion
    tracing::info!(
        task_id = %task_id,
        turns = result.turns,
        status = ?result.status,
        "Agent loop completed"
    );

    Ok(result.final_output)
}
```

### Step 2: Commit

```bash
git add src/runtime/executor.rs
git commit -m "feat(runtime): add agent: verb execution to executor

- AgentLoop integration
- MCP client setup for agent
- Result extraction

Co-Authored-By: Claude <noreply@anthropic.com>"
```

---

## Task 7: Enhanced EventLog with Generation ID

**Files:**
- Modify: `src/event/log.rs`

### Step 1: Add generation_id to WorkflowStarted

Update EventKind:

```rust
WorkflowStarted {
    task_count: usize,
    /// Unique generation ID for this execution
    generation_id: String,
    /// Hash of workflow file for cache invalidation
    workflow_hash: String,
    /// Nika version
    nika_version: String,
},
```

### Step 2: Add token tracking to ProviderResponded

```rust
ProviderResponded {
    task_id: Arc<str>,
    /// API request ID (for debugging)
    request_id: Option<String>,
    /// Input tokens
    input_tokens: u32,
    /// Output tokens
    output_tokens: u32,
    /// Cache read tokens (if any)
    cache_read_tokens: u32,
    /// Time to first token (ms)
    ttft_ms: Option<u64>,
    /// Finish reason
    finish_reason: String,
    /// Estimated cost in USD
    cost_usd: f64,
},
```

### Step 3: Add context assembly event

```rust
ContextAssembled {
    task_id: Arc<str>,
    /// Sources included in context
    sources: Vec<ContextSource>,
    /// Items excluded (with reasons)
    excluded: Vec<ExcludedItem>,
    /// Total tokens in assembled context
    total_tokens: u32,
    /// Budget utilization percentage
    budget_used_pct: f32,
    /// Was context truncated?
    truncated: bool,
},
```

Add helper structs:

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContextSource {
    pub node: String,
    pub tokens: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExcludedItem {
    pub node: String,
    pub reason: String,
}
```

### Step 4: Commit

```bash
git add src/event/log.rs
git commit -m "feat(event): enhance EventLog with generation_id and token tracking

- generation_id for unique execution identification
- Detailed token split in ProviderResponded
- ContextAssembled event for observability

Co-Authored-By: Claude <noreply@anthropic.com>"
```

---

## Task 8: Create NDJSON Trace Writer

**Files:**
- Create: `src/event/trace.rs`
- Modify: `src/event/mod.rs`

### Step 1: Create trace writer

Create `src/event/trace.rs`:

```rust
//! NDJSON Trace Writer
//!
//! Writes events to newline-delimited JSON files for debugging and replay.

use crate::event::{Event, EventLog};
use crate::error::Result;
use std::fs::{self, File};
use std::io::{BufWriter, Write};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use parking_lot::Mutex;

/// Directory for trace files
const TRACE_DIR: &str = ".nika/traces";

/// NDJSON trace writer
pub struct TraceWriter {
    writer: Arc<Mutex<BufWriter<File>>>,
    path: PathBuf,
}

impl TraceWriter {
    /// Create a new trace writer for a generation
    pub fn new(generation_id: &str) -> Result<Self> {
        // Ensure trace directory exists
        let trace_dir = Path::new(TRACE_DIR);
        fs::create_dir_all(trace_dir)?;

        // Create trace file
        let filename = format!("{}.ndjson", generation_id);
        let path = trace_dir.join(&filename);
        let file = File::create(&path)?;
        let writer = BufWriter::new(file);

        tracing::info!(path = %path.display(), "Created trace file");

        Ok(Self {
            writer: Arc::new(Mutex::new(writer)),
            path,
        })
    }

    /// Write a single event to the trace file
    pub fn write_event(&self, event: &Event) -> Result<()> {
        let json = serde_json::to_string(event)?;

        let mut writer = self.writer.lock();
        writeln!(writer, "{}", json)?;
        writer.flush()?;

        Ok(())
    }

    /// Write all events from an EventLog
    pub fn write_all(&self, event_log: &EventLog) -> Result<()> {
        let events = event_log.events();
        for event in events {
            self.write_event(&event)?;
        }
        Ok(())
    }

    /// Get the trace file path
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Close the trace writer (flushes buffer)
    pub fn close(&self) -> Result<()> {
        let mut writer = self.writer.lock();
        writer.flush()?;
        Ok(())
    }
}

/// Generate a unique generation ID
pub fn generate_generation_id() -> String {
    use chrono::Utc;

    let now = Utc::now();
    let timestamp = now.format("%Y-%m-%dT%H-%M-%S");
    let random: u32 = rand::random::<u32>() % 0xFFFF;

    format!("{}-{:04x}", timestamp, random)
}

/// Calculate workflow hash (for cache invalidation)
pub fn calculate_workflow_hash(yaml: &str) -> String {
    use xxhash_rust::xxh3::xxh3_64;

    let hash = xxh3_64(yaml.as_bytes());
    format!("xxh3:{:016x}", hash)
}

/// List all trace files
pub fn list_traces() -> Result<Vec<TraceInfo>> {
    let trace_dir = Path::new(TRACE_DIR);

    if !trace_dir.exists() {
        return Ok(vec![]);
    }

    let mut traces = Vec::new();

    for entry in fs::read_dir(trace_dir)? {
        let entry = entry?;
        let path = entry.path();

        if path.extension().map(|e| e == "ndjson").unwrap_or(false) {
            let metadata = entry.metadata()?;
            let generation_id = path.file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or("unknown")
                .to_string();

            traces.push(TraceInfo {
                generation_id,
                path,
                size_bytes: metadata.len(),
                created: metadata.created().ok(),
            });
        }
    }

    // Sort by creation time (newest first)
    traces.sort_by(|a, b| b.created.cmp(&a.created));

    Ok(traces)
}

/// Information about a trace file
#[derive(Debug)]
pub struct TraceInfo {
    pub generation_id: String,
    pub path: PathBuf,
    pub size_bytes: u64,
    pub created: Option<std::time::SystemTime>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_generation_id_format() {
        let id = generate_generation_id();
        // Format: YYYY-MM-DDTHH-MM-SS-XXXX
        assert!(id.len() > 20);
        assert!(id.contains('T'));
    }

    #[test]
    fn test_workflow_hash() {
        let yaml = "schema: test\ntasks: []";
        let hash = calculate_workflow_hash(yaml);
        assert!(hash.starts_with("xxh3:"));
        assert_eq!(hash.len(), 21); // "xxh3:" + 16 hex chars
    }
}
```

### Step 2: Add chrono and rand dependencies

Add to Cargo.toml:

```toml
chrono = "0.4"
rand = "0.8"
```

### Step 3: Export trace module

Modify `src/event/mod.rs`:

```rust
mod trace;
pub use trace::*;
```

### Step 4: Run tests

Run: `cd tools/nika && cargo test trace`
Expected: All tests pass

### Step 5: Commit

```bash
git add src/event/trace.rs Cargo.toml
git commit -m "feat(event): add NDJSON trace writer

- TraceWriter for streaming events to files
- generation_id and workflow_hash utilities
- list_traces() for CLI trace listing

Co-Authored-By: Claude <noreply@anthropic.com>"
```

---

## Task 9: Create Example Agent Workflow

**Files:**
- Create: `examples/agent-novanet.yaml`

### Step 1: Create example

```yaml
# Example: Agent with NovaNet MCP tools
#
# This workflow demonstrates the agent: verb for agentic execution.
# Run with: cargo run -- run examples/agent-novanet.yaml

schema: "nika/workflow@0.2"
provider: claude

mcp:
  novanet:
    command: cargo
    args:
      - run
      - --manifest-path
      - ../../novanet-dev/tools/novanet-mcp/Cargo.toml

tasks:
  # Single-step agent that uses tools autonomously
  - id: generate_content
    agent:
      prompt: |
        Generate native French content for the QR Code homepage hero block.

        INSTRUCTIONS:
        1. Use novanet_generate to get full context for entity "qr-code" in locale "fr-FR"
        2. Use the denomination_forms EXACTLY as provided (no paraphrasing)
        3. Generate a JSON object with:
           - title: H1 for the hero (use denomination_forms.title)
           - subtitle: Short tagline
           - description: 2-3 sentences (use denomination_forms.text)
           - cta_text: Call-to-action button text

        When complete, output the JSON and say "GENERATION_COMPLETE".

      provider: claude
      model: claude-sonnet-4
      mcp:
        - novanet
      max_turns: 5
      stop_conditions:
        - "GENERATION_COMPLETE"
    output:
      format: json
```

### Step 2: Commit

```bash
git add examples/agent-novanet.yaml
git commit -m "docs: add agent-novanet example workflow

Demonstrates:
- agent: verb with MCP tools
- Stop conditions
- Multi-turn reasoning

Co-Authored-By: Claude <noreply@anthropic.com>"
```

---

## Summary

After completing MVP 2, Nika will have:

1. **Provider Types** - Message, ToolDefinition, ChatResponse
2. **LlmProvider.chat()** - Multi-turn with tools
3. **Claude Tool Calling** - Full implementation
4. **AgentParams** - AST for agent: verb
5. **AgentLoop** - Agentic execution engine
6. **Enhanced Events** - generation_id, token split, context assembly
7. **NDJSON Traces** - Persistent event logs

**Verify Success:**

```bash
# All tests pass
cargo test

# Example workflows parse
cargo run -- validate examples/invoke-novanet.yaml
cargo run -- validate examples/agent-novanet.yaml

# Trace files created
ls -la .nika/traces/
```

**Next:** Proceed to MVP 3 (TUI + CLI Trace) plan.
