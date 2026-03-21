# Nika Architecture

Schema `nika/workflow@0.12` | v0.36.0

---

## Overview

```
┌─────────────────────────────────────────────────────────────────┐
│                         NIKA v0.36.0                            │
├─────────────────────────────────────────────────────────────────┤
│                                                                 │
│  .nika.yaml                                                     │
│       │                                                         │
│       ▼                                                         │
│  ┌─────────────────┐                                            │
│  │ Three-Phase AST │  Raw → Analyzed → Lowered                  │
│  └─────────────────┘                                            │
│       │                                                         │
│       ▼                                                         │
│  ┌─────────────────┐                                            │
│  │   IndexedDag    │  Kahn's topological sort                   │
│  └─────────────────┘                                            │
│       │                                                         │
│       ▼                                                         │
│  ┌─────────────────┐                                            │
│  │    Executor     │  Parallel task execution                   │
│  └─────────────────┘                                            │
│       │                                                         │
│       ├── infer  → LLM (8 providers via rig-core)               │
│       ├── exec   → Shell (blocklist + NFKC)                     │
│       ├── fetch  → HTTP (9 extract modes)                       │
│       ├── invoke → MCP Client (rmcp 0.16)                       │
│       └── agent  → Agentic Loop (guardrails + completion)       │
│       │                                                         │
│       ▼                                                         │
│  ┌─────────────────┐                                            │
│  │   RunContext    │  Task results + CAS media                   │
│  └─────────────────┘                                            │
│                                                                 │
│  TUI: 3 Views (Studio, Command, Control)                        │
│  LSP: 12 handlers (nika-lsp-core + nika-lsp)                    │
│  43 builtin tools | 41 event types | NDJSON traces              │
│                                                                 │
└─────────────────────────────────────────────────────────────────┘
```

---

## Crates

```
nika/
├── tools/nika/          # CLI binary + TUI + runtime (100k+ LOC)
├── tools/nika-core/     # Zero-dep AST core (binding, catalogs, spans)
├── tools/nika-lsp-core/ # Protocol-agnostic LSP intelligence
└── tools/nika-lsp/      # Standalone LSP server (tower-lsp-server 0.23)
```

## Source Tree (tools/nika/)

```
src/
├── main.rs              # CLI entry (clap)
├── lib.rs               # Public API
├── error.rs             # NikaError (NIKA-XXX codes, 50+ ranges)
├── ast/                 # Three-phase: Raw → Analyzed → Lower (40+ files)
│   ├── raw/             #   Phase 1: YAML → Raw AST (spans)
│   ├── analyzed/        #   Phase 2: Validated, resolved
│   ├── analyzer/        #   Validation + transformation
│   └── lower.rs         #   Phase 3: → Runtime types
├── dag/                 # DAG validation + cycle detection
│   ├── flow.rs          #   Immutable HashMap DAG
│   ├── indexed.rs       #   Vec adjacency + Kahn's algorithm
│   └── stable.rs        #   petgraph StableGraph (TUI)
├── runtime/             # Execution engine
│   ├── runner.rs        #   Workflow orchestration
│   ├── executor/        #   5 verb dispatch + decompose
│   ├── rig_agent_loop/  #   Agent loop (chat, streaming, thinking)
│   ├── builtin/         #   43 tools (12 core + 26 media + 5 file)
│   └── security.rs      #   Command blocklist + env validation
├── mcp/                 # MCP client pool (rmcp 0.16)
├── provider/            # 8 providers (rig-core + mistral.rs native)
├── binding/             # Templates, transforms (27), JSONPath
├── event/               # 41 event types + NDJSON tracing
├── media/               # CAS store (blake3 + zstd)
├── display/             # CLI rendering (summary, dag_render, colors)
├── tui/                 # Terminal UI (3 views, 40+ widgets)
├── lsp/                 # Embedded LSP (feature-gated)
├── cli/                 # Subcommands
├── init/                # 30 templates (6 tiers)
├── secrets/             # Keyring + daemon IPC
└── tools/               # File tools (read, write, edit, glob, grep)
```

---

## 5 Semantic Verbs

| Verb | Purpose | Key Features |
|------|---------|--------------|
| `infer:` | LLM generation | Vision, extended thinking, structured output, guardrails |
| `exec:` | Shell command | Blocklist, timeout, NFKC normalization |
| `fetch:` | HTTP + extraction | 9 modes: markdown, article, jsonpath, feed, etc. |
| `invoke:` | MCP tool call | Schema validation, retry, connection pooling |
| `agent:` | Multi-turn loop | Guardrails, completion modes, stop_sequences, HITL |

---

## Key Architectural Decisions

### Three-Phase AST Pipeline

```
YAML → Raw (spans) → Analyzed (validated) → Lowered (runtime types)
```

rustc-inspired. Each phase has pure guarantees. No validation after Lower.

### Immutable DAG

After construction, the dependency graph is frozen. Enables safe concurrent execution via tokio.

### Content-Addressable Storage (CAS)

blake3 hashing, zstd compression, reflink-copy. Media never duplicated. 26 tools operate on CAS hashes.

### Structured Output (5-Layer Defense)

0: Provider-native (DynamicSubmitTool) → 1: Extractor → 2: Extract+Validate → 3: Retry → 4: LLM repair

### Event Sourcing

41 event types, NDJSON traces, append-only EventLog. Full replay capability for debugging.

### Zero Cypher

Nika never talks to databases directly. All graph access goes through MCP `invoke:`.

---

## Providers

| Provider | Via | Features |
|----------|-----|----------|
| Claude | rig-core | Extended thinking, vision, streaming |
| OpenAI | rig-core | Vision, structured output |
| Mistral | rig-core | Streaming, tool calling |
| Groq | rig-core | High-speed inference |
| DeepSeek | rig-core | Reasoning, streaming |
| Gemini | rig-core | Vision, multimodal |
| xAI | rig-core | Streaming, tool calling |
| Native | mistral.rs | Local GGUF + VisionHf, offline |

---

## Dependencies

| Crate | Purpose |
|-------|---------|
| rig-core 0.32 | LLM provider abstraction |
| rmcp 0.16 | MCP client (stdio transport) |
| mistral.rs 0.7 | Local GGUF inference |
| ratatui 0.30 | Terminal UI framework |
| petgraph | DAG (StableGraph) |
| tree-sitter | YAML syntax highlighting |
| serde-saphyr | YAML parsing (with bomb protection) |
| blake3 | CAS hashing |
| reqwest | HTTP client |
| tokio | Async runtime |

---

## Agent Loop

```
┌─────────────────────────────────────────┐
│             AGENT LOOP                  │
├─────────────────────────────────────────┤
│                                         │
│  1. Initial prompt + skills injection   │
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
│       ├── Guardrails (length/schema/    │
│       │    regex/LLM validation)        │
│       ├── Completion mode (explicit/    │
│       │    natural/pattern)             │
│       ├── stop_sequences matched?       │
│       ├── Confidence threshold?         │
│       └── max_turns / token_budget?     │
│       │                                 │
│       ├── Met? → Return result          │
│       └── Not met? → Continue loop      │
│                                         │
└─────────────────────────────────────────┘
```

---

## NovaNet Integration

```
NIKA WORKFLOW ──invoke:──> NOVANET MCP ──Cypher──> NEO4J
```

Zero Cypher in workflow YAML. NovaNet MCP provides semantic tools.
