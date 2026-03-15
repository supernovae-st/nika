# Nika

Semantic YAML workflow engine for AI tasks — DAG execution, MCP integration, multi-provider LLM support.

**Version**: v0.27.0 | **Tests**: 6,264 | **Target**: [QR Code AI](https://qrcode-ai.com)

---

## Why Nika Exists

AI workflows buried in code are untraceable, non-reproducible, and hard to debug. Nika executes YAML-defined DAG workflows with 5 semantic verbs, providing full observability via NDJSON traces. Workflows become version-controlled, human-readable artifacts.

---

## Architecture

```
nika/
├── tools/nika/          # Rust binary (all source code)
│   ├── src/
│   │   ├── ast/         # Three-phase: raw → analyzed → lower
│   │   ├── core/        # Provider/model/MCP definitions
│   │   ├── dag/         # DAG validation
│   │   ├── runtime/     # Execution engine
│   │   ├── mcp/         # MCP client (rmcp v0.16)
│   │   ├── provider/    # rig-core v0.32 + native (mistral.rs)
│   │   ├── tui/         # Terminal UI (ratatui)
│   │   ├── binding/     # Data flow ({{with.alias}})
│   │   ├── event/       # NDJSON trace writer
│   │   └── secrets/     # Keychain + daemon IPC
│   ├── CLAUDE.md        # Detailed tool context
│   └── Cargo.toml
└── docs/                # Plans + research
```

---

## 5 Semantic Verbs

| Verb | Purpose | Example |
|------|---------|---------|
| `infer:` | LLM generation | `infer: "Generate a headline"` |
| `exec:` | Shell command | `exec: "npm run build"` |
| `fetch:` | HTTP request | `fetch: { url: "...", method: "GET" }` |
| `invoke:` | MCP tool call | `invoke: { tool: "novanet_search", mcp: "novanet" }` |
| `agent:` | Multi-turn loop | `agent: { prompt: "Research...", mcp: [...] }` |

---

## 7 Inference Backends

| Provider | Type | Model Examples |
|----------|------|----------------|
| `anthropic` | Cloud | Claude Opus, Sonnet, Haiku |
| `openai` | Cloud | GPT-4, GPT-4o |
| `mistral` | Cloud | Mistral Large, Medium |
| `groq` | Cloud | Llama, Mixtral (fast) |
| `deepseek` | Cloud | DeepSeek Chat, Coder |
| `gemini` | Cloud | Gemini 2.0 Flash |
| `native` | Local | GGUF models via mistral.rs |

---

## 4 TUI Views

| View | Key | Purpose |
|------|-----|---------|
| Studio | `1` / `s` | Browser + YAML editor + DAG preview |
| Runner | `2` / `r` | Real-time execution monitoring |
| Chat | `3` / `c` | Conversational agent interface |
| Settings | `4` / `,` | Provider config, theme, preferences |

---

## Commands

```bash
# Workflow execution
nika workflow.nika.yaml          # Run workflow (positional)
nika run workflow.nika.yaml      # Run workflow (explicit)
nika check workflow.nika.yaml    # Validate syntax + DAG
nika check file.yaml --strict    # + test MCP connections

# TUI
nika ui                          # Launch TUI (Studio view)
nika chat                        # Chat view directly
nika studio [file]               # Studio with file

# Project
nika init                        # Initialize .nika/ directory
nika new                         # Create workflow from template

# Traces
nika trace list                  # List execution traces
nika trace show <id>             # Display trace events

# Provider management
nika provider list               # Show providers + API key status
nika provider set anthropic      # Store key in OS keychain
nika provider test claude        # Test connection
nika provider migrate            # Move env vars to keychain

# MCP management
nika mcp list -w workflow.yaml   # List servers in workflow
nika mcp test workflow.yaml srv  # Test server connection
nika mcp tools workflow.yaml srv # List available tools

# Model management (native inference)
nika model list                  # List local models
nika model pull llama3.2:1b      # Download from HuggingFace
nika model info qwen3:8b         # Show model details

# Package management
nika pkg list                    # List installed packages
nika pkg add @spn/core           # Add package from registry
nika pkg search seo              # Search registry

# Config
nika config list                 # Show all config values
nika config set editor.theme dark

# Development
nika doctor                      # Check system health
nika schema list                 # List schema versions
nika completion zsh              # Generate shell completions
```

---

## Workflow Syntax

```yaml
schema: nika/workflow@0.12
workflow: example
provider: anthropic

tasks:
  - id: step1
    infer: "Generate a title for an AI blog post"

  - id: step2
    with:
      title: step1                # Bind step1 output to 'title'
    infer: "Write intro for: {{with.title}}"
    depends_on: [step1]
```

**Key syntax**: Use `with:` for bindings, `{{with.alias}}` for templates.

---

## Integration with NovaNet

Nika connects to NovaNet via MCP (never direct Neo4j):

```yaml
schema: nika/workflow@0.12
workflow: generate-content

mcp:
  servers:
    novanet:
      command: cargo
      args: ["run", "--manifest-path", "../novanet/tools/novanet-mcp/Cargo.toml"]

tasks:
  - id: get_context
    invoke:
      mcp: novanet
      tool: novanet_context
      params:
        focus_key: "qr-code"
        locale: "fr-FR"
        mode: "page"
    with.ctx: context

  - id: generate
    with:
      ctx: $get_context
    infer: "Generate landing page using: {{with.ctx}}"
```

---

## Key Files

| Path | Purpose |
|------|---------|
| `tools/nika/CLAUDE.md` | Detailed tool context |
| `tools/nika/src/ast/action.rs` | 5 verbs definition |
| `tools/nika/src/core/providers.rs` | 18 providers (6 LLM + 11 MCP + 1 Local) |
| `tools/nika/src/runtime/executor.rs` | Task dispatch |
| `tools/nika/src/provider/rig.rs` | RigProvider + NikaMcpTool |
| `tools/nika/schemas/nika-workflow.schema.json` | JSON Schema for validation |

---

## Conventions

| Aspect | Convention |
|--------|------------|
| File extension | `.nika.yaml` |
| Schema version | `nika/workflow@0.12` |
| Binding syntax | `with:` block + `{{with.alias}}` |
| Provider selection | Auto-detect from env vars or explicit |
| Error codes | `NIKA-XXX` (see `src/error.rs`) |
| Tests | TDD preferred, 80% coverage |
| Commits | `type(scope): description` |
