# Quality Methodology — Zero Silent Failures

**Date**: 2026-03-28
**Status**: DRAFT — will be enriched with audit agent findings

---

## PART 1: THE PROBLEM

After weeks of bug-fixing (30+ agents, 350+ workflows, 55 bugs found), bugs keep appearing.
The root cause is not individual bugs — it's **systemic quality gaps**:

1. **Silent failures** — code returns 0/None/default instead of erroring
2. **Duplicated logic** — fixes applied to 1 of 3 copies
3. **Weak tests** — tests that pass regardless of code correctness
4. **Missing events** — state transitions without telemetry
5. **Stringly-typed APIs** — string matching instead of enum dispatch

---

## PART 2: NEW RULES (add to CLAUDE.md)

### Rule 1: No Silent Zeros

```rust
// FORBIDDEN — silently returns 0 tokens
let tokens = usage.map(|u| u.input_tokens).unwrap_or(0);

// REQUIRED — log when fallback is used
let tokens = match usage {
    Some(u) => u.input_tokens,
    None => {
        tracing::warn!(task_id = %id, "No token usage from provider, estimating");
        estimate_tokens(prompt.len())
    }
};
```

**Enforcement**: `cargo clippy` custom lint or grep CI check for `unwrap_or(0)`.

### Rule 2: No Catch-All Match Arms

```rust
// FORBIDDEN — swallows unknown variants
match event {
    Known1 => handle1(),
    Known2 => handle2(),
    _ => {} // SILENT SWALLOW
}

// REQUIRED — log unknown variants
match event {
    Known1 => handle1(),
    Known2 => handle2(),
    other => {
        tracing::warn!(?other, "Unhandled event variant");
    }
}
```

**Enforcement**: `#[deny(clippy::wildcard_enum_match_arm)]` in workspace Cargo.toml.

### Rule 3: Every State Transition Emits an Event

For task lifecycle: `Pending → Scheduled → Running → Completed/Failed`
For agent turns: `Started → Streaming → Completed → GuardrailCheck → Retry/Done`
For MCP: `Connecting → Connected → CallStarted → CallCompleted → Disconnected`

**Every arrow MUST have an EventKind emission.** If a state change happens without
an event, it's a bug.

### Rule 4: Tests Must Assert VALUES, Not Just Ok/Err

```rust
// FORBIDDEN — always passes if function returns Ok
assert!(result.is_ok());

// REQUIRED — verify the actual value
let result = result.unwrap();
assert_eq!(result.tokens, 42);
assert!(result.cost > 0.0, "Cost must be positive for paid providers");
```

### Rule 5: Every Bug Fix Needs a Regression Test

Not just "add a test" — the test must:
1. **FAIL before the fix** (prove the bug exists)
2. **PASS after the fix** (prove the fix works)
3. **Test the edge case**, not just the happy path

### Rule 6: No Duplication — Extract or Die

If you're about to write similar code in 2+ places: STOP. Extract a generic function.
The 3-copy `run_claude`/`run_openai`/`run_generic` pattern is the poster child of this failure.

### Rule 7: Errors Must Include Context

```rust
// FORBIDDEN — loses the "what" and "where"
return Err(NikaError::PromptError(e.to_string()));

// REQUIRED — include task ID, operation, and original error
return Err(NikaError::ProviderError {
    task_id: id.to_string(),
    provider: "anthropic".to_string(),
    operation: "infer_stream",
    source: e.to_string(),
});
```

---

## PART 3: TOOLING ADDITIONS

### 3.1 cargo-mutants — Find Weak Tests

```bash
# Install
cargo install --locked cargo-mutants

# Run on specific crate
cargo mutants -p nika-engine -- --lib

# Run on specific file (focus on known problem areas)
cargo mutants -p nika-engine -f src/provider/cost.rs -- --lib
cargo mutants -p nika-engine -f src/runtime/security.rs -- --lib
```

**How it works**: Injects mutations (e.g., `<` → `<=`, `+` → `-`, returns `""` instead of value).
If tests still pass = the test is WEAK. Each surviving mutant = a test that needs strengthening.

**When to run**: Before every release. On PR for changed files.

### 3.2 error-stack — Rich Error Context (evaluation)

`error-stack` preserves FULL error chains with:
- Automatic source locations
- Attachable metadata (task_id, provider, model)
- Error trees (not just chains)
- SpanTrace integration

**Migration path**: Gradual. Wrap new NikaError variants with `Report<NikaError>` in hot paths first (provider layer). Don't rewrite everything at once.

```rust
use error_stack::{Report, ResultExt};

// Before
fn infer(&self, prompt: &str) -> Result<String, NikaError> {
    provider.call(prompt).map_err(|e| NikaError::ProviderError { ... })?
}

// After (error-stack)
fn infer(&self, prompt: &str) -> Result<String, Report<NikaError>> {
    provider.call(prompt)
        .change_context(NikaError::ProviderError { ... })
        .attach_printable(format!("model: {}", self.model))
        .attach_printable(format!("prompt_len: {}", prompt.len()))?
}
```

### 3.3 nutype — Validated Newtypes

Prevent invalid states at the type level:

```rust
use nutype::nutype;

#[nutype(validate(greater = 0))]
pub struct TokenCount(u64);

#[nutype(validate(finite, greater_or_equal = 0.0))]
pub struct CostUsd(f64);

#[nutype(validate(greater = 0.0, less_or_equal = 2.0))]
pub struct Temperature(f32);
```

Now `TokenCount(0)` is a compile-time error. `CostUsd(f64::NAN)` is impossible.

### 3.4 Typestate Pattern for Task Lifecycle

Make it impossible to emit wrong events for wrong states:

```rust
struct Task<S: TaskState> { id: String, _state: PhantomData<S> }
struct Pending;
struct Running;
struct Completed;

impl Task<Pending> {
    fn schedule(self, event_log: &EventLog) -> Task<Running> {
        event_log.emit(TaskScheduled { id: &self.id }); // GUARANTEED
        Task { id: self.id, _state: PhantomData }
    }
}

impl Task<Running> {
    fn complete(self, event_log: &EventLog, result: Value) -> Task<Completed> {
        event_log.emit(TaskCompleted { id: &self.id }); // GUARANTEED
        Task { id: self.id, _state: PhantomData }
    }
}
// Task<Pending>.complete() → COMPILE ERROR
// Task<Completed>.schedule() → COMPILE ERROR
```

### 3.5 proptest — Property-Based Testing

Find edge cases that hand-written tests miss:

```rust
use proptest::prelude::*;

proptest! {
    #[test]
    fn template_roundtrip(input in "\\PC{1,100}") {
        // Any string passed through template resolution should survive
        let result = resolve_template(&format!("prefix_{}_suffix", input), &ctx);
        prop_assert!(result.contains("prefix_"));
        prop_assert!(result.contains("_suffix"));
    }

    #[test]
    fn cost_never_negative(
        input_tokens in 0u64..1_000_000,
        output_tokens in 0u64..1_000_000,
        cached in 0u64..1_000_000,
    ) {
        let cost = calculate_cost_with_cache(
            ProviderKind::Claude, "claude-sonnet-4-6",
            input_tokens, output_tokens, cached
        );
        prop_assert!(cost >= 0.0);
        prop_assert!(cost.is_finite());
    }
}
```

### 3.6 Strict Clippy Configuration

Add to workspace `Cargo.toml`:

```toml
[workspace.lints.clippy]
# Deny dangerous patterns
unwrap_used = "deny"           # Force explicit error handling
wildcard_enum_match_arm = "warn"  # Catch silent swallows
manual_assert = "warn"
redundant_else = "warn"

[workspace.lints.rust]
# Language-level strictness
unsafe_code = "deny"
missing_debug_implementations = "warn"
```

---

## PART 4: CI PIPELINE ADDITIONS

```yaml
# New CI steps (add to existing pipeline)
steps:
  # Existing
  - cargo test --workspace --lib
  - cargo clippy --workspace -- -D warnings

  # NEW: Mutation testing (weekly or pre-release)
  - cargo mutants -p nika-engine -f src/provider/cost.rs -- --lib
  - cargo mutants -p nika-engine -f src/runtime/security.rs -- --lib
  - cargo mutants -p nika-core -f src/binding/transform.rs -- --lib

  # NEW: Property-based tests
  - cargo test --workspace --lib -- proptest

  # NEW: Silent failure grep (fast, every PR)
  - grep -rn "unwrap_or(0)" src/ && exit 1 || true
  - grep -rn "unwrap_or_default()" src/ | grep -v "test" && exit 1 || true
  - grep -rn "_ => {}" src/ | grep -v "test" && exit 1 || true
```

---

## PART 5: EXECUTION PLAN (will be enriched with agent findings)

### Phase 0: Immediate (this session)
- [ ] Compile all 5 agent findings into this document
- [ ] Prioritize by severity
- [ ] Add to CLAUDE.md rules

### Phase 1: The Big Refactor (Session A, ~4h)
- [ ] H14: Extract `run_agent_loop<C>` — kill 1200 LOC duplication
- [ ] H6: Wire token_budget into LimitTracker
- [ ] H7: Extended thinking multi-turn

### Phase 2: Quality Infrastructure (Session B, ~2h)
- [ ] Add cargo-mutants to CI
- [ ] Run mutation testing on cost.rs, security.rs, transform.rs
- [ ] Fix all surviving mutants (= strengthen weak tests)
- [ ] Add proptest for cost calculation, template resolution

### Phase 3: Bug Sweep (Session C, ~4h)
- [ ] Fix ALL remaining bugs from v0.51 plan
- [ ] Fix ALL new bugs from agent audits
- [ ] Each fix = test FIRST (TDD), then code

### Phase 4: Hardening (Session D, ~2h)
- [ ] Add strict clippy lints
- [ ] Grep-and-fix all unwrap_or(0), _ => {}, .ok()
- [ ] Add missing events for every state transition
- [ ] Error context enrichment in provider layer

---

*This document will be updated with findings from 7 parallel agents.*
