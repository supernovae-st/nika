# Nika v0.2 NovaNet Integration — Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Add `invoke:` and `agent:` verbs to Nika for NovaNet MCP integration, enabling intelligent YAML workflows with knowledge graph memory.

**Architecture:** Nika v0.2 becomes the "body" (execution engine) that connects to NovaNet's "brain" (knowledge graph) via MCP protocol. Workflows use high-level semantic tools (novanet_generate, novanet_traverse) instead of raw Cypher queries.

**Tech Stack:** Rust 1.75+, tokio, rmcp (Anthropic MCP SDK), serde, neo4rs (indirect via MCP)

---

## Task 1: Add MCP Client Infrastructure

**Files:**
- Create: `src/mcp/mod.rs`
- Create: `src/mcp/client.rs`
- Create: `src/mcp/types.rs`
- Modify: `src/lib.rs`
- Modify: `Cargo.toml`
- Test: `tests/mcp_client_test.rs`

### Step 1: Write the failing test

```rust
// tests/mcp_client_test.rs
use nika::mcp::{McpClient, McpConfig};

#[tokio::test]
async fn test_mcp_client_creation() {
    let config = McpConfig {
        name: "novanet".to_string(),
        command: "cargo".to_string(),
        args: vec!["run".to_string(), "--manifest-path".to_string(),
                   "/path/to/novanet-mcp/Cargo.toml".to_string()],
        env: std::collections::HashMap::new(),
    };

    let client = McpClient::new(config);
    assert!(client.is_ok());
}

#[tokio::test]
async fn test_mcp_tool_call() {
    let client = McpClient::mock();
    let result = client.call_tool("novanet_describe", serde_json::json!({
        "target": "schema"
    })).await;

    assert!(result.is_ok());
}
```

### Step 2: Run test to verify it fails

Run: `cargo test --test mcp_client_test`
Expected: FAIL with "unresolved import `nika::mcp`"

### Step 3: Add dependencies to Cargo.toml

```toml
# Cargo.toml - add to [dependencies]
rmcp = { version = "0.15", features = ["client", "transport-child-process"] }
```

### Step 4: Create MCP types module

```rust
// src/mcp/types.rs
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Clone, Deserialize)]
pub struct McpConfig {
    pub name: String,
    pub command: String,
    pub args: Vec<String>,
    #[serde(default)]
    pub env: HashMap<String, String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ToolCall {
    pub name: String,
    pub arguments: serde_json::Value,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ToolResult {
    pub content: Vec<ContentBlock>,
    pub is_error: bool,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ContentBlock {
    #[serde(rename = "type")]
    pub content_type: String,
    pub text: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ResourceContent {
    pub uri: String,
    pub mime_type: Option<String>,
    pub text: Option<String>,
}
```

### Step 5: Create MCP client module

```rust
// src/mcp/client.rs
use crate::error::{NikaError, Result};
use crate::mcp::types::{McpConfig, ToolCall, ToolResult, ResourceContent};
use rmcp::{ServiceExt, transport::TokioChildProcess};
use std::sync::Arc;
use tokio::sync::RwLock;

pub struct McpClient {
    config: McpConfig,
    service: Arc<RwLock<Option<rmcp::Client>>>,
}

impl McpClient {
    pub fn new(config: McpConfig) -> Result<Self> {
        Ok(Self {
            config,
            service: Arc::new(RwLock::new(None)),
        })
    }

    pub async fn connect(&self) -> Result<()> {
        let transport = TokioChildProcess::new(&self.config.command, &self.config.args)?;
        let client = rmcp::Client::new(transport).await?;

        let mut service = self.service.write().await;
        *service = Some(client);
        Ok(())
    }

    pub async fn call_tool(&self, name: &str, params: serde_json::Value) -> Result<ToolResult> {
        let service = self.service.read().await;
        let client = service.as_ref()
            .ok_or_else(|| NikaError::McpNotConnected(self.config.name.clone()))?;

        let result = client.call_tool(name, params).await
            .map_err(|e| NikaError::McpToolError(e.to_string()))?;

        Ok(result.into())
    }

    pub async fn read_resource(&self, uri: &str) -> Result<ResourceContent> {
        let service = self.service.read().await;
        let client = service.as_ref()
            .ok_or_else(|| NikaError::McpNotConnected(self.config.name.clone()))?;

        let result = client.read_resource(uri).await
            .map_err(|e| NikaError::McpResourceError(e.to_string()))?;

        Ok(result.into())
    }

    #[cfg(test)]
    pub fn mock() -> Self {
        Self {
            config: McpConfig {
                name: "mock".to_string(),
                command: "echo".to_string(),
                args: vec![],
                env: std::collections::HashMap::new(),
            },
            service: Arc::new(RwLock::new(None)),
        }
    }
}
```

### Step 6: Create MCP module entry point

```rust
// src/mcp/mod.rs
mod client;
mod types;

pub use client::McpClient;
pub use types::{McpConfig, ToolCall, ToolResult, ResourceContent};
```

### Step 7: Add MCP error variants

```rust
// src/error.rs - add to NikaError enum
#[error("MCP server '{0}' not connected")]
McpNotConnected(String),

#[error("MCP tool error: {0}")]
McpToolError(String),

#[error("MCP resource error: {0}")]
McpResourceError(String),
```

### Step 8: Export MCP module

```rust
// src/lib.rs - add
pub mod mcp;
```

### Step 9: Run test to verify it passes

Run: `cargo test --test mcp_client_test`
Expected: PASS

### Step 10: Commit

```bash
git add src/mcp/ tests/mcp_client_test.rs Cargo.toml src/error.rs src/lib.rs
git commit -m "feat(mcp): add MCP client infrastructure for tool/resource calls

Co-Authored-By: Claude Opus 4.5 <noreply@anthropic.com>"
```

---

## Task 2: Add `invoke:` Verb

**Files:**
- Modify: `src/ast/action.rs`
- Create: `src/ast/invoke.rs`
- Modify: `src/runtime/executor.rs`
- Test: `tests/invoke_test.rs`

### Step 1: Write the failing test

```rust
// tests/invoke_test.rs
use nika::ast::{TaskAction, InvokeParams};

#[test]
fn test_invoke_params_deserialization() {
    let yaml = r#"
invoke:
  mcp: novanet
  tool: novanet_generate
  params:
    mode: block
    page_key: homepage
    block_key: hero
    locale: fr-FR
"#;

    let action: TaskAction = serde_yaml::from_str(yaml).unwrap();

    match action {
        TaskAction::Invoke { invoke } => {
            assert_eq!(invoke.mcp, "novanet");
            assert_eq!(invoke.tool, Some("novanet_generate".to_string()));
        }
        _ => panic!("Expected Invoke variant"),
    }
}

#[test]
fn test_invoke_resource() {
    let yaml = r#"
invoke:
  mcp: novanet
  resource: "entity://qr-code-generator"
"#;

    let action: TaskAction = serde_yaml::from_str(yaml).unwrap();

    match action {
        TaskAction::Invoke { invoke } => {
            assert_eq!(invoke.resource, Some("entity://qr-code-generator".to_string()));
        }
        _ => panic!("Expected Invoke variant"),
    }
}
```

### Step 2: Run test to verify it fails

Run: `cargo test --test invoke_test`
Expected: FAIL with "unknown variant `invoke`"

### Step 3: Create InvokeParams struct

```rust
// src/ast/invoke.rs
use serde::Deserialize;

/// Parameters for MCP tool or resource invocation.
///
/// Use `tool` + `params` for tool calls.
/// Use `resource` for resource reads.
#[derive(Debug, Clone, Deserialize)]
pub struct InvokeParams {
    /// MCP server name (must be configured in workflow or manifest)
    pub mcp: String,

    /// Tool name to invoke (e.g., "novanet_generate")
    pub tool: Option<String>,

    /// Tool parameters as JSON
    pub params: Option<serde_json::Value>,

    /// Resource URI to read (e.g., "entity://qr-code-generator")
    pub resource: Option<String>,
}

impl InvokeParams {
    /// Validate that either tool or resource is specified, not both.
    pub fn validate(&self) -> Result<(), String> {
        match (&self.tool, &self.resource) {
            (Some(_), Some(_)) => Err("Cannot specify both 'tool' and 'resource'".to_string()),
            (None, None) => Err("Must specify either 'tool' or 'resource'".to_string()),
            _ => Ok(()),
        }
    }
}
```

### Step 4: Add Invoke variant to TaskAction

```rust
// src/ast/action.rs - add to TaskAction enum
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TaskAction {
    Infer { infer: InferParams },
    Exec { exec: ExecParams },
    Fetch { fetch: FetchParams },
    Invoke { invoke: InvokeParams },  // NEW
}

// Add to imports
use crate::ast::invoke::InvokeParams;
```

### Step 5: Export invoke module

```rust
// src/ast/mod.rs - add
mod invoke;
pub use invoke::InvokeParams;
```

### Step 6: Implement invoke execution

```rust
// src/runtime/executor.rs - add to execute_task match
TaskAction::Invoke { invoke } => {
    invoke.validate().map_err(|e| NikaError::ValidationError(e))?;

    let mcp_client = self.get_mcp_client(&invoke.mcp).await?;

    let result = if let Some(tool) = &invoke.tool {
        let params = invoke.params.clone().unwrap_or(serde_json::Value::Null);
        let tool_result = mcp_client.call_tool(tool, params).await?;

        // Extract text from content blocks
        tool_result.content.iter()
            .filter_map(|block| block.text.clone())
            .collect::<Vec<_>>()
            .join("\n")
    } else if let Some(resource) = &invoke.resource {
        let resource_content = mcp_client.read_resource(resource).await?;
        resource_content.text.unwrap_or_default()
    } else {
        unreachable!("validate() ensures tool or resource is set")
    };

    Ok(serde_json::Value::String(result))
}
```

### Step 7: Run test to verify it passes

Run: `cargo test --test invoke_test`
Expected: PASS

### Step 8: Commit

```bash
git add src/ast/invoke.rs src/ast/action.rs src/ast/mod.rs src/runtime/executor.rs tests/invoke_test.rs
git commit -m "feat(invoke): add invoke: verb for MCP tool and resource calls

Co-Authored-By: Claude Opus 4.5 <noreply@anthropic.com>"
```

---

## Task 3: Add `agent:` Verb

**Files:**
- Create: `src/ast/agent.rs`
- Modify: `src/ast/action.rs`
- Create: `src/runtime/agent_loop.rs`
- Modify: `src/runtime/executor.rs`
- Test: `tests/agent_test.rs`

### Step 1: Write the failing test

```rust
// tests/agent_test.rs
use nika::ast::{TaskAction, AgentParams};

#[test]
fn test_agent_params_deserialization() {
    let yaml = r#"
agent:
  prompt: |
    Generate native content for the homepage hero block.
    Use @entity:qr-code-generator for the main concept.
    Follow denomination_forms EXACTLY.
  provider: claude
  mcp:
    - novanet
  max_turns: 10
  stop_conditions:
    - "GENERATION_COMPLETE"
    - "VALIDATION_PASSED"
"#;

    let action: TaskAction = serde_yaml::from_str(yaml).unwrap();

    match action {
        TaskAction::Agent { agent } => {
            assert_eq!(agent.mcp, vec!["novanet"]);
            assert_eq!(agent.max_turns, Some(10));
            assert!(agent.prompt.contains("denomination_forms"));
        }
        _ => panic!("Expected Agent variant"),
    }
}

#[test]
fn test_agent_with_scope() {
    let yaml = r#"
agent:
  prompt: "Simple analysis task"
  scope: minimal
"#;

    let action: TaskAction = serde_yaml::from_str(yaml).unwrap();

    match action {
        TaskAction::Agent { agent } => {
            assert_eq!(agent.scope, Some("minimal".to_string()));
        }
        _ => panic!("Expected Agent variant"),
    }
}
```

### Step 2: Run test to verify it fails

Run: `cargo test --test agent_test`
Expected: FAIL with "unknown variant `agent`"

### Step 3: Create AgentParams struct

```rust
// src/ast/agent.rs
use serde::Deserialize;

/// Parameters for agentic execution with tool access.
///
/// The agent runs an LLM loop that can call MCP tools until
/// completion or max_turns is reached.
#[derive(Debug, Clone, Deserialize)]
pub struct AgentParams {
    /// System/user prompt for the agent
    pub prompt: String,

    /// LLM provider (claude, openai, etc.)
    pub provider: Option<String>,

    /// Model override
    pub model: Option<String>,

    /// MCP servers the agent can access
    #[serde(default)]
    pub mcp: Vec<String>,

    /// Maximum agentic turns before stopping
    pub max_turns: Option<u32>,

    /// Conditions that trigger early stop
    #[serde(default)]
    pub stop_conditions: Vec<String>,

    /// Scope preset (full, minimal, debug, default)
    pub scope: Option<String>,
}

impl Default for AgentParams {
    fn default() -> Self {
        Self {
            prompt: String::new(),
            provider: None,
            model: None,
            mcp: vec![],
            max_turns: Some(20),
            stop_conditions: vec![],
            scope: Some("default".to_string()),
        }
    }
}
```

### Step 4: Add Agent variant to TaskAction

```rust
// src/ast/action.rs - add to TaskAction enum
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TaskAction {
    Infer { infer: InferParams },
    Exec { exec: ExecParams },
    Fetch { fetch: FetchParams },
    Invoke { invoke: InvokeParams },
    Agent { agent: AgentParams },  // NEW
}

// Add to imports
use crate::ast::agent::AgentParams;
```

### Step 5: Export agent module

```rust
// src/ast/mod.rs - add
mod agent;
pub use agent::AgentParams;
```

### Step 6: Create agent loop module

```rust
// src/runtime/agent_loop.rs
use crate::error::{NikaError, Result};
use crate::ast::AgentParams;
use crate::mcp::McpClient;
use crate::provider::LlmProvider;
use std::collections::HashMap;
use std::sync::Arc;

pub struct AgentLoop {
    params: AgentParams,
    provider: Arc<dyn LlmProvider>,
    mcp_clients: HashMap<String, Arc<McpClient>>,
}

impl AgentLoop {
    pub fn new(
        params: AgentParams,
        provider: Arc<dyn LlmProvider>,
        mcp_clients: HashMap<String, Arc<McpClient>>,
    ) -> Self {
        Self { params, provider, mcp_clients }
    }

    pub async fn run(&self) -> Result<serde_json::Value> {
        let max_turns = self.params.max_turns.unwrap_or(20);
        let mut conversation = vec![];
        let mut turn = 0;

        // Initial prompt
        conversation.push(Message::user(&self.params.prompt));

        loop {
            if turn >= max_turns {
                return Ok(serde_json::json!({
                    "status": "max_turns_reached",
                    "turns": turn,
                    "conversation": conversation
                }));
            }

            // Get LLM response with tool definitions
            let tools = self.build_tool_definitions().await?;
            let response = self.provider.chat(&conversation, Some(&tools)).await?;

            conversation.push(Message::assistant(&response.content));

            // Check for tool calls
            if let Some(tool_calls) = response.tool_calls {
                for tool_call in tool_calls {
                    let result = self.execute_tool_call(&tool_call).await?;
                    conversation.push(Message::tool_result(&tool_call.id, &result));
                }
            }

            // Check stop conditions
            if self.check_stop_conditions(&response.content) {
                return Ok(serde_json::json!({
                    "status": "completed",
                    "turns": turn,
                    "result": response.content
                }));
            }

            // Check if no tool calls (natural completion)
            if response.tool_calls.is_none() || response.tool_calls.as_ref().map(|t| t.is_empty()).unwrap_or(true) {
                return Ok(serde_json::json!({
                    "status": "natural_completion",
                    "turns": turn,
                    "result": response.content
                }));
            }

            turn += 1;
        }
    }

    fn check_stop_conditions(&self, content: &str) -> bool {
        self.params.stop_conditions.iter()
            .any(|condition| content.contains(condition))
    }

    async fn execute_tool_call(&self, tool_call: &ToolCall) -> Result<String> {
        // Parse MCP server from tool name (e.g., "novanet__novanet_generate")
        let parts: Vec<&str> = tool_call.name.splitn(2, "__").collect();
        if parts.len() != 2 {
            return Err(NikaError::InvalidToolName(tool_call.name.clone()));
        }

        let mcp_name = parts[0];
        let tool_name = parts[1];

        let client = self.mcp_clients.get(mcp_name)
            .ok_or_else(|| NikaError::McpNotConnected(mcp_name.to_string()))?;

        let result = client.call_tool(tool_name, tool_call.arguments.clone()).await?;

        Ok(result.content.iter()
            .filter_map(|block| block.text.clone())
            .collect::<Vec<_>>()
            .join("\n"))
    }

    async fn build_tool_definitions(&self) -> Result<Vec<ToolDefinition>> {
        let mut tools = vec![];

        for (name, client) in &self.mcp_clients {
            let mcp_tools = client.list_tools().await?;
            for tool in mcp_tools {
                tools.push(ToolDefinition {
                    name: format!("{}__{}", name, tool.name),
                    description: tool.description,
                    input_schema: tool.input_schema,
                });
            }
        }

        Ok(tools)
    }
}
```

### Step 7: Implement agent execution

```rust
// src/runtime/executor.rs - add to execute_task match
TaskAction::Agent { agent } => {
    // Get provider
    let provider = self.get_provider(&agent.provider).await?;

    // Get MCP clients
    let mut mcp_clients = HashMap::new();
    for mcp_name in &agent.mcp {
        let client = self.get_mcp_client(mcp_name).await?;
        mcp_clients.insert(mcp_name.clone(), client);
    }

    // Run agent loop
    let agent_loop = AgentLoop::new(agent.clone(), provider, mcp_clients);
    agent_loop.run().await
}
```

### Step 8: Run test to verify it passes

Run: `cargo test --test agent_test`
Expected: PASS

### Step 9: Commit

```bash
git add src/ast/agent.rs src/ast/action.rs src/ast/mod.rs src/runtime/agent_loop.rs src/runtime/executor.rs tests/agent_test.rs
git commit -m "feat(agent): add agent: verb for agentic execution with MCP tools

Co-Authored-By: Claude Opus 4.5 <noreply@anthropic.com>"
```

---

## Task 4: Update Schema Version to 0.2

**Files:**
- Modify: `src/ast/workflow.rs`
- Modify: `src/validation/schema.rs`
- Test: `tests/schema_version_test.rs`

### Step 1: Write the failing test

```rust
// tests/schema_version_test.rs
use nika::ast::Workflow;

#[test]
fn test_schema_version_0_2() {
    let yaml = r#"
schema: "nika/workflow@0.2"
provider: claude

mcp:
  novanet:
    command: "cargo"
    args: ["run", "--manifest-path", "/path/to/novanet-mcp/Cargo.toml"]

tasks:
  - id: generate
    invoke:
      mcp: novanet
      tool: novanet_generate
      params:
        mode: block
"#;

    let workflow: Workflow = serde_yaml::from_str(yaml).unwrap();
    assert_eq!(workflow.schema, "nika/workflow@0.2");
    assert!(workflow.mcp.is_some());
}

#[test]
fn test_schema_version_backward_compat() {
    let yaml = r#"
schema: "nika/workflow@0.1"
provider: claude

tasks:
  - id: hello
    infer:
      prompt: "Say hello"
"#;

    let workflow: Workflow = serde_yaml::from_str(yaml).unwrap();
    assert_eq!(workflow.schema, "nika/workflow@0.1");
}
```

### Step 2: Run test to verify it fails

Run: `cargo test --test schema_version_test`
Expected: FAIL (mcp field not recognized)

### Step 3: Add MCP config to Workflow

```rust
// src/ast/workflow.rs - add to Workflow struct
use crate::mcp::McpConfig;
use std::collections::HashMap;

#[derive(Debug, Clone, Deserialize)]
pub struct Workflow {
    pub schema: String,
    pub provider: String,
    pub model: Option<String>,
    pub tasks: Vec<Arc<Task>>,
    pub flows: Vec<Flow>,

    /// MCP server configurations (new in v0.2)
    #[serde(default)]
    pub mcp: Option<HashMap<String, McpConfig>>,
}
```

### Step 4: Update schema validation

```rust
// src/validation/schema.rs
pub fn validate_schema_version(schema: &str) -> Result<SchemaVersion> {
    match schema {
        "nika/workflow@0.1" => Ok(SchemaVersion::V0_1),
        "nika/workflow@0.2" => Ok(SchemaVersion::V0_2),
        _ => Err(NikaError::InvalidSchemaVersion(schema.to_string())),
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum SchemaVersion {
    V0_1,
    V0_2,
}

impl SchemaVersion {
    pub fn supports_invoke(&self) -> bool {
        matches!(self, SchemaVersion::V0_2)
    }

    pub fn supports_agent(&self) -> bool {
        matches!(self, SchemaVersion::V0_2)
    }
}
```

### Step 5: Run test to verify it passes

Run: `cargo test --test schema_version_test`
Expected: PASS

### Step 6: Commit

```bash
git add src/ast/workflow.rs src/validation/schema.rs tests/schema_version_test.rs
git commit -m "feat(schema): bump to v0.2 with MCP configuration support

Co-Authored-By: Claude Opus 4.5 <noreply@anthropic.com>"
```

---

## Task 5: Add Provider Tool Support

**Files:**
- Modify: `src/provider/claude.rs`
- Modify: `src/provider/openai.rs`
- Create: `src/provider/types.rs`
- Test: `tests/provider_tools_test.rs`

### Step 1: Write the failing test

```rust
// tests/provider_tools_test.rs
use nika::provider::{Message, ToolDefinition, ChatResponse};

#[test]
fn test_tool_definition() {
    let tool = ToolDefinition {
        name: "novanet__novanet_generate".to_string(),
        description: "Generate content context for a block or page".to_string(),
        input_schema: serde_json::json!({
            "type": "object",
            "properties": {
                "mode": { "type": "string", "enum": ["block", "page"] },
                "page_key": { "type": "string" }
            },
            "required": ["mode", "page_key"]
        }),
    };

    assert!(tool.name.contains("__"));
}

#[test]
fn test_chat_response_with_tools() {
    let response = ChatResponse {
        content: "I'll generate the content now.".to_string(),
        tool_calls: Some(vec![
            ToolCall {
                id: "call_123".to_string(),
                name: "novanet__novanet_generate".to_string(),
                arguments: serde_json::json!({"mode": "block"}),
            }
        ]),
    };

    assert!(response.tool_calls.is_some());
    assert_eq!(response.tool_calls.as_ref().unwrap().len(), 1);
}
```

### Step 2: Create provider types

```rust
// src/provider/types.rs
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Message {
    pub role: String,
    pub content: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_call_id: Option<String>,
}

impl Message {
    pub fn user(content: &str) -> Self {
        Self { role: "user".to_string(), content: content.to_string(), tool_call_id: None }
    }

    pub fn assistant(content: &str) -> Self {
        Self { role: "assistant".to_string(), content: content.to_string(), tool_call_id: None }
    }

    pub fn tool_result(id: &str, content: &str) -> Self {
        Self { role: "tool".to_string(), content: content.to_string(), tool_call_id: Some(id.to_string()) }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct ToolDefinition {
    pub name: String,
    pub description: String,
    pub input_schema: serde_json::Value,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ToolCall {
    pub id: String,
    pub name: String,
    pub arguments: serde_json::Value,
}

#[derive(Debug, Clone)]
pub struct ChatResponse {
    pub content: String,
    pub tool_calls: Option<Vec<ToolCall>>,
}
```

### Step 3: Update LlmProvider trait

```rust
// src/provider/mod.rs
use crate::provider::types::{Message, ToolDefinition, ChatResponse};

#[async_trait]
pub trait LlmProvider: Send + Sync {
    async fn infer(&self, prompt: &str) -> Result<String>;

    async fn chat(
        &self,
        messages: &[Message],
        tools: Option<&[ToolDefinition]>
    ) -> Result<ChatResponse>;
}
```

### Step 4: Implement for Claude provider

```rust
// src/provider/claude.rs - add chat method
async fn chat(
    &self,
    messages: &[Message],
    tools: Option<&[ToolDefinition]>,
) -> Result<ChatResponse> {
    let mut request = serde_json::json!({
        "model": self.model,
        "max_tokens": 4096,
        "messages": messages
    });

    if let Some(tools) = tools {
        request["tools"] = serde_json::to_value(tools.iter().map(|t| {
            serde_json::json!({
                "name": t.name,
                "description": t.description,
                "input_schema": t.input_schema
            })
        }).collect::<Vec<_>>())?;
    }

    let response = self.client
        .post("https://api.anthropic.com/v1/messages")
        .header("x-api-key", &self.api_key)
        .header("anthropic-version", "2023-06-01")
        .json(&request)
        .send()
        .await?;

    let body: serde_json::Value = response.json().await?;

    // Parse response content and tool_use blocks
    let content = body["content"]
        .as_array()
        .and_then(|arr| arr.iter()
            .find(|b| b["type"] == "text")
            .and_then(|b| b["text"].as_str()))
        .unwrap_or("")
        .to_string();

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

    Ok(ChatResponse { content, tool_calls })
}
```

### Step 5: Run test to verify it passes

Run: `cargo test --test provider_tools_test`
Expected: PASS

### Step 6: Commit

```bash
git add src/provider/types.rs src/provider/mod.rs src/provider/claude.rs tests/provider_tools_test.rs
git commit -m "feat(provider): add tool/function calling support for agentic workflows

Co-Authored-By: Claude Opus 4.5 <noreply@anthropic.com>"
```

---

## Task 6: Integration Test with NovaNet

**Files:**
- Create: `tests/integration/novanet_workflow.rs`
- Create: `examples/novanet-generation.yaml`

### Step 1: Create example workflow

```yaml
# examples/novanet-generation.yaml
schema: "nika/workflow@0.2"
provider: claude

mcp:
  novanet:
    command: "cargo"
    args: ["run", "--manifest-path", "../../novanet-hq/tools/novanet-mcp/Cargo.toml"]

tasks:
  # 1. Discover page structure
  - id: discover
    invoke:
      mcp: novanet
      tool: novanet_describe
      params:
        target: page
        filters:
          key: homepage
    output:
      format: json

  # 2. Get generation context for hero block
  - id: hero_context
    use:
      page: discover.key
    invoke:
      mcp: novanet
      tool: novanet_generate
      params:
        mode: block
        page_key: "{{use.page}}"
        block_key: hero
        locale: fr-FR
        token_budget: 8000
    output:
      format: json

  # 3. Agent generates content using context
  - id: generate_hero
    use:
      context: hero_context
    agent:
      prompt: |
        Generate native French content for the hero block.

        CONTEXT:
        {{use.context}}

        RULES:
        - Use ONLY denomination_forms values from context
        - NO invention, NO paraphrase of entity names
        - Follow @ references for brand voice and style

        Output JSON matching the block schema.
      provider: claude
      mcp:
        - novanet
      max_turns: 5
      stop_conditions:
        - "GENERATION_COMPLETE"
    output:
      format: json

flows:
  - source: discover
    target: hero_context
  - source: hero_context
    target: generate_hero
```

### Step 2: Write integration test

```rust
// tests/integration/novanet_workflow.rs
use nika::Workflow;
use std::path::Path;

#[tokio::test]
#[ignore] // Requires running NovaNet MCP server
async fn test_novanet_generation_workflow() {
    let workflow_path = Path::new("examples/novanet-generation.yaml");
    let workflow = Workflow::from_file(workflow_path).unwrap();

    // Validate workflow structure
    assert_eq!(workflow.schema, "nika/workflow@0.2");
    assert_eq!(workflow.tasks.len(), 3);

    // Validate MCP config
    let mcp = workflow.mcp.as_ref().unwrap();
    assert!(mcp.contains_key("novanet"));

    // Validate task types
    assert!(matches!(workflow.tasks[0].action, TaskAction::Invoke { .. }));
    assert!(matches!(workflow.tasks[1].action, TaskAction::Invoke { .. }));
    assert!(matches!(workflow.tasks[2].action, TaskAction::Agent { .. }));
}

#[tokio::test]
#[ignore] // Requires running NovaNet MCP server and Neo4j
async fn test_full_novanet_execution() {
    let workflow_path = Path::new("examples/novanet-generation.yaml");
    let result = nika::run(workflow_path).await;

    assert!(result.is_ok());

    let output = result.unwrap();
    let hero_content = output.get("generate_hero").unwrap();

    // Verify output has expected structure
    assert!(hero_content.get("title").is_some());
    assert!(hero_content.get("description").is_some());
}
```

### Step 3: Run integration test (manual)

```bash
# Start NovaNet MCP server in another terminal
cd /path/to/novanet-hq/tools/novanet-mcp
cargo run

# Run integration test
cargo test --test integration -- --ignored
```

### Step 4: Commit

```bash
git add tests/integration/ examples/novanet-generation.yaml
git commit -m "test(integration): add NovaNet MCP workflow integration tests

Co-Authored-By: Claude Opus 4.5 <noreply@anthropic.com>"
```

---

## Summary

After completing all 6 tasks, Nika v0.2 will have:

1. **MCP Client Infrastructure** - Connect to any MCP server via child process transport
2. **`invoke:` Verb** - Call MCP tools or read MCP resources
3. **`agent:` Verb** - Run agentic loops with MCP tool access
4. **Schema v0.2** - With MCP configuration support
5. **Provider Tool Support** - Claude/OpenAI can call tools in agentic mode
6. **NovaNet Integration** - Example workflow demonstrating the full pipeline

**Nika v0.2 + NovaNet MCP = Intelligent YAML Workflows with Knowledge Graph Memory**

```
┌─────────────────────────────────────────────────────────────────┐
│  NIKA v0.2 WORKFLOW                                             │
├─────────────────────────────────────────────────────────────────┤
│                                                                 │
│  invoke: novanet_describe  →  Discover entities/pages/locales  │
│  invoke: novanet_generate  →  Get full generation context      │
│  agent: (with mcp: novanet) → Generate content with tools      │
│                                                                 │
│  ZERO CYPHER in workflow YAML                                   │
│  NovaNet MCP handles all graph complexity                       │
│                                                                 │
└─────────────────────────────────────────────────────────────────┘
```
