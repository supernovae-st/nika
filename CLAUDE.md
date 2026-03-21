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
```
