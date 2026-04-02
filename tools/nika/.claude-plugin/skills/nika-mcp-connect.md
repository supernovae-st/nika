---
name: nika-mcp-connect
description: Set up and test MCP server connections for Nika workflows. Configure .mcp.json (preferred) or .nika/mcp.yaml, add servers by alias, test connectivity, and troubleshoot connection issues.
user-invocable: true
allowed-tools: Bash, Read, Write, Edit, Glob, Grep
argument-hint: "[add <server> | test <server> | list | configure | novanet]"
---

# Nika MCP Connect

> Set up MCP server connections for Nika workflows.

## Process

### Step 1: Assess Current MCP State

```bash
# Check project .mcp.json (Claude Code convention — preferred)
cat .mcp.json 2>/dev/null || echo "No .mcp.json at project root"

# Check global MCP config
cat ~/.nika/mcp.yaml 2>/dev/null || echo "No global config at ~/.nika/mcp.yaml"

# Check legacy project MCP config
cat .nika/mcp.yaml 2>/dev/null || echo "No legacy project config at .nika/mcp.yaml"

# List configured servers via CLI
nika mcp list 2>/dev/null || echo "nika mcp list failed"

# Check available aliases
nika mcp aliases 2>/dev/null | head -30
```

### Step 2: Determine What the User Needs

Based on the argument or conversation:

#### `add <server>` — Add a new MCP server

```bash
# Using a built-in alias (100 available)
nika mcp add neo4j
nika mcp add perplexity
nika mcp add github
nika mcp add firecrawl
nika mcp add filesystem

# With custom options
nika mcp add neo4j --global  # Add to ~/.nika/mcp.yaml
nika mcp add custom-api --command "npx" --args "-y @custom/server"
```

#### `test <server>` — Test an MCP server connection

You need a workflow file that declares the MCP server:

```bash
# Test server connectivity
nika mcp test workflow.nika.yaml server_name

# List available tools on the server
nika mcp tools workflow.nika.yaml server_name
```

#### `list` — Show all configured servers

```bash
nika mcp list              # All levels
nika mcp list --global     # Global only
nika mcp list --project    # Project only
nika mcp list -w file.yaml # Workflow-specific
```

#### `configure` — Interactive configuration

Guide through creating or editing MCP config:

```json
// .mcp.json (project-level — Claude Code convention, preferred)
{
  "mcpServers": {
    "novanet": {
      "command": "cargo",
      "args": ["run", "--manifest-path", "../novanet/tools/novanet-mcp/Cargo.toml"],
      "env": {
        "NEO4J_URI": "bolt://localhost:7687",
        "NEO4J_PASSWORD": "${NEO4J_PASSWORD}"
      }
    },
    "perplexity": {
      "command": "npx",
      "args": ["-y", "perplexity-mcp"],
      "env": {
        "PERPLEXITY_API_KEY": "${PERPLEXITY_API_KEY}"
      }
    }
  }
}
```

#### `novanet` — NovaNet-specific setup

```bash
# Check NovaNet binary
ls ../novanet/tools/novanet-mcp/Cargo.toml 2>/dev/null || echo "NovaNet MCP not found"

# Check Neo4j
curl -s http://localhost:7474 2>/dev/null && echo "Neo4j: running" || echo "Neo4j: not running"

# Add NovaNet to project config
nika mcp add novanet --command cargo --args "run,--manifest-path,../novanet/tools/novanet-mcp/Cargo.toml"
```

### Step 3: Workflow Integration

After adding a server, show how to use it in a workflow:

```yaml
schema: nika/workflow@0.12
workflow: mcp-example

# Reference the server (from project/global config)
mcp:
  server_name: {}  # Uses config from .nika/mcp.yaml

# Or define inline
mcp:
  custom:
    command: npx
    args: ["-y", "@package/server"]
    env:
      API_KEY: "${API_KEY}"

tasks:
  # invoke: verb for tool calls
  - id: call_tool
    invoke:
      mcp: server_name
      tool: tool_name
      params:
        key: value

  # agent: verb for autonomous tool use
  - id: research
    agent:
      prompt: "Research this topic"
      mcp: [server_name, other_server]
      max_turns: 10
```

### Step 4: Verify Connection

```bash
# Create a minimal test workflow
cat > /tmp/test-mcp.nika.yaml << 'EOF'
schema: nika/workflow@0.12
workflow: mcp-test

mcp:
  <server_name>:
    command: <command>
    args: [<args>]

tasks:
  - id: test_connection
    invoke:
      mcp: <server_name>
      tool: <tool_name>
      params: {}
EOF

# Validate
nika check /tmp/test-mcp.nika.yaml --strict

# Clean up
rm /tmp/test-mcp.nika.yaml
```

## MCP Server Alias Categories

| Category | Count | Examples |
|----------|-------|---------|
| Anthropic Official | 8 | filesystem, memory, puppeteer, brave-search |
| Databases | 8 | neo4j, postgres, mysql, sqlite, mongodb |
| Search & Web | 8 | perplexity, firecrawl, brave-search, exa |
| Developer Tools | 8 | github, gitlab, linear, sentry |
| Productivity | 8 | slack, google-drive, notion |
| AI & Specialized | 8 | langchain, e2b, sequential-thinking |
| Image & Media | varies | replicate, stability |
| Communication | varies | discord, telegram |

Full list: `nika mcp aliases`

## Common Issues

| Issue | Cause | Fix |
|-------|-------|-----|
| "Server failed to start" | Binary not found | Check `which <command>` |
| "Tool not found" | Wrong tool name | Run `nika mcp tools` to list available |
| "Connection timeout" | Server crashed on startup | Check server logs, env vars |
| "Permission denied" | Missing API key | Set `${VAR}` in env |
| "Server exits immediately" | Incompatible transport | Ensure server supports stdio |

## Claude Code MCP Integration

This plugin also configures Nika as an MCP server for Claude Code itself.
See `.mcp.json` in the plugin directory. When `nika mcp serve` is available,
Claude Code can directly call Nika tools (check, run, validate) via MCP.

## Rules

- ALWAYS test connectivity after adding a server
- NEVER hardcode API keys in config files (use `${VAR}` syntax)
- PREFER .mcp.json at project root (Claude Code convention) over .nika/mcp.yaml
- VERIFY the server supports stdio transport
- USE aliases when available (cleaner than full package names)
