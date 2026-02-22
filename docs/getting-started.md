# Getting Started with Nika

Nika is a DAG workflow runner for AI tasks. Define workflows in YAML, execute them with automatic dependency resolution and parallel processing.

## Installation

### From crates.io (Recommended)

```bash
cargo install nika
```

### From Source

```bash
git clone https://github.com/supernovae-studio/nika.git
cd nika
cargo install --path crates/nika-cli
```

### Prerequisites

- Rust 1.86+ (install via [rustup](https://rustup.rs/))
- An LLM API key (Anthropic, OpenAI, or others)

## Quick Start

### 1. Set Up Your API Key

```bash
# For Claude (Anthropic)
export ANTHROPIC_API_KEY=your-key-here

# For OpenAI
export OPENAI_API_KEY=your-key-here
```

### 2. Create Your First Workflow

Create a file named `hello.nika.yaml`:

```yaml
schema: "nika/workflow@0.4"
provider: claude

tasks:
  - id: greet
    infer: "Say hello and share one fun fact about coffee."
```

### 3. Run It

```bash
nika hello.nika.yaml
```

That's it! Nika will execute the workflow and display the LLM's response.

## Core Concepts

### Workflows

A workflow is a YAML file containing:

- **Schema version**: Declares compatibility (`nika/workflow@0.4`)
- **Provider**: Default LLM provider (`claude`, `openai`, etc.)
- **Tasks**: A list of tasks to execute

### Tasks

Each task has:

- **id**: Unique identifier (snake_case)
- **One action verb**: `infer`, `exec`, `fetch`, `invoke`, or `agent`
- **Optional dependencies**: Via `use:` or `depends_on:`

### The 5 Action Verbs

| Verb | Purpose | Example |
|------|---------|---------|
| `infer:` | Generate text with an LLM | `infer: "Summarize this"` |
| `exec:` | Run a shell command | `exec: "npm run build"` |
| `fetch:` | Make an HTTP request | `fetch: { url: "..." }` |
| `invoke:` | Call an MCP tool | `invoke: { server: x, tool: y }` |
| `agent:` | Multi-turn agentic execution | `agent: { prompt: "..." }` |

## Basic Examples

### Text Generation (infer)

```yaml
schema: "nika/workflow@0.4"
provider: claude

tasks:
  - id: poem
    infer: "Write a haiku about programming"
```

### Chaining Tasks

```yaml
schema: "nika/workflow@0.4"
provider: claude

tasks:
  - id: generate
    infer: "List 3 startup ideas"
    output:
      format: json

  - id: evaluate
    use:
      ideas: generate
    infer: |
      Evaluate these startup ideas:
      {{use.ideas}}

      Pick the best one and explain why.
```

### Parallel Execution

```yaml
schema: "nika/workflow@0.4"
provider: claude

tasks:
  - id: translate
    for_each: ["French", "Spanish", "German"]
    as: lang
    concurrency: 3
    infer: "Translate 'Hello, world!' to {{use.lang}}"
```

## CLI Commands

```bash
# Run a workflow
nika workflow.nika.yaml

# Validate without executing
nika check workflow.nika.yaml

# Interactive TUI
nika                    # Home view
nika chat               # Chat mode
nika studio file.yaml   # Editor mode

# View execution traces
nika trace list
nika trace show <id>
```

## Supported LLM Providers

Nika supports 6 providers via [rig-core](https://github.com/0xPlaygrounds/rig):

| Provider | Environment Variable | Example Model |
|----------|---------------------|---------------|
| Claude (Anthropic) | `ANTHROPIC_API_KEY` | `claude-sonnet-4-20250514` |
| OpenAI | `OPENAI_API_KEY` | `gpt-4o` |
| Mistral | `MISTRAL_API_KEY` | `mistral-large-latest` |
| Groq | `GROQ_API_KEY` | `llama-3.1-70b-versatile` |
| DeepSeek | `DEEPSEEK_API_KEY` | `deepseek-chat` |
| Ollama | (local) | `llama3` |

Specify the provider in your workflow:

```yaml
provider: openai
model: gpt-4o
```

Or override per-task:

```yaml
tasks:
  - id: fast_task
    infer:
      prompt: "Quick summary"
      provider: groq
      model: llama-3.1-70b-versatile
```

## File Naming Convention

All Nika workflow files should use the `.nika.yaml` extension:

```
workflow.nika.yaml     # Correct
workflow.yaml          # Works but not recommended
```

## Next Steps

- Read the [YAML Reference](yaml-reference.md) for complete syntax documentation
- Explore [example workflows](../examples/) for common patterns
- Check the [API documentation](https://docs.rs/nika) for library usage

## Troubleshooting

### "API key not found"

Ensure your environment variable is set:

```bash
echo $ANTHROPIC_API_KEY  # Should show your key
```

### "Validation failed"

Run validation to see detailed errors:

```bash
nika check workflow.nika.yaml
```

### Enable Debug Logging

```bash
RUST_LOG=debug nika workflow.nika.yaml
```

## License

Nika is licensed under AGPL-3.0. See [LICENSE](../LICENSE) for details.
