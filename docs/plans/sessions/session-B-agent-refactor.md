# Session B: Agent Loop Refactor (~4-5h)

## Context
Nika workflow engine. Workspace: `tools/` (12 Rust crates).
Master plan: `docs/plans/2026-03-28-v051-master-quality-plan.md` — READ PART 2 FIRST.

## Mission: Kill 1200 LOC duplication. Fix token_budget. Fix extended_thinking.

This is the BIGGEST architectural fix. `providers.rs` has 3 near-identical 420-line methods.
After this session: 1505 LOC → ~600 LOC, token_budget enforced, thinking integrated.

---

### Phase 1: Extract `run_agent_loop<C>` (~2h)

**File**: `nika-engine/src/runtime/rig_agent_loop/providers.rs` (1505 LOC)

1. Read ALL three methods: `run_claude` (110-527), `run_openai` (528-941), `run_generic_provider_impl` (1080-1505)
2. Identify the 5 differences (client construction, ProviderKind, extended_thinking, model prefix stripping, log messages)
3. Create the generic method:

```rust
async fn run_agent_loop<C: CompletionClient>(
    &mut self,
    client: C,
    model_name: &str,
    provider_kind: Option<ProviderKind>,
) -> Result<RigAgentLoopResult, NikaError>
where
    C::CompletionModel: Clone + 'static,
    <C::CompletionModel as rig::completion::CompletionModel>::Response: Send,
```

4. Move the shared logic (retry loop, guardrails, limits, events) into this method
5. Rewrite `run_claude`, `run_openai` as 5-line wrappers calling `run_agent_loop`
6. Delete `run_generic_provider_impl` (replaced by `run_agent_loop`)
7. `run_mistral/groq/deepseek/gemini/xai` stay as thin wrappers

**Key**: Don't change behavior. Pure refactor. All 55 existing agent tests must pass.

### Phase 2: Wire token_budget (~30min)

**File**: Same + `nika-engine/src/runtime/rig_agent_loop/mod.rs`

In `run_agent_loop`, before the main loop:
```rust
// Wire token_budget from AgentParams into LimitTracker
if let Some(budget) = self.params.token_budget {
    self.limit_tracker.set_token_limit(budget as u64);
}
```

**Test**: Create agent with `token_budget: 100`, run with mock that returns 200 tokens.
Verify result has `RigAgentStatus::TokenBudgetExceeded`.

### Phase 3: Integrate extended_thinking (~1h)

**File**: `nika-engine/src/runtime/rig_agent_loop/thinking.rs`

Currently `run_claude_with_thinking` (314-512) is a separate 200-line method: single-turn, no tools, no retry.

Two options:
A) **Integrate into main loop** (preferred): Add thinking mode as a flag in `run_agent_loop`. Use `stream_with_thinking` instead of `stream_with_tools` when enabled. Keep retry + guardrails.
B) **Validate at analyzer** (minimum viable): If `extended_thinking: true` AND tools are configured, emit NIKA error at analysis time.

Choose A if time allows, B otherwise.

### Phase 4: Verify (~30min)

1. `cargo test -p nika-engine --lib -- rig_agent_loop` — ALL 55 tests pass
2. `cargo test --workspace --lib` — full suite green
3. `cargo clippy --workspace -- -D warnings` — 0 warnings
4. Line count: `wc -l providers.rs` should be ~500-600 (was 1505)

---

## Files Modified
- `nika-engine/src/runtime/rig_agent_loop/providers.rs` — main refactor
- `nika-engine/src/runtime/rig_agent_loop/thinking.rs` — integrate or validate
- `nika-engine/src/runtime/rig_agent_loop/mod.rs` — LimitTracker wiring
- `nika-engine/src/runtime/rig_agent_loop/tests.rs` — add token_budget + thinking tests

## Commit Strategy
- Commit 1: Extract `run_agent_loop<C>` (pure refactor, no behavior change)
- Commit 2: Wire `token_budget` into LimitTracker
- Commit 3: Extended thinking integration or validation
- Commit 4: New tests for token_budget + thinking

## Risk Mitigation
This is a large refactor. If stuck:
1. Use git worktree for isolation
2. Run tests after EVERY method extraction (not just at the end)
3. Keep the old methods commented out until all tests pass
4. If run_claude has subtle differences from run_openai, document them as TODOs
