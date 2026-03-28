# v0.51 Enriched Mega Prompt — 6 Phases, Full Methodology

> **For Claude:** This is a comprehensive implementation plan produced by 11 audit agents.
> REQUIRED SKILLS: `spn-powers:executing-plans`, `spn-powers:test-driven-development`,
> `spn-powers:verification-before-completion`, `spn-powers:systematic-debugging`,
> `spn-rust:rust-core`, `spn-rust:rust-async`, `spn-rust:rust-architect`

**Date**: 2026-03-29
**Baseline**: v0.50.0, schema @0.12, 8634 tests, all passing
**Branch**: main (push directly, granular commits)

---

## Required Reading (DO THIS FIRST)

```
docs/plans/2026-03-29-v051-enriched-mega-prompt.md  # THIS FILE
docs/plans/2026-03-27-inference-routing-roadmap.md   # Levels 1-6 (1-3 done)
docs/plans/2026-03-28-v1-master-plan.md              # v1.0 master plan
tools/nika/CLAUDE.md                                  # Crate structure + conventions
```

---

## Phase 1: Confirmed Bug Fixes (5 tasks, ~45 min)

> Skill: `spn-powers:systematic-debugging` — root cause before fix

### 1.1 — CRITICAL: model.unwrap_or("default") → wrong cost

**Files:** `tools/nika-engine/src/runtime/executor/infer.rs`
**Lines:** 474, 626, 694, 840, 1165 (search `unwrap_or("default")`)

**Root cause:** When a task has no explicit `model:`, cost calculation receives the literal string `"default"` instead of the provider's actual default model. Falls back to DEFAULT_PRICING ($5/$15 per million) — wrong for every provider.

**Fix:**
```rust
// BEFORE:
model.unwrap_or("default")
// AFTER:
model.unwrap_or_else(|| provider.default_model())
```

**TDD test:** Create a mock workflow with `provider: mock` (no model:), verify cost is $0.00 (mock), not DEFAULT_PRICING.

### 1.2 — IMPORTANT: hourly_rate on custom endpoints is dead code

**Files:** `tools/nika-engine/src/provider/cost.rs`, `tools/nika-engine/src/provider/endpoints.rs`

**Root cause:** `ResolvedEndpoint.hourly_rate` is stored but never read by any cost calculation path. Custom endpoints use token-based pricing via `ProviderKind::OpenAI` fallback — wrong for self-hosted GPUs.

**Fix:** Add `calculate_hourly_cost()` to cost.rs. In infer executor, when provider is `OpenAiCompat` and endpoint has `hourly_rate`, use time-based cost:
```rust
pub fn calculate_hourly_cost(duration_secs: f64, hourly_rate: f64) -> f64 {
    (duration_secs / 3600.0) * hourly_rate
}
```

**TDD test:** Custom endpoint with `hourly_rate: 3.0`, 60s workflow → cost should be $0.05.

### 1.3 — IMPORTANT: Runtime template validation out of sync

**Files:** `tools/nika-engine/src/dag/validate.rs` (lines 267-299)

**Root cause:** `extract_task_templates` (runtime path, called from `validate_bindings`) doesn't check `exec.env`, `exec.cwd`, `fetch.headers`, `fetch.json` — but the analyzer-side `extract_templates_from_action` (lines 72-125) does.

**Fix:** Align both functions. Add to the runtime version:
```rust
// Exec
if let Some(ref cwd) = exec.cwd { templates.push(cwd.clone()); }
for value in exec.env.values() { templates.push(value.clone()); }
// Fetch
for value in fetch.headers.values() { templates.push(value.clone()); }
if let Some(ref json) = fetch.json { collect_string_values(json, &mut templates); }
```

**TDD test:** Workflow with `exec: { env: { TOKEN: "{{with.undeclared}}" } }` should fail validation.

### 1.4 — IMPORTANT: Agent cost hardcodes ProviderKind

**Files:** `tools/nika-engine/src/runtime/rig_agent_loop/providers.rs` (lines 165-171, 545-551)

**Root cause:** `run_claude()` hardcodes `ProviderKind::Claude`, `run_openai()` hardcodes `ProviderKind::OpenAI` in cost tracking. When auto-routing dispatches to wrong path, costs are wrong.

**Fix:** Use `ProviderKind::parse()` from the actual provider name instead of hardcoding:
```rust
let pk = self.provider_kind.unwrap_or_else(|| {
    ProviderKind::parse(provider_name).unwrap_or(ProviderKind::OpenAI)
});
```

### 1.5 — IMPORTANT: for_each loop var parsed as Task reference

**Files:** `tools/nika-core/src/ast/analyzer/analyze.rs` (line 783), `tools/nika-core/src/binding/entry.rs` (line 426)

**Root cause:** `parse_with_entry(expr)` calls `BindingPath::parse()` without loop var hints. `$item` in a for_each is classified as `BindingSource::Task("item")` instead of `LoopVar("item")`.

**Fix:** Extract `as_var` from `raw.for_each` before processing with_refs. Post-classify: if parsed source task_id matches the known loop variable name, skip the implicit dep extraction.

**TDD test:** Workflow with `for_each: items, as: item, with: { item: $item }` should parse without error.

---

## Phase 2: Paranoid Test Wave (10 tasks, ~1h)

> Skill: `spn-powers:test-driven-development` — RED-GREEN-REFACTOR for each

### Missing test categories (from test coverage audit):

| # | Test | File | What it covers |
|---|------|------|----------------|
| 2.1 | `test_execute_with_routing_fallback_success` | executor/mod.rs tests | Fallback from failing to passing provider |
| 2.2 | `test_execute_with_routing_chain_exhausted` | executor/mod.rs tests | All providers fail → NIKA-037 |
| 2.3 | `test_execute_with_routing_single_provider` | executor/mod.rs tests | Single-provider chain (no loop) |
| 2.4 | `test_execute_with_routing_non_llm_verb` | executor/mod.rs tests | exec/fetch bypass routing |
| 2.5 | `test_fallback_triggered_event_serialization` | nika-event tests | FallbackTriggered serde roundtrip |
| 2.6 | `test_bench_zero_duration_no_panic` | display/bench tests | Division by zero guard |
| 2.7 | `test_for_each_zero_items` | runner.rs tests | Empty for_each produces empty array |
| 2.8 | `test_routing_yaml_parsed_correctly` | AST parser tests | routing: { fallback: [a, b] } deserializes |
| 2.9 | `test_cost_custom_endpoint_model_with_slash` | cost.rs tests | "meta-llama/Llama-3.1-70B" falls to default |
| 2.10 | `test_classify_fallback_reason_rate_limited` | executor/mod.rs tests | 429 in error message → "rate_limited" |

### Mock OpenAI server for integration tests

Use **httpmock** crate for contract testing:
```toml
[dev-dependencies]
httpmock = "0.8"
```

```rust
#[tokio::test]
async fn test_openai_compat_endpoint() {
    let server = MockServer::start_async().await;
    server.mock_async(|when, then| {
        when.path("/v1/chat/completions").method(POST);
        then.status(200).json_body(json!({
            "choices": [{"message": {"content": "hello"}}],
            "usage": {"prompt_tokens": 10, "completion_tokens": 5}
        }));
    }).await;
    // Test RigProvider::OpenAiCompat against mock
}
```

---

## Phase 3: Rust Architecture Refactor (6 tasks, ~2h)

> Skill: `spn-rust:rust-architect` — trait extraction, module splitting

### 3.1 — Extract generic infer helper to eliminate 8-arm match duplication

**File:** `tools/nika-engine/src/provider/rig.rs` (3,598 lines → ~2,800)

Create `infer_generic<C: ProviderClient>()` helper. Reduces every `infer()`, `infer_with_options()`, `infer_with_tools()` from 8-arm match to 2-arm (Native vs generic).

**Impact:** -800 LOC, fewer bugs per new provider.

### 3.2 — Split rig.rs into sub-modules

| New file | Contents | Saves |
|----------|----------|-------|
| `provider/infer.rs` | infer(), infer_with_options() | ~600 LOC |
| `provider/stream.rs` | infer_stream(), consume_rig_stream() | ~600 LOC |
| `provider/vision.rs` | infer_vision(), infer_vision_stream() | ~400 LOC |
| `provider/mcp_tool.rs` | NikaMcpTool, ToolDyn impl | ~300 LOC |

### 3.3 — Group TaskExecutor fields

```rust
struct ExecutorInfra { http_client, cas, builtin_router, policy_enforcer, cancel_token }
struct WorkflowConfig { default_provider, default_model, skills_map, endpoints, agents }
```

15 fields → 5 composable groups.

### 3.4 — Move StreamChunk to nika-event or runtime/stream.rs

23-variant enum mixing stream data with TUI activity events. Separate into `StreamDelta` (core) + `ActivityEvent` (TUI).

### 3.5 — Unify ProviderKind with core catalog

`ProviderKind::parse()` reimplements alias resolution from `core::find_provider()`. Eliminate duplication.

### 3.6 — Extract ProviderCallCorrelator from RunStats

`pending_providers: HashMap` in RunStats is bookkeeping, not stats. Separate struct.

---

## Phase 4: Performance Optimization (5 tasks, ~1h)

> Skill: `spn-rust:rust-perf` — allocation hunting, lock contention

### 4.1 — Eliminate bindings.to_value() deep clone

**File:** `executor/infer.rs:128`, `runner.rs:958`

Replace `to_value()` (O(n) deep clone) with `bindings.iter()` (zero-copy references). Saves ~1MB/loop for for_each with large payloads.

### 4.2 — Avoid action.clone() in routing fallback

**File:** `executor/mod.rs:455`

Clone only the provider field, not the entire action. For vision tasks with base64 images, this avoids megabyte-scale clones per fallback attempt:
```rust
// Instead of cloning entire action, pass provider override separately
fn execute_with_provider_override(&self, task_id, action, provider_override, ...)
```

### 4.3 — Replace RwLock<Vec<Event>> with append-only log

**File:** `nika-event/src/log.rs`

EventLog uses `RwLock<Vec<Event>>` which blocks all readers during any write. For high-frequency `StreamingDelta` events, this creates contention. Consider `crossbeam::queue::SegQueue` or `parking_lot::RwLock` (already used elsewhere).

### 4.4 — Cache template resolution results

**File:** `binding/template.rs`

Templates like `{{with.data}}` are resolved on every access. For immutable bindings, cache the resolved value after first resolution.

### 4.5 — Lazy event serialization in trace writer

**File:** `nika-event/src/trace.rs`

`TraceWriter` serializes every event to JSON. For workflows with 1000+ events, defer serialization to a background task.

---

## Phase 5: Telemetry & Observability (4 tasks, ~1h)

> Skill: `spn-rust:rust-async` — tokio integration

### 5.1 — Add OpenTelemetry trace spans

```toml
[dependencies]
opentelemetry = { version = "0.31", features = ["trace"] }
opentelemetry_sdk = { version = "0.31", features = ["rt-tokio"] }
tracing-opentelemetry = "0.32"
```

Add `#[instrument]` spans to: `Runner::run()`, `TaskExecutor::execute()`, `run_infer()`, `run_agent()`.

### 5.2 — Add workflow-level metrics

Track per-workflow: total_cost_usd, total_tokens, total_duration_ms, task_count, error_count. Expose via `RunStats` summary.

### 5.3 — Add structured_attempts/success_layer to run summary

Currently accumulated in RunStats but never displayed. Show in summary: "structured output: 3 attempts, succeeded at layer 2".

### 5.4 — Add fallback_count to RunStats

Count FallbackTriggered events. Display in summary: "routing: 2 fallbacks triggered".

---

## Phase 6: Level 4 — Smart Routing (12 tasks, ~3h)

> See: `docs/plans/2026-03-27-inference-routing-roadmap.md` § Level 4

### Key tasks:

1. `SmartRoutingConfig` in AST (capabilities, budget, priority)
2. `ProviderCapability` enum: Text, Json, Vision, Reasoning, ToolCalling, LongContext
3. Capability filtering (remove providers that can't handle task)
4. `Dag::critical_path_set()` — forward+backward longest-path
5. Scoring algorithm with configurable weights
6. Budget tracking with `Arc<AtomicU64>` micro-dollar fixed-point
7. Bench cache bootstrap for speed/quality scores
8. Wire router into executor
9. `SmartRouteDecision` event + NIKA-038/039 errors
10. Display routing decisions in live renderer
11. `nika run --explain-routing` flag
12. Tests with mock providers

---

## Methodology: Phase-by-Phase Verification

### Before each phase:

```bash
git status                              # Must be clean
cargo test --workspace --lib            # Baseline (8634+ tests)
cargo clippy --workspace --all-targets  # Zero warnings
```

### During each task:

1. **Read** the code before changing it
2. **Write test first** (RED) — test must fail
3. **Implement** the fix (GREEN) — test passes
4. **Verify** — `cargo test --workspace --lib`
5. **Commit** — granular, 1 logical change
6. **Push** — immediately after commit

### After each phase:

```bash
# Full verification checklist
cargo test --workspace --lib             # All tests pass
cargo clippy --workspace --all-targets   # Zero warnings
cargo fmt --check                        # Formatted
nika bench /tmp/bench-test.nika.yaml --providers mock -n 1  # Bench works
nika run /tmp/bench-test.nika.yaml --provider mock          # Run works
nika check /tmp/bench-test.nika.yaml                        # Check works
```

### Between phases: Launch 3-5 audit agents

```
spn-powers:code-reviewer — review all changes from this phase
spn-rust:rust-architect — verify patterns are correct
spn-rust:rust-security — check for new vulnerabilities
```

### Skills to use per phase:

| Phase | Primary Skill | Secondary |
|-------|--------------|-----------|
| 1 (Bug fixes) | `systematic-debugging` | `test-driven-development` |
| 2 (Tests) | `test-driven-development` | `verification-before-completion` |
| 3 (Refactor) | `rust-core`, `rust-architect` | `requesting-code-review` |
| 4 (Perf) | `rust-perf` | `verification-before-completion` |
| 5 (Telemetry) | `rust-async` | `rust-core` |
| 6 (Smart Routing) | `executing-plans` | `test-driven-development` |

---

## Constraints

- **Never `cargo test` without `--lib`** (keychain popups)
- **Never `anyhow`** — always `NikaError` with NIKA-XXX codes
- **5 parallel Claude sessions** may run — check `git status` before editing
- **Pre-commit hooks** enforce fmt + clippy --all-targets
- **Zero backward compat** — only @0.12 matters
- **Commit format**: `type(scope): description` with both co-authors

### Error code allocation:

| Code | Purpose | Phase |
|------|---------|-------|
| NIKA-037 | FallbackChainExhausted | Done (Level 3) |
| NIKA-038 | RoutingBudgetExceeded | Phase 6 (Level 4) |
| NIKA-039 | NoCapableProvider | Phase 6 (Level 4) |

---

## Quick Reference

```bash
# Test
cargo test --workspace --lib              # 8634+ tests
cargo test -p nika-engine --lib -- routing  # Routing tests
cargo test -p nika-engine --lib -- display  # Display tests

# Dev dependencies for mock server
# Add to nika-engine/Cargo.toml [dev-dependencies]:
# httpmock = { version = "0.8", features = ["standalone"] }

# Key files
tools/nika-engine/src/provider/rig.rs           # 3,598 lines (split target)
tools/nika-engine/src/provider/cost.rs          # Pricing tables
tools/nika-engine/src/runtime/executor/mod.rs   # execute_with_routing()
tools/nika-engine/src/runtime/executor/infer.rs # run_infer()
tools/nika-engine/src/runtime/runner.rs         # Task execution loop
tools/nika-core/src/ast/routing.rs              # RoutingConfig
tools/nika-core/src/binding/entry.rs            # BindingPath parsing
tools/nika-event/src/log.rs                     # EventKind (44+ variants)
```

---

## How to Start

```
1. Read this prompt + routing roadmap + v1 master plan
2. cargo test --workspace --lib (verify 8634+ baseline)
3. Phase 1, Task 1.1 (model.unwrap_or fix)
4. Commit + push after EACH task
5. After Phase 1 → launch 3 audit agents
6. Phase 2 (paranoid tests)
7. After Phase 2 → launch audit agents
8. Continue through phases
```
