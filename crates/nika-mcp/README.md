# nika-mcp

MCP (Model Context Protocol) client integration for Nika.

## Overview

This crate provides the MCP client for Nika workflows:

- **McpClient** - Manages MCP server connections
- **Protocol** - JSON-RPC 2.0 message handling
- **Validation** - Schema validation for tool parameters
- **RMCP Adapter** - Integration with Anthropic's rmcp SDK

## Architecture

```
nika-mcp/
├── client.rs       # McpClient with caching and reconnection
├── protocol.rs     # JSON-RPC request/response types
├── types.rs        # McpConfig, ToolResult
├── rmcp_adapter.rs # rmcp 0.16 integration
└── validation/     # Schema validation
    ├── validator.rs
    ├── enhancer.rs
    └── schema_cache.rs
```

## Usage

```rust
use nika_mcp::McpClient;

// Create client from config
let client = McpClient::new(config).await?;

// Call a tool
let result = client.call_tool("novanet_generate", params).await?;

// List available tools
let tools = client.list_tools().await?;
```

## Features

- **Connection pooling** - Reuses MCP server connections
- **Auto-reconnect** - Handles server restarts gracefully
- **Schema caching** - Caches tool schemas for validation
- **Mock mode** - For testing without real servers

## License

MIT
