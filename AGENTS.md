# Nika

Semantic YAML workflow engine for AI tasks. Schema `nika/workflow@0.12` | [QR Code AI](https://qrcode-ai.com)

## 5 Verbs

| Verb | Purpose |
|------|---------|
| `infer:` | LLM generation |
| `exec:` | Shell command |
| `fetch:` | HTTP request |
| `invoke:` | MCP tool call |
| `agent:` | Multi-turn loop |

## Workflow Syntax

`with:` for bindings, `{{with.alias}}` for templates, `.nika.yaml` extension.

## Integration with NovaNet

Nika connects to NovaNet via MCP only (Zero Cypher rule). Use `invoke:` verb.

## TUI Views

`1/s` Studio | `2/c` Command | `3/x` Control

## Commands

```bash
# Workflows
nika run workflow.nika.yaml            # Execute workflow
nika run workflow.nika.yaml --no-live  # Force classic append-only output
nika check workflow.nika.yaml          # Validate syntax + DAG
nika check workflow.nika.yaml --strict # + test MCP connections
nika new my-flow --verb infer          # Create new workflow
nika workflow graph flow.nika.yaml     # Visualize DAG

# Direct verbs
nika infer "Explain AI"                # Quick LLM call
nika fetch https://blog.com --extract article  # HTTP + extraction
nika invoke nika:dimensions photo.jpg  # Builtin tool
nika agent "Research AI" --turns 5     # Multi-turn agent

# Interactive
nika ui                                # TUI
nika chat                              # Chat mode
nika studio                            # Studio editor

# Models & providers
nika model list                        # Cloud models + pricing
nika model info claude-sonnet-4-6      # Model details
nika model recommend                   # Smart recommendation
nika provider list                     # API key status
nika provider set anthropic            # Store key in keychain
nika mcp list                          # MCP server connections

# Learning
nika init --course                     # 12-level course (44 exercises)
nika course status                     # Constellation progress map
nika course next                       # Open next exercise
nika showcase list                     # Browse 115 showcase workflows
nika showcase extract <name>           # Extract showcase to current dir

# Project
nika init                              # Interactive project setup
nika config list                       # Show configuration
nika pkg list                          # Package management
nika media stats                       # Media store stats

# System
nika doctor --fix                      # System health + auto-repair
nika daemon status                     # Background daemon (Unix)
nika cache stats                       # LLM response cache (Unix)
nika setup                             # API key setup wizard
nika features                          # Compiled feature flags
nika completion zsh                    # Shell completions
nika trace list                        # Execution traces
nika help                              # Full command reference
nika help verbs                        # Deep-dive: 5 semantic verbs
```
