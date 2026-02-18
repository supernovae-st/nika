# MVP 4: Real Integration - Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Remove mock mode and validate Nika against real NovaNet MCP server with Neo4j.

**Architecture:** Integration testing infrastructure using cargo test features, Docker-based Neo4j, and real MCP stdio communication.

**Tech Stack:** Rust test framework, testcontainers-rs (Neo4j), tokio-test, real rmcp client

**Prerequisites:** MVP 3 completed (TUI + CLI trace working with mocks)

---

## Task 1: Integration Test Infrastructure

**Files:**
- Create: `nika-dev/tools/nika/tests/integration/mod.rs`
- Create: `nika-dev/tools/nika/tests/integration/helpers.rs`
- Modify: `nika-dev/tools/nika/Cargo.toml`

**Step 1: Add integration feature flag to Cargo.toml**

```toml
[features]
default = []
tui = ["ratatui", "crossterm"]
integration = ["testcontainers"]

[dev-dependencies]
testcontainers = "0.15"
```

**Step 2: Create test helper module**

```rust
// tests/integration/helpers.rs
use std::process::{Child, Command, Stdio};
use std::time::Duration;
use tokio::time::sleep;

pub struct NovaNetMcp {
    process: Child,
    pub port: u16,
}

impl NovaNetMcp {
    pub async fn start() -> Result<Self, Box<dyn std::error::Error>> {
        // Start NovaNet MCP server
        let process = Command::new("node")
            .args(["../../novanet-dev/tools/novanet-mcp/dist/index.js"])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()?;

        // Wait for server to be ready
        sleep(Duration::from_secs(2)).await;

        Ok(Self { process, port: 0 }) // stdio mode, no port
    }

    pub fn stdio(&mut self) -> (std::process::ChildStdin, std::process::ChildStdout) {
        (
            self.process.stdin.take().unwrap(),
            self.process.stdout.take().unwrap(),
        )
    }
}

impl Drop for NovaNetMcp {
    fn drop(&mut self) {
        let _ = self.process.kill();
    }
}

pub struct Neo4jContainer {
    pub bolt_url: String,
}

impl Neo4jContainer {
    pub async fn start() -> Result<Self, Box<dyn std::error::Error>> {
        // Using testcontainers
        // For now, assume Neo4j is running locally
        Ok(Self {
            bolt_url: "bolt://localhost:7687".to_string(),
        })
    }
}
```

**Step 3: Create integration test module**

```rust
// tests/integration/mod.rs
mod helpers;

pub use helpers::{NovaNetMcp, Neo4jContainer};
```

**Step 4: Run to verify structure compiles**

Run: `cargo test --features integration --no-run`
Expected: Compiles successfully

**Step 5: Commit**

```bash
git add tests/integration/ Cargo.toml
git commit -m "feat(test): add integration test infrastructure

- Add integration feature flag
- Create NovaNetMcp helper for real MCP server
- Create Neo4jContainer helper for testcontainers

Co-Authored-By: Claude <noreply@anthropic.com>"
```

---

## Task 2: Remove Mock Mode from McpClient

**Files:**
- Modify: `nika-dev/tools/nika/src/mcp/client.rs`
- Create: `nika-dev/tools/nika/src/mcp/mock.rs`

**Step 1: Extract mock to separate module**

```rust
// src/mcp/mock.rs
use super::types::{ToolCallResult, ContentBlock};
use serde_json::Value;

pub struct MockMcpClient {
    name: String,
    tools: Vec<String>,
}

impl MockMcpClient {
    pub fn new(name: &str) -> Self {
        Self {
            name: name.to_string(),
            tools: vec![
                "novanet_generate".to_string(),
                "novanet_describe".to_string(),
                "novanet_traverse".to_string(),
            ],
        }
    }

    pub async fn call_tool(&self, name: &str, params: Value) -> Result<ToolCallResult, crate::NikaError> {
        // Return mock responses based on tool name
        let content = match name {
            "novanet_generate" => {
                let entity = params.get("entity")
                    .and_then(|v| v.as_str())
                    .unwrap_or("unknown");
                serde_json::json!({
                    "entity": entity,
                    "generated": {
                        "title": format!("Mock Title for {}", entity),
                        "text": format!("Mock generated content for {} entity.", entity)
                    }
                })
            }
            "novanet_describe" => {
                serde_json::json!({
                    "description": "Mock entity description",
                    "properties": {}
                })
            }
            _ => serde_json::json!({"result": "mock"})
        };

        Ok(ToolCallResult {
            content: vec![ContentBlock::Text {
                text: serde_json::to_string_pretty(&content).unwrap()
            }],
            is_error: false,
        })
    }

    pub fn list_tools(&self) -> &[String] {
        &self.tools
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_mock_novanet_generate() {
        let mock = MockMcpClient::new("novanet");
        let result = mock.call_tool(
            "novanet_generate",
            serde_json::json!({"entity": "qr-code", "locale": "fr-FR"})
        ).await.unwrap();

        assert!(!result.is_error);
        assert!(!result.content.is_empty());
    }
}
```

**Step 2: Update McpClient to use trait-based abstraction**

```rust
// src/mcp/client.rs - updated
use crate::NikaError;
use super::types::{McpConfig, ToolCallResult, ToolDefinition};
use serde_json::Value;
use std::sync::Arc;
use parking_lot::RwLock;

#[async_trait::async_trait]
pub trait McpClientTrait: Send + Sync {
    async fn call_tool(&self, name: &str, params: Value) -> Result<ToolCallResult, NikaError>;
    fn list_tools(&self) -> Vec<ToolDefinition>;
    fn name(&self) -> &str;
}

pub struct McpClient {
    config: McpConfig,
    connected: Arc<RwLock<bool>>,
    tools: Arc<RwLock<Vec<ToolDefinition>>>,
    // Real rmcp client handle will go here
}

impl McpClient {
    pub fn new(config: McpConfig) -> Result<Self, NikaError> {
        Ok(Self {
            config,
            connected: Arc::new(RwLock::new(false)),
            tools: Arc::new(RwLock::new(Vec::new())),
        })
    }

    pub async fn connect(&self) -> Result<(), NikaError> {
        // Real connection logic using rmcp
        // Spawn child process, establish stdio communication
        *self.connected.write() = true;
        Ok(())
    }

    pub fn is_connected(&self) -> bool {
        *self.connected.read()
    }
}

#[async_trait::async_trait]
impl McpClientTrait for McpClient {
    async fn call_tool(&self, name: &str, params: Value) -> Result<ToolCallResult, NikaError> {
        if !self.is_connected() {
            return Err(NikaError::McpNotConnected {
                name: self.config.name.clone()
            });
        }

        // Real MCP call using rmcp
        todo!("Implement real MCP call")
    }

    fn list_tools(&self) -> Vec<ToolDefinition> {
        self.tools.read().clone()
    }

    fn name(&self) -> &str {
        &self.config.name
    }
}
```

**Step 3: Run tests**

Run: `cargo test mcp`
Expected: PASS

**Step 4: Commit**

```bash
git add src/mcp/
git commit -m "refactor(mcp): extract mock to separate module

- Create McpClientTrait for abstraction
- Move mock logic to mcp/mock.rs
- Prepare McpClient for real rmcp integration

Co-Authored-By: Claude <noreply@anthropic.com>"
```

---

## Task 3: Implement Real MCP Connection with rmcp

**Files:**
- Modify: `nika-dev/tools/nika/src/mcp/client.rs`
- Modify: `nika-dev/tools/nika/Cargo.toml`

**Step 1: Write failing integration test**

```rust
// tests/integration/mcp_test.rs
use nika::mcp::{McpClient, McpConfig};

#[tokio::test]
#[cfg(feature = "integration")]
async fn test_real_novanet_connection() {
    let config = McpConfig {
        name: "novanet".to_string(),
        command: "node".to_string(),
        args: vec!["../../novanet-dev/tools/novanet-mcp/dist/index.js".to_string()],
        env: std::collections::HashMap::new(),
    };

    let client = McpClient::new(config).unwrap();
    client.connect().await.unwrap();

    assert!(client.is_connected());

    let tools = client.list_tools();
    assert!(tools.iter().any(|t| t.name == "novanet_generate"));
}
```

**Step 2: Run test to verify it fails**

Run: `cargo test --features integration test_real_novanet_connection`
Expected: FAIL (not implemented yet)

**Step 3: Implement real MCP connection**

```rust
// src/mcp/client.rs - full implementation
use crate::NikaError;
use super::types::{McpConfig, ToolCallResult, ToolDefinition, ContentBlock};
use serde_json::Value;
use std::sync::Arc;
use parking_lot::RwLock;
use tokio::process::{Child, Command};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use std::process::Stdio;

pub struct McpClient {
    config: McpConfig,
    process: Arc<RwLock<Option<Child>>>,
    tools: Arc<RwLock<Vec<ToolDefinition>>>,
    request_id: Arc<RwLock<u64>>,
}

impl McpClient {
    pub fn new(config: McpConfig) -> Result<Self, NikaError> {
        Ok(Self {
            config,
            process: Arc::new(RwLock::new(None)),
            tools: Arc::new(RwLock::new(Vec::new())),
            request_id: Arc::new(RwLock::new(0)),
        })
    }

    pub async fn connect(&self) -> Result<(), NikaError> {
        let mut cmd = Command::new(&self.config.command);
        cmd.args(&self.config.args);
        cmd.stdin(Stdio::piped());
        cmd.stdout(Stdio::piped());
        cmd.stderr(Stdio::piped());

        for (key, value) in &self.config.env {
            cmd.env(key, value);
        }

        let child = cmd.spawn().map_err(|e| NikaError::McpSpawnFailed {
            name: self.config.name.clone(),
            source: e.to_string(),
        })?;

        *self.process.write() = Some(child);

        // Send initialize request
        self.send_initialize().await?;

        // Fetch tools list
        self.fetch_tools().await?;

        Ok(())
    }

    async fn send_initialize(&self) -> Result<(), NikaError> {
        let request = serde_json::json!({
            "jsonrpc": "2.0",
            "id": self.next_id(),
            "method": "initialize",
            "params": {
                "protocolVersion": "2024-11-05",
                "capabilities": {},
                "clientInfo": {
                    "name": "nika",
                    "version": env!("CARGO_PKG_VERSION")
                }
            }
        });

        self.send_request(&request).await?;
        let _response = self.read_response().await?;

        // Send initialized notification
        let notification = serde_json::json!({
            "jsonrpc": "2.0",
            "method": "notifications/initialized"
        });
        self.send_request(&notification).await?;

        Ok(())
    }

    async fn fetch_tools(&self) -> Result<(), NikaError> {
        let request = serde_json::json!({
            "jsonrpc": "2.0",
            "id": self.next_id(),
            "method": "tools/list",
            "params": {}
        });

        self.send_request(&request).await?;
        let response = self.read_response().await?;

        if let Some(result) = response.get("result") {
            if let Some(tools) = result.get("tools").and_then(|t| t.as_array()) {
                let mut tool_defs = Vec::new();
                for tool in tools {
                    if let (Some(name), Some(description)) = (
                        tool.get("name").and_then(|n| n.as_str()),
                        tool.get("description").and_then(|d| d.as_str())
                    ) {
                        tool_defs.push(ToolDefinition {
                            name: name.to_string(),
                            description: description.to_string(),
                            input_schema: tool.get("inputSchema").cloned()
                                .unwrap_or(serde_json::json!({})),
                        });
                    }
                }
                *self.tools.write() = tool_defs;
            }
        }

        Ok(())
    }

    async fn send_request(&self, request: &Value) -> Result<(), NikaError> {
        let mut process = self.process.write();
        let child = process.as_mut().ok_or(NikaError::McpNotConnected {
            name: self.config.name.clone(),
        })?;

        let stdin = child.stdin.as_mut().ok_or(NikaError::McpNotConnected {
            name: self.config.name.clone(),
        })?;

        let json = serde_json::to_string(request).unwrap();
        stdin.write_all(json.as_bytes()).await.map_err(|e| {
            NikaError::McpCommunicationError {
                name: self.config.name.clone(),
                source: e.to_string(),
            }
        })?;
        stdin.write_all(b"\n").await.map_err(|e| {
            NikaError::McpCommunicationError {
                name: self.config.name.clone(),
                source: e.to_string(),
            }
        })?;
        stdin.flush().await.map_err(|e| {
            NikaError::McpCommunicationError {
                name: self.config.name.clone(),
                source: e.to_string(),
            }
        })?;

        Ok(())
    }

    async fn read_response(&self) -> Result<Value, NikaError> {
        let mut process = self.process.write();
        let child = process.as_mut().ok_or(NikaError::McpNotConnected {
            name: self.config.name.clone(),
        })?;

        let stdout = child.stdout.as_mut().ok_or(NikaError::McpNotConnected {
            name: self.config.name.clone(),
        })?;

        let mut reader = BufReader::new(stdout);
        let mut line = String::new();
        reader.read_line(&mut line).await.map_err(|e| {
            NikaError::McpCommunicationError {
                name: self.config.name.clone(),
                source: e.to_string(),
            }
        })?;

        serde_json::from_str(&line).map_err(|e| {
            NikaError::McpParseError {
                name: self.config.name.clone(),
                source: e.to_string(),
            }
        })
    }

    fn next_id(&self) -> u64 {
        let mut id = self.request_id.write();
        *id += 1;
        *id
    }

    pub fn is_connected(&self) -> bool {
        self.process.read().is_some()
    }

    pub fn list_tools(&self) -> Vec<ToolDefinition> {
        self.tools.read().clone()
    }

    pub async fn call_tool(&self, name: &str, params: Value) -> Result<ToolCallResult, NikaError> {
        let request = serde_json::json!({
            "jsonrpc": "2.0",
            "id": self.next_id(),
            "method": "tools/call",
            "params": {
                "name": name,
                "arguments": params
            }
        });

        self.send_request(&request).await?;
        let response = self.read_response().await?;

        if let Some(error) = response.get("error") {
            return Ok(ToolCallResult {
                content: vec![ContentBlock::Text {
                    text: error.to_string(),
                }],
                is_error: true,
            });
        }

        let result = response.get("result").cloned().unwrap_or(serde_json::json!({}));
        let content = if let Some(content_array) = result.get("content").and_then(|c| c.as_array()) {
            content_array.iter().map(|c| {
                if let Some(text) = c.get("text").and_then(|t| t.as_str()) {
                    ContentBlock::Text { text: text.to_string() }
                } else {
                    ContentBlock::Text { text: c.to_string() }
                }
            }).collect()
        } else {
            vec![ContentBlock::Text { text: result.to_string() }]
        };

        Ok(ToolCallResult {
            content,
            is_error: result.get("isError").and_then(|e| e.as_bool()).unwrap_or(false),
        })
    }
}

impl Drop for McpClient {
    fn drop(&mut self) {
        if let Some(mut child) = self.process.write().take() {
            let _ = child.start_kill();
        }
    }
}
```

**Step 4: Add missing error variants**

```rust
// Add to src/error.rs
#[error("[NIKA-101] MCP server '{name}' spawn failed: {source}")]
McpSpawnFailed { name: String, source: String },

#[error("[NIKA-102] MCP server '{name}' communication error: {source}")]
McpCommunicationError { name: String, source: String },

#[error("[NIKA-103] MCP server '{name}' parse error: {source}")]
McpParseError { name: String, source: String },
```

**Step 5: Run integration test**

Run: `cargo test --features integration test_real_novanet_connection`
Expected: PASS (with NovaNet MCP server available)

**Step 6: Commit**

```bash
git add src/mcp/ src/error.rs tests/integration/
git commit -m "feat(mcp): implement real MCP connection with stdio

- Full JSON-RPC 2.0 implementation
- Initialize, fetch tools, call tools
- Proper error handling with codes
- Drop impl kills child process

Co-Authored-By: Claude <noreply@anthropic.com>"
```

---

## Task 4: Test invoke: with Real NovaNet MCP

**Files:**
- Create: `nika-dev/tools/nika/tests/integration/invoke_test.rs`

**Step 1: Write integration test for invoke verb**

```rust
// tests/integration/invoke_test.rs
use nika::Workflow;
use std::path::PathBuf;

#[tokio::test]
#[cfg(feature = "integration")]
async fn test_invoke_novanet_generate() {
    // This test requires:
    // 1. NovaNet MCP server built and available
    // 2. Neo4j running with seed data

    let workflow_yaml = r#"
name: test-invoke
version: "1.0"

mcp:
  novanet:
    command: node
    args:
      - ../../novanet-dev/tools/novanet-mcp/dist/index.js

tasks:
  generate:
    invoke: novanet_generate
    params:
      entity: "qr-code"
      locale: "en-US"
      forms:
        - text
        - title
"#;

    let workflow: Workflow = serde_yaml::from_str(workflow_yaml).unwrap();

    // Run workflow
    let result = nika::run_workflow(&workflow).await;

    assert!(result.is_ok(), "Workflow should succeed: {:?}", result.err());

    let output = result.unwrap();
    assert!(output.tasks.contains_key("generate"));

    let task_output = &output.tasks["generate"];
    assert!(task_output.success);

    // Verify generated content structure
    let content: serde_json::Value = serde_json::from_str(&task_output.output).unwrap();
    assert!(content.get("generated").is_some());
}

#[tokio::test]
#[cfg(feature = "integration")]
async fn test_invoke_novanet_describe() {
    let workflow_yaml = r#"
name: test-describe
version: "1.0"

mcp:
  novanet:
    command: node
    args:
      - ../../novanet-dev/tools/novanet-mcp/dist/index.js

tasks:
  describe:
    invoke: novanet_describe
    params:
      entity: "qr-code"
"#;

    let workflow: Workflow = serde_yaml::from_str(workflow_yaml).unwrap();
    let result = nika::run_workflow(&workflow).await;

    assert!(result.is_ok());
}
```

**Step 2: Run test**

Run: `cargo test --features integration test_invoke_novanet`
Expected: PASS

**Step 3: Commit**

```bash
git add tests/integration/invoke_test.rs
git commit -m "test(integration): add invoke verb tests with real NovaNet

- Test novanet_generate with entity/locale/forms
- Test novanet_describe
- Validates MCP connection and tool calling

Co-Authored-By: Claude <noreply@anthropic.com>"
```

---

## Task 5: Test agent: with Real NovaNet MCP

**Files:**
- Create: `nika-dev/tools/nika/tests/integration/agent_test.rs`

**Step 1: Write integration test for agent verb**

```rust
// tests/integration/agent_test.rs
use nika::Workflow;

#[tokio::test]
#[cfg(feature = "integration")]
async fn test_agent_with_novanet_tools() {
    // Requires ANTHROPIC_API_KEY
    if std::env::var("ANTHROPIC_API_KEY").is_err() {
        eprintln!("Skipping: ANTHROPIC_API_KEY not set");
        return;
    }

    let workflow_yaml = r#"
name: test-agent
version: "1.0"

mcp:
  novanet:
    command: node
    args:
      - ../../novanet-dev/tools/novanet-mcp/dist/index.js

tasks:
  research:
    agent: "Research the QR code entity and describe its properties"
    provider: claude
    model: claude-sonnet-4-20250514
    tools:
      - novanet_describe
      - novanet_traverse
    max_turns: 5
"#;

    let workflow: Workflow = serde_yaml::from_str(workflow_yaml).unwrap();
    let result = nika::run_workflow(&workflow).await;

    assert!(result.is_ok(), "Agent workflow should succeed: {:?}", result.err());

    let output = result.unwrap();
    let task_output = &output.tasks["research"];
    assert!(task_output.success);

    // Verify agent used tools
    assert!(task_output.tool_calls > 0, "Agent should have called tools");
}
```

**Step 2: Run test**

Run: `ANTHROPIC_API_KEY=sk-... cargo test --features integration test_agent_with_novanet`
Expected: PASS

**Step 3: Commit**

```bash
git add tests/integration/agent_test.rs
git commit -m "test(integration): add agent verb tests with real NovaNet

- Agent uses novanet_describe and novanet_traverse
- Validates multi-turn tool calling
- Requires ANTHROPIC_API_KEY

Co-Authored-By: Claude <noreply@anthropic.com>"
```

---

## Task 6: Validate denomination_forms (ADR-033)

**Files:**
- Create: `nika-dev/tools/nika/tests/integration/denomination_test.rs`
- Create: `nika-dev/tools/nika/src/validation/denomination.rs`

**Step 1: Create denomination validation module**

```rust
// src/validation/denomination.rs
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// ADR-033: Entity naming rules
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DenominationForms {
    /// Singular form (e.g., "QR Code")
    pub singular: String,
    /// Plural form (e.g., "QR Codes")
    pub plural: String,
    /// With article (e.g., "a QR Code", "the QR Code")
    #[serde(default)]
    pub with_article: Option<String>,
    /// Possessive form (e.g., "QR Code's")
    #[serde(default)]
    pub possessive: Option<String>,
}

impl DenominationForms {
    pub fn validate(&self) -> Result<(), DenominationError> {
        if self.singular.is_empty() {
            return Err(DenominationError::EmptySingular);
        }
        if self.plural.is_empty() {
            return Err(DenominationError::EmptyPlural);
        }
        // Plural should typically differ from singular
        if self.singular == self.plural {
            // This is a warning, not an error (some words are same)
        }
        Ok(())
    }
}

#[derive(Debug, thiserror::Error)]
pub enum DenominationError {
    #[error("Singular form cannot be empty")]
    EmptySingular,
    #[error("Plural form cannot be empty")]
    EmptyPlural,
    #[error("Missing required form: {0}")]
    MissingForm(String),
}

/// Validate denomination_forms in NovaNet response
pub fn validate_novanet_response(response: &serde_json::Value) -> Result<(), DenominationError> {
    if let Some(generated) = response.get("generated") {
        // Check if denomination_forms exists when expected
        if let Some(forms) = generated.get("denomination_forms") {
            let denomination: DenominationForms = serde_json::from_value(forms.clone())
                .map_err(|_| DenominationError::MissingForm("parse error".to_string()))?;
            denomination.validate()?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_valid_denomination() {
        let forms = DenominationForms {
            singular: "QR Code".to_string(),
            plural: "QR Codes".to_string(),
            with_article: Some("a QR Code".to_string()),
            possessive: Some("QR Code's".to_string()),
        };
        assert!(forms.validate().is_ok());
    }

    #[test]
    fn test_empty_singular() {
        let forms = DenominationForms {
            singular: "".to_string(),
            plural: "QR Codes".to_string(),
            with_article: None,
            possessive: None,
        };
        assert!(matches!(forms.validate(), Err(DenominationError::EmptySingular)));
    }
}
```

**Step 2: Create integration test**

```rust
// tests/integration/denomination_test.rs
use nika::validation::denomination::validate_novanet_response;

#[tokio::test]
#[cfg(feature = "integration")]
async fn test_denomination_forms_in_generate() {
    let workflow_yaml = r#"
name: test-denomination
version: "1.0"

mcp:
  novanet:
    command: node
    args:
      - ../../novanet-dev/tools/novanet-mcp/dist/index.js

tasks:
  generate:
    invoke: novanet_generate
    params:
      entity: "qr-code"
      locale: "en-US"
      forms:
        - denomination_forms
"#;

    let workflow: nika::Workflow = serde_yaml::from_str(workflow_yaml).unwrap();
    let result = nika::run_workflow(&workflow).await.unwrap();

    let output = &result.tasks["generate"];
    let response: serde_json::Value = serde_json::from_str(&output.output).unwrap();

    // Validate denomination_forms structure
    let validation = validate_novanet_response(&response);
    assert!(validation.is_ok(), "denomination_forms should be valid: {:?}", validation.err());
}
```

**Step 3: Run test**

Run: `cargo test --features integration test_denomination`
Expected: PASS

**Step 4: Commit**

```bash
git add src/validation/ tests/integration/denomination_test.rs
git commit -m "feat(validation): add denomination_forms ADR-033 validation

- DenominationForms struct with singular/plural/article/possessive
- validate_novanet_response helper
- Integration test with real NovaNet

Co-Authored-By: Claude <noreply@anthropic.com>"
```

---

## Task 7: Setup CI Pipeline for Integration Tests

**Files:**
- Create: `nika-dev/.github/workflows/integration.yml`

**Step 1: Create GitHub Actions workflow**

```yaml
# .github/workflows/integration.yml
name: Integration Tests

on:
  pull_request:
    branches: [main]
  push:
    branches: [main]
  workflow_dispatch:

env:
  CARGO_TERM_COLOR: always

jobs:
  integration:
    runs-on: ubuntu-latest

    services:
      neo4j:
        image: neo4j:5.15
        ports:
          - 7474:7474
          - 7687:7687
        env:
          NEO4J_AUTH: neo4j/testpassword
          NEO4J_PLUGINS: '["apoc"]'
        options: >-
          --health-cmd "wget -q --spider http://localhost:7474 || exit 1"
          --health-interval 10s
          --health-timeout 5s
          --health-retries 5

    steps:
      - uses: actions/checkout@v4

      - name: Setup Node.js
        uses: actions/setup-node@v4
        with:
          node-version: '20'

      - name: Setup Rust
        uses: dtolnay/rust-action@stable

      - name: Cache Cargo
        uses: actions/cache@v4
        with:
          path: |
            ~/.cargo/bin/
            ~/.cargo/registry/index/
            ~/.cargo/registry/cache/
            ~/.cargo/git/db/
            target/
          key: ${{ runner.os }}-cargo-${{ hashFiles('**/Cargo.lock') }}

      - name: Build NovaNet MCP
        run: |
          cd ../novanet-dev/tools/novanet-mcp
          npm install
          npm run build

      - name: Seed Neo4j
        run: |
          cd ../novanet-dev
          npm run db:seed
        env:
          NEO4J_URI: bolt://localhost:7687
          NEO4J_USER: neo4j
          NEO4J_PASSWORD: testpassword

      - name: Run Integration Tests
        run: |
          cd tools/nika
          cargo test --features integration
        env:
          NEO4J_URI: bolt://localhost:7687
          NEO4J_USER: neo4j
          NEO4J_PASSWORD: testpassword
          ANTHROPIC_API_KEY: ${{ secrets.ANTHROPIC_API_KEY }}
```

**Step 2: Commit**

```bash
git add .github/workflows/integration.yml
git commit -m "ci: add integration test workflow

- Neo4j service container
- Build NovaNet MCP server
- Seed database
- Run cargo test --features integration

Co-Authored-By: Claude <noreply@anthropic.com>"
```

---

## Summary

After completing MVP 4, you will have:

- Real MCP connection (no mocks) to NovaNet
- Integration tests for `invoke:` and `agent:` verbs
- denomination_forms (ADR-033) validation
- CI pipeline running integration tests on every PR

**Total tasks:** 7
**Dependencies:** NovaNet MCP server, Neo4j, ANTHROPIC_API_KEY for agent tests
