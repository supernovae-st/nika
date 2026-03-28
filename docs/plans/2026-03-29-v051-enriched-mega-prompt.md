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

---

## Appendix A: Deep Agent Findings (Wave 2 — 6 agents)

### A1. Paranoid Bug Hunt (confirmed silent bugs)

| # | Severity | File:Line | Bug |
|---|----------|-----------|-----|
| P1 | **Important** | runner.rs:2253-2262 | Empty `for_each` emits zero events — TUI spinners stuck |
| P2 | Suggestion | cost.rs:600 | HuggingFace model names with `/` fall to DEFAULT_PRICING silently |
| P3 | Suggestion | executor/mod.rs:466 | Non-retryable errors (MissingApiKey) still try all fallback providers |
| P4 | Suggestion | template.rs:3454 | Stale `"BUG:"` comment in a passing test (bug was fixed) |

**Fix P1:** In the empty for_each branch (runner.rs:2253), emit `ForEachStarted` + `ForEachCompleted` events with `total_items: 0` before inserting the empty result.

### A2. Test Coverage Gaps (17 specific tests needed)

**CRITICAL gaps (no tests at all):**
1. `execute_with_routing()` — zero tests for the entire fallback chain
2. `FallbackTriggered` missing from event serde roundtrip test (38 of 60 variants covered)
3. `classify_fallback_reason()` — all branches untested

**Tests to write (priority order):**

```
# Routing (executor/tests.rs)
test_execute_with_routing_fallback_success
test_execute_with_routing_chain_exhausted
test_execute_with_routing_non_llm_verb_skips
test_classify_fallback_reason_all_branches

# Events (nika-event/log.rs)
Update all_38_variants() → all_60_variants() (+22 missing)

# Security (executor/tests_wiremock.rs)
test_fetch_rejects_crlf_in_header_key
test_fetch_rejects_crlf_in_header_value

# Structured output (structured_output tests)
test_layer4_uses_repair_callback_over_infer
test_layer4_falls_back_to_infer_when_no_repair

# Bench
test_bench_zero_duration_no_panic
test_bench_rejects_zero_iterations

# Agent
test_agent_max_turns_1_stops_gracefully
```

### A3. Rust Architecture Improvements

**rig.rs (3,598 lines → target ~2,800):**
- Extract `infer_generic<C: ProviderClient>()` — eliminates 8-arm match duplication (-800 LOC)
- Split into: `provider/infer.rs`, `provider/stream.rs`, `provider/vision.rs`, `provider/mcp_tool.rs`
- Move `StreamChunk` (23 variants) to `nika-event` — split into `StreamDelta` + `ActivityEvent`

**TaskExecutor (15 fields → 5 composable structs):**
```rust
struct ExecutorInfra { http_client, cas, builtin_router, policy_enforcer, cancel_token }
struct WorkflowConfig { default_provider, default_model, skills_map, endpoints, agents }
```

**Error system:**
- Finish domain error migration (error_domains.rs exists but unused)
- Delete unused `FixSuggestion` trait (line 78) — miette `Diagnostic` already serves this purpose
- Fix NIKA-096 placement (in wrong section)

**Cost system:**
- Move pricing tables to TOML data file (contributor-friendly updates)
- Unify `ProviderKind` with `core::find_provider()` catalog

### A4. Performance Optimizations (8 findings, priority-ranked)

| # | Finding | Severity | Save | Fix |
|---|---------|----------|------|-----|
| F1 | `bindings.to_value()` deep clone | HIGH | ~1MB/loop | Use `bindings.iter()` |
| F2 | `action.clone()` in routing fallback | HIGH | ~2MB (vision) | Pass provider override separately |
| F3 | `strip_think_tags()` 2x string copy | MEDIUM | 2×response_len | `Cow<str>` + ASCII check |
| F4 | `try_parse_json_str()` speculative parse | MEDIUM | failed parse/ref | Conditional guard |
| F5 | `EventLog::emit()` write lock contention | MEDIUM | lock contention | `parking_lot::Mutex` |
| F6 | `lower_action()` per for_each iteration | MEDIUM | action_size×iters | Hoist + Arc |
| F7 | `classify_fallback_reason()` .to_string() | LOW | ~10 bytes/error | Return `&'static str` |
| F8 | `redact_for_event()` always allocates | LOW | ~200 bytes/task | `Cow<str>` |

### A5. Telemetry Stack (recommended crates)

```toml
# In-process percentiles (no network, CLI-friendly)
hdrhistogram = "7.5"

# Structured JSON logging (feature already in tracing-subscriber)
tracing-subscriber = { features = ["json"] }

# Optional: OpenTelemetry export (feature-gated)
[features]
telemetry-otlp = ["opentelemetry/0.31", "tracing-opentelemetry/0.32"]
```

**Priority 1:** `hdrhistogram` for end-of-run p50/p90/p99 in bench + run summary
**Priority 2:** `tracing-subscriber` JSON mode for structured log files
**Priority 3:** `WorkflowCostSummary` aggregation (pure logic, no deps)
**Priority 4:** OpenTelemetry spans (feature-gated behind `telemetry-otlp`)

### A6. Mock OpenAI Server for Integration Tests

**Recommended:** `llmposter` v0.4.0 (Rust-native, in-process, fixture-driven):
```toml
[dev-dependencies]
llmposter = "0.4"
```
- Speaks real OpenAI/Anthropic wire protocol
- Supports streaming SSE, tool calls, failure simulation (429, truncation)
- In-process axum server, drops when test ends
- Use for: custom endpoint wiring, streaming SSE, retry on 429

**Alternative Layer:** Serde snapshot fixtures from real providers in `tests/fixtures/`.

---

## Appendix B: Consolidated Priority Matrix

| Priority | Task | Phase | Impact | Effort |
|----------|------|-------|--------|--------|
| P0 | execute_with_routing() tests | 2 | Critical — untested feature | 30 min |
| P0 | Update event serde roundtrip (38→60) | 2 | Critical — serde regression | 20 min |
| P1 | model.unwrap_or("default") fix | 1 | Wrong cost for all no-model tasks | 10 min |
| P1 | Empty for_each zero events fix | 1 | TUI stuck spinners | 15 min |
| P1 | bindings.to_value() deep clone | 4 | 1MB/loop for for_each | 15 min |
| P2 | hourly_rate dead code fix | 1 | Wrong cost for self-hosted GPUs | 30 min |
| P2 | Template validation sync | 1 | Silent validation gap | 20 min |
| P2 | CRLF injection tests | 2 | Security regression risk | 15 min |
| P2 | strip_think_tags Cow | 4 | 2×alloc per LLM response | 15 min |
| P3 | rig.rs generic helper | 3 | -800 LOC, maintainability | 2h |
| P3 | rig.rs module split | 3 | Navigability | 1h |
| P3 | Agent cost ProviderKind fix | 1 | Wrong cost in agent loop | 30 min |
| P4 | hdrhistogram for percentiles | 5 | Better bench/run stats | 1h |
| P4 | action.clone() routing fix | 4 | 2MB save for vision | 1h |
| P5 | Level 4 Smart Routing | 6 | Major feature | 3h |
