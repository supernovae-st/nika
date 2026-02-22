---
name: nika-arch
description: Display Nika architecture diagram and module structure
---

# Nika Architecture (v0.7.1)

## Crate Structure (6-Crate Workspace)

```
nika/
├── Cargo.toml        # Workspace manifest
├── crates/
│   ├── nika-core/    # Core types, AST, DAG, events (516 tests)
│   │   └── src/
│   │       ├── ast/      # Workflow, Task, TaskAction
│   │       ├── dag/      # DAG validation
│   │       ├── event/    # EventLog (22 variants)
│   │       ├── binding/  # Data flow ({{use.alias}})
│   │       ├── store/    # DataStore
│   │       └── error.rs  # NikaError (40+ variants)
│   │
│   ├── nika-mcp/     # MCP client (rmcp v0.16) (132 tests)
│   │   └── src/
│   │       ├── client.rs # McpClient, connect, call_tool
│   │       └── types.rs  # McpConfig, ToolCallResult
│   │
│   ├── nika-provider/# LLM providers via rig-core v0.31 (30 tests)
│   │   └── src/
│   │       └── rig.rs    # RigProvider + NikaMcpTool
│   │
│   ├── nika-runtime/ # Execution engine (177 tests)
│   │   └── src/
│   │       ├── executor.rs     # Task dispatch (5 verbs)
│   │       ├── runner.rs       # Workflow orchestration
│   │       ├── spawn.rs        # SpawnAgentTool (nested agents)
│   │       └── rig_agent_loop.rs # RigAgentLoop
│   │
│   ├── nika-tui/     # Terminal UI (ratatui) (806 tests)
│   │   └── src/
│   │       ├── app.rs    # State machine + Tab navigation
│   │       ├── views/    # Chat, Home, Studio, Monitor
│   │       └── widgets/  # DAG, AgentTurns, CommandPalette
│   │
│   └── nika-cli/     # CLI binary (662 tests)
│       └── src/
│           └── main.rs   # CLI entry point
│
├── examples/         # Workflow examples
└── schemas/          # JSON Schema for YAML validation
```

## Dependency Graph

```
nika-core ← nika-mcp ← nika-provider ← nika-runtime ← nika-tui ← nika-cli
    │           │           │               │
    │           │           │               └── Runner, Executor, AgentLoop
    │           │           └── RigProvider, NikaMcpTool
    │           └── McpClient, Protocol, Validation
    └── AST, DAG, Store, Event, Config, Error
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
│              │RigAgent │           │   MCP    │        │EXEC │ │
│              │  Loop   │           │ CLIENT   │        │/FETCH││
│              └─────────┘           └────┬─────┘        └─────┘ │
│                    │                    │                       │
│                    ▼                    ▼                       │
│              ┌─────────┐           ┌──────────┐                │
│              │rig-core │           │ NovaNet  │                │
│              │Providers│           │MCP Server│                │
│              └─────────┘           └──────────┘                │
│                                                                 │
└─────────────────────────────────────────────────────────────────┘
```

## 5 Semantic Verbs

| Verb | Crate | Purpose |
|------|-------|---------|
| `infer:` | nika-runtime | LLM text generation (shorthand: `infer: "prompt"`) |
| `exec:` | nika-runtime | Shell command (shorthand: `exec: "command"`) |
| `fetch:` | nika-runtime | HTTP request |
| `invoke:` | nika-mcp | MCP tool call |
| `agent:` | nika-runtime | Multi-turn agentic with spawn_agent |

## Event Flow (22 variants)

```
Task Start ──► EventLog ──► NDJSON File
                 │
                 ├── WorkflowStarted / WorkflowCompleted
                 ├── TaskStarted / TaskCompleted / TaskFailed
                 ├── McpConnected / McpError
                 ├── McpInvoke / McpResponse
                 ├── AgentStart / AgentTurn / AgentComplete
                 ├── AgentSpawned
                 ├── ProviderCalled / ProviderResponded
                 ├── TemplateResolved / ContextAssembled
                 └── StreamChunk (with Metrics)
```

## TUI Architecture (v0.7.1)

```
┌────────────────────────────────────────────────────────────────┐
│  [Chat]  [Home]  [Studio]  [Monitor]    ← Tab navigation       │
│  Alt+←/→ to navigate | Alt+W to close | Ctrl+P fuzzy search   │
├────────────────────────────────────────────────────────────────┤
│                                                                │
│  Chat: Agent conversation + streaming                          │
│  Home: Workflow browser + quick actions                        │
│  Studio: YAML editor + live preview                            │
│  Monitor: System health + MCP status                           │
│                                                                │
├────────────────────────────────────────────────────────────────┤
│  SessionContextBar: tokens | cost | MCP status                 │
│  CommandPalette: ⌘K fuzzy search                               │
│  ActivityStack: hot/warm/queued tasks                          │
└────────────────────────────────────────────────────────────────┘
```

## Key Files

| File | Purpose |
|------|---------|
| `crates/nika-core/src/error.rs` | All error codes (NIKA-000 to NIKA-119) |
| `crates/nika-core/src/ast/action.rs` | TaskAction enum definition |
| `crates/nika-runtime/src/executor.rs` | Main task dispatch logic |
| `crates/nika-runtime/src/rig_agent_loop.rs` | Agent loop with rig-core |
| `crates/nika-runtime/src/spawn.rs` | Nested agent spawning |
| `crates/nika-mcp/src/client.rs` | MCP connection and tool calling |

## Related Skills

- `/nika-run` — Execute workflows
- `/nika-diagnose` — Debug failing workflows
- `/nika-binding` — Data binding syntax
- `/nika-spec` — YAML syntax reference
