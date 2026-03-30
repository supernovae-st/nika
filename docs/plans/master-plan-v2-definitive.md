# Nika Master Plan v2 — Definitive (10-Agent Deep Audit)

> Generated: 2026-03-30 | Base: v0.52.0 | 8,914 tests | 10 specialized agents
> Corrections: Previous assessment had 8 errors. This version is verified against code.

---

## CORRECTIONS FROM PREVIOUS ASSESSMENT

| Previous Claim | Reality (Verified by Agent) |
|---|---|
| "Structured output is prompt engineering" | **WRONG** — L0a uses native `response_format: json_schema`, L0b uses `DynamicSubmitTool` + `tool_choice: Required`. Both are provider-API-level, NOT prompt injection. |
| "5 panics on user input" | **WRONG** — Only **1 real panic** (`infer.rs:224` empty provider chain). The others (runner.rs:3971,5210,5394, install.rs:947, context.rs:477) are all inside `#[cfg(test)]` blocks. |
| "8/9 fetch extract modes untested" | **WRONG** — All 9 modes are **fully implemented AND tested** (103 tests total). Even jsonpath has 19 tests. Only llm_txt has 0 tests but is production-ready. |
| "Vision has zero tests" | **PARTIALLY WRONG** — Vision is fully implemented with CAS→base64, SSRF protection, 7 provider support. Missing: E2E with real API call only. |
| "Artifact writing untested" | **WRONG** — 8 path tests + source binding tests + manifest tests. Missing: full workflow E2E only. |
| "Retry not tested" | **WRONG** — Retry with exponential backoff is fully implemented with `tokio::time::sleep`. Task-level retry wraps routing fallback. Missing: structured output retry has no delays between attempts. |
| "P-ORCHESTRATE zero implementation" | **WRONG** — 40% done. `wrap_as_orchestrator()` is complete with 5 tests. Missing: literally 1 line to wire it into Runner. |
| "Provider fallback untested" | **WRONG** — Routing fallback with 10 error categories is implemented. Task retry + routing fallback compose correctly. |

---

## THE REAL BLOCKERS (Verified)

### TIER 1: Must Fix (3 items, ~6h total)

#### 1. Model Routing — 12 Issues, 1 Root Cause

**Root cause:** No centralized `ModelResolver`. Model resolution scattered across 6+ files.

| # | Issue | Severity | File |
|---|-------|----------|------|
| 1 | gpt-4o hardcoded as OpenAI default | HIGH | `rig/mod.rs:432` |
| 2 | TUI hardcodes models on provider switch | HIGH | `app/mod.rs:617`, `routing.rs:312` |
| 3 | Fallback chain doesn't change model (groq→openai keeps llama model) | HIGH | `infer.rs:162-229` |
| 4 | Cost tracking uses fallback model, not actual | HIGH | `infer.rs:561,629,741,810,966` |
| 5 | "unknown" model in structured output context | MEDIUM | `infer.rs:1021` |
| 6 | ProviderCalled event reports wrong model | MEDIUM | `infer.rs:361-366` |
| 7 | strip_model_prefix only in agent, not infer | MEDIUM | `rig_agent_loop/mod.rs:173` |
| 8 | No model validation before API call | MEDIUM | `infer.rs` |
| 9 | Silent fallback to $5/$15 default pricing | HIGH | `cost.rs:616` |
| 10 | Compressor hardcodes "claude-haiku-4-5" | LOW | `runner.rs:1284` |
| 11 | Native model reports "native-model" | LOW | `rig/mod.rs:443` |
| 12 | Dual pricing catalogs (core vs engine) | LOW | architecture debt |

**Solution:** `ModelResolver` in `nika-core/src/catalogs/resolver.rs` (design complete, see agent output):
- `ResolvedModel { model_id, provider_id, source: ModelSource }`
- `ModelSource::Task | Workflow | ProviderDefault | FallbackSubstituted { original, position }`
- `validate(provider, model) -> Compatible | Incompatible | Unknown`
- Single `PROVIDER_DEFAULTS` table used everywhere
- **1 new file, 7 files modified. Zero new dependencies.**

**Effort:** 4h

#### 2. Structured Output Layer 0 — Missing Context in Safety-Net

| # | Issue | Severity | File |
|---|-------|----------|------|
| 1 | L0 safety-net engine missing `with_original_prompt()` | HIGH | `infer.rs:557-561, 737-741` |
| 2 | L0 safety-net engine missing `with_provider_context()` | MEDIUM | same |
| 3 | L0 safety-net engine missing `with_repair_callback()` | MEDIUM | same |
| 4 | No aggregate timeout on `engine.validate()` (worst case 20 min) | MEDIUM | `structured_output.rs` |
| 5 | Abandoned stream channels (64-slot buffer never read) | LOW | `infer.rs:541, 907` |
| 6 | No ProviderCalled event for L0b tool injection | LOW | `infer.rs:718` |
| 7 | Structured output retry has NO delays between attempts | MEDIUM | `runner.rs:642-816` |

**Comparison:** Streaming-path engine (lines 1041-1048) correctly wires all three. L0 engines don't.

**Fix:** Wire `with_original_prompt()`, `with_provider_context()`, `with_repair_callback()` to L0 engines. Add `tokio::time::timeout(600s)` around `engine.validate()`. Add 200ms delay in structured output retry loop.

**Effort:** 1.5h

#### 3. Output Scanner — Dead Security Code

**File:** `nika-engine/src/runtime/output_scanner.rs`

The LLM output injection scanner (`scan_output()`, `sanitize_output()`) is **implemented and tested but NEVER CALLED** in the runtime pipeline. Zero call sites outside its own module.

This means:
- Zero-width characters, bidirectional overrides (U+202E), exfiltration patterns (`curl $SECRET`), prompt injection markers — all flow through unscanned
- The template engine's Pass 2 isolation protects against `{{context.files.secret}}` injection, but the audit trail (SecurityScanFinding events) is missing

**Fix:** Wire `scan_output()` into `run_infer()` after provider response. Emit `SecurityScanFinding` events.

**Effort:** 30min

---

### TIER 2: Should Fix (5 items, ~8h total)

#### 4. P-ORCHESTRATE — 1 Line to Enable

`wrap_as_orchestrator()` is **fully implemented with 5 unit tests** but **never called**.

**Fix:** Add to `Runner::with_event_log()`:
```rust
if workflow.goal.is_some() {
    workflow = crate::runtime::orchestrate::wrap_as_orchestrator(workflow);
}
```

Then write 1 E2E test with a `goal:` workflow. **Effort:** 1h

#### 5. 1 Real Panic to Fix

`infer.rs:224` — `last_error.unwrap()` panics on empty provider chain.

**Trigger:** `provider_chain: []` (empty array) in workflow YAML.

**Fix:** `last_error.unwrap_or_else(|| NikaError::ProviderNotConfigured { provider: "none (empty provider chain)".to_string() })`

**Effort:** 10min

#### 6. Security: TOCTOU DNS Rebinding Gap

`resolve_and_check_ssrf()` resolves hostname, checks IPs, then the HTTP client resolves again. A DNS server with TTL=0 can return safe IP for check, malicious IP for connection.

**Fix:** Use reqwest's `.resolve(hostname, validated_addr)` to pin the DNS resolution.

**Effort:** 2h

#### 7. Security: Shell Alias Bypass

Multi-line `exec:` with `shell: true` allows:
```yaml
exec:
  command: |
    alias s_u_d_o='sudo'
    s_u_d_o cat /etc/shadow
  shell: true
```

**Fix:** Prepend `unalias -a 2>/dev/null;` to shell commands, or add `alias ` to blocklist.

**Effort:** 30min

#### 8. Security: API Key Redaction Gaps

Missing patterns: Groq (`gsk_`), Gemini (`AIza`), xAI (`xai-`).

**Fix:** Add 3 regex patterns to `redact_for_event()`.

**Effort:** 30min

---

### TIER 3: Nice to Have (6 items, ~12h total)

#### 9. Mock Provider — Structured Output Support

Mock returns fixed text, can't test structured output without real API.

**Fix:** When task has `structured:` spec, mock generates valid JSON from schema.

**Effort:** 3h

#### 10. Configurable Mock for Failure Simulation

No way to test retry/fallback without real API failures.

**Fix:** `MockProvider::with_error(NikaError)` constructor + call recording.

**Effort:** 2h

#### 11. Performance: Value Clone Elimination

`get_resolved()` clones `serde_json::Value` for EVERY eager binding lookup. For `for_each` with 50 items × 3 template vars = 150 unnecessary deep clones.

**Fix:** Split into `get_ref()` (borrow, zero-cost for eager) + `resolve_lazy()` (compute, clone only for lazy).

**Effort:** 2h

#### 12. Performance: TransformExpr Pre-Parsing

`TransformExpr::parse()` called per template match in `for_each`. Same expression parsed N times.

**Fix:** Parse transforms once during `parse_template_expr()`, store as `Option<TransformExpr>` instead of `Vec<String>`.

**Effort:** 2h

#### 13. Performance: DAG compute_depths O(V²) → O(V+E)

Replace iterative `while !remaining` with Kahn's BFS topological sort. `compute_layers()` already does this correctly — `compute_depths()` is the old version.

**Effort:** 1h

#### 14. Vision E2E Test

Vision is fully implemented but needs 1 real API test (send image to Claude/OpenAI).

**Effort:** 1h

---

## UPDATED FEATURE STATUS MATRIX

| Feature | Implementation | Testing | Prod Ready |
|---------|---------------|---------|-----------|
| **infer** (simple) | 100% | E2E 6/7 providers | **YES** |
| **infer** (structured output) | 100% (real L0 native) | E2E 6/7 providers | **YES** (L0 context gap) |
| **exec** | 100% | E2E mock + security | **YES** |
| **fetch** (all 9 extract modes) | 100% | 103 tests | **YES** |
| **fetch** (response: full/binary) | 100% | Wiremock + unit | **YES** |
| **invoke** (builtin) | 100% | Comprehensive | **YES** |
| **invoke** (MCP) | 100% | Mock + protocol | **BETA** |
| **agent** (multi-turn) | 100% | Real Claude tested | **BETA** |
| **for_each** | 100% | E2E + concurrency | **YES** |
| **depends_on / DAG** | 100% | Comprehensive | **YES** |
| **bindings / transforms** (38) | 100% | Comprehensive | **YES** |
| **retry** (task-level) | 100% with backoff | Unit tests | **YES** |
| **retry** (structured output) | 95% (missing delays) | Partial | **BETA** |
| **provider fallback** | 100% (10 error categories) | Unit + integration | **YES** |
| **extended thinking** | 100% | Real Claude tested | **YES** |
| **daemon auto-start** | 100% | Integration tests | **YES** |
| **vision / multimodal** | 100% (7 providers) | Parse + mock only | **BETA** |
| **artifact writing** | 100% (4 modes) | Unit + source binding | **BETA** |
| **custom endpoints** | 100% | Config parsing only | **BETA** |
| **include workflows** | 100% parsing | Zero execution tests | **ALPHA** |
| **P-ORCHESTRATE** | 40% (unwired) | Unit (wrapping only) | **NOT READY** |
| **P-RECORD** | 80% | Partial | **ALPHA** |
| **P-CONTEXT** | 80% | TUI only | **ALPHA** |
| **agent guardrails** | 100% parsing | Parse only | **ALPHA** |

---

## SECURITY AUDIT RESULTS

| # | Finding | Severity | Status |
|---|---------|----------|--------|
| SEC-01 | output_scanner never called — LLM injection unscanned | HIGH | **OPEN** |
| SEC-02 | TOCTOU DNS rebinding gap | MEDIUM | **OPEN** |
| SEC-03 | Shell alias bypass via multi-line commands | MEDIUM | **OPEN** |
| SEC-04 | API key redaction missing Groq/Gemini/xAI patterns | LOW-MED | **OPEN** |
| SEC-05 | validate_artifact_path doesn't resolve symlinks (documented) | LOW-MED | **KNOWN** |
| SEC-06 | $env reads ANY env var without allowlist | LOW | **BY DESIGN** |
| SEC-07 | MCP tool results trusted without size limit | LOW | **OPEN** |

**Well-Implemented Security (confirmed):**
- NFKC normalization + zero-width stripping
- Template injection isolation (Pass 2 checks original template only)
- SSRF: IPv4/IPv6/mapped/compatible/CGN/metadata all blocked
- Post-redirect SSRF check
- Env var name validation (POSIX compliant)
- Sensitive env var stripping from exec children
- API key masking in Debug impls
- CRLF injection prevention in headers

---

## EXECUTION PLAN (REVISED)

### Sprint 1: "Model Routing + Critical Fixes" (1 day)

| # | Task | Effort | Impact |
|---|------|--------|--------|
| 1 | Create `ModelResolver` in nika-core | 2h | Fixes 12 routing issues |
| 2 | Wire ModelResolver into infer executor | 1h | Correct model everywhere |
| 3 | Wire ModelResolver into agent executor + TUI | 1h | Consistent display |
| 4 | Fix L0 structured output context (original_prompt, provider_context) | 1h | Better L3/L4 retry quality |
| 5 | Fix empty provider chain panic (`infer.rs:224`) | 10min | Zero panics |
| 6 | Wire output_scanner into run_infer | 30min | LLM output security |
| 7 | Add 200ms delay to structured output retry | 15min | No rate limit spam |

### Sprint 2: "P-ORCHESTRATE + Security" (1 day)

| # | Task | Effort | Impact |
|---|------|--------|--------|
| 1 | Wire `wrap_as_orchestrator()` into Runner (1 line) | 15min | Enable P-ORCHESTRATE |
| 2 | E2E test for goal-driven workflow | 1h | Validate orchestration |
| 3 | Pin DNS resolution via reqwest .resolve() | 2h | Close TOCTOU gap |
| 4 | Add `alias ` to shell blocklist | 30min | Close alias bypass |
| 5 | Add Groq/Gemini/xAI patterns to redaction | 30min | Complete key masking |
| 6 | Add aggregate timeout to structured output engine | 30min | Prevent 20min zombie retries |
| 7 | Add size limit on MCP tool results | 30min | Prevent memory exhaustion |

### Sprint 3: "Mock + E2E" (1 day)

| # | Task | Effort | Impact |
|---|------|--------|--------|
| 1 | Mock provider structured output support | 3h | Test without API keys |
| 2 | Mock failure simulation | 1h | Test retry/fallback paths |
| 3 | Vision E2E test (real API) | 1h | Validate full vision flow |
| 4 | Artifact E2E test (verify files on disk) | 1h | Validate artifact writing |
| 5 | Agent guardrails E2E test | 1h | Validate NIKA-112 |

### Sprint 4: "Performance + Polish" (1 day)

| # | Task | Effort | Impact |
|---|------|--------|--------|
| 1 | get_resolved() → get_ref() for eager bindings | 2h | Eliminate Value clones |
| 2 | Pre-parse TransformExpr in template AST | 2h | No re-parsing in for_each |
| 3 | resolve_alias_path Cow<Value> | 1h | Less allocation |
| 4 | DAG compute_depths Kahn's algorithm | 1h | O(V+E) consistency |
| 5 | Run 502 example workflows | 4h automated | Mass validation |

### Sprint 5: "Release v0.53" (half day)

| # | Task | Effort |
|---|------|--------|
| 1 | Version bump + CHANGELOG | 1h |
| 2 | Final test suite run | 30min |
| 3 | Tag + push | 15min |

---

## CONFIDENCE MATRIX (REVISED)

| Component | Confidence | Key Evidence |
|-----------|-----------|-------------|
| Parser / AST | **VERY HIGH** | 900+ tests, 3-phase validated |
| DAG scheduling | **VERY HIGH** | Kahn's algorithm, cycle detection |
| Bindings / 38 transforms | **VERY HIGH** | Comprehensive unit + E2E |
| exec verb | **VERY HIGH** | Security blocklist + NFKC + tests |
| fetch (all 9 modes) | **VERY HIGH** | 103 tests including wiremock |
| Structured output (5 layers) | **HIGH** | Real native L0, 6/7 providers validated |
| Provider routing | **LOW** | 12 issues, needs ModelResolver |
| Retry + fallback | **HIGH** | Full implementation with backoff |
| Security (SSRF/blocklist) | **HIGH** | 54 security tests, 3 vulns in v0.52 |
| output_scanner | **NONE** | Dead code — never called |
| P-ORCHESTRATE | **LOW** | 40% done, 1 line to enable |
| Vision | **HIGH** (impl) / **LOW** (testing) | Full impl, zero E2E |
| Daemon | **HIGH** | Auto-start + IPC tested |
| TUI | **VERY HIGH** | 2153 tests |
