# Nika v0.8.0 Architecture

**Native Intelligence Kernel Agent**

---

## Overview

Nika is a DAG workflow runner for AI tasks that connects to knowledge graphs via MCP.

```
┌─────────────────────────────────────────────────────────────────┐
│                         NIKA v0.8.0                              │
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
│  │    Executor     │  ← Parallel task execution (for_each)      │
│  └─────────────────┘                                            │
│       │                                                         │
│       ├── infer  → LLM (6 providers: Claude, OpenAI, etc)       │
│       ├── exec   → Shell                                        │
│       ├── fetch  → HTTP Client                                  │
│       ├── invoke → MCP Client (tools + resources)               │
│       └── agent  → Agentic Loop + Full Streaming                │
│       │                                                         │
│       ▼                                                         │
│  ┌─────────────────┐                                            │
│  │   RunContext     │  ← Task results for downstream             │
│  └─────────────────┘                                            │
│                                                                 │
│  TUI: 4 Views (Chat, Home, Studio, Monitor)                    │
│       Full Streaming + Real-time Trace Visualization           │
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
│   └── validate.rs   # Cycle detection, depends_on resolution
│
├── runtime/          # Execution engine
│   ├── runner.rs     # Workflow runner
│   ├── executor.rs   # Task executor
│   ├── agent_loop.rs # Agentic execution (NEW)
│   └── output.rs     # Output processing
│
├── binding/          # Data flow
│   ├── entry.rs      # BindingEntry, BindingSpec
│   ├── template.rs   # {{with.alias}} substitution
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

## Workflow Schema v0.12

```yaml
schema: "nika/workflow@0.12"
provider: claude

# MCP server configurations
mcp:
  servers:
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
        entity: "qr-code"
        locale: "fr-FR"
        forms: ["text", "title"]

  - id: generate_locales
    depends_on: [context]
    for_each: ["en-US", "fr-FR", "de-DE"]
    as: locale
    invoke:
      mcp: novanet
      tool: novanet_generate
      params:
        entity: "qr-code"
        locale: "{{with.locale}}"

  - id: synthesize
    depends_on: [context, generate_locales]
    with:
      ctx: context
      results: generate_locales
    agent:
      prompt: "Synthesize {{with.results}} using context {{with.ctx}}"
      mcp: [novanet]
      max_turns: 10
      tool_choice: auto
```

---

## Execution Flow

```
1. Parse YAML workflow
2. Build DAG from depends_on declarations
3. Validate (cycles, missing deps)
4. Connect MCP servers
5. Execute tasks in topological order:
   - Parallel when no dependencies
   - Wait for upstream completion
   - Substitute {{with.alias}} templates
6. Store results in RunContext
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

## v0.8.0 Features (Complete)

- `for_each:` - Parallel iteration with concurrency control ✅
- `spawn_agent` - Nested agents with depth limits ✅
- `decompose:` - Runtime DAG expansion ✅
- `lazy:` bindings - Deferred context loading ✅
- Full streaming - All 6 providers with real-time token delivery ✅
- TUI 4-view architecture - Chat, Home, Studio, Monitor ✅
- Edit history - Undo/Redo in Studio with intelligent coalescing ✅
- Session persistence - Auto-save chat conversations ✅
- Config system - `.nika/config.toml` for user preferences ✅
- Solarized theme - Third theme option alongside Default and Custom ✅

## Future (v0.9+)

- `guard:` - Conditional execution
- `output.schema:` - JSON Schema validation with runtime checks
- `invoke.prompt:` - MCP prompt templates
- Manifest file (`nika.yaml`) for multi-workflow projects
- Native `.rmcp_tools()` via rmcp version upgrade
- Mouse selection and clipboard integration in TUI
