# v0.50.0 Security Fixes + Release Handoff

**Date**: 2026-03-29
**From**: Phase 0 stabilize session (2026-03-28)
**Status**: DRAFT — awaiting 10-agent deep dive completion

---

## Session Summary (2026-03-28)

### What shipped (pushed to main)
- `preset:` field on tasks (agents block → model routing)
- `retry:` on all verbs (was fetch-only)
- TaskRetry event + display rendering
- 22 workflows fixed ($ prefix, model, to_yaml, typos)
- Error code table fix (NIKA-160 is parse, not policy)
- VS Code extension v0.50.0 (snippets, schema URL, model in template)
- JSON schema: preset added, both copies synced
- validate() preset gaps fixed
- o3 pricing synced ($10→$2)
- Newline injection in shell mode blocked (CRITICAL security fix)
- 4 gate tests for preset (feature + error)
- CHANGELOG v0.50.0

### Test status
- 4509+ tests passing, 0 failures
- 6/6 E2E preset tests PASS (mock provider)

---

## Bug Hunt Results (78 bugs from 20 agents across 2 waves)

### CRITICAL (1 remaining)
| # | Bug | Component | Effort |
|---|-----|-----------|--------|
| 3 | `on_limit_reached.action` parsed but never read at runtime | agent/limit_tracker | 30 min |

### HIGH (9 remaining)
| # | Bug | Component | Effort |
|---|-----|-----------|--------|
| 4 | LLM guardrails silently skipped (run_guardrails_async missing) | agent/guardrails | 2h (or document as v0.51) |
| 5 | extended_thinking is single-turn (no tools, no loop) | agent/thinking | 2h (or document) |
| 6 | was_last_call_cached() AtomicBool race | MCP/client | 30 min |
| 7 | Reconnect doesn't re-populate validator cache | MCP/client | 20 min |
| 8 | Resource blob errors silently swallowed | MCP/invoke | 15 min |
| 9 | DNS rebinding SSRF bypass (pre-connect string check) | fetch/policy | 1h |
| 10 | No streaming response size limit (OOM) | fetch | 30 min |
| 11 | Debug derive leaks api_key | provider/endpoints | 10 min |
| 12 | IPv6 SSRF in endpoint validation | provider/endpoints | 15 min |

### MEDIUM (23 remaining)
See bug hunt report from previous session.

---

## Fix Plan (ordered by priority)

### Wave 1: Security (before release tag)

#### Fix 1.1: Block newline injection in shell mode
**Status: DONE** (commit 916896254)

#### Fix 1.2: api_key Debug leak (Bug #11)
**File**: `nika-engine/src/provider/endpoints.rs`
**Change**: Replace `#[derive(Debug)]` with manual `Debug` impl that masks api_key
**LOC**: ~15

#### Fix 1.3: IPv6 SSRF in endpoint validation (Bug #12)
**File**: `nika-engine/src/provider/endpoints.rs:70-112`
**Change**: Add IPv6-mapped metadata check (`::ffff:169.254.169.254`)
**LOC**: ~10

#### Fix 1.4: DNS rebinding SSRF (Bug #9)
**File**: `nika-engine/src/runtime/policy.rs` + `executor/mod.rs`
**Change**: Add reqwest `resolve` callback or post-connect IP check
**Complexity**: MEDIUM — reqwest doesn't have native IP-level SSRF blocking
**Alternative**: Document as known limitation, add warning in docs
**LOC**: ~30-50

#### Fix 1.5: Response size streaming limit (Bug #10)
**File**: `nika-engine/src/runtime/executor/fetch.rs`
**Change**: Replace `response.text().await` with streaming reader that aborts at limit
**LOC**: ~40

### Wave 2: Agent correctness (can ship post-release)

#### Fix 2.1: on_limit_reached.action (Bug #3) — CRITICAL
**File**: `nika-engine/src/runtime/rig_agent_loop/providers.rs`
**Change**: Read LimitTracker config, match on LimitAction::{CompletePartial,Fail,Escalate}
**LOC**: ~25

#### Fix 2.2: MCP cache hit race (Bug #6)
**File**: `nika-mcp/src/client.rs`
**Change**: Return cache_hit from call_tool instead of using shared AtomicBool
**LOC**: ~20

#### Fix 2.3: MCP reconnect validator cache (Bug #7)
**File**: `nika-mcp/src/client.rs:807-827`
**Change**: Call list_tools() after reconnect to re-populate schema cache
**LOC**: ~10

#### Fix 2.4: Resource blob silent errors (Bug #8)
**File**: `nika-engine/src/runtime/executor/invoke.rs:382-397`
**Change**: Add fatal_error tracking like the tool-call path
**LOC**: ~10

### Wave 3: Documentation + deferred

#### Doc 3.1: LLM guardrails (Bug #4)
**Decision**: Document as "agent verb only, async LLM guardrails coming in v0.51"
**Alternative**: Implement run_guardrails_async (2h)

#### Doc 3.2: extended_thinking limitations (Bug #5)
**Decision**: Document as "extended_thinking disables tools and multi-turn"
**Alternative**: Wire rig-core thinking+tools when available

---

## Preset Unit Tests Plan

Zero automated coverage. Need:

### Parser tests (nika-core)
1. `test_parse_preset_field` — YAML with `preset: think` parses correctly
2. `test_known_task_keys_includes_preset` — preset in the known keys test YAML

### Analyzer tests (nika-core)
3. `test_preset_exempts_missing_model` — infer with preset but no model passes
4. `test_preset_unknown_emits_error` — preset: ghost → NIKA-144
5. `test_preset_no_agents_emits_error` — preset without agents block → NIKA-144
6. `test_preset_available_hint` — error lists available preset names

### Runner tests (nika-engine)
7. `test_preset_resolves_provider_model` — mock workflow with preset
8. `test_preset_task_override_wins` — task provider beats preset
9. `test_preset_injects_system_prompt` — system from preset used
10. `test_preset_injects_temperature` — temperature from preset used

---

## Release v0.50.0 Checklist

### Pre-tag
- [ ] Wave 1 security fixes committed
- [ ] `cargo check --workspace` = 0 errors
- [ ] `cargo clippy --workspace -- -D warnings` = 0 warnings
- [ ] `cargo test --workspace --lib` (skip env-dependent) = 0 failures
- [ ] CHANGELOG.md has v0.50.0 entry
- [ ] All Cargo.toml versions = 0.50.0
- [ ] VS Code package.json version = 0.50.0

### Secrets verification (GitHub)
- [ ] VSCE_PAT (VS Code Marketplace) — may need renewal
- [ ] OVSX_PAT (Open VSX for Cursor) — optional but recommended
- [ ] NPM_TOKEN (npm @supernovae/nika)
- [ ] CARGO_REGISTRY_TOKEN (crates.io)
- [ ] HOMEBREW_TAP_TOKEN (supernovae-st/homebrew-tap)
- [ ] APPLE_* secrets (macOS notarization) — if available
- [ ] DOCKERHUB_USERNAME + DOCKERHUB_TOKEN

### Tag + push
```bash
git tag v0.50.0
git push origin v0.50.0
```

### Monitor
- GitHub Actions: release.yml pipeline
- VS Code Marketplace: nika-lang
- npm: @supernovae/nika
- crates.io: nika
- Docker Hub: supernovae/nika
- Homebrew: `brew install supernovae-st/tap/nika`

---

## Phase 1 Prep (post-release)

### P-MODEL (v0.51)
- preset: DONE ✓
- Next: routing chains (primary → fallback), smart model selection
- Next: `nika bench` integration with preset system

### P-RECORD (v0.52)
- Compression event recording (NDJSON → compressed archives)
- Replay from traces

### P-ORCHESTRATE (v0.53)
- `goal:` verb (LLM-driven workflow generation)
- Dynamic task decomposition

### P-CONTEXT (v0.54)
- Token budgets per task
- Context window management
- Automatic summarization for long agents

### LSP Improvements
- Task-level unknown key detection
- Preset completion (suggest agent names)
- Inline cost estimates with correct pricing
