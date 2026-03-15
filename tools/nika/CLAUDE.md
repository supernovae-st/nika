# Nika CLI

DAG workflow runner for AI tasks with MCP integration.

**v0.27.0** | 6,532 tests | Zero clippy warnings

---

## Architecture

```
tools/nika/src/
├── main.rs           # CLI entry point (clap)
├── lib.rs            # Public API
├── error.rs          # NikaError with codes (40+ variants)
├── core/             # Zero-dep definitions
│   ├── providers.rs  # KNOWN_PROVIDERS (18: 6 LLM + 11 MCP + 1 Local)
│   ├── models.rs     # KNOWN_MODELS (16 curated for native inference)
│   └── mcp_aliases.rs # MCP_ALIASES (48 server shortcuts)
├── ast/              # Three-phase YAML parsing
│   ├── raw/          # Phase 1: YAML → Raw AST (spans preserved)
│   ├── analyzed/     # Phase 2: Raw → Analyzed (validated, TaskId interning)
│   └── lower.rs      # Phase 3: Analyzed → Runtime Workflow
├── dag/              # DAG validation + cycle detection
├── runtime/          # Execution engine
│   ├── executor.rs   # Task dispatch (5 verbs + for_each)
│   ├── runner.rs     # Workflow orchestration
│   ├── rig_agent_loop.rs # rig-core AgentBuilder
│   └── builtin/      # 12 builtin tools (7 core + 5 file)
├── mcp/              # MCP client (rmcp v0.16)
├── provider/         # LLM providers
│   ├── rig.rs        # RigProvider (6 cloud providers via rig-core v0.32)
│   └── native/       # NativeRuntime (mistral.rs for GGUF models)
├── binding/          # Data flow ({{with.alias}})
├── event/            # NDJSON trace (22 event variants)
├── secrets/          # Keychain + daemon IPC
├── tui/              # 4-view Terminal UI
└── lsp/              # Language Server Protocol (feature-gated)
```

---

## CLI Commands

```bash
# Run workflows
nika workflow.nika.yaml           # Run (positional arg)
nika run workflow.nika.yaml       # Run (explicit)
nika check workflow.nika.yaml     # Validate
nika check workflow.yaml --strict # Validate + test MCP connections

# TUI
nika ui                           # TUI (Studio view)
nika ui --view=chat               # TUI (Chat view)
nika chat                         # Chat shortcut
nika studio [file]                # Studio shortcut

# Project
nika init                         # Initialize .nika/
nika new                          # Workflow creation wizard

# Provider Management
nika provider list                # Show providers + status
nika provider set anthropic       # Store key in keychain
nika provider test openai         # Validate key

# Model Management (native inference)
nika model list                   # List local models
nika model pull qwen3:8b          # Download from HuggingFace
nika model info llama3.2:1b       # Show model details

# MCP
nika mcp add neo4j                # Add server (48 aliases)
nika mcp list                     # List configured
nika mcp tools <server>           # List available tools

# Package Management
nika pkg list                     # List installed packages
nika pkg add @spn/core            # Install package

# Traces
nika trace list                   # List execution traces
nika trace show <id>              # Display events

# Config
nika config list                  # Show all config
nika config set editor.theme dark

# Utilities
nika doctor                       # System health check
nika completion bash              # Shell completions
nika lsp                          # Start LSP server
```

---

## Providers

**6 LLM providers** via rig-core v0.32:
- `anthropic` (Claude)
- `openai` (GPT-4)
- `mistral`
- `groq`
- `deepseek`
- `gemini`

**1 Local provider** via mistral.rs:
- `native` (GGUF models)

**Auto-detection priority** (`RigProvider::auto()`):
```
ANTHROPIC_API_KEY → OpenAI → Mistral → Groq → DeepSeek → Gemini
```

---

## Verbs (5)

| Verb | Purpose | Shorthand |
|------|---------|-----------|
| `infer:` | LLM generation | `infer: "prompt"` |
| `exec:` | Shell command | `exec: "command"` |
| `fetch:` | HTTP request | - |
| `invoke:` | MCP tool call | - |
| `agent:` | Multi-turn agentic loop | - |

---

## TUI (4 Views)

| Key | View | Purpose |
|-----|------|---------|
| `1/s` | Studio | Browser + Editor + DAG Preview |
| `2/r` | Runner | Real-time execution monitoring |
| `3/c` | Chat | Conversational agent interface |
| `4/,` | Settings | Provider config, preferences |

Navigation: `Tab` cycles forward, `Shift+Tab` backward, `1-4` jump directly.

---

## Builtin Tools (12)

**Core tools (7)** - available in `invoke:`:
- `nika:sleep` - Pause execution
- `nika:log` - Emit log event
- `nika:emit` - Custom event to EventLog
- `nika:assert` - Validate condition
- `nika:prompt` - Request user input (HITL)
- `nika:run` - Execute nested workflow
- `nika:complete` - Signal agent completion

**File tools (5)** - available in `agent:` only:
- `nika:read` - Read file
- `nika:write` - Create/overwrite file
- `nika:edit` - Modify file
- `nika:glob` - Find files by pattern
- `nika:grep` - Search content

---

## Binding Syntax

Use `with:` blocks to bind task outputs:

```yaml
tasks:
  - id: step1
    infer: "Generate something"

  - id: step2
    with:
      result: step1               # Bind step1 output
    infer: "Process: {{with.result}}"
```

Template patterns:
- `{{with.alias}}` - Task output
- `{{with.data.field}}` - Nested JSON access
- `{{with.items[0]}}` - Array indexing
- `{{context.files.X}}` - Context file
- `{{inputs.param}}` - Input parameter

---

## Schema Versions

| Version | Features |
|---------|----------|
| `@0.1` | infer, exec, fetch |
| `@0.2` | +invoke, +agent, +mcp |
| `@0.3` | +for_each parallelism |
| `@0.5` | +decompose, +lazy bindings, +spawn_agent |
| `@0.6` | +multi-provider (6 providers) |
| `@0.9` | +context: file loading, +include: DAG fusion |
| `@0.10` | +three-phase AST, +analyzer validation |
| `@0.11` | +native inference (mistral.rs) |
| `@0.12` | +exec.env, +fetch.json, +inputs refs, +$inputs binding |

---

## Three-Phase AST

```
YAML Source
    ↓
[Phase 1: Parser] → RawWorkflow (marked_yaml, spans preserved)
    ↓
[Phase 2: Analyzer] → AnalyzedWorkflow (validated, TaskId interning)
    ↓
[Phase 3: lower()] → Workflow (runtime types)
    ↓
Runtime Execution
```

---

## Error Codes

| Range | Category |
|-------|----------|
| NIKA-000-009 | Workflow |
| NIKA-010-019 | Task |
| NIKA-020-029 | DAG |
| NIKA-030-039 | Provider |
| NIKA-040-049 | Binding |
| NIKA-050-059 | Security |
| NIKA-060-069 | JSON validation |
| NIKA-100-109 | MCP |
| NIKA-110-119 | Agent |
| NIKA-140-149 | AST analysis |
| NIKA-280-289 | Artifacts |

---

## Key Files

| Path | Purpose |
|------|---------|
| `src/core/providers.rs` | KNOWN_PROVIDERS (18) |
| `src/core/models.rs` | KNOWN_MODELS (16) |
| `src/core/mcp_aliases.rs` | MCP_ALIASES (48) |
| `src/ast/lower.rs` | Three-phase lowering |
| `src/runtime/executor.rs` | Task dispatch |
| `src/runtime/rig_agent_loop.rs` | Agent execution |
| `src/tui/views/mod.rs` | TuiView enum (4 views) |
| `schemas/nika-workflow.schema.json` | JSON Schema |

---

## Testing

```bash
cargo test                    # Run 6,532 tests
cargo test --features lsp     # Include LSP tests
cargo clippy -- -D warnings   # Lint
cargo fmt                     # Format
```

**Test types:**
- Unit tests: In-file `#[cfg(test)]` modules
- Integration tests: `tests/` directory
- Snapshot tests: insta for YAML/JSON
- Property tests: proptest for parser fuzzing

---

## Conventions

- **Imports:** Group by std, external, internal
- **Error handling:** Use `NikaError` with codes, not `anyhow`
- **Logging:** `tracing` macros (debug!, info!, warn!, error!)
- **Tests:** TDD preferred - write failing test first
- **Commits:** Conventional commits with scope
