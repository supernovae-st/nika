# v0.51 Master Handoff — Next Session Mega Prompt

> **For Claude:** REQUIRED SUB-SKILL: Use `superpowers:executing-plans` to implement this plan.
> Use `superpowers:test-driven-development` for all code changes.
> Use `superpowers:verification-before-completion` before claiming anything is done.
> Commit granularly (1 fix = 1 commit). Push after each commit.

**Date**: 2026-03-29
**Current**: v0.50.0, schema @0.12, 8634 tests, all passing
**Branch**: main (push directly)

---

## Session Context

### What Was Done (last 2 sessions)

**Level 2 (nika bench)** — COMPLETE:
- `nika bench workflow.nika.yaml --providers anthropic,h100 -n 3`
- Rich display: header, speed, cost, profile (Gantt), quality (LLM-as-judge), verdict
- `--json` export, `--eval` quality evaluation, `--profile` Gantt bars
- Bench cache persistence (.nika/bench-cache/)
- ProviderCallStat.provider/model tracking, hourly_rate on endpoints

**Level 3 (Fallback Chains)** — COMPLETE:
- `routing: { fallback: [h100, anthropic] }` parsed from YAML
- `execute_with_routing()` wraps execute() with fallback loop
- `FallbackTriggered` event + `NIKA-037 FallbackChainExhausted` error
- classify_fallback_reason: rate_limited, auth_failed, timeout, structured_failure
- Display in both LiveRenderer and CliRenderer

**Security Audit (5 agents)** — 11 fixes pushed:
- ImageUrl DNS rebinding SSRF
- AWS_ACCESS_KEY_ID + cloud credentials in env strip
- CRLF header injection in fetch
- SVG xlink:href SSRF
- Bench division by zero

### Plans to Read (in order)

1. `docs/plans/2026-03-27-inference-routing-roadmap.md` — Levels 1-6 master roadmap (Level 1-3 done)
2. `docs/plans/2026-03-28-v1-master-plan.md` — v1.0 master plan (Phase 0-2)
3. `tools/nika/CLAUDE.md` — Crate structure, test commands, conventions
4. `tools/nika-engine/src/display/bench.rs` — Bench display (1192 lines)

---

## Wave 1: Audit Bug Fixes (5 tasks, ~30 min)

These are confirmed bugs from 5-agent deep audit. All have exact file locations and fix descriptions.

### Task 1.1 — CRITICAL: model.unwrap_or("default") wrong pricing

**Files:** `tools/nika-engine/src/runtime/executor/infer.rs` (lines 474, 626, 694, 840, 1165)

**Bug:** When model is `None`, cost calculation receives `"default"` string. The actual model used (e.g., `provider.default_model()`) is never passed.

**Fix:** Replace all `model.unwrap_or("default")` with `model.unwrap_or_else(|| provider.default_model())` in cost calls. Search for `unwrap_or("default")` in infer.rs.

**Test:** Verify cost is non-zero for a workflow without explicit model: field.

### Task 1.2 — IMPORTANT: hourly_rate on endpoints is dead code

**Files:** `tools/nika-engine/src/provider/endpoints.rs`, `tools/nika-engine/src/provider/cost.rs`

**Bug:** `ResolvedEndpoint.hourly_rate` is stored but never used. Custom endpoint cost falls back to DEFAULT_PRICING ($5/$15 per million) which is wrong for self-hosted GPUs.

**Fix:** In `cost.rs`, add a `calculate_hourly_cost(duration_secs: f64, hourly_rate: f64) -> f64` function. In executor, when provider is `OpenAiCompat` and endpoint has `hourly_rate`, use time-based cost instead of token-based.

**Test:** Verify custom endpoint with `hourly_rate: 3.0` reports time-based cost.

### Task 1.3 — IMPORTANT: Runtime template validation out of sync

**Files:** `tools/nika-engine/src/dag/validate.rs` (lines 267-299)

**Bug:** `extract_task_templates` (runtime path) doesn't check exec.env, exec.cwd, fetch.headers, fetch.json — but the analyzer-side `extract_templates_from_action` does.

**Fix:** Align both functions. Copy the missing extraction logic from the analyzer version (lines 72-125) to the runtime version (lines 267-299).

**Test:** Create a workflow with `{{with.alias}}` in exec env — should fail validation if alias is undeclared.

### Task 1.4 — IMPORTANT: Agent cost hardcodes ProviderKind

**Files:** `tools/nika-engine/src/runtime/rig_agent_loop/providers.rs` (lines 165-171, 545-551)

**Bug:** `run_claude()` hardcodes `ProviderKind::Claude`, `run_openai()` hardcodes `ProviderKind::OpenAI`. When auto-routing dispatches to wrong path, cost tracking uses wrong pricing.

**Fix:** Extract provider kind from `self.params.provider` using `ProviderKind::parse()` instead of hardcoding. Already done correctly in `run_generic_provider_impl`.

**Test:** Mock agent with `provider: groq` should not use Claude pricing.

### Task 1.5 — IMPORTANT: for_each loop var parsed as Task ref

**Files:** `tools/nika-core/src/ast/analyzer/analyze.rs` (line 783), `tools/nika-core/src/binding/entry.rs` (line 426)

**Bug:** `parse_with_entry(expr)` calls `BindingPath::parse(path_str)` without loop var hints. `$url` in a for_each context is classified as `BindingSource::Task("url")` instead of `LoopVar`.

**Fix:** Before processing with_refs, extract `as_var` from `raw.for_each`. Use `BindingPath::parse_with_loop_vars` or post-classify known loop var names.

**Test:** Workflow with `for_each: ... as: item` + `with: { item: $item }` should parse without error.

---

## Wave 2: Level 4 — Smart Routing (12 tasks)

**See:** `docs/plans/2026-03-27-inference-routing-roadmap.md` § Level 4

**Depends on:** Level 1 (done) + Level 3 (done)

**Key deliverables:**
1. `SmartRoutingConfig` in AST (capabilities, budget, priority)
2. `ProviderCapability` enum: Text, Json, Vision, Reasoning, ToolCalling, LongContext, Fast, Offline
3. Capability filtering (remove providers that can't handle task)
4. `Dag::critical_path_set()` — forward+backward longest-path
5. Scoring algorithm with configurable weights (cost/speed/quality/balanced)
6. Budget tracking with `Arc<AtomicU64>` micro-dollar fixed-point
7. Bench cache bootstrap for speed/quality scores
8. Wire router into executor
9. `SmartRouteDecision` event + NIKA-038/039 errors
10. Display routing decisions in live renderer
11. `nika run --explain-routing` flag
12. Tests

---

## Wave 3: Level 5 — Auto-Optimization (10 tasks)

**See:** `docs/plans/2026-03-27-inference-routing-roadmap.md` § Level 5

**Key deliverables:**
- `nika optimize workflow.nika.yaml --providers anthropic,h100,native --budget 0.05`
- Run bench internally, evaluate quality, solve assignment
- Generate `routing.rules` config from solver output
- Interactive apply with cliclack

---

## Wave 4: Architecture Improvements

### 4.1 — Cost system redesign

Current problems:
- `DEFAULT_PRICING` fallback produces wrong costs for unknown models
- Custom endpoints use token-based pricing (wrong for self-hosted)
- No cost tracking for native/local models
- Agent loop cost hardcodes ProviderKind

**Target:** Unified `CostCalculator` trait with implementations for:
- `TokenBasedCost` (cloud providers — pricing table lookup)
- `HourlyRateCost` (self-hosted — duration × hourly_rate)
- `FreeCost` (native, mock)

### 4.2 — Binding pipeline cleanup

- Unify `extract_task_templates` (runtime) with `extract_templates_from_action` (analyzer)
- Fix for_each loop var classification
- Add template validation for invoke params recursive extraction

### 4.3 — Display system improvements

- Show structured_attempts/success_layer in run summary
- Add fallback count to RunStats
- Fix bench header padding for non-ASCII workflow names

---

## Methodology

### Workflow: Question → Research → Skills → Test → Code → Verify → Commit

1. **Read plans** listed above before any code
2. **TDD**: Write test first, watch it fail, implement, verify
3. **Granular commits**: 1 logical change = 1 commit, push immediately
4. **Verification**: `cargo test --workspace --lib` (8634+ tests, always `--lib`)
5. **Pre-commit**: format + clippy must pass (hooks enforce this)
6. **Co-authors**: Always include both co-author lines

### Skills to use

| Skill | When |
|-------|------|
| `spn-powers:executing-plans` | Executing this plan |
| `spn-powers:test-driven-development` | All code changes |
| `spn-powers:verification-before-completion` | Before claiming done |
| `spn-powers:systematic-debugging` | When tests fail |
| `spn-rust:rust-core` | Rust patterns, error handling |
| `spn-rust:rust-async` | Tokio async patterns |

### Important constraints

- **Never `cargo test` without `--lib`** (keychain popups)
- **Never use `anyhow`** — always `NikaError` with NIKA-XXX codes
- **5 parallel Claude sessions** may be running — avoid editing files that other agents are actively modifying
- **Pre-commit hooks** run `cargo fmt` + `cargo clippy --all-targets` — both must pass
- **Zero backward compat** — only @0.12 schema matters

### Error code ranges

| Range | Category |
|-------|----------|
| 035-036 | Custom endpoints |
| 037 | Fallback chain exhausted |
| 038 | Routing budget exceeded (Level 4) |
| 039 | No capable provider (Level 4) |

---

## How to Start

```
1. Read this plan + inference-routing-roadmap.md + v1-master-plan.md
2. Run `cargo test --workspace --lib` to verify baseline (8634 tests)
3. Start Wave 1 (audit bug fixes) — task by task
4. Commit + push after each task
5. After Wave 1, start Wave 2 (Level 4 Smart Routing)
6. Between waves: run 5-agent audit to catch regressions
```

---

## Quick Reference

```bash
# Test
cargo test --workspace --lib             # All crates (8634+)
cargo test -p nika-engine --lib          # Engine only
cargo test -p nika-engine --lib -- display  # Display tests
cargo clippy --workspace --all-targets   # Zero warnings

# Run
nika bench workflow.nika.yaml --providers mock --iterations 1
nika bench workflow.nika.yaml --providers mock --json
nika run workflow.nika.yaml --provider mock --dry-run

# Files
tools/nika-engine/src/runtime/executor/mod.rs    # execute_with_routing()
tools/nika-engine/src/runtime/executor/infer.rs  # run_infer()
tools/nika-engine/src/provider/cost.rs           # calculate_cost()
tools/nika-engine/src/display/bench.rs           # bench display
tools/nika-core/src/ast/routing.rs               # RoutingConfig
tools/nika-event/src/log.rs                      # EventKind (44 variants)
```
