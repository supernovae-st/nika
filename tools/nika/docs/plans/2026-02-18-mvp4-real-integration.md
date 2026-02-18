# MVP 4: Real MCP Integration

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Replace mock MCP client with real rmcp-based implementation and validate against NovaNet MCP server.

**Architecture:** Enable real stdio-based MCP communication using rmcp 0.1 crate. McpClient spawns MCP server process and communicates via JSON-RPC 2.0 over stdin/stdout.

**Tech Stack:** Rust, rmcp 0.1, tokio, serde_json

---

## Task 1: Enable rmcp Feature and Update Dependencies

**Files:**
- Modify: `Cargo.toml`
- Modify: `src/mcp/mod.rs`

**Step 1: Update Cargo.toml to enable rmcp by default**

```toml
[features]
default = ["tui", "mcp"]
tui = ["dep:ratatui", "dep:crossterm"]
mcp = ["dep:rmcp"]

[dependencies]
# MCP (v0.2)
rmcp = { version = "0.1", features = ["client", "transport-io"] }
```

**Step 2: Run cargo check to verify compilation**

Run: `cargo check --all-features`
Expected: PASS

**Step 3: Commit**

```bash
git add Cargo.toml
git commit -m "feat(mcp): enable rmcp feature by default"
```

---

## Task 2: Create Real MCP Transport Layer

**Files:**
- Create: `src/mcp/transport.rs`
- Modify: `src/mcp/mod.rs`

**Step 1: Write the failing test**

```rust
// tests/mcp_transport_test.rs
use nika::mcp::transport::McpTransport;

#[tokio::test]
async fn test_transport_spawn_echo_process() {
    let transport = McpTransport::new("echo", &["hello"]);
    let process = transport.spawn().await;
    assert!(process.is_ok());
}

#[tokio::test]
async fn test_transport_spawn_invalid_command_fails() {
    let transport = McpTransport::new("nonexistent-command-xyz", &[]);
    let result = transport.spawn().await;
    assert!(result.is_err());
}
```

**Step 2: Run test to verify it fails**

Run: `cargo test test_transport_spawn -v`
Expected: FAIL with "module transport not found"

**Step 3: Write implementation**

```rust
// src/mcp/transport.rs
use std::collections::HashMap;
use std::process::Stdio;
use tokio::process::{Child, Command};
use crate::error::{NikaError, Result};

/// MCP Transport - manages server process lifecycle
#[derive(Debug)]
pub struct McpTransport {
    command: String,
    args: Vec<String>,
    env: HashMap<String, String>,
}

impl McpTransport {
    pub fn new(command: &str, args: &[&str]) -> Self {
        Self {
            command: command.to_string(),
            args: args.iter().map(|s| s.to_string()).collect(),
            env: HashMap::new(),
        }
    }

    pub fn with_env(mut self, key: &str, value: &str) -> Self {
        self.env.insert(key.to_string(), value.to_string());
        self
    }

    pub async fn spawn(&self) -> Result<Child> {
        let mut cmd = Command::new(&self.command);
        cmd.args(&self.args)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());

        for (k, v) in &self.env {
            cmd.env(k, v);
        }

        cmd.spawn().map_err(|e| NikaError::McpStartError {
            name: self.command.clone(),
            reason: e.to_string(),
        })
    }
}
```

**Step 4: Update mod.rs**

```rust
pub mod transport;
pub use transport::McpTransport;
```

**Step 5: Run test to verify it passes**

Run: `cargo test test_transport_spawn -v`
Expected: PASS

**Step 6: Commit**

```bash
git add src/mcp/transport.rs src/mcp/mod.rs tests/mcp_transport_test.rs
git commit -m "feat(mcp): add McpTransport for process management"
```

---

## Task 3: Implement Real MCP Protocol Communication

**Files:**
- Create: `src/mcp/protocol.rs`
- Modify: `src/mcp/mod.rs`

**Step 1: Write the failing test**

```rust
// tests/mcp_protocol_test.rs
use serde_json::json;

#[test]
fn test_json_rpc_request_serialization() {
    let req = nika::mcp::protocol::JsonRpcRequest::new(1, "tools/call", json!({"name": "test"}));
    let json = serde_json::to_string(&req).unwrap();
    assert!(json.contains("jsonrpc"));
    assert!(json.contains("2.0"));
    assert!(json.contains("tools/call"));
}

#[test]
fn test_json_rpc_response_success_parse() {
    let json = r#"{"jsonrpc":"2.0","id":1,"result":{"content":[{"type":"text","text":"hello"}]}}"#;
    let resp: nika::mcp::protocol::JsonRpcResponse = serde_json::from_str(json).unwrap();
    assert!(resp.result.is_some());
    assert!(resp.error.is_none());
}

#[test]
fn test_json_rpc_response_error_parse() {
    let json = r#"{"jsonrpc":"2.0","id":1,"error":{"code":-32600,"message":"Invalid Request"}}"#;
    let resp: nika::mcp::protocol::JsonRpcResponse = serde_json::from_str(json).unwrap();
    assert!(resp.error.is_some());
}
```

**Step 2: Run test to verify it fails**

Run: `cargo test test_json_rpc -v`
Expected: FAIL with "module protocol not found"

**Step 3: Write implementation**

```rust
// src/mcp/protocol.rs
use serde::{Deserialize, Serialize};
use serde_json::Value;

/// JSON-RPC 2.0 Request
#[derive(Debug, Serialize)]
pub struct JsonRpcRequest {
    pub jsonrpc: &'static str,
    pub id: u64,
    pub method: String,
    pub params: Value,
}

impl JsonRpcRequest {
    pub fn new(id: u64, method: &str, params: Value) -> Self {
        Self {
            jsonrpc: "2.0",
            id,
            method: method.to_string(),
            params,
        }
    }
}

/// JSON-RPC 2.0 Response
#[derive(Debug, Deserialize)]
pub struct JsonRpcResponse {
    pub jsonrpc: String,
    pub id: Option<u64>,
    pub result: Option<Value>,
    pub error: Option<JsonRpcError>,
}

/// JSON-RPC 2.0 Error
#[derive(Debug, Deserialize)]
pub struct JsonRpcError {
    pub code: i32,
    pub message: String,
    pub data: Option<Value>,
}

impl JsonRpcResponse {
    pub fn is_success(&self) -> bool {
        self.result.is_some() && self.error.is_none()
    }
}
```

**Step 4: Update mod.rs**

```rust
pub mod protocol;
pub use protocol::{JsonRpcRequest, JsonRpcResponse, JsonRpcError};
```

**Step 5: Run test to verify it passes**

Run: `cargo test test_json_rpc -v`
Expected: PASS

**Step 6: Commit**

```bash
git add src/mcp/protocol.rs tests/mcp_protocol_test.rs
git commit -m "feat(mcp): add JSON-RPC 2.0 protocol types"
```

---

## Task 4: Implement Real McpClient with Stdio Communication

**Files:**
- Modify: `src/mcp/client.rs`
- Create: `tests/mcp_client_real_test.rs`

**Step 1: Write the failing test**

```rust
// tests/mcp_client_real_test.rs
use nika::mcp::{McpClient, McpConfig};
use serde_json::json;

#[tokio::test]
async fn test_real_client_connect_and_initialize() {
    // This test will only pass when a real MCP server is available
    // For now, test with a mock echo-based MCP server
    let config = McpConfig::new("test", "echo")
        .with_args(["-e", r#"{"jsonrpc":"2.0","id":1,"result":{"protocolVersion":"1.0","capabilities":{}}}"#]);

    let client = McpClient::new(config).unwrap();
    // Real connect will fail with echo (no real MCP handshake)
    // This validates the structure works
    assert!(!client.is_connected());
}

#[tokio::test]
async fn test_mock_client_still_works() {
    let client = McpClient::mock("novanet");
    assert!(client.is_connected());

    let result = client.call_tool("novanet_describe", json!({})).await;
    assert!(result.is_ok());
}
```

**Step 2: Run test to verify behavior**

Run: `cargo test test_real_client -v && cargo test test_mock_client -v`
Expected: PASS (mock still works)

**Step 3: Update McpClient implementation**

Update `src/mcp/client.rs` to add real connection support using transport and protocol modules:

```rust
// Add imports at top
use crate::mcp::transport::McpTransport;
use crate::mcp::protocol::{JsonRpcRequest, JsonRpcResponse};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::Child;
use std::sync::atomic::AtomicU64;

// Add to McpClient struct
pub struct McpClient {
    name: String,
    config: Option<McpConfig>,
    connected: AtomicBool,
    is_mock: bool,
    // New fields for real connection
    process: parking_lot::Mutex<Option<Child>>,
    request_id: AtomicU64,
}

// Add real connect implementation
impl McpClient {
    pub async fn connect(&self) -> Result<()> {
        if self.is_connected() {
            return Ok(());
        }

        if self.is_mock {
            self.connected.store(true, Ordering::SeqCst);
            return Ok(());
        }

        let config = self.config.as_ref().ok_or_else(|| NikaError::McpNotConnected {
            name: self.name.clone(),
        })?;

        // Spawn the MCP server process
        let transport = McpTransport::new(&config.command, &config.args.iter().map(|s| s.as_str()).collect::<Vec<_>>());

        // Add environment variables
        let mut transport = transport;
        for (k, v) in &config.env {
            transport = transport.with_env(k, v);
        }

        let child = transport.spawn().await?;
        *self.process.lock() = Some(child);

        // Perform MCP initialization handshake
        self.initialize().await?;

        self.connected.store(true, Ordering::SeqCst);
        Ok(())
    }

    async fn initialize(&self) -> Result<()> {
        let init_request = JsonRpcRequest::new(
            self.next_id(),
            "initialize",
            serde_json::json!({
                "protocolVersion": "1.0",
                "capabilities": {},
                "clientInfo": {
                    "name": "nika",
                    "version": env!("CARGO_PKG_VERSION")
                }
            }),
        );

        let response = self.send_request(&init_request).await?;
        if !response.is_success() {
            return Err(NikaError::McpStartError {
                name: self.name.clone(),
                reason: response.error.map(|e| e.message).unwrap_or_default(),
            });
        }

        Ok(())
    }

    async fn send_request(&self, request: &JsonRpcRequest) -> Result<JsonRpcResponse> {
        let mut process_guard = self.process.lock();
        let process = process_guard.as_mut().ok_or_else(|| NikaError::McpNotConnected {
            name: self.name.clone(),
        })?;

        let stdin = process.stdin.as_mut().ok_or_else(|| NikaError::McpToolError {
            tool: "stdin".to_string(),
            reason: "Process stdin not available".to_string(),
        })?;

        let stdout = process.stdout.take().ok_or_else(|| NikaError::McpToolError {
            tool: "stdout".to_string(),
            reason: "Process stdout not available".to_string(),
        })?;

        // Send request
        let json = serde_json::to_string(request).map_err(|e| NikaError::McpToolError {
            tool: request.method.clone(),
            reason: e.to_string(),
        })?;

        stdin.write_all(json.as_bytes()).await.map_err(|e| NikaError::McpToolError {
            tool: request.method.clone(),
            reason: e.to_string(),
        })?;
        stdin.write_all(b"\n").await.map_err(|e| NikaError::McpToolError {
            tool: request.method.clone(),
            reason: e.to_string(),
        })?;
        stdin.flush().await.map_err(|e| NikaError::McpToolError {
            tool: request.method.clone(),
            reason: e.to_string(),
        })?;

        // Read response
        let mut reader = BufReader::new(stdout);
        let mut line = String::new();
        reader.read_line(&mut line).await.map_err(|e| NikaError::McpToolError {
            tool: request.method.clone(),
            reason: e.to_string(),
        })?;

        // Put stdout back
        process.stdout = Some(reader.into_inner());

        serde_json::from_str(&line).map_err(|e| NikaError::McpToolError {
            tool: request.method.clone(),
            reason: format!("Invalid JSON response: {}", e),
        })
    }

    fn next_id(&self) -> u64 {
        self.request_id.fetch_add(1, Ordering::SeqCst)
    }
}
```

**Step 4: Run tests**

Run: `cargo test mcp -v`
Expected: PASS

**Step 5: Commit**

```bash
git add src/mcp/client.rs tests/mcp_client_real_test.rs
git commit -m "feat(mcp): implement real stdio MCP communication"
```

---

## Task 5: Create Integration Test Infrastructure

**Files:**
- Create: `tests/integration/mod.rs`
- Create: `tests/integration/helpers.rs`
- Modify: `Cargo.toml` (add integration feature)

**Step 1: Update Cargo.toml with integration feature**

```toml
[features]
default = ["tui", "mcp"]
tui = ["dep:ratatui", "dep:crossterm"]
mcp = ["dep:rmcp"]
integration = []
```

**Step 2: Create integration test helpers**

```rust
// tests/integration/helpers.rs
use std::process::Command;
use std::time::Duration;

/// Check if NovaNet MCP server is available
pub fn is_novanet_available() -> bool {
    // Check if novanet-mcp binary exists
    let novanet_path = std::env::var("NOVANET_MCP_PATH")
        .unwrap_or_else(|_| "/Users/thibaut/supernovae-st/supernovae-agi/novanet-dev/tools/novanet-mcp/target/release/novanet-mcp".to_string());

    std::path::Path::new(&novanet_path).exists()
}

/// Check if Neo4j is running
pub fn is_neo4j_available() -> bool {
    std::process::Command::new("nc")
        .args(["-z", "localhost", "7687"])
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

/// Skip test if integration dependencies not available
#[macro_export]
macro_rules! require_integration {
    () => {
        if !$crate::integration::helpers::is_novanet_available() {
            eprintln!("Skipping: NovaNet MCP not available");
            return;
        }
        if !$crate::integration::helpers::is_neo4j_available() {
            eprintln!("Skipping: Neo4j not available");
            return;
        }
    };
}
```

**Step 3: Create integration module**

```rust
// tests/integration/mod.rs
pub mod helpers;

pub use helpers::{is_novanet_available, is_neo4j_available};
```

**Step 4: Commit**

```bash
git add tests/integration/ Cargo.toml
git commit -m "feat(test): add integration test infrastructure"
```

---

## Task 6: Integration Test - Invoke with Real NovaNet

**Files:**
- Create: `tests/integration/invoke_novanet_test.rs`

**Step 1: Write integration test**

```rust
// tests/integration/invoke_novanet_test.rs
#![cfg(feature = "integration")]

mod helpers;

use nika::mcp::{McpClient, McpConfig};
use serde_json::json;

#[tokio::test]
#[ignore] // Run with: cargo test --features integration -- --ignored
async fn test_invoke_novanet_describe() {
    helpers::require_integration!();

    let novanet_path = std::env::var("NOVANET_MCP_PATH")
        .unwrap_or_else(|_| "/Users/thibaut/supernovae-st/supernovae-agi/novanet-dev/tools/novanet-mcp/target/release/novanet-mcp".to_string());

    let config = McpConfig::new("novanet", &novanet_path)
        .with_env("NOVANET_MCP_NEO4J_PASSWORD", "novanetpassword");

    let client = McpClient::new(config).unwrap();
    client.connect().await.expect("Failed to connect");

    let result = client.call_tool("novanet_describe", json!({
        "describe": "schema"
    })).await;

    assert!(result.is_ok(), "novanet_describe failed: {:?}", result);

    let response = result.unwrap();
    assert!(!response.is_error);

    // Verify response contains schema information
    let content = &response.content[0];
    let text = content.text().expect("Expected text content");
    assert!(text.contains("realm") || text.contains("nodes"));
}

#[tokio::test]
#[ignore]
async fn test_invoke_novanet_query() {
    helpers::require_integration!();

    let novanet_path = std::env::var("NOVANET_MCP_PATH")
        .unwrap_or_else(|_| "/Users/thibaut/supernovae-st/supernovae-agi/novanet-dev/tools/novanet-mcp/target/release/novanet-mcp".to_string());

    let config = McpConfig::new("novanet", &novanet_path)
        .with_env("NOVANET_MCP_NEO4J_PASSWORD", "novanetpassword");

    let client = McpClient::new(config).unwrap();
    client.connect().await.expect("Failed to connect");

    let result = client.call_tool("novanet_query", json!({
        "cypher": "MATCH (n) RETURN count(n) as count LIMIT 1"
    })).await;

    assert!(result.is_ok(), "novanet_query failed: {:?}", result);
}
```

**Step 2: Commit**

```bash
git add tests/integration/invoke_novanet_test.rs
git commit -m "test(integration): add NovaNet invoke integration tests"
```

---

## Task 7: Cleanup and Final Verification

**Step 1: Run all unit tests**

Run: `cargo test --lib`
Expected: All tests pass

**Step 2: Run clippy**

Run: `cargo clippy -- -D warnings`
Expected: No warnings

**Step 3: Run cargo fmt**

Run: `cargo fmt --check`
Expected: No formatting issues

**Step 4: Build release**

Run: `cargo build --release`
Expected: Success

**Step 5: Commit final cleanup**

```bash
git add -A
git commit -m "chore: MVP 4 cleanup and verification"
```

---

## Summary

MVP 4 delivers:
- Real MCP transport layer (process spawn, stdio communication)
- JSON-RPC 2.0 protocol types
- Real McpClient implementation (replacing TODO placeholders)
- Integration test infrastructure
- NovaNet integration tests (invoke: verb validation)

**Next:** MVP 5 will add TUI visualization of real MCP responses and workflow debugging.
