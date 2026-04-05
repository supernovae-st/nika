# Inference Routing — Master Execution Prompt

> Copy-paste the **Prompt** section into a new Claude Code session at `~/dev/supernovae/nika/tools/nika`.

---

## Context for the human

This prompt drives the implementation of Nika's inference routing system — from custom endpoints (Level 1) through fleet management (Level 6). It's designed for a fresh Claude Code session with zero prior context.

**Plans to read (in order):**
1. `docs/plans/2026-03-27-custom-endpoints.md` — Level 1 detailed (15 tasks, 7 phases)
2. `docs/plans/2026-03-27-inference-routing-roadmap.md` — Levels 1-6 master roadmap (1898 lines)
3. `docs/gpu-cloud-landscape-2025-2026.md` — GPU reference
4. `tools/nika-engine/src/display/bench.rs` — Level 2 display code (already written, 1210 lines)

**What exists already:**
- `bench.rs` display module is written (structs + formatters + 17 tests)
- Routing module architecture is designed (traits, structs, module tree)
- Level 1 plan has exact code snippets with line numbers

---

## Prompt

````
You are implementing the Inference Routing system for Nika — a Rust workflow engine that orchestrates LLM calls across multiple providers.

## What you're building

An intelligent inference orchestrator that routes every LLM call to the optimal backend based on speed, cost, quality, and task requirements. Nika sees the entire workflow DAG — it knows which tasks are critical, which need structured output, which need vision. No other tool has this context.

6 levels, each independently shippable:
- Level 1: Custom Endpoints — connect to vLLM, TGI, Ollama via `base_url`
- Level 2: nika bench — compare providers with speed/cost/quality metrics
- Level 3: Fallback Chains — try cheap first, fall back to expensive
- Level 4: Smart Routing — auto-select provider per task
- Level 5: Auto-Optimization — `nika optimize` generates routing config
- Level 6: Fleet Management — multi-GPU load balancing

## Required reading (DO THIS FIRST)

Before writing ANY code, read these files thoroughly:

```
# Level 1 detailed plan (15 tasks, exact code, line numbers)
docs/plans/2026-03-27-custom-endpoints.md

# Master roadmap (all 6 levels, Rust architecture, CLI designs, daemon combos)
docs/plans/2026-03-27-inference-routing-roadmap.md

# Codebase reference
tools/nika/CLAUDE.md
tools/nika-engine/src/display/bench.rs   # Already written — 1210 lines, 17 tests

# Understand existing patterns
tools/nika-engine/src/provider/rig.rs           # RigProvider enum
tools/nika-engine/src/runtime/executor/mod.rs   # TaskExecutor, get_rig_provider()
tools/nika-engine/src/runtime/executor/infer.rs # run_infer() — where provider is resolved
tools/nika-engine/src/config.rs                 # NikaConfig
tools/nika-engine/src/error_domains.rs          # ProviderError pattern
tools/nika-core/src/ast/raw/parser.rs           # How YAML fields are parsed
```

Read ALL of them before starting. The plans contain exact code, line numbers, and architectural decisions validated against the codebase.

## Execution methodology

### Skill: superpowers:executing-plans

Use the `superpowers:executing-plans` skill for task-by-task execution. This means:
1. Read the plan
2. Review it critically (check assumptions against current code)
3. Execute tasks in batches (3-5 tasks per batch)
4. After each batch: pause, review, report what was done
5. Never skip ahead — each batch builds on the previous

### Skill: superpowers:test-driven-development

Every implementation task follows RED-GREEN-REFACTOR:
1. **RED**: Write the failing test FIRST
2. **GREEN**: Write minimal code to pass
3. **REFACTOR**: Clean up while tests still pass

This is not optional. Do not write implementation before tests.

### Skill: superpowers:verification-before-completion

Before claiming ANY task is done:
1. Run `cargo test --workspace --lib` (NEVER without `--lib` — triggers macOS Keychain popups)
2. Run `cargo clippy --workspace -- -D warnings` (zero warnings policy)
3. Verify the specific test you wrote actually passes
4. Only THEN mark the task complete

### Skill: superpowers:systematic-debugging

When something fails:
1. Read the error message completely
2. Identify the root cause (don't guess)
3. Fix the actual problem (don't work around it)
4. Verify the fix doesn't break anything else

## Quality gates (enforced at every commit)

```bash
# ALL must pass before every commit
cargo test --workspace --lib          # 8400+ tests, safe
cargo clippy --workspace -- -D warnings  # Zero warnings
```

If a test fails, FIX IT before moving on. Never skip failing tests.

## Commit format

```
type(scope): concise description

Co-Authored-By: Nika 🦋 <nika@supernovae.studio>
```

Types: `feat`, `fix`, `refactor`, `test`, `docs`
Scopes: `provider`, `routing`, `bench`, `ast`, `config`, `cli`, `daemon`, `tui`, `display`

One logical change = one commit. Don't batch unrelated fixes.

## Codebase conventions (MUST follow)

- **Errors**: `NikaError` with NIKA-XXX codes, NEVER `anyhow`. Domain sub-enums in `error_domains.rs` with `From` impls.
- **AST**: Always Raw → Analyzed → Lower. Never skip phases.
- **New fields**: Always `#[serde(default)]` for backward compatibility.
- **Providers**: `RigProvider::from_name()` for catalog. `from_name_with_endpoints()` for custom. `openai::Client::from_url(key, url)` for custom base URL.
- **Tests**: `cargo test --lib` ALWAYS. `insta` for snapshots. Mock provider for no-API-call tests.
- **Logging**: `tracing` macros (`debug!`, `info!`, `warn!`).
- **Concurrency**: `Arc<DashMap>` for caches, `Arc<AtomicU64>` for counters, `parking_lot::RwLock` for mutable shared state.
- **Display**: Use existing helpers from `display/colors.rs`, `display/icons.rs`, `display/cli_format.rs`. Never hardcode colors or icons.
- **Config**: TOML at `~/.config/nika/config.toml`. `NikaConfig::load()?.with_env()`.

## Level execution order

### Level 1: Custom Endpoints

Read: `docs/plans/2026-03-27-custom-endpoints.md`

15 tasks across 7 phases. Execute in order. Key deliverables:
- `provider/endpoints.rs` (NEW) — CustomEndpointConfig, URL validation
- `config.rs` — add `endpoints` field to NikaConfig
- `error_domains.rs` — NIKA-035, NIKA-036
- AST: `base_url` field on RawWorkflow, RawTask, AnalyzedWorkflow, AnalyzedTask, InferParams, AgentParams
- `rig.rs` — OpenAiCompat variant, from_name_with_endpoints()
- `executor/mod.rs` — wire CustomEndpointMap into TaskExecutor
- `executor/infer.rs` — inline base_url creates transient provider
- `executor/agent.rs` — thread base_url into RigAgentLoop
- `runner.rs` — load config, resolve endpoints
- `nika-cli/src/provider.rs` — show endpoints in `nika provider list`

After Level 1: verify with `cargo test --workspace --lib` (all 8400+ pass).

### Level 2: nika bench

Read: roadmap Level 2 section + `display/bench.rs` (already written).

The display module exists. You need to build:
1. CLI command (`Bench` variant in main.rs Commands enum)
2. Argument parsing (`nika-cli/src/bench.rs`)
3. Bench loop: parse → override provider → Runner::quiet().run() → replay events into RunStats
4. Aggregation (mean, percentiles across iterations)
5. Wire display functions from `display/bench.rs`
6. `--eval` mode with LLM-as-judge
7. Bench cache persistence (`.nika/bench-cache/`)

IMPORTANT — RunStats quirks (from review):
- No `RunStats::from_events()` — replay events via `apply_event()` loop
- `ProviderCallStat` has no `provider_name` — add it, or join ProviderCalled + ProviderResponded by task_id
- Add `hourly_rate: Option<f64>` to `CustomEndpointConfig` for local cost estimation

### Level 3: Fallback Chains

Read: roadmap Level 3 section.

Key architecture decision: routing config lives at workflow/task level in AST (NOT inside InferParams). Resolution to endpoints happens in the executor at runtime.

Interaction with existing retry: task retry runs first (intra-provider), then fallback (inter-provider).

### Level 4: Smart Routing

Read: roadmap "Routing Module Architecture" section.

Build `nika-engine/src/routing/` module:
```
routing/
├── mod.rs           # RoutingStrategy trait + ProviderSlot + BenchEntry
├── error.rs         # RoutingError (NIKA-320..325)
├── budget.rs        # BudgetTracker (AtomicU64 micro-dollars)
├── capability.rs    # CapabilityFilter (bitflags)
├── bench_cache.rs   # BenchCache (DashMap + JSON persistence + EMA)
├── critical_path.rs # CriticalPathAnalyzer (forward+backward longest-path)
├── selector.rs      # ProviderSelector (filter → budget → strategy)
└── strategy/
    ├── mod.rs
    ├── direct.rs    # No routing
    ├── fallback.rs  # Ordered priority
    ├── smart.rs     # Cost/latency optimization
    └── fleet.rs     # Multi-provider race
```

The roadmap has FULL Rust code for every struct and trait. Use it as reference.

### Level 5: Auto-Optimization

Combines Level 2 (bench) + Level 4 (routing). The optimizer:
1. Runs bench internally
2. Evaluates quality
3. Solves assignment (greedy: per-task, budget-constrained, critical-path-boosted)
4. Generates `routing.rules` config
5. Interactive apply via cliclack

### Level 6: Fleet Management

Config-driven multi-endpoint pool with health checks, load balancing, and live dashboard. Uses `nika fleet` CLI (not `nika status` — avoids daemon conflict).

## What NOT to do

- Do NOT use `anyhow` for errors — always `NikaError` with codes
- Do NOT run `cargo test` without `--lib` (keychain popups)
- Do NOT skip AST phases (Raw → Analyzed → Lower)
- Do NOT hardcode colors/icons — use `display/colors.rs` and `display/icons.rs`
- Do NOT add backward-compat shims — zero users means zero compat debt
- Do NOT write documentation files unless explicitly asked
- Do NOT batch unrelated changes in one commit
- Do NOT guess file paths — use Glob/Grep to verify before editing
- Do NOT implement features beyond what each level requires

## How to start

1. Read all plans listed in "Required reading"
2. Check git status — are there uncommitted changes from Level 1?
3. If Level 1 is incomplete, continue from where it left off
4. If Level 1 is done, start Level 2
5. Execute task-by-task with the executing-plans skill
6. Batch 3-5 tasks, review, report, continue
7. Commit after each logical unit (1-3 tasks)

Start now. Read the plans first.
````
