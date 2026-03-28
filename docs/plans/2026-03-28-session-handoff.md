# Session Handoff — 2026-03-28

**Session**: Mega bug-fix session (~50 commits, 2 review waves, 15 fix agents)
**Baseline**: v0.50.0 with ~8500 tests
**Final**: 3803 engine + 754 core + 328 media + 140 cli + 2153 tui = **7178+ tests, 0 failures**
**Branch**: `main` (all pushed to origin)

---

## What Was Done

### Wave 1: Custom Endpoints (9 commits)
- Box::leak memory leak in OpenAiCompat name()/default_model()
- Cost $0.00 for custom endpoints → cost_provider_kind()
- Wire custom endpoints from config.toml into Runner/CLI/TUI
- Schema JSON sync (base_url, MCP url+transport, verb-less tasks, server::tool)
- NIKA-035/036 error codes for endpoints
- Strip `<think>` tags from reasoning model responses
- endpoint_url field on ProviderCalled event
- Permission erasure fix (set_custom_endpoints setter)

### Wave 2: Bug Hunt (5 review agents → 30 findings → 16 fixes)
- Retry on all verbs (was fetch-only)
- Daemon test isolation (NIKA_HOME tempdir)
- Broken example workflows (when:, {{date}})
- strip_think_tags case-insensitive + `<thinking>` variant
- Backoff multiplier NaN/0.0 validation
- Multi-turn provider stats attribution
- u64 precision in coerce_json_types
- SVG xlink:href nuanced (allow #fragment, block external)
- Cost cache overclaim fix
- DNS rebinding false positive on raw IPs

### Wave 3: Deep Review (10 review agents → 45 findings → 15 fixes)
- **4 CRITICALs**: timeout on all infer paths, SVG data URI bypass, Timeout in retry, run_openai stop_reason
- **7 HIGHs**: SSRF parse failure, xlink unquoted, think tags on L0+streaming, guardrail override, turn_count, budget, fix hints
- **6 schema fixes**: DecomposeSpec required, InferParams cleanup, OutputPolicy schema_ref, AgentParams from, FetchParams retry, sync

---

## What STILL Needs Fixing

### CRITICAL / HIGH (from review, not yet fixed)

| # | Bug | File | Why Not Fixed |
|---|-----|------|---------------|
| 1 | **Template injection** when original has legit context ref + injected ref in with: value | `binding/template.rs:498` | Complex — needs position map or separate Pass 2 extraction |
| 2 | **Binary fetch OOM** — `response.bytes().await` bypasses streaming size limit on chunked encoding | `fetch.rs:450` | Needs streaming binary path, not trivial |
| 3 | **Vision cost wildly inaccurate** — input tokens only count text prompt, not image data | `infer.rs:1150` | Needs heuristic: `est_in += total_bytes / 750` |
| 4 | **ProviderCalled emitted before budget reserve** — orphaned event on budget exhaustion | `infer.rs:276-300` | Move emit after reserve_tokens |
| 5 | **Preset not applied during structured retry** — retry uses task.provider not effective_provider | `runner.rs:626-643` | Needs `get_retry_config` to accept effective values |
| 6 | **MaxTurnsReached dead code** — rig-core silently truncates, status shows "end_turn" | `types.rs:33` | Needs turn tracking across MultiTurnStreamItem |
| 7 | **for_each items {{inputs.xxx\|transform}}** — pipe transforms silently ignored | `runner.rs:1923` | Needs full TransformExpr pipeline for for_each |
| 8 | **validate_template_refs skips context/inputs validation** | `dag/validate.rs:62` | Needs extract_context_refs + extract_input_refs |
| 9 | **extract_templates misses fetch.headers, agent.system** | `dag/validate.rs:72` | Add fields to extraction |

### MEDIUM (from review, not yet fixed)

| # | Bug | File |
|---|-----|------|
| 10 | for_each fail_fast=false produces null entries in output | `runner.rs:2416` |
| 11 | EventLog Relaxed atomic allows out-of-order Vec insertions | `log.rs:1005` |
| 12 | ImageUrl bytes not counted in 100MB vision size limit | `infer.rs:962` |
| 13 | Gemini 2.5 Pro tiered pricing not modeled | `cost.rs:294` |
| 14 | NIKA-033 overloaded (InvalidConfig + LSP deprecated model) | `error.rs` |
| 15 | StructuredOutputRepairFailed in is_recoverable may cause infinite retry | `error.rs:1057` |
| 16 | TUI silently drops TaskRetry/FetchRetry events | `event_handler/mod.rs:468` |
| 17 | structured_success_layer overwrites instead of max() | `renderer.rs:280` |
| 18 | endpoints.rs missing ::1 and ::ffff:127.x block | `endpoints.rs` |
| 19 | is_in_json_context false positive for templates starting with {{ | `template.rs:1565` |
| 20 | llm_txt sub-requests skip DNS rebinding check | `fetch.rs:531` |
| 21 | HttpResponse elapsed_ms accumulates across retries | `fetch.rs:309` |
| 22 | extract_links ignores base_url, returns raw relative hrefs | `extract.rs:236` |
| 23 | Task ID validation mismatch (schema strict, analyzer permissive) | `analyze.rs:1250` |

---

## Next Session Plan

### Phase A: Fix Remaining CRITICALs + HIGH Security (2h)

**Priority order:**
1. **Template injection** (#1) — the only remaining security vulnerability
2. **Binary fetch OOM** (#2) — use `read_body_with_limit` for binary too
3. **ProviderCalled before budget** (#4) — swap 2 lines
4. **Vision cost heuristic** (#3) — add `total_bytes / 750` to estimate

### Phase B: Real Provider Integration Test (2h)

**Goal**: Run actual .nika.yaml workflows against real providers, not just unit tests.

```bash
# Verify API keys
nika provider list

# Test each provider with a simple workflow
nika run examples/diamond.nika.yaml --provider anthropic
nika run examples/diamond.nika.yaml --provider openai
nika run examples/diamond.nika.yaml --provider gemini

# Test custom endpoint (vLLM on H100)
# Requires config.toml with [endpoints.h100]
nika run examples/diamond.nika.yaml --provider h100

# Test structured output
nika run examples/test-foreach-schema.nika.yaml

# Test media pipeline
nika run examples/test-file-output.nika.yaml

# Test agent: verb
nika run examples/agents-preset.nika.yaml

# Test retry behavior (use mock with deliberate failures)
nika run examples/test-schema-retry.nika.yaml --provider mock
```

**What to watch for:**
- Custom endpoint cost shows non-zero in summary
- strip_think_tags works on Qwen responses (no `<think>` in output)
- Retry events appear in live renderer for transient failures
- preset: resolves provider/model correctly
- Vision workflows work with Claude/OpenAI

### Phase C: Fix MEDIUMs + Another Review Wave (2h)

1. Fix #5-#9 from the HIGH list above
2. Fix easy MEDIUMs (#10, #16, #17, #18)
3. Run another 5-agent review wave
4. Repeat until < 5 findings

### Phase D: Release v0.50.1 (1h)

1. Bump version
2. Update CHANGELOG
3. Tag + push
4. Verify CI pipeline
5. Publish to crates.io if ready

---

## Infrastructure TODO (Not Code)

| Task | Effort | Blocks |
|------|--------|--------|
| Deploy registry.supernovae.studio | 4h | All `nika pkg` remote |
| Renew VSCE_PAT for VS Code marketplace | 1h | Extension publish |
| Configure vLLM endpoint in CI for integration tests | 2h | Custom endpoint CI |
| Set up Telegram webhook trigger | 1h | Remote triggers |

---

## Key Files Modified This Session

```
nika-engine/src/provider/rig.rs          — Box::leak, cost, timeouts
nika-engine/src/provider/cost.rs         — cache cost, ProviderKind::parse
nika-engine/src/provider/endpoints.rs    — SSRF IPv6
nika-engine/src/runtime/runner.rs        — retry all verbs, endpoints, Timeout
nika-engine/src/runtime/executor/infer.rs — SSRF, think tags, budget, events
nika-engine/src/runtime/executor/verbs.rs — strip_think_tags, u64 coerce
nika-engine/src/runtime/executor/fetch.rs — backoff validation
nika-engine/src/runtime/rig_agent_loop/  — guardrails, stop_reason, turn_count
nika-engine/src/display/renderer.rs      — multi-turn stats
nika-engine/src/display/live.rs          — ANSI stripping
nika-engine/src/error.rs                 — NIKA-035/036, fix hints
nika-engine/src/error_domains.rs         — From impl preservation
nika-event/src/log.rs                    — endpoint_url field
nika-media/src/tools/safety.rs           — SVG sanitizer (xlink, data URI)
nika/schemas/nika-workflow.schema.json   — 6 schema-parser sync fixes
```
