# Session B: Agent Loop Refactor (~4-5h)

## Context
Nika workflow engine. Workspace: `tools/` (12 Rust crates).
Master plan: `docs/plans/2026-03-28-v051-master-quality-plan.md` — READ PART 2 FIRST.

## Mission: Kill 1200 LOC duplication. Fix token_budget. Fix extended_thinking.

This is the BIGGEST architectural fix. `providers.rs` has 3 near-identical 420-line methods.
After this session: 1505 LOC -> ~600 LOC, token_budget enforced, thinking integrated.

---

## PART 0: LINE-BY-LINE DIFF ANALYSIS

### Three methods compared

| | `run_claude` (L110-515) | `run_openai` (L528-927) | `run_generic_provider_impl` (L1080-1505) |
|---|---|---|---|
| LOC | 405 | 399 | 425 |
| Called by | `run_auto` directly | `run_auto` directly | `run_mistral`, `run_groq`, `run_deepseek`, `run_gemini`, `run_xai` |

### Every difference, line by line

#### D1: Client construction (ESSENTIAL)

```rust
// run_claude L117
let client = anthropic::Client::from_env();
// run_openai L530
let client = openai::Client::from_env();
// run_generic — client passed as argument (already generic!)
async fn run_generic_provider_impl<C>(&mut self, client: C, ...) where C: CompletionClient
```

**Verdict: ESSENTIAL.** Each provider has its own Client type. But `run_generic_provider_impl` already solves this — the caller creates the client. `run_claude` and `run_openai` just need to become callers of the generic method.

#### D2: Extended thinking short-circuit (ESSENTIAL — Claude-only)

```rust
// run_claude L112-114 — ONLY in run_claude
if self.params.extended_thinking == Some(true) {
    return self.run_claude_with_thinking().await;
}
// run_openai — ABSENT
// run_generic — ABSENT
```

**Verdict: ESSENTIAL.** Extended thinking is a Claude-specific API feature (`additional_params.thinking`). The short-circuit returns early to a completely separate single-turn method. However, this should be moved to the caller wrapper, not embedded in the loop.

#### D3: ProviderKind for cost calculation (ESSENTIAL)

```rust
// run_claude L168-174
crate::provider::cost::ProviderKind::Claude,
// run_openai L581-587
crate::provider::cost::ProviderKind::OpenAI,
// run_generic L1140-1151 — uses Option<ProviderKind> parameter
provider_kind.map(|pk| calculate_cost_with_cache(pk, ...)).unwrap_or(0.0)
```

**Verdict: ESSENTIAL but already parameterized.** `run_generic` takes `Option<ProviderKind>` — this is the correct design. `run_claude` and `run_openai` hardcode it. The generic method wraps cost calculation in `Option::map` (slightly different from the direct call in `run_claude`/`run_openai`), which is a minor behavioral difference:
- `run_claude`/`run_openai`: always compute cost (no `Option`)
- `run_generic`: wraps in `map(|pk| ...).unwrap_or(0.0)` — functionally identical since `run_claude`/`run_openai` pass `Some(ProviderKind)`.

#### D4: Model name extraction — `.clone()` vs reference (ACCIDENTAL)

```rust
// run_claude L120-127 — clones raw_model, converts to String
let raw_model = self.params.model.clone().ok_or_else(|| ...)?;
let model_name = Self::strip_model_prefix(&raw_model).to_string();
// run_openai L533-540 — identical pattern
let raw_model = self.params.model.clone().ok_or_else(|| ...)?;
let model_name = Self::strip_model_prefix(&raw_model).to_string();
// run_generic L1091 — takes &str, strips in-place (no allocation)
let model_name = Self::strip_model_prefix(model_name);
```

**Verdict: ACCIDENTAL.** `run_claude` and `run_openai` do model extraction inline, `run_generic` receives it already extracted. The generic approach is better — extract in the thin wrapper, pass `&str` to the loop.

#### D5: Provider name in `params.provider` backfill (run_generic ONLY)

```rust
// run_generic L1096-1100 — backfills params.provider for stop_sequences key mapping
if self.params.provider.is_none() {
    if let Some(ref pk) = provider_kind {
        self.params.provider = Some(pk.to_provider_id().to_string());
    }
}
// run_claude — ABSENT (params.provider is already "anthropic" from run_auto dispatch)
// run_openai — ABSENT (params.provider is already "openai" from run_auto dispatch)
```

**Verdict: ACCIDENTAL / DEFENSIVE.** This is only needed because auto-detect mode leaves `params.provider` as `None`. Safe to include in the generic method unconditionally — it's a no-op when `params.provider` is already set.

#### D6: Log message strings — provider names in tracing (ACCIDENTAL)

```rust
// run_claude L193: "Claude agent limit exceeded after first turn"
// run_openai L605: "OpenAI agent limit exceeded after first turn"
// run_generic L1168: "Agent limit exceeded after first turn"
```

Same pattern repeats for:
- `"Claude agent limit exceeded during retry loop"` vs `"OpenAI..."` vs `"Agent..."`
- `"Claude agent limit exceeded during guardrail retry loop"` vs `"OpenAI..."` vs `"Agent..."`
- `"Retrying Claude due to guardrail failure"` vs `"Retrying OpenAI..."` vs `"Retrying due to..."`
- `"Claude guardrail retries exhausted..."` vs `"OpenAI guardrail retries exhausted..."` vs `"Guardrail retries exhausted..."`

**Verdict: ACCIDENTAL.** Pure copy-paste drift. The provider name should be interpolated from `provider_kind`. The generic method already omits the provider name — just use that pattern.

#### D7: cost calculation call sites — direct vs Option::map (ACCIDENTAL)

```rust
// run_claude — 5 direct calls:
crate::provider::cost::calculate_cost_with_cache(
    crate::provider::cost::ProviderKind::Claude, &model_name, ...);

// run_openai — 5 direct calls:
crate::provider::cost::calculate_cost_with_cache(
    crate::provider::cost::ProviderKind::OpenAI, &model_name, ...);

// run_generic — 5 calls via Option::map:
provider_kind.map(|pk| calculate_cost_with_cache(pk, model_name, ...)).unwrap_or(0.0)
```

**Verdict: ACCIDENTAL.** `run_generic` already handles this correctly with `Option`. The 10 direct calls in `run_claude`/`run_openai` are copy-paste.

### Summary of differences

| # | Difference | Type | Lines affected | Resolution |
|---|---|---|---|---|
| D1 | Client construction | ESSENTIAL | 1 line per method | Move to thin wrapper |
| D2 | Extended thinking short-circuit | ESSENTIAL | 3 lines, Claude only | Move to `run_claude` wrapper |
| D3 | ProviderKind enum value | ESSENTIAL | Parameter already in generic | Use `Option<ProviderKind>` (already done) |
| D4 | Model name extraction style | ACCIDENTAL | 3 lines per method | Extract in wrapper, pass `&str` |
| D5 | `params.provider` backfill | ACCIDENTAL | 4 lines, generic only | Keep in generic (no-op when set) |
| D6 | Provider name in log messages | ACCIDENTAL | ~10 instances | Interpolate from `provider_kind` |
| D7 | Direct vs Option::map cost calc | ACCIDENTAL | 5 per method (15 total) | Use `Option::map` pattern |

**ESSENTIAL differences: 3 (client construction, extended thinking, provider kind)**
**ACCIDENTAL differences: 4 (model extraction, backfill, log strings, cost calc pattern)**

---

## PART 1: CAN rig-core's CompletionClient UNIFY ALL 3?

### rig-core 0.32/0.33 type system

```rust
// From rig-core/src/client/completion.rs
pub trait CompletionClient {
    type CompletionModel: CompletionModel<Client = Self>;
    fn completion_model(&self, model: impl Into<String>) -> Self::CompletionModel;
}
```

All three provider clients (`anthropic::Client`, `openai::Client`, `mistral::Client`, etc.) implement `CompletionClient`. The associated type `CompletionModel` differs per provider:
- `anthropic::Client::CompletionModel` = `anthropic::completion::CompletionModel`
- `openai::Client::CompletionModel` = `openai::completion::CompletionModel`
- etc.

### Required trait bounds

Our `stream_with_tools` method requires:
```rust
M: rig::completion::CompletionModel + Clone + 'static,
<M as rig::completion::CompletionModel>::Response: Send,
```

This means the generic signature must propagate:
```rust
async fn run_agent_loop<C>(&mut self, client: C, model_name: &str, provider_kind: Option<ProviderKind>)
    -> Result<RigAgentLoopResult, NikaError>
where
    C: CompletionClient,
    C::CompletionModel: Clone + 'static,
    <C::CompletionModel as rig::completion::CompletionModel>::Response: Send,
```

### Type system limitations

1. **No issue**: `CompletionClient` is the right abstraction level. All 7 cloud providers implement it.
2. **No issue**: The `Clone + 'static + Send` bounds are satisfied by all rig-core provider models (verified by existing `run_generic_provider_impl` which already works for Mistral/Groq/DeepSeek/Gemini/xAI).
3. **Potential issue**: `run_claude` and `run_openai` are called with concrete types today. Making them call the generic method means the generic method must be monomorphized for `anthropic::Client` and `openai::Client`. This is **fine** — it's just compile-time cost, no runtime difference.
4. **No issue**: Extended thinking doesn't go through the generic loop — it short-circuits in the wrapper. So no thinking-related bounds needed on the generic method.

### Verdict: YES, `CompletionClient` can unify all 3.

`run_generic_provider_impl` already proves this works. The refactor is simply:
1. Rename `run_generic_provider_impl` to `run_agent_loop`
2. Make `run_claude` and `run_openai` thin wrappers that call `run_agent_loop`
3. Delete the 800 LOC of duplicated logic from `run_claude`/`run_openai`

---

## PART 2: EXACT GENERIC SIGNATURE

```rust
/// Core agent execution loop — used by ALL providers.
///
/// This is THE implementation of the agent loop: streaming, retry, guardrails,
/// limits, events. Every provider-specific `run_*` method is a thin wrapper
/// that constructs a client and calls this.
///
/// # Arguments
/// - `client`: Provider-specific client (e.g., `anthropic::Client::from_env()`)
/// - `model_name`: Model name after prefix stripping (e.g., "claude-sonnet-4-6")
/// - `provider_kind`: Used for cost calculation. `None` = $0 cost.
///
/// # Type constraints
/// - `C: CompletionClient` — rig-core unified provider trait
/// - `C::CompletionModel: Clone + 'static` — needed for retry (model reuse across turns)
/// - `Response: Send` — needed for async streaming across `.await` points
async fn run_agent_loop<C>(
    &mut self,
    client: C,
    model_name: &str,
    provider_kind: Option<crate::provider::cost::ProviderKind>,
) -> Result<RigAgentLoopResult, NikaError>
where
    C: CompletionClient,
    C::CompletionModel: Clone + 'static,
    <C::CompletionModel as rig::completion::CompletionModel>::Response: Send,
{
    // Backfill params.provider for stop_sequences key mapping (no-op when already set)
    if self.params.provider.is_none() {
        if let Some(ref pk) = provider_kind {
            self.params.provider = Some(pk.to_provider_id().to_string());
        }
    }

    let model_name = Self::strip_model_prefix(model_name);
    let model = client.completion_model(model_name);
    let tools = self.tools_as_boxed();
    let max_turns = self.params.max_turns.unwrap_or(10) as usize;
    let base_prompt = self.params.prompt.clone();
    let max_retries = self.get_low_confidence_config()
        .map(|c| c.max_retries)
        .unwrap_or(2);

    // Helper: compute cost for a turn
    let compute_cost = |input: u64, output: u64, cached: u64| -> f64 {
        provider_kind
            .map(|pk| crate::provider::cost::calculate_cost_with_cache(
                pk, model_name, input, output, cached,
            ))
            .unwrap_or(0.0)
    };

    let provider_label = provider_kind
        .map(|pk| pk.display_name())
        .unwrap_or("Unknown");

    // ... (rest of the loop body — taken from run_generic_provider_impl)
    // All tracing::warn! messages use `provider_label` for interpolation
    // instead of hardcoded "Claude" / "OpenAI"
}
```

### Provider-specific concerns handled OUTSIDE the generic loop

| Concern | Where handled | How |
|---|---|---|
| Extended thinking | `run_claude()` wrapper | Short-circuit before calling `run_agent_loop` |
| stop_sequences key | `stop_sequences_params()` (already exists in mod.rs) | Uses `params.provider` string, set by D5 backfill |
| Cost calculation | `compute_cost` closure inside `run_agent_loop` | Uses `Option<ProviderKind>`, `unwrap_or(0.0)` |
| Model prefix stripping | Inside `run_agent_loop` | `strip_model_prefix()` already handles all prefixes |

---

## PART 3: MIGRATION PLAN (step-by-step, tests green at each step)

### Step 0: Preparation (5 min)

```bash
# Baseline: verify all tests pass
cargo test -p nika-engine --lib -- rig_agent_loop
# Record count (expect ~55 tests)
```

### Step 1: Rename `run_generic_provider_impl` to `run_agent_loop` (10 min)

**Why this first**: It already works, already generic, already used by 5 providers. Pure rename.

1. In `providers.rs`, rename `run_generic_provider_impl` to `run_agent_loop`
2. Update all 5 callers: `run_mistral`, `run_groq`, `run_deepseek`, `run_gemini`, `run_xai`
3. Run tests — must be green (pure rename, no behavior change)

**Commit point**: `refactor(agent): rename run_generic_provider_impl to run_agent_loop`

### Step 2: Make `run_claude` call `run_agent_loop` (~45 min)

This is the big one. Replace 405 LOC with ~10 lines.

1. Rewrite `run_claude`:
```rust
pub async fn run_claude(&mut self) -> Result<RigAgentLoopResult, NikaError> {
    // Extended thinking: separate path (single-turn, no tools, no retry)
    if self.params.extended_thinking == Some(true) {
        return self.run_claude_with_thinking().await;
    }
    let client = anthropic::Client::from_env();
    let model_name = self.params.model.clone().ok_or_else(|| NikaError::ValidationError {
        reason: "model field is required for LLM verbs (NIKA-034)".to_string(),
    })?;
    self.run_agent_loop(client, &model_name, Some(ProviderKind::Claude)).await
}
```

2. Run ALL tests:
```bash
cargo test -p nika-engine --lib -- rig_agent_loop
cargo test -p nika-engine --lib -- agent  # broader agent tests
```

3. Verify no behavior change: mock tests pass, same event emission, same result shape.

**Critical risk**: `run_claude` uses `ProviderKind::Claude` directly (no `Option::map`), while `run_agent_loop` uses `provider_kind.map(|pk| ...).unwrap_or(0.0)`. Since we pass `Some(ProviderKind::Claude)`, the `map` unwraps and the computation is identical. No risk.

**Commit point**: `refactor(agent): run_claude delegates to run_agent_loop (-395 LOC)`

### Step 3: Make `run_openai` call `run_agent_loop` (~15 min)

Same pattern as step 2 but simpler (no extended thinking):

```rust
pub async fn run_openai(&mut self) -> Result<RigAgentLoopResult, NikaError> {
    let client = openai::Client::from_env();
    let model_name = self.params.model.clone().ok_or_else(|| NikaError::ValidationError {
        reason: "model field is required for LLM verbs (NIKA-034)".to_string(),
    })?;
    self.run_agent_loop(client, &model_name, Some(ProviderKind::OpenAI)).await
}
```

Run tests. All green.

**Commit point**: `refactor(agent): run_openai delegates to run_agent_loop (-390 LOC)`

### Step 4: Clean up log messages (~10 min)

In `run_agent_loop`, replace hardcoded provider names with interpolated label:

```rust
let provider_label = provider_kind
    .map(|pk| pk.display_name())
    .unwrap_or("Unknown");

// Then in tracing:
tracing::warn!(
    task_id = %self.task_id,
    provider = provider_label,
    "Agent limit exceeded after first turn"
);
```

Run tests. All green.

**Commit point**: `refactor(agent): interpolate provider name in log messages`

### Step 5: Verify line count (~5 min)

```bash
wc -l nika-engine/src/runtime/rig_agent_loop/providers.rs
# Expected: ~550-620 lines (was 1505)
# Breakdown: run_agent_loop (~420) + run_mock (~60) + run_auto (~50)
#   + 7 thin wrappers (~8 lines each = ~56) + imports/docs (~30)
```

**Total commit: 4 commits, tests green at every step.**

---

## PART 4: Wire token_budget (~30 min)

**File**: `nika-engine/src/runtime/rig_agent_loop/providers.rs` (inside `run_agent_loop`)

### The bug (SF9)

`AgentParams.token_budget` is parsed from YAML but NEVER wired into `LimitTracker`. The `LimitTracker` only gets limits from `AgentParams.limits.max_tokens`, not `token_budget`.

### The fix

At the top of `run_agent_loop`, before the main loop:
```rust
// Wire token_budget into LimitTracker (fixes SF9)
// token_budget is a shorthand for limits.max_tokens
if let Some(budget) = self.params.token_budget {
    if self.limit_tracker.config().max_tokens == 0 {
        // Only override if limits.max_tokens is not already set
        // (explicit limits: config takes precedence over shorthand)
        self.limit_tracker.set_max_tokens(budget as u64);
    }
}
```

This requires adding `set_max_tokens` to `LimitTracker`:
```rust
/// Override max_tokens limit (used for token_budget shorthand).
pub fn set_max_tokens(&mut self, max: u64) {
    self.config.max_tokens = max;
}
```

### Test

```rust
#[test]
fn test_token_budget_wired_into_limit_tracker() {
    let params = AgentParams {
        prompt: "Test".to_string(),
        token_budget: Some(100),
        ..Default::default()
    };
    let event_log = EventLog::new();
    let mcp_clients = FxHashMap::default();
    let agent = RigAgentLoop::new("test".to_string(), params, event_log, mcp_clients).unwrap();
    // After construction, the limit tracker should have max_tokens = 100
    // (wiring happens in run_agent_loop, so we test via run_mock or a dedicated method)
}
```

**Commit point**: `fix(agent): wire token_budget into LimitTracker (SF9)`

---

## PART 5: Integrate extended_thinking (~1h)

**File**: `nika-engine/src/runtime/rig_agent_loop/thinking.rs`

### Current state (SF10)

`run_claude_with_thinking` (L330-528 in thinking.rs):
- Single-turn only (no multi-turn tool loop)
- No retry loop
- No guardrail retry loop
- No limit tracking
- Reports 1 turn always
- Calculates cost once at the end, not per-turn

This means an agent with `extended_thinking: true` + `tools: [...]` + `guardrails: [...]` silently runs in degraded mode: tools are ignored, guardrails run once without retry, limits are unchecked.

### Option A: Integrate into main loop (preferred)

Modify `run_agent_loop` to handle extended thinking as a mode flag:

```rust
// Inside run_agent_loop, determine if thinking mode should be used
let use_thinking = self.params.extended_thinking == Some(true)
    && provider_kind == Some(ProviderKind::Claude);

// First attempt: choose streaming strategy
let mut result = if use_thinking {
    // Extended thinking: use stream_with_thinking() which adds thinking config
    // to the completion request's additional_params
    self.stream_with_thinking(model.clone(), &current_prompt, tools, max_turns).await?
} else {
    self.stream_with_tools(model.clone(), &current_prompt, tools, max_turns).await?
};
```

This requires extracting a new `stream_with_thinking` method that works WITH tools, not the current single-turn approach.

**Key changes to thinking.rs:**
1. Rename current `run_claude_with_thinking` to `run_claude_with_thinking_legacy` (keep for fallback)
2. Create `stream_with_thinking` that mirrors `stream_with_tools` but:
   - Adds `additional_params` with `thinking` config
   - Forces `temperature: 1.0` (Anthropic constraint)
   - Calculates `max_tokens > thinking_budget`
   - Captures `ReasoningDelta` and `Reasoning` chunks
3. Remove the short-circuit in `run_claude` wrapper (extended thinking now goes through the main loop)

**Risk**: Extended thinking with tools is a real API call pattern that may have provider-side limitations. Need to verify with Anthropic's API docs whether extended thinking + tool use is supported simultaneously.

**Mitigation**: If tools + thinking is not supported by the API, validate at the analyzer level (NIKA-034) and emit a clear error. The main loop still works — just without tools.

### Option B: Validate at analyzer (minimum viable)

In the AST analyzer phase, if `extended_thinking: true` AND `tools:` is non-empty, emit:
```
NIKA-034: extended_thinking does not support tools. Remove tools: or disable extended_thinking.
```

This is safe but disappointing — it prevents users from using thinking + tools.

### Decision tree

- If Anthropic API supports thinking + tools -> **Option A**
- If not -> **Option B** at analyzer, plus refactor `run_claude_with_thinking` to use limits + guardrails

### Test

```rust
#[tokio::test]
async fn test_extended_thinking_with_guardrails_integration() {
    // This test verifies that extended thinking goes through guardrails
    // (currently broken: single-turn path runs guardrails but no retry)
    // TODO: requires mock provider that returns thinking blocks
}
```

**Commit point**: `fix(agent): integrate extended_thinking into main loop with retry+guardrails (SF10)`

---

## PART 6: WHAT COULD GO WRONG

### Risk 1: Streaming race conditions

**Scenario**: Multiple `stream_with_tools` calls (retry loop) sharing the same `stream_tx` channel.

**Analysis**: Not a real risk. Each `stream_with_tools` call creates a new stream. The `stream_tx` is a `mpsc::Sender` — multiple sends are fine. The TUI receiver sees tokens from all retries sequentially.

**BUT**: The TUI might show retry tokens mixed with previous attempt tokens. This is cosmetic, not a correctness issue. The `response_text` is always overwritten by `FinalResponse`, so the final output is correct.

### Risk 2: Token counting drift

**Scenario**: `stream_with_tools` returns token counts from `FinalResponse.usage()`, but during retries, we accumulate `total_input_tokens += result.input_tokens`. If a retry fails mid-stream (timeout), the partial tokens are lost.

**Analysis**: Current behavior is the same across all 3 methods — this is not a regression from the refactor. But it IS a pre-existing bug worth documenting.

**Mitigation**: After the refactor, add a TODO for "token estimation fallback when streaming fails mid-stream" (M-tok1 from master plan).

### Risk 3: Cost calculation rounding

**Scenario**: `calculate_cost_with_cache` returns `f64`. Multiple additions (`total_cost += turn_cost`) accumulate floating-point error.

**Analysis**: At current token costs (~$0.003/1K tokens), floating-point error is negligible (< 1e-15 per addition). Would require ~10^12 turns to accumulate a $0.01 error.

**Mitigation**: The `is_finite()` check in the final `ProviderResponded` event already handles NaN/Infinity. No action needed.

### Risk 4: `params.provider` mutation across retries

**Scenario**: The D5 backfill (`self.params.provider = Some(...)`) mutates `self.params`. If `run_agent_loop` is called twice (e.g., after a failure + retry at the workflow level), `params.provider` is already set from the first call.

**Analysis**: This is a no-op — the `if self.params.provider.is_none()` guard prevents double-setting. No risk.

### Risk 5: Monomorphization code bloat

**Scenario**: `run_agent_loop<C>` is monomorphized for 7 provider types: `anthropic::Client`, `openai::Client`, `mistral::Client`, `groq::Client`, `deepseek::Client`, `gemini::Client`, `xai::Client`. Each generates ~420 lines of machine code.

**Analysis**: 7 * 420 lines = ~2940 lines of generated code. This is LESS than the current 1505 lines of source code that compile to ~1505 lines of machine code for 3 separate methods, plus the 425-line generic that's already monomorphized for 5 types. Net change is roughly neutral or slightly larger due to `anthropic::Client` and `openai::Client` now going through the generic path.

**Mitigation**: Not needed. The binary size increase is negligible (~50KB at most). If it becomes an issue, use `#[inline(never)]` on the generic method body or extract the non-generic parts into helper functions.

### Risk 6: Accidental behavior change in run_mock

**Scenario**: `run_mock` is a special case that doesn't go through `run_agent_loop`. After the refactor, someone might accidentally make `run_mock` call `run_agent_loop` with a mock client.

**Analysis**: `run_mock` should stay independent — it's the only method that doesn't need an API key. No action needed, but document this in the code.

---

## PART 7: E2E VERIFICATION WORKFLOWS

These `.nika.yaml` files test the agent verb end-to-end. Run with `nika run --provider mock` for CI, or with real providers for manual verification.

### 7.1: Agent + tools + guardrails + explicit completion

```yaml
# tests/e2e/agent-guardrails-explicit.nika.yaml
schema: "nika/workflow@0.12"
workflow: agent-guardrails-explicit
description: "Agent with tools, guardrails, and explicit completion mode"
provider: mock

tasks:
  - id: research
    agent:
      prompt: "Research the topic 'Rust async patterns' and provide a structured summary."
      tools: [nika:complete, nika:log]
      max_turns: 5
      completion:
        mode: explicit
      guardrails:
        - type: length
          min_words: 50
          max_words: 500
          on_failure: retry
        - type: regex
          pattern: "(?i)(summary|conclusion|findings)"
          message: "Response must contain 'Summary', 'Conclusion', or 'Findings'"
          on_failure: retry
```

### 7.2: Agent + extended_thinking (Claude-specific)

```yaml
# tests/e2e/agent-extended-thinking.nika.yaml
schema: "nika/workflow@0.12"
workflow: agent-extended-thinking
description: "Agent with extended thinking (Claude-only feature)"
provider: anthropic
model: claude-sonnet-4-20250514

tasks:
  - id: think_deeply
    agent:
      prompt: "Analyze the trade-offs between actor-based and CSP-based concurrency models in Rust."
      extended_thinking: true
      thinking_budget: 8000
      max_turns: 1
      guardrails:
        - type: length
          min_words: 100
          on_failure: fail
```

### 7.3: Agent + token_budget (must stop early)

```yaml
# tests/e2e/agent-token-budget.nika.yaml
schema: "nika/workflow@0.12"
workflow: agent-token-budget
description: "Agent with strict token budget — must stop before completing fully"
provider: mock

tasks:
  - id: limited_agent
    agent:
      prompt: "Write a comprehensive essay about the history of computing from 1940 to 2025."
      token_budget: 1000
      max_turns: 10
      limits:
        on_limit_reached:
          action: complete_partial
```

### 7.4: Agent + max_turns: 2 (must respect)

```yaml
# tests/e2e/agent-max-turns.nika.yaml
schema: "nika/workflow@0.12"
workflow: agent-max-turns
description: "Agent limited to 2 turns — must produce output within constraint"
provider: mock

tasks:
  - id: quick_agent
    agent:
      prompt: "List 3 benefits of Rust's ownership system."
      max_turns: 2
      tools: [nika:complete]
      completion:
        mode: natural
```

### 7.5: Agent + cost limit (max_cost_usd)

```yaml
# tests/e2e/agent-cost-limit.nika.yaml
schema: "nika/workflow@0.12"
workflow: agent-cost-limit
description: "Agent with a cost ceiling — must stop before exceeding budget"
provider: mock

tasks:
  - id: budgeted_agent
    agent:
      prompt: "Explain quantum computing in simple terms."
      max_turns: 20
      limits:
        max_cost_usd: 0.50
        on_limit_reached:
          action: fail
```

### Running E2E tests

```bash
# Dry-run validation (syntax + DAG, no API calls)
nika check tests/e2e/agent-guardrails-explicit.nika.yaml
nika check tests/e2e/agent-extended-thinking.nika.yaml
nika check tests/e2e/agent-token-budget.nika.yaml
nika check tests/e2e/agent-max-turns.nika.yaml
nika check tests/e2e/agent-cost-limit.nika.yaml

# Mock execution (no API keys needed)
nika run tests/e2e/agent-guardrails-explicit.nika.yaml --provider mock
nika run tests/e2e/agent-token-budget.nika.yaml --provider mock
nika run tests/e2e/agent-max-turns.nika.yaml --provider mock
nika run tests/e2e/agent-cost-limit.nika.yaml --provider mock

# Real execution (requires API keys)
nika run tests/e2e/agent-extended-thinking.nika.yaml  # Claude only
```

---

## PART 8: VERIFICATION CHECKLIST

### Phase 1 (refactor) — after each commit
- [ ] `cargo test -p nika-engine --lib -- rig_agent_loop` — ALL tests pass
- [ ] `cargo test --workspace --lib` — full suite green
- [ ] `cargo clippy --workspace -- -D warnings` — 0 warnings
- [ ] `wc -l providers.rs` — should decrease ~800 LOC per step

### Phase 2 (token_budget) — after commit
- [ ] New test: `test_token_budget_wired_into_limit_tracker` passes
- [ ] Existing limit tracker tests still pass
- [ ] `LimitTracker::set_max_tokens` has test

### Phase 3 (extended_thinking) — after commit
- [ ] If Option A: thinking + tools works end-to-end
- [ ] If Option B: analyzer rejects `extended_thinking: true` + `tools: [...]`
- [ ] Existing thinking tests still pass

### Final verification
- [ ] `cargo test --workspace --lib` — full suite (expect 8600+)
- [ ] `cargo clippy --workspace -- -D warnings` — 0 warnings
- [ ] `wc -l providers.rs` reports ~550-620 (was 1505)
- [ ] E2E workflows pass `nika check`
- [ ] Git history: 4-6 clean commits, each with tests green

---

## Files Modified
- `nika-engine/src/runtime/rig_agent_loop/providers.rs` — main refactor (1505 -> ~600 LOC)
- `nika-engine/src/runtime/rig_agent_loop/thinking.rs` — integrate or validate
- `nika-engine/src/runtime/rig_agent_loop/mod.rs` — token_budget wiring (if needed in constructor)
- `nika-engine/src/runtime/limit_tracker.rs` — add `set_max_tokens` method
- `nika-engine/src/runtime/rig_agent_loop/tests.rs` — add token_budget + thinking tests
- `tests/e2e/agent-*.nika.yaml` — 5 E2E verification workflows

## Commit Strategy
- Commit 1: `refactor(agent): rename run_generic_provider_impl to run_agent_loop`
- Commit 2: `refactor(agent): run_claude delegates to run_agent_loop (-395 LOC)`
- Commit 3: `refactor(agent): run_openai delegates to run_agent_loop (-390 LOC)`
- Commit 4: `refactor(agent): interpolate provider name in log messages`
- Commit 5: `fix(agent): wire token_budget into LimitTracker (SF9)`
- Commit 6: `fix(agent): integrate extended_thinking into main loop (SF10)` or `fix(agent): validate extended_thinking + tools at analyzer (SF10)`
- Commit 7: `test(agent): add E2E workflows for agent verb coverage`

## Risk Mitigation
This is a large refactor. If stuck:
1. Use git worktree for isolation (`git worktree add ../nika-session-b session-b`)
2. Run tests after EVERY method extraction (not just at the end)
3. Keep the old methods commented out until all tests pass
4. `run_generic_provider_impl` already works — the refactor is making `run_claude`/`run_openai` use it
5. If extended thinking + tools is not supported by Anthropic API, fall back to Option B (analyzer validation)
6. If any step breaks tests, `git stash` and investigate before proceeding
