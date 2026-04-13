# Session 14 — Enriched Plan (post-Socratic)

> ⚠️ **SUPERSEDED 2026-04-11** — Session 14 HAS SHIPPED with a radically different scope than this doc assumed. This plan was written 2026-04-10 assuming S13 was not yet done; it targeted 21 commits for Infer + Agent extraction. The actual S14 shipped 5 Wave A–B commits + S14.5 hotfix (W14-B2 engine infer.rs bridge + Wave C agent extraction DEFERRED to S15+).
>
> **For the real S14 record, read:**
> - `20b-session14-scope-correction.md` — scope correction postmortem
> - `16-session-journal.md` (Session 14 entry, lines 287+) — full commit log
> - `23-session15-mega-prompt.md` — canonical S15 doc
>
> **Historical value only.**
>
> ---

> **Purpose:** enriched and corrected Session 14 plan incorporating all findings from the Socratic review (doc 19), AMEND-3 (doc 13), and the Session 12 post-mortem (doc 14). Supersedes doc 08 where they conflict.
>
> **Goal:** Extract `nika-verb-infer` (2157 LOC) + `nika-verb-agent` (602 LOC plus 2500 LOC of agent loop) into their own crates, delete `TaskExecutor` entirely, dissolve `nika-engine` toward a thin shim.
>
> **Precondition:** Session 13 complete — `nika-runtime` exists with `VerbCapabilities` + `dispatch()` wired for Exec/Fetch/Invoke. `nika-policy`, `nika-extract`, `nika-shield`-prerequisites live.
>
> **Budget (revised per AMEND-3 + Socratic):** 21 commits, 17–22h over 2 working days.
> **Launch gate:** 2026-05-05 (J-25 as of 2026-04-10).
>
> **Status:** PLANNED, not started. Depends on S13 completion.

---

## 1. Delta from doc 08

Doc 08 (`08-session14-extraction-2.md`) is the canonical plan. This document enriches it with:

1. **Wave A0 added** — 1 new commit for test migration prerequisite (per AMEND-3: 107 TaskExecutor call sites across 16 files, not the 15 originally claimed).
2. **Wave C expanded** — revised scope for the 107-site migration footprint.
3. **Plan B documented** — explicit fallback if W14-C1 (`rig_agent_loop/` surgery) blocks.
4. **GATE-S14-1, S14-2, S14-3 integrated** — Socratic findings become mandatory pre-wave checks.
5. **Provider trait enrichment re-verified** — ensure `infer_stream_with_options` is included (doc 08 omits it).
6. **Binary size baseline** — record before Wave A, compare after Wave D.
7. **RunContext handling clarified** — whether it migrates in S14 (probably not; defer).

**Revised total:** 21 commits (was 20), 17–22h (was 14–18h).

---

## 2. Wave structure (revised)

| Wave | Commits | Hours | Delta from doc 08 |
|---|---|---|---|
| **Wave A0** — test migration prerequisite | 1 | 3–4 | **NEW** |
| Wave A — Prerequisites | 4 | 3–4 | — |
| Wave B1 — nika-verb-infer | 5 | 4–5 | — |
| Wave B2 — nika-verb-agent | 4 | 4–5 | — |
| **Wave C** — TaskExecutor dissolution | 5 | 2–3 | same commit count, revised scope |
| Wave D — Close | 2 | 1 | — |
| **Total** | **21** | **17–22** | +1 commit, +3–4h |

---

## 3. Wave A0 — Test migration prerequisite (NEW)

### Context

AMEND-3 in doc 13 revised the TaskExecutor call-site count from "~15" (ADR-004 original claim) to **107 sites across 16 files**, of which:
- **Production code (4 sites):** `nika-engine::runtime::runner::mod.rs` (2), `nika-cli::bench.rs` (1), `nika-cli::verbs.rs` (1)
- **Engine tests (94 sites):** `executor/tests.rs` (88), `tests_wiremock.rs` (2), `tests_shield_*.rs` (4)
- **Integration tests in nika crate (8 sites):** across 6 test files

The 94 + 8 = 102 test sites all construct TaskExecutor directly. They cannot be deleted atomically with TaskExecutor. Wave C needs a prerequisite migration.

### W14-A0 — `refactor(tests): migrate TaskExecutor-direct tests to Runner-based helpers`

**Scope:** 102 test sites across 12 test files.

**Strategy:** introduce a `TestRunner` helper in `nika-engine::runtime::runner::test_helpers`. Each test replaces direct TaskExecutor construction with `TestRunner::new`, which internally builds a `Runner` with mock capabilities.

**Files touched:** 13 files total (1 create, 12 modify).

**Time budget:** 3–4 hours of mechanical work.

**Verification:**
- `cargo test --workspace --lib` zero regressions
- Grep confirms no test outside test_helpers constructs TaskExecutor directly

---

## 4. Wave A — Prerequisites (4 commits)

### W14-A1 — `feat(kernel): enrich Provider trait` (GATE-S14-1)

**Gate check:** before starting, verify `infer_stream_with_options` is in the enriched trait. Grep the engine's infer.rs for all non-trait RigProvider method calls. Expected list:

1. `infer_vision()`
2. `infer_with_tools()`
3. `infer_with_options()`
4. `infer_stream_with_options()` ← **add to plan if missing from doc 08**
5. `supports_vision()`
6. `supports_native_structured_output()`
7. `supports_thinking()`
8. `is_anthropic_compatible()` (renamed from `is_anthropic()`)

**File:** `tools/nika-kernel/src/provider.rs`

All new methods have default impls returning `ProviderError::Unsupported`. Existing impls compile unchanged.

`InferOptions`, `ToolDef`, `ToolChoice` move from `nika-engine::provider::rig::inference` into `nika-kernel::provider`. Pure data, no I/O.

**TDD:** 3–4 unit tests for default-impl contracts.

### W14-A2 — `refactor(engine): impl Provider for RigProvider — fill enriched trait`

Fill all new trait methods on `impl Provider for RigProvider`, delegating to the existing concrete methods. Rename concrete methods to `*_inner` so trait methods shadow them without name collision.

**Verification:** `cargo test --workspace --lib` green + golden tests green.

### W14-A3 — `feat(runtime): ProviderRegistry trait + ProviderRegistryImpl`

**Files:**
- Create: `tools/nika-runtime/src/provider_registry.rs`
- Modify: `tools/nika-runtime/src/capabilities.rs` (add `provider_registry` field to `VerbCapabilities`)
- Modify: `tools/nika-engine/src/runtime/executor/mod.rs` (TaskExecutor delegates provider lookup to ProviderRegistryImpl)

`ProviderRegistry` is a trait in nika-runtime. `ProviderRegistryImpl` wraps the DashMap cache + custom endpoints. TaskExecutor's `get_rig_provider()` becomes a thin wrapper on `registry.get()`. No behavior change.

**Verification:** existing provider-cache tests still green.

### W14-A4 — `feat(shield): create nika-shield L1 crate`

**Files:**
- Create: `tools/nika-shield/Cargo.toml`
- Create: `tools/nika-shield/src/lib.rs`
- Create: `tools/nika-shield/src/context.rs` (moved from engine shield.rs)
- Create: `tools/nika-shield/src/spotlight.rs` (moved)
- Create: `tools/nika-shield/src/canary.rs` (moved)
- Modify: `tools/Cargo.toml` workspace members
- Modify: engine's shield.rs / spotlight.rs / canary.rs → shim re-exports

**Pre-flight grep (MANDATORY):**
1. `grep -rn "use crate::event" tools/nika-engine/src/runtime/canary.rs tools/nika-engine/src/runtime/spotlight.rs` — change `crate::event::EventKind` to `nika_event::EventKind`.
2. `grep -rn "crate::error" tools/nika-engine/src/runtime/shield.rs tools/nika-engine/src/runtime/canary.rs tools/nika-engine/src/runtime/spotlight.rs` — if they touch `NikaError`, extract local error types to break the dep.

**Verification:**
```
cargo test -p nika-shield --lib
cargo test --workspace --lib
cargo tree -p nika-shield --edges normal | grep -v "nika-core\|nika-event"
```

---

## 5. Wave B1 — nika-verb-infer (5 commits, ~4–5h)

### W14-B0 — Pre-work: StructuredOutputEngine dep audit (GATE-S14-3, no commit)

Before W14-B1, run:
```
grep -rn "use crate::" tools/nika-engine/src/runtime/structured_output.rs
grep -rn "use crate::provider" tools/nika-engine/src/runtime/structured_output.rs
```

**Decision tree:**
- If `provider::rig::*` imports = ZERO: safe, `nika-verb-infer` re-uses via nika-engine's pub API. Proceed to W14-B1 as planned.
- If `provider::rig::*` imports exist:
  - **Option A:** move `StructuredOutputEngine` to `nika-runtime` in W14-B1 (adds ~500 LOC scope).
  - **Option B:** define `ProviderCallback` trait in `nika-kernel`; engine-side impl delegates to RigProvider.
  - **Option C:** accept `nika-verb-infer`'s TEMP `nika-engine` dep and document it with Phase 15 cleanup note.

**Recommendation:** Option C (pragmatic, matches S14's already-accepted TEMP engine deps).

### W14-B1 — `feat(verb-infer): create nika-verb-infer crate skeleton`

**Files:**
- Create: `tools/nika-verb-infer/Cargo.toml`
- Create: `tools/nika-verb-infer/src/lib.rs`
- Create: `tools/nika-verb-infer/src/error.rs`
- Create: `tools/nika-verb-infer/src/caps.rs` (re-export InferCaps from nika-runtime)
- Create stub modules: `prompt.rs`, `vision.rs`, `guardrails.rs`, `callbacks.rs`, `structured.rs`, `run.rs`

**Justify the `nika-engine` dep explicitly in the file's doc comment:**
> "TEMP: re-uses `StructuredOutputEngine` and `provider::rig` types until Phase 15 extracts them."

### W14-B2 — `feat(verb-infer): implement prompt/vision/guardrails/callbacks modules`

Move from infer.rs:
- **prompt.rs** (lines 105–377): Spotlight wrapping, template resolve, skills injection, canary injection, schema loading, context assembly.
- **vision.rs** (lines 1647–1910): vision inference path + `detect_image_media_type` helper. Takes `Arc<dyn BlobStore>` + `Arc<dyn Provider>` (enriched).
- **guardrails.rs** (lines ~1960–2000): 4 guardrail types (length, schema, regex, llm). Pure eval.
- **callbacks.rs** (lines 48–72): inference callback builder now takes `Arc<dyn Provider>` not `&RigProvider`.

### W14-B3 — `feat(verb-infer): implement structured.rs + main run() function`

**structured.rs:** thin wrapper around `StructuredOutputEngine` (reused from nika-engine pub API per W14-B0 decision).

**run.rs:** main `pub async fn run()`. Mirrors the current infer verb body but calls free functions. Target: ~300 LOC vs 2157 monolith.

### W14-B4 — `feat(runtime): wire dispatch() TaskAction::Infer arm`

Fill the Infer arm in `nika-runtime::dispatch`. Add `infer_caps()` builder on `VerbCapabilities`.

### W14-B5 — `refactor(engine) + chore: bridge + delete infer.rs (-2157 LOC)`

**Two-step** (bridge first, delete second, both in the same commit for atomicity):

1. The TaskExecutor infer method becomes a bridge to `nika_verb_infer::run()`.
2. Run full test suite + golden. Must be green.
3. Delete `nika-engine/src/runtime/executor/infer.rs` — mod declaration removed, file deleted.

**Verification:**
```
find tools/nika-engine/src -name "*.rs" | xargs wc -l | tail -1
# EXPECTED: ~141,600 (down from ~143,800 S13 end)
cargo test --workspace --lib
cargo clippy --workspace --lib -- -D warnings
cargo test -p nika-engine --lib runner::tests_golden_verbs
```

---

## 6. Wave B2 — nika-verb-agent (4 commits, highest risk)

### Pre-work W14-C0 — Import-path mapping table (NO COMMIT, ~30–45min) (GATE-S14-2)

Before ANY code change, grep every file in `rig_agent_loop/` for `use crate::`:

```
for f in tools/nika-engine/src/runtime/rig_agent_loop/*.rs; do
  echo "=== $f ==="
  grep -n "^use crate::" "$f"
done > /tmp/s14-import-audit.txt
```

Build the mapping table and save as `docs/plans/constellation-session12-rework/scratch-s14-import-map.md`.

**Expected count:** 30–50 distinct imports across 7 files. **If grep finds >80, STOP and audit before W14-C1.**

### W14-C1 — `feat(verb-agent): create crate + verbatim move of agent_loop/`

**HIGHEST RISK COMMIT OF THE WHOLE REFACTOR.** Time-box **3 hours**. If `cargo check` errors exceed 100 after mapping applied, execute Plan B (section 9).

**Files:**
- Create: `tools/nika-verb-agent/Cargo.toml`
- Create: `tools/nika-verb-agent/src/lib.rs`
- Create: `tools/nika-verb-agent/src/error.rs`
- Create: `tools/nika-verb-agent/src/caps.rs`
- Create: `tools/nika-verb-agent/src/run.rs` (stub, filled in C2)
- Create: `tools/nika-verb-agent/src/decompose.rs` (moved)
- Create: `tools/nika-verb-agent/src/agent_loop/` (7 files, verbatim from engine with paths rewritten)

**Do not run tests after this commit** — `run.rs` is still a stub, compile is all we want.

**Verification:** `cargo check -p nika-verb-agent`. Expect 30–50 errors from the mapping; apply the table iteratively. When `cargo check` passes, commit.

### W14-C2 — `feat(verb-agent): implement run.rs`

Mirrors the current agent verb body (~602 LOC) calling the moved `crate::agent_loop::RigAgentLoop`.

**Verification:**
```
cargo test -p nika-verb-agent --lib
# The ~70 rig_agent_loop tests that used to live in nika-engine now run in nika-verb-agent.
```

### W14-C3 — `feat(runtime): wire dispatch() TaskAction::Agent arm`

Fill the Agent arm in `nika-runtime::dispatch`. Add `agent_caps()` builder on `VerbCapabilities`.

### W14-C4 — `refactor(engine) + chore: delete agent.rs + decompose.rs + rig_agent_loop/`

**Two-step:**
1. The TaskExecutor agent method delegates to `nika_verb_agent::run()`. Golden tests pass.
2. Delete `executor/agent.rs` (−602 LOC), `executor/decompose.rs` (−352 LOC), entire `rig_agent_loop/` directory (~2500 LOC). **Total: −3454 LOC from nika-engine.**

**Verification:**
```
find tools/nika-engine/src -name "*.rs" | xargs wc -l | tail -1
# EXPECTED: ~138,100 (down from ~141,600 W14-B5 end)
cargo test --workspace --lib
cargo test -p nika-engine --lib runner::tests_golden_verbs
```

---

## 7. Wave C — TaskExecutor dissolution (5 commits, revised)

### W14-D1 — `refactor(runtime): Runner builds VerbCapabilities directly, no TaskExecutor`

**Gate:** per GATE-S13-2 (Socratic Q9), the Runner may or may not have moved to nika-runtime in S13. Handle both cases:
- **If Runner is still in nika-engine at S14 start:** move it to nika-runtime in W14-D1 (adds scope, ~500 LOC).
- **If Runner is already in nika-runtime (S13 did the move):** just update it to build VerbCapabilities directly.

Runner currently holds `executor: TaskExecutor`. After all 5 verbs delegate through dispatch(), Runner constructs `VerbCapabilities` once at startup and passes to `dispatch()` directly. TaskExecutor becomes a thin wrapper around VerbCapabilities (zero logic), then is deleted in W14-D2.

### W14-D2 — `chore(engine): delete TaskExecutor struct + constructor logic`

**Files:**
- Modify: `tools/nika-engine/src/runtime/executor/mod.rs` — delete `TaskExecutor` struct and all 8 `with_*` builder methods (~300 LOC of constructor logic)
- Move shared helpers (`estimate_tokens`, `strip_think_tags`, `redact_for_event`, etc.) from `verbs.rs` to `nika-runtime::util` (or a new shared util crate)

**Verification:** full workspace test suite green.

### W14-D3 — `chore(engine): delete runtime/executor/ directory`

After W14-D2, the `executor/` directory should only contain residual test shells. Migrate any remaining test infrastructure to each verb crate's test module OR `nika-runtime::tests`. Delete the directory.

**Engine LOC delta:** −1000 to −1500 LOC.

### W14-D4 — `chore(engine): remove shield re-export shims from runtime/`

Delete `nika-engine/src/runtime/shield.rs`, `spotlight.rs`, `canary.rs` (the shim re-exports from W14-A4). Any code still importing them directly switches to `nika_shield::*`. Grep-and-replace.

### W14-D5 — `chore(workspace): nika-engine marked as thin shim in Cargo.toml + doc`

Update `tools/nika-engine/Cargo.toml` and `src/lib.rs` doc comment:
```
# nika-engine (dissolution phase 14 — targets Phase 15 for full deletion)
# Residual content:
#   - provider/rig/               concrete RigProvider
#   - runtime/boot.rs             BootSequence, BootPhase
#   - runtime/structured_output.rs
#   - runtime/chat_workflow.rs
# Target LOC after S14: <30k (from 148k pre-Constellation)
```

---

## 8. Wave D — Close (2 commits, ~1h)

### W14-E1 — `docs(constellation): ARCHITECTURE.md — S14 complete`

Update:
- Crate count: 28 → **36** (S12: +2, S13: +4, S14: +3: nika-shield, nika-verb-infer, nika-verb-agent)
- Engine LOC: 148,792 → **~138,100** (−10,692 net)
- 5 verb crates exist and are wired through `dispatch()`
- TaskExecutor deleted
- nika-shield added to the diamond diagram
- Mention nika-engine is in thin-shim dissolution state

### W14-E2 — `chore: session14 memory + binary size record`

Run `cargo build --release -p nika` and record binary size. Compare to Session 12 baseline (**118 MB**).

**Prediction:** 118 MB → 120–123 MB.

**If it grows >5 MB:** audit with `cargo bloat -p nika --release`. Consider `cargo-hakari` workspace-hack adoption (Phase 15 backlog).

---

## 9. Plan B — if Wave B2 blocks

**Trigger:** W14-C1 import surgery fails (>100 errors after mapping applied, or hidden coupling not in the table).

**Contingency path:**

1. **Rollback W14-C1:** `git reset --hard <before-C1>`
2. **Skip Wave B2 entirely.** Agent extraction deferred to Phase 15+.
3. **Continue with Wave C PARTIAL:**
   - W14-D1: Runner builds VerbCapabilities BUT still holds a reference to TaskExecutor for the agent verb path only.
   - W14-D2: delete TaskExecutor's 4 non-agent verb methods but KEEP the agent verb method. Narrow the 22-field struct to the subset needed by agent only.
   - Skip W14-D3 (can't delete executor/ dir; agent verb still lives there).
   - W14-D4: still remove shield shims (nika-shield is independent of agent).
   - W14-D5: doc comment acknowledges "agent verb not yet extracted; Phase 15+"
4. **Wave D commits adjusted:**
   - W14-E1: ARCHITECTURE.md reflects 4/5 verbs extracted, TaskExecutor partial.
   - W14-E2: memory file documents the Plan B path + Phase 15 commitment.

**Engine LOC after Plan B:** ~141,000 instead of ~138,100. Delta: +2,900 LOC (agent + partial executor).

**Crate count after Plan B:** 35 (no nika-verb-agent).

**Launch impact:** none. Nika still ships on 2026-05-05 with a partial but functional refactor.

**Post-launch commitment:** Phase 15 completes the agent extraction. Estimated 1–2 days of focused work after launch.

---

## 10. 22-field migration table

From ADR-004 (doc 05). Maps each TaskExecutor field to its destination.

| Field | Destination | Notes |
|---|---|---|
| `http_client: reqwest::Client` | **DELETED** | Superseded by `Arc<dyn HttpClient>` |
| `rig_provider_cache: Arc<DashMap<...>>` | **DELETED** | Superseded by `Arc<dyn ProviderRegistry>` |
| `mcp_pool: McpClientPool` | VerbCapabilities | May become `Arc<dyn McpPool>` per GATE-S13-5 |
| `default_provider: Arc<str>` | ProviderRegistry | Internal |
| `default_model: Option<Arc<str>>` | ProviderRegistry | Internal |
| `event_log: EventLog` | VerbCapabilities | Cheap Clone |
| `builtin_router: Arc<BuiltinToolRouter>` | VerbCapabilities | May become `Arc<dyn BuiltinRouter>` per GATE-S13-4 |
| `policy_enforcer: Arc<RwLock<PolicyEnforcer>>` | VerbCapabilities | **GATE-S13-3:** change to `Arc<PolicyEnforcer>` with interior mutability |
| `cancel_token: CancellationToken` | VerbCapabilities | |
| `cas: Arc<CasStore>` | **DELETED** | Superseded by `Arc<dyn BlobStore>` |
| `tool_ctx: Arc<ToolContext>` | Inside BuiltinRouter | Not top-level cap |
| `skill_injector: Arc<SkillInjector>` | VerbCapabilities | |
| `skills_map: Arc<HashMap<String, String>>` | VerbCapabilities | |
| `workflow_base_dir: PathBuf` | VerbCapabilities | |
| `skills_base_dir: PathBuf` | VerbCapabilities | |
| `project_root: Option<PathBuf>` | VerbCapabilities | |
| `working_dir_mode: Option<String>` | VerbCapabilities | |
| `custom_endpoints: Arc<CustomEndpointMap>` | ProviderRegistry | Internal |
| `resolved_agents: Arc<ResolvedAgents>` | VerbCapabilities | Agent verb only |
| `robots_cache: Option<Arc<RobotsCache>>` | Inside FetchAux | Fetch verb only |
| `domain_rate_limiter: Option<Arc<DomainRateLimiter>>` | Inside FetchAux | |
| `cookie_jar: Arc<CookieStoreRwLock>` | Inside FetchAux | |
| `fetch_cache: Arc<FetchCache>` | Inside FetchAux | |
| `workflow_tasks: Arc<Vec<AnalyzedTask>>` | RunContext | Run-scoped data, not cap |
| `shield: SecurityContext` | VerbCapabilities (as `nika_shield::ShieldContext`) | Via nika-shield |

**Distribution summary:**
- **VerbCapabilities:** 13 fields
- **ProviderRegistry:** 4 fields (internal)
- **FetchAux:** 4 fields (internal to fetch)
- **BuiltinRouter:** 1 field (internal)
- **RunContext:** 1 field
- **DELETED:** 3 fields (superseded by trait objects)

---

## 11. Gates from Socratic review

These must be resolved BEFORE or DURING the relevant wave:

| Gate | Wave | Resolution |
|---|---|---|
| **GATE-S14-1** — Provider trait enrichment must include `infer_stream_with_options` | W14-A1 | Added to trait sketch above |
| **GATE-S14-2** — rig_agent_loop/ import mapping table complete before W14-C1 | W14-C0 (pre-work) | Time-boxed 30–45 min |
| **GATE-S14-3** — StructuredOutputEngine dep audit | W14-B0 (pre-work) | Option C recommended |
| **GATE-S13-3 carry-over** — policy_enforcer `!Send` issue | Must be fixed in S13 (blocks S14) | `Arc<PolicyEnforcer>` with interior mutability |

---

## 12. Done criteria

- [ ] All 21 commits landed (20 original + 1 W14-A0 test migration)
- [ ] `cargo test --workspace --lib` green (target: ~10,850 tests)
- [ ] `cargo clippy --workspace --lib -- -D warnings` clean
- [ ] `cargo build --release -p nika` green, binary size recorded and compared to 118 MB baseline
- [ ] Engine LOC: ~138,100 (target ~10k below S13)
- [ ] Crate count: 32 (post-S13) → **35 or 36**
- [ ] `tools/nika-engine/src/runtime/executor/` directory deleted (or partial per Plan B)
- [ ] All 5 verb crates exist and are wired through `nika-runtime::dispatch()` (or 4/5 per Plan B)
- [ ] Golden regression suite still passing byte-for-byte
- [ ] Per-verb crate unit tests added
- [ ] ARCHITECTURE.md updated
- [ ] `project_constellation_session14.md` created in user memory
- [ ] User authorized push
- [ ] `git push origin main` completed

---

## References

- [`08-session14-extraction-2.md`](08-session14-extraction-2.md) — original S14 plan (superseded by this doc where they conflict)
- [`05-adr-004-delete-task-executor.md`](05-adr-004-delete-task-executor.md) — ADR for TaskExecutor deletion
- [`09-risk-register.md`](09-risk-register.md) — landmines R14-1 through R14-8
- [`13-plan-corrections.md`](13-plan-corrections.md) — AMEND-3 applied in this doc
- [`14-session12-handoff-postmortem.md`](14-session12-handoff-postmortem.md) — S12 post-mortem
- [`15-session13-mega-prompt.md`](15-session13-mega-prompt.md) — S13 mega-prompt
- [`16-session-journal.md`](16-session-journal.md) — chronological journal
- [`19-socratic-review.md`](19-socratic-review.md) — Socratic cross-session review

---

**End of Session 14 enriched plan. Status: PLANNED, not started. Precondition: Session 13 complete.**
