<div align="center">

# Nika

**Native Intelligence Kernel Agent**

[![Crates.io](https://img.shields.io/crates/v/nika.svg)](https://crates.io/crates/nika)
[![Docs.rs](https://docs.rs/nika/badge.svg)](https://docs.rs/nika)
[![License: AGPL-3.0](https://img.shields.io/badge/License-AGPL--3.0-blue.svg)](LICENSE)
[![Rust](https://img.shields.io/badge/rust-1.86+-orange.svg?logo=rust)](https://www.rust-lang.org/)

*DAG workflow runner for AI tasks with MCP integration*

[Installation](#installation) | [Quick Start](#quick-start) | [Documentation](#documentation) | [Contributing](#contributing)

</div>

---

Nika executes YAML-defined workflows as directed acyclic graphs (DAGs). Supports LLM inference, shell commands, HTTP requests, and MCP tool calling with data flow between tasks.

## Features

| Feature | Description |
|---------|-------------|
| **5 Actions** | `infer:` (LLM) • `exec:` (shell) • `fetch:` (HTTP) • `invoke:` (MCP) • `agent:` (agentic) |
| **DAG Execution** | Parallel processing when dependencies allow |
| **Data Flow** | `use:` blocks + `{{use.alias}}` templates |
| **for_each** | Parallel iteration over arrays with concurrency control |
| **MCP Integration** | Connect to MCP servers for tool calling |
| **6 Providers** | Claude, OpenAI, Mistral, Groq, DeepSeek, Ollama via rig-core |
| **TUI** | Real-time workflow visualization with 4-view architecture |

## Installation

```bash
cargo install nika
```

Or build from source:

```bash
git clone https://github.com/supernovae-studio/nika.git
cd nika
cargo install --path crates/nika-cli
```

## Quick Start

Create a workflow file:

```yaml
# hello.nika.yaml
schema: "nika/workflow@0.3"
provider: claude

tasks:
  - id: greet
    infer: "Say hello in French"
```

Run it:

```bash
export ANTHROPIC_API_KEY=your-key
nika hello.nika.yaml
```

## Actions

### infer (LLM)

```yaml
infer: "Your prompt"  # Shorthand

# Or with options
infer:
  prompt: "Your prompt"
  provider: openai
  model: gpt-4o-mini
```

### exec (shell)

```yaml
exec: "npm run build"  # Shorthand

# Or with options
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

### invoke (MCP)

Call tools from MCP servers:

```yaml
invoke:
  server: novanet
  tool: novanet_generate
  params:
    entity: "qr-code"
    locale: "fr-FR"
```

### agent (Agentic Loop)

Multi-turn agentic execution with tool access:

```yaml
agent:
  prompt: "Research and summarize recent AI papers"
  mcp: [novanet, perplexity]
  max_turns: 10
```

## Data Flow

Pass data between tasks using `use:` blocks:

```yaml
tasks:
  - id: weather
    infer: "Get Paris weather as JSON"
    output:
      format: json

  - id: recommend
    use:
      forecast: weather.summary
      temp: weather.temp ?? 20
    infer: |
      Weather: {{use.forecast}} at {{use.temp}}C
      Suggest an activity.

flows:
  - source: weather
    target: recommend
```

## Parallel Execution

Execute tasks in parallel with `for_each`:

```yaml
tasks:
  - id: generate_pages
    for_each: ["fr-FR", "en-US", "de-DE"]
    as: locale
    concurrency: 5
    invoke:
      server: novanet
      tool: novanet_generate
      params:
        entity: "landing-page"
        locale: "{{use.locale}}"
```

## Commands

```bash
nika                          # TUI Home view
nika chat                     # Chat view
nika studio [file]            # Studio view
nika workflow.nika.yaml       # Run workflow
nika check workflow.yaml      # Validate
nika trace list               # List traces
nika trace show <id>          # Show trace details
```

## Crates

| Crate | Description |
|-------|-------------|
| `nika` | CLI binary |
| `nika-core` | Core types, AST, DAG validation |
| `nika-mcp` | MCP client integration |
| `nika-provider` | LLM providers (rig-core wrapper) |
| `nika-runtime` | Execution engine |
| `nika-tui` | Terminal UI |

## Documentation

- [Full Specification](docs/SPEC.md)
- [Examples](examples/)
- [API Documentation](https://docs.rs/nika)

## Contributing

See [CONTRIBUTING.md](CONTRIBUTING.md) for guidelines.

## License

AGPL-3.0 — See [LICENSE](LICENSE) for details.

---

<div align="center">

**[SuperNovae Studio](https://supernovae.studio)**

Made with Rust

</div>
