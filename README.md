# Nika CLI

> **Private Repository** - Proprietary closed-source CLI for Nika workflow orchestration.

The [Nika Specification](https://github.com/supernovae-studio/nika) is open source (Apache 2.0).
This CLI is closed source and distributed as a binary.

## Architecture v4.5

**7 Keywords with Type Inference:**

| Keyword | Category | Context | Description |
|---------|----------|---------|-------------|
| `agent:` | agent | main | Main Agent (shared context) |
| `subagent:` | agent | isolated | Subagent (isolated 200K context) |
| `shell:` | tool | - | Execute shell command |
| `http:` | tool | - | HTTP request |
| `mcp:` | tool | - | MCP server::tool |
| `function:` | tool | - | path::functionName |
| `llm:` | tool | - | Stateless LLM call |

**Connection Matrix:**
```
agent: -> agent:/subagent:/tool  OK
subagent: -> agent:              NO (needs bridge)
subagent: -> subagent:           NO (can't spawn from subagent)
subagent: -> tool                OK (this is the bridge)
tool -> agent:/subagent:/tool    OK

Bridge: subagent: -> tool -> agent: OK
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
| `nika validate [path]` | OK | Validate .nika.yaml workflow files |
| `nika run <workflow>` | OK | Run a workflow with provider |
| `nika init [name]` | OK | Initialize new .nika project |
| `nika tui` | OK | Launch TUI dashboard |
| `nika add <package>` | Soon | Install community package |
| `nika publish <file>` | Soon | Publish to registry |
| `nika auth login` | Soon | Authenticate with registry |

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
# v4.5 keyword syntax
mainAgent:
  model: claude-sonnet-4-5
  systemPrompt: "You are a helpful assistant."

tasks:
  - id: analyze
    agent: "Analyze the input."

  - id: save
    mcp: filesystem::write_file
    args:
      path: "output.txt"

flows:
  - source: analyze
    target: save
```

## Test Coverage

```
OK 47 unit tests (lib)
OK 17 integration tests (CLI)
OK 64 total tests passing
```

## Related

- [Nika Specification](https://github.com/supernovae-studio/nika) (Apache 2.0)
- [Documentation](https://nika.sh/docs)

## License

Proprietary - (c) 2025 SuperNovae Studio. All rights reserved.
