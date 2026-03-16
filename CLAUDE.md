# Nika

Semantic YAML workflow engine for AI tasks.

**v0.27.0** | 6,259 tests | Schema `@0.12` | [QR Code AI](https://qrcode-ai.com)

## Why Nika Exists

AI workflows buried in code are untraceable and non-reproducible. Nika executes YAML-defined DAG workflows with 5 semantic verbs, providing full observability via NDJSON traces.

## 5 Semantic Verbs

| Verb | Purpose | Example |
|------|---------|---------|
| `infer:` | LLM generation | `infer: "Generate a headline"` |
| `exec:` | Shell command | `exec: "npm run build"` |
| `fetch:` | HTTP request | `fetch: { url: "...", method: "GET" }` |
| `invoke:` | MCP tool call | `invoke: { mcp: "novanet", tool: "novanet_search" }` |
| `agent:` | Multi-turn loop | `agent: { prompt: "Research...", mcp: [...] }` |

## 7 Inference Backends

6 cloud via rig-core v0.32: `anthropic`, `openai`, `mistral`, `groq`, `deepseek`, `gemini`
1 local via mistral.rs: `native` (GGUF models)

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
      title: step1
    infer: "Write intro for: {{with.title}}"
    depends_on: [step1]
```

**Key syntax**: `with:` for bindings, `{{with.alias}}` for templates.

## Integration with NovaNet

Nika connects to NovaNet via MCP (never direct Neo4j):

```yaml
mcp:
  novanet:
    command: cargo
    args: ["run", "--manifest-path", "../novanet/tools/novanet-mcp/Cargo.toml"]

tasks:
  - id: get_context
    invoke:
      mcp: novanet
      tool: novanet_context
      params: { focus_key: "qr-code", locale: "fr-FR", mode: "page" }

  - id: generate
    with:
      ctx: get_context
    infer: "Generate landing page using: {{with.ctx}}"
```

## 4 TUI Views

| Key | View | Purpose |
|-----|------|---------|
| `1/s` | Studio | Browser + YAML editor + DAG preview |
| `2/r` | Runner | Real-time execution monitoring |
| `3/c` | Chat | Conversational agent interface |
| `4/,` | Settings | Provider config, preferences |

## Quick Commands

```bash
nika check workflow.nika.yaml    # Validate
nika run workflow.nika.yaml      # Execute
nika ui                          # TUI
nika provider list               # API key status
nika trace show <id>             # Debug
```

## Conventions

| Aspect | Convention |
|--------|------------|
| File extension | `.nika.yaml` |
| Schema | `nika/workflow@0.12` |
| Bindings | `with:` + `{{with.alias}}` |
| Error codes | `NIKA-XXX` (see `tools/nika/src/error.rs`) |
| Tests | TDD preferred, 80% coverage |
