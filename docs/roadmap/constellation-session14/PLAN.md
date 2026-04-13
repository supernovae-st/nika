# Session 14 — Constellation Refactor Plan (REVISED post-Phase 2)

> Generated 2026-04-11, post Phase 0/1/2. Gitignored — local only.
> Supersedes the MEGA-PROMPT's original wave plan based on 4-agent findings.

## Baseline (2026-04-11, pre-S14)

```
HEAD:      a3e8d8ab8  docs(constellation): ARCHITECTURE.md update for Session 13 (S13-E1)
Crates:    32
Tests:     10,840 passing, 0 failing
Clippy:    0 warnings (pending re-verification)
Engine:    146,557 LOC (target ≤100k via Constellation, ≤138k in S14)
Binary:    ~118 MB
```

## Phase 1 Findings Summary

### Agent A (infer.rs Provider methods)

- `infer.rs` calls 9 RigProvider methods (concrete type, NOT kernel trait)
- The unified `InferRequest` struct covers ALL cases — zero new `infer_*` methods needed on kernel trait
- Only addition: `supports_response_format()` capability query
- `InferResponse` needs 3 new fields: `cost_usd`, `request_id`, `finish_reason`
- `InferCallback` type in StructuredOutputEngine is the hardest migration — resolution: keep it in engine, nika-verb-infer does its own retry via direct InferRequest calls (Option B)

### Agent B (fetch bridge)

- FetchAux types are bare TaskExecutor fields (robots_cache, domain_rate_limiter, cookie_jar, fetch_cache)
- `FetchAuxBundle` as handoff-planned is WRONG — reqwest-specific types can't cross nika-verb-fetch
- Correct strategy: **shallow bridge** — gate on `!needs_custom_client && max_attempts==1 && !llm_txt && response==Text`
- Covers ~80% of real-world fetch tasks
- 2 P0 test gaps in nika-verb-fetch: HttpResponse assertion ignores all fields, no extract: path test

### Agent C (S13 bridge bugs)

**P0 (fix immediately, pre-S14):**
1. `invoke.rs:300` + `nika-verb-invoke/lib.rs:85` — duplicate `McpInvoke` event on every builtin call
2. `exec.rs` `NonZeroExit` mapping drops `exit_code` — `{ stderr, .. }` loses critical info

**P1 (fix before MCP path goes live):**
3. `VerbInvokeError::Mcp → NikaError::InvokeParamError` — wrong semantics
4. `NullBlobStore`/`NullHttpClient` silently fail — add debug_assert

### Agent D (TaskExecutor dissolution)

- Runner lives in nika-engine (NOT yet in nika-runtime)
- `task_dispatch.rs` (830 LOC) is the live single-task pipeline; has 14+ call sites to template_resolve, lower_action, lower_output
- **Wave D cannot proceed in S14** — requires migrating binding/lowering to nika-runtime first
- Active external consumers of TaskExecutor: only 3 (nika-cli/verbs.rs, nika-cli/bench.rs, integration tests)
- Full Wave D only saves ~7k LOC (engine → ~138-140k, far short of ≤100k)
- Wave D deferred to S15; S14 focuses on extraction, not dissolution

## GATE-S14 Resolutions

| Gate | Status | Outcome |
|------|--------|---------|
| S14-1 (Provider trait) | ⚠️ scope REVISED | Add `supports_response_format()` + expand InferResponse (3 fields). No new infer_* methods. |
| S14-2 (rig_agent_loop) | ✅ table compiled | 10 TEMP engine deps for nika-verb-agent (documented) |
| S14-3 (StructuredOutputEngine) | ✅ CLEAN | No provider::rig imports. InferCallback stays in engine (Option B). |
| S14-NEW1 (Fetch bridge) | ✅ strategy REVISED | Shallow bridge, no FetchAuxBundle |
| S14-NEW2 (McpPool real impl) | ✅ feasible | McpPoolAdapter wraps `Arc<McpClientPool>` |

## Commit Plan (15 commits)

### Pre-Wave: P0 Bug Fixes (2 commits)

| Label | Commit | LOC | Risk |
|-------|--------|-----|------|
| S14-BUG1 | fix(engine): include exit_code in NonZeroExit error mapping | ~5 | low |
| S14-BUG2 | fix(engine): remove duplicate McpInvoke event in invoke.rs builtin bridge | ~15 | low |

### Wave A: Foundation (4 commits)

| Label | Commit | LOC | Risk |
|-------|--------|-----|------|
| W14-A0 | feat(kernel): expand InferResponse + add supports_response_format() | ~80 | low |
| W14-A1 | feat(engine): impl supports_response_format() on RigProvider + populate InferResponse | ~40 | low |
| W14-A2 | test(verb-fetch): fix P0 test gaps (HttpResponse fields + extract path) | ~60 | low |
| W14-A3 | feat(engine): shallow bridge fetch simple-path → nika-verb-fetch | ~100 | medium |

### Wave B: Infer Extraction (4 commits)

| Label | Commit | LOC | Risk |
|-------|--------|-----|------|
| W14-B0 | feat(engine): McpPoolAdapter + fix P1 VerbInvokeError::Mcp mapping | ~120 | medium |
| W14-B1 | feat(verb-infer): create nika-verb-infer crate with InferInput/run() + 4 tests | ~400 | high |
| W14-B2 | refactor(engine): TaskExecutor::run_infer delegates to nika-verb-infer | ~400 | high |
| W14-B3 | feat(runtime): wire dispatch Infer arm | ~40 | low |

### Wave C: Agent Extraction (3 commits, stretch goal)

| Label | Commit | LOC | Risk |
|-------|--------|-----|------|
| W14-C1 | feat(verb-agent): create nika-verb-agent + move rig_agent_loop (TEMP deps) | ~500 | high |
| W14-C2 | refactor(engine): TaskExecutor::run_agent delegates to nika-verb-agent | ~150 | medium |
| W14-C3 | feat(runtime): wire dispatch Agent arm | ~40 | low |

### Wave E: Cleanup (2 commits)

| Label | Commit | LOC | Risk |
|-------|--------|-----|------|
| W14-E0 | chore(engine): remove NullBlobStore/NullHttpClient shims | ~40 | low |
| W14-E1 | docs(constellation): ARCHITECTURE.md update for Session 14 | ~50 | low |

## Deferred to S15

- Wave D (TaskExecutor dissolution)
- Wave D prerequisite: migrate `task_dispatch.rs` binding/lowering to nika-runtime
- `dispatch()` activation as live code path (all 5 arms)
- `nika-verb-exec` >1MB test refactor (G1 regression, already landed in S13)
- Provider trait consolidation beyond capability queries

## Sacred Invariants (inherited, enforced)

1. `parking_lot::RwLockReadGuard` never held across `.await`
2. `kill_on_drop(true)` on every tokio::process::Command
3. `tokio::try_join!` for concurrent pipe reads
4. >1MB regression test for any subprocess code
5. Golden test oracle captures lifecycle AND output
6. `cargo check --no-default-features` in every verify ritual
7. `policy_enforcer.read().clone()` before async boundary
8. 4 tests minimum per verb crate
9. Caps structs with `new()` constructors (non_exhaustive)
10. `pre_validated: true` only from verb crates via engine bridge
11. TEMP deps documented in Cargo.toml, tracked for S15 removal
12. FetchAux abstraction deferred — concrete reqwest types stay in engine bridge
13. Error format preserved through `From` impls
14. One crate = one reason (10-word rule)
15. No crate >15k LOC, no file >1500 LOC
16. Every side effect behind a trait
17. v0 = zero backward compat
18. Zero unwrap/expect in hot path

## Verification Ritual (after each commit)

```bash
cd tools/
cargo check --workspace 2>&1 | grep "^error" | wc -l       # expect 0
cargo check --workspace --no-default-features 2>&1 | grep "^error" | wc -l  # expect 0
cargo test --workspace --lib 2>&1 | tail -3                # all pass
# After Wave B+: cargo clippy --workspace -- -D warnings
```

## Commit Signature

```
type(scope): description (W14-XN)

Co-Authored-By: Nika 🦋 <nika@supernovae.studio>
```

NEVER Claude co-author. Types: feat, fix, refactor, docs, test, chore. Scopes: kernel, engine, verb-infer, verb-agent, verb-fetch, verb-invoke, runtime.
