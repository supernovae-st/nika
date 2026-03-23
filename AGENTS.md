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
nika check workflow.nika.yaml    # Validate
nika run workflow.nika.yaml      # Execute
nika ui                          # TUI
nika provider list               # API key status
nika init                        # Interactive project setup (wizard)
nika init --course               # Generate 12-level learning course (44 exercises)
nika init --minimal              # Minimal scaffold (5 workflows, 1 per verb)
nika course status               # Show constellation progress map
nika course next                 # Open next exercise
nika course check [level]        # Validate exercises
nika course hint [exercise]      # Progressive hints (3 tiers)
nika course run <exercise>       # Run a course exercise
nika course info [level]         # Show course/level details
nika course reset <level>        # Reset a level
nika course watch                # Auto-check on file save
nika showcase list               # Browse 115 showcase workflows
nika showcase extract <name>     # Extract a showcase to current dir
```
