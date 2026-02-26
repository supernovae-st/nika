# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.12.0] - 2026-02-26

### Added
- **ARMADA CI System** - 10-station quality checkpoint system
  - Station 1: Format (`cargo fmt --check`)
  - Station 2: Lint (`cargo clippy -- -D warnings`)
  - Station 3: Tests (`cargo nextest run` - 2,997 tests)
  - Station 4: Coverage (`cargo llvm-cov` >70%)
  - Station 5: Docs (`cargo doc --no-deps`)
  - Station 6: Security (`cargo audit` + `cargo deny`)
  - Station 7-8: AI Reviews (CodeRabbit + Claude)
  - Station 9: Conventional commits validation
  - Station 10: Version lock enforcement (0.x.x forever)
- **Version Lock Enforcement** - Nika will NEVER be v1.0.0
  - Rust tests (`tests/version_lock_test.rs`)
  - CI workflow (`.github/workflows/version-lock.yml`)
  - Claude Code hooks (PreToolUse blocks v1.x)
  - release-plz configured for 0.x.x
- **/ship Skill** - One-command shipping workflow
  - Detects changes → Creates branch → Commits → Pushes → Creates PR
  - Waits for CI → Enables auto-merge → Cleans up
- **6-Views Architecture** - Complete TUI restructure
  - View enum: Home, Chat, Studio, Monitor, Settings, Help
  - Full keyboard navigation (1-6 keys)
  - Cross-view state synchronization
- **TaskBox Widgets** - Compact/expanded modes with animations
  - `InferBox`, `ExecBox`, `FetchBox`, `InvokeBox`, `AgentBox`
  - `BorderPulse` animation for running state
  - `TokenVelocity` real-time metrics
  - `RenderMode` enum for detail levels

### Statistics
- **2,997 tests passing** (277 new in v0.10-v0.12)
- **Zero clippy warnings**
- **11 Claude Code skills**, **7 hooks**, **27 rules**

## [0.11.0] - 2026-02-25

### Added
- **Production Wiring** - Complete integration of all TUI components
  - Chat DAG widgets wired into ChatView
  - Settings and Help views integrated
  - MonitorView with View trait implementation
- **release-plz Automation** - Automated release PR creation
  - Conventional commits → CHANGELOG generation
  - git-cliff for changelog formatting
  - GitHub release creation

### Changed
- Rebrand FORTRESS → ARMADA (cosmic pirate theme)
- Version bump to v0.11.0

## [0.10.0] - 2026-02-25

### Added
- **Chat DAG Widgets** (108 tests)
  - `ChatNodeBox` - Individual chat message as graph node (4 kinds, 4 states)
  - `ChatEdgeLine` - @N reference edges between nodes (Bezier curves)
  - `ChatTaskQueue` - Task execution queue with 5-verb icons
  - `ChatDagPanel` - Full DAG visualization (nodes + edges combined)
- **Animation System**
  - `AnimationTicker` - 60fps coordinated animation utility
  - `AnimationState` - Running/Paused/Stopped states
  - Easing utilities for smooth transitions
- **Nika Intro Animation** - ASCII art explosion into matrix rain (15 frames, 1.5s)

### Statistics
- **2,720+ tests passing** (108 new chat widget tests)

## [0.9.0] - 2026-02-25

### Added
- **StableGraph Foundation** (v0.9.0) - Stable NodeIndex for chat DAG
  - `StableDag<T>` wrapper using petgraph::StableGraph
  - Stable NodeIndex preserved after node deletion
  - Edge cascading on node removal
  - 17 unit tests for stability guarantees
- **ChatWorkflow Struct** (v0.9.1) - DAG wrapper for chat messages
  - `ChatWorkflow` wraps `StableDag<ChatMessage>`
  - Auto-edge creation for sequential messages (1→2→3)
  - `add_message()` and `add_message_parallel()` methods
  - Thread-safe with `parking_lot::Mutex`
  - Message counter for @N references
  - 45 unit tests for workflow operations
- **@mention Binding System** (v0.9.2) - Reference previous messages
  - Parse `@N`, `@last`, `@all`, `@N..M` mention syntax
  - `MentionParser` with regex-based extraction
  - `resolve_mention()` converts to indices
  - `mentions_to_wiring()` generates WiringSpec bindings
  - `//` parallel marker detection
  - ChatWorkflow integration with auto-edge creation
  - 58 unit tests for parsing and resolution
- **Builtin Tools** (v0.9.3) - 6 nika:* prefixed tools
  - `nika:sleep` - Delay execution with millisecond precision
  - `nika:log` - Emit log events via tracing (trace/debug/info/warn/error)
  - `nika:emit` - Custom event emission with payload
  - `nika:assert` - Condition validation with custom messages
  - `nika:prompt` - Interactive user prompts (HITL integration)
  - `nika:run` - Sub-workflow execution with validation
  - `BuiltinToolRouter` with is_builtin(), dispatch(), extract_name()
  - All tools implement `BuiltinTool` trait with async dispatch
  - 96 unit tests (16 per tool)
- **WIRING Checkpoint Tests** - Integration validation
  - WIRING-0: StableDag foundation (3 tests)
  - WIRING-1: ChatWorkflow ↔ StableDag (6 tests)
  - WIRING-2: ChatWorkflow ↔ @mention Bindings (13 tests)
  - WIRING-3: BuiltinRouter ↔ Executor (13 tests)
  - 35 integration tests validating component wiring

### Architecture
- **Chat-as-DAG** - Unified architecture where chat = workflow DAG
  - Every message is a DAG node with stable index
  - @mentions create explicit data flow edges
  - Builtin tools provide workflow primitives
  - Foundation for TaskBox visualization (v0.10)

### Statistics
- **2,720+ tests passing** (216 new in v0.9.x + 35 WIRING tests)
- **Zero clippy warnings**
- v0.9.x adds 251 tests total (target was 131, exceeded by +120)

## [0.8.0] - 2026-02-23

### Added
- **Edit History (Undo/Redo)** - `src/tui/edit_history.rs` with intelligent coalescing
  - Ctrl+Z/Ctrl+Y support in ChatOverlayState
  - Intelligent grouping of rapid keystrokes (500ms timeout)
  - Preserve user intent across edits
  - 19 tests for edge cases and coalescing logic
- **Session Persistence** - `src/tui/session.rs` saves/loads chat conversations
  - Storage: `.nika/sessions/*.json` per session
  - Atomic writes using temp + rename pattern
  - Auto-cleanup to maintain max 50 sessions
  - Fast deserialization with serde
  - 13 tests for persistence and recovery
- **Solarized Theme** - Third theme option in theme system
  - `ThemeMode::Solarized` variant alongside Default and Custom
  - Based on Ethan Schoonover's color palette
  - High contrast for accessibility
  - Warmth and precision for terminal readability
- **Config System** - `.nika/config.toml` for persistent TUI preferences
  - `TuiSettings`: theme, font_size, ui_density
  - `ChatSettings`: auto_save, session_limit, history_limit
  - `StudioSettings`: auto_format, tab_width, line_numbers
  - `PathSettings`: custom session/trace directories
  - Type-safe TOML serialization with serde

### Statistics
- **1,879 tests passing**
- **Zero clippy warnings**

## [0.7.2] - 2026-02-23

### Added
- **GitHub Actions CI/CD** - Complete workflow automation
  - `ci.yml`: Format, clippy, test, coverage, security audit, build
  - `release.yml`: Cross-platform release binaries (Linux, macOS, Windows)
  - `dependabot.yml`: Automated dependency updates
- **Token Tracking for Standard Mode** - Streaming migration for accurate token counts
  - `StreamingResult` struct captures response, input_tokens, output_tokens, thinking
  - `stream_completion_with_tokens()` helper uses `model.stream()` for pure streaming
  - `stream_with_tools()` routes: streaming when no tools (full tokens), agent.prompt() when tools (0 tokens)
  - Token tracking works for Claude, OpenAI, Mistral, Groq, DeepSeek, Ollama when no tools

### Fixed
- **Token tracking returned 0 for non-thinking mode** - All `run_*()` methods now
  return accurate token counts via streaming API when no tools are used
  - Uses rig-core's `GetTokenUsage` trait on `StreamedAssistantContent::Final`
  - Chat methods (`chat_continue_*`) still return 0 tokens (rig-core `Chat` trait limitation)

### Statistics
- **2,323 tests passing**
- **6 LLM providers** with full token tracking (when no tools)

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
- Default model updated to `claude-sonnet-4-6`

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

[Unreleased]: https://github.com/supernovae-st/nika-dev/compare/v0.12.0...HEAD
[0.12.0]: https://github.com/supernovae-st/nika-dev/compare/v0.11.0...v0.12.0
[0.11.0]: https://github.com/supernovae-st/nika-dev/compare/v0.10.0...v0.11.0
[0.10.0]: https://github.com/supernovae-st/nika-dev/compare/v0.9.0...v0.10.0
[0.9.0]: https://github.com/supernovae-st/nika-dev/compare/v0.8.0...v0.9.0
[0.8.0]: https://github.com/supernovae-st/nika-dev/compare/v0.7.2...v0.8.0
[0.7.2]: https://github.com/supernovae-st/nika-dev/compare/v0.7.1...v0.7.2
[0.7.1]: https://github.com/supernovae-st/nika-dev/compare/v0.7.0...v0.7.1
[0.7.0]: https://github.com/supernovae-st/nika-dev/compare/v0.6.0...v0.7.0
[0.6.0]: https://github.com/supernovae-st/nika-dev/compare/v0.5.2...v0.6.0
[0.5.2]: https://github.com/supernovae-st/nika-dev/compare/v0.5.0...v0.5.2
[0.5.0]: https://github.com/supernovae-st/nika-dev/compare/v0.4.1...v0.5.0
[0.4.1]: https://github.com/supernovae-st/nika-dev/compare/v0.3.0...v0.4.1
[0.3.0]: https://github.com/supernovae-st/nika-dev/compare/v0.1.0...v0.3.0
[0.1.0]: https://github.com/supernovae-st/nika-dev/releases/tag/v0.1.0
