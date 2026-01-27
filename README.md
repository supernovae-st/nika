# Nika

**Native Intelligence Kernel Agent** 🦀

> DAG workflow runner for AI tasks

Nika executes YAML-defined workflows as directed acyclic graphs (DAGs). Supports LLM inference, shell commands, and HTTP requests with data flow between tasks.

## Installation

```bash
cargo install --path .
```

## Quick Start

```yaml
# hello.nika.yaml
schema: "nika/workflow@0.1"
provider: claude

tasks:
  - id: greet
    infer:
      prompt: "Say hello in French"
```

```bash
export ANTHROPIC_API_KEY=your-key
nika run hello.nika.yaml
```

## Features

- **3 actions**: `infer:` (LLM), `exec:` (shell), `fetch:` (HTTP)
- **DAG execution**: Parallel when dependencies allow
- **Data flow**: `use:` blocks + `{{use.alias}}` templates
- **Providers**: Claude, OpenAI, Mock

## Example

```yaml
schema: "nika/workflow@0.1"
provider: claude

tasks:
  - id: weather
    infer:
      prompt: "Get Paris weather as JSON: {summary, temp}"
    output:
      format: json

  - id: recommend
    use:
      forecast: weather.summary
      temp: weather.temp ?? 20
    infer:
      prompt: |
        Weather: {{use.forecast}} at {{use.temp}}C
        Suggest an activity.

flows:
  - source: weather
    target: recommend
```

## Actions

### infer (LLM)

```yaml
infer:
  prompt: "Your prompt"
  provider: openai   # optional
  model: gpt-4o-mini # optional
```

### exec (shell)

```yaml
exec:
  command: "npm run build"
```

### fetch (HTTP)

```yaml
fetch:
  url: "https://api.example.com"
  method: POST
  headers:
    Authorization: "Bearer {{use.token}}"
```

## Providers

| Provider | Env Variable | Models |
|----------|--------------|--------|
| `claude` | `ANTHROPIC_API_KEY` | claude-sonnet-4-*, claude-haiku-* |
| `openai` | `OPENAI_API_KEY` | gpt-4o, gpt-4o-mini |
| `mock` | - | (testing) |

## Commands

```bash
nika run <workflow.yaml>      # Execute
nika validate <workflow.yaml> # Validate only
```

## Documentation

See [spec/SPEC.md](spec/SPEC.md) for full specification.

## License

MIT - Built by [SuperNovae Studio](https://supernovae.studio)
