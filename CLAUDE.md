# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project Overview

Nika CLI is the **command-line validator** for `.nika.yaml` workflows. Built in Rust, it validates workflows against the Nika v4.3 specification.

- **Language**: Rust
- **License**: BSL-1.1 (converts to Apache 2.0 on 2029-01-01)
- **Specification**: See `../nika/` for the open standard
- **GitHub**: https://github.com/supernovae-studio/nika-cli

## Commands

```bash
# Build
cargo build                      # Debug build
cargo build --release            # Release build

# Test
cargo test                       # Run all tests
cargo test --lib                 # Library tests only

# Lint
cargo clippy --all-targets --all-features -- -D warnings
cargo fmt --all -- --check       # Check formatting
cargo fmt --all                  # Fix formatting

# Run CLI
cargo run -- validate workflow.nika.yaml
cargo run -- validate ./examples/
cargo run -- init my-project
```

## Nika v4.3 Architecture

**7 Keywords (type-inferring):**

| Keyword | Category | Description |
|---------|----------|-------------|
| `prompt:` | agent | Main Agent (shared context) |
| `spawn:` | agent | Subagent (isolated 200K context) |
| `shell:` | tool | Execute shell command |
| `http:` | tool | Make HTTP request |
| `mcp:` | tool | MCP server::tool (:: separator) |
| `function:` | tool | path::fn (:: separator) |
| `llm:` | tool | One-shot LLM (stateless) |

**Example (v4.3):**
```yaml
tasks:
  - id: analyze
    prompt: "Analyze this code"

  - id: deep-research
    spawn: "Security audit"
    allowedTools: [Read, Grep]

  - id: bridge
    function: aggregate::collect

  - id: test
    shell: "npm test"
```

**13 Registry Types:**
```
TEMPLATES:  workflows/, agents/, tools/
INJECTS:    skills/, prompts/
RUNTIME:    hooks/, guardrails/, policies/
QUALITY:    evaluators/, rules/
CONNECT:    adapters/, memory/, schemas/
```

**File Formats:**
- `.md` (frontmatter): agents/, skills/, prompts/
- `.yaml`: all other types

## Connection Rules

```
prompt: → prompt:/spawn:/tools   OK
spawn: → prompt:                 NO (needs bridge)
spawn: → spawn:                  NO (can't spawn from spawn)
spawn: → tools                   OK (this is the bridge)
tools → prompt:/spawn:/tools     OK

Bridge pattern: spawn: → function: → prompt: OK
```

## 5-Layer Validation

The CLI implements 5-layer validation:

1. **Schema** - YAML structure validity
2. **Tasks** - Keyword detection and field validation
3. **Flows** - Flow definition validation
4. **Connections** - Connection matrix rules (the key rule!)
5. **Graph** - DAG structure, cycles, orphans

## Development Guidelines

1. **TDD**: Write tests before implementation
2. **Clippy clean**: No warnings allowed in CI
3. **Formatted**: Run `cargo fmt` before committing
4. **No panics**: Use `Result` and `?` operator
5. **Error messages**: Always include fix suggestions

## Skills from nika-brain

This project inherits skills from `../nika-brain/.claude/skills/`:
- `rust-development` - Rust patterns, Cargo.toml, error handling
- `nika-validation` - 5-layer validation implementation
- `nika-context` - Workflow structure, connection rules
- `ratatui-tui` - Terminal UI components

## Repository Structure

```
nika-cli/
├── Cargo.toml
├── src/
│   ├── main.rs           # CLI entry point (clap)
│   ├── lib.rs            # Public API
│   ├── commands/         # Subcommand implementations
│   ├── validator/        # 5-layer validation
│   ├── parser/           # YAML parsing
│   └── output/           # Result formatting
└── tests/
    └── fixtures/         # Test workflow files
```
