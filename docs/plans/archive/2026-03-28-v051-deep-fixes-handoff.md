# v0.51 Deep Fixes Handoff — 3 Architectural Bugs

> **Copy this entire prompt into a fresh Claude Code session at `/Users/thibaut/dev/supernovae/nika/tools`**

---

## CONTEXT

3 bugs remain from the v0.50 mega audit that require **architectural changes** (wider types, new code paths). The v0.50 session fixed 5/8 bugs — these 3 survived because they cross module boundaries and need careful propagation.

**Session state:** 8,634 tests pass, 0 failures. All previous fixes pushed. Clean working tree on `main`.

**Approach:** TDD strict (RED -> GREEN -> REFACTOR). `cargo test --workspace --lib` always with `--lib`. 1 fix = 1 commit. Granular commits, push after each.

**Methodology:**
1. Use `rust-core` skill for Rust patterns + error handling
2. Use `test-driven-development` skill for RED -> GREEN -> REFACTOR
3. Use `verification-before-completion` skill before each commit
4. Use `find-docs` for rig-core API exploration (Context7)
5. Use `systematic-debugging` if anything breaks unexpectedly

---

## PHASE 0: RESEARCH (mandatory before ANY code)

### 0.1 — Verify baseline

```bash
cd /Users/thibaut/dev/supernovae/nika/tools
cargo test --workspace --lib 2>&1 | grep "test result:" | awk '{sum+=$4} END {print sum, "tests"}'
git log --oneline -5
git status --short
```

### 0.2 — rig-core API surface for thinking + tools

**CRITICAL:** Before fixing Bug 3, we MUST know if rig-core 0.33+ supports `additional_params()` on `AgentBuilder`. This determines the entire approach.

```bash
# Check rig-core version
grep "rig-core" tools/Cargo.lock | head -3

# Search for additional_params on AgentBuilder
find target -name "*.rs" -path "*/rig-core/*" -exec grep -l "additional_params" {} \; 2>/dev/null | head -5

# Check if AgentBuilder has additional_params
grep -rn "additional_params" target/debug/build/rig-core-*/out/ 2>/dev/null | head -5
```

**Also use Context7 (find-docs skill):**
```
ctx7 library "rig-core rust llm" "AgentBuilder additional_params tools"
```

### 0.3 — Anthropic API thinking token fields

Use `perplexity_search_web`:
1. **"anthropic api thinking tokens usage response field 2025"** — find the exact field name for thinking tokens in the usage response
2. **"rig-core rust anthropic extended thinking tool_use 2025"** — check if rig-core exposes thinking tokens

### 0.4 — Swarm analysis (3 agents in parallel)

**Agent 1 — Thinking Token Deep Dive:**
> "Read tools/nika-engine/src/runtime/rig_agent_loop/thinking.rs COMPLETELY. Read tools/nika-engine/src/runtime/rig_agent_loop/streaming.rs. Trace how token_usage() is extracted from rig-core. Does rig-core's GetTokenUsage trait expose thinking_input_tokens? Search the rig-core source in target/ for any mention of thinking. Report exact API surface."

**Agent 2 — InferCallback Refactor Design:**
> "Read tools/nika-engine/src/runtime/structured_output.rs (the InferCallback type at line 65, try_layer_3 at line 442, try_layer_4 at line 623). Read tools/nika-engine/src/runtime/executor/infer.rs (callback construction at line 873, engine init at line 891). Design 2 options: (A) widen InferCallback to take InferOptions struct, (B) store temperature/system on StructuredOutputEngine and inject into retry prompt. Report trade-offs."

**Agent 3 — Tools + Thinking Compatibility:**
> "Read tools/nika-engine/src/runtime/rig_agent_loop/thinking.rs and tools/nika-engine/src/runtime/rig_agent_loop/streaming.rs. Find stream_with_tools() and stream_with_tools_streaming(). Determine: can we add additional_params(thinking_config) to the AgentBuilder path? Read tools/nika-engine/src/runtime/rig_agent_loop/mod.rs for tools_as_boxed(). Propose how to merge thinking + tools into one code path."

---

## BUG 1: Thinking Tokens Not Priced Separately

### The Problem

Anthropic charges thinking tokens at the **input token rate** (not output rate). Currently, thinking tokens are lumped into `output_tokens`, causing cost to be calculated at the output rate ($15/M for Opus) instead of the input rate ($15/M for Opus input — same for Opus, but $3/$15 for Sonnet where it matters: thinking should be $3/M not $15/M).

### Files to Modify

1. `tools/nika-engine/src/provider/cost.rs` — ModelPricing struct + pricing tables
2. `tools/nika-engine/src/runtime/rig_agent_loop/thinking.rs` — token extraction + cost call
3. `tools/nika-engine/src/runtime/rig_agent_loop/streaming.rs` — token extraction

### Architecture

**Option A (recommended): Separate thinking_tokens counter**

```rust
// In cost.rs — add to calculate functions:
pub fn calculate_with_thinking(
    &self,
    input_tokens: u64,
    output_tokens: u64,
    thinking_tokens: u64,
) -> f64 {
    // Thinking tokens priced at INPUT rate (Anthropic policy)
    let thinking_cost = (thinking_tokens as f64 / 1_000_000.0) * self.input_per_million;
    let input_cost = (input_tokens as f64 / 1_000_000.0) * self.input_per_million;
    let output_cost = (output_tokens as f64 / 1_000_000.0) * self.output_per_million;
    let cost = thinking_cost + input_cost + output_cost;
    if cost.is_finite() { cost } else { 0.0 }
}
```

```rust
// In thinking.rs — extract thinking tokens separately:
let mut thinking_tokens: u64 = 0;
// ... in streaming loop:
StreamedAssistantContent::ReasoningDelta { .. } => {
    thinking_tokens += estimate_tokens(content.len());
}
// ... in cost calculation:
cost_usd: pricing.calculate_with_thinking(input_tokens, output_tokens, thinking_tokens),
```

**Option B: New ModelPricing field**

Add `thinking_per_million: Option<f64>` to ModelPricing. Only Claude models set it. Falls back to `input_per_million` if None.

**Recommendation:** Option A is simpler — no schema change needed for the pricing table. The Anthropic pricing rule (thinking = input rate) is universal for all Claude models.

### TDD Steps

1. Write test: `calculate_with_thinking` gives lower cost than lumping thinking into output
2. RED — function doesn't exist
3. Implement `calculate_with_thinking` in cost.rs
4. GREEN
5. Write test: thinking.rs captures separate thinking token count
6. Wire thinking_tokens through the cost call
7. Run `cargo test --workspace --lib`
8. Commit

### Estimated: ~150 lines

---

## BUG 2: Structured Output Retries Ignore Temperature/System

### The Problem

`InferCallback` takes only `String` (the retry prompt). The original task's `temperature` and `system` prompt are lost. Retries happen with default model settings.

### Files to Modify

1. `tools/nika-engine/src/runtime/structured_output.rs` — InferCallback type + engine fields
2. `tools/nika-engine/src/runtime/executor/infer.rs` — callback construction + engine init

### Architecture

**Option A (recommended): Store on StructuredOutputEngine, inject into prompt**

Don't change InferCallback signature (breaking change). Instead:

```rust
// In structured_output.rs:
pub struct StructuredOutputEngine {
    // ... existing fields ...
    original_system: Option<String>,
    original_temperature: Option<f64>,
}

impl StructuredOutputEngine {
    pub fn with_system(mut self, system: Option<String>) -> Self {
        self.original_system = system;
        self
    }
    pub fn with_temperature(mut self, temp: Option<f64>) -> Self {
        self.original_temperature = temp;
        self
    }
}
```

Then in `generate_retry_prompt()`, prepend the system context:
```rust
pub fn generate_retry_prompt(&self, original_prompt: &str, ...) -> String {
    let system_context = self.original_system.as_deref()
        .map(|s| format!("System context: {}\n\n", s))
        .unwrap_or_default();
    let temp_hint = self.original_temperature
        .map(|t| format!("\n(Use temperature={:.1} for consistency)", t))
        .unwrap_or_default();
    format!("{system_context}{original_prompt}\n\n...{temp_hint}")
}
```

**Option B: Widen InferCallback to InferRequest struct**

```rust
pub struct InferRequest {
    pub prompt: String,
    pub temperature: Option<f64>,
    pub system: Option<String>,
}
pub type InferCallback = Arc<
    dyn Fn(InferRequest) -> Pin<Box<dyn Future<Output = Result<String, NikaError>> + Send>>
        + Send + Sync,
>;
```

This is cleaner but requires updating ALL callback construction sites and the provider.infer() call.

**Recommendation:** Option A is safer (no API change). Temperature is a hint in the prompt, not a parameter — but it's better than nothing. Option B is the right long-term fix but more invasive.

### TDD Steps

1. Write test: `engine.with_system("expert").with_temperature(0.0)` stores values
2. Write test: `generate_retry_prompt` includes system context when set
3. RED
4. Add fields + builder methods to StructuredOutputEngine
5. Modify `generate_retry_prompt` to use stored system/temperature
6. GREEN
7. Wire in `executor/infer.rs:891` — pass system/temperature to engine
8. Run `cargo test --workspace --lib`
9. Commit

### Estimated: ~110 lines

---

## BUG 3: Extended Thinking Agent Drops Tools

### The Problem

`run_claude_with_thinking()` uses `model.completion_request().additional_params(thinking).build().stream()` which doesn't support tools. The normal agent path uses `AgentBuilder::new(model).tools(tools)` which doesn't support `additional_params`.

### Files to Modify

1. `tools/nika-engine/src/runtime/rig_agent_loop/thinking.rs` — entire approach
2. `tools/nika-engine/src/runtime/rig_agent_loop/streaming.rs` — extend stream_with_tools
3. `tools/nika-engine/src/runtime/rig_agent_loop/providers.rs` — dispatch logic

### Architecture

**CRITICAL DEPENDENCY:** Must determine if rig-core's AgentBuilder supports `additional_params()` (Phase 0.2 research).

**Option A: If AgentBuilder supports additional_params**

Extend `stream_with_tools()` to accept thinking params:

```rust
// In streaming.rs:
pub(super) async fn stream_with_tools<M>(
    &mut self,
    model: M,
    prompt: &str,
    tools: Vec<Box<dyn rig::tool::ToolDyn>>,
    max_turns: usize,
    thinking_budget: Option<u64>,  // NEW
) -> Result<StreamingResult, NikaError> {
    // ...
    let mut builder = AgentBuilder::new(model)
        .preamble(&preamble)
        .tools(tools)
        .max_tokens(effective_max_tokens);

    if let Some(budget) = thinking_budget {
        builder = builder.additional_params(json!({
            "thinking": { "type": "enabled", "budget_tokens": budget }
        }));
    }
    // ...
}
```

Then in `providers.rs`, remove the early return:
```rust
pub async fn run_claude(&mut self) -> Result<RigAgentLoopResult, NikaError> {
    let tools = self.tools_as_boxed();
    let thinking_budget = if self.params.extended_thinking == Some(true) {
        Some(self.params.effective_thinking_budget())
    } else {
        None
    };

    // Single path for both thinking and non-thinking
    let result = self.stream_with_tools(model, &prompt, tools, max_turns, thinking_budget).await?;
}
```

**Option B: If AgentBuilder does NOT support additional_params**

Create a hybrid path that manually handles tools in the thinking streaming loop:

```rust
// In thinking.rs — add manual tool dispatch:
async fn run_claude_with_thinking(&mut self) -> Result<...> {
    let tools = self.tools_as_boxed();

    for turn in 0..max_turns {
        let request = model.completion_request(&prompt)
            .preamble(preamble)
            .additional_params(thinking_config)
            .tools(&tools)  // ← IF available on completion_request
            .build();

        let stream = model.stream(request).await?;
        // ... handle thinking + tool calls
    }
}
```

**Option C: Use rig-core's completion API with tool_choice**

Instead of AgentBuilder, use the raw completion API:
```rust
let mut request = model.completion_request(&prompt)
    .preamble(preamble)
    .additional_params(json!({
        "thinking": { "type": "enabled", "budget_tokens": budget },
        "tools": serialize_tools(&tools),
        "tool_choice": "auto"
    }))
    .build();
```

This puts tools in `additional_params` alongside thinking config. Anthropic's API accepts both.

**Recommendation:** Do Phase 0.2 research first. If Option A works, it's the cleanest (5 lines changed). If not, Option C is the most pragmatic.

### TDD Steps

1. **Phase 0.2 FIRST** — determine rig-core capability
2. Write test: agent with extended_thinking=true AND tools produces tool calls
3. RED (tools dropped)
4. Implement chosen option
5. GREEN
6. Write test: agent with extended_thinking=true, no tools, still works (regression)
7. Run `cargo test --workspace --lib`
8. Commit

### Estimated: 65-285 lines (depends on rig-core capability)

---

## EXECUTION ORDER

```
Phase 0: Research                        ← 15 min
  ├── 0.1 Baseline verification
  ├── 0.2 rig-core API exploration       ← CRITICAL for Bug 3
  ├── 0.3 Anthropic thinking tokens API
  └── 0.4 Swarm analysis (3 agents)

Phase 1: Bug 2 — Retry temp/system       ← Easiest, no external deps
  ├── TDD: engine fields + builder
  ├── TDD: retry prompt with system
  ├── Wire in executor/infer.rs
  └── Commit + push

Phase 2: Bug 1 — Thinking tokens cost    ← Medium, self-contained
  ├── TDD: calculate_with_thinking
  ├── Extract thinking_tokens in thinking.rs
  ├── Wire cost calculation
  └── Commit + push

Phase 3: Bug 3 — Thinking + tools        ← Hardest, depends on Phase 0.2
  ├── If Option A: extend stream_with_tools (small change)
  ├── If Option C: manual tool injection via additional_params
  ├── TDD: agent with thinking + tools
  └── Commit + push

Phase 4: Verification
  ├── cargo test --workspace --lib
  ├── cargo clippy --workspace -- -D warnings
  ├── git push
  └── Update memory
```

---

## RULES

1. **TDD STRICT** — test that fails first, always
2. **`--lib` ALWAYS** — NEVER `cargo test` without `--lib`
3. **1 fix = 1 commit** — format: `type(scope): description`
4. **Phase 0.2 is BLOCKING** — do NOT start Bug 3 without rig-core research
5. **Don't change InferCallback signature** for Bug 2 (Option A) — too many callers
6. **Thinking tokens = input rate** — this is Anthropic's pricing policy
7. **Push after each commit** — other sessions may be running

---

## CO-AUTHORS

```
Co-Authored-By: Nika 🦋 <nika@supernovae.studio>
```

---

## SKILLS TO USE

- **@rust-core** — Rust patterns, error handling, serde
- **@rust-async** — tokio async patterns for streaming
- **@test-driven-development** — TDD for every task
- **@find-docs** — Context7 for rig-core API docs
- **@systematic-debugging** — if anything breaks
- **@verification-before-completion** — before each commit
- **@spn-powers:executing-plans** — execute this plan task-by-task

---

## LINKS

| Resource | Path |
|----------|------|
| Cost module | `tools/nika-engine/src/provider/cost.rs` |
| Thinking loop | `tools/nika-engine/src/runtime/rig_agent_loop/thinking.rs` |
| Streaming | `tools/nika-engine/src/runtime/rig_agent_loop/streaming.rs` |
| Provider dispatch | `tools/nika-engine/src/runtime/rig_agent_loop/providers.rs` |
| Structured output | `tools/nika-engine/src/runtime/structured_output.rs` |
| Infer executor | `tools/nika-engine/src/runtime/executor/infer.rs` |
| InferCallback type | `tools/nika-engine/src/runtime/structured_output.rs:65` |
| ModelPricing struct | `tools/nika-engine/src/provider/cost.rs:109` |
| Nika dev reference | `tools/nika/CLAUDE.md` |
| Previous bug report | `docs/plans/2026-03-28-bug-report-workflow-v2.md` |
| Previous handoff | `docs/plans/2026-03-28-engine-fixes-round2-master-prompt.md` |
