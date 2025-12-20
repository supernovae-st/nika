# Nika CLI

> ⚠️ **Private Repository** - Proprietary closed-source CLI for Nika workflow orchestration.

The [Nika Specification](https://github.com/supernovae-studio/nika) is open source (Apache 2.0).
This CLI is closed source and distributed as a binary.

## Architecture v3

**2 task types with scope:**

| Type | Scope | Token Cost | Description |
|------|-------|------------|-------------|
| `agent` | `main` | 500+ | LLM reasoning, shares context |
| `agent` | `isolated` | 8000+ | Separate 200K context |
| `action` | - | 0 | Deterministic operations |

**Connection Matrix:**
```
agent(main) → agent(main) ✅    agent(main) → action ✅    agent(main) → agent(isolated) ✅
agent(isolated) → agent(main) ❌    agent(isolated) → action ✅    agent(isolated) → agent(isolated) ❌
action → agent(main) ✅    action → action ✅    action → agent(isolated) ✅

Bridge: agent(isolated) → action → agent(main) ✅
```

## Requirements

- Rust 1.75+ (install via [rustup](https://rustup.rs/))
- Claude Code CLI (for workflow execution with `--provider claude`)

## Installation

```bash
# Build from source
cargo build --release

# Install locally
cargo install --path .

# Or via Homebrew (coming soon)
brew install supernovae-studio/tap/nika
```

## Commands

| Command | Status | Description |
|---------|--------|-------------|
| `nika validate [path]` | ✅ | Validate .nika.yaml workflow files |
| `nika run <workflow>` | ✅ | Run a workflow with provider |
| `nika init [name]` | ✅ | Initialize new .nika project |
| `nika tui` | ✅ | Launch TUI dashboard |
| `nika add <package>` | ⏳ | Install community package |
| `nika publish <file>` | ⏳ | Publish to registry |
| `nika auth login` | ⏳ | Authenticate with registry |

## Usage

### Validate Workflows

```bash
# Validate single file
nika validate my-workflow.nika.yaml

# Validate directory (recursive)
nika validate .

# Output formats
nika validate --format pretty   # Default, human-readable
nika validate --format json     # Machine-readable
nika validate --format compact  # One-liner summary

# Verbose output
nika validate --verbose
```

### Run Workflows

```bash
# Run with Claude (default)
nika run my-workflow.nika.yaml

# Choose provider
nika run my-workflow.nika.yaml --provider claude
nika run my-workflow.nika.yaml --provider openai
nika run my-workflow.nika.yaml --provider ollama

# Verbose execution
nika run my-workflow.nika.yaml --verbose
```

### Initialize Project

```bash
# Create new project
nika init my-project

# Initialize in current directory
nika init .
```

Creates:
```
my-project/
├── .nika/
│   ├── workflows/    # Reusable workflow library
│   ├── tasks/        # Custom task definitions
│   └── .gitignore    # Excludes cache/secrets
├── main.nika.yaml    # Entry point workflow
└── nika.yaml         # Project manifest
```

### TUI Dashboard

```bash
nika tui
```

## Development

```bash
# Build
cargo build

# Run
cargo run -- --help

# Test
cargo test

# Test with verbose output
cargo test -- --nocapture

# Release build
cargo build --release
```

## Project Structure

```
src/
├── main.rs          # CLI entry point (clap)
├── lib.rs           # Library exports
├── workflow.rs      # Workflow types & parsing
├── validator.rs     # 5-layer validation pipeline
├── runner.rs        # Workflow execution engine
├── init.rs          # Project initialization
└── tui/             # Terminal UI dashboard
    ├── mod.rs       # TUI entry & main loop
    ├── app.rs       # App rendering
    ├── events.rs    # Keyboard handling
    ├── state.rs     # Application state
    ├── theme.rs     # Colors & styling
    ├── widgets.rs   # Custom widgets
    └── runtime/     # Execution backends
```

## Dependencies

- **clap** - CLI argument parsing
- **colored** - Terminal colors
- **ratatui** - Terminal UI framework
- **crossterm** - Cross-platform terminal
- **serde_yaml** - YAML parsing
- **anyhow** - Error handling
- **walkdir** - Directory traversal
- **tokio** - Async runtime

## Workflow Example

```yaml
mainAgent:
  model: claude-sonnet-4-5
  systemPrompt: "You are a helpful assistant."

tasks:
  - id: analyze
    type: agent
    prompt: "Analyze the input."

  - id: save
    type: action
    run: Write
    file: "output.txt"

flows:
  - source: analyze
    target: save
```

## Test Coverage

```
✓ 47 unit tests (lib)
✓ 17 integration tests (CLI)
✓ 64 total tests passing
```

## Related

- [Nika Specification](https://github.com/supernovae-studio/nika) (Apache 2.0)
- [Documentation](https://nika.sh/docs)

## License

Proprietary - © 2025 SuperNovae Studio. All rights reserved.
