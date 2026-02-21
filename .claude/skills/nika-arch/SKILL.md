---
name: nika-arch
description: Display Nika architecture diagram and module structure
---

# Nika Architecture (v0.5.2)

## Module Structure

```
tools/nika/src/
├── main.rs           # CLI entry (run, check, tui, trace, chat, studio)
├── lib.rs            # Public API
├── error.rs          # NikaError (40+ variants, NIKA-XXX codes)
│
├── ast/              # YAML Parsing
│   ├── workflow.rs   # Workflow, Task structs
│   ├── action.rs     # TaskAction enum (5 variants)
│   ├── invoke.rs     # InvokeParams
│   ├── agent.rs      # AgentParams with stop_conditions
│   ├── decompose.rs  # DecomposeSpec (v0.5 MVP 8)
│   └── output.rs     # OutputSpec
│
├── dag/              # DAG Validation
│   ├── graph.rs      # Petgraph-based DAG
│   └── validator.rs  # Cycle detection
│
├── runtime/          # Execution Engine
│   ├── executor.rs   # Task dispatch (5 verbs + for_each)
│   ├── runner.rs     # Workflow orchestration
│   ├── output.rs     # Output format handling
│   ├── spawn.rs      # SpawnAgentTool (v0.5 nested agents)
│   └── rig_agent_loop.rs  # RigAgentLoop with rig::AgentBuilder
│
├── mcp/              # MCP Client (rmcp v0.16)
│   ├── client.rs     # McpClient, connect, call_tool
│   └── types.rs      # McpConfig, ToolCallResult
│
├── provider/         # LLM Providers (rig-core v0.31 only)
│   └── rig.rs        # RigProvider + NikaMcpTool (ToolDyn)
│
├── event/            # Observability
│   ├── log.rs        # EventLog (20 variants)
│   └── trace.rs      # NDJSON writer
│
├── tui/              # Terminal UI (4-view architecture)
│   ├── app.rs        # State machine + Tab navigation
│   ├── theme.rs      # Color palette
│   ├── views/        # 4 main views
│   │   ├── chat.rs   # Chat view (agent conversation)
│   │   ├── home.rs   # Home view (workflow browser)
│   │   ├── studio.rs # Studio view (YAML editor)
│   │   └── monitor.rs# Monitor view (system health)
│   └── widgets/      # Reusable components
│       ├── dag.rs    # DAG visualization
│       ├── agent_turns.rs  # Agent turn history
│       ├── session_context.rs # Token/cost bar
│       ├── mcp_call_box.rs    # MCP call display
│       ├── infer_stream_box.rs# Streaming inference
│       ├── activity_stack.rs  # Hot/warm/cold tasks
│       └── command_palette.rs # ⌘K fuzzy search
│
├── binding/          # Data Flow ({{use.alias}})
│   ├── entry.rs      # UseEntry with lazy flag (v0.5)
│   └── resolve.rs    # LazyBinding resolution
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

| Verb | Module | Purpose |
|------|--------|---------|
| `infer:` | executor.rs | LLM text generation (shorthand: `infer: "prompt"`) |
| `exec:` | executor.rs | Shell command (shorthand: `exec: "command"`) |
| `fetch:` | executor.rs | HTTP request |
| `invoke:` | mcp/client.rs | MCP tool call |
| `agent:` | rig_agent_loop.rs | Multi-turn agentic with spawn_agent |

## Event Flow (20 variants)

```
Task Start ──► EventLog ──► NDJSON File
                 │
                 ├── WorkflowStarted
                 ├── TaskStarted
                 ├── InferStarted / InferCompleted
                 ├── McpToolCalled / McpToolResponded
                 ├── AgentTurnStarted / AgentTurnCompleted
                 ├── AgentSpawned / AgentSpawnedCompleted
                 ├── ThinkingStarted / ThinkingCompleted
                 ├── TaskCompleted / TaskFailed
                 └── WorkflowCompleted
```

## TUI Architecture (v0.5.2)

```
┌────────────────────────────────────────────────────────────────┐
│  [Chat]  [Home]  [Studio]  [Monitor]    ← Tab navigation       │
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
| `src/error.rs` | All error codes (NIKA-000 to NIKA-119) |
| `src/ast/action.rs` | TaskAction enum definition |
| `src/runtime/executor.rs` | Main task dispatch logic |
| `src/runtime/rig_agent_loop.rs` | Agent loop with rig-core |
| `src/runtime/spawn.rs` | Nested agent spawning (MVP 8) |
| `src/mcp/client.rs` | MCP connection and tool calling |

## Related Skills

- `/nika-run` — Execute workflows
- `/nika-diagnose` — Debug failing workflows
- `/nika-binding` — Data binding syntax
- `/nika-spec` — YAML syntax reference
