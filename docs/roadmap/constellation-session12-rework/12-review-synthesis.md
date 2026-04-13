# Constellation V2.3 — Plan Review Synthesis

> Independent critical review of all 12 plan documents by `feature-dev:code-reviewer` agent, 2026-04-10.

**Overall verdict:** 🟡 **YELLOW** — architecture is correct, ADRs are airtight, but three operational blindspots need resolution before execution.

---

## Top 5 strengths

1. **ADRs are airtight and internally consistent.** ADR-001 correctly rejects `trait Verb` for a closed 5-verb set. ADR-002 correctly rejects `Option<Arc<dyn>>` god-contexts. ADR-003 correctly identifies the purity boundary for extract. ADR-004 correctly frames the god-object problem. None contradict each other.
2. **Bridge pattern is the right strategy** for keeping main green throughout the refactor.
3. **Circular dependency problem is identified and pre-solved** before it bites (Session 13 commit 1.3 resolves it inline).
4. **Reqwest exception is correctly documented** rather than hidden (intentional, flagged with TEMP).
5. **Gate criteria in doc 10 are executable, not aspirational** — paste-able shell commands with expected outputs.

## Top 5 concerns (ranked by blocking severity)

### CONCERN-1 — Golden test infrastructure does not exist (100% confidence, BLOCKING for S13)

Doc 10 requires golden e2e tests as the "regression oracle" for Session 13/14 deletion commits. Doc 08 repeats this requirement. **But no golden suite exists in the codebase today.** `grep -r golden tools/` returns 4 unrelated files, not test infrastructure.

**Consequence:** Sessions 13-14 start without their stated safety net. Deletion commits proceed blind.

**Resolution (GATE-1):** add an explicit Session 12 commit `test(runtime): add golden e2e tests for all 5 verbs via Runner::run` as commit 11 in doc 06. These tests must use `provider: mock`, go through `Runner::run`, and assert output string + event sequence.

### CONCERN-2 — 107 TaskExecutor constructor calls (not 15) (95% confidence, BLOCKING for S14 Wave C)

ADR-004 claims "~15 call sites" for `TaskExecutor::new` / `with_policy`. **Actual count verified by grep: 107 occurrences across 16 files**, of which 88 are in `nika-engine/src/runtime/executor/tests.rs` alone.

**Consequence:** Session 14 Wave C estimated at 2-3 hours for TaskExecutor dissolution is wrong. Realistic budget is 5-8 hours including test migrations.

**Resolution (GATE-3):** update ADR-004 with the correct count. Budget W14-A0 prerequisite at 3-4 hours for test migration. Update Session 14 total estimate: 14-18h → 17-22h.

### CONCERN-3 — ExecCaps definition contradiction between docs 06 and 07 (90% confidence, BLOCKING for S13 start)

- **Doc 06 (Session 12):** `ExecCaps` lives in `nika-kernel/src/caps.rs` with `policy: &dyn PolicyChecker` (ADR-002 design, trait object)
- **Doc 07 (Session 13):** `ExecCaps` re-defined in `nika-runtime/src/capabilities.rs` with `policy: &PolicyEnforcer` (concrete type)

**Consequence:** Verb crates cannot be unit-tested with mock `PolicyChecker` if they receive a concrete `PolicyEnforcer`. The entire point of ADR-002 is compile-time capability enforcement with mockable traits.

**Additional risk:** `VerbCapabilities::exec_caps` accessor using `&*self.policy_enforcer.read` holds a `parking_lot::RwLockReadGuard` across the `a` lifetime. Any future call after an `.await` while holding the guard would deadlock (the guard is not `Send`).

**Resolution (GATE-2):** decide definitively — Caps structs live in `nika-kernel` with `&dyn PolicyChecker`. Update both doc 06 and doc 07 to match.

### CONCERN-4 — Session 14 fallback timeline is implicit (85% confidence, ADVISORY)

Risk register R14-1 rates `rig_agent_loop/` import surgery at P0 × High. Rollback says "Agent verb extraction moves to Phase 15." **But this rollback blows the Session 14 goal**: without agent extraction, TaskExecutor cannot be deleted (it still has `run_agent`), and Phase 15 now owes both agent extraction AND full engine dissolution.

**Consequence:** 40% cumulative failure probability has no documented contingency timeline that still meets the 2026-05-05 launch gate.

**Resolution (GATE-4):** add an explicit "Plan B" section to doc 00 covering the R14-1 fallback. Decide: if W14-C1 blocks, does Nika launch with TaskExecutor still alive?

### CONCERN-5 — dispatch function todo! stubs would break existing tests (82% confidence, BLOCKING for S13)

Session 13 commit 1.2 creates `nika-runtime::dispatch` with 5 `todo!` arms. Commit 2.1 fills the Exec arm, commit 3.1 fills Invoke, commit 4.1 fills Fetch. Infer and Agent arms stay `todo!` until Session 14.

**Consequence:** if `nika-runtime::dispatch` is the live code path during S13, every existing e2e test using `infer:` or `agent:` verbs panics with `unimplemented!`. The full `tests_e2e_workflow.rs` suite would fail.

**Resolution (GATE-4 supplement):** clarify in doc 07 that `nika-engine::task_dispatch` remains the live code path during S13, while `nika-runtime::dispatch` is constructed in parallel and only becomes the live path when Wave B4 of Session 14 wires the last arm.

---

## Pre-execution gate list (MUST be resolved before S12 commits resume)

- **GATE-1 (BLOCKING for S13):** golden e2e tests must be added as explicit S12 commit 11. Not optional.
- **GATE-2 (BLOCKING for S13 start):** `ExecCaps` definition consolidated in `nika-kernel` with `&dyn PolicyChecker`. Both doc 06 and doc 07 updated to match.
- **GATE-3 (BLOCKING for S14 Wave C):** ADR-004 corrected from "~15 call sites" to "107 sites across 16 files". S14 budget revised. W14-A0 scoped explicitly.
- **GATE-4 (ADVISORY):** Session 14 fallback timeline documented in doc 00. dispatch stub strategy clarified in doc 07.
- **GATE-5 (ADVISORY):** `extract.rs` pre-move audit run. ✅ **VERIFIED** — only 2 `use crate::` imports, no `reqwest::` calls, extract is genuinely pure, safe for verbatim move.

## Open questions for Thibaut

1. **Session 14 fallback timeline** — if W14-C1 (`rig_agent_loop/` surgery) blocks, does Nika launch on 2026-05-05 with TaskExecutor still alive (Phase 15 completes it)? Or do we hard-freeze on 2026-05-04 and push launch if needed?

2. **nika-runtime permanence** — is `nika-runtime` the permanent L3 home after Phase 15 engine dissolution, or does it also dissolve into smaller crates?

3. **Crates.io publication** — are `nika-policy` and `nika-extract` `publish = true` at launch (public semver commitment) or `publish = false` (internal only)?

4. **StructuredOutputEngine dep** — has `grep -rn "use crate::provider" tools/nika-engine/src/runtime/structured_output.rs` been run? If yes, what is the result? This changes Session 14 scope significantly if it imports `provider::rig::*`.

5. **4-day timeline reality** — S12+S13+S14 = 28-34h of focused work in 4 days. Are launch-prep activities (CLA, VS Code marketplace, branch protection, etc.) happening in parallel, or is this window dedicated to the refactor?

---

## Verdict on execution

The plan is **executable tonight for Session 12 with GATE-1, GATE-2, GATE-3 corrections applied**. The ADRs stand. The architecture is correct. The risks are understood.

**Not executable without the gate corrections:**
- Session 13 deletion commits without GATE-1 golden tests = no safety net
- Session 13 start without GATE-2 `ExecCaps` consolidation = contradictory types
- Session 14 Wave C without GATE-3 correct test migration budget = timeline miss

**Action:** apply corrections via [`13-plan-corrections.md`](13-plan-corrections.md) before S12 commit 1 lands. Corrections are additive (no rewrite of existing docs) — a separate "amendments" document that the reader consults alongside docs 06-10.

---

**Review agent:** `feature-dev:code-reviewer`
**Dispatched:** 2026-04-10
**Verification method:** full read of all 12 plan docs + grep verification of claimed call counts + grep verification of extract.rs purity
**Confidence:** high for architecture verdict, medium for timeline estimates
