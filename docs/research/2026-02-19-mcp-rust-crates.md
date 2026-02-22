# MCP Rust Crates Research

**Date:** 2026-02-19
**Status:** Completed
**Context:** Nika v0.3 MCP integration

## Summary

Research on available Rust crates for Model Context Protocol (MCP) implementation, comparing with Nika's current custom implementation.

## Available Crates

### 1. rmcp (Recommended)

| Attribute | Value |
|-----------|-------|
| **Version** | 0.16.0 |
| **Downloads** | 3.76M |
| **Last Updated** | ~15 hours ago |
| **Maintainer** | Anthropic |
| **Repository** | github.com/anthropics/mcp-rust-sdk |

**Features:**
- `server` - MCP server implementation
- `client` - MCP client implementation
- `transport-io` - stdio transport

**Dependencies:** tokio, serde, schemars

**Verdict:** Ecosystem leader, actively maintained by Anthropic.

### 2. rust-mcp-sdk

| Attribute | Value |
|-----------|-------|
| **Version** | 0.8.3 |
| **Downloads** | 76K |
| **Last Updated** | 16 days ago |

**Features:**
- MCP protocol spec 2025-11-25 (latest)
- Transports: Stdio, Streamable HTTP, SSE
- Multi-client concurrency
- OAuth Authentication
- DNS Rebinding Protection
- MCP Tasks support
- Batch Messages

**Dependencies:** rust-mcp-schema

**Verdict:** Full-featured, type-safe via rust-mcp-schema.

### 3. tower-mcp

| Attribute | Value |
|-----------|-------|
| **Version** | 0.6.0 |
| **Downloads** | 1.6K |
| **Last Updated** | 8 days ago |

**Features:**
- Tower Service abstraction
- Middleware: tracing, metrics, rate limiting, auth
- Multiple transports: stdio, HTTP, WebSocket
- Backward compatibility (2025-03-26 and 2025-11-25 specs)

**Verdict:** Best for axum/tonic integration, middleware-first approach.

## Nika Current State

### Declared but Unused

```toml
# Cargo.toml line 58
rmcp = { version = "0.1", features = ["client", "transport-io"], optional = true }
```

**Grep `rmcp::` in src/ → No matches found**

The `rmcp` dependency is declared but never imported. Nika has a complete custom MCP implementation.

### Custom Implementation

| Module | Purpose | Lines |
|--------|---------|-------|
| `mcp/client.rs` | McpClient with mock/real modes | ~1000 |
| `mcp/protocol.rs` | JSON-RPC 2.0 types | ~150 |
| `mcp/transport.rs` | Process spawn via tokio | ~200 |
| `mcp/types.rs` | McpConfig, ToolCallResult | ~300 |

### Features Implemented

- [x] Initialize handshake (protocol 2024-11-05)
- [x] Tool calls (`tools/call`)
- [x] Resource reads (`resources/read`)
- [x] Tool listing (`tools/list`)
- [x] Mock mode for testing
- [x] Automatic reconnection (3 retries)
- [x] IO lock for concurrent access
- [x] Timeout handling (MCP_CALL_TIMEOUT)
- [x] RAII stdin/stdout restoration
- [x] Drop impl for process cleanup

### Missing from Spec 2025-11-25

- [ ] Prompts API (`prompts/list`, `prompts/get`)
- [ ] Resource subscriptions
- [ ] Logging support
- [ ] Progress notifications
- [ ] Cancellation
- [ ] Sampling API

## Recommendations

### Option A: Keep Custom Implementation (Recommended)

**Pros:**
- Full control over behavior
- Lightweight (no extra dependencies)
- Already works and tested
- Mock mode is custom-tailored

**Cons:**
- Manual maintenance for new MCP features
- Must track spec changes ourselves
- No community contributions

**When to choose:** If current features suffice and stability is priority.

### Option B: Migrate to rmcp 0.16

**Pros:**
- Maintained by Anthropic
- Latest MCP spec support
- Community-backed
- 3.76M downloads = battle-tested

**Cons:**
- Breaking changes from 0.1 to 0.16
- Must rewrite MCP integration
- Lose custom mock mode

**When to choose:** If we need latest MCP features (prompts, sampling, etc.)

### Option C: Evaluate rust-mcp-sdk

**Pros:**
- Type-safe schema types
- Full feature set
- OAuth support

**Cons:**
- Heavier dependency
- Less adoption than rmcp
- Learning curve

**When to choose:** If we need OAuth or schema validation.

## Decision

**Status:** Keep custom implementation for v0.3

**Rationale:**
1. Current impl is working and tested
2. v0.3 focus is on for_each and observability, not MCP features
3. rmcp upgrade can be a v0.4 milestone if needed

**Action Items:**
- [x] Remove unused `rmcp = "0.1"` from Cargo.toml (declutter)
- [ ] Document custom MCP impl in CLAUDE.md
- [ ] Consider rmcp migration for v0.4 if new MCP features needed

## References

- [MCP Spec 2025-11-25](https://modelcontextprotocol.io/specification)
- [rmcp on crates.io](https://crates.io/crates/rmcp)
- [rust-mcp-sdk on crates.io](https://crates.io/crates/rust-mcp-sdk)
- [tower-mcp on lib.rs](https://lib.rs/crates/tower-mcp)
