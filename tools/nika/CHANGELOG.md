# Changelog

All notable changes to Nika are documented in this file.

Format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/).
This project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.5.2] - 2026-02-21

### Added
- **CLI DX Refresh** - Streamlined command-line interface
  - `nika` alone launches TUI Home view (browse workflows)
  - `nika chat` starts Chat view with optional `--provider` and `--model`
  - `nika studio [file]` starts Studio view for YAML editing
  - `nika check` replaces `nika validate` (alias kept for compatibility)
  - Positional argument: `nika workflow.nika.yaml` runs workflow directly
- **TUI 4-View Architecture** - Unified interface with Tab navigation
  - Chat view: Conversational agent with 5-verb support
  - Home view: File browser for `.nika.yaml` files
  - Studio view: YAML editor with live validation
  - Monitor view: Real-time 4-panel observer (DAG, Reasoning, NovaNet)
- **App Builder Methods** - Fluent API for TUI configuration
  - `with_initial_view()` - Set starting view
  - `with_studio_file()` - Pre-load file in Studio
  - `with_broadcast_receiver()` - Wire event streaming

### Changed
- CLI structure uses `Option<Commands>` for default TUI behavior
- All entry points now use unified `run_unified()` method
- Documentation updated across all CLAUDE.md files and skills

### Fixed
- `run_unified()` now called from all TUI entry points (was only `run()`)
- Async response polling wired in main event loop
- MCP client lazy initialization with `DashMap + OnceCell` caching

### Statistics
- **1747 tests passing** (80 skipped)
- **4 entry points**: standalone, workflow, chat, studio
- **All 6 plan phases implemented**

## [0.5.1] - 2026-02-20

### Added
- **Verb Shorthand Syntax** - Simplified YAML for common cases
  - `infer: "prompt"` instead of `infer: { prompt: "..." }`
  - `exec: "command"` instead of `exec: { command: "..." }`
- **TUI Spinners** - 4 themed spinner types (rocket, stars, orbit, cosmic)
- **Animation Widgets** - PulseText, ParticleBurst, ShakeText
- **StatusBar Enhancements** - Provider indicator, token counter, MCP status
- **DAG Visualization** - Verb-specific icons for each task type

### Changed
- Default model updated from `claude-3-5-sonnet-latest` to `claude-sonnet-4-20250514`

### Fixed
- Validation preview now shows actual validation results
- Session context properly tracks MCP server connections

## [0.5.0] - 2026-02-19

### Added
- **MVP 8: RLM Enhancements** - 5 new features for agentic workflows
  - Reasoning capture: `thinking` field in AgentTurn events
  - Nested agents: `spawn_agent` internal tool with depth protection
  - Schema introspection: `novanet_introspect` MCP tool support
  - Dynamic decomposition: `decompose:` modifier for DAG expansion
  - Lazy context loading: `lazy: true` binding modifier
- **SpawnAgentTool** - Implements `rig::ToolDyn` for nested agent spawning
  - Depth limit protection (default: 3, max: 10)
  - Emits `AgentSpawned` event for observability
  - 17 unit tests + ToolDyn integration tests
- **DecomposeSpec** - Runtime DAG expansion via MCP traversal
  - Strategies: semantic, static, nested
  - `traverse:` arc specifier, `max_items:` limit
- **Lazy Bindings** - Deferred resolution until first access
  - `lazy: true` flag in `use:` block
  - `default:` fallback value
- **TraceWriter** - NDJSON execution traces in `.nika/traces/`
  - `nika trace list` and `nika trace show <id>` commands

### Changed
- Production mode uses `run_auto()` for automatic provider selection
- AgentParams includes `depth_limit` field

### Statistics
- **683+ tests passing**
- **spawn_agent**: 17 tests
- **decompose**: 12 tests
- **lazy bindings**: 8 tests

## [0.4.1] - 2026-02-18

### Fixed
- **Token Tracking** - Accurate counts in streaming mode (extended thinking)
  - `input_tokens`, `output_tokens`, `total_tokens` now populated
  - Uses rig's `GetTokenUsage` trait on `StreamedAssistantContent::Final`

### Changed
- `run_claude_with_thinking()` extracts tokens from streaming response

## [0.4.0] - 2026-02-17

### Breaking Changes
- **rig-core Migration** - Complete provider rewrite
  - Deleted: `ClaudeProvider`, `OpenAIProvider`, `provider/types.rs`
  - Deleted: `AgentLoop` (replaced by `RigAgentLoop`)
  - Deleted: `resilience/` module (never wired)
  - Deleted: `UseWiring` alias (use `WiringSpec`)

### Added
- **RigProvider** - Unified LLM provider wrapper for rig-core v0.31
  - `RigProvider::claude()` - Anthropic provider
  - `RigProvider::openai()` - OpenAI provider
  - 20+ providers available via rig-core
- **RigAgentLoop** - Agent loop using rig's `AgentBuilder`
  - `run_auto()` - Automatic provider selection
  - `run_claude()`, `run_openai()`, `run_mock()`
- **NikaMcpTool** - Implements `rig::ToolDyn` for MCP integration

### Changed
- All agent workflows now use rig-core
- MCP tools use `NikaMcpTool` wrapper

### Statistics
- **621+ tests passing**

## [0.3.0] - 2026-02-15

### Added
- **for_each Parallelism** - Parallel iteration with `tokio::spawn` JoinSet
  - `for_each:` array or binding expression
  - `as:` loop variable name
  - `concurrency:` max parallel executions
  - `fail_fast:` stop on first error
- **Schema v0.3** - `nika/workflow@0.3`

### Changed
- Task execution supports `for_each` modifier

## [0.2.0] - 2026-02-10

### Added
- **MCP Integration** - invoke: and agent: verbs
  - `invoke:` - Single MCP tool call
  - `agent:` - Multi-turn agentic loop with tool use
- **Schema v0.2** - `nika/workflow@0.2`
- **MCP Configuration** - `mcp:` block in workflow YAML

### Changed
- 5 semantic verbs now complete (infer, exec, fetch, invoke, agent)

## [0.1.0] - 2026-02-05

### Added
- **Initial Release** - DAG workflow runner for AI tasks
- **3 Core Verbs** - infer:, exec:, fetch:
- **DAG Execution** - Dependency-based task ordering
- **Binding System** - `use:` block and `{{use.alias}}` templates
- **EventLog** - 16 event variants for observability
- **TUI** - Terminal UI with ratatui (feature-gated)
- **Schema v0.1** - `nika/workflow@0.1`

[Unreleased]: https://github.com/supernovae-st/nika-dev/compare/v0.5.2...HEAD
[0.5.2]: https://github.com/supernovae-st/nika-dev/compare/v0.5.1...v0.5.2
[0.5.1]: https://github.com/supernovae-st/nika-dev/compare/v0.5.0...v0.5.1
[0.5.0]: https://github.com/supernovae-st/nika-dev/compare/v0.4.1...v0.5.0
[0.4.1]: https://github.com/supernovae-st/nika-dev/compare/v0.4.0...v0.4.1
[0.4.0]: https://github.com/supernovae-st/nika-dev/compare/v0.3.0...v0.4.0
[0.3.0]: https://github.com/supernovae-st/nika-dev/compare/v0.2.0...v0.3.0
[0.2.0]: https://github.com/supernovae-st/nika-dev/compare/v0.1.0...v0.2.0
[0.1.0]: https://github.com/supernovae-st/nika-dev/releases/tag/v0.1.0
