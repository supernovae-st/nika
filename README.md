<div align="center">

# 🦀 Nika

**Native Intelligence Kernel Agent**

[![Rust](https://img.shields.io/badge/rust-1.75+-orange.svg?logo=rust)](https://www.rust-lang.org/)
[![License: MIT](https://img.shields.io/badge/License-MIT-blue.svg)](LICENSE)
[![Version](https://img.shields.io/badge/version-0.1.0-green.svg)](CHANGELOG.md)

*DAG workflow runner for AI tasks*

[Installation](#installation) • [Quick Start](#quick-start) • [Documentation](#documentation)

</div>

---

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

| Feature | Description |
|---------|-------------|
| **3 Actions** | `infer:` (LLM) • `exec:` (shell) • `fetch:` (HTTP) |
| **DAG Execution** | Parallel processing when dependencies allow |
| **Data Flow** | `use:` blocks + `{{use.alias}}` templates |
| **Providers** | Claude, OpenAI, Mock |

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
nika run <workflow.yaml>      # Execute workflow
nika validate <workflow.yaml> # Validate only
```

## Documentation

See [spec/SPEC.md](spec/SPEC.md) for full specification.

---

<div align="center">

**[SuperNovae Studio](https://supernovae.studio)**

MIT License • Made with 🦀 in Rust

</div>
