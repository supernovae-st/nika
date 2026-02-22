# Nika MCP Client API Reference

This document describes Nika's MCP (Model Context Protocol) client implementation for connecting to MCP servers.

## Overview

Nika's MCP client enables workflows to invoke tools and read resources from MCP servers. The client handles:

- JSON-RPC 2.0 over stdio transport
- Connection management (connect, disconnect, reconnect)
- Automatic reconnection on connection failures
- Tool listing and invocation
- Resource reading
- Both real and mock modes for testing

## Architecture

```
┌─────────────────────────────────────────────────────────────────────────────┐
│  NIKA MCP CLIENT                                                            │
├─────────────────────────────────────────────────────────────────────────────┤
│                                                                             │
│  ┌───────────────┐     ┌─────────────────┐     ┌─────────────────────┐     │
│  │  McpClient    │────▶│  McpTransport   │────▶│  MCP Server         │     │
│  │               │     │                 │     │  (e.g., NovaNet)    │     │
│  │  • connect()  │     │  • spawn()      │     │                     │     │
│  │  • call_tool()│     │  • send()       │     │  • Tools (7)        │     │
│  │  • list_tools │     │  • receive()    │     │  • Resources (4)    │     │
│  │  • read_res() │     │  • close()      │     │  • Prompts (6)      │     │
│  │  • reconnect()│     │                 │     │                     │     │
│  └───────────────┘     └─────────────────┘     └─────────────────────┘     │
│                                                                             │
│  Protocol: JSON-RPC 2.0 over stdio (stdin/stdout)                           │
│                                                                             │
└─────────────────────────────────────────────────────────────────────────────┘
```

## Configuration

### McpConfig

```rust
pub struct McpConfig {
    /// Server name for logging and error messages
    pub name: String,
    /// Command to spawn the MCP server
    pub command: String,
    /// Arguments to pass to the command
    pub args: Vec<String>,
    /// Environment variables for the server process
    pub env: HashMap<String, String>,
}
```

### YAML Configuration

```yaml
mcp:
  servers:
    novanet:
      command: "cargo"
      args:
        - run
        - --manifest-path
        - ../novanet-mcp/Cargo.toml
      env:
        NEO4J_URI: "bolt://localhost:7687"
        NEO4J_USER: "neo4j"
        NEO4J_PASSWORD: "${NEO4J_PASSWORD}"
```

## Client API

### Creating a Client

```rust
use nika::mcp::{McpClient, McpConfig};

let config = McpConfig {
    name: "novanet".to_string(),
    command: "cargo".to_string(),
    args: vec!["run".to_string(), "--manifest-path".to_string(), "path/to/Cargo.toml".to_string()],
    env: HashMap::new(),
};

let client = McpClient::new(config)?;
```

### Connecting

```rust
client.connect().await?;

// Check connection status
if client.is_connected() {
    println!("Connected!");
}
```

### Listing Available Tools

```rust
let tools = client.list_tools().await?;

for tool in tools {
    println!("Tool: {} - {}", tool.name, tool.description.unwrap_or_default());
}
```

Returns: `Vec<ToolInfo>` where each tool has:
- `name`: Tool identifier (e.g., "novanet_describe")
- `description`: Optional tool description
- `input_schema`: JSON schema for parameters

### Calling Tools

```rust
use serde_json::json;

let result = client.call_tool(
    "novanet_describe",
    json!({ "describe": "schema" })
).await?;

// ToolResult structure
println!("Content: {}", result.text());
println!("Is error: {}", result.is_error);
```

**Parameters:**
- `tool_name`: Name of the tool to call
- `arguments`: JSON object with tool parameters

**Returns:** `ToolResult`
```rust
pub struct ToolResult {
    pub content: Vec<ToolContent>,
    pub is_error: bool,
}

impl ToolResult {
    pub fn text(&self) -> String; // Get concatenated text content
}
```

### Reading Resources

```rust
let resource = client.read_resource("entity://qr-code").await?;

println!("URI: {}", resource.uri);
println!("MIME Type: {:?}", resource.mime_type);
println!("Content: {:?}", resource.text);
```

**Parameters:**
- `uri`: Resource URI (e.g., "entity://qr-code")

**Returns:** `ResourceContent`
```rust
pub struct ResourceContent {
    pub uri: String,
    pub mime_type: Option<String>,
    pub text: Option<String>,
    pub blob: Option<String>,
}
```

### Reconnecting

The client automatically reconnects on connection errors. You can also manually reconnect:

```rust
client.reconnect().await?;
```

### Disconnecting

```rust
client.disconnect().await?;
```

## Error Handling

### Error Types

| Error | Code | Description |
|-------|------|-------------|
| `McpNotConnected` | NIKA-100 | Client not connected to server |
| `McpConnectionFailed` | NIKA-101 | Failed to connect or reconnect |
| `McpSpawnFailed` | NIKA-102 | Failed to spawn server process |
| `McpProtocolError` | NIKA-103 | JSON-RPC protocol error |
| `McpToolError` | NIKA-104 | Tool returned an error |
| `McpResourceNotFound` | NIKA-105 | Resource URI not found |

### Reconnection Logic

The client automatically retries with reconnection on transient errors:

1. Detects connection errors (broken pipe, connection reset, etc.)
2. Attempts reconnection up to 3 times
3. Waits 100ms between attempts
4. Returns error if all retries fail

```rust
// Connection errors that trigger reconnection:
fn is_connection_error(error: &NikaError) -> bool {
    let error_str = error.to_string().to_lowercase();
    error_str.contains("broken pipe")
        || error_str.contains("connection reset")
        || error_str.contains("transport")
        || error_str.contains("failed to write")
        || error_str.contains("i/o error")
}
```

## Mock Mode

For testing, create a mock client that doesn't spawn a real server:

```rust
let mock_client = McpClient::mock("novanet");

// Mock client provides:
// - list_tools() returns predefined mock tools
// - call_tool() returns mock responses
// - read_resource() returns mock content
```

## NovaNet MCP Tools

When connected to NovaNet MCP server, these tools are available:

| Tool | Description | Parameters |
|------|-------------|------------|
| `novanet_describe` | Describe schema, stats, locales, or relations | `{ "describe": "schema" \| "stats" \| "locales" \| "relations" }` |
| `novanet_query` | Execute read-only Cypher query | `{ "cypher": "MATCH (n) RETURN n LIMIT 10" }` |
| `novanet_traverse` | Walk graph from a starting node | `{ "start": "Entity", "depth": 2 }` |
| `novanet_generate` | Generate localized content | `{ "entity": "qr-code", "locale": "fr-FR", "forms": ["text", "title"] }` |
| `novanet_relate` | Create relationships | `{ "from": "...", "to": "...", "type": "..." }` |
| `novanet_mutate` | Modify entities | `{ "key": "...", "properties": {...} }` |
| `novanet_validate` | Validate graph integrity | `{ "check": "orphans" \| "cycles" \| "coherence" }` |

### Example: Describe Schema

```rust
let result = client.call_tool(
    "novanet_describe",
    json!({ "describe": "schema" })
).await?;

// Returns JSON with node types, arc types, layers, etc.
let schema: serde_json::Value = serde_json::from_str(&result.text())?;
```

### Example: Query Entities

```rust
let result = client.call_tool(
    "novanet_query",
    json!({
        "cypher": "MATCH (e:Entity) RETURN e.key AS key LIMIT 5"
    })
).await?;

// Returns JSON array of results
```

### Example: Generate Content

```rust
let result = client.call_tool(
    "novanet_generate",
    json!({
        "entity": "qr-code",
        "locale": "fr-FR",
        "forms": ["text", "title", "meta_description"]
    })
).await?;

// Returns generated content for the entity in French
```

## Integration with Agent Loop

The agent loop uses MCP client for tool calling:

```yaml
tasks:
  - id: generate
    agent:
      prompt: "Generate content for the QR code entity"
      mcp:
        - novanet
      max_turns: 10
```

The agent loop:
1. Connects to specified MCP servers
2. Lists available tools from each server
3. Passes tool definitions to LLM
4. Executes tool calls in parallel
5. Returns results to LLM for next turn

## Best Practices

### 1. Connection Management

```rust
// Connect once, reuse client
let client = McpClient::new(config)?;
client.connect().await?;

// Multiple tool calls use same connection
for entity in entities {
    let result = client.call_tool("novanet_generate", json!({ "entity": entity })).await?;
}

// Disconnect when done
client.disconnect().await?;
```

### 2. Error Handling

```rust
match client.call_tool("novanet_query", params).await {
    Ok(result) => {
        if result.is_error {
            // Tool executed but returned error (e.g., invalid Cypher)
            eprintln!("Tool error: {}", result.text());
        } else {
            // Success
            println!("Result: {}", result.text());
        }
    }
    Err(e) => {
        // Connection or protocol error
        eprintln!("MCP error: {}", e);
    }
}
```

### 3. Concurrent Calls

The client is thread-safe and supports concurrent calls:

```rust
use std::sync::Arc;
use tokio::task::JoinSet;

let client = Arc::new(McpClient::new(config)?);
client.connect().await?;

let mut join_set = JoinSet::new();

for i in 0..5 {
    let client = Arc::clone(&client);
    join_set.spawn(async move {
        client.call_tool("novanet_describe", json!({"describe": "stats"})).await
    });
}

while let Some(result) = join_set.join_next().await {
    // Process results
}
```

## Troubleshooting

### Connection Failures

1. **Server not found**: Verify command path in config
2. **Spawn failed**: Check command and args are correct
3. **Broken pipe**: Server crashed - check server logs

### Tool Errors

1. **Unknown tool**: Tool name doesn't match server's tools
2. **Invalid params**: Check tool's input_schema
3. **Tool error**: Check is_error flag and result.text()

### Resource Errors

1. **Not found**: URI doesn't match any resource
2. **Method not supported**: Server doesn't implement resources/read

## See Also

- [MCP Specification](https://spec.modelcontextprotocol.io/)
- [Nika Workflow Patterns](./WORKFLOW-PATTERNS.md)
- [NovaNet MCP Server](../../novanet-dev/tools/novanet-mcp/)
