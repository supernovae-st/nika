# Session K: Inference Routing -- Levels 1-3 (~5-6h)

## Context
Nika workflow engine. Workspace: `tools/` (12 Rust crates). Main branch, 8600+ tests.
Source plan: `docs/plans/2026-03-27-inference-routing-roadmap.md` -- READ IT FIRST.
Phase 1.1 plan: `docs/plans/2026-03-28-phase1-model.md` for fallback chain design.
Dev reference: `tools/nika/CLAUDE.md` for conventions.

Depends on: Session J (Phase 0) for `preset:` wiring to be in place.

Key files:
- `tools/nika-engine/src/provider/rig.rs` (3675 LOC) -- RigProvider, all inference
- `tools/nika-engine/src/provider/endpoints.rs` (446 LOC) -- custom endpoint config
- `tools/nika-engine/src/provider/cost.rs` (1280 LOC) -- cost calculation
- `tools/nika-engine/src/runtime/executor/infer.rs` (1283 LOC) -- infer execution
- `tools/nika-engine/src/runtime/executor/agent.rs` (428 LOC) -- agent execution
- `tools/nika-event/src/log.rs` (3961 LOC) -- event definitions

## Mission: Add provider fallback chains and `nika bench` command

Transform Nika from a single-provider workflow engine into an intelligent inference router.
Level 1 (custom endpoints) is already done. This session adds Level 2 (`nika bench`) and
Level 3 (fallback chains with `provider: [groq, anthropic]` syntax).

### Methodology
For EVERY change: read code -> write failing test -> fix -> verify -> commit.
`cargo test --workspace --lib` (always --lib). 1 fix = 1 commit.

---

## PART 1: Fallback Chains (Level 3, P-MODEL Part 2)

### Task 1: AST changes for provider_chain

**File**: `tools/nika-core/src/ast/raw/task.rs` (209 LOC)
Add `pub provider_chain: Option<Spanned<Vec<Spanned<String>>>>` to `RawTask`.

**File**: `tools/nika-core/src/ast/raw/parser.rs` (3372 LOC)
In `parse_task()`, detect if `provider:` node is a Sequence and parse as `provider_chain`
instead of single string.

**File**: `tools/nika-core/src/ast/analyzed/task.rs` (599 LOC)
Add `pub provider_chain: Option<Vec<String>>` to `AnalyzedTask`.

**File**: `tools/nika-engine/src/ast/workflow.rs` (1021 LOC)
Add `pub provider_chain: Option<Vec<String>>` to Task struct.

**File**: `tools/nika-engine/src/ast/lower.rs` (2716 LOC)
Propagate `provider_chain` through lowering.

**Tests**:
- `provider: [groq, anthropic]` parsed as chain of 2
- `provider: anthropic` parsed as single string (regression)
- `provider: [single]` with 1 entry works

**Estimated LOC**: ~35
**Commit**: `feat(ast): parse provider as string or array for fallback chains`

### Task 2: InferParams + AgentParams changes

**File**: `tools/nika-engine/src/ast/action.rs` (1818 LOC)
Add `pub provider_chain: Option<Vec<String>>` to `InferParams`.

**File**: `tools/nika-engine/src/ast/agent.rs` (1401 LOC)
Add `pub provider_chain: Option<Vec<String>>` to `AgentParams`.

**Estimated LOC**: ~15
**Commit**: `feat(ast): add provider_chain field to InferParams and AgentParams`

### Task 3: Executor fallback loop in run_infer

**File**: `tools/nika-engine/src/runtime/executor/infer.rs` (1283 LOC)

Currently: `let provider_name = resolved_provider.as_deref().unwrap_or(&self.default_provider);`

Change to: if `infer.provider_chain` is `Some`, iterate through the chain. Try each provider;
on `ProviderError` (MissingApiKey, RateLimit, Timeout), try the next. If all fail, return last error.

```rust
async fn try_providers_with_fallback(
    &self,
    chain: &[String],
    task: &Task,
    prompt: &str,
    // ... other params
) -> Result<String, NikaError> {
    let mut last_error = None;
    for (i, provider_name) in chain.iter().enumerate() {
        match self.try_single_provider(provider_name, task, prompt).await {
            Ok(result) => return Ok(result),
            Err(e) if is_retriable_for_fallback(&e) && i < chain.len() - 1 => {
                self.event_log.emit(EventKind::ProviderFallback {
                    task_id: task.id.clone(),
                    from: provider_name.clone(),
                    to: chain[i + 1].clone(),
                    reason: format!("{}", e),
                });
                last_error = Some(e);
            }
            Err(e) => return Err(e),
        }
    }
    Err(last_error.unwrap_or_else(|| NikaError::ProviderError {
        reason: "Fallback chain exhausted".to_string(),
    }))
}
```

**Tests**:
- First provider succeeds -> no fallback
- First fails, second succeeds -> ProviderFallback event emitted
- All fail -> last error returned
- Single-provider chain -> same as plain provider

**Estimated LOC**: ~60
**Commit**: `feat(runtime): implement provider fallback chain in infer executor`

### Task 4: Executor fallback loop in run_agent

**File**: `tools/nika-engine/src/runtime/executor/agent.rs` (428 LOC)
Same pattern as infer. If `resolved_agent.provider_chain` is Some, try each provider.

**Estimated LOC**: ~40
**Commit**: `feat(runtime): implement provider fallback chain in agent executor`

### Task 5: New EventKind -- ProviderFallback

**File**: `tools/nika-event/src/log.rs` (3961 LOC)

```rust
ProviderFallback {
    task_id: Arc<str>,
    from: String,
    to: String,
    reason: String,
},
```

Update `task_id()` match arm. Update display formatters.

**File**: `tools/nika-engine/src/display/format_event.rs` (739 LOC)
Add formatting for ProviderFallback event.

**Tests**:
- Serialization round-trip
- `task_id()` returns correct value
- Display format

**Estimated LOC**: ~25
**Commit**: `feat(event): add ProviderFallback event for fallback chain tracking`

### Task 6: New error code NIKA-037
**File**: `tools/nika-engine/src/error_domains.rs`
Add `FallbackChainExhausted` variant.
**Estimated LOC**: ~10
**Commit**: `feat(error): add NIKA-037 FallbackChainExhausted error code`

---

## PART 2: nika bench (Level 2)

### Task 7: CLI command + argument parsing

**File**: `tools/nika/src/main.rs` (add `Bench` variant to Commands enum)
**File**: `tools/nika-cli/src/bench.rs` (NEW, ~300 LOC)

Arguments: `--iterations N`, `--providers a,b,c`, `--profile`, `--json`

**Commit**: `feat(cli): add nika bench command with argument parsing`

### Task 8: Bench runner loop

**File**: `tools/nika-cli/src/bench.rs`

```rust
async fn run_bench(yaml: &str, providers: &[String], iterations: usize) {
    let parsed = parse_workflow(yaml)?;
    for provider_name in providers {
        let mut iteration_stats = Vec::new();
        for _ in 0..iterations {
            let mut workflow = clone_and_override_provider(parsed.clone(), provider_name);
            let event_log = EventLog::new();
            let mut runner = Runner::with_event_log(workflow, event_log.clone())?.quiet();
            runner.run().await?;
            let mut stats = RunStats::default();
            event_log.with_events(|events| {
                for event in events { stats.apply_event(event); }
            });
            iteration_stats.push(stats);
        }
        aggregate_and_store(provider_name, iteration_stats);
    }
    display_comparison_table(all_results);
}
```

**Known constraint**: `RunStats::apply_event()` is incremental -- replay events manually.
**Known constraint**: `ProviderCallStat` has no `provider_name` field -- add it (Task 8b).

**Estimated LOC**: ~150
**Commit**: `feat(bench): implement bench runner loop with stats aggregation`

### Task 8b: Add provider field to ProviderCallStat

**File**: `tools/nika-engine/src/display/renderer.rs` (find `ProviderCallStat`)
Add `pub provider: String` field. Populate from `ProviderCalled.provider` event.
**Estimated LOC**: ~15
**Commit**: `feat(display): add provider field to ProviderCallStat for bench attribution`

### Task 9: Comparison table display

**File**: `tools/nika-cli/src/bench.rs`

Speed section (TTFT percentiles, tok/s, total), cost section ($/run, token breakdown),
profile section (per-task bars), summary with verdict.

Use existing display helpers from `nika-engine/src/display/`.

**Estimated LOC**: ~120
**Commit**: `feat(bench): display comparison table with speed, cost, and profile sections`

### Task 10: Bench cache persistence

**File**: `tools/nika-cli/src/bench.rs`
Persist bench results to `.nika/bench-cache/<workflow_hash>.json`.
Add `--json` flag for raw export.

**Estimated LOC**: ~40
**Commit**: `feat(bench): persist bench results and add --json export`

---

## E2E Verification Workflows

### test-fallback-chain.nika.yaml
```yaml
schema: "nika/workflow@0.12"
workflow: test-fallback-chain
description: "E2E: provider fallback chain tries providers in order"

tasks:
  - id: generate
    provider: [nonexistent_provider, mock]
    infer: "Hello from fallback chain"
    # Expected: nonexistent_provider fails, falls back to mock, succeeds
```
**Run**: `nika run test-fallback-chain.nika.yaml`

### test-fallback-exhausted.nika.yaml
```yaml
schema: "nika/workflow@0.12"
workflow: test-fallback-exhausted

tasks:
  - id: fail_all
    provider: [nonexistent_1, nonexistent_2]
    infer: "This should fail with NIKA-037"
    # Expected: NIKA-037 FallbackChainExhausted
```
**Run**: `nika run test-fallback-exhausted.nika.yaml` -> error

### test-bench.nika.yaml
```yaml
schema: "nika/workflow@0.12"
workflow: bench-test
provider: mock

tasks:
  - id: step1
    infer: "Hello"
  - id: step2
    depends_on: [step1]
    with: { data: $step1 }
    infer: "Process: {{with.data}}"
```
**Run**: `nika bench test-bench.nika.yaml --providers mock --iterations 3`

---

## After All Fixes

```bash
cd tools && cargo test --workspace --lib       # All pass
cd tools && cargo clippy --workspace -- -D warnings  # Zero warnings
nika bench examples/agents-preset.nika.yaml --providers mock --iterations 2  # Works
nika run test-fallback-chain.nika.yaml         # Fallback triggers and succeeds
```

---

## Commit Strategy (11 commits)

```
# Part 1: Fallback chains
feat(ast): parse provider as string or array for fallback chains
feat(ast): add provider_chain field to InferParams and AgentParams
feat(runtime): implement provider fallback chain in infer executor
feat(runtime): implement provider fallback chain in agent executor
feat(event): add ProviderFallback event for fallback chain tracking
feat(error): add NIKA-037 FallbackChainExhausted error code

# Part 2: nika bench
feat(cli): add nika bench command with argument parsing
feat(bench): implement bench runner loop with stats aggregation
feat(display): add provider field to ProviderCallStat for bench attribution
feat(bench): display comparison table with speed, cost, and profile sections
feat(bench): persist bench results and add --json export
```
