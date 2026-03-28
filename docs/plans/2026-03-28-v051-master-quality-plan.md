# v0.51 Master Quality Plan — Zero Silent Failures

**Date**: 2026-03-28
**Author**: 7 deep-audit agents + 2 research agents + manual analysis
**State**: main @ a5408ec0d, 8613 tests, 0 clippy warnings

---

## EXECUTIVE SUMMARY

After 7 parallel agents audited the entire codebase, we found **~100 issues**:
- **50 bugs** from audit agents (3 CRITICAL, 16 HIGH, 21 MEDIUM, 10 LOW)
- **20 bugs** from original v0.51 plan (still open)
- **30+ quality gaps** (weak tests, missing events, architectural debt)

The root cause is NOT individual bugs — it's **3 systemic failures**:

1. **Errors lose context as they propagate** — `unwrap_or(0)`, `.ok()`, `_ => {}`
2. **State transitions skip events** — no compile-time guarantee of event emission
3. **Tests assert existence, not correctness** — `assert!(result.is_ok())` everywhere

---

## PART 1: ALL BUGS — Unified List (70+ items)

### Tier 0: CRITICAL (3) — Fix IMMEDIATELY

| ID | Source | Bug | File | Fix |
|----|--------|-----|------|-----|
| CR1 | Test Audit | SchemaGuardrail `.check()` only validates `required` — no type/pattern/enum checking | `nika-core/src/ast/guardrails.rs:332` | Use `jsonschema` crate for full validation |
| CR2 | Test Audit | Agent status tests are tautologies (test PartialEq derive, not behavior) | `nika-engine/src/runtime/rig_agent_loop/tests.rs:12-30` | Replace with integration tests that run the agent loop |
| CR3 | Test Audit | Agent result test only checks `Debug` format, never runs the agent | Same file, line 21 | Write real execution tests |

### Tier 1: HIGH — Security (6)

| ID | Source | Bug | File | Fix |
|----|--------|-----|------|-----|
| S1 | Security | `python3 -c` only blocks `import socket` — `import os` passes | `security.rs:56-57` | Block `python -c`, `python3 -c`, `python2 -c` generically |
| S2 | Security | `bash -c`, `zsh -c`, `sh -c` not blocked at all | `security.rs` blocklist | Add all shell `-c` variants |
| S3 | Security | DNS rebinding TOCTOU: check resolves safe, connect resolves private | `policy.rs:76-114` | Custom `reqwest::dns::Resolve` that checks IPs at connect time |
| S4 | Security | SSRF redirect targets not DNS-checked (string check only) | `executor/mod.rs:128-151` | DNS resolve in redirect policy |
| S5 | Security | Template `resolve` Pass 3 has no `trusted_inputs` allowlist | `binding/template.rs:1177` | Add `trusted_inputs` HashSet like Pass 2's `trusted_context` |
| S6 | Security | `resolve_with` lacks `trusted_context` allowlist (vs `resolve`) | `binding/template.rs:494` | Port allowlist from `resolve` |

### Tier 2: HIGH — Silent Failures (10)

| ID | Source | Bug | File | Fix |
|----|--------|-----|------|-----|
| SF1 | Silent | SSRF DNS failure/timeout defaults to ALLOW | `policy.rs:105-112` | Default to BLOCK (fail-closed) |
| SF2 | Silent | Missing ProviderResponded event on Layer 0a no-spec path | `executor/infer.rs:523-538` | Add event emission before return |
| SF3 | Silent | for_each binding failures: TaskResult::failed() but NO TaskFailed event | `runner.rs:1800-1809` | Emit TaskFailed event |
| SF4 | Silent | for_each "items could not be resolved" — no TaskFailed event | `runner.rs:2246-2261` | Emit TaskFailed event |
| SF5 | Silent | `jsonschema::validator_for(schema).ok()` silently disables validation | `runner.rs:656` | Return error if schema is invalid |
| SF6 | Silent | EventLog silently drops trace writes with `let _ =` | `nika-event/src/log.rs:1042` | Log at `warn!` on failure |
| SF7 | Silent | Daemon job state updates silently dropped | `nika-daemon/src/services/jobs.rs:215-241` | Log failures |
| SF8 | Silent | `debug!` used for errors that should be `warn!` | Multiple files | Upgrade log levels |
| SF9 | Original | `token_budget` on agent verb NEVER enforced | `rig_agent_loop/mod.rs` | Wire into LimitTracker (needs H14 refactor) |
| SF10 | Original | `extended_thinking` agent = single-turn, no tools | `rig_agent_loop/thinking.rs` | Validate at analyzer or integrate into main loop |

### Tier 3: HIGH — Architecture Debt (3)

| ID | Source | Bug | File | Fix |
|----|--------|-----|------|-----|
| AD1 | Original | 1505 LOC duplicated across 3 agent provider loops | `providers.rs` | Extract `run_agent_loop<C>` generic |
| AD2 | Test Audit | Zero unit tests for token_budget/limits/max_cost in agent loop | `rig_agent_loop/tests.rs` | Add tests after AD1 refactor |
| AD3 | Test Audit | 4 extended_thinking tests only check constructor, never `.run()` | Same file | Write execution tests |

### Tier 4: MEDIUM (25+)

| ID | Bug | Quick description |
|----|-----|-------------------|
| M-tok1 | Token counts = 0 when `Final` stream event missing | Fallback estimation |
| M-tok2 | Native vision returns 0 tokens (StreamResult default) | Add estimation |
| M-tok3 | Layer 0b uses estimated tokens instead of actual | Use provider response |
| M-tok4 | Vision always uses heuristic tokens | Return StreamResult from vision |
| M-sec1 | `xargs`, `find -exec`, `awk system()` not blocked | Add to blocklist |
| M-sec2 | Symlinks in artifact dir escape boundary | Canonicalize after mkdir |
| M-sec3 | Traces persist forever, no rotation | Add TTL cleanup |
| M-sec4 | `redact_for_event` doesn't redact `sk-*` patterns | Add pattern matching |
| M-tst1 | 3 test files manipulate env vars without `#[serial]` | Add `#[serial]` |
| M-tst2 | 232 instances of `assert!(result.is_ok())` without value check | Strengthen |
| M-tst3 | Daemon tests use `sleep(100ms)` for startup — flaky | Readiness signal |
| M-tst4 | `test_invoke_tool_with_template_params` never checks resolved params | Assert values |
| M-orig1 | `for_each` ordering with `concurrency: 1` | Verify or fix |
| M-orig2 | `routing:` parsed but dead code | Remove or implement |
| M-orig3 | `manifest: true` never writes artifacts.json | Implement |
| M-orig4 | `fetch:` short form rejected by JSON schema | Fix schema |
| M-orig5 | `format: markdown` rejected by schema | Fix enum |
| M-orig6 | `{{for_each.index}}` unavailable in artifact paths | Inject variable |
| M-orig7 | `extract: llm_txt` returns raw HTML fallback | Return error |
| M-orig8 | Temperature not validated per-provider | Add validation |
| M-orig9 | Schema guardrail: only checks required (= CR1 fix) | Full JSON Schema |

### Tier 5: LOW (15+)

`join()` pipe `|` escape, `compact` empty strings, `round` type mismatch, vision TTFT null,
summary box width, stale comments, error code doc mismatches, tautological tests, trivial derive tests,
cache sleep fragility, for_each skipped iterations no event.

---

## PART 2: THE REFACTOR — Agent Provider Loop Unification

### Problem

`providers.rs` = 1505 LOC with 3 identical ~420-line methods:

```
run_claude  (110-527)  = client + stream + retry + guardrails + limits + events
run_openai  (528-941)  = client + stream + retry + guardrails + limits + events
run_generic (1080-1505) = client + stream + retry + guardrails + limits + events
```

The ONLY differences:
- Line 1: `anthropic::Client::from_env()` vs `openai::Client::from_env()`
- Hardcoded `ProviderKind::Claude` vs `ProviderKind::OpenAI`
- `run_claude` has extended_thinking shortcut

### Solution

```rust
/// One generic method to rule them all
async fn run_agent_loop<C: CompletionClient>(
    &mut self,
    client: C,
    model_name: &str,
    provider_kind: Option<ProviderKind>,
) -> Result<RigAgentLoopResult, NikaError>
where
    C::CompletionModel: Clone + 'static,
{
    // Wire token_budget into LimitTracker (fixes SF9)
    if let Some(budget) = self.params.token_budget {
        self.limit_tracker.set_token_limit(budget as u64);
    }

    // Extended thinking: mode flag, not separate method (fixes SF10)
    let use_thinking = self.params.extended_thinking == Some(true)
        && provider_kind == Some(ProviderKind::Claude);

    // === THE LOOP (written ONCE) ===
    let model = client.completion_model(model_name);
    let tools = self.tools_as_boxed();
    let max_turns = self.params.max_turns.unwrap_or(10) as usize;

    // First attempt
    let mut result = if use_thinking {
        self.stream_with_thinking(model.clone(), &prompt).await?
    } else {
        self.stream_with_tools(model.clone(), &prompt, tools, max_turns).await?
    };

    // Record turn + check limits (ONE place, not THREE)
    let cost = calculate_cost_with_cache(provider_kind, model_name, ...);
    self.limit_tracker.record_turn(result.input_tokens, result.output_tokens, cost);

    if let Some(exceeded) = self.limit_tracker.check_limits() {
        return self.handle_limit_exceeded(exceeded, &result);
    }

    // Confidence retry loop (ONE implementation)
    let mut retry_count = 0;
    while self.should_retry(&status, retry_count) {
        // ... retry logic ...
    }

    // Guardrails (ONE check)
    let guardrail_result = self.check_guardrails(&result.response);

    // Build result (ONE builder)
    self.build_result(result, guardrail_result, retry_count)
}
```

### Callers become 5-line wrappers

```rust
pub async fn run_claude(&mut self) -> Result<RigAgentLoopResult, NikaError> {
    let client = anthropic::Client::from_env();
    let model = self.params.model.clone().ok_or(/* ... */)?;
    self.run_agent_loop(client, &model, Some(ProviderKind::Claude)).await
}

pub async fn run_openai(&mut self) -> Result<RigAgentLoopResult, NikaError> {
    let client = openai::Client::from_env();
    let model = self.params.model.clone().ok_or(/* ... */)?;
    self.run_agent_loop(client, &model, Some(ProviderKind::OpenAI)).await
}
// run_mistral, run_groq, run_deepseek, run_gemini, run_xai = same pattern
```

### Impact: 1505 LOC → ~600 LOC, fixes SF9 + SF10 + AD1 + AD2

---

## PART 3: QUALITY INFRASTRUCTURE

### 3.1 Crates — Complete Arsenal (30+ crates, 5 tiers)

#### Tier P0: ADOPT NOW (highest impact, low effort)

| Crate | Purpose | Downloads | Impact |
|-------|---------|-----------|--------|
| `tracing-error` | SpanTrace in every error — automatic execution context | 40M | Every error tells you WHERE it happened |
| `nutype` | Validated newtypes: `TokenCount(0)` = impossible | 3.3M | Eliminates silent zeros at type level |
| `proptest` | Property-based testing for transforms, DAG, cost | 107M | Finds edge cases unit tests miss |
| `cargo-mutants` | Mutation testing — finds weak tests | 268K | Reveals 232+ weak `is_ok()` assertions |
| `static_assertions` | Compile-time size/type/const checks | 93M | Catch struct layout changes at compile time |

#### Tier P1: ADOPT NEXT SPRINT

| Crate | Purpose | Downloads | Impact |
|-------|---------|-----------|--------|
| `error-stack` | Context accumulation — never lose error chain | 3.1M | Rich error reports with metadata |
| `cargo-deny` | License + advisory + duplicate dep checking | 31M | AGPL compliance + CVE detection |
| `cargo-audit` | Known vulnerability scanning | 14M | Security gate in CI |
| `cargo-semver-checks` | API semver violation detection | 3.8M | Pre-publish safety |
| `rstest` | Parametric tests + fixtures | 12M | Reduce test boilerplate, test matrices |
| `strum` | Enum utilities: iteration, string conversion | 47M | Replace stringly-typed provider/verb matching |
| `derive_more` | Auto-derive From, Display, Error | 137M | Less boilerplate = fewer conversion bugs |
| `wiremock` | HTTP mock server for fetch: tests | 9M | Test fetch/extract without real HTTP |

#### Tier P2: ADOPT FOR ARCHITECTURE IMPROVEMENTS

| Crate | Purpose | Downloads | Impact |
|-------|---------|-----------|--------|
| `statig` | Hierarchical state machines | 3.4M | Compile-time task lifecycle enforcement |
| `bon` | Compile-time checked builders | 26M | Prevent missing fields in complex structs |
| `enum-map` | Type-safe enum → value mapping | 18M | Replace HashMap<String, _> for providers |
| `console-subscriber` | Tokio Console — async task inspection | 32M | Debug hangs and task starvation |
| `tower` | Retry/timeout/rate-limit middleware | 97M | Structured resilience for provider calls |
| `tower-retry` + `backoff` | Exponential backoff with jitter | 44M | Replace hand-rolled retry loops |

#### Tier P3: ADOPT FOR TESTING DEPTH

| Crate | Purpose | Downloads | Impact |
|-------|---------|-----------|--------|
| `insta` | Snapshot testing (already in use — verify coverage) | 25M | Regression detection for complex outputs |
| `bolero` | Coverage-guided fuzzing | 1.2M | Find crashes in parser/template engine |
| `fake` | Realistic fake data generation | 6M | Better test fixtures |
| `test-strategy` | Async proptest support | 2M | Property-based testing with Tokio |
| `cargo-careful` | Extra UB checks (Miri-lite) | 500K | Catch undefined behavior in media/CAS |
| `cargo-fuzz` | LibFuzzer integration | 2M | Fuzz YAML parser + template parser |
| `serial_test` | `#[serial]` for env-var tests | 16M | Fix 3 race-condition test files |

#### Tier POST-MVP: PRODUCTION OBSERVABILITY

| Crate | Purpose | Downloads | Impact |
|-------|---------|-----------|--------|
| `tracing-opentelemetry` | Export spans to Jaeger/Tempo | 130M | Distributed tracing for workflows |
| `metrics` + `metrics-exporter-prometheus` | Counters/histograms/gauges | 74M | nika_tokens_total, nika_cost_usd |
| `tracing-forest` | Group concurrent span output by tree | 1.8M | Readable for_each logs |
| `tracing-flame` | Generate flamegraphs from tracing | 3M | Performance bottleneck analysis |
| `tokio-metrics` | Runtime metrics (task counts, poll times) | 5M | Async health monitoring |

### 3.2 Patterns to Implement (P0)

**TaskEventGuard** — guaranteed event emission:
```rust
struct TaskEventGuard { task_id: String, emitter: Arc<EventLog>, completed: bool }
impl Drop for TaskEventGuard {
    fn drop(&mut self) {
        if !self.completed {
            self.emitter.emit(TaskFailed { ... }); // GUARANTEED on any exit path
        }
    }
}
```

**Parse, Don't Validate** — eliminate silent zeros:
```rust
// Raw from provider (maybe zero)
struct RawProviderResponse { input_tokens: u64, output_tokens: u64 }

// Validated (guaranteed non-zero for success)
struct ValidatedUsage { input: NonZeroU64, output: NonZeroU64 }

// The ONLY way to create ValidatedUsage is from_raw(), which logs on zero
impl ValidatedUsage {
    fn from_raw(raw: RawProviderResponse) -> Result<Self, TokenEstimate> { ... }
}
```

### 3.3 Clippy Strict Mode

```toml
# workspace Cargo.toml
[workspace.lints.clippy]
unwrap_used = "deny"
wildcard_enum_match_arm = "warn"
manual_assert = "warn"

[workspace.lints.rust]
unsafe_code = "deny"
```

### 3.4 CI Additions

```bash
# Mutation testing (weekly)
cargo mutants -p nika-engine --in-diff HEAD~10 -- --lib

# Silent failure grep (every PR)
! grep -rn 'unwrap_or(0)' tools/nika-engine/src/ --include='*.rs' | grep -v test
! grep -rn 'unwrap_or_default()' tools/nika-engine/src/ --include='*.rs' | grep -v test

# Property tests
cargo test --workspace --lib -- proptest
```

---

## PART 4: RULES (add to CLAUDE.md / developer guide)

### Rule 1: No Silent Zeros
Every `unwrap_or(0)` or `unwrap_or_default()` must be replaced with explicit logging.

### Rule 2: No Catch-All Swallows
Every `_ => {}` in a match must at minimum `tracing::warn!()`.

### Rule 3: Every State Transition = Event
Use `TaskEventGuard` pattern. Drop without completion = TaskFailed.

### Rule 4: Tests Assert Values, Not Just Ok
`assert!(result.is_ok())` is BANNED alone. Must verify the actual value.

### Rule 5: Every Fix = Regression Test (TDD)
Test must FAIL before fix, PASS after.

### Rule 6: No Duplication > 10 Lines
Extract or die. Use generics.

### Rule 7: Errors Include Full Context
Use `tracing-error` SpanTrace. Every error carries task_id + workflow + verb.

### Rule 8: Fail-Closed Security
DNS failure = BLOCK. Schema invalid = ERROR. Unknown variant = WARN + explicit handling.

### Rule 9: Never Mark Bugs as "Investigated"
A bug is DONE only with code + test. "Deferred" = "I failed" — say it honestly.

---

## PART 5: EXECUTION PLAN

### Session A: Security Hardening (~2h)
1. S1+S2: Block `python3 -c`, `bash -c`, `zsh -c` generically
2. SF1: DNS failure → BLOCK (fail-closed)
3. S5+S6: Template injection — `trusted_inputs` + `resolve_with` allowlist
4. S3+S4: Custom reqwest DNS Resolve for SSRF (investigate complexity)

### Session B: The Big Refactor (~4h)
1. AD1: Extract `run_agent_loop<C>` (1505 → 600 LOC)
2. SF9: Wire `token_budget` into LimitTracker
3. SF10: Integrate extended_thinking into main loop
4. AD2+AD3: Write real agent execution tests

### Session C: Silent Failure Sweep (~3h)
1. CR1: SchemaGuardrail full JSON Schema validation
2. SF2-SF4: Add missing events (ProviderResponded, TaskFailed for for_each)
3. SF5: Error on invalid JSON Schema (not silent .ok())
4. SF6-SF8: Fix log levels and silent drops
5. TaskEventGuard pattern implementation

### Session D: Quality Infrastructure (~2h)
1. Add `tracing-error`, `nutype`, `proptest`, `cargo-mutants` to deps
2. Run cargo-mutants on cost.rs, security.rs, transform.rs
3. Write proptest strategies for templates + pipe transforms
4. Fix all surviving mutants

### Session E: Test Strengthening (~3h)
1. CR2+CR3: Replace tautological agent tests
2. Fix 232 bare `assert!(result.is_ok())` (top 50 by impact)
3. Add `#[serial]` to env-var tests
4. Add readiness signals to daemon tests
5. Remaining M-orig bugs (schema, manifest, for_each.index, etc.)

### Session F: Polish (~1h)
1. All Tier 5 LOW bugs
2. Strict clippy lint activation
3. CI pipeline additions
4. Documentation updates

---

## VERIFICATION CHECKLIST

Before declaring v0.51 quality-complete:
- [ ] Zero surviving mutants in cost.rs + security.rs + transform.rs
- [ ] Zero `unwrap_or(0)` outside test code
- [ ] Zero `_ => {}` without logging
- [ ] Every task state transition emits an event (TaskEventGuard)
- [ ] `cargo test --workspace --lib` = 0 failures (expect 9000+)
- [ ] `cargo clippy --workspace -- -D warnings` = 0 warnings
- [ ] All 70+ bugs have commits or proven-not-a-bug (with test)
- [ ] tracing-error integrated (SpanTrace on all NikaError)
- [ ] proptest for transforms + DAG + cost
- [ ] Security: python3 -c blocked, DNS fail-closed, SSRF redirect checked

---

---

## PART 6: TELEMETRY AUDIT FINDINGS (Agent 6)

55 EventKind variants found. Only ~25 tested. 30 emitted in production with ZERO tests.

### HIGH — Wrong/Missing Event Data

| ID | Bug | File | Fix |
|----|-----|------|-----|
| EV1 | `ContextAssembled` always reports `budget_used_pct: 0.0`, `truncated: false`, `excluded: []` — all hardcoded | `infer.rs:142-149` | Populate fields or remove them |
| EV2 | Chat path (`chat.rs`) NEVER emits `ProviderResponded` — tokens/cost invisible | `chat.rs` (entire file) | Add ProviderResponded after each chat turn |
| EV3 | `PolicyBlocked` event = ZERO tests (security-critical) | `fetch.rs:146`, `exec.rs:45` | Add test that blocked commands emit PolicyBlocked |
| EV4 | `FallbackTriggered` event = ZERO tests (routing feature) | `executor/mod.rs:479` | Add test for provider fallback |
| EV5 | MCP disconnect/reconnect = NO events at all | `client.rs:758,798` | Add McpDisconnected, McpReconnected events |

### MEDIUM — Missing Event Tests (28 variants)

`ArtifactWritten`, `ArtifactFailed`, `MediaExtracted`, `MediaProcessed`, `MediaStored`,
`MediaStoreFailed`, `VisionContentResolved`, `ExecCompleted`, `FetchRetry`,
`BindingDefaultApplied`, `BindingTransformApplied`, `BindingEnvResolved`,
`ForEachStarted`, `ForEachCompleted`, `ExtractApplied`, `DecomposeStarted`,
`DecomposeCompleted`, `ProviderInitialized`, `StreamingDelta`, `AgentStart`,
`AgentTurn`, `AgentComplete`, `BootPhaseCompleted`, `NativeModelLoaded`,
`TaskScheduled`, `GuardrailPassed`, `MediaIntegrityCheck`, `ContextAssembled`

Each needs at minimum one test verifying:
1. The event IS emitted at the right time
2. The event data (task_id, tokens, cost, duration) is CORRECT

### MEDIUM — Estimated Tokens in Events

| ID | Bug | File |
|----|-----|------|
| EV6 | Structured output retry reports chars/4 estimate, not actual | `structured_output.rs:526` |
| EV7 | Structured output repair reports chars/4 estimate, not actual | `structured_output.rs:690` |
| EV8 | Non-streaming fallback reports estimated tokens | `infer.rs:638,707` |

### Missing EventKind Variants (should be added)

| Event | Where it should fire | Why |
|-------|---------------------|-----|
| `McpDisconnected` | `client.rs:758` disconnect() | Pool/TUI need to know server died |
| `McpReconnected` | `client.rs:798` reconnect() | Track server recovery |
| `ProviderRateLimited` | `provider/rig.rs:1102` on 429 | Separate from TaskFailed for observability |

### Doc Staleness

`log.rs:5` says "44 variants across 15 categories" — actual: **55 variants across 18 categories**.

---

*This plan incorporates findings from: 1 silent-failure agent (16 bugs), 1 security agent (10 bugs),
1 test-quality agent (24 findings), 1 telemetry agent (40+ findings), 3 research agents (30+ crate recommendations),
plus 20 bugs from original v0.51 plan. Architecture agent findings pending.*
