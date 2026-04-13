# Risk Register — Constellation V2.3 Sessions 12/13/14

> Every known landmine across the 3-session refactor, ranked by severity × likelihood with concrete mitigation plans.

**Last updated:** 2026-04-10

## Severity legend

- **P0** (blocker): if this triggers, main is broken and the session stops
- **P1** (major): if this triggers, the current wave stops but other work can continue
- **P2** (minor): if this triggers, the current commit needs rework but the session proceeds
- **P3** (cosmetic): annoyance, low impact

## Likelihood legend

- **High (H):** plausible on any average session
- **Medium (M):** plausible on a refactor-heavy session
- **Low (L):** edge case, unexpected failure mode

---

## Session 12 (Foundation) — risks

### R12-1 — `nika-policy` crate has a cyclic dep on `nika-engine`

**Severity × Likelihood:** P0 × M
**Scope:** Commits S12-F5, S12-F6 (policy extraction)

**Description:** `PolicyEnforcer` currently lives in `nika-engine/src/runtime/policy.rs` (1263 LOC). When extracted to `nika-policy`, it may import `NikaError`, `RunContext`, or other engine-only types, creating a circular dep: `nika-engine → nika-policy → nika-engine`.

**Detection:** `cargo tree -p nika-policy | grep nika-engine` must be empty after S12-F5 lands.

**Mitigation:**
1. Pre-move audit: `grep -rn "use crate::\(error\|store\|runtime\)" nika-engine/src/runtime/policy.rs` — list every cross-crate import before moving.
2. Define a local `PolicyError` in `nika-policy` (not `NikaError`). Consumers map via `impl From<PolicyError> for NikaError` on the engine side.
3. Any `RunContext` reference becomes a generic parameter or a trait (`PolicyContext` defined in nika-policy).
4. If audit reveals >3 engine imports, redesign: keep PolicyEnforcer as a trait impl in nika-engine but expose `PolicyChecker` trait from `nika-kernel` only. Session 12 still delivers the trait; the impl extraction moves to Session 13.

**Rollback:** `git reset --hard` the S12-F5 commit. Policy extraction becomes optional for Session 12.

---

### R12-2 — `extract.rs` has hidden engine deps

**Severity × Likelihood:** P1 × M
**Scope:** Commit S12-F7 (nika-extract creation)

**Description:** `extract.rs` (1327 LOC) may import `crate::util::*`, `crate::error::NikaError`, or call into `crate::runtime::*`. Verbatim extraction would break.

**Detection:** `grep -rn "use crate::" nika-engine/src/runtime/executor/extract.rs` before the move.

**Mitigation:**
1. Pre-move import audit (documented in Session 12 plan).
2. Copy any used `util::*` helpers into `nika-extract` as local free functions (they're likely pure).
3. Replace `NikaError` with a local `ExtractError`; map at the consumer boundary.
4. If any import calls into `runtime::*` (unlikely — extract is pure), reject the verbatim move and reshape extract first.

**Rollback:** delete the `nika-extract` crate, revert S12-F7 and S12-F8, keep extract in nika-engine.

---

### R12-3 — `HttpClient::send_streaming` breaks existing `ReqwestClient` callers

**Severity × Likelihood:** P1 × L
**Scope:** Commit S12-F2 (HttpClient streaming method)

**Description:** Adding a required method to a trait breaks all existing impls. `ReqwestClient` in `nika-http`, `MockHttpClient` in `nika-kernel-mock`, any test doubles.

**Mitigation:** default impl returning `HttpError::Unsupported`:

```rust
async fn send_streaming(&self, req: HttpRequest) -> Result<HttpStreamResponse, HttpError> {
    Err(HttpError::Unsupported("send_streaming".into()))
}
```

Production `ReqwestClient` overrides the default with a real streaming impl in the same commit. All existing tests continue to pass unchanged.

**Rollback:** remove the default impl; revert commit.

---

### R12-4 — `*Caps` structs don't compile without wiring

**Severity × Likelihood:** P2 × L
**Scope:** Commit S12-F9 (add `ExecCaps`/`FetchCaps`/...)

**Description:** Adding the 5 context struct types to `nika-kernel` without any consumer may leave them flagged as `dead_code` by clippy.

**Mitigation:** add `#[allow(dead_code)]` on the structs with a doc comment: "wired in Session 13"; or create compile-time `Send + Sync` proof tests in the same commit:

```rust
#[cfg(test)]
mod compile_tests {
    use super::*;
    fn _assert_send_sync<T: Send + Sync>() {}
    #[test] fn exec_caps_is_send_sync() { _assert_send_sync::<ExecCaps<'_>>(); }
    // etc.
}
```

The tests use the structs, silencing dead-code warnings.

---

## Session 13 (Extraction Pass 1) — risks

### R13-1 — `EventKind` migration prerequisite

**Severity × Likelihood:** P0 × M
**Scope:** Session 13 start

**Description:** `EventKind` may still live in `nika-engine::event`, meaning every verb crate would need to depend on `nika-engine` just to emit events — circular.

**Detection:** `grep -rn "pub enum EventKind" tools/nika-event/ tools/nika-engine/` before S13 starts.

**Mitigation:**
1. If `EventKind` is already in `nika-event` (L1): no work needed. The code-architect research agent noted this is the case — `nika-engine/src/event/mod.rs` is `pub use nika_event::*`. Confirmed as of 2026-04-10.
2. If not: S13 starts with a prerequisite commit "feat(event): migrate EventKind from nika-engine to nika-event". This is a ~40-variant enum move with all call sites updated. ~1h of mechanical work.

**Rollback:** keep verb crates with a TEMP `nika-engine` dep documented in Cargo.toml.

---

### R13-2 — `for_each` spawn can't cross borrowed `'a` lifetime

**Severity × Likelihood:** P1 × H
**Scope:** Commits S13 verb extractions

**Description:** The Runner's `for_each` path spawns tasks via `tokio::spawn(async move { ... })`. Borrowed `ExecCaps<'a>` cannot cross `spawn` — the borrow doesn't outlive `'static`.

**Mitigation:** provide an owned variant of each caps struct:

```rust
pub struct ExecCapsOwned {
    pub shell: Arc<dyn ShellExecutor>,
    pub policy: Arc<dyn PolicyChecker>,
    pub events: EventLog,  // cheap Clone
    // ... all Arcs and Clones ...
}
impl VerbCapabilities {
    pub fn exec_caps_owned(&self) -> ExecCapsOwned { /* clone Arcs */ }
}
```

The `for_each` scheduler explicitly uses `_owned()` at spawn points. The borrowed variant stays for sequential calls. Cost is visible at the call site.

**Documentation:** both variants must be documented in the same source file, with a comment explaining "use borrowed for sequential, owned for spawn".

---

### R13-3 — `PolicyEnforcer` trait coercion fails at type level

**Severity × Likelihood:** P1 × M
**Scope:** Commit S13 bridge (TaskExecutor → nika-verb-exec)

**Description:** The bridge stage builds `ExecCaps { policy: &*self.policy_enforcer.read() as &dyn PolicyChecker }`. This requires `PolicyEnforcer: PolicyChecker`, which S12 wires in `nika-policy`. If the coercion fails at compile time (e.g., `RwLockReadGuard` doesn't deref to `&PolicyEnforcer` in the right way), the bridge is broken.

**Mitigation:** pre-verify in S12: write a compile-time test in `nika-policy` that coerces a `RwLockReadGuard<PolicyEnforcer>` to `&dyn PolicyChecker`. If it fails, reshape the trait signature before S13.

Alternative: TaskExecutor can hold `Arc<dyn PolicyChecker>` directly instead of `Arc<RwLock<PolicyEnforcer>>` — but that loses the `add_allowed_host` mutation path. Which is acceptable because mutation only happens during construction, not during task execution.

**Rollback:** keep TaskExecutor with concrete `PolicyEnforcer`; use a wrapper type that implements `PolicyChecker` via internal read lock.

---

### R13-4 — `nika-verb-fetch` needs `FetchAux` trait design

**Severity × Likelihood:** P1 × M
**Scope:** Commit S13 fetch extraction

**Description:** Fetch needs cookie jar + ETag cache + rate limiter + robots cache. These can be:
- (α) 4 separate traits in `nika-kernel` (fat kernel)
- (β) A single `FetchAux` struct holding 4 concrete types (kept as concrete, not traits)
- (γ) Concrete types passed directly (no trait layer for fetch aux)

**Mitigation:** Session 13 plan (part 4) defers the decision by trying (β) first as the pragmatic choice. If the verb crate's test boundary becomes painful (can't mock the aux layer), upgrade to (α) in Session 14 or later.

**Accepted tradeoff:** `nika-verb-fetch` may temporarily depend on concrete `RobotsCache`/`FetchCache`/`DomainRateLimiter` types from `nika-engine::runtime::*`. Documented as TEMP with a "Phase 15 cleanup" note.

---

### R13-5 — `Runner::run_task` hot path assumes `TaskExecutor::clone()` is cheap

**Severity × Likelihood:** P1 × L
**Scope:** S13 Runner migration

**Description:** The Runner clones `TaskExecutor` into spawned futures. Replacing with `VerbCapabilities::clone` (or borrowed slices) may alter timing in the hot path. Regressions in workflow benchmarks would surface here.

**Mitigation:** golden e2e tests act as the regression oracle. No perf benchmark gate for S13 (Nika has no bench suite yet; Phase 15+ concern).

---

## Session 14 (Extraction Pass 2 + Dissolution) — risks

### R14-1 — `rig_agent_loop/` import surgery explodes

**Severity × Likelihood:** P0 × H (THE highest-risk step in the whole refactor)
**Scope:** Commit W14-C1 (verbatim move of rig_agent_loop/)

**Description:** Moving 7 files totaling ~2500 LOC from `nika-engine/src/runtime/rig_agent_loop/` to `nika-verb-agent/src/agent_loop/` requires rewriting every `use crate::` import path. Plan estimates 30-50 unresolved imports. Actual may be >100.

**Detection:** after the verbatim move, `cargo check -p nika-verb-agent 2>&1 | grep -c "error\[E0433\]"` counts unresolved imports.

**Mitigation:**
1. **Pre-work (no commit):** Grep every `use crate::` in all 7 files. Build an explicit mapping table:
   ```
   use crate::provider::rig::RigProvider → use nika_engine::provider::rig::RigProvider
   use crate::runtime::SkillInjector     → use nika_engine::runtime::SkillInjector
   use crate::mcp::McpClient             → use nika_engine::mcp::McpClient
   use crate::event::{EventKind, EventLog} → use nika_event::{EventKind, EventLog}
   use crate::runtime::shield::SecurityContext → use nika_shield::ShieldContext
   ```
2. Save the mapping as `docs/plans/constellation-session12-rework/scratch-s14-import-map.md` (delete after C2 commits).
3. **Time-box 3 hours.** If `cargo check` still has >50 errors after the mapping is applied, pause and audit.
4. **Acceptance criterion:** `nika-verb-agent` compiles with a temporary `nika-engine` dep. Document the dep as TEMP in Cargo.toml with Phase 15 cleanup note.

**Rollback:** `git reset --hard` W14-C1. `rig_agent_loop/` stays in nika-engine for S14. Agent verb extraction moves to Phase 15.

---

### R14-2 — `Provider` trait enrichment cascades

**Severity × Likelihood:** P1 × H
**Scope:** Commits W14-A1, W14-A2

**Description:** Adding 5 new methods to the `Provider` trait (including default impls) breaks every existing `Provider` impl: `RigProvider`, `MockProvider` in nika-kernel-mock, any test doubles.

**Mitigation:**
1. All new methods have default impls returning `ProviderError::Unsupported`. Existing impls compile unchanged.
2. `impl Provider for RigProvider` overrides all 5 defaults in W14-A2. This is mechanical because `RigProvider` already has the methods — just delegating.
3. `MockProvider` in nika-kernel-mock may need default override-ers if tests rely on `unwrap_or_else` behavior.
4. Run full workspace tests after W14-A2. Any failures are trait surface mismatches, fix in place.

**Rollback:** remove the new trait methods, revert W14-A1+A2.

---

### R14-3 — `StructuredOutputEngine` drags `nika-engine` into `nika-verb-infer`

**Severity × Likelihood:** P0 × M
**Scope:** Commit W14-B3 (verb-infer structured.rs)

**Description:** `StructuredOutputEngine` lives in `nika-engine/src/runtime/structured_output.rs`. If it imports `crate::provider::rig::*`, `nika-verb-infer` can't re-use it without a nika-engine dep, and the dep cascades through to anything using infer.

**Detection:** `grep -rn "use crate::provider" tools/nika-engine/src/runtime/structured_output.rs` — pre-check before W14-B3.

**Mitigation:**
1. **Pre-check W14-B1:** audit `StructuredOutputEngine` imports. If it only uses `EventLog`, `NikaError`, and AST types: safe to re-use.
2. If it imports `provider::rig::*`: two options:
   - (α) Move `StructuredOutputEngine` to `nika-runtime` in W14-B1 (adds scope)
   - (β) Define a `ProviderCallback` trait in `nika-kernel` that `StructuredOutputEngine` consumes; engine-side impl delegates to RigProvider
3. Accept the TEMP dep if both options are too invasive: `nika-verb-infer` depends on `nika-engine` via Cargo.toml with a Phase 15 cleanup note.

**Rollback:** defer `nika-verb-infer` to Phase 15, keep `infer.rs` in engine.

---

### R14-4 — Golden tests don't go through `Runner::run`

**Severity × Likelihood:** P1 × H
**Scope:** Session 14 prerequisite

**Description:** If golden e2e tests constructed via `TaskExecutor::new()` directly exist (added in Session 12 closure or S13), they break when TaskExecutor is deleted. Migrating them at the same time as deleting TaskExecutor is painful.

**Mitigation:**
1. **Prerequisite check (Session 14 start):** grep `TaskExecutor::new\|TaskExecutor::with_policy` in `tools/nika-engine/src/runtime/` tests. If any exist, they must be rewritten to use `Runner::from_bootstrap` + `Runner::run()` BEFORE any Wave A work.
2. **Session 12 closure task:** ensure any golden tests added in Session 12 use `Runner::run` as the entry point, NOT `TaskExecutor::run_infer` etc.
3. If Session 14 discovers executor-based tests: fix them as a prerequisite (W14-A0 commit).

---

### R14-5 — `nika-shield` move breaks concurrent shield tests

**Severity × Likelihood:** P1 × M
**Scope:** Commit W14-A4

**Description:** `shield.rs`, `spotlight.rs`, `canary.rs` move from `nika-engine/src/runtime/` to `nika-shield/src/`. Tests for these modules (e.g., `tests_shield_spotlight.rs`, `tests_shield_canary.rs` in executor/) must move with the subjects or be rewritten.

**Mitigation:** move tests alongside the subjects. `tests_shield_spotlight.rs` becomes `nika-shield/src/spotlight.rs` test module. `tests_shield_canary.rs` becomes `nika-shield/src/canary.rs` test module.

---

### R14-6 — `CanarySystem` imports `EventKind` via `nika-engine` re-export

**Severity × Likelihood:** P2 × L
**Scope:** Commit W14-A4

**Description:** If `canary.rs` does `use crate::event::EventKind;` (crate = nika-engine), the move to nika-shield requires changing to `use nika_event::EventKind;`. Easy to miss.

**Mitigation:** pre-move grep. Rewrite imports in the same commit that moves the files.

---

### R14-7 — Binary size regression > 5 MB

**Severity × Likelihood:** P3 × M
**Scope:** Commit W14-E2

**Description:** Splitting into 7+ new crates may duplicate generic instantiations, growing the release binary from 118 MB baseline.

**Mitigation:**
1. Prediction: **+2-5 MB** is acceptable (rust-architect agent prediction).
2. **+5 MB or more:** audit for duplicate `serde::Serialize` derivations, duplicate `Arc<dyn Provider>` vtables.
3. LTO + strip should amortize most duplication at release time.
4. If regression exceeds 8 MB: investigate `cargo bloat -p nika --release` and consider `cargo-hakari` workspace-hack adoption (already on the Phase 15 backlog per `project_aggressive_targets_v23.md`).

**Rollback:** none needed — binary size is observational, not a gate.

---

### R14-8 — `decompose.rs` migration breaks DAG expansion

**Severity × Likelihood:** P1 × L
**Scope:** Commit W14-C4

**Description:** `decompose.rs` handles `for_each: decompose:` expansion strategies (semantic, static, nested). Moving to `nika-runtime::decompose::expand()` changes the call path from `self.decompose(...)` to a free function. If the Runner assumes decompose runs within `TaskExecutor` context, the migration breaks it.

**Mitigation:**
1. Pre-move: audit Runner's call sites for decompose.
2. Migration makes decompose take `&McpClientPool, &EventLog` as parameters — explicit. The Runner passes these from its VerbCapabilities.
3. Existing decompose tests move to nika-runtime.

---

## Cross-session risks (spanning all 3)

### R-ALL-1 — Hooks auto-commit interferes with refactor

**Severity × Likelihood:** P2 × M
**Scope:** any commit touching Rust files

**Description:** Pre-commit hooks run clippy and formatting checks. During bulk refactors, a clippy auto-fix may stage unrelated files, contaminating the commit. Memory file `feedback_hooks_auto_commit.md` documents this.

**Mitigation:**
1. **Always use specific file paths with `git add`** — never `git add .` or `git add -A`.
2. If a hook auto-stages unrelated files, unstage them before committing.
3. For high-risk sessions (S14), consider using a worktree so the refactor is isolated from `main`.

---

### R-ALL-2 — Memory file drift during multi-session refactor

**Severity × Likelihood:** P3 × H
**Scope:** any session

**Description:** Session memory files (`project_constellation_session*.md`) must be written at the end of each session. If skipped, the next session lacks context and may repeat decisions.

**Mitigation:**
1. Each session plan has a "close" commit: `chore: session{N} memory + MEMORY.md update`.
2. The close commit is the last commit of the session and is non-skippable.

---

### R-ALL-3 — Golden e2e test infrastructure regression

**Severity × Likelihood:** P0 × M
**Scope:** all sessions

**Description:** If the golden e2e test suite breaks, every subsequent commit loses its safety net. Refactors proceed blind.

**Mitigation:**
1. **Gate criterion:** every commit must run `cargo test --workspace --lib` and have it green. Non-negotiable.
2. If golden tests themselves are broken (not the code under test), STOP and fix the tests before proceeding.
3. Golden tests live in `tests/golden/` or per-crate `src/tests/`. Never in `examples/`.

---

### R-ALL-4 — External contributor or PR lands mid-refactor

**Severity × Likelihood:** P1 × L
**Scope:** any session

**Description:** An external PR may land on main while refactor is in progress, causing merge conflicts in executor/*.rs files being deleted.

**Mitigation:**
1. Freeze `main` PRs during high-risk sessions (S13 extraction, S14 dissolution).
2. If an PR needs to land, rebase the refactor branch onto the new main after.

---

### R-ALL-5 — Launch gate pressure forces scope cuts

**Severity × Likelihood:** P1 × M
**Scope:** all sessions

**Description:** J-25 launch gate (2026-05-05) may force abandoning Session 14 partway through if other launch blockers surface.

**Mitigation:**
1. **Checkpoint between waves.** Session 14 has 4 waves (A/B1/B2/C). After each wave, the state is stable — main compiles, tests pass. If we stop between waves, we can pick up next session.
2. **S12 foundation is the "minimum viable refactor"** — it delivers real wins (nika-policy + nika-extract + kernel trait extensions) even if S13/S14 are deferred to post-launch.
3. **Phase 15 is post-launch by design** — full engine dissolution is not a launch blocker.

---

## Rollback hierarchy

From least to most invasive:

1. **Revert one commit:** `git revert <hash>` — for isolated regression, non-destructive
2. **Revert a wave:** `git reset --hard <wave-start-commit>` — between waves within a session
3. **Revert a session:** `git reset --hard c5ea27438` — pre-S12 HEAD
4. **Full rollback:** `git reset --hard <pre-constellation-commit>` — nuclear option

All destructive rollbacks (level 2+) require explicit user authorization per the sacred invariants.

---

## Probability of success (cumulative)

With the mitigations applied and golden tests as the safety net:

- **Session 12 foundation:** 95% success (well-scoped, low-risk, just landing 10 commits)
- **Session 13 extraction 1:** 85% success (dependent on R13-2 + R13-3 not biting)
- **Session 14 extraction 2:** 70% success (R14-1 import surgery is the wild card)
- **All 3 sessions complete:** ~60% cumulative

**Fallback plan:** if S14 blocks, ship the refactor through S13 (engine ~143k LOC, 4 verb crates extracted). The V2.3 ≤100k target slips to Phase 15, but Nika ships on May 5 with a dramatically cleaner architecture either way.
