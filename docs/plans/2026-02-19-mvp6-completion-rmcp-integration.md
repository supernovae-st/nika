# Plan: MVP 6 Completion + rmcp Integration

**Date:** 2026-02-19
**Status:** In Progress
**Target:** Nika v0.3.0 release

## Overview

This plan completes MVP 6 (v0.3 features) and integrates the `rmcp` crate to replace our custom MCP implementation with the Anthropic-maintained SDK.

```
┌─────────────────────────────────────────────────────────────────────────────┐
│  EXECUTION ORDER                                                            │
├─────────────────────────────────────────────────────────────────────────────┤
│                                                                             │
│  PHASE 1: MVP 6 COMPLETION (~2h)                                           │
│  ├── 1.1 Create v0.3 showcase examples                                     │
│  ├── 1.2 Update README with quick-start                                    │
│  └── 1.3 Validate all examples run                                         │
│                                                                             │
│  PHASE 2: RMCP INTEGRATION (~3h)                                           │
│  ├── 2.1 Add rmcp 0.16 dependency                                          │
│  ├── 2.2 Create rmcp adapter layer                                         │
│  ├── 2.3 Migrate McpClient to use rmcp                                     │
│  ├── 2.4 Update tests                                                      │
│  └── 2.5 Remove custom protocol/transport modules                          │
│                                                                             │
│  PHASE 3: VALIDATION (~1h)                                                 │
│  ├── 3.1 Run full test suite                                               │
│  ├── 3.2 Test with real NovaNet MCP                                        │
│  └── 3.3 Update documentation                                              │
│                                                                             │
│  🎯 RESULT: Nika v0.3.0 with rmcp-backed MCP client                        │
│                                                                             │
└─────────────────────────────────────────────────────────────────────────────┘
```

---

## Phase 1: MVP 6 Completion

### 1.1 Create v0.3 Showcase Examples

**Goal:** Demonstrate new v0.3 features (for_each, observability, MCP events)

| Example | Purpose | Features Shown |
|---------|---------|----------------|
| `v03-parallel-locales.yaml` | Generate content for 5 locales in parallel | `for_each`, `concurrency: 5` |
| `v03-agent-with-tools.yaml` | Agent using multiple MCP tools | `agent:`, tool calling, stop conditions |
| `v03-resilience-demo.yaml` | Show retry/circuit breaker | `resilience:` config, error handling |

**Files to create:**
```
examples/
├── v03-parallel-locales.yaml      # NEW
├── v03-agent-with-tools.yaml      # NEW
└── v03-resilience-demo.yaml       # NEW
```

**v03-parallel-locales.yaml structure:**
```yaml
schema: "nika/workflow@0.2"
provider: claude

mcp:
  novanet:
    command: cargo
    args: [run, --manifest-path, ../../../novanet-dev/tools/novanet-mcp/Cargo.toml]

tasks:
  - id: locales
    exec:
      command: "echo '[\"fr-FR\", \"en-US\", \"es-ES\", \"de-DE\", \"ja-JP\"]'"
    output:
      format: json

  - id: generate_all
    for_each:
      items: $locales
      as: locale
      concurrency: 5
    use:
      loc: locale
    invoke:
      mcp: novanet
      tool: novanet_generate
      params:
        entity: "qr-code"
        locale: "{{use.loc}}"
    output:
      format: json

  - id: summary
    use:
      results: generate_all
    infer:
      prompt: |
        Summarize the generation results for all locales:
        {{use.results}}

        Output a JSON with locale → status mapping.
```

### 1.2 Update README

**Goal:** Quick-start guide for new users

**Sections to add:**
- Installation (cargo install)
- Quick Start (run first workflow)
- v0.3 Features highlight
- MCP Integration guide

### 1.3 Validate Examples

**Goal:** All examples must parse and run (with mock)

```bash
# Validation commands
for f in examples/*.yaml; do
  cargo run -- validate "$f"
done

# Run with mock (no real MCP needed)
cargo run -- run examples/v03-parallel-locales.yaml --mock
```

---

## Phase 2: rmcp Integration

### 2.1 Add rmcp Dependency

**Goal:** Add rmcp 0.16 with required features

**Cargo.toml changes:**
```toml
[dependencies]
# MCP - Anthropic's official Rust SDK
rmcp = { version = "0.16", features = ["client", "transport-child-process"] }
```

**Feature investigation:**
```
rmcp features:
- client          # MCP client implementation
- server          # MCP server implementation (not needed)
- transport-io    # stdio transport
- transport-child-process  # spawn child process (what we need)
- transport-sse   # Server-Sent Events (not needed)
```

### 2.2 Create rmcp Adapter Layer

**Goal:** Thin adapter to preserve our API while using rmcp internals

**New file: `src/mcp/rmcp_adapter.rs`**
```rust
//! Adapter layer between Nika's MCP types and rmcp crate
//!
//! This module provides conversion between:
//! - nika::mcp::McpConfig ↔ rmcp::transport::ChildProcessTransport
//! - nika::mcp::ToolCallResult ↔ rmcp::types::CallToolResult
//! - nika::mcp::ResourceContent ↔ rmcp::types::ReadResourceResult

use rmcp::{Client, ClientCapabilities, Implementation};
use rmcp::transport::ChildProcessTransport;

use crate::mcp::types::{McpConfig, ToolCallResult, ResourceContent};
use crate::error::{NikaError, Result};

/// Convert McpConfig to rmcp ChildProcessTransport
pub fn config_to_transport(config: &McpConfig) -> Result<ChildProcessTransport> {
    let mut cmd = std::process::Command::new(&config.command);
    cmd.args(&config.args);
    for (k, v) in &config.env {
        cmd.env(k, v);
    }

    ChildProcessTransport::new(cmd)
        .map_err(|e| NikaError::McpStartError {
            name: config.name.clone(),
            reason: e.to_string(),
        })
}

/// Convert rmcp CallToolResult to nika ToolCallResult
pub fn convert_tool_result(result: rmcp::types::CallToolResult) -> ToolCallResult {
    let content = result.content.into_iter().map(|c| {
        match c {
            rmcp::types::Content::Text { text } => ContentBlock::text(text),
            rmcp::types::Content::Image { data, mime_type } => {
                ContentBlock::image(data, mime_type)
            }
            // Handle other content types
            _ => ContentBlock::text(format!("{:?}", c)),
        }
    }).collect();

    ToolCallResult {
        content,
        is_error: result.is_error.unwrap_or(false),
    }
}
```

### 2.3 Migrate McpClient

**Goal:** Replace custom JSON-RPC implementation with rmcp

**Changes to `src/mcp/client.rs`:**

| Before (Custom) | After (rmcp) |
|-----------------|--------------|
| Manual JSON-RPC serialization | `rmcp::Client::call_tool()` |
| Custom stdin/stdout handling | `rmcp::transport::ChildProcessTransport` |
| Manual protocol handshake | `rmcp::Client::initialize()` |
| Custom reconnection logic | Keep (rmcp doesn't handle this) |

**Key changes:**
```rust
pub struct McpClient {
    name: String,
    config: Option<McpConfig>,
    connected: AtomicBool,
    is_mock: bool,

    // REMOVE: process, request_id, io_lock
    // ADD: rmcp client
    rmcp_client: Option<rmcp::Client<ChildProcessTransport>>,
}

impl McpClient {
    pub async fn connect(&self) -> Result<()> {
        if self.is_mock { return Ok(()); }

        let config = self.config.as_ref().ok_or(...)?;
        let transport = rmcp_adapter::config_to_transport(config)?;

        let client = rmcp::Client::new(transport)
            .with_capabilities(ClientCapabilities::default())
            .with_implementation(Implementation {
                name: "nika".to_string(),
                version: env!("CARGO_PKG_VERSION").to_string(),
            });

        client.initialize().await.map_err(...)?;

        self.rmcp_client = Some(client);
        self.connected.store(true, Ordering::SeqCst);
        Ok(())
    }

    pub async fn call_tool(&self, name: &str, params: Value) -> Result<ToolCallResult> {
        if self.is_mock { return Ok(self.mock_tool_call(name, params)); }

        let client = self.rmcp_client.as_ref().ok_or(...)?;

        let result = client.call_tool(name, params).await
            .map_err(|e| NikaError::McpToolError {
                tool: name.to_string(),
                reason: e.to_string(),
            })?;

        Ok(rmcp_adapter::convert_tool_result(result))
    }
}
```

### 2.4 Update Tests

**Goal:** All existing tests must pass with rmcp backend

| Test Category | Changes Needed |
|---------------|----------------|
| Mock tests | No change (mock mode preserved) |
| Unit tests | Update for new internal structure |
| Integration tests | Test with real rmcp transport |

**New test: `tests/rmcp_integration_test.rs`**
```rust
#[tokio::test]
#[ignore] // Requires real MCP server
async fn test_rmcp_real_connection() {
    let config = McpConfig::new("novanet", "cargo")
        .with_args(["run", "--manifest-path", "..."]);

    let client = McpClient::new(config).unwrap();
    client.connect().await.unwrap();

    let tools = client.list_tools().await.unwrap();
    assert!(!tools.is_empty());
}
```

### 2.5 Remove Custom Modules

**Goal:** Delete code replaced by rmcp

**Files to delete:**
```
src/mcp/
├── protocol.rs    # DELETE - replaced by rmcp JSON-RPC
├── transport.rs   # DELETE - replaced by rmcp transport
└── mod.rs         # UPDATE - remove deleted module references
```

**Files to keep:**
```
src/mcp/
├── client.rs        # KEEP - updated to use rmcp
├── types.rs         # KEEP - Nika's public API types
├── rmcp_adapter.rs  # NEW - conversion layer
└── mod.rs           # UPDATE
```

---

## Phase 3: Validation

### 3.1 Full Test Suite

```bash
# All tests must pass
cargo test

# Clippy must be clean
cargo clippy -- -D warnings

# Documentation must build
cargo doc --no-deps
```

### 3.2 Real NovaNet Test

```bash
# Start Neo4j (required for NovaNet MCP)
docker start neo4j-novanet

# Run integration test
cargo test --features integration test_rmcp_real

# Run example with real MCP
cargo run -- run examples/invoke-novanet.yaml
```

### 3.3 Documentation Updates

| File | Updates |
|------|---------|
| `CLAUDE.md` | Update MCP section for rmcp |
| `README.md` | Add rmcp mention in deps |
| `src/mcp/mod.rs` | Update module docs |
| `CHANGELOG.md` | Add v0.3.0 entry |

---

## Task Checklist

### Phase 1: MVP 6 Completion
- [ ] 1.1.1 Create `v03-parallel-locales.yaml`
- [ ] 1.1.2 Create `v03-agent-with-tools.yaml`
- [ ] 1.1.3 Create `v03-resilience-demo.yaml`
- [ ] 1.2.1 Update README quick-start section
- [ ] 1.2.2 Add v0.3 features highlight
- [ ] 1.3.1 Validate all examples parse
- [ ] 1.3.2 Run examples with mock mode

### Phase 2: rmcp Integration
- [ ] 2.1.1 Add rmcp 0.16 to Cargo.toml
- [ ] 2.1.2 Verify rmcp features available
- [ ] 2.2.1 Create `rmcp_adapter.rs`
- [ ] 2.2.2 Implement config_to_transport
- [ ] 2.2.3 Implement convert_tool_result
- [ ] 2.3.1 Update McpClient struct
- [ ] 2.3.2 Migrate connect() to rmcp
- [ ] 2.3.3 Migrate call_tool() to rmcp
- [ ] 2.3.4 Migrate list_tools() to rmcp
- [ ] 2.3.5 Migrate read_resource() to rmcp
- [ ] 2.3.6 Keep mock mode working
- [ ] 2.3.7 Keep reconnection logic
- [ ] 2.4.1 Update unit tests
- [ ] 2.4.2 Add rmcp integration test
- [ ] 2.5.1 Delete protocol.rs
- [ ] 2.5.2 Delete transport.rs
- [ ] 2.5.3 Update mod.rs

### Phase 3: Validation
- [ ] 3.1.1 cargo test passes
- [ ] 3.1.2 cargo clippy clean
- [ ] 3.1.3 cargo doc builds
- [ ] 3.2.1 Test with real NovaNet
- [ ] 3.2.2 Run example workflows
- [ ] 3.3.1 Update CLAUDE.md
- [ ] 3.3.2 Update README.md
- [ ] 3.3.3 Add CHANGELOG entry

---

## Success Criteria

### MVP 6 Complete
- [ ] 3 new v0.3 example workflows
- [ ] README has quick-start guide
- [ ] All examples validate

### rmcp Integration Complete
- [ ] `cargo test` passes (100%)
- [ ] Mock mode still works
- [ ] Real MCP connection works
- [ ] ~500 lines of code removed (protocol.rs, transport.rs)
- [ ] rmcp 0.16 in dependencies

### v0.3.0 Release Ready
- [ ] Version bumped in Cargo.toml
- [ ] CHANGELOG updated
- [ ] Git tag created

---

## Risks and Mitigations

| Risk | Mitigation |
|------|------------|
| rmcp API breaking changes | Pin to 0.16.x, review changelog |
| Mock mode breaks | Keep mock logic in McpClient, not in rmcp |
| Reconnection breaks | Keep custom reconnection, rmcp handles single connections |
| Performance regression | Benchmark before/after, rmcp is well-optimized |

---

## References

- [rmcp crate](https://crates.io/crates/rmcp)
- [rmcp GitHub](https://github.com/anthropics/mcp-rust-sdk)
- [MCP Spec 2025-11-25](https://modelcontextprotocol.io/specification)
- [Nika MCP Research](./2026-02-19-mcp-rust-crates.md)
