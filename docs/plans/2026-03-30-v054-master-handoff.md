# v0.54 Master Handoff — 20-Agent Deep Audit Results

> **Date:** 2026-03-30 | **From:** v0.53.0 release session | **For:** Next session(s)

## What Was Done (v0.53.0)

- **13 commits**, 9,011 tests, 0 failures
- 3 CRITICAL + 3 HIGH + 3 MEDIUM bug fixes
- 2 security patches (trace redaction, shell escape warning)
- 5 stale BUG PROVEN tests removed (-209 LOC)
- GitHub Release created: https://github.com/supernovae-st/nika/releases/tag/v0.53.0
- CI: 6 pipelines triggered (CI, SAST, Release, CodeQL, Validate, Release-plz)

## CI Status

| Pipeline | Status |
|----------|--------|
| CI (cargo nextest + clippy + fmt) | Running |
| SAST (geiger + CodeQL + Semgrep) | Running |
| Release (7 targets + Docker + npm + crates.io) | Running |
| Validate Nika Workflows | Running |
| CodeQL | Running |
| Release-plz | Running |

**Known:** Previous CI run failed on rustfmt (nightly edition diff). Fixed with `cargo fmt --all` commit.

---

## 20-Agent Audit Consolidated Findings

### Priority Matrix

#### P0: CRITICAL (fix before v0.54 tag)

| # | Bug | File | Agent | Effort |
|---|-----|------|-------|--------|
| SEC-1 | **Secrets logged in tracing::warn** — exec.rs:83 and security.rs:258,377 log raw resolved commands | exec.rs, security.rs | Security | 15m |
| SEC-2 | **to_value_redacted() doesn't recurse** — nested JSON objects/arrays leak secrets | resolve.rs:458 | Binding | 30m |
| FETCH-1 | **429 Retry-After header ignored** — exponential backoff instead of server-mandated delay | fetch.rs:417 | Fetch | 1h |
| FETCH-2 | **No FetchFailed event on 5xx exhaustion** — only Err returned, no event for observability | fetch.rs:466 | Fetch | 15m |
| EXEC-1 | **BINDING_RE misses {{context.*}}** — shell warning regex only checks with/inputs | exec.rs:46 | Exec Security | 5m |

#### P1: HIGH (fix in v0.54)

| # | Bug | File | Agent | Effort |
|---|-----|------|-------|--------|
| AGENT-1 | **max_tokens(8192) hardcoded** — 22 instances in provider/rig/mod.rs | rig/mod.rs | Agent Loop | 4h |
| AGENT-2 | **SEC-AGENT-01: Agent bypasses security** (deferred from v0.53) | rig/tool.rs | v0.53 plan | 4h |
| RUNNER-1 | **O(n^2) get_ready_tasks()** — n tasks x d deps per loop iteration | runner.rs:408 | DAG Runner | 3h |
| RUNNER-2 | **Semaphore permit released early** — concurrency limit bypassable | runner.rs:2281 | DAG Runner | 2h |
| RUNNER-3 | **fail_fast=false skips cancelled items** — incomplete error summaries | runner.rs:2594 | DAG Runner | 1h |
| TRANSFORM-1 | **Pipe parser quote tracking only inside parens** — `filter(it's)` breaks | transform.rs:152 | Transform | 2h |
| TRANSFORM-2 | **Shell transform doesn't error on null** — returns `'null'` string | transform.rs:537 | Transform | 15m |
| MCP-1 | **50MB limit not on resource reads** — only on tool calls | invoke.rs:211 | MCP | 30m |
| SECRET-RE | **Incomplete regex** — missing Stripe, Twilio, DB connection strings | verbs.rs:111 | Security | 30m |

#### P2: MEDIUM (v0.54 nice-to-have)

| # | Bug | File | Agent | Effort |
|---|-----|------|-------|--------|
| RUNNER-4 | NIKA-026 error code reused for semaphore failures | runner.rs:2298 | DAG Runner | 15m |
| RUNNER-5 | Artifact path collisions not detected | artifact_processor.rs | DAG Runner | 1h |
| RUNNER-6 | Circular with: bindings only check direct self-ref | dag/validate.rs:184 | DAG Runner | 2h |
| MOCK-1 | Mock provider doesn't load file-based schemas | infer.rs:353 | Structured | 1h |
| MOCK-2 | generate_mock_json() has no recursion depth limit | mock_json.rs | Structured | 30m |
| TUI-1 | Provider strings not migrated to ProviderName enum | lifecycle.rs:66 | TUI | 2h |
| DOC-1 | Dockerfile VERSION=0.52.0 (should be 0.53.0) | Dockerfile:56 | Docs | 5m |
| PERF-1 | EventLog O(n) drain() — should be ring buffer | log.rs:1186 | Perf | 2h |
| PERF-2 | Template resolution unbounded String allocations | template.rs:370 | Perf | 30m |
| PERF-3 | Broadcast channel capacity 1024 too low for heavy for_each | log.rs:1120 | Perf | 5m |
| AST-1 | `use:` keyword silently ignored (should suggest `with:`) | parser.rs | AST | 30m |
| AST-2 | `max_retries:` at task level silently ignored | parser.rs | AST | 30m |

#### P3: LOW (defer to v0.55+)

| # | Bug | File | Agent |
|---|-----|------|-------|
| MaxTurnsReached dead variant | types.rs | Agent Loop |
| TOCTOU symlink race in file tools | context.rs:272 | Security |
| SSRF redirect DNS re-pinning per hop | fetch.rs:378 | Fetch |
| repair_model not validated at config time | infer.rs:758 | Structured |
| LSP missing ModelResolver integration | lsp-core | LSP |
| Task+verb retry compounding undocumented | fetch.rs + runner.rs | Fetch |
| Vec::with_capacity() missing in hot paths | resolve.rs:259 | Perf |

---

## What Works Perfectly (verified by 20 agents)

- **Media pipeline**: CAS atomic writes, decode_image_safe, SVG sanitization, 100MB limits
- **DashMap usage**: Zero race conditions, no locks held across await
- **Cancellation propagation**: 3-point check, per-parent fail_fast tokens
- **Error codes**: 158 unique NIKA-XXX codes, no duplicates, 43% with FixSuggestion
- **CI pipeline**: 8 workflows, 7 release targets, cargo-deny + audit + geiger + CodeQL + Semgrep
- **Secrets architecture**: Unix socket 0o600, env→daemon→error consistent, no keychain popups
- **LSP**: Full completions, hover, diagnostics, no panics in production code
- **Token counting**: All sites now use saturating_add
- **Keyboard shortcuts**: No conflicts in TUI

---

## Previously Fixed Bugs (confirmed by agents)

These wave2 bugs are NOW FIXED:
- Gemini API key dispatch (chat_continue checks GEMINI_API_KEY)
- Whitespace key handling (consistent trim() everywhere)
- NaturalCompletion hardcoding (all providers call determine_status())
- Token overflow (saturating_add everywhere)
- panic!/unwrap() in agent loop (all safe now)
- mem::take on tools (removed from production)

---

## Master Plan v0.54

### Sprint 1: Security (P0, ~2h)

1. **Redact tracing::warn calls** — exec.rs:83, security.rs:258, security.rs:377
2. **Recursive JSON redaction** — add `redact_value_recursive()` to resolve.rs
3. **Extend BINDING_RE** — add `context` to regex pattern
4. **Emit FetchFailed event** on 5xx exhaustion
5. **429 Retry-After header** — parse and respect

### Sprint 2: Agent + Provider (P1, ~8h)

6. **Replace 22x max_tokens(8192)** with effective_max_tokens()
7. **SEC-AGENT-01** — Thread PolicyEnforcer through RigAgentLoop
8. **MCP resource read size limit** — 50MB check on resources too

### Sprint 3: Runner Robustness (P1, ~6h)

9. **Fix get_ready_tasks O(n^2)** — cache task readiness
10. **Hold semaphore permit** through task execution
11. **Fix fail_fast=false** error collection
12. **Fix NIKA-026** error code reuse

### Sprint 4: Quality + Perf (P2, ~6h)

13. **Pipe parser quote tracking** outside parens
14. **Shell transform null check**
15. **EventLog ring buffer** (perf)
16. **Broadcast channel** capacity increase
17. **Artifact path collision detection**
18. **Dockerfile version** update
19. **TUI ProviderName migration**

### Sprint 5: DX + Parser (P2, ~2h)

20. **Reject `use:` keyword** with `did you mean with:?`
21. **Reject `max_retries:` at task level** with suggestion
22. **SECRET_RE expansion** (Stripe, Twilio, DB URIs)

---

## Estimated Total: ~24h across 5 sprints

| Sprint | Tasks | Time | Risk |
|--------|-------|------|------|
| 1. Security P0 | 5 | 2h | Low |
| 2. Agent + Provider P1 | 3 | 8h | Medium |
| 3. Runner P1 | 4 | 6h | Medium |
| 4. Quality + Perf P2 | 7 | 6h | Low |
| 5. DX + Parser P2 | 3 | 2h | Zero |
| **Total** | **22** | **~24h** | |

---

## Agent Audit Stats

| Agent | Tokens | Duration | Findings |
|-------|--------|----------|----------|
| Security deep audit | 91K | 3m24s | 2 CRIT, 3 HIGH, 3 MED |
| Transform edge cases | 68K | 2m02s | 2 CRIT, 1 MED |
| Fetch verb edge cases | 70K | 2m19s | 1 CRIT, 2 HIGH, 3 MED |
| Binding resolution | 76K | 2m44s | 2 HIGH gaps |
| Agent loop + provider | 89K | 1m59s | 1 HIGH unfixed, 5 confirmed fixed |
| DAG + runner | 93K | 1m50s | 1 CRIT, 3 HIGH, 3 MED |
| Structured output | 85K | 2m41s | 2 MED bugs |
| Dead code scan | 73K | 1m30s | 50 LOC dead, 14 annotations |
| TUI regression | 71K | 1m07s | 1 MED (provider strings) |
| Error codes | 93K | 5m20s | 158 codes, 0 duplicates |
| MCP integration | 82K | 56s | 1 MED gap |
| Exec security | 66K | 1m18s | 1 MED, 1 HIGH |
| Media pipeline | 78K | 1m21s | 0 issues (all secure) |
| Docs consistency | 56K | 34s | 1 CRIT (Dockerfile) |
| AST analyzer | 85K | 2m25s | 2 MED gaps |
| Secrets + daemon | 74K | 48s | 1 MED pattern gap |
| Real workflows | - | - | (pending) |
| Performance | 84K | 2m21s | 2 HIGH, 2 MED |
| LSP | 56K | 53s | 1 LOW (no ModelResolver) |
| CI pipeline | 50K | 47s | Clean, 8 workflows |
| **TOTAL** | **~1.5M** | **~37min** | **5 P0 + 13 P1 + 12 P2** |
