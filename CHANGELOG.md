# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added

- **Lazy bindings (MVP 8 Phase 5)** - Defer binding resolution until first access with `lazy: true`
- **Decompose modifier (MVP 8 Phase 4)** - Runtime DAG expansion via MCP traversal
- **SpawnAgentTool foundation (MVP 8 Phase 2)** - Internal tool for nested agent spawning (stub)

## [0.4.1] - 2026-02-19

### Fixed

- **Token tracking in streaming mode** - `run_claude_with_thinking()` now extracts tokens from `StreamedAssistantContent::Final` via rig's `GetTokenUsage` trait
- **AgentTurnMetadata accuracy** - `input_tokens` and `output_tokens` are now correctly populated in extended thinking mode

### Added

- **MVP 8 Phase 1: Reasoning capture** - `thinking` field captured in AgentTurn events
- **15 lazy binding tests** - Comprehensive test suite for lazy binding feature
- **11 decompose tests** - Test coverage for decompose modifier

## [0.4.0] - 2026-02-19

### Breaking Changes

- **Removed deprecated providers** - `ClaudeProvider`, `OpenAIProvider`, `provider::types` deleted
- **Removed `AgentLoop`** - Replaced by `RigAgentLoop` with rig's AgentBuilder
- **Removed `resilience/` module** - Entire module deleted (was never wired into runtime)

### Added

- **RigAgentLoop** - Full agentic execution using rig-core's AgentBuilder
- **NikaMcpTool** - Implements rig's `ToolDyn` trait for MCP tool bridging
- **Pure rig-core architecture** - All LLM operations now use rig-core v0.31

### Changed

- **621+ tests passing** - Comprehensive test coverage after migration
- **~1,420 lines removed** - Code reduction from removing deprecated providers

### Migration Guide

```rust
// Old (v0.3)
use nika::provider::ClaudeProvider;
let provider = ClaudeProvider::new()?;
let result = provider.infer("prompt", None).await?;

// New (v0.4)
use nika::provider::rig::RigProvider;
let provider = RigProvider::claude()?;
let result = provider.infer("prompt", None).await?;
```

## [0.3.1] - 2026-02-19

### Added

- **rig-core integration** - New `RigAgentLoop` using rig-core's AgentBuilder for agentic execution
- **RigProvider.infer()** - Simple text completion via rig-core (now used by `infer:` verb)
- **NikaMcpTool** - Implements rig's `ToolDyn` trait for MCP tool bridging
- **24 rig tests** - Comprehensive test suite for rig-based providers and agent loop
- **Advanced workflow YAML tests** - UC-001/002/003 use cases with NovaNet integration

### Changed

- **`infer:` verb migrated to rig-core** - executor.rs now uses `RigProvider.infer()` instead of deprecated providers
- **Provider migration started** - Old providers (claude.rs, openai.rs) marked as deprecated
- **Migration path documented** - Clear guidance for transitioning from old Provider trait to rig-core
- Provider module now recommends `RigProvider` for `infer:` and `RigAgentLoop` for `agent:` verb

### Deprecated

- `ClaudeProvider` - Use `RigProvider.infer()` for infer, `RigAgentLoop` for agent
- `OpenAIProvider` - Use `RigProvider.infer()` for infer, `RigAgentLoop` for agent
- `provider::types` - Use rig-core types directly

## [0.3.0] - 2026-02-19

### Added

- **Quickstart examples** - Two new example workflows demonstrating v0.3 features:
  - `examples/quickstart-mcp.nika.yaml` - MCP integration with NovaNet
  - `examples/quickstart-multilang.nika.yaml` - Multi-locale generation with `for_each`
- Schema version: `nika/workflow@0.3`

### Changed

- Updated rmcp dependency for MCP client features

## [0.2.0] - 2026-02-15

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
- Schema version: `nika/workflow@0.2`

### Changed

- Schema bumped from @0.1 to @0.2
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
