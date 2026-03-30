# v0.51 Session Handoff — Phase 1 Progress

**Date**: 2026-03-28
**Baseline at start**: 8613 tests, clean main branch
**Tests at end**: 8622 tests, all passing
**Commits**: 3 (mixed into auto-commits by pre-commit hooks)

## What Was Done (Phase 1: Bug Fixes)

### 1.1 — DONE: model.unwrap_or("default") → provider.default_model()
- **Commit**: `580ad5e90` (clean) + hourly_rate in `5d5a1de86` (mixed with docs)
- **Fix**: 5 occurrences in `infer.rs` replaced `model.unwrap_or("default")` with `model.unwrap_or_else(|| provider.default_model())`
- **Tests**: 2 new in cost.rs — `default_string_is_not_a_valid_model_name`, `all_provider_default_models_have_real_pricing`
- **Files**: `nika-engine/src/runtime/executor/infer.rs`, `nika-engine/src/provider/cost.rs`

### 1.2 — DONE: hourly_rate wired into cost calculation
- **Commit**: `5d5a1de86` (mixed with docs by hook)
- **Fix**: Added `calculate_hourly_cost()` in cost.rs, `endpoint_hourly_rate()` helper on TaskExecutor, wired main streaming path to prefer hourly_rate for custom endpoints
- **Tests**: 5 new — zero/negative duration, zero rate, basic calculations
- **Files**: `nika-engine/src/provider/cost.rs`, `nika-engine/src/runtime/executor/infer.rs`, `nika-engine/src/runtime/executor/mod.rs`

### 1.3 — DONE: Runtime template extraction synced with analyzer
- **Commit**: `5e1c43b0b` (mixed with docs by hook)
- **Fix**: Added exec.cwd, exec.env values, fetch.headers values, fetch.json to `extract_task_templates()` in validate.rs
- **Tests**: 2 new — `extract_templates_exec_includes_cwd_and_env`, `extract_templates_fetch_includes_headers_and_json`
- **Files**: `nika-engine/src/dag/validate.rs`

## What Remains (Phase 1 continued)

### 1.4 — IN PROGRESS: Agent cost hardcoded ProviderKind
- **Status**: Was reading the code when session stopped
- **Location**: `nika-engine/src/runtime/rig_agent_loop/providers.rs` lines 168-174, 581-587, plus ~10 more sites
- **Bug**: Each provider function (run_claude, run_openai, etc.) hardcodes its ProviderKind. Custom endpoints routed through OpenAiCompat get OpenAI pricing.
- **Fix approach** (from mega prompt): Use `ProviderKind::parse(provider_name)` instead of hardcoding. Or pass the ProviderKind from the caller.
- **Note**: This is architecturally correct for direct provider calls but wrong for custom endpoints. Evaluate if this is actually a bug or working as designed (custom endpoints DO use OpenAI-compatible API).

### 1.5 — TODO: for_each loop var validation
- **Location**: `nika-core/src/ast/analyzer/analyze.rs` line 1217, `nika-core/src/ast/raw/parser.rs` line 997
- **Bug**: `as_var` accepts any string without validation (empty, invalid identifiers, reserved words)
- **Fix approach**: Add identifier validation in analyzer

## Remaining Phases (2-6)

All untouched. See `docs/plans/2026-03-29-v051-enriched-mega-prompt.md` for full plan.

- **Phase 2**: 17 tests (routing, event serde, CRLF, bench edge cases)
- **Phase 3**: 6 refactors (rig.rs -800 LOC, module split)
- **Phase 4**: 8 perf fixes (bindings clone, strip_think_tags Cow)
- **Phase 5**: 4 telemetry (hdrhistogram, WorkflowCostSummary)
- **Phase 6**: 12 tasks (Level 4 Smart Routing)

## Key Findings from Analysis

- **Event serde roundtrip**: Already 38/38 variants tested (100%) — mega prompt's "38 of 60" was wrong
- **execute_with_routing()**: Confirmed ZERO tests — P0 priority
- **Empty for_each**: Confirmed zero events emitted (runner.rs:2235) — TUI spinners stuck
- **Pre-commit hooks**: Auto-commit unrelated docs with code changes. Use worktrees for clean commits.

## Codebase Notes

- `hourly_rate` is only wired into the main streaming path in infer.rs. The 4 other cost sites (structured output Layer 0, tool injection, non-streaming, vision) still use token-based cost. This is acceptable for now since streaming is the primary path.
- Test count: 8622 (was 8613 at session start)
