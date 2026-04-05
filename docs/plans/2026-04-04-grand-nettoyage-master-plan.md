# Grand Nettoyage — Master Plan v0.69+

> **Goal**: Stabilize, clean, polish, optimize Nika before May 5 launch.
> **Philosophy**: Zero dead code. Improve everything. Delete nothing. Elegant architecture.
> **Duration**: 13 sessions, 5 weeks, ~31 days.
> **Generated**: 2026-04-04 from 10-agent deep audit (all completed).

---

## Audit Summary

| Domain | Score | Key Finding |
|--------|-------|-------------|
| Crate Topology | A- | 18 crates, 380k LOC, 5-tier clean. nika-engine=44% too dominant |
| Engine Core | B+ | runner.rs 8,252 LOC monolith, for_each ~600 LOC duplication |
| Structured Output | A- | 5 layers robust, L1 ghost, cache naive clear-all |
| Builtin Tools | A | Clean trait architecture, Mutex→RwLock jq, inject path |
| Security | B+ | 1 HIGH + 5 MEDIUM, defense-in-depth mature |
| CLI & UX | B | Excellent onboarding, help drift, cost estimation weak |
| Provider | B | rig-core solid, mock sentinel, raw HTTP 3x duplication |
| TUI | B+ | Smart rendering, 5x verb duplication, Action enum bloat |
| Tests | B+ | 9,847 tests, auth.rs=0 tests(!), 24 untested error codes, 4 proptests |
| Dead Code | B+ | Clippy clean, 37 findings (13H, 14M, 10L), well-maintained |

---

## Session Plan

### Session 1 — Security Hardening (IMMEDIATE)

**Priority**: P0 — ship blockers
**Estimated scope**: ~15 targeted fixes

#### 1.1 Shell Quote-Breakout Fix (M-2)
- **File**: `nika-engine/src/runtime/executor/exec.rs`
- **Issue**: Bindings inside single quotes are exempt from `| shell` requirement, but resolved values containing `'` can break out of quoting
- **Fix**: When a binding is in single-quote context AND resolved value contains `'`, either require `| shell` or reject with NIKA-053
- **Test**: injection test with `value' && echo pwned && echo '`

#### 1.2 Exec Blocklist Gaps (H-1)
- **File**: `nika-engine/src/runtime/security.rs`
- **Issue**: `command `, `builtin `, `nohup `, `nice `, `timeout `, `strace ` missing from BLOCKLIST
- **Fix**: Add 6 shell prefix commands to BLOCKLIST
- **Test**: one test per prefix

#### 1.3 KDF Parameter Upgrade (M-5)
- **File**: `nika-vault/src/lib.rs:598`
- **Issue**: Argon2i memory=64KB, 300x below OWASP minimum
- **Fix**: `1 << 16` → `1 << 21` (2 MiB). Consider Argon2id.
- **Migration**: Existing vaults auto-migrate on next write (re-encrypt with new params)
- **Test**: verify vault roundtrip with new params

#### 1.4 Serve Percent-Encoding (M-4)
- **File**: `nika-serve/src/routes/workflows.rs`
- **Issue**: `validate_workflow_path` doesn't check `%2e%2e`, `%2f`, `%5c`
- **Fix**: Add percent-encoding checks matching SDK's `validate_path_segment`
- **Test**: percent-encoded traversal attempts

#### 1.5 Artifact Symlink Resolution (M-3)
- **File**: `nika-engine/src/io/security.rs`
- **Issue**: `validate_artifact_path` uses logical normalization, not `canonicalize()`
- **Fix**: After parent dir creation, call `validate_canonicalized_boundary()` before write
- **Test**: symlink-to-outside-dir test

#### 1.6 Shell Injection Patterns Extension (L-1)
- **File**: `nika-engine/src/runtime/security.rs`
- **Issue**: Data injection check only covers `$(`, backtick, `<(`, `<<<`
- **Fix**: Add `&&`, `||`, `;`, `>`, `>>`, `|` to `SHELL_INJECTION_PATTERNS` for single-quote-exempt bindings
- **Test**: each metachar pattern

---

### Session 2 — Engine Core: Runner Decomposition

**Priority**: P1 — architectural foundation
**Estimated scope**: ~1500 LOC refactored (not rewritten)

#### 2.1 Extract for_each Module
- **File**: `nika-engine/src/runtime/runner.rs` lines 2107-2710
- **Target**: New `nika-engine/src/runtime/for_each.rs`
- **Content**: `ForEachExpander` struct with unified item resolution (5 binding formats → 1 generic resolver), spawn logic, result aggregation
- **Dedup**: 600 LOC → ~200 LOC via single `resolve_items()` fn dispatching by `BindingSource`

#### 2.2 Extract Scheduling Module
- **File**: `nika-engine/src/runtime/runner.rs` lines 567-700
- **Target**: New `nika-engine/src/runtime/scheduler.rs`
- **Content**: `TaskScheduler` with `get_ready_tasks()`, `all_done()`, dependency checking
- **Optimize**: Consider bitset for completed tasks instead of O(remaining × deps)

#### 2.3 Unify DAG Representations
- **Files**: `dag/flow.rs` (HashMap Dag) + `dag/indexed.rs` (IndexedDag)
- **Target**: Single `IndexedDag` for both runtime and TUI
- **Action**: Runner switches from `Dag::get_dependencies(name: &str)` to `IndexedDag` O(1) lookups
- **Benefit**: Remove string-keyed dependency checks at runtime

#### 2.4 Dead Code Cleanup in Runner
- Remove `_completed` counter (never read)
- Remove `cookie_jar` and `fetch_cache` `#[allow(dead_code)]` fields from TaskExecutor
- Clean `#[allow(dead_code)]` on any other items

---

### Session 3 — Provider Layer Cleanup

**Priority**: P1 — architecture elegance
**Estimated scope**: ~500 LOC deduped

#### 3.1 Mock as RigProvider Variant
- **File**: `nika-engine/src/provider/rig/mod.rs`
- **Issue**: Mock is string sentinel `if provider_name == "mock"` scattered across infer.rs
- **Fix**: Add `RigProvider::Mock` variant implementing same interface
- **Benefit**: Exhaustive match, no special-case code

#### 3.2 Deduplicate Raw HTTP Logic
- **File**: `nika-engine/src/provider/rig/mod.rs`
- **Issue**: OpenAiCompat raw HTTP logic copied 3x (infer, infer_with_options, infer_with_tools)
- **Fix**: Extract `raw_openai_compat_request()` helper
- **Benefit**: ~300 LOC → ~100 LOC

#### 3.3 ModelResolver Everywhere
- **Files**: `nika-engine/src/runtime/rig_agent_loop/providers.rs`
- **Issue**: Agent loop hardcodes fallback model names bypassing ModelResolver
- **Fix**: Wire `ModelResolver::resolve()` into agent loop's per-provider dispatch
- **Benefit**: Single source of truth for default models

#### 3.4 Cost Estimation in CLI
- **File**: `nika-cli/src/verbs.rs` (explain command)
- **Issue**: Fixed-rate $0.003/infer formula ignores model/tokens
- **Fix**: Use actual `ModelPricing` from cost.rs for estimation

---

### Session 4 — Structured Output Polish

**Priority**: P2 — quality
**Estimated scope**: ~200 LOC changed

#### 4.1 Remove Ghost Layer 1
- **Files**: `nika-core/src/ast/structured.rs`, `nika-engine/src/runtime/structured_output.rs`
- **Issue**: `enable_extractor` silently accepted in YAML, documented as "future", never implemented
- **Fix**: Remove `enable_extractor` from `StructuredOutputSpec`, add NIKA-010 validation error if present in YAML
- **Migration**: Any workflow using `enable_extractor: true` gets a clear error instead of silent ignore

#### 4.2 LRU Cache for Schema/Validator
- **File**: `nika-engine/src/runtime/output.rs:40-56`
- **Issue**: Schema cache and validator cache use "clear all when cap exceeded" — cache thrash under many schemas
- **Fix**: Replace both `DashMap` caches with `mini_moka::sync::Cache` (LRU + TTL) or `lru` + `parking_lot::Mutex`
- **Capacity**: Keep 512 entries, evict LRU

#### 4.3 L0 Fallback Improvement
- **File**: `nika-engine/src/runtime/executor/infer.rs:724`
- **Issue**: L0a network failure skips L0b entirely (double-billing protection)
- **Fix**: Distinguish validation failures (skip L0b) from transport errors (try L0b)
- **Test**: Mock transport error → verify L0b is attempted

#### 4.4 Accurate Token Counts for L0b
- **File**: `nika-engine/src/runtime/executor/infer.rs:1305-1306`
- **Issue**: L0b uses `estimate_tokens()` instead of real provider counts
- **Fix**: Extract actual token usage from rig-core's `ChatResponse` / `ToolCallResult`

#### 4.5 Double File Read Fix
- **File**: `nika-engine/src/runtime/structured_output.rs:1132-1145`
- **Issue**: Standalone `validate_structured_output()` re-reads example file for key reordering
- **Fix**: Cache example value in the function scope

---

### Session 5 — TUI Polish & Deduplication

**Priority**: P2 — UX wow
**Estimated scope**: ~400 LOC deduped + visual improvements

#### 5.1 Extract verb_from_task_type
- **Files**: 5 files with identical match expression
- **Fix**: Single `VerbColor::from_task_type()` method
- **Test**: existing tests pass

#### 5.2 Consolidate App Constructor
- **File**: `nika-tui/src/app/mod.rs`
- **Issue**: `new()` and `new_standalone()` share 15 identical field inits
- **Fix**: `App::new_inner(config)` builder, both constructors delegate

#### 5.3 Clean Action Enum
- **File**: `nika-tui/src/app/types.rs`
- **Issue**: 39 variants, many unwired (ToggleBreakpoint, ExportTrace, etc.)
- **Fix**: Remove unwired variants. Add them back when actually implemented.

#### 5.4 Remove Dead ActivityStack Widget
- **File**: `nika-tui/src/widgets/activity_stack.rs`
- **Fix**: Keep data types (used by ChatView), remove dead rendering widget

#### 5.5 Deduplicate format_duration_ms
- **Files**: `views/command.rs:335` + engine display
- **Fix**: Move to shared `nika-core/src/util.rs` or `nika-tui/src/utils.rs`

#### 5.6 Monitor View Visual Polish
- Review 4-panel layout proportions
- Improve TaskBox visual density
- Verify DAG edge rendering at various terminal sizes
- Test all 3 themes (Dark, Light, Violet) for visual consistency

---

### Session 6 — CLI UX Cohérence

**Priority**: P2 — launch readiness
**Estimated scope**: ~300 LOC

#### 6.1 Fix Help Text Drift
- **File**: `nika/src/cli/help.rs:496`
- **Issue**: `topic_templates()` says "29 transforms" — actual count is 50
- **Fix**: Generate count dynamically from transform registry, or update to 50

#### 6.2 Complete Help Metadata
- **File**: `nika/src/cli/help.rs`
- **Issue**: `bench`, `explain`, `switch`, `vault`, `clean` missing from `get_short_desc()`/`get_example()`
- **Fix**: Add entries for all commands

#### 6.3 Typed Config Management
- **File**: `nika-cli/src/config.rs`
- **Issue**: `config get/set` manipulates TOML as raw string — fragile
- **Fix**: Deserialize to `NikaTomlConfig` struct, modify, re-serialize
- **Benefit**: Key validation, type safety, no TOML corruption

#### 6.4 Improve Explain Cost Estimation
- **File**: `nika-cli/src/verbs.rs`
- **Issue**: Fixed-rate formula ($0.003/infer, $0.05/agent_turn)
- **Fix**: Use `ModelPricing` tables + estimated token counts from workflow analysis

#### 6.5 Remove External Dependencies
- Replace `curl` subprocess in remote workflow download with `reqwest`
- Replace `tail -f` in daemon logs with Rust file watcher (notify crate already in deps)

#### 6.6 Error Message Audit
- Review all `NikaError` variants for help text quality
- Ensure every error has an actionable suggestion
- Test error display at different terminal widths

---

### Session 7 — Builtin Tools Hardening

**Priority**: P2 — correctness
**Estimated scope**: ~200 LOC

#### 7.1 JQ Cache: Mutex → RwLock
- **File**: `nika-core/src/binding/transform.rs:1110`
- **Issue**: `Mutex<lru::LruCache>` serializes all reads in for_each loops
- **Fix**: `parking_lot::RwLock<lru::LruCache>` (read-heavy workload)

#### 7.2 Fix nika:inject Path Validation
- **File**: `nika-engine/src/runtime/builtin/data/io.rs:60`
- **Issue**: Uses `std::env::current_dir()` instead of `ToolContext.working_dir`
- **Fix**: Pass `working_dir` from ToolContext to inject tool

#### 7.3 Add deny_unknown_fields to Tool Params
- **Files**: All builtin tool param structs
- **Issue**: Extra params silently ignored (serde default)
- **Fix**: Add `#[serde(deny_unknown_fields)]` to all `*Params` structs
- **Benefit**: Catch typos like `expresion` instead of `expression`

#### 7.4 Remove Deprecated nika:json_query
- **Files**: `nika-engine/src/runtime/builtin/data/jq.rs`, router.rs
- **Issue**: Still registered, emits warning on every call
- **Fix**: Remove entirely. Workflows using it get NIKA-107 "unknown tool"
- **Migration note**: Update all documentation/showcases that reference json_query

---

### Session 8 — Crate Architecture: Engine Decomposition

**Priority**: P1 — reduce nika-engine from 44% to ~35%
**Estimated scope**: crate extraction, zero behavior change

#### 8.1 Extract nika-builtins Crate
- **Source**: `nika-engine/src/runtime/builtin/` (61 tools, ~13k LOC)
- **Target**: New `tools/nika-builtins/` crate
- **Content**: `BuiltinTool` trait, `BuiltinToolRouter`, all tool implementations
- **Dependencies**: nika-core (types), nika-media (MediaOp bridge)
- **Benefit**: Independent testing, cleaner engine, tool discovery
- **Risk**: Low — tools already use trait boundary, clean cut

#### 8.2 Extract nika-display Crate
- **Source**: `nika-engine/src/display/` (6 renderers, ~8k LOC)
- **Target**: New `tools/nika-display/` crate
- **Content**: LiveRenderer, CliRenderer, format_event, summary, header, colors, dag_render
- **Dependencies**: nika-event (EventKind), nika-core (types)
- **Benefit**: Decouple rendering from execution, reusable by CLI/TUI/serve

#### 8.3 Workspace Cargo.toml Update
- Add new members to workspace
- Update all internal dependency declarations
- Verify feature forwarding chain still works
- Run full test suite

#### 8.4 Update CI & Release Scripts
- New crates in CI matrix
- Publish order for crates.io (if applicable)

---

### Session 9 — Rust Pro Optimization

**Priority**: P2 — performance & elegance
**Estimated scope**: analysis + targeted fixes

#### 9.1 Allocation Audit
- Profile hot paths: template resolution, binding resolution, for_each item dispatch
- Check for unnecessary `.to_string()` / `.clone()` on `Arc<str>` paths
- Verify `Cow::Borrowed` fast path in template engine is hit in common cases

#### 9.2 Async Pattern Review
- Audit `spawn_blocking` usage (should be for CPU-heavy only)
- Review `tokio::select!` biased branches for correctness
- Check for unnecessary `.await` chains that could be `join!`
- Verify `DashMap` usage patterns (no lock-then-await anti-pattern)

#### 9.3 Error Handling Elegance
- Review `NikaError` variant count — too many? Group by subsystem?
- Check for `unwrap()` in non-test code
- Verify all `?` chains produce useful error context (not just "unknown error")

#### 9.4 TaskExecutor Struct Optimization
- **File**: `nika-engine/src/runtime/executor/mod.rs`
- **Issue**: 20+ fields, cloned on every task spawn
- **Fix**: Split into `TaskExecutorShared` (Arc-wrapped, shared) and `TaskExecutorLocal` (per-task)
- **Benefit**: Reduce clone overhead, cleaner ownership

#### 9.5 get_ready_tasks Optimization
- **File**: `nika-engine/src/runtime/runner.rs:567`
- **Issue**: O(remaining × avg_deps) on every loop iteration
- **Fix**: Maintain a `ready_set` updated incrementally on task completion (reverse adjacency list)
- **Benefit**: O(completed_deps) per iteration instead of O(remaining)

---

### Session 10 — Test Quality Upgrade

**Priority**: P1 (auth.rs is P0) — confidence
**Estimated scope**: ~50 new tests, ~100 assertions upgraded

#### 10.1 [P0] auth.rs Test Suite
- **File**: `nika-serve/src/auth.rs` (58 LOC, 0 tests!)
- Add: valid token acceptance, invalid token rejection (401), missing header (401)
- Add: malformed "Bearer" prefix, health endpoint bypass
- Add: timing attack resistance verification (constant-time comparison)
- This is the security boundary of the HTTP API

#### 10.2 [P0] Serve Integration Tests
- Start actual `nika serve` server in test
- POST workflow, stream SSE events, verify completion
- Test concurrent workflow execution
- Currently zero integration tests for serve layer

#### 10.3 [P1] nika-storage Test Coverage
- 18 async public functions, only 2 tested
- Add: insert/get roundtrip, update_state transitions
- Add: increment_retry monotonicity, delete_old_jobs correctness
- Add: checkpoint save/load, artifact storage

#### 10.4 [P1] Security Test Suite for New Fixes
- Tests for all Session 1 fixes (shell quote-breakout, blocklist gaps, KDF, etc.)
- Add proptest for template parser (4,676 LOC handles arbitrary input)
- Add proptest for YAML parser (untrusted YAML edge cases)
- Test 24 untested error codes (NIKA-035/036/037/171/270/281/282...)

#### 10.5 [P1] Cross-Provider Structured Output Tests
- One test validating same schema against all 7 providers (mock mode)
- Verify L0a/L0b/L2 paths are exercised per provider

#### 10.6 [P2] Weak Assertion Cleanup
- Upgrade ~200 `assert!(result.is_ok())` → value validation with context
- Upgrade ~170 `assert!(!value.is_empty())` → content validation
- Add `#[serial]` to `nika-cli/src/onboarding.rs` tests (env var race)
- Remove `test_server_stub_compiles` (empty test body)

#### 10.7 [P2] Display & Rendering Tests
- `renderer.rs` (1,593 LOC) has zero tests
- Add insta snapshot tests for critical rendering paths
- TUI: verify all 3 views render without panic, all 3 themes
- Test event handling for all 58 EventKind variants

#### 10.8 [P3] CLI Subcommand Tests
- `workflow.rs` (666 LOC), `mcp.rs` (603 LOC), `pkg.rs` (587 LOC), `schema.rs` (497 LOC) have 0 tests
- Add smoke tests for each subcommand

---

### Session 11 — Dead Code & Legacy Purge

**Priority**: P2 — v0 philosophy
**Estimated scope**: 37 findings from audit (13 HIGH, 14 MEDIUM, 10 LOW)

#### 11.1 Workspace Cleanup (30 min)
- Remove `nika-napi` from workspace members (deprecated, link failure)
- Remove `nika-py` from workspace members (link failure)
- Drop 4 unused deps: `tokio-stream` (nika-sdk, nika-serve), `dirs` (nika-mcp), `serde` (nika-napi)
- Delete `JsonQueryTool` struct + router registration + tests (~100 LOC)
- Delete dead test helpers: `run_expect_fail`, `setup_with_working_memory`

#### 11.2 Legacy Fallback Removal (v0 = no legacy)
- Remove `.nika/config.toml` fallback from 5 files:
  - `nika-tui/src/config.rs:207-218`
  - `nika-tui/src/startup.rs:203-211`
  - `nika-cli/src/config.rs:26,172` (DotNika variant)
  - `nika-cli/src/doctor.rs:459-465`
  - `nika-engine/src/runtime/boot.rs:107,583-608` (ConfigSource::DotNika)
- Remove `{{use.alias}}` regex from LSP template validation (never valid in @0.12)

#### 11.3 LSP Dead Code Cleanup
- Delete `DaemonBridge` struct (entire struct annotated dead, not on launch roadmap)
- Delete 6 unused position.rs functions (`offset_to_position`, etc.)
- Delete unused `ast_integration` functions (`extract_task_ids_from_ast`, etc.)
- Delete dead fields: `DocumentState::line_count`, `ParsedWorkflow::success`, `TaskDefinition::span`, `TemplateRef::full_path`
- Delete `is_novanet_server()`, `get_task_line_range()`

#### 11.4 Engine Dead Code
- Remove `TaskExecutor::cookie_jar` and `fetch_cache` (infrastructure never wired)
- Remove `RigAgentLoop::mcp_clients` (held for unimplemented run_claude)
- Inline deprecated `border_color()` into `border_color_themed()` in TUI
- Clean TUI Action enum: remove unwired variants (ToggleBreakpoint, MouseClick*, etc.)

#### 11.5 Consolidate Duplicated Logic
- Unify 3 `find_project_root` implementations into nika-core
  - nika-cli/src/config.rs, nika-engine/src/core/mcp_config.rs, nika-tui/src/lib.rs
- Move test-only types to `#[cfg(test)]` modules:
  - `MediaType` enum + `from_mime()` (nika-media/src/types.rs)
  - `MediaProcessor::new()` and `with_budget()` (nika-media/src/processor.rs)

#### 11.6 Fix Incorrect Annotations
- Remove wrong `#[allow(dead_code)]` on items that ARE used:
  - `dependency_missing()` in nika-media (called by thumbhash/color)
  - `png_crc()` / `crc32_table()` in media tests (called within same file)
- Remove overly broad `#[allow(dead_code)]` on solarized palette module
- Use `_` prefix for RAII fields instead of dead_code annotation (TrackedTask, cancel_token)

#### 11.7 TODO/FIXME Triage
- Fix or track 2 remaining TODOs:
  - `nika-lsp-core/src/parse/recovery.rs:31` — tree-sitter API migration
  - `nika-tui/src/widgets/provider_modal/loader.rs:147` — native model discovery
- Verify zero other TODO/FIXME/HACK/XXX in production code

#### 11.8 SDK Dead Fields
- Remove unused deserialized fields: `ArtifactListResponse::job_id/count`, `RunResponse::status`, `SetupResult::message`

---

### Session 12 — Integration & Smoke Testing

**Priority**: P1 — launch confidence
**Estimated scope**: full pass

#### 12.1 Full Workflow Smoke Suite
- Run 10 representative workflows end-to-end (real providers)
- Cover: infer, exec, fetch, invoke, agent verbs
- Cover: for_each, structured output, vision, artifacts

#### 12.2 CLI Command Audit
- Execute every CLI command at least once
- Verify help text, error paths, edge cases
- Test `nika doctor --fix` on clean machine

#### 12.3 TUI Walkthrough
- Open every view, trigger every keybinding
- Run a workflow through Monitor view
- Edit a workflow in Studio view
- Verify all 3 themes

#### 12.4 nika serve Integration
- Start serve, hit every endpoint
- Concurrent workflow execution
- SSE streaming verification

---

### Session 13 — Final Polish & Documentation

**Priority**: P1 — launch
**Estimated scope**: docs + final touches

#### 13.1 CHANGELOG Update
- Document all changes from Grand Nettoyage
- Version bump strategy: v0.69.0 (cleanup) → v0.70.0 (launch candidate)

#### 13.2 README Polish
- Verify all claims match reality
- Update feature counts, transform counts
- Screenshots/recordings of TUI

#### 13.3 Error Code Documentation
- Verify NIKA-XXX error code table is complete
- Add any missing codes from new validation

#### 13.4 Help System Final Pass
- All commands have descriptions + examples
- Transform count matches reality
- Provider help shows correct status

#### 13.5 Release Candidate Build
- `cargo test --workspace --lib`
- `cargo clippy --workspace`
- Cross-platform build verification
- Tag v0.70.0-rc1

---

## Session Execution Order

```
Week 1 (Apr 7-11):   S1 Security     → S2 Engine Refactor
Week 2 (Apr 14-18):  S3 Provider     → S4 Structured Output → S5 TUI
Week 3 (Apr 21-25):  S6 CLI UX       → S7 Tools → S8 Crate Extraction
Week 4 (Apr 28-May 2): S9 Rust Pro   → S10 Tests → S11 Dead Code
Week 5 (May 2-5):    S12 Integration → S13 Polish → LAUNCH
```

## Dependencies

```
S1 (Security)     → independent, do first
S2 (Engine)       → independent, do early (foundation for later)
S3 (Provider)     → after S2 (runner changes affect provider dispatch)
S4 (Structured)   → after S3 (provider changes affect L0)
S5 (TUI)          → after S2 (runner decomposition affects event flow)
S6 (CLI)          → independent
S7 (Tools)        → independent
S8 (Crate Arch)   → after S2+S7 (extract modules that are already clean)
S9 (Rust Pro)     → after S2+S3+S8 (optimize after refactoring)
S10 (Tests)       → after S1-S8 (test the new code)
S11 (Dead Code)   → after S2-S8 (refactoring exposes dead code)
S12 (Integration) → after S1-S11 (validate everything)
S13 (Polish)      → last
```

## Metrics

| Metric | Before | Target |
|--------|--------|--------|
| nika-engine LOC | 168,000 | <120,000 (+ nika-builtins + nika-display) |
| runner.rs LOC | 8,252 | <3,000 (+ for_each.rs + scheduler.rs) |
| Crate count | 18 | 20 (+ builtins + display) |
| Security findings | 1H+5M | 0H+0M |
| Dead code findings | 37 (13H+14M+10L) | 0 |
| #[allow(dead_code)] | ~30 annotations | 0 unjustified (RAII fields use _ prefix) |
| TODO/FIXME count | 2 | 0 |
| Unused cargo deps | 4 | 0 |
| Test count | ~9,800 | 10,000+ |
| Help text accuracy | ~80% | 100% |
| Cross-provider SO tests | 0 | 7 providers |
| Circular deps | 0 | 0 (maintain) |

---

## Notes

- **Egghead (nika-memory)**: NOT in scope. Design complete, zero code. Post-launch only.
- **New features**: ZERO. This is stabilization only.
- **Backward compat**: v0 = 0 users. Break freely.
- **Each session**: 1 commit per logical fix. Test before commit.
- **Crate extractions**: Zero behavior change, pure structural moves.
- **Plan source**: 10-agent deep audit, all 10 completed. Full codebase coverage.
