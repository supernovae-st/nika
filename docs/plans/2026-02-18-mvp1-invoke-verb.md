# MVP 1: Invoke Verb — MCP Tool Calling

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Enable Nika to call NovaNet MCP tools via the `invoke:` verb, establishing the foundation for knowledge graph integration.

**Architecture:** Nika spawns NovaNet MCP server as child process, communicates via stdio using MCP protocol. The `invoke:` verb allows workflows to call any MCP tool without agentic loops.

**Tech Stack:** Rust, rmcp (MCP SDK), tokio, serde

**Estimated Time:** 4-6 hours

**Prerequisites:** MVP 0 (DX Setup Core) completed

---

## Task 1: Create MCP Types Module

**Files:**
- Create: `src/mcp/mod.rs`
- Create: `src/mcp/types.rs`
- Modify: `src/lib.rs`

### Step 1: Write the types module

Create `src/mcp/types.rs`:

```rust
//! MCP Protocol Types
//!
//! Types for MCP client-server communication.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Configuration for an MCP server
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct McpConfig {
    /// Server name (used as reference in workflows)
    pub name: String,
    /// Command to spawn the server
    pub command: String,
    /// Arguments for the command
    #[serde(default)]
    pub args: Vec<String>,
    /// Environment variables
    #[serde(default)]
    pub env: HashMap<String, String>,
    /// Working directory (optional)
    pub cwd: Option<String>,
}

/// A tool call request
#[derive(Debug, Clone, Serialize)]
pub struct ToolCallRequest {
    pub name: String,
    pub arguments: serde_json::Value,
}

/// Result from a tool call
#[derive(Debug, Clone, Deserialize)]
pub struct ToolCallResult {
    pub content: Vec<ContentBlock>,
    #[serde(default)]
    pub is_error: bool,
}

impl ToolCallResult {
    /// Extract text content from all blocks
    pub fn text(&self) -> String {
        self.content
            .iter()
            .filter_map(|block| block.text.as_ref())
            .cloned()
            .collect::<Vec<_>>()
            .join("\n")
    }

    /// Check if result contains an error
    pub fn is_error(&self) -> bool {
        self.is_error
    }
}

/// A content block in tool results
#[derive(Debug, Clone, Deserialize)]
pub struct ContentBlock {
    #[serde(rename = "type")]
    pub content_type: String,
    pub text: Option<String>,
    pub data: Option<String>,
    pub mime_type: Option<String>,
}

/// Resource content from MCP
#[derive(Debug, Clone, Deserialize)]
pub struct ResourceContent {
    pub uri: String,
    pub mime_type: Option<String>,
    pub text: Option<String>,
}

/// Tool definition from MCP server
#[derive(Debug, Clone, Deserialize)]
pub struct ToolDefinition {
    pub name: String,
    pub description: Option<String>,
    #[serde(rename = "inputSchema")]
    pub input_schema: serde_json::Value,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_mcp_config_deserialize() {
        let yaml = r#"
name: novanet
command: cargo
args:
  - run
  - -p
  - novanet-mcp
env:
  NEO4J_URI: bolt://localhost:7687
"#;
        let config: McpConfig = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(config.name, "novanet");
        assert_eq!(config.command, "cargo");
        assert_eq!(config.args.len(), 3);
        assert_eq!(config.env.get("NEO4J_URI"), Some(&"bolt://localhost:7687".to_string()));
    }

    #[test]
    fn test_tool_result_text_extraction() {
        let result = ToolCallResult {
            content: vec![
                ContentBlock {
                    content_type: "text".to_string(),
                    text: Some("Hello".to_string()),
                    data: None,
                    mime_type: None,
                },
                ContentBlock {
                    content_type: "text".to_string(),
                    text: Some("World".to_string()),
                    data: None,
                    mime_type: None,
                },
            ],
            is_error: false,
        };
        assert_eq!(result.text(), "Hello\nWorld");
    }
}
```

### Step 2: Create mod.rs

Create `src/mcp/mod.rs`:

```rust
//! MCP (Model Context Protocol) Client Module
//!
//! Provides client functionality for connecting to MCP servers.
//!
//! # Architecture
//!
//! Nika spawns MCP servers as child processes and communicates via stdio.
//! Each MCP server can provide tools and resources that workflows can use.
//!
//! # Example
//!
//! ```yaml
//! mcp:
//!   novanet:
//!     command: cargo
//!     args: [run, -p, novanet-mcp]
//! ```

mod types;
mod client;

pub use types::*;
pub use client::*;
```

### Step 3: Add mcp module to lib.rs

Modify `src/lib.rs` to add:

```rust
pub mod mcp;
```

### Step 4: Run cargo check

Run: `cd tools/nika && cargo check`
Expected: Compiles (client module will be empty stub for now)

### Step 5: Commit

```bash
git add src/mcp/
git commit -m "feat(mcp): add MCP types module

- McpConfig for server configuration
- ToolCallRequest/ToolCallResult for tool calls
- ContentBlock and ResourceContent types

Co-Authored-By: Claude <noreply@anthropic.com>"
```

---

## Task 2: Create MCP Client

**Files:**
- Create: `src/mcp/client.rs`
- Create: `tests/mcp_client_test.rs`

### Step 1: Write failing test

Create `tests/mcp_client_test.rs`:

```rust
//! MCP Client Tests

use nika::mcp::{McpClient, McpConfig};
use std::collections::HashMap;

#[test]
fn test_mcp_client_creation() {
    let config = McpConfig {
        name: "test".to_string(),
        command: "echo".to_string(),
        args: vec!["hello".to_string()],
        env: HashMap::new(),
        cwd: None,
    };

    let client = McpClient::new(config);
    assert!(client.is_ok());
    assert!(!client.unwrap().is_connected());
}

#[test]
fn test_mcp_client_config_validation() {
    let config = McpConfig {
        name: "".to_string(), // Invalid: empty name
        command: "echo".to_string(),
        args: vec![],
        env: HashMap::new(),
        cwd: None,
    };

    let client = McpClient::new(config);
    assert!(client.is_err());
}

#[tokio::test]
async fn test_mcp_client_mock_tool_call() {
    let client = McpClient::mock("novanet");

    // Mock should return predefined response
    let result = client.call_tool("novanet_describe", serde_json::json!({
        "target": "schema"
    })).await;

    assert!(result.is_ok());
    let response = result.unwrap();
    assert!(!response.is_error());
}
```

### Step 2: Run test to verify it fails

Run: `cd tools/nika && cargo test --test mcp_client_test`
Expected: FAIL with "cannot find value `McpClient`"

### Step 3: Implement MCP Client

Create `src/mcp/client.rs`:

```rust
//! MCP Client Implementation
//!
//! Manages connection to MCP servers via child process stdio.

use crate::error::{NikaError, Result};
use crate::mcp::types::*;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;

/// MCP Client for communicating with MCP servers
pub struct McpClient {
    config: McpConfig,
    connected: Arc<RwLock<bool>>,
    /// Cached tool definitions
    tools: Arc<RwLock<Vec<ToolDefinition>>>,
    /// Mock mode for testing
    mock_mode: bool,
}

impl McpClient {
    /// Create a new MCP client with the given configuration
    pub fn new(config: McpConfig) -> Result<Self> {
        // Validate config
        if config.name.is_empty() {
            return Err(NikaError::ValidationError {
                reason: "MCP server name cannot be empty".to_string(),
            });
        }
        if config.command.is_empty() {
            return Err(NikaError::ValidationError {
                reason: "MCP server command cannot be empty".to_string(),
            });
        }

        Ok(Self {
            config,
            connected: Arc::new(RwLock::new(false)),
            tools: Arc::new(RwLock::new(Vec::new())),
            mock_mode: false,
        })
    }

    /// Create a mock client for testing
    pub fn mock(name: &str) -> Self {
        Self {
            config: McpConfig {
                name: name.to_string(),
                command: "mock".to_string(),
                args: vec![],
                env: HashMap::new(),
                cwd: None,
            },
            connected: Arc::new(RwLock::new(true)),
            tools: Arc::new(RwLock::new(vec![
                ToolDefinition {
                    name: "novanet_describe".to_string(),
                    description: Some("Describe NovaNet schema".to_string()),
                    input_schema: serde_json::json!({"type": "object"}),
                },
                ToolDefinition {
                    name: "novanet_generate".to_string(),
                    description: Some("Generate context".to_string()),
                    input_schema: serde_json::json!({"type": "object"}),
                },
            ])),
            mock_mode: true,
        }
    }

    /// Check if client is connected
    pub fn is_connected(&self) -> bool {
        // For sync check, we use try_read
        self.connected.try_read().map(|g| *g).unwrap_or(false)
    }

    /// Get the server name
    pub fn name(&self) -> &str {
        &self.config.name
    }

    /// Connect to the MCP server
    ///
    /// Spawns the server as a child process and initializes the connection.
    pub async fn connect(&self) -> Result<()> {
        if self.mock_mode {
            return Ok(());
        }

        // TODO: Implement real MCP connection using rmcp
        // For now, mark as connected for testing
        let mut connected = self.connected.write().await;
        *connected = true;

        tracing::info!(
            server = %self.config.name,
            command = %self.config.command,
            "MCP server connected (stub)"
        );

        Ok(())
    }

    /// Disconnect from the MCP server
    pub async fn disconnect(&self) -> Result<()> {
        let mut connected = self.connected.write().await;
        *connected = false;
        Ok(())
    }

    /// Call a tool on the MCP server
    pub async fn call_tool(&self, name: &str, params: serde_json::Value) -> Result<ToolCallResult> {
        if !*self.connected.read().await {
            return Err(NikaError::McpNotConnected {
                name: self.config.name.clone(),
            });
        }

        if self.mock_mode {
            return Ok(self.mock_tool_response(name, &params));
        }

        // TODO: Implement real MCP tool call using rmcp
        tracing::debug!(
            server = %self.config.name,
            tool = %name,
            "Calling MCP tool (stub)"
        );

        Err(NikaError::McpToolError {
            tool: name.to_string(),
            reason: "Real MCP connection not yet implemented".to_string(),
        })
    }

    /// Read a resource from the MCP server
    pub async fn read_resource(&self, uri: &str) -> Result<ResourceContent> {
        if !*self.connected.read().await {
            return Err(NikaError::McpNotConnected {
                name: self.config.name.clone(),
            });
        }

        if self.mock_mode {
            return Ok(ResourceContent {
                uri: uri.to_string(),
                mime_type: Some("application/json".to_string()),
                text: Some(r#"{"mock": true}"#.to_string()),
            });
        }

        // TODO: Implement real MCP resource read
        Err(NikaError::McpResourceNotFound {
            uri: uri.to_string(),
        })
    }

    /// List available tools from the MCP server
    pub async fn list_tools(&self) -> Result<Vec<ToolDefinition>> {
        Ok(self.tools.read().await.clone())
    }

    /// Generate mock response for testing
    fn mock_tool_response(&self, name: &str, params: &serde_json::Value) -> ToolCallResult {
        match name {
            "novanet_describe" => ToolCallResult {
                content: vec![ContentBlock {
                    content_type: "text".to_string(),
                    text: Some(r#"{"nodes": 62, "arcs": 182}"#.to_string()),
                    data: None,
                    mime_type: None,
                }],
                is_error: false,
            },
            "novanet_generate" => ToolCallResult {
                content: vec![ContentBlock {
                    content_type: "text".to_string(),
                    text: Some(serde_json::json!({
                        "entity": {
                            "key": params.get("entity").and_then(|v| v.as_str()).unwrap_or("unknown"),
                            "denomination_forms": [
                                {"type": "text", "value": "qr code", "priority": 1},
                                {"type": "title", "value": "QR Code", "priority": 1}
                            ]
                        },
                        "token_count": 850
                    }).to_string()),
                    data: None,
                    mime_type: None,
                }],
                is_error: false,
            },
            _ => ToolCallResult {
                content: vec![ContentBlock {
                    content_type: "text".to_string(),
                    text: Some(format!("Mock response for tool: {}", name)),
                    data: None,
                    mime_type: None,
                }],
                is_error: false,
            },
        }
    }
}

impl std::fmt::Debug for McpClient {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("McpClient")
            .field("name", &self.config.name)
            .field("connected", &self.is_connected())
            .field("mock_mode", &self.mock_mode)
            .finish()
    }
}
```

### Step 4: Run tests

Run: `cd tools/nika && cargo test --test mcp_client_test`
Expected: All tests pass

### Step 5: Commit

```bash
git add src/mcp/client.rs tests/mcp_client_test.rs
git commit -m "feat(mcp): add MCP client with mock support

- McpClient::new() for real connections
- McpClient::mock() for testing
- call_tool() and read_resource() methods
- Mock responses for novanet_describe/generate

Co-Authored-By: Claude <noreply@anthropic.com>"
```

---

## Task 3: Add Invoke Params to AST

**Files:**
- Create: `src/ast/invoke.rs`
- Modify: `src/ast/mod.rs`
- Create: `tests/invoke_parse_test.rs`

### Step 1: Write failing test

Create `tests/invoke_parse_test.rs`:

```rust
//! Invoke verb parsing tests

use nika::ast::InvokeParams;

#[test]
fn test_invoke_params_tool_call() {
    let yaml = r#"
mcp: novanet
tool: novanet_generate
params:
  mode: block
  entity: qr-code
  locale: fr-FR
"#;

    let params: InvokeParams = serde_yaml::from_str(yaml).unwrap();
    assert_eq!(params.mcp, "novanet");
    assert_eq!(params.tool, Some("novanet_generate".to_string()));
    assert!(params.resource.is_none());

    let p = params.params.unwrap();
    assert_eq!(p["mode"], "block");
    assert_eq!(p["entity"], "qr-code");
}

#[test]
fn test_invoke_params_resource_read() {
    let yaml = r#"
mcp: novanet
resource: "entity://qr-code-generator"
"#;

    let params: InvokeParams = serde_yaml::from_str(yaml).unwrap();
    assert_eq!(params.mcp, "novanet");
    assert!(params.tool.is_none());
    assert_eq!(params.resource, Some("entity://qr-code-generator".to_string()));
}

#[test]
fn test_invoke_params_validation_both_tool_and_resource() {
    let params = InvokeParams {
        mcp: "novanet".to_string(),
        tool: Some("novanet_describe".to_string()),
        params: None,
        resource: Some("entity://test".to_string()),
    };

    let result = params.validate();
    assert!(result.is_err());
    assert!(result.unwrap_err().contains("Cannot specify both"));
}

#[test]
fn test_invoke_params_validation_neither() {
    let params = InvokeParams {
        mcp: "novanet".to_string(),
        tool: None,
        params: None,
        resource: None,
    };

    let result = params.validate();
    assert!(result.is_err());
    assert!(result.unwrap_err().contains("Must specify either"));
}
```

### Step 2: Run test to verify it fails

Run: `cd tools/nika && cargo test --test invoke_parse_test`
Expected: FAIL with "cannot find type `InvokeParams`"

### Step 3: Create InvokeParams

Create `src/ast/invoke.rs`:

```rust
//! Invoke Action Parameters
//!
//! The `invoke:` verb allows workflows to call MCP tools or read MCP resources.
//!
//! # Example - Tool Call
//!
//! ```yaml
//! - id: get_context
//!   invoke:
//!     mcp: novanet
//!     tool: novanet_generate
//!     params:
//!       mode: block
//!       entity: qr-code
//!       locale: fr-FR
//! ```
//!
//! # Example - Resource Read
//!
//! ```yaml
//! - id: get_entity
//!   invoke:
//!     mcp: novanet
//!     resource: "entity://qr-code-generator"
//! ```

use serde::Deserialize;

/// Parameters for the `invoke:` verb
#[derive(Debug, Clone, Deserialize)]
pub struct InvokeParams {
    /// MCP server name (must be configured in workflow `mcp:` section)
    pub mcp: String,

    /// Tool name to call (e.g., "novanet_generate")
    /// Mutually exclusive with `resource`
    pub tool: Option<String>,

    /// Tool parameters as JSON object
    #[serde(default)]
    pub params: Option<serde_json::Value>,

    /// Resource URI to read (e.g., "entity://qr-code")
    /// Mutually exclusive with `tool`
    pub resource: Option<String>,
}

impl InvokeParams {
    /// Validate that exactly one of `tool` or `resource` is specified
    pub fn validate(&self) -> Result<(), String> {
        match (&self.tool, &self.resource) {
            (Some(_), Some(_)) => {
                Err("Cannot specify both 'tool' and 'resource' in invoke".to_string())
            }
            (None, None) => {
                Err("Must specify either 'tool' or 'resource' in invoke".to_string())
            }
            _ => Ok(()),
        }
    }

    /// Check if this is a tool call
    pub fn is_tool_call(&self) -> bool {
        self.tool.is_some()
    }

    /// Check if this is a resource read
    pub fn is_resource_read(&self) -> bool {
        self.resource.is_some()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_invoke_params_default_params() {
        let yaml = r#"
mcp: test
tool: test_tool
"#;
        let params: InvokeParams = serde_yaml::from_str(yaml).unwrap();
        assert!(params.params.is_none());
    }

    #[test]
    fn test_invoke_params_is_tool_call() {
        let params = InvokeParams {
            mcp: "test".to_string(),
            tool: Some("test_tool".to_string()),
            params: None,
            resource: None,
        };
        assert!(params.is_tool_call());
        assert!(!params.is_resource_read());
    }
}
```

### Step 4: Export from mod.rs

Modify `src/ast/mod.rs` to add:

```rust
mod invoke;
pub use invoke::InvokeParams;
```

### Step 5: Run tests

Run: `cd tools/nika && cargo test --test invoke_parse_test`
Expected: All tests pass

### Step 6: Commit

```bash
git add src/ast/invoke.rs tests/invoke_parse_test.rs
git commit -m "feat(ast): add InvokeParams for invoke: verb

- MCP server reference
- Tool call with params
- Resource read support
- Validation for mutual exclusivity

Co-Authored-By: Claude <noreply@anthropic.com>"
```

---

## Task 4: Add Invoke Variant to TaskAction

**Files:**
- Modify: `src/ast/action.rs`
- Create: `tests/task_action_test.rs`

### Step 1: Write failing test

Create `tests/task_action_test.rs`:

```rust
//! TaskAction parsing tests including invoke

use nika::ast::TaskAction;

#[test]
fn test_task_action_invoke_variant() {
    let yaml = r#"
invoke:
  mcp: novanet
  tool: novanet_generate
  params:
    mode: block
"#;

    let action: TaskAction = serde_yaml::from_str(yaml).unwrap();

    match action {
        TaskAction::Invoke { invoke } => {
            assert_eq!(invoke.mcp, "novanet");
            assert_eq!(invoke.tool, Some("novanet_generate".to_string()));
        }
        _ => panic!("Expected Invoke variant, got {:?}", action),
    }
}

#[test]
fn test_task_action_infer_still_works() {
    let yaml = r#"
infer:
  prompt: "Say hello"
"#;

    let action: TaskAction = serde_yaml::from_str(yaml).unwrap();

    match action {
        TaskAction::Infer { infer } => {
            assert_eq!(infer.prompt, "Say hello");
        }
        _ => panic!("Expected Infer variant"),
    }
}

#[test]
fn test_task_action_all_variants() {
    // Infer
    let infer: TaskAction = serde_yaml::from_str(r#"
infer:
  prompt: test
"#).unwrap();
    assert!(matches!(infer, TaskAction::Infer { .. }));

    // Exec
    let exec: TaskAction = serde_yaml::from_str(r#"
exec:
  command: echo hello
"#).unwrap();
    assert!(matches!(exec, TaskAction::Exec { .. }));

    // Fetch
    let fetch: TaskAction = serde_yaml::from_str(r#"
fetch:
  url: https://example.com
"#).unwrap();
    assert!(matches!(fetch, TaskAction::Fetch { .. }));

    // Invoke
    let invoke: TaskAction = serde_yaml::from_str(r#"
invoke:
  mcp: test
  tool: test_tool
"#).unwrap();
    assert!(matches!(invoke, TaskAction::Invoke { .. }));
}
```

### Step 2: Run test to verify it fails

Run: `cd tools/nika && cargo test --test task_action_test`
Expected: FAIL with "unknown variant `invoke`"

### Step 3: Add Invoke variant to TaskAction

Modify `src/ast/action.rs`:

```rust
//! Task Action Types - the 4 action verbs (v0.2)
//!
//! Defines the task action variants:
//! - `InferParams`: One-shot LLM call
//! - `ExecParams`: Shell command execution
//! - `FetchParams`: HTTP request
//! - `InvokeParams`: MCP tool/resource call (NEW v0.2)

use rustc_hash::FxHashMap;
use serde::Deserialize;

use crate::ast::InvokeParams;

/// Infer action - one-shot LLM call
#[derive(Debug, Clone, Deserialize)]
pub struct InferParams {
    pub prompt: String,
    /// Override provider for this task
    #[serde(default)]
    pub provider: Option<String>,
    /// Override model for this task
    #[serde(default)]
    pub model: Option<String>,
}

/// Exec action - shell command
#[derive(Debug, Clone, Deserialize)]
pub struct ExecParams {
    pub command: String,
}

/// Fetch action - HTTP request
#[derive(Debug, Clone, Deserialize)]
pub struct FetchParams {
    pub url: String,
    #[serde(default = "default_method")]
    pub method: String,
    #[serde(default)]
    pub headers: FxHashMap<String, String>,
    pub body: Option<String>,
}

fn default_method() -> String {
    "GET".to_string()
}

/// The 4 task action types (v0.2)
#[derive(Debug, Clone, Deserialize)]
#[serde(untagged)]
pub enum TaskAction {
    Infer { infer: InferParams },
    Exec { exec: ExecParams },
    Fetch { fetch: FetchParams },
    Invoke { invoke: InvokeParams },
}
```

### Step 4: Run tests

Run: `cd tools/nika && cargo test --test task_action_test`
Expected: All tests pass

### Step 5: Commit

```bash
git add src/ast/action.rs tests/task_action_test.rs
git commit -m "feat(ast): add Invoke variant to TaskAction

- TaskAction now has 4 variants (infer, exec, fetch, invoke)
- Backward compatible with v0.1 workflows

Co-Authored-By: Claude <noreply@anthropic.com>"
```

---

## Task 5: Add MCP Config to Workflow

**Files:**
- Modify: `src/ast/workflow.rs`
- Create: `tests/workflow_mcp_test.rs`

### Step 1: Write failing test

Create `tests/workflow_mcp_test.rs`:

```rust
//! Workflow MCP configuration tests

use nika::ast::Workflow;

#[test]
fn test_workflow_with_mcp_config() {
    let yaml = r#"
schema: "nika/workflow@0.2"
provider: claude

mcp:
  novanet:
    command: cargo
    args:
      - run
      - -p
      - novanet-mcp
    env:
      NEO4J_URI: bolt://localhost:7687

tasks:
  - id: get_context
    invoke:
      mcp: novanet
      tool: novanet_generate
      params:
        mode: block
"#;

    let workflow: Workflow = serde_yaml::from_str(yaml).unwrap();

    assert_eq!(workflow.schema, "nika/workflow@0.2");
    assert!(workflow.mcp.is_some());

    let mcp = workflow.mcp.as_ref().unwrap();
    assert!(mcp.contains_key("novanet"));

    let novanet = mcp.get("novanet").unwrap();
    assert_eq!(novanet.command, "cargo");
    assert_eq!(novanet.args.len(), 3);
}

#[test]
fn test_workflow_without_mcp_config_v01() {
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
    assert!(workflow.mcp.is_none());
}

#[test]
fn test_workflow_mcp_config_minimal() {
    let yaml = r#"
schema: "nika/workflow@0.2"
provider: claude

mcp:
  echo:
    command: echo

tasks:
  - id: test
    invoke:
      mcp: echo
      tool: test
"#;

    let workflow: Workflow = serde_yaml::from_str(yaml).unwrap();

    let mcp = workflow.mcp.as_ref().unwrap();
    let echo = mcp.get("echo").unwrap();
    assert_eq!(echo.command, "echo");
    assert!(echo.args.is_empty());
    assert!(echo.env.is_empty());
}
```

### Step 2: Run test to verify it fails

Run: `cd tools/nika && cargo test --test workflow_mcp_test`
Expected: FAIL (mcp field not recognized)

### Step 3: Read current workflow.rs

Run: `head -60 tools/nika/src/ast/workflow.rs`

### Step 4: Add MCP config to Workflow

The Workflow struct needs to add:

```rust
use crate::mcp::McpConfig;
use std::collections::HashMap;

// In Workflow struct, add:
/// MCP server configurations (new in v0.2)
/// Key is the server name used in invoke: blocks
#[serde(default)]
pub mcp: Option<HashMap<String, McpConfigInline>>,
```

Also add inline config struct:

```rust
/// Inline MCP config (without name field, as name is the map key)
#[derive(Debug, Clone, Deserialize)]
pub struct McpConfigInline {
    pub command: String,
    #[serde(default)]
    pub args: Vec<String>,
    #[serde(default)]
    pub env: HashMap<String, String>,
    pub cwd: Option<String>,
}
```

### Step 5: Run tests

Run: `cd tools/nika && cargo test --test workflow_mcp_test`
Expected: All tests pass

### Step 6: Commit

```bash
git add src/ast/workflow.rs tests/workflow_mcp_test.rs
git commit -m "feat(ast): add MCP configuration to Workflow

- Optional mcp: block for v0.2 workflows
- McpConfigInline for inline server config
- Backward compatible with v0.1 (mcp is optional)

Co-Authored-By: Claude <noreply@anthropic.com>"
```

---

## Task 6: Implement Invoke Execution

**Files:**
- Modify: `src/runtime/executor.rs`
- Create: `tests/invoke_execution_test.rs`

### Step 1: Write failing test

Create `tests/invoke_execution_test.rs`:

```rust
//! Invoke execution tests

use nika::ast::{TaskAction, InvokeParams};
use nika::mcp::McpClient;
use std::sync::Arc;
use std::collections::HashMap;

#[tokio::test]
async fn test_invoke_execution_tool_call() {
    // Setup mock MCP client
    let client = Arc::new(McpClient::mock("novanet"));

    let invoke = InvokeParams {
        mcp: "novanet".to_string(),
        tool: Some("novanet_generate".to_string()),
        params: Some(serde_json::json!({
            "mode": "block",
            "entity": "qr-code"
        })),
        resource: None,
    };

    // Execute invoke
    let result = execute_invoke(&invoke, &client).await;

    assert!(result.is_ok());
    let value = result.unwrap();
    assert!(value.is_string() || value.is_object());
}

#[tokio::test]
async fn test_invoke_execution_resource_read() {
    let client = Arc::new(McpClient::mock("novanet"));

    let invoke = InvokeParams {
        mcp: "novanet".to_string(),
        tool: None,
        params: None,
        resource: Some("entity://qr-code".to_string()),
    };

    let result = execute_invoke(&invoke, &client).await;

    assert!(result.is_ok());
}

// Helper function to be implemented
async fn execute_invoke(
    invoke: &InvokeParams,
    client: &McpClient,
) -> Result<serde_json::Value, nika::error::NikaError> {
    invoke.validate().map_err(|e| nika::error::NikaError::ValidationError { reason: e })?;

    if let Some(tool) = &invoke.tool {
        let params = invoke.params.clone().unwrap_or(serde_json::Value::Null);
        let result = client.call_tool(tool, params).await?;

        // Try to parse as JSON, fallback to string
        let text = result.text();
        match serde_json::from_str(&text) {
            Ok(v) => Ok(v),
            Err(_) => Ok(serde_json::Value::String(text)),
        }
    } else if let Some(resource) = &invoke.resource {
        let content = client.read_resource(resource).await?;
        match content.text {
            Some(text) => match serde_json::from_str(&text) {
                Ok(v) => Ok(v),
                Err(_) => Ok(serde_json::Value::String(text)),
            },
            None => Ok(serde_json::Value::Null),
        }
    } else {
        unreachable!("validate() ensures tool or resource is set")
    }
}
```

### Step 2: Add invoke execution to executor

In `src/runtime/executor.rs`, add handling for `TaskAction::Invoke`:

```rust
TaskAction::Invoke { invoke } => {
    invoke.validate().map_err(|e| NikaError::ValidationError { reason: e })?;

    let mcp_client = self.get_mcp_client(&invoke.mcp).await?;

    let result = if let Some(tool) = &invoke.tool {
        let params = invoke.params.clone().unwrap_or(serde_json::Value::Null);

        // Emit event
        self.event_log.emit(EventKind::McpToolCalled {
            task_id: task_id.clone().into(),
            tool: tool.clone(),
            params: params.clone(),
        });

        let start = std::time::Instant::now();
        let tool_result = mcp_client.call_tool(tool, params).await?;
        let duration_ms = start.elapsed().as_millis() as u64;

        // Emit response event
        self.event_log.emit(EventKind::McpToolResponded {
            task_id: task_id.clone().into(),
            tool: tool.clone(),
            duration_ms,
            is_error: tool_result.is_error(),
        });

        let text = tool_result.text();
        serde_json::from_str(&text).unwrap_or(serde_json::Value::String(text))
    } else if let Some(resource) = &invoke.resource {
        let content = mcp_client.read_resource(resource).await?;
        content.text
            .and_then(|t| serde_json::from_str(&t).ok())
            .unwrap_or(serde_json::Value::Null)
    } else {
        unreachable!()
    };

    Ok(result)
}
```

### Step 3: Run tests

Run: `cd tools/nika && cargo test invoke`
Expected: All invoke tests pass

### Step 4: Commit

```bash
git add src/runtime/executor.rs tests/invoke_execution_test.rs
git commit -m "feat(runtime): implement invoke: verb execution

- Tool call with MCP client
- Resource read support
- Event emission for observability

Co-Authored-By: Claude <noreply@anthropic.com>"
```

---

## Task 7: Add MCP Events to EventLog

**Files:**
- Modify: `src/event/log.rs`

### Step 1: Add new event variants

Add to `EventKind` enum in `src/event/log.rs`:

```rust
// ═══════════════════════════════════════════
// MCP EVENTS (NEW v0.2)
// ═══════════════════════════════════════════
McpToolCalled {
    task_id: Arc<str>,
    tool: String,
    params: serde_json::Value,
},
McpToolResponded {
    task_id: Arc<str>,
    tool: String,
    duration_ms: u64,
    is_error: bool,
},
McpResourceRead {
    task_id: Arc<str>,
    uri: String,
},
```

### Step 2: Update task_id() method

```rust
pub fn task_id(&self) -> Option<&str> {
    match self {
        // ... existing matches ...
        Self::McpToolCalled { task_id, .. }
        | Self::McpToolResponded { task_id, .. }
        | Self::McpResourceRead { task_id, .. } => Some(task_id),
        // ...
    }
}
```

### Step 3: Add tests

```rust
#[test]
fn test_mcp_event_serialization() {
    let event = EventKind::McpToolCalled {
        task_id: "ctx".into(),
        tool: "novanet_generate".to_string(),
        params: json!({"mode": "block"}),
    };

    let json = serde_json::to_value(&event).unwrap();
    assert_eq!(json["type"], "mcp_tool_called");
    assert_eq!(json["tool"], "novanet_generate");
}
```

### Step 4: Run tests

Run: `cd tools/nika && cargo test event`
Expected: All event tests pass

### Step 5: Commit

```bash
git add src/event/log.rs
git commit -m "feat(event): add MCP event variants

- McpToolCalled: when invoke calls a tool
- McpToolResponded: when tool returns result
- McpResourceRead: when resource is read

Co-Authored-By: Claude <noreply@anthropic.com>"
```

---

## Task 8: Create Example Workflow

**Files:**
- Create: `examples/invoke-novanet.yaml`

### Step 1: Create example workflow

```yaml
# Example: Invoke NovaNet MCP tools
#
# This workflow demonstrates the invoke: verb for MCP integration.
# Run with: cargo run -- run examples/invoke-novanet.yaml

schema: "nika/workflow@0.2"
provider: claude

mcp:
  novanet:
    command: cargo
    args:
      - run
      - --manifest-path
      - ../../novanet-dev/tools/novanet-mcp/Cargo.toml
    env:
      RUST_LOG: info

tasks:
  # Step 1: Discover page structure
  - id: discover
    invoke:
      mcp: novanet
      tool: novanet_describe
      params:
        target: page
        filters:
          key: homepage

  # Step 2: Get generation context for hero block
  - id: hero_context
    use:
      page: discover
    invoke:
      mcp: novanet
      tool: novanet_generate
      params:
        mode: block
        page_key: "{{use.page.key}}"
        block_key: hero
        locale: fr-FR
        token_budget: 8000
    output:
      format: json

  # Step 3: Generate content using context
  - id: generate_hero
    use:
      ctx: hero_context
    infer:
      prompt: |
        Generate native French content for the hero block.

        CONTEXT:
        {{use.ctx}}

        RULES:
        - Use ONLY denomination_forms values from context
        - NO invention, NO paraphrase of entity names
        - Follow @ references for brand voice and style

        Output JSON matching the block schema.
    output:
      format: json

flows:
  - source: discover
    target: hero_context
  - source: hero_context
    target: generate_hero
```

### Step 2: Commit

```bash
git add examples/invoke-novanet.yaml
git commit -m "docs: add invoke-novanet example workflow

Demonstrates:
- MCP server configuration
- invoke: verb for tool calls
- Data flow between invoke and infer tasks

Co-Authored-By: Claude <noreply@anthropic.com>"
```

---

## Task 9: Integration Test

**Files:**
- Create: `tests/integration/invoke_workflow.rs`

### Step 1: Create integration test

```rust
//! Integration test for invoke workflow

use std::path::Path;

#[test]
fn test_invoke_workflow_parses() {
    let yaml = std::fs::read_to_string("examples/invoke-novanet.yaml")
        .expect("Example workflow should exist");

    let workflow: nika::ast::Workflow = serde_yaml::from_str(&yaml)
        .expect("Workflow should parse");

    assert_eq!(workflow.schema, "nika/workflow@0.2");
    assert_eq!(workflow.tasks.len(), 3);

    // Verify MCP config
    let mcp = workflow.mcp.expect("Should have MCP config");
    assert!(mcp.contains_key("novanet"));

    // Verify task types
    assert!(matches!(
        workflow.tasks[0].action,
        nika::ast::TaskAction::Invoke { .. }
    ));
    assert!(matches!(
        workflow.tasks[1].action,
        nika::ast::TaskAction::Invoke { .. }
    ));
    assert!(matches!(
        workflow.tasks[2].action,
        nika::ast::TaskAction::Infer { .. }
    ));
}

#[tokio::test]
#[ignore] // Requires running NovaNet MCP server
async fn test_invoke_workflow_executes() {
    // This test requires:
    // 1. NovaNet MCP server running
    // 2. Neo4j with test data

    let result = nika::run(Path::new("examples/invoke-novanet.yaml")).await;
    assert!(result.is_ok());
}
```

### Step 2: Run integration test

Run: `cd tools/nika && cargo test --test integration`
Expected: Parse test passes, execution test is ignored

### Step 3: Commit

```bash
git add tests/integration/
git commit -m "test: add invoke workflow integration tests

- Parse test validates example workflow structure
- Execution test (ignored) for full E2E testing

Co-Authored-By: Claude <noreply@anthropic.com>"
```

---

## Summary

After completing MVP 1, Nika will have:

1. **MCP Types Module** - Configuration and response types
2. **MCP Client** - Connect to MCP servers, call tools, read resources
3. **InvokeParams** - AST representation of invoke: verb
4. **TaskAction::Invoke** - New action variant
5. **Workflow MCP Config** - Optional mcp: block in workflow YAML
6. **Invoke Execution** - Runtime support for invoke: verb
7. **MCP Events** - Event logging for observability
8. **Example Workflow** - Demonstrates invoke: with NovaNet

**Verify Success:**

```bash
# All tests pass
cargo test

# Example workflow parses
cargo run -- validate examples/invoke-novanet.yaml

# With running NovaNet MCP (optional)
cargo run -- run examples/invoke-novanet.yaml
```

**Next:** Proceed to MVP 2 (Agent Verb + Observability) plan.
