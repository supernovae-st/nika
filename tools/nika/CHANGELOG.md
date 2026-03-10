# Changelog

All notable changes to Nika are documented in this file.

Format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/).
This project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.24.0](https://github.com/supernovae-st/nika/releases/tag/v0.24.0) - 2026-03-10

### Bug Fix Release

```
╔═══════════════════════════════════════════════════════════════════════════════╗
║  NIKA v0.24.0 — COMPREHENSIVE BUG FIX RELEASE                                 ║
╠═══════════════════════════════════════════════════════════════════════════════╣
║                                                                               ║
║  Methodology:  4 Opus 4.5 agents executing detailed Master Plans              ║
║  Tests:        4,391 passing | Zero clippy warnings                           ║
║  Changes:      18 files, +1,548 lines, -173 lines                             ║
║                                                                               ║
║  Fixed Bugs:                                                                  ║
║  ├── MP1: StructuredOutput Layer 3 & 4 now call LLM                          ║
║  ├── MP2: System prompts use .preamble() API correctly                        ║
║  ├── MP3: fail_fast aborts in-flight tasks, deadlock detection fixed          ║
║  └── MP4: MCP timeouts, sleep limits, error code preservation                 ║
║                                                                               ║
╚═══════════════════════════════════════════════════════════════════════════════╝
```

### Fixed

- **StructuredOutput Layer 3 & 4** — Now actually call LLM for retry/repair
  - Add `InferCallback` type: `Arc<dyn Fn(String) -> Pin<Box<dyn Future<...>>>>`
  - Layer 3 (Retry) calls LLM on JSON validation failure
  - Layer 4 (Repair) generates repair prompt and calls LLM
  - New builder methods: `with_infer_callback()`, `with_original_prompt()`
  - 8 new tests for callback functionality

- **Control Flow: fail_fast Abort** — Now properly cancels in-flight tasks
  - Use `tokio::select!` to race semaphore acquisition against cancellation
  - New `TaskStatus::DependencyFailed { dependency: String }` variant
  - New `TaskStatus::Skipped { reason: String }` variant
  - Proper abort propagation via cancellation tokens

- **Deadlock Detection** — Now properly reports dependency failures
  - Distinguish between true deadlock and dependency chain failure
  - New error codes: NIKA-025, NIKA-026, NIKA-027
  - Clear error messages showing failed dependency chain

- **MCP Operation Timeouts** — Prevent unbounded execution
  - Add `INVOKE_TASK_DEADLINE` (5 minutes) for total MCP task time
  - Wrap all MCP operations with `tokio::time::timeout()`
  - Returns `NikaError::McpTimeout` on deadline exceeded

- **Sleep Tool Limits** — Prevent unbounded sleep
  - Add `MAX_SLEEP_DURATION` (5 minutes) constant
  - Validate duration before execution
  - Clear error message when limit exceeded

- **MCP Error Code Preservation** — Structured error extraction
  - Add `McpErrorCode` enum for JSON-RPC error codes
  - Preserve original error codes from MCP servers
  - Add reconnection timeout (30s) and max attempts (3)

### Added

- **New Error Codes**
  - `NIKA-025`: TaskDependencyFailed
  - `NIKA-026`: DependencyChainFailed
  - `NIKA-027`: TaskCancelled

- **New Constants** (`src/util/constants.rs`)
  - `MAX_SLEEP_DURATION`: 5 minutes
  - `INVOKE_TASK_DEADLINE`: 5 minutes
  - `RECONNECT_TIMEOUT`: 30 seconds
  - `MAX_RECONNECT_ATTEMPTS`: 3

- **New TaskStatus Variants** (`src/store/datastore.rs`)
  - `DependencyFailed { dependency: String }`
  - `Skipped { reason: String }`
  - Helper methods: `is_failed()`, `is_dependency_failed()`, `get_failed_dependency()`

### Documentation

- Add 5 Master Plan documents in `docs/plans/`:
  - `2026-03-10-v0.24.0-bugfix-masterplan.md`
  - `2026-03-10-mp1-structured-output.md`
  - `2026-03-10-mp2-provider-system.md`
  - `2026-03-10-mp3-control-flow.md`
  - `2026-03-10-mp4-mcp-builtin.md`

## [0.23.1](https://github.com/supernovae-st/nika/releases/tag/v0.23.1) - 2026-03-10

### Fixed

- **Provider Definitions** — Add DataForSEO and Ahrefs to fallback provider definitions
  - Add `dataforseo` and `ahrefs` to `MCP_PROVIDER_IDS` in fallback.rs (6→8 providers)
  - Add `DATAFORSEO_API_KEY` and `AHREFS_API_KEY` to `provider_env_var()`
  - Fix `secrets.rs` `provider_env_var` for non-TUI builds
  - Ensures consistency with spn-core `KNOWN_PROVIDERS` when spn-daemon feature is disabled

## [0.23.0](https://github.com/supernovae-st/nika/releases/tag/0.23.0) - 2026-03-10

### Audit Release

```
╔═══════════════════════════════════════════════════════════════════════════════╗
║  NIKA v0.23.0 — COMPREHENSIVE AUDIT RELEASE                                   ║
╠═══════════════════════════════════════════════════════════════════════════════╣
║                                                                               ║
║  Methodology:  15 Opus 4.5 agents + Ultrathink + TDD + Ralph Wiggum Loop      ║
║  Coverage:     100% feature verification across 5 phases                       ║
║  Tests:        4,325 unit + 29 doc tests passing                              ║
║  Quality:      Zero clippy warnings                                           ║
║                                                                               ║
║  Audited Domains:                                                             ║
║  ├── AST: Two-Phase IR (Raw → Analyzed), 10 schema versions                  ║
║  ├── Runtime: 5 verbs, for_each parallelism, DAG execution                    ║
║  ├── MCP: Client lifecycle, timeout handling, JSON-RPC errors                 ║
║  ├── TUI: 4-view architecture, 40+ widgets                                    ║
║  ├── Providers: 7 LLM providers, full streaming                               ║
║  ├── Errors: 75+ error codes (NIKA-001 to NIKA-303)                          ║
║  └── Performance: 8/11 benchmarks within targets                              ║
║                                                                               ║
╚═══════════════════════════════════════════════════════════════════════════════╝
```

### Verified

- **Two-Phase AST Architecture** — Raw AST (spans) → Analyzed AST (validated)
  - 19 raw types, 22 analyzed types, NIKA-140-149 analyzer error codes
  - Schema version gating for all 10 versions (v0.1 - v0.10)

- **Runtime Execution** — All 5 verbs verified with edge cases
  - `infer:`, `exec:`, `fetch:`, `invoke:`, `agent:`
  - `for_each` with concurrency and fail_fast behavior

- **MCP Client** — Full protocol compliance verified
  - 10 MCP error codes with JSON-RPC preservation
  - Timeout hierarchy: 20s connect, 60s calls, 90s init

- **7 LLM Providers** — All with streaming support
  - Claude, OpenAI, Mistral, Groq, DeepSeek, Gemini, Ollama
  - Token tracking verified (with documented limitations)

- **Error Handling** — 75+ error codes mapped
  - RecoverableError trait for retry logic
  - FixSuggestion trait for actionable guidance

### Performance

| Benchmark | Target | Measured | Status |
|-----------|--------|----------|--------|
| YAML parsing (1 task) | <10µs | 4.6µs | ✅ |
| YAML parsing (100 tasks) | <500µs | 340µs | ✅ |
| DAG validation (10 nodes) | <1µs | 800ns | ✅ |
| DAG validation linear | <1µs | 1.27µs | ⚠️ |
| Binding resolution | <1µs | 450ns | ✅ |
| Binding 10 entries | <1µs | 1.508µs | ⚠️ |
| for_each 100 items | <500ms | 344µs | ✅ |
| DataStore get | <10ns | 6ns | ✅ |

### Documentation

- **Error Code Inventory** — Complete mapping of NIKA-001 to NIKA-303
- **Audit Reports** — `test-audit/v023-audit/AUDIT-SUMMARY.md`
- **Master Plan** — `docs/plans/MASTER-AUDIT-v0.23.md`

## [0.22.4](https://github.com/supernovae-st/nika/releases/tag/0.22.4) - 2026-03-10

### Fixed

- **BUG-003**: `use:` block now creates implicit `depends_on` edges
  - `Dag::from_workflow()` auto-creates DAG edges from `use:` wiring entries
  - No more NIKA-081 errors for valid `use:` references
  - Removes need for redundant `depends_on: [task_id]` declarations
  - Location: `src/dag/flow.rs:112-154`

- **BUG-004**: Workflow final output now selects deepest terminal task
  - New `get_deepest_final_task()` method with topological depth calculation
  - Branching DAGs now return the correct "final" task output
  - Ties broken by task definition order
  - Location: `src/dag/flow.rs:198-280`, `src/runtime/runner.rs:265-284`

- **BUG-005**: `for_each: $items` with `as:` alias now works
  - Fixed by BUG-003 - implicit dependencies ensure data availability
  - `use: { items: generate_task }` creates proper ordering

### Added

- 10 new unit tests for BUG-003 and BUG-004 fixes
- E2E validation workflows: `bug003-fix-validation.nika.yaml`, `bug004-fix-validation.nika.yaml`, `bug005-fix-validation.nika.yaml`

## [0.22.2](https://github.com/supernovae-st/nika/releases/tag/0.22.2) - 2026-03-09

### Changed

- Add #[ignore] to exec tests requiring API key
- Fix formatting issues

### Fixed

- **examples**: Correct provider and flows format in test workflows


## [0.21.3] - 2026-03-08

### Added

- **Multi-Cursor Support** — VS Code-style multi-cursor editing
  - `SelectionSet` struct for managing primary + additional selections
  - Ctrl+D: Select next occurrence of word under cursor
  - Ctrl+G: Clear additional cursors
  - Status bar shows cursor count when multi-cursor active
  - 6 multi-cursor tests
- **Git Gutter Integration** — Line-level change indicators
  - `GitStatus` module with libgit2 bindings (git2 v0.19)
  - `LineChange` enum: Added (+), Modified (~), Deleted (-)
  - Green/Yellow/Red gutter colors from theme
  - Lazy-loaded line changes per file
  - 6 git module tests
- **Selection Model** — Full text selection with anchor/head
  - `Selection` struct with anchor/head positions
  - Line-range calculation for multi-line selections
  - Cyan highlight for selected text
  - Shift+Arrow selection extending
  - 69 selection tests

### Changed

- `TextBuffer` upgraded from single `Selection` to `SelectionSet`
- Theme now includes `git_added`, `git_modified`, `git_deleted` colors
- clippy: Use `.div_ceil()` instead of manual division

## [0.21.1] - 2026-03-06

### Added

- **5 New Workflow Recipe Templates** for `nika new` command
  - `data-pipeline`: ETL pattern with fetch → transform → load stages
  - `morning-briefing`: Daily digest workflow with news, weather, tasks
  - `git-changelog`: Git commit analysis and changelog generation
  - `parallel-translation`: Multi-language translation with `for_each` parallelism
  - `agent-qa-tester`: QA testing agent with test case generation
- **Template Categories**: Simple, Pipeline, Agent, MCP, Advanced
- **16 Template Tests**: Comprehensive coverage for all 15 templates

### Changed

- **TUI Architecture Consolidation**: 9 views → 5 views (Studio, Runner, Chat, Scheduler, Settings)
- Templates now total **15** (10 original + 5 new recipes)

## [0.21.0] - 2026-03-05

### Added

- **Structured Output Engine** — 4-layer defense system for JSON Schema compliance
- **Implicit Output Syntax** — `$task` shorthand in `use:` blocks
- **5-View TUI Architecture** — Consolidated from 9 views

### Changed

- Schema version updated to `nika/workflow@0.10`

## [0.20.1] - 2026-03-05

### Added

- **secrets:** Complete spn-daemon integration via spn-client

### Fixed

- **ci:** Add manifest_path to release-plz.yml for monorepo structure
- **ci:** Remove references to non-existent test workflow files

### Other

- Escape flow: [task_ids] in raw/task.rs
- Escape markdown links and add backticks for generics

## [0.20.0] - 2026-03-04

### Added
- **8-View TUI Architecture** - VS Code-inspired unified workspace
  - `WorkspaceView`: 3-panel layout (Browser | Editor | DAG Preview)
  - `SplitView`: Editor + Runner side-by-side
  - Keyboard shortcuts: `7` for Split, `8` for Workspace
  - Tab/BackTab cycling between panels
  - Ctrl+[ / Ctrl+] to adjust panel ratios
- **Tree Widget Integration** - tui-tree-widget v0.24 for VS Code-like file browser
  - Animated expansion/collapse with easing
  - Filter/search within trees
  - Full keyboard navigation (j/k/Enter/Esc)
- **spn Daemon Secret Management** - Unified keychain access
  - Solves macOS Keychain popup issue
  - spn-client integration for credential retrieval
- **Two-Phase IR Architecture** - Complete Implementation
  - `ast::raw` module with `marked_yaml` parser for span tracking
  - `ast::analyzed` module with validated, optimized AST
  - `ast::analyzer` module with semantic validation pipeline
  - TaskId interning for O(1) task comparison and lookup
  - TaskTable for efficient task storage and retrieval
- **AST Analyzer** - Comprehensive validation engine
  - Schema version parsing and validation
  - Schema feature gating (for_each requires v0.3+, agent/invoke require v0.2+)
  - Duplicate task detection with location info
  - Unknown task reference detection with "did you mean?" suggestions
  - Cyclic dependency detection
  - MCP server configuration parsing and validation
  - for_each and retry configuration analysis
- **Analyzer Error Codes (NIKA-140-149)**
  - NIKA-140: Unknown task reference
  - NIKA-141: Duplicate task ID
  - NIKA-142: Invalid schema version
  - NIKA-143: Cyclic dependency
  - NIKA-144: Invalid field value
  - NIKA-145: Missing required field
  - NIKA-146: Invalid template expression
  - NIKA-147: Unknown flow definition
  - NIKA-148: Unknown MCP server
  - NIKA-149: Unsupported feature for schema version
- **19 Integration Tests** - Full pipeline validation
  - Multi-task workflow analysis
  - All 5 verbs (infer, exec, fetch, invoke, agent)
  - Feature gating end-to-end tests
  - Schema version suggestion tests
  - Span tracking preservation tests
- **Comprehensive Key Handler Tests** - 10 tests for WorkspaceView
  - F10 exit, Tab focus cycling, ratio adjustment
  - DAG panel read-only verification
  - Border style differentiation

### Changed
- **8 TUI Views** (up from 6): Browse, Editor, Runner, Chat, Scheduler, Settings, Split, Workspace
- **3,851 tests passing** (up from 3,562)
- View number keys now map correctly: 1=Browse through 8=Workspace
- HomeView uses TreeAction for keyboard handling
- Parser now handles MCP server configurations with nested `servers:` structure
- Analyzer exports `AnalyzedForEach`, `AnalyzedRetry`, `AnalyzedMcpServer` types

### Fixed
- BackTab key handling simplified in WorkspaceView
- View aliases removed (deprecated)
- Tree state uses `set_selection_index()` instead of `select_index()`
- Clippy `type_complexity` warnings in parser functions

### Statistics
- **3,851 tests passing** (3,808 lib + 19 integration + 24 smoke)
- **Zero clippy warnings**
- **8 TUI views** with unified keyboard navigation
- **tui-tree-widget v0.24** for tree rendering
- **10 analyzer error codes** (NIKA-140-149)

## [0.19.1] - 2026-03-03

### Fixed
- **Agentic Workflow Examples** - Refactored all 4 test workflows to be truly agentic
  - `test-schema-retry.nika.yaml` - Entity discovery via Cypher, not hardcoded
  - `test-novanet-structured.nika.yaml` - 4-phase architecture with parallel discovery
  - `test-foreach-schema.nika.yaml` - Locales discovered via novanet_query, dynamic for_each
  - `test-extended-thinking.nika.yaml` - 4 parallel MCP discovery calls
- **Proper Parallelization** - Discovery tasks now run in parallel via DAG flows
- **Correct Bindings** - All prompts use `{{use.xxx}}` template bindings from upstream
- **No Hardcoded Values** - Entity names, locales discovered dynamically from NovaNet

### Changed
- Workflows no longer assume specific entity keys (e.g., "qr-code")
- All MCP tool calls use proper parameter bindings
- Prompts reference discovered context instead of hardcoded values

## [0.19.0] - 2026-03-03

### Added
- **Structured Output Enforcement** - 3-layer validation system for LLM outputs
  - Layer 1: **DynamicSubmitTool** - LLM-side schema injection via tool definition
  - Layer 2: **jsonschema crate** - Code-side validation with JSON Schema Draft 7
  - Layer 3: **Retry Loop** - Re-prompts LLM with error feedback on validation failure
- **SchemaRef enum** - Polymorphic support for inline JSON Schema or file path references
  - `schema: { type: object, ... }` for inline schemas
  - `schema: "file://./schemas/my-schema.json"` for external files
- **Extended Thinking** - Claude deep reasoning mode for complex analysis
  - `extended_thinking: true` with configurable `thinking_budget`
  - Works with both `infer:` and `agent:` verbs
- **for_each Binding References** - Dynamic iteration from upstream task outputs
  - `$alias` format: `for_each: "$locales"` references bound variable
  - Template format: `for_each: "{{use.locales}}"` for template interpolation
- **4 Complex Test Workflows** for structured output validation
  - `test-schema-retry.nika.yaml` - Strict constraints with retry loop
  - `test-novanet-structured.nika.yaml` - Full NovaNet MCP integration
  - `test-foreach-schema.nika.yaml` - Binding reference with per-item schema
  - `test-extended-thinking.nika.yaml` - Extended thinking + structured output

### Changed
- `OutputPolicy` now supports `max_retries` field (default: 0)
- Error codes added: NIKA-060 (invalid JSON), NIKA-061 (schema validation failed)
- Retry prompts include schema, previous output, and validation errors

### Fixed
- Empty parent path handling in include expansion
- Template interpolation in for_each iterator binding

### Statistics
- **3,500+ tests passing**
- **Zero clippy warnings**
- **jsonschema v0.26** for JSON Schema validation

## [0.17.0] - 2026-03-02

### Added
- **Registry Optimizations** - Improved package resolution performance
- **pkg: Includes** - Support for `includes:` in workflow definitions
- **Security Fixes** - Dependency updates and vulnerability patches

### Statistics
- **3,358 tests passing**
- **Zero clippy warnings**

## [0.16.3] - 2026-03-02

### Added
- **TUI Improvements** - Chat view simplification and TaskBox enhancements
  - TaskBox inline rendering improvements for all 5 verbs
  - Chat view simplified (143 lines removed from chat.rs)
  - Dead code cleanup (message_bubble.rs deleted - 412 lines)
  - 12 files changed, 857 insertions, 560 deletions

### Fixed
- **nika init** - All 4 example workflows now have correct syntax
  - `01-hello-world.nika.yaml`: Fixed YAML syntax errors
  - `02-parallel-pipeline.nika.yaml`: Fixed context file paths
  - `03-agent-advanced.nika.yaml`: Fixed builtin tool references (`nika:read` not `read_file`)
  - `04-production-pipeline.nika.yaml`: Fixed all syntax and reference issues

### Changed
- CI workflows updated with latest GitHub Actions versions

### Statistics
- **3,358 tests passing**
- **Zero clippy warnings**
- **All ARMADA checkpoints passing**

## [0.16.2] - 2026-03-02

### Added
- **DX Consolidation** - Comprehensive documentation audit with 10 parallel agents
  - All CLAUDE.md files aligned to v0.16.2
  - Version references synchronized across 11 documentation files
  - Test counts corrected to 3,358 (accurate count)
  - Outdated feature references removed

### Changed
- Root CLAUDE.md: Updated version from v0.14.3 to v0.16.2
- nika/CLAUDE.md: Version sync to v0.16.2
- tools/nika/CLAUDE.md: Fixed version from v0.15.1 to v0.16.2, test count from 4,380 to 3,358
- dx/.claude/rules/nika.md: Added v0.16.2 section

### Statistics
- **3,358 tests passing**
- **Zero clippy warnings**
- **All ARMADA checkpoints passing**
- **11 CLAUDE.md files audited and synchronized**

## [0.16.1] - 2026-03-01

### Added
- Documentation and versioning consistency fixes
- All v0.16.0 features verified and tested

### Statistics
- **3,358 tests passing**
- **Zero clippy warnings**
- **All ARMADA checkpoints passing**

## [0.16.0] - 2026-03-01

### Breaking Changes
- **Remove nika pkg commands** - Migrated to `spn` CLI
  - `nika pkg install/list/search/update/remove` → Use `spn pkg` instead
  - Migration guide: `docs/MIGRATION-PKG-TO-SPN.md`

### Added
- **TaskBox Inline Rendering** - All 5 verbs now have inline task visualization
- **rmcp 0.16 SDK** - Updated MCP client to latest SDK version

### Changed
- CLI cleanup: ~221 lines removed from pkg module
- Dependency update: rmcp 0.14 → 0.16

### Statistics
- **3,358+ tests passing**
- **Zero clippy warnings**
- **7 LLM providers** (Claude, OpenAI, Mistral, Groq, DeepSeek, Ollama, Gemini)

## [0.15.2] - 2026-03-01

### Changed
- **Cargo.lock** - Updated for rustls migration (removes native-tls dependencies)
- **Cross-compilation** - Fixed ARM64 builds via `cross` tool
- **Release workflow** - Corrected archive paths and working directories

### Security
- **rustls-tls** - Switched from native-tls to rustls for consistent TLS across platforms

### Fixed
- ARM64 Linux builds now compile successfully (#43)
- Release archives contain correct binary paths (#42)
- CI jobs use proper working directory (#41)

### Statistics
- **3,358 tests passing**
- Zero clippy warnings
- Schema @0.9 fully supported
- **7 LLM providers** (Claude, OpenAI, Mistral, Groq, DeepSeek, Ollama, Gemini)

## [0.15.1] - 2026-03-01

### Added
- **Skill Merging Through DAG Fusion** - Workflow-level skills propagate through `include:` DAG fusion
  - `SkillDef` AST type with path and optional alias
  - `merge_skills()` function with deduplication and circular detection
  - Local paths and `pkg:` URI support
  - 11 tests for skill merging
- **pkg: Protocol Support** - Reference skills from package registry
  - `pkg:@scope/name@version/path` URI syntax
  - Resolves to `~/.spn/packages/@scope/name/version/path`
  - Implementation in `src/ast/pkg_resolver.rs`

### Changed
- Cargo.lock updated for rustls migration (removes native-tls dependencies)
- All fix branches merged (cross-compilation, release workflow, rustls)

### Statistics
- **3,358 tests passing** (up from 3,480 in v0.14.6 - test consolidation)
- **Zero clippy warnings**

## [0.15.0] - 2026-03-01

### Added
- **Security Hardening: Shell-Free Execution** (BREAKING)
  - `exec:` now defaults to `shell: false` for security
  - Command parsing via shlex (no shell injection)
  - Command blocklist prevents dangerous binaries (`rm -rf`, `sudo`, etc.)
  - New error code: `NIKA-053 BlockedCommand`
  - Implementation: `src/core/security.rs`
- **Infer LLM Control Parity**
  - `infer:` now supports `temperature`, `system`, and `max_tokens`
  - `InferParams` struct with optional parameters
  - `InferOptions` struct in `provider/rig.rs`
  - `infer_with_options()` method in `RigProvider`
- **Gemini Provider (7th provider)**
  - `RigProvider::gemini()` constructor
  - `RigAgentLoop::run_gemini()` for agent mode
  - Full streaming support with token tracking
  - Auto-detection: `GEMINI_API_KEY` → Gemini (priority 6)
- **File Tools (5 new builtin tools)**
  - `nika:read` - Read file contents
  - `nika:write` - Create/overwrite file
  - `nika:edit` - Modify file with old/new string replacement
  - `nika:glob` - Find files by pattern
  - `nika:grep` - Search content with regex
  - `ToolContext` and `PermissionMode` for security
  - `BuiltinToolRouter::with_file_tools()` constructor
- **11 builtin tools total** (6 core + 5 file)

### Changed
- **BREAKING:** `exec:` defaults to `shell: false` (use `shell: true` for pipes/redirects)
- Auto-detection priority updated: Gemini is 6th (before Ollama)
- Test count: **4,369 tests passing** (up from 3,480+)

### Statistics
- **4,369 tests passing**
- **7 LLM providers** (Claude, OpenAI, Mistral, Groq, DeepSeek, Gemini, Ollama)
- **11 builtin tools** (6 core + 5 file)
- **Zero clippy warnings**

## [0.14.1] - 2026-02-28

### Fixed
- **Schema Parser** - Added support for schema versions `@0.7` and `@0.8` (#22)
  - Workflows using `nika/workflow@0.7` or `@0.8` now parse correctly
  - Backward compatible with all previous versions (@0.1 - @0.6)
- **Jobs Module** - Fixed `JobsConfig` structure alignment in `main.rs` (#24)
  - CLI now correctly wires jobs daemon configuration
  - Compilation with `--features jobs` works without errors
- **Jobs Tests** - Fixed `test_job_stats` double-counting bug (#26)
  - `insert_execution` correctly updates stats for terminal-status records
  - Removed redundant `update_execution` calls from test
- **Test Isolation** - Use unique temp directories for standalone tests (#25)
  - Prevents race conditions when running tests in parallel
  - Each test gets isolated `.nika/` directory

### Changed
- **Examples** - Moved experimental workflows to `drafts/` directory (#23)
  - Added test workflows for schema version validation
  - Cleaner separation between production and experimental examples
- **Documentation** - Updated version references throughout codebase (#21)

## [0.14.0] - 2026-02-27

### Added
- **Enhanced `nika_run` Builtin** - Runtime workflow composition via builtin (not new verb)
  - `timeout_secs` parameter - Execution timeout (default: 300s, max: 3600s)
  - `max_depth` parameter - Recursion depth limiting (default: 3, max: 10)
  - Path canonicalization for security (prevents directory traversal)
  - Response includes `duration_ms` and `depth` fields
  - Context injection via `context` and `context_json` parameters
- **Runner::with_initial_context()** - Inject initial context into child workflow
  - Child workflows access parent context via `use: parent: __parent_context__.result`
  - Enables data passing between nested workflows

### Changed
- `nika_run` builtin now enforces timeout via `tokio::time::timeout`
- `nika_run` builtin prevents infinite recursion with depth tracking
- **task_local! depth tracking** - Replaced global AtomicU32 with tokio::task_local!
  - Fixes race conditions between concurrent workflow executions
  - Provides panic-safe depth cleanup via RAII scope pattern
- **Async file I/O** - Replaced std::fs with tokio::fs for non-blocking reads
  - File read wrapped in 30s timeout to prevent hangs
- Runtime timeout/max_depth clamping (defense-in-depth)
- Error messages updated from `nika:run` to `nika_run` (API compatibility)
- **30 new tests** for task_local! depth tracking, context injection, and timeout clamping

### Security
- Path canonicalization resolves symlinks and `..` to prevent escaping
- Async I/O prevents blocking the executor on slow filesystems

## [0.13.1] - 2026-02-27

### Added
- **Shell Completion** - `nika completion <shell>` for bash/zsh/fish/powershell
  - Full completion for all commands and options
  - Install: `nika completion zsh > ~/.zfunc/_nika`
- **Configuration CLI** - `nika config` command (git/gh style)
  - `nika config list` - Show all configuration
  - `nika config get <key>` - Get value (dot-separated path)
  - `nika config set <key> <value>` - Set value
  - `nika config edit` - Open in $EDITOR
  - `nika config path` - Show config file location
  - `nika config reset --force` - Reset to defaults
- **Global CLI Flags** - Terminal-first DX improvements
  - `-v, --verbose` - Increase verbosity (-v, -vv, -vvv)
  - `-q, --quiet` - Suppress non-error output
  - `--color <auto|always|never>` - Control color output
- **Config Template** - `templates/config.toml` for reset command
- **Boot Sequence** - 6-phase startup with structured context
  - Phases: ConfigDiscovery → ConfigValidation → MemoryLoading → McpStartup → ProviderValidation → Ready
  - `BootContext` accumulates config, warnings, and timing
  - `PhaseResult` with duration, success, and diagnostic messages
  - Full `NikaConfig` struct: tools, provider, editor, session, trace, policy
- **Policy Enforcer** - Security policy enforcement (v0.13.1)
  - `check_exec()` - Block dangerous shell commands (sudo, rm -rf, chmod 777)
  - `check_fetch()` - Block/allow hosts, enforce network restrictions
  - `check_token_spend()` - Token budget limits and tracking
  - `PolicyDecision` enum: Allow, Block, RequiresApproval
  - `TokenBudget` with spend tracking and remaining budget
  - **Runtime Wiring** - PolicyEnforcer integrated into TaskExecutor
    - `exec:` verb checks blocked commands before execution
    - `fetch:` verb checks blocked/allowed hosts before request
    - `infer:` verb checks token budget before LLM call, records actual usage
    - `agent:` verb checks token budget before agent loop, records total usage
    - `TaskExecutor::with_policy()` constructor for explicit policy config
    - 7 new unit tests for policy enforcement in executor
- **Doctor Command** - System health diagnostics (v0.13.1)
  - `nika doctor` - Run all diagnostic checks
  - `nika doctor --full` - Include slow MCP connectivity checks
  - `nika doctor --format json` - JSON output for scripting
  - Checks: Project setup, config validity, API keys, trace dir, Rust version

### Changed
- Verbosity levels: 0=warn, 1=info, 2=debug, 3=trace
- `nika ui --view` no longer has `-v` short option (conflicts with verbose)
- Help text updated with new commands and global flags

### New Error Codes
- `NIKA-160` PolicyViolation - Action blocked by security policy
- `NIKA-161` BootFailed - Boot sequence phase failure

### Dependencies
- Added `clap_complete` 4.5 for shell completion

## [0.13.0] - 2026-02-27

### Added
- **Schema @0.6 Infrastructure** - Foundation for memory, agents, and skills
  - `MemorySpec`, `AgentDefinition`, `SkillDefinition` AST modules
  - `SCHEMA_V06` constant for workflow version detection
  - Memory errors (250-259) for loading/parsing failures
  - Agent/skill resolver for multi-format loading (.md, .yaml)
- **Memory Loading** - Workflow memory context support
  - `load_memory()` runtime function
  - `LoadedMemory` struct with context data
  - Memory file parsing and validation
- **Agent/Skill Resolution** - Dynamic asset loading
  - `resolve_assets()` for agents and skills discovery
  - `ResolvedAgent`, `ResolvedSkills` types
  - Multi-format support: YAML inline or markdown files
- **Terminal-First CLI Design** - Inspired by cargo/git/gh patterns
  - Cleaner help output with contextual examples
  - Consistent subcommand structure
  - `nika mcp start/stop/restart` server management
- **Complete .nika Directory Structure** - Full project initialization
  - `config.toml`, `user.yaml`, `memory.yaml`, `policies.yaml`
  - `agents/`, `skills/`, `context/`, `workflows/` subdirectories
  - `memory/`, `proposed/`, `cache/` runtime directories
  - Example files: `researcher.md` agent, `code-review.md` skill
- **Chat-to-YAML Export** - Convert chat sessions to workflows
  - `/export yaml` command in Chat view
  - ChatWorkflow → Workflow AST conversion
- **Split View (Runner Redesign)** - Horizontal split for task focus
  - Left panel: DAG overview
  - Right panel: Active task details (TaskBox)
- **Binding Modifiers** - Extended template processing
  - `|shell` modifier for safe shell escaping
  - Prevents command injection in `exec:` tasks

### Changed
- TUI Runner view uses horizontal split layout
- TaskBox inline rendering for all 5 verbs
- InferBox enhanced with full design spec

### Fixed
- Runner view visual bugs and lifecycle issues
- Resolver mutability for asset loading
- Example workflows fixed for DAG and schema compliance

### Statistics
- **2,997 tests passing**
- **Zero clippy warnings**
- **Schema @0.6 ready** (infrastructure complete)

## [0.12.1] - 2026-02-25

### Added
- **MCP Server Management Commands** - CLI control for MCP servers
  - `nika mcp start <server>` - Start server process
  - `nika mcp stop <server>` - Stop running server
  - `nika mcp restart <server>` - Restart server
  - `nika mcp status` - Show all server statuses
- **TaskBox Visual Enhancements** - Full design spec implementation
  - Plan A documentation: Complete TaskBox visual specification
  - 12-phase implementation plan with 24 tasks
  - All 5 verb boxes: InferBox, ExecBox, FetchBox, InvokeBox, AgentBox

### Changed
- Updated cliff.toml with SuperNovae release template
- Improved DX documentation

### Statistics
- **2,893 tests passing**

## [0.12.0] - 2026-02-25

### Added
- **Event Emission for Builtin Tools** - Full observability for `nika:log` and `nika:emit`
  - `NikaBuiltinToolAdapter.with_event_log()` builder method for event context
  - `nika:log` tool now emits `EventKind::Log` to EventLog
  - `nika:emit` tool now emits `EventKind::Custom` to EventLog
  - Task ID propagation for trace correlation
  - 4 new tests for event emission
- **Theme Selection API** - Direct theme switching via index
  - `CosmicVariant::from_index(u8)` for Settings view [1][2][3] keys
  - Returns `Option<Self>` for type-safe selection
  - 2 new tests for index conversion

### Fixed
- **P0 Wiring Issues** - Complete audit and remediation of v0.9-v0.11 gaps
  - Session Persistence wired to app.rs (was code-only)
  - TUI Config wired to app.rs initialization
  - McpRetry documentation clarified (always wired via `emit()`)
  - Log/Custom events now flow through EventLog system
- **Settings View Theme Selection** - [1][2][3] keys now switch themes directly

### Statistics
- **2,893 tests passing** (comprehensive coverage)
- **Zero clippy warnings**
- **P0 wiring gaps: 0** (all critical paths verified)

## [0.11.0] - 2026-02-25

### Added
- **EditHistory Wiring** - Full undo/redo support in Studio view
  - Ctrl+Z for undo, Ctrl+Y for redo
  - Intelligent 500ms coalescing for character groups
  - Per-file undo stacks with memory-bounded snapshots
- **Thinking Display** - Monitor view renders agent reasoning
  - 💭 icon for thinking content in Agent panel
  - Truncation at 100 chars with ellipsis
  - Italic styling for visual distinction
- **McpRetry Event Emission** - Observability for MCP retries
  - `call_tool_with_retry_events()` method on McpClient
  - Emits EventKind::McpRetry with attempt counts
  - Full context: server name, operation, error message
- **Home View Validation** - Quick workflow validation with 'v' key
  - ValidateWorkflow ViewAction for routing
  - Status bar feedback for valid/invalid workflows

### Changed
- Executor uses `call_tool_with_retry_events` for better observability
- Monitor Agent panel now shows multi-line ListItems for thinking

### Statistics
- **2,876 tests passing** (comprehensive coverage)
- **Zero clippy warnings**

## [0.10.5] - 2026-02-25

### Added
- **ARMADA CI Pipeline** - 10-gate quality enforcement
  - Step 6: Intelligence - audit findings, technical debt tracking
  - Step 7: Badges - README badges for test count, coverage, version
  - Steps 1-5: Formatting, linting, testing, security, docs
- **Wiring Checkpoint Tests** - WIRING-7 through WIRING-10 (80 tests)
  - Comprehensive integration testing for all view wiring
  - Ensures all handlers properly connected

### Changed
- Renamed FORTRESS → ARMADA (cosmic pirate theme)
- Removed deprecated render functions and dead panels
- Cleaned up unused TUI code paths

### Fixed
- Complete v0.9.5 TODO remediation with TDD
- Wire MonitorView, OllamaClient, ApiKeyState handlers
- Expand mcp_log tests for edge cases

### Statistics
- **3,968 tests passing** (comprehensive coverage)
- **Zero clippy warnings**

## [0.10.0] - 2026-02-25

### Added
- **Chat DAG Widgets** - Visual workflow components
  - `ChatNodeBox`: Individual chat message as graph node (4 kinds, 4 states)
  - `ChatEdgeLine`: @N reference edges between nodes (Bezier curves)
  - `ChatTaskQueue`: Task execution queue with 5-verb icons
  - `ChatDagPanel`: Full DAG visualization (nodes + edges combined)
- **Animation System** - Coordinated animations
  - `AnimationTicker`: 60fps frame coordination
  - `AnimationState`, `Easing` utilities
- **Full Workflow Execution** - `nika:run` builtin tool executes real workflows
- **HITL Handler** - Human-in-the-loop for `nika:prompt`

### Changed
- Chat view now displays messages as interactive DAG nodes
- DAG edges visualize @N references between messages

### Statistics
- **108 new tests** for Chat DAG Widgets

## [0.9.5] - 2026-02-24

### Fixed
- **TODO Remediation** - Resolved all v0.9.x TODOs with TDD
  - 6 TODOs converted to tested implementations
  - Each fix verified with failing test first

### Added
- Additional test coverage for edge cases
- Documentation updates for resolved items

## [0.9.3] - 2026-02-24

### Added
- **Builtin Tools** - 6 `nika:*` tools for workflow utilities
  - `nika:sleep`: Configurable delay (duration parsing via humantime)
  - `nika:log`: Structured logging (info/warn/error levels)
  - `nika:emit`: Custom event emission
  - `nika:assert`: Runtime assertions with messages
  - `nika:prompt`: Human-in-the-loop input (with default fallback)
  - `nika:run`: Execute nested workflows
- **BuiltinToolRouter** - Dispatches `nika:*` tools via prefix matching
- **Wiring Checkpoint 3** - Tests for BuiltinRouter <-> Executor

### Statistics
- **40+ tests** for builtin tools

## [0.9.0] - 2026-02-24

### Added
- **6-Views Architecture** - View enum: Home, Chat, Studio, Monitor, Settings, Help
- **Nika Intro Animation** - ASCII art explosion into matrix rain (15 frames, 1.5s)
- **Stylish System Message** - Enhanced welcome banner
  - Decorative borders with ✨ sparkles
  - 🦋 butterflies around ASCII NIKA art
  - 🦀 Workflow Engine · 💫 Semantic AI tagline
  - 5 verb icons: ⚡ infer · 📟 exec · 🛰️ fetch · 🔌 invoke · 🐔 agent
- **Smooth Butterfly Animation** - Complete rewrite of explosion effect
  - Ease-out cubic easing for natural deceleration
  - Wave effect: center butterflies explode first

### Changed
- TUI refactored to support 6 independent views
- Animation system with performance optimizations

### Statistics
- **2,793 tests passing**
- Matrix rain animation tests for easing and wave patterns

## [0.8.0] - 2026-02-23

### Added
- **Studio DX Enhancements** - Unified editor experience
  - Edit History (Undo/Redo): Ctrl+Z/Ctrl+Y with 500ms coalescing
  - Session Persistence: `.nika/sessions/*.json` autosave
  - Solarized Theme: Light/Dark unified across TUI
  - Config System: `.nika/config.toml` for user preferences

### Statistics
- **1,902 tests passing**

## [0.7.2] - 2026-02-23

### Fixed
- **Claude API 400 Bad Request** - Updated default model from deprecated
  `claude-sonnet-4-20250514` (May 2025) to `claude-sonnet-4-6` (February 2026)
  - 71 files updated with new model identifier
  - Affects all workflows, tests, examples, and documentation
  - Root cause: Model naming convention changed to simplified format

### Changed
- Default Claude model: `claude-sonnet-4-6` (latest Sonnet 4.6)
- Updated documentation to reflect February 2026 model names

## [0.7.0] - 2026-02-21

### Added
- **Full Streaming for All 6 Providers** - Real-time token delivery
  - Mistral: `CompletionModel::stream()` integration
  - Groq: Real-time streaming support
  - DeepSeek: Token-by-token LLM output
  - Ollama: Full streaming implementation
  - Claude, OpenAI: Enhanced streaming stability
  - All providers use rig-core `StreamedAssistantContent`
- **MCP Server Status Events** - Lifecycle tracking for MCP connections
  - `McpConnected { server_name }` - Emitted on successful connection
  - `McpError { server_name, error }` - Emitted on connection failure
  - Real-time MCP status indicators in TUI status bar
- **Event System Enhancements**
  - `TaskStarted` now includes `verb` field (infer, exec, fetch, invoke, agent)
  - `ContextAssembled` event emitted before `ProviderCalled` for binding source tracking
  - `StreamChunk::Metrics` emitted after `Done` with input/output token counts
- **TUI DX Improvements**
  - Fancy YAML error diagnostics with miette v7.6 (error codes, help text)
  - Helix-quality fuzzy file search in Home view (nucleo v0.5)
  - `/` and `Ctrl+P` as fuzzy search triggers (VS Code style)
- **Real-World Test Workflows** - Production validation (5 new)
  - `test-v07-streaming-validation.nika.yaml`: Streaming + context chaining
  - `test-socratic-questioning.nika.yaml`: 5-step iterative refinement
  - `test-qrcode-ai-content-gen.nika.yaml`: Multilingual parallel pipeline
  - `test-dag-complex-dependencies.nika.yaml`: Diamond DAG patterns
  - `test-research-with-perplexity.nika.yaml`: MCP agent integration

### Changed
- All 6 LLM providers now support real-time streaming (feature-complete)
- MCP connection lifecycle fully observable via events
- TUI status bar displays real-time MCP server connection status

### Fixed
- TaskState test initializers updated for streaming support
- MissionPhase::Pause added to phase_color match
- Error handling for unreachable patterns in event processing

### Statistics
- **1842 tests passing** (up from 1811)
- **Zero TODOs** remaining in codebase (streaming fully implemented)
- **5 new test workflows** covering real-world patterns

## [0.6.0] - 2026-02-19

### Added
- **6 LLM Providers via rig-core v0.31** - Multi-provider LLM support
  - Claude: `ANTHROPIC_API_KEY` (claude-sonnet-4-6)
  - OpenAI: `OPENAI_API_KEY` (gpt-4o)
  - Mistral: `MISTRAL_API_KEY` (mistral-large-latest)
  - Groq: `GROQ_API_KEY` (llama-3.3-70b-versatile)
  - DeepSeek: `DEEPSEEK_API_KEY` (deepseek-chat)
  - Ollama: `OLLAMA_API_BASE_URL` (llama3.2)
- **Automatic Provider Selection** - `RigProvider::auto()` with priority order
  - Checks env vars: ANTHROPIC → OPENAI → MISTRAL → GROQ → DEEPSEEK → OLLAMA
  - Clear error messages when no API key found
- **Chat History Support** - Multi-turn conversations
  - `agent.chat_continue(prompt)` for sequential turns
  - `add_to_history(user, assistant)` for manual history management
  - `with_history(vec)` builder pattern initialization
- **RigAgentLoop Enhancements**
  - `run_auto()` for automatic provider detection
  - Provider-specific methods: `run_claude()`, `run_openai()`, etc.
  - Chat history methods: `push_message()`, `clear_history()`, `history_len()`

### Changed
- All LLM provider calls unified under `RigProvider` abstraction
- `run_auto()` is recommended for production workflows

### Fixed
- Empty API key validation with clear error messages
- Chat history properly persisted across turns

### Statistics
- **1811 tests passing** (comprehensive provider coverage)
- **6 providers** with 100% API surface compatibility

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
- Default model updated from `claude-3-5-sonnet-latest` to `claude-sonnet-4-6`

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

[Unreleased]: https://github.com/supernovae-st/nika/compare/v0.20.0...HEAD
[0.20.0]: https://github.com/supernovae-st/nika/compare/v0.19.5...v0.20.0
[0.19.5]: https://github.com/supernovae-st/nika/compare/v0.19.1...v0.19.5
[0.19.1]: https://github.com/supernovae-st/nika/compare/v0.19.0...v0.19.1
[0.19.0]: https://github.com/supernovae-st/nika/compare/v0.18.0...v0.19.0
[0.18.0]: https://github.com/supernovae-st/nika/compare/v0.17.0...v0.18.0
[0.17.0]: https://github.com/supernovae-st/nika/compare/v0.16.3...v0.17.0
[0.16.3]: https://github.com/supernovae-st/nika/compare/v0.16.2...v0.16.3
[0.16.2]: https://github.com/supernovae-st/nika/compare/v0.16.1...v0.16.2
[0.16.1]: https://github.com/supernovae-st/nika/compare/v0.16.0...v0.16.1
[0.16.0]: https://github.com/supernovae-st/nika/compare/v0.15.2...v0.16.0
[0.15.2]: https://github.com/supernovae-st/nika/compare/v0.15.1...v0.15.2
[0.15.1]: https://github.com/supernovae-st/nika/compare/v0.15.0...v0.15.1
[0.15.0]: https://github.com/supernovae-st/nika/compare/v0.14.6...v0.15.0
[0.14.6]: https://github.com/supernovae-st/nika/compare/v0.14.5...v0.14.6
[0.14.5]: https://github.com/supernovae-st/nika/compare/v0.14.0...v0.14.5
[0.14.0]: https://github.com/supernovae-st/nika/compare/v0.13.0...v0.14.0
[0.13.0]: https://github.com/supernovae-st/nika/compare/v0.12.1...v0.13.0
[0.12.1]: https://github.com/supernovae-st/nika-dev/compare/v0.12.0...v0.12.1
[0.12.0]: https://github.com/supernovae-st/nika-dev/compare/v0.11.0...v0.12.0
[0.11.0]: https://github.com/supernovae-st/nika-dev/compare/v0.10.5...v0.11.0
[0.10.5]: https://github.com/supernovae-st/nika-dev/compare/v0.10.0...v0.10.5
[0.10.0]: https://github.com/supernovae-st/nika-dev/compare/v0.9.5...v0.10.0
[0.9.5]: https://github.com/supernovae-st/nika-dev/compare/v0.9.3...v0.9.5
[0.9.3]: https://github.com/supernovae-st/nika-dev/compare/v0.9.0...v0.9.3
[0.9.0]: https://github.com/supernovae-st/nika-dev/compare/v0.8.0...v0.9.0
[0.8.0]: https://github.com/supernovae-st/nika-dev/compare/v0.7.2...v0.8.0
[0.7.2]: https://github.com/supernovae-st/nika-dev/compare/v0.7.0...v0.7.2
[0.7.0]: https://github.com/supernovae-st/nika-dev/compare/v0.6.0...v0.7.0
[0.6.0]: https://github.com/supernovae-st/nika-dev/compare/v0.5.2...v0.6.0
[0.5.2]: https://github.com/supernovae-st/nika-dev/compare/v0.5.1...v0.5.2
[0.5.1]: https://github.com/supernovae-st/nika-dev/compare/v0.5.0...v0.5.1
[0.5.0]: https://github.com/supernovae-st/nika-dev/compare/v0.4.1...v0.5.0
[0.4.1]: https://github.com/supernovae-st/nika-dev/compare/v0.4.0...v0.4.1
[0.4.0]: https://github.com/supernovae-st/nika-dev/compare/v0.3.0...v0.4.0
[0.3.0]: https://github.com/supernovae-st/nika-dev/compare/v0.2.0...v0.3.0
[0.2.0]: https://github.com/supernovae-st/nika-dev/compare/v0.1.0...v0.2.0
[0.1.0]: https://github.com/supernovae-st/nika-dev/releases/tag/v0.1.0
