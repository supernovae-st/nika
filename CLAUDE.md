# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project Overview

Nika CLI is the **command-line validator and executor** for `.nika.yaml` workflows. Built in Rust, it validates workflows against the Nika v4.7.1 specification and executes them via multiple LLM providers.

- **Language**: Rust (async with tokio)
- **License**: BSL-1.1 (converts to Apache 2.0 on 2029-01-01)
- **Specification**: See `./spec/` (symlink to `../nika-docs/spec`)
- **GitHub**: https://github.com/supernovae-studio/nika-cli

## Multi-Provider Support

Nika CLI supports multiple LLM providers:

| Provider | Status | Environment Variable |
|----------|--------|---------------------|
| `claude` | Production | Uses Claude CLI |
| `openai` | Production | `OPENAI_API_KEY` |
| `ollama` | Stub | Local Ollama instance |
| `mistral` | Stub | `MISTRAL_API_KEY` |
| `mock` | Testing | None (configurable responses) |

**Usage:**
```bash
nika run workflow.nika.yaml --provider claude    # Production (default)
nika run workflow.nika.yaml --provider openai    # OpenAI API
nika run workflow.nika.yaml --provider mock      # Testing
```

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

## Nika v4.7.1 Architecture

**7 Keywords (type-inferring):**

| Keyword | Category | Description |
|---------|----------|-------------|
| `agent:` | context | Main Agent (shared context) |
| `subagent:` | isolated | Subagent (isolated 200K context) |
| `shell:` | tool | Execute shell command |
| `http:` | tool | Make HTTP request |
| `mcp:` | tool | MCP server::tool (:: separator) |
| `function:` | tool | path::fn (:: separator) |
| `llm:` | tool | One-shot LLM (stateless) |

**Example (v4.6):**
```yaml
agent:
  model: claude-sonnet-4-5
  systemPrompt: "You are a helpful assistant."

tasks:
  - id: analyze
    agent: "Analyze this code"

  - id: deep-research
    subagent: "Security audit"
    allowedTools: [Read, Grep]

  - id: bridge
    function: aggregate::collect

  - id: test
    shell: "npm test"

flows:
  - source: analyze
    target: deep-research
```

## v4.7.1 Performance Optimizations

| Component | File | Optimization |
|-----------|------|--------------|
| Template Resolution | `src/template.rs` | Single-pass tokenization + DashMap cache (85% faster) |
| Task IDs | `src/smart_string.rs` | Inline storage ≤31 chars (93% faster) |
| Context Sharing | `src/runner.rs` | Arc<str> zero-copy (96% faster) |
| Memory Pool | `src/context_pool.rs` | Reusable ExecutionContext (70% less alloc) |

## Connection Rules (v4.7.1)

```
agent: → agent:/subagent:/tools   OK
subagent: → agent:                 OK (via WorkflowRunner auto-write)
subagent: → subagent:              NO (can't spawn from subagent)
subagent: → tools                  OK (returns data)
tools → agent:/subagent:/tools     OK

Bridge pattern: subagent: → function: → agent: OPTIONAL (for transformation)
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

## Skills from nika-hub

This project inherits skills from `.claude/` (symlink to `../.claude/`):
- `rust-development` - Rust patterns, Cargo.toml, error handling
- `nika-validation` - 5-layer validation implementation
- `nika-context` - Workflow structure, connection rules
- `ratatui-tui` - Terminal UI components

## Repository Structure

```
nika-cli/
├── Cargo.toml
├── .claude -> ../.claude         # Shared skills
├── spec -> ../nika-docs/spec     # Specification
├── src/
│   ├── main.rs           # CLI entry point (clap)
│   ├── lib.rs            # Public API
│   ├── workflow.rs       # Core data structures
│   ├── validator.rs      # 5-layer validation
│   ├── runner.rs         # Workflow execution (async, Arc<str>)
│   ├── template.rs       # v4.6 single-pass resolver
│   ├── smart_string.rs   # v4.6 inline string storage
│   ├── context_pool.rs   # v4.6 memory pool
│   ├── init.rs           # Project initialization
│   ├── provider/         # Multi-provider support
│   │   ├── mod.rs        # Provider trait + factory
│   │   ├── claude.rs     # Claude CLI provider
│   │   ├── openai.rs     # OpenAI API (real)
│   │   ├── ollama.rs     # Ollama local (stub)
│   │   ├── mistral.rs    # Mistral API (stub)
│   │   └── mock.rs       # Mock provider for testing
│   └── tui/              # Terminal UI (ratatui)
└── tests/
    ├── fixtures/         # Test workflow files
    └── stress_test.rs    # Resource limits tests
```
