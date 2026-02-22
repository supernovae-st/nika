# nika-cli

Command-line interface for Nika workflow engine.

## Overview

This is the main entry point binary that provides:

- **TUI Launch** - `nika` alone opens interactive terminal UI
- **Workflow Execution** - `nika workflow.yaml` runs directly
- **Chat Mode** - `nika chat` for conversational AI
- **Studio Mode** - `nika studio` for YAML editing
- **Validation** - `nika check` for workflow validation

## Commands

```bash
# Launch TUI (Home view)
nika

# Run workflow directly
nika workflow.nika.yaml

# Chat mode
nika chat
nika chat --provider openai --model gpt-4

# Studio mode
nika studio
nika studio workflow.yaml

# Run workflow (explicit)
nika run workflow.yaml
nika run workflow.yaml --provider claude

# Validate workflow
nika check workflow.yaml
nika check workflow.yaml --strict

# Initialize project
nika init

# Manage traces
nika trace list
nika trace show <id>
nika trace export <id>
```

## Environment Variables

| Variable | Description |
|----------|-------------|
| `ANTHROPIC_API_KEY` | Claude API key |
| `OPENAI_API_KEY` | OpenAI API key |
| `MISTRAL_API_KEY` | Mistral API key |
| `GROQ_API_KEY` | Groq API key |
| `DEEPSEEK_API_KEY` | DeepSeek API key |

## Installation

```bash
# From source
cargo install --path crates/nika-cli

# Or build release
cargo build --release
./target/release/nika
```

## License

MIT
