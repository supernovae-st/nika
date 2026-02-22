# Nika v0.2 Architecture

**Native Intelligence Kernel Agent**

---

## Overview

Nika is a DAG workflow runner for AI tasks that connects to knowledge graphs via MCP.

```
┌─────────────────────────────────────────────────────────────────┐
│                         NIKA v0.2                               │
├─────────────────────────────────────────────────────────────────┤
│                                                                 │
│  YAML Workflow                                                  │
│       │                                                         │
│       ▼                                                         │
│  ┌─────────────────┐                                            │
│  │   DAG Builder   │  ← Validate dependencies                   │
│  └─────────────────┘                                            │
│       │                                                         │
│       ▼                                                         │
│  ┌─────────────────┐                                            │
│  │    Executor     │  ← Parallel task execution                 │
│  └─────────────────┘                                            │
│       │                                                         │
│       ├── infer  → LLM Provider (Claude/OpenAI)                 │
│       ├── exec   → Shell                                        │
│       ├── fetch  → HTTP Client                                  │
│       ├── invoke → MCP Client (NEW v0.2)                        │
│       └── agent  → Agent Loop + MCP (NEW v0.2)                  │
│       │                                                         │
│       ▼                                                         │
│  ┌─────────────────┐                                            │
│  │   DataStore     │  ← Task results for downstream             │
│  └─────────────────┘                                            │
│                                                                 │
└─────────────────────────────────────────────────────────────────┘
```

---

## 5 Semantic Verbs

| Verb | Purpose | v0.1 | v0.2 |
|------|---------|------|------|
| `infer:` | LLM inference | ✓ | ✓ |
| `exec:` | Shell command | ✓ | ✓ |
| `fetch:` | HTTP request | ✓ | ✓ |
| `invoke:` | MCP tool/resource | - | NEW |
| `agent:` | Agentic loop with MCP | - | NEW |

---

## Module Structure

```
src/
├── main.rs           # CLI entry point
├── lib.rs            # Library exports
├── error.rs          # NikaError enum
│
├── ast/              # Domain model
│   ├── workflow.rs   # Workflow, Task
│   ├── action.rs     # TaskAction (5 variants)
│   ├── infer.rs      # InferParams
│   ├── exec.rs       # ExecParams
│   ├── fetch.rs      # FetchParams
│   ├── invoke.rs     # InvokeParams (NEW)
│   └── agent.rs      # AgentParams (NEW)
│
├── dag/              # DAG validation
│   ├── flow.rs       # Flow, FlowEndpoint
│   └── validate.rs   # Cycle detection
│
├── runtime/          # Execution engine
│   ├── runner.rs     # Workflow runner
│   ├── executor.rs   # Task executor
│   ├── agent_loop.rs # Agentic execution (NEW)
│   └── output.rs     # Output processing
│
├── binding/          # Data flow
│   ├── entry.rs      # UseEntry, UseWiring
│   ├── template.rs   # {{use.alias}} substitution
│   └── resolve.rs    # Path resolution
│
├── mcp/              # MCP client (NEW)
│   ├── mod.rs        # Module entry
│   ├── client.rs     # McpClient
│   └── types.rs      # McpConfig, ToolCall, etc.
│
├── store/            # Runtime state
│   └── datastore.rs  # TaskResult storage
│
├── provider/         # LLM providers
│   ├── mod.rs        # LlmProvider trait
│   ├── types.rs      # Message, ToolDefinition (NEW)
│   ├── claude.rs     # Anthropic API
│   └── openai.rs     # OpenAI API
│
└── validation/       # Schema validation
    └── schema.rs     # Version checking
```

---

## Key Types

### TaskAction (5 variants)

```rust
pub enum TaskAction {
    // v0.1
    Infer { infer: InferParams },
    Exec { exec: ExecParams },
    Fetch { fetch: FetchParams },

    // v0.2 (NEW)
    Invoke { invoke: InvokeParams },
    Agent { agent: AgentParams },
}
```

### InvokeParams

```rust
pub struct InvokeParams {
    pub mcp: String,                    // MCP server name
    pub tool: Option<String>,           // Tool to call
    pub params: Option<serde_json::Value>,
    pub resource: Option<String>,       // Resource URI to read
}
```

### AgentParams

```rust
pub struct AgentParams {
    pub prompt: String,
    pub provider: Option<String>,
    pub model: Option<String>,
    pub mcp: Vec<String>,               // MCP servers to access
    pub max_turns: Option<u32>,
    pub stop_conditions: Vec<String>,
    pub scope: Option<String>,
}
```

### McpClient

```rust
pub struct McpClient {
    config: McpConfig,
    service: Arc<RwLock<Option<rmcp::Client>>>,
}

impl McpClient {
    pub async fn call_tool(&self, name: &str, params: Value) -> Result<ToolResult>;
    pub async fn read_resource(&self, uri: &str) -> Result<ResourceContent>;
    pub async fn list_tools(&self) -> Result<Vec<ToolDefinition>>;
}
```

---

## Workflow Schema v0.2

```yaml
schema: "nika/workflow@0.2"
provider: claude

# MCP server configurations (NEW in v0.2)
mcp:
  novanet:
    command: "cargo"
    args: ["run", "-p", "novanet-mcp"]
    env:
      NEO4J_URI: "bolt://localhost:7687"

tasks:
  - id: context
    invoke:
      mcp: novanet
      tool: novanet_generate
      params:
        mode: block
        page_key: homepage
        block_key: hero
        locale: fr-FR

  - id: generate
    use:
      ctx: context
    agent:
      prompt: "Generate content using {{use.ctx}}"
      mcp: [novanet]
      max_turns: 10

flows:
  - source: context
    target: generate
```

---

## Execution Flow

```
1. Parse YAML workflow
2. Build DAG from flows
3. Validate (cycles, missing deps)
4. Connect MCP servers
5. Execute tasks in topological order:
   - Parallel when no dependencies
   - Wait for upstream completion
   - Substitute {{use.alias}} templates
6. Store results in DataStore
7. Return final outputs
```

---

## Agent Loop

```
┌─────────────────────────────────────────┐
│             AGENT LOOP                  │
├─────────────────────────────────────────┤
│                                         │
│  1. Initial prompt                      │
│       │                                 │
│       ▼                                 │
│  2. LLM response (with tool defs)       │
│       │                                 │
│       ├── Tool calls? ─────┐            │
│       │                    ▼            │
│       │             Execute via MCP     │
│       │                    │            │
│       │                    ▼            │
│       │             Add tool results    │
│       │                    │            │
│       ◄────────────────────┘            │
│       │                                 │
│  3. Check stop conditions               │
│       │                                 │
│       ├── Met? → Return result          │
│       │                                 │
│       └── Not met? → Continue loop      │
│                                         │
│  4. max_turns reached → Stop            │
│                                         │
└─────────────────────────────────────────┘
```

---

## Dependencies

| Crate | Purpose |
|-------|---------|
| clap | CLI argument parsing |
| tokio | Async runtime |
| serde | Serialization |
| serde_yaml | YAML parsing |
| reqwest | HTTP client |
| dashmap | Concurrent hashmap |
| thiserror | Error types |
| tracing | Logging |
| rmcp | MCP client (NEW v0.2) |

---

## NovaNet Integration

Nika v0.2 connects to NovaNet via MCP:

```
NIKA WORKFLOW
     │
     │ invoke: novanet_generate
     ▼
NOVANET MCP
     │
     │ Cypher queries (hidden)
     ▼
NEO4J (61 nodes, 182 arcs)
```

**Key principle**: Zero Cypher in workflow YAML. NovaNet MCP provides semantic tools.

---

## Future (v0.3+)

- `for_each:` - Parallel iteration
- `guard:` - Conditional execution
- `output.schema:` - JSON Schema validation
- `invoke.prompt:` - MCP prompt templates
- Manifest file (`nika.yaml`) for multi-workflow projects
