# Nika Master Plan — Production Readiness

> Generated: 2026-03-30 | Base: v0.52.0 | 8,914 lib tests | 35/40 E2E pass
> Source: 4 deep audit agents (structured output, routing, untested features, error handling)

---

## TL;DR — The 3 Real Blockers

1. **Model routing is broken** — gpt-4o hardcoded as default, model name doesn't follow fallback chain, cost tracking reports wrong model
2. **10 features implemented but never tested end-to-end** — P-ORCHESTRATE, P-RECORD, vision, include, 8/9 fetch extract modes, custom endpoints, artifact writing, guardrail enforcement, retry execution, context budget enforcement
3. **3 panics on user input in production code** — runner.rs crashes on malformed JSON instead of returning errors

---

## SECTION A: BUGS (fix immediately)

### A1. CRITICAL — Panics on User Input

| # | File:Line | Code | Fix |
|---|-----------|------|-----|
| A1.1 | `runner.rs:3971` | `unwrap_or_else(\|_\| panic!("Should be valid JSON array: {output}"))` | Return `NikaError::ExecutionError` |
| A1.2 | `runner.rs:5210` | `panic!("Output should be JSON array, got: {}", output)` | Return `NikaError` |
| A1.3 | `runner.rs:5394` | `panic!("Should be JSON array, got: {}", output)` | Return `NikaError` |
| A1.4 | `nika-cli/src/machine/install.rs:947` | `panic!("line {} has bare {{item}} outside mistakes table")` | Return `Result<_, InstallError>` |
| A1.5 | `tools/context.rs:477,500,563` | `env::current_dir().unwrap()` | `.map_err(NikaError::from)?` |

**Effort:** 1h. **Impact:** Eliminates all panics on user input.

### A2. HIGH — Model Routing Mismatch

| # | Issue | File | Fix |
|---|-------|------|-----|
| A2.1 | gpt-4o hardcoded as OpenAI default | `provider/rig/mod.rs:432` | Use model catalog default, not hardcoded constant |
| A2.2 | TUI hardcodes model on provider switch | `tui/app/mod.rs:617`, `routing.rs:312-316`, `lifecycle.rs:67`, `chat_overlay.rs:86,762` | Use `ProviderName::default_model()` from catalog |
| A2.3 | Fallback chain doesn't change model | `executor/infer.rs:162-229` | Resolve model per-provider in fallback chain |
| A2.4 | "unknown" model in structured output | `executor/infer.rs:1021` | Use `provider.default_model()` |
| A2.5 | Cost tracking uses wrong model | `executor/infer.rs:561,629,741,810,966,1311` | Capture actual model from rig-core response |
| A2.6 | ProviderCalled event wrong model | `executor/infer.rs:361-366` | Emit model AFTER rig-core resolves it |
| A2.7 | Prefix stripping inconsistent | `rig_agent_loop/mod.rs:173` vs `executor/infer.rs` | Centralize in one function used everywhere |
| A2.8 | No model validation before API call | `executor/infer.rs` | Validate against `ModelCatalog` before calling |
| A2.9 | Silent fallback to $5/$15 default pricing | `provider/cost.rs:616` | Log warning for unknown models |

**Effort:** 4h. **Impact:** Correct model routing, accurate cost tracking, predictable behavior.

**Root cause:** Model resolution is scattered across 6+ files with no single source of truth. Fix: create `ModelResolver::resolve(provider, model, workflow_default) -> ResolvedModel` used everywhere.

### A3. MEDIUM — Error Handling Gaps

| # | Issue | File | Fix |
|---|-------|------|-----|
| A3.1 | Grep/glob tools no result limit | `tools/grep.rs:183` | Add `MAX_MATCHES = 10_000` constant |
| A3.2 | File I/O without timeout | Various | Wrap with `tokio::time::timeout(30s, ...)` |
| A3.3 | AgentParams.scope parsed but ignored | `rig_agent_loop/mod.rs:285` | Remove from API or implement |

---

## SECTION B: UNTESTED FEATURES (test or remove)

### Status Matrix

| Feature | Parsing | Unit Tests | E2E Execution | Real API | Status |
|---------|---------|------------|---------------|----------|--------|
| **infer** | YES | YES | YES | YES (6/7) | **PROD READY** |
| **exec** | YES | YES | YES | N/A | **PROD READY** |
| **fetch** (markdown) | YES | YES | YES | YES | **PROD READY** |
| **fetch** (8 other modes) | YES | NO | NO | NO | **NOT TESTED** |
| **invoke** (builtin) | YES | YES | YES | N/A | **PROD READY** |
| **invoke** (MCP) | YES | YES | YES (mock) | Partial | **BETA** |
| **agent** (multi-turn) | YES | YES | YES | YES (Claude) | **BETA** |
| **structured output** | YES | YES | YES | YES (6/7) | **PROD READY** |
| **for_each** | YES | YES | YES | YES | **PROD READY** |
| **depends_on / DAG** | YES | YES | YES | YES | **PROD READY** |
| **bindings / transforms** | YES | YES | YES | YES | **PROD READY** |
| **retry** | YES | Parsing only | NO | NO | **NOT TESTED** |
| **provider fallback** | YES | YES | NO | NO | **NOT TESTED** |
| **P-ORCHESTRATE** | YES | Parsing only | NO | NO | **NOT TESTED** |
| **P-RECORD** | YES | Partial | NO | NO | **NOT TESTED** |
| **P-CONTEXT** | YES | TUI only | NO | NO | **NOT TESTED** |
| **agent guardrails** | YES | Parsing only | NO | NO | **NOT TESTED** |
| **vision / multimodal** | YES | NO | NO | NO | **NOT TESTED** |
| **custom endpoints** | YES | NO | NO | NO | **NOT TESTED** |
| **artifact writing** | YES | Unit (mock) | NO | NO | **NOT TESTED** |
| **include workflows** | YES | NO | NO | NO | **NOT TESTED** |
| **extended thinking** | YES | YES | YES | YES | **PROD READY** |
| **daemon auto-start** | YES | YES | YES | YES | **PROD READY** |

### B1. Must-Test Before Any Release Claim

| # | Feature | Test Plan | Effort |
|---|---------|-----------|--------|
| B1.1 | **Retry with backoff** | Create workflow that fails on first attempt (use mock server returning 429), succeeds on second. Verify delay, backoff multiplier, max_attempts. | 2h |
| B1.2 | **Provider fallback chain** | `provider: [groq, openai]` with invalid Groq key. Verify fallback to OpenAI. Check ProviderFallback event. | 1h |
| B1.3 | **Fetch 8 extract modes** | Test article, text, selector, metadata, links, jsonpath, feed, llm_txt on real URLs. | 3h |
| B1.4 | **Agent guardrails** | Create agent with length guardrail (max 50 words). Verify NIKA-112 on violation. Test retry vs fail. | 2h |
| B1.5 | **Artifact writing** | Workflow with `artifact: { path: "output.json" }`. Verify file exists on disk with correct content. | 1h |

### B2. Should-Test (v0.53 release)

| # | Feature | Test Plan | Effort |
|---|---------|-----------|--------|
| B2.1 | **P-ORCHESTRATE** | Goal-driven workflow with `goal:` + `orchestrate:`. Verify agent wrapper, rounds, completion. | 4h |
| B2.2 | **P-RECORD** | Workflow with `record: { compress: true }`. Verify NDJSON persistence. | 2h |
| B2.3 | **P-CONTEXT** | Workflow with `context_budget: 1000`. Verify truncation on large bindings. | 2h |
| B2.4 | **Vision / multimodal** | Send image to Claude/OpenAI via `content: [{type: image}]`. Verify description. | 2h |
| B2.5 | **Custom endpoints** | Configure Ollama/vLLM endpoint. Run infer. | 1h |
| B2.6 | **Include workflows** | Create main + partial workflow. Verify tasks merge correctly. | 1h |

### B3. Run the 502 Example Workflows

```bash
# Automated execution plan
for f in examples/gates/feature/*.nika.yaml; do
  nika run "$f" --provider mock --no-live 2>&1 | tail -1
done | tee /tmp/gate-results.txt

grep -c "DONE" /tmp/gate-results.txt    # Count successes
grep -c "error" /tmp/gate-results.txt   # Count failures
```

Expected: ~60% pass rate initially. Each failure is a potential parser/engine bug.

---

## SECTION C: STRUCTURED OUTPUT — Honest Assessment

### What Actually Happens (Verified by Code Audit)

Layer 0 is **REAL native structured output**, NOT prompt engineering:

| Provider | Method | How |
|----------|--------|-----|
| OpenAI, Groq, DeepSeek, xAI | **Layer 0a**: `response_format: { type: "json_schema" }` | Native API parameter |
| Anthropic, Mistral, Gemini | **Layer 0b**: `DynamicSubmitTool` + `tool_choice: Required` | Native tool_use at API level |

The "CRITICAL OUTPUT REQUIREMENT" text visible in CLI output is the **display** of what was sent to the provider, not prompt injection. The schema goes through rig-core's `additional_params` (L0a) or `tools` + `tool_choice` (L0b).

### Where It Breaks

| # | Issue | Impact | Fix |
|---|-------|--------|-----|
| C1 | L0a (response_format) only works for OpenAI-compat providers | Anthropic/Mistral/Gemini use L0b which is less reliable | Implement Anthropic native tool_result extraction |
| C2 | L0b display shows raw schema in CLI output | Looks like prompt engineering (confusing) | Clean display — hide schema from CLI output |
| C3 | Error from L0a/L0b not propagated to L2+ | If L0 silently produces invalid JSON, L2 catches it but doesn't know WHY | Chain error context between layers |
| C4 | Mock provider doesn't support structured output | Can't test structured output without real API | Implement mock structured response |
| C5 | `repair_model` fallback not tested | L4 repair with cheap model never tested E2E | Test with intentionally broken L2 response |

### Confidence Level

- **Simple schemas (3-5 fields):** HIGH confidence. Works on 6/7 providers.
- **Complex schemas (nested, enums, arrays):** MEDIUM confidence. E2E tested but not stress-tested.
- **Edge cases (10-level nesting, contradictory constraints):** LOW confidence. Only mock parse tests.

---

## SECTION D: ARCHITECTURE DEBT

### D1. Model Resolution Needs Centralization

**Problem:** Model name resolved in 6+ places with inconsistent logic.

**Current flow:**
```
Task YAML → AnalyzedTask.model (Option<String>)
  → executor resolves: task model || workflow model || provider default
  → rig-core receives model
  → cost tracking uses: model || provider.default_model() || "unknown"
  → events report: model || provider.default_model()
```

**Fix:** Create `ModelResolver` struct:
```rust
pub struct ResolvedModel {
    pub name: String,        // The actual model name sent to API
    pub provider: ProviderName,
    pub source: ModelSource, // Task, Workflow, ProviderDefault, Fallback
}

impl ModelResolver {
    pub fn resolve(
        task_model: Option<&str>,
        workflow_model: Option<&str>,
        provider: ProviderName,
    ) -> ResolvedModel;
}
```

Use in ALL paths: executor, cost, events, display.

### D2. Provider-Model Compatibility Validation

**Problem:** No validation that a model belongs to the requested provider.

`provider: anthropic, model: gpt-4o` → sent to Anthropic API → 400 error.
`provider: [groq, openai], model: llama-3.3-70b` → fallback to OpenAI with Groq model → 404.

**Fix:** `ModelCatalog::is_compatible(provider, model) -> bool` check before API call. Return `NIKA-033 IncompatibleModel` early.

### D3. Feature Flags for Unfinished Features

**Problem:** P-ORCHESTRATE, P-RECORD, P-CONTEXT are parseable and appear to work, but are minimally tested. Users might rely on them and hit bugs.

**Options:**
1. **Remove from parser** until tested → breaking change for anyone using them
2. **Add warning on use** → `tracing::warn!("P-ORCHESTRATE is experimental")`
3. **Gate behind feature flag** → `#[cfg(feature = "experimental")]`

**Recommendation:** Option 2 — warn on use. Least disruptive.

### D4. Structured Output Mock Provider

**Problem:** `provider: mock` returns deterministic text, not JSON. Can't test structured output without real API keys.

**Fix:** When a task has `structured:` spec, mock provider should return valid JSON matching the schema. Generate from schema: `{ "name": "mock_string", "age": 0, "skills": ["mock"] }`.

---

## SECTION E: RESEARCH NEEDED

| # | Question | Why It Matters | How to Answer |
|---|----------|---------------|---------------|
| E1 | Does rig-core's `tool_choice: Required` actually force tool use on all providers? | If some providers ignore it, L0b fails silently | Test with Mistral, Gemini — check if tool_call is in response |
| E2 | What happens when rig-core receives an unsupported model name? | Silent fallback? Error? Hang? | Test with `model: "nonexistent-model-v99"` |
| E3 | Does `response_format: json_schema` work on Groq with Llama models? | Groq may not support structured output for open models | Test with real Groq API |
| E4 | Can agents spawn sub-agents that spawn sub-sub-agents? | depth_limit exists but never tested beyond depth 2 | Test with `depth_limit: 3` |
| E5 | What's the actual memory footprint of a 50-task workflow? | No profiling has been done | Run `cargo bench` or `valgrind` |
| E6 | How does Nika handle provider rate limits in for_each? | `concurrency: 10` might hit 429 on all 10 simultaneously | Test with Groq (strict rate limits) |

---

## SECTION F: EXECUTION PLAN

### Sprint 1: "Stop Crashing" (1 day)

```
[ ] A1: Fix 5 panics on user input (1h)
[ ] A2.1-A2.2: Fix gpt-4o hardcoded default + TUI models (2h)
[ ] A2.4: Fix "unknown" model in structured output (15min)
[ ] D4: Mock provider returns valid JSON for structured output (2h)
[ ] Run full test suite, verify 0 panics (30min)
```

**Commit pattern:** 1 fix = 1 commit. Test before commit.

### Sprint 2: "Model Routing" (1 day)

```
[ ] D1: Create ModelResolver centralized resolution (3h)
[ ] A2.3: Fallback chain resolves model per-provider (1h)
[ ] A2.5-A2.6: Cost + events use actual resolved model (1h)
[ ] D2: Add model-provider compatibility validation (1h)
[ ] A2.9: Warn on unknown model pricing (30min)
```

### Sprint 3: "Test What We Ship" (2 days)

```
[ ] B1.1: Retry with backoff E2E test (2h)
[ ] B1.2: Provider fallback chain E2E test (1h)
[ ] B1.3: Fetch 8 extract modes E2E test (3h)
[ ] B1.4: Agent guardrails E2E test (2h)
[ ] B1.5: Artifact writing E2E test (1h)
[ ] B3: Run 502 example workflows (4h)
[ ] E1-E3: Research questions (2h)
```

### Sprint 4: "Experimental Features" (2 days)

```
[ ] B2.1: P-ORCHESTRATE E2E test (4h)
[ ] B2.2: P-RECORD E2E test (2h)
[ ] B2.3: P-CONTEXT E2E test (2h)
[ ] B2.4: Vision E2E test (2h)
[ ] B2.5: Custom endpoints test (1h)
[ ] B2.6: Include workflows test (1h)
[ ] D3: Add experimental warnings (1h)
```

### Sprint 5: "Production Polish" (1 day)

```
[ ] A3.1: Grep/glob result limits (1h)
[ ] A3.2: File I/O timeouts (2h)
[ ] C2: Clean structured output CLI display (1h)
[ ] C4: Mock structured output support (done in Sprint 1)
[ ] Version bump to v0.53.0 (1h)
```

---

## METRICS TARGET

| Metric | Current (v0.52) | Target (v0.53) |
|--------|-----------------|----------------|
| Panics on user input | 5 | **0** |
| Model routing correct | NO | **YES** |
| Features tested E2E | 11/22 | **18/22** |
| Providers validated | 6/7 | **7/7** |
| Example workflows run | 0/502 | **400+/502** |
| Fetch extract modes tested | 1/9 | **9/9** |
| Structured output mock | NO | **YES** |
| Cost tracking accuracy | ~50% | **>95%** |

---

## CONFIDENCE ASSESSMENT

| Component | Confidence | Reason |
|-----------|-----------|--------|
| Parser (YAML → AST) | **HIGH** | 900+ unit tests, all patterns covered |
| DAG scheduling | **HIGH** | Well-tested, topological sort proven |
| Bindings / templates | **HIGH** | 38 transforms, extensive tests |
| exec verb | **HIGH** | Simple, well-tested, security blocklist proven |
| fetch verb (markdown) | **HIGH** | Works on real URLs, tested |
| fetch verb (8 others) | **LOW** | Zero E2E tests |
| infer verb (simple) | **HIGH** | 6/7 providers validated |
| infer verb (structured) | **MEDIUM** | Works but mock can't test it, L0 display confusing |
| agent verb | **MEDIUM** | Works for simple cases, multi-turn untested in depth |
| invoke verb (builtin) | **HIGH** | 30+ tools, good coverage |
| invoke verb (MCP) | **MEDIUM** | Protocol tested, real server connection partial |
| Provider routing | **LOW** | 12 issues found, model mismatch, wrong costs |
| P-ORCHESTRATE | **VERY LOW** | Zero execution tests |
| P-RECORD | **VERY LOW** | Zero execution tests |
| P-CONTEXT | **LOW** | TUI tested, enforcement untested |
| Vision | **VERY LOW** | Zero tests |
| Retry | **LOW** | Parsing only, no execution test |
| Error handling | **MEDIUM** | Good patterns but 5 panics in production |
| Daemon | **HIGH** | Well-tested, auto-start works |
| TUI | **HIGH** | 2153 tests, extensive rendering coverage |
