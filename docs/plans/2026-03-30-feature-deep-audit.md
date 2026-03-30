# Feature Deep Audit — 10-Agent Report + Real Workflow Tests

> **Date:** 2026-03-30 | **Agents:** 10 | **Tokens:** ~850K | **Workflows tested:** 3

## Real Workflow Test Results

| Workflow | Provider | Result | Finding |
|----------|----------|--------|---------|
| orchestrate-mock | mock | **3/3 tasks PASS** | Orchestration works end-to-end |
| orchestrate-simple | anthropic | **FAIL** — no credits | NIKA-031 credit balance |
| orchestrate-openai | openai | **Tasks OK, orchestrator FAIL** | LLM generates bad YAML for nika:run |

### Critical Finding: Orchestrator YAML Generation Failure

The orchestrator agent tried to call `nika:run` with inline YAML but failed 3 times:
1. Missing `schema:` field → NIKA-212
2. Used `steps:` instead of `tasks:` → NIKA-163
3. Missing task `id:` field → NIKA-161

**Root cause:** System prompt doesn't include YAML syntax examples. The LLM doesn't know Nika's schema.

**Fix:** Add a YAML template example to `build_system_prompt()` in orchestrate.rs.

---

## Feature Completeness Matrix

| Feature | Complete | Production Ready | Key Gap |
|---------|----------|-----------------|---------|
| **Structured Output (L0-L4)** | 95% | Yes | No `$ref` support, no recursion depth limit |
| **Agent Verb** | 88% | Yes | Scope not wired, LLM guardrails stub |
| **Fetch (9 modes)** | 100% | Yes | 429 Retry-After header ignored |
| **for_each + DAG** | 95% | Yes | No nested for_each, no conditional `when:` |
| **Custom Endpoints** | 100% | Yes | Agent presets (8 named) not wired |
| **Media Pipeline (25 tools)** | 100% | Yes | All tools complete, no stubs |
| **TUI (3 views)** | 95% | Yes | Editor line-based not char-level |
| **CLI (23 commands)** | 95% | Yes | All functional, minor polish gaps |
| **Orchestrate** | 40% | **No** | Limits not enforced, no YAML examples in prompt |
| **Record + Context** | 100% | Yes | Mature compression, token budgeting |
| **JSON Schema (validator)** | 85% | Partial | 9 fields missing from embedded schema |

---

## P0: Must Fix Before Launch

| # | Issue | File | Effort | Source |
|---|-------|------|--------|--------|
| 1 | **Orchestrator system prompt needs YAML syntax example** | orchestrate.rs:20 | 30m | Real workflow test |
| 2 | **JSON schema missing 9 fields** (max_duration_secs, timeout, shorthand infer fields) | schemas/*.json | 1h | Schema sync agent |
| 3 | **confidence_target not validated** (accepts NaN, negative, >1.0) | orchestrate.rs | 10m | Orchestrate agent |
| 4 | **nika:complete schema mismatch** — declares confidence+reasoning required, actually optional | complete.rs:173 | 10m | Orchestrate agent |
| 5 | **Secrets in tracing::warn** (from v0.54 handoff, still open) | exec.rs:83, security.rs | 15m | Security audit |
| 6 | **to_value_redacted() not recursive** (from v0.54 handoff) | resolve.rs:458 | 30m | Binding audit |

## P1: High Priority

| # | Issue | File | Effort | Source |
|---|-------|------|--------|--------|
| 7 | **max_tokens(8192) hardcoded** — 22 instances | rig/mod.rs | 4h | Agent audit |
| 8 | **Orchestrate max_rounds/max_cost not enforced at runtime** | runner.rs | 2h | Orchestrate agent |
| 9 | **No E2E integration tests for orchestrate** | tests/ | 3h | Orchestrate agent |
| 10 | **Agent scope (full/minimal/debug) parsed but not wired** | mod.rs:285 | 2h | Agent audit |
| 11 | **LLM guardrails type: llm not implemented** | thinking.rs:57 | 3h | Agent audit |
| 12 | **Pipe parser quote tracking outside parens** | transform.rs:152 | 2h | v0.54 handoff |
| 13 | **O(n^2) get_ready_tasks()** | runner.rs:408 | 3h | Runner audit |
| 14 | **Semaphore permit released early** | runner.rs:2281 | 2h | Runner audit |
| 15 | **429 Retry-After header ignored** | fetch.rs:417 | 1h | Fetch audit |
| 16 | **No FetchFailed event on 5xx exhaustion** | fetch.rs:466 | 15m | Fetch audit |

## P2: Medium Priority

| # | Issue | File | Effort | Source |
|---|-------|------|--------|--------|
| 17 | 8 named agent presets not wired (think, lite, search...) | presets/ | 4h | Provider audit |
| 18 | Guardrail retry hardcoded to 2 | providers.rs:570 | 15m | Agent audit |
| 19 | TUI provider strings not migrated to ProviderName enum | lifecycle.rs:66 | 2h | TUI audit |
| 20 | EventLog O(n) drain() → ring buffer | log.rs:1186 | 2h | Perf audit |
| 21 | Broadcast channel capacity 1024 too low | log.rs:1120 | 5m | Perf audit |
| 22 | Dockerfile VERSION=0.52.0 | Dockerfile:56 | 5m | Docs audit |
| 23 | No conditional `when:` on tasks | parser.rs | Design decision | DAG audit |
| 24 | No nested for_each | runner.rs | Design decision | DAG audit |
| 25 | fail_fast=false skips cancelled items | runner.rs:2594 | 1h | Runner audit |
| 26 | Artifact path collisions not detected | artifact_processor.rs | 1h | Runner audit |
| 27 | SECRET_RE missing Stripe/Twilio/DB patterns | verbs.rs:111 | 30m | Security audit |
| 28 | BINDING_RE misses {{context.*}} | exec.rs:46 | 5m | Exec audit |

---

## What's Excellent (confirmed by 10 agents)

### Media Pipeline: 25/25 tools COMPLETE, ZERO stubs
- CAS: blake3 atomic writes, zstd compression, O_EXCL dedup
- All 3 tiers fully implemented with comprehensive tests
- C2PA provenance signing with ephemeral Ed25519 certificates
- QR validation with multi-decoder + 0-100 scan score

### Structured Output: 94 tests, 5-layer defense working
- L0b (tool injection via DynamicSubmitTool): 9 tests
- L2 (extract+validate): 21 tests, 4-strategy JSON extraction
- L3 (retry with feedback): 5 tests, prompt includes errors
- L4 (LLM repair): 3 tests, separate repair model support
- from_example schema derivation: 9 tests

### Agent System: Sophisticated multi-turn loop
- 7 providers fully working with streaming
- Guardrails (regex/length/schema): integrated with retry
- Limits (cost/tokens/duration/turns): all enforced
- Spawn agent: depth-limited recursive delegation
- MCP tool integration with binary content staging

### Fetch: All 9 extract modes production-ready
- markdown (htmd), article (dom_smoothie), text, selector, metadata, links (scraper)
- jsonpath (serde_json_path), feed (feed_rs), llm_txt (built-in)
- 3-layer SSRF: string check → DNS pinning → post-redirect
- Binary CAS pipeline fully integrated

### CLI: 23 commands, 12-level course, 120+ showcases
- TUI: 3-view architecture (Studio/Command/Control)
- Chat: streaming, thinking accumulation, session persistence
- Doctor: 8 diagnostic sections with auto-fix
- Daemon: Unix socket IPC, auto-start, system service

### DAG: Correct wave scheduling with per-parent fail_fast
- Diamond pattern: proper multi-dep merging
- for_each_index: 0-based iteration counter
- include: recursive with prefix namespacing
- Bug #26 fixed: per-parent cancellation tokens

---

## Sprint Plan (updated with new findings)

### Sprint 1: Schema + Orchestrator Fix (~3h)
1. Add 9 missing fields to JSON schema
2. Add YAML syntax example to orchestrator system prompt
3. Validate confidence_target bounds
4. Fix nika:complete schema (only result required)
5. Redact tracing::warn calls

### Sprint 2: Security + Correctness (~3h)
6. Recursive JSON redaction in to_value_redacted()
7. Extend BINDING_RE to {{context.*}}
8. Expand SECRET_RE patterns
9. Emit FetchFailed event on 5xx exhaustion
10. 429 Retry-After header support

### Sprint 3: Agent + Provider (~8h)
11. Replace 22x max_tokens(8192)
12. Wire agent scope (full/minimal/debug)
13. Implement LLM guardrails
14. Wire 8 named agent presets
15. Orchestrate: enforce max_rounds + max_cost at runtime

### Sprint 4: Runner + Perf (~6h)
16. Fix O(n^2) get_ready_tasks
17. Hold semaphore permit through execution
18. EventLog ring buffer
19. Broadcast channel capacity increase
20. Fix fail_fast=false error collection

### Sprint 5: E2E Tests + Polish (~4h)
21. Orchestrate E2E integration tests
22. TUI ProviderName migration
23. Dockerfile version update
24. Artifact collision detection

**Total: ~24h across 5 sprints**
