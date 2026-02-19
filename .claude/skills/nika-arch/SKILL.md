---
name: nika-arch
description: Display Nika architecture diagram and module structure
---

# Nika Architecture

## Module Structure

```
tools/nika/src/
├── main.rs           # CLI entry (run, validate, tui, trace)
├── lib.rs            # Public API
├── error.rs          # NikaError (40+ variants, NIKA-XXX codes)
│
├── ast/              # YAML Parsing
│   ├── workflow.rs   # Workflow, Task structs
│   ├── action.rs     # TaskAction enum (5 variants)
│   ├── invoke.rs     # InvokeParams
│   └── agent.rs      # AgentParams
│
├── dag/              # DAG Validation
│   ├── graph.rs      # Petgraph-based DAG
│   └── validator.rs  # Cycle detection
│
├── runtime/          # Execution Engine
│   ├── executor.rs   # Task dispatch (5 verbs)
│   ├── runner.rs     # Workflow orchestration
│   └── agent_loop.rs # Agentic multi-turn (v0.2)
│
├── mcp/              # MCP Client (v0.2)
│   ├── client.rs     # McpClient, connect, call_tool
│   └── types.rs      # McpConfig, ToolCallResult
│
├── provider/         # LLM Providers
│   ├── mod.rs        # Provider trait
│   ├── claude.rs     # Anthropic API
│   └── openai.rs     # OpenAI API
│
├── event/            # Observability
│   ├── log.rs        # EventLog (16 variants)
│   └── trace.rs      # NDJSON writer
│
├── resilience/       # Fault Tolerance (v0.2)
│   ├── retry.rs      # Exponential backoff
│   ├── circuit_breaker.rs  # Closed/Open/HalfOpen
│   ├── rate_limiter.rs     # Token bucket
│   └── metrics.rs    # Performance stats
│
├── tui/              # Terminal UI (feature-gated)
│   ├── app.rs        # State machine
│   ├── ui.rs         # 4-panel renderer
│   └── panels/       # Individual panels
│
├── binding/          # Data Flow
│   ├── template.rs   # {{use.alias}} resolution
│   └── wiring.rs     # use: block processing
│
└── store/            # DataStore
    └── data_store.rs # Task result storage
```

## Data Flow

```
┌─────────────────────────────────────────────────────────────────┐
│                        NIKA RUNTIME                              │
├─────────────────────────────────────────────────────────────────┤
│                                                                 │
│  YAML Workflow                                                  │
│       │                                                         │
│       ▼                                                         │
│  ┌─────────┐     ┌───────────┐     ┌──────────────┐            │
│  │   AST   │ ──► │    DAG    │ ──► │   EXECUTOR   │            │
│  │ Parser  │     │ Validator │     │              │            │
│  └─────────┘     └───────────┘     └──────┬───────┘            │
│                                           │                     │
│                    ┌──────────────────────┼──────────────────┐  │
│                    │                      │                  │  │
│                    ▼                      ▼                  ▼  │
│              ┌─────────┐           ┌──────────┐        ┌─────┐ │
│              │ PROVIDER│           │   MCP    │        │EXEC │ │
│              │(Claude) │           │ CLIENT   │        │/FETCH││
│              └─────────┘           └────┬─────┘        └─────┘ │
│                    │                    │                       │
│                    ▼                    ▼                       │
│              ┌─────────┐           ┌──────────┐                │
│              │Anthropic│           │ NovaNet  │                │
│              │   API   │           │MCP Server│                │
│              └─────────┘           └──────────┘                │
│                                                                 │
└─────────────────────────────────────────────────────────────────┘
```

## 5 Semantic Verbs

| Verb | Module | Purpose |
|------|--------|---------|
| `infer:` | executor.rs | LLM text generation |
| `exec:` | executor.rs | Shell command |
| `fetch:` | executor.rs | HTTP request |
| `invoke:` | mcp/client.rs | MCP tool call |
| `agent:` | agent_loop.rs | Multi-turn agentic |

## Event Flow

```
Task Start ──► EventLog ──► NDJSON File
                 │
                 ├── WorkflowStarted
                 ├── TaskStarted
                 ├── InferStarted / InferCompleted
                 ├── McpToolCalled / McpToolResponded
                 ├── AgentTurnStarted / AgentTurnCompleted
                 ├── TaskCompleted / TaskFailed
                 └── WorkflowCompleted
```

## Key Files

| File | Purpose |
|------|---------|
| `src/error.rs` | All error codes (NIKA-000 to NIKA-119) |
| `src/ast/action.rs` | TaskAction enum definition |
| `src/runtime/executor.rs` | Main task dispatch logic |
| `src/mcp/client.rs` | MCP connection and tool calling |

## Related Skills

- `/nika-run` — Execute workflows
- `/nika-diagnose` — Debug failing workflows
- `/nika-binding` — Data binding syntax
