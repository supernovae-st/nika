# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.7.2] - 2026-02-22

### Added
- **GitHub Actions CI/CD** - Complete workflow automation
  - `ci.yml`: Format, clippy, test, coverage, security audit, build
  - `release.yml`: Cross-platform release binaries (Linux, macOS, Windows)
  - `dependabot.yml`: Automated dependency updates

### Statistics
- **2,323 tests passing**

## [0.7.1] - 2026-02-21

### Added
- **TUI Navigation Refresh** - VS Code-like tab system
  - Tab bar with full path display and active indicator
  - `Alt+←/→` to navigate between tabs
  - `Alt+W` / `Ctrl+W` to close tabs
  - `Ctrl+P` / `/` for fuzzy file search (Helix/VS Code style)
- **spawn_tracked** - Background task lifecycle management in TUI
  - MCP server connections tracked as background tasks
  - Real-time status indicators in status bar

### Statistics
- **1,842+ tests passing** (Nika lib + integration tests)
- **6 LLM providers** with full streaming (Claude, OpenAI, Mistral, Groq, DeepSeek, Ollama)

## [0.7.0] - 2026-02-21

### Added
- **Full Streaming for All 6 Providers** - Real-time token delivery across Claude, OpenAI,
  Mistral, Groq, DeepSeek, and Ollama via rig-core `StreamedAssistantContent`
- **MCP Server Status Events** - `McpConnected` / `McpError` lifecycle tracking
- **Event System Enhancements** - `verb` field in `TaskStarted`, `ContextAssembled` event,
  `StreamChunk::Metrics` for token counting
- **TUI DX** - miette v7.6 YAML error diagnostics, nucleo v0.5 fuzzy file search

### Statistics
- **1,842 tests passing** (up from 1,811)

## [0.6.0] - 2026-02-20

### Added
- **6 LLM Providers** - Claude, OpenAI, Mistral, Groq, DeepSeek, Ollama via rig-core
- **Auto-detection** - `RigProvider::auto()` checks env vars in priority order
- **Chat history** - `agent.chat(prompt, history)` via rig's `Chat` trait
- **New methods** - `chat_continue()`, `add_to_history()`, `with_history()`

### Changed
- Provider priority order: Anthropic → OpenAI → Mistral → Groq → DeepSeek → Ollama
- Default model updated to `claude-sonnet-4-20250514`

## [0.5.2] - 2026-02-20

### Added
- **CLI DX Refresh** - Streamlined command-line interface
  - `nika` alone launches TUI Home view
  - `nika chat` starts Chat view with `--provider` and `--model` options
  - `nika studio [file]` starts Studio view
  - `nika check` replaces `validate` (alias kept)
  - Positional: `nika workflow.nika.yaml` runs directly
- **TUI 4-View Architecture** - Unified interface with Tab navigation
  - Chat, Home, Studio, Monitor views
  - Keybindings: `a/h/s/m` or `Tab` to switch

### Fixed
- Async response polling now wired in main event loop
- MCP client lazy initialization with DashMap + OnceCell

## [0.5.0] - 2026-02-19

### Added
- **MVP 8: RLM Enhancements** - Complete RLM-on-KG implementation
  - Phase 1: Reasoning capture (`thinking` field in AgentTurn events)
  - Phase 2: Nested agents (`spawn_agent` internal tool with depth protection)
  - Phase 3: Schema introspection (`novanet_introspect` MCP tool)
  - Phase 4: Dynamic decomposition (`decompose:` modifier for runtime DAG expansion)
  - Phase 5: Lazy bindings (`lazy: true` for deferred binding resolution)
- **15 lazy binding tests** - Comprehensive test suite
- **11 decompose tests** - Test coverage for decompose modifier

### Statistics
- **MVP 8 complete** (RLM enhancements)
- **1,747 tests passing**

## [0.4.1] - 2026-02-19

### Fixed
- **Token tracking in streaming mode** - `run_claude_with_thinking()` now extracts tokens from `StreamedAssistantContent::Final` via rig's `GetTokenUsage` trait
- **AgentTurnMetadata accuracy** - `input_tokens` and `output_tokens` are now correctly populated in extended thinking mode

### Added
- **Reasoning capture** - `thinking` field captured in AgentTurn events
- **rig-core integration** - New `RigAgentLoop` using rig-core's AgentBuilder
- **RigProvider.infer()** - Simple text completion via rig-core
- **NikaMcpTool** - Implements rig's `ToolDyn` trait for MCP tool bridging
- **24 rig tests** - Comprehensive test suite for rig-based providers

### Breaking Changes
- **Removed deprecated providers** - `ClaudeProvider`, `OpenAIProvider`, `provider::types` deleted
- **Removed `AgentLoop`** - Replaced by `RigAgentLoop` with rig's AgentBuilder
- **Removed `resilience/` module** - Entire module deleted (was never wired into runtime)

### Changed
- **~1,420 lines removed** - Code reduction from removing deprecated providers
- **`infer:` verb migrated to rig-core** - executor.rs now uses `RigProvider.infer()`
- **621+ tests passing** - Comprehensive test coverage after migration

### Migration Guide

```rust
// Old (v0.3)
use nika::provider::ClaudeProvider;
let provider = ClaudeProvider::new()?;
let result = provider.infer("prompt", None).await?;

// New (v0.4+)
use nika::provider::rig::RigProvider;
let provider = RigProvider::claude()?;
let result = provider.infer("prompt", None).await?;
```

## [0.3.0] - 2026-02-19

### Added
- **Two new verbs** per ADR-001:
  - `invoke:` - MCP tool calls (connects to NovaNet)
  - `agent:` - Multi-turn agentic loops with tool use
- **MCP client integration** - Connect to MCP servers like NovaNet
- **Resilience patterns**:
  - Retry with exponential backoff + jitter
  - Circuit breaker (Closed → Open → HalfOpen)
  - Rate limiting per provider
- **for_each parallelism** - Iterate over arrays with concurrency control
- **TUI** - Terminal UI for workflow monitoring (feature-gated)
- **Quickstart examples** - Two new example workflows:
  - `examples/quickstart-mcp.nika.yaml` - MCP integration with NovaNet
  - `examples/quickstart-multilang.nika.yaml` - Multi-locale generation with `for_each`
- Schema version: `nika/workflow@0.3`

### Changed
- Schema bumped from @0.1 to @0.3
- 16 EventLog variants for comprehensive observability

## [0.1.0] - 2025-01-27

### Added
- Initial release of Nika CLI
- YAML workflow parsing with schema validation (`nika/workflow@0.1`)
- DAG-based task execution with parallel processing
- Three action types:
  - `infer:` - LLM inference calls
  - `exec:` - Shell command execution
  - `fetch:` - HTTP requests
- Data flow between tasks via `use:` blocks
- Template system with `{{use.alias}}` syntax
- Default values with `??` operator
- Output formatting (text/json) with optional JSON Schema validation
- Provider support: Claude, OpenAI, Mock
- Structured error codes (NIKA-0xx)
- Lock-free DataStore with DashMap
- Event logging for execution tracing

### Commands
- `nika run <workflow.yaml>` - Execute a workflow
- `nika validate <workflow.yaml>` - Validate without execution

[Unreleased]: https://github.com/supernovae-st/nika-dev/compare/v0.7.1...HEAD
[0.7.1]: https://github.com/supernovae-st/nika-dev/compare/v0.7.0...v0.7.1
[0.7.0]: https://github.com/supernovae-st/nika-dev/compare/v0.6.0...v0.7.0
[0.6.0]: https://github.com/supernovae-st/nika-dev/compare/v0.5.2...v0.6.0
[0.5.2]: https://github.com/supernovae-st/nika-dev/compare/v0.5.0...v0.5.2
[0.5.0]: https://github.com/supernovae-st/nika-dev/compare/v0.4.1...v0.5.0
[0.4.1]: https://github.com/supernovae-st/nika-dev/compare/v0.3.0...v0.4.1
[0.3.0]: https://github.com/supernovae-st/nika-dev/compare/v0.1.0...v0.3.0
[0.1.0]: https://github.com/supernovae-st/nika-dev/releases/tag/v0.1.0
