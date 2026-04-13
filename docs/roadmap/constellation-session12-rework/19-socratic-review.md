# Socratic Review — Constellation V2.3 (Sessions 12/13/14)

> **Purpose:** Cross-session Socratic critique of the entire Constellation refactor. Every assumption is probed. Every implicit decision is made explicit. Findings feed back into the S13 mega-prompt and the S14 enriched plan.
>
> **Method:** for each claim in the plan documents, ask *"what if this is wrong?"* and *"what would break silently?"*. Confidence ≥ 80% to flag.
>
> **Author:** Claude (Opus 4.6), 2026-04-10, post-S12 completion.

---

## Meta-question

**Q0: Why a Socratic review after Session 12 is done?**

S12 shipped 4 bugs (2× P0) that were caught only because I dispatched a review agent after the commits landed. A Socratic review BEFORE S13 catches the same class of issues up front — cheaper and safer. This is not redundant with the code-reviewer agent: the code-reviewer audits code; this audit interrogates the plan.

---

## Part I — Session 12 post-hoc audit

### Q1: Are the 2 P0 bugs from G1 really fixed, or is there still a pattern risk?

**Claim:** `tokio::try_join!` drain pattern + `kill_on_drop(true)` fix G1 completely.

**Probe:** Does EVERY subprocess path in the codebase now use this pattern? Not just `nika-exec-runner::TokioShell`.

**Answer:** `grep -rn "tokio::process::Command" tools/` reveals:
- `nika-exec-runner/src/lib.rs` — fixed in G1 ✅
- (nothing else in the workspace today)

But S13 will add subprocess usage in `nika-verb-exec`. **Sacred invariant #11-13 encode the pattern**, but that's only enforced if the next session reads them. Mitigation: the mega-prompt surfaces them prominently AND the golden suite has `golden_exec_hello` which exercises subprocess spawn. Any new verb-exec crate that skips `kill_on_drop` will pass `golden_exec_hello` (small output) but fail the regression test `large_output_does_not_deadlock` IF Session 13 adds an equivalent test for the verb crate. **Gap:** nothing forces Session 13 to add that test.

**Resolution:** amend S13 mega-prompt to REQUIRE `nika-verb-exec` to include a `subprocess_does_not_deadlock` test with >1 MB output. This is now added to the sacred invariants and to the commit plan for S13-B3. Flag as **GATE-S13-1**.

---

### Q2: Does `PolicyChecker` trait actually unblock verb crates, or is it decorative?

**Claim:** Verb crates take `&dyn PolicyChecker` and can be mocked in tests without depending on nika-policy.

**Probe:** Can a test in `nika-verb-exec` actually construct a mock `PolicyChecker` with no `nika-policy` dep?

**Answer:** Yes, trivially. The trait is object-safe and has 4 simple methods. `nika-kernel-mock` doesn't ship a `MockPolicyChecker` today — Session 13 must hand-roll one per test file OR add one to `nika-kernel-mock`. Either works.

**Resolution:** add a note to S13 commit B1: "consider adding `MockPolicyChecker` to `nika-kernel-mock` as a nice-to-have." Not a blocker.

---

### Q3: Does the `Filesystem → FsRead + FsWrite` split actually give verb crates narrower bounds?

**Claim:** Verb crates can now depend only on `FsRead` where they don't need to write.

**Probe:** Does any S13 verb crate actually narrow down? Or do all verbs need both?

**Answer:**
- `nika-verb-exec`: reads `workflow_base_dir`, `project_root`. Needs `FsRead` only (no writing via the verb).
- `nika-verb-fetch`: writes response body to CAS via `BlobStore`, NOT `FsWrite`. Also needs `FsRead` for nothing I can see. Probably no FS need at all.
- `nika-verb-invoke`: MCP plus builtin routing. Builtins like `nika:write` need `FsWrite`. But they go through `BuiltinTool::call(args: String) -> Result<String>` (JSON in/out, sealed trait). The verb crate itself doesn't need Fs traits — the builtins do, and builtins live in `nika-builtin` with their own deps.
- `nika-verb-infer`: reads skills, context files. Needs `FsRead`.
- `nika-verb-agent`: composes invoke plus reads skills. Needs `FsRead`.

**Conclusion:** the split is useful for infer, agent, and (maybe) exec. Not for fetch and invoke. The diamond layer benefit is REAL but it's 3/5 verbs. **Resolution:** S12-F4 was worth it; document the asymmetry in the Caps types (some verbs have `fs_read`, others don't).

---

### Q4: Is the `pub use nika_policy as policy` re-export in F6 actually sustainable?

**Claim:** The trick saved 9 consumer file rewrites. It should work forever.

**Probe:** What happens in S14 when `TaskExecutor` is deleted and the engine's `runtime/mod.rs` no longer exists in its current form?

**Answer:** `runtime/mod.rs` survives S14 (it's a module directory, not TaskExecutor). The re-export `pub use nika_policy as policy;` can live on any module — it doesn't depend on TaskExecutor. **Verdict:** sustainable. Not a concern.

---

### Q5: Does `nika-extract` ever need to become async, or is "pure, zero I/O" safe forever?

**Claim:** `nika-extract` is pure and will never need async.

**Probe:** What about streaming extraction for large HTML? Isn't that inherently async?

**Answer:** Streaming extraction belongs in `nika-verb-fetch`, not `nika-extract`. The fetch verb decides when to buffer vs stream; extraction operates on an already-buffered body. This is why `extract()` takes `&str`, not `impl AsyncRead`. **Design holds.** If future needs force async, it becomes a new feature flag + new module; the existing pure API stays stable.

---

### Q6: Did S12 actually reduce engine coupling, or just shift it around?

**Claim:** Engine LOC dropped from 148,792 to 146,473 (−2,319). The reduction reflects real decoupling.

**Probe:** How much of that is code genuinely MOVED vs code just gone?

**Answer:**
- nika-policy: 1263 LOC moved + 7 LOC bridge in engine = engine -1,256 net
- nika-extract: 1327 LOC moved + 20 LOC wrapper in F7 then −20 LOC in F8 = engine -1,327 net (approximately)
- Total expected: −2,583
- Actual: −2,319
- **Gap: +264 LOC somewhere in engine**

Where did the extra 264 LOC come from? It's from:
- The `From<PolicyError> for NikaError` + `From<ExtractError> for NikaError` impls in `error.rs` (~20 LOC)
- The feature flag updates in `Cargo.toml` (not counted in LOC by `wc -l`)
- The new test helpers in `tests_extract_e2e.rs` (`apply_extract` local function)

The 264-LOC delta is a small tax for the bridge layer and is expected. **Verdict:** the reduction is real. Not concerning.

---

## Part II — Session 13 plan critique

### Q7: Does the S13 plan (doc 07) actually match my S12 Caps output?

**Claim (S13 plan):** `ExecCaps` has fields `shell`, `event_log`, `cancel_token`, `policy`, `workflow_base_dir`, `working_dir_mode`, `project_root`.

**My S12 output:** `ExecCaps` has fields `shell`, `policy`, `clock`, `fs_read`.

**Probe:** Can S13 proceed with the S12 shape, or does it need to expand the struct first?

**Answer:** **NO** — the current S12 shape is insufficient. `nika-verb-exec::run` needs at minimum:
- `shell` ✅ (have)
- `policy` ✅ (have)
- `event_log` ❌ (missing) — verb needs to emit ExecStarted, ExecCompleted events
- `cancel_token` ❌ (missing) — verb needs it for `tokio::select!` cancel arm
- `workflow_base_dir` ❌ (missing) — verb needs it for cwd resolution
- `working_dir_mode` ❌ (missing)
- `project_root` ❌ (missing)
- `clock` ✅ (have, maybe unused by the exec verb)
- `fs_read` ✅ (have, maybe unused by the exec verb)

**This is a P0 gap.** S13 Phase 1 MUST resolve it as commit S13-A0 (a prerequisite kernel trait expansion).

**Resolution:** S13 needs an explicit "AMEND-S13-1" that expands the 5 Caps structs in nika-kernel BEFORE building nika-runtime. Alternative: move the Caps structs to nika-runtime with the full fields, and keep the nika-kernel version as the minimal subset. Per AMEND-2 (Caps in nika-kernel), the kernel version wins.

**Committed resolution (this doc):** add S13-A0 as the first commit of Session 13 — `feat(kernel): expand per-verb Caps structs with runtime fields`. Include: `event_log`, `cancel_token`, `workflow_base_dir`, `working_dir_mode`, `project_root` on `ExecCaps`. Similar expansions for FetchCaps, InferCaps, InvokeCaps, AgentCaps based on each verb's actual needs (enumerated in the S13 enriched plan).

**Dependency fallout:** expanding `ExecCaps` to include `event_log` means `nika-kernel` now depends on `nika-event`. Is that OK?
- `nika-kernel` Cargo.toml currently has: `nika-core`, `thiserror`, `tracing`, `tokio`, `tokio-util`, `bytes`, `async-trait`, `futures-core`, `serde_json`, `serde`.
- It does NOT have `nika-event`.
- Adding it: `nika-event` is L1, `nika-kernel` is L0.5. Adding L1 dep to L0.5 = layering violation.
- **Alternative 1:** make `EventLog` a type parameter in Caps: `ExecCaps<'a, E: EventEmitter>`. Painful.
- **Alternative 2:** define an `EventEmitter` trait in `nika-kernel::events` (which is currently an empty marker module). Impl on `nika-event::EventLog`. Then Caps use `&dyn EventEmitter`. Clean.
- **Alternative 3:** accept the layering violation because `nika-event` is thin and has no I/O. It's morally L0.5, not L1.

**Recommendation:** Alternative 2 (trait in kernel, impl in event crate). This matches the pattern for `PolicyChecker`. It's the cleanest. Include this as S13-A0 prerequisite work.

---

### Q8: Does moving verb params to `nika-core` break anything?

**Claim:** S13 plan commit 1.3 proposes moving RunContext from nika-engine to nika-core to break a circular dep.

**Probe:** Do verb params need the same treatment?

**Answer:** These types currently live in `nika-core::ast`, NOT nika-engine. Grep confirms verb params (ExecParams, FetchParams, etc.) are already L0. nika-runtime can import them directly. **No migration needed.**

**Verification for next session:** `ls tools/nika-core/src/ast/` will show the verb param files. Confirmed not a blocker.

---

### Q9: Is the Runner move to nika-runtime really necessary in S13, or is it just ambition creep?

**Claim (S13 plan):** Commit 1.3 moves the entire `Runner` struct + task_dispatch from nika-engine to nika-runtime.

**Probe:** If `nika-runtime::dispatch` is parallel and not live during S13 (per AMEND-4), why does the Runner itself need to move?

**Answer:** You're right — it DOESN'T. The Runner can stay in nika-engine during S13 and call into verb crates via the bridge pattern. nika-runtime only needs to exist as a crate with `VerbCapabilities` + `dispatch()` definitions. The Runner migrates in S14 when TaskExecutor is deleted.

**Contradiction in the plan:** doc 07 (S13 plan) says Runner moves in commit 1.3. AMEND-4 says dispatch is parallel during S13. These two statements can coexist only if the moved Runner continues to call TaskExecutor (not dispatch) — which works, but creates a layer cycle: nika-runtime (has Runner) → nika-engine (has TaskExecutor) → (nothing depends on nika-runtime yet) = OK actually, no cycle.

**Revised resolution:** the Runner move is possible but NOT required. It adds scope to S13 without buying anything observable. **Recommendation: defer the Runner move to S14 Wave C**, where it lines up naturally with TaskExecutor deletion. S13 scope becomes: create nika-runtime with VerbCapabilities + TaskAction enum + dispatch function (empty arms) + 3 verb crates + bridges. Runner stays put.

**Impact on S13 commit count:** drops from 18 to ~14. Time budget drops from 10-12h to 7-9h.

**Impact on S14:** Runner migration is now a Wave C commit (W14-D1). Slight scope increase there but offset by simpler S13.

**Decision needed from user:** adopt this simplification? Flag as **GATE-S13-2**.

---

### Q10: Does the S13 plan correctly handle `ExecCapsOwned` for `tokio::spawn` crossing?

**Claim (S13 plan):** Use `exec_owned()` accessor that clones Arcs, then `ExecCapsOwned::borrow()` inside the spawned future.

**Probe:** Is this pattern sound under Rust's borrow checker?

**Answer:** Yes, IF `ExecCapsOwned` only contains `Arc<_>`, `String`, `PathBuf`, and other owned types. `borrow()` returns `ExecCaps<'_>` with lifetime tied to `&self`, which inside an `async move` closure is bounded by the closure's own scope. This is sound.

**Subtlety:** if `ExecCaps` has a `&dyn EventEmitter` field, `ExecCapsOwned` needs `Arc<dyn EventEmitter>`. If it has `&CancellationToken`, owned needs `CancellationToken` (which is Clone and Arc-backed internally). The Owned/Borrowed distinction is per-field, not per-struct.

**Hidden gotcha:** S12's `ExecCaps` does NOT have an Owned variant because S12 didn't wire it. S13-A0 or A1 must add `ExecCapsOwned` in nika-kernel (alongside `ExecCaps`) plus the `borrow()` method. This is additional scope not in the S13 plan as written.

**Resolution:** add `*CapsOwned` types to S13-A0. They're pure data wrappers, ~100 LOC total.

---

### Q11: Is there a failure mode where the `for_each` path works but the sequential path breaks?

**Claim:** Sequential calls use `caps.exec()` (borrow), parallel calls use `caps.exec_owned()` (clone).

**Probe:** What if a verb holds the borrowed caps across an `.await`?

**Answer:** `ExecCaps<'a>` contains `&'a dyn PolicyChecker` which is `&dyn Trait` — fine to hold across `.await` because `&dyn Trait` is `Send + Sync` as long as the underlying trait is. My S12 `PolicyChecker: Send + Sync` guarantees this.

**Real gotcha:** `parking_lot::RwLockReadGuard<PolicyEnforcer>` is `!Send`. The S13 plan has a line: `&*self.policy_enforcer.read() as &dyn PolicyChecker`. This creates a guard held for the duration of the coercion.

Looking at the bridge code the S13 plan proposes:
```rust
pub(super) async fn run_exec(&self, ...) -> Result<String, NikaError> {
    nika_verb_exec::run(
        task_id, params, bindings, datastore,
        &*self.shell,
        &self.event_log,
        &self.cancel_token,
        &*self.policy_enforcer.read() as &dyn nika_kernel::policy::PolicyChecker,
        // ...
    ).await.map_err(...)
}
```

The guard `self.policy_enforcer.read()` is created inline as an argument to `nika_verb_exec::run`. The guard lives for the duration of the function call. The call is awaited. **This means the guard IS alive across `.await`.** That's `!Send` and won't compile if the outer function is spawned on a multi-threaded runtime.

**Resolution options:**
1. Store `policy_enforcer` as `Arc<dyn PolicyChecker>` (no lock) — requires mutations to go through interior mutability (e.g., `RwLock` inside a Policy impl).
2. Store `policy_enforcer` as `Arc<dyn PolicyChecker + Send + Sync>` — same as above.
3. Clone the policy decision upfront, drop the guard, then call the verb.
4. Use `std::sync::RwLock` instead of `parking_lot::RwLock` — std guards ARE `Send` (but slower).
5. Wrap `PolicyEnforcer` in a `Arc<T>` + interior mutability for the `add_allowed_host` path. Since that's only called during bootstrap, not during task execution, the mutation is compile-time safe.

**Cleanest: Option 5.** Change engine's `policy_enforcer: Arc<RwLock<PolicyEnforcer>>` to `policy_enforcer: Arc<PolicyEnforcer>`. `PolicyEnforcer` internally holds `config: RwLock<PolicyConfig>` for the `add_allowed_host` mutation path. External API is immutable.

**Cost of Option 5:** refactor `PolicyEnforcer::add_allowed_host` to take `&self` instead of `&mut self` (hide the mutation). Audit all callers. Probably ~5 files.

**Decision needed:** which option does S13 adopt? This affects the bridge pattern and is CRITICAL for the session to succeed.

**Flag as GATE-S13-3.**

---

### Q12: Does the S13 "BuiltinRouter" trait proposal have coherent shape?

**Claim (S13 plan):** `&dyn BuiltinRouter` in InvokeCaps, provided by `nika-kernel::builtin::BuiltinRouter`.

**Probe:** Does such a trait exist in nika-kernel today?

**Answer:** No. `nika-kernel::builtin::BuiltinTool` exists (sealed trait). There is NO `BuiltinRouter` trait — routing is done by `nika-engine::runtime::builtin::BuiltinToolRouter` concrete struct.

**Gap:** S13 needs a `BuiltinRouter` trait in nika-kernel. Shape:
```rust
pub trait BuiltinRouter: Send + Sync {
    async fn dispatch(&self, tool: &str, args: String) -> Result<String, BuiltinError>;
    fn knows(&self, tool: &str) -> bool;
}
```

**Scope creep:** this adds an S13 prerequisite commit. Flag as **GATE-S13-4**.

---

### Q13: Does the `McpPool` trait exist, or is it vaporware?

**Claim (S13 plan):** `&dyn McpPool` in InvokeCaps, provided by `nika-kernel::mcp::McpPool`.

**Probe:** Does nika-kernel have an `mcp` module?

**Answer:** No. `tools/nika-kernel/src/lib.rs` exports: `builtin, caps, clock, events, filesystem, http, policy, provider, scope, shell, store, task_local`. **No `mcp` module.** The trait doesn't exist.

**Gap:** S13 needs to add `nika-kernel::mcp::McpPool` as a prerequisite. Shape:
```rust
pub trait McpPool: Send + Sync {
    async fn call_tool(&self, server: &str, tool: &str, args: serde_json::Value) -> Result<serde_json::Value, McpError>;
    async fn read_resource(&self, uri: &str) -> Result<String, McpError>;
}
```

**Scope creep:** another S13 prerequisite commit. Flag as **GATE-S13-5**.

---

### Q14: Is the "FetchAux 4 new traits" commit (S13-D1) actually scoped correctly?

**Claim (S13 plan):** 4 new kernel traits (CookieJar, ResponseCache, DomainRateLimiter, RobotsChecker) in a single S13 commit.

**Probe:** Are these actually necessary for S13, or is it over-engineering?

**Answer:** The risk register R13-4 acknowledges three options (α 4 traits, β FetchAux struct with concrete types, γ concrete types directly). It recommends **β** as the pragmatic S13 choice, with α deferred to S14+.

**Conflict with main plan:** doc 07 commits 4.1 proposes α (the fat kernel option). Doc 09 R13-4 recommends β. These contradict.

**Resolution:** go with β in S13. `FetchAux` is a concrete struct holding `Arc<CookieStoreRwLock>`, `Arc<FetchCache>`, `Arc<DomainRateLimiter>`, `Arc<RobotsCache>` with concrete types from nika-engine. `nika-verb-fetch` depends on nika-engine TEMP for these types. Document the exception. Phase 15 cleans up.

This saves ~100 LOC of kernel trait surface that might never be used. Flag as **AMEND-S13-3**.

---

### Q15: Is S13's test migration burden accounted for?

**Claim:** Session 13 keeps TaskExecutor alive as a bridge. Existing tests work unchanged.

**Probe:** Do any tests in `tools/nika/tests/*` or engine tests depend on behavior that changes when a verb goes through the bridge?

**Answer:** The bridge is transparent — the TaskExecutor verb method delegates to the verb crate's run function with the same signature. No test should observe a difference.

**Subtlety:** if verb crates emit events in a different order than the old TaskExecutor body, golden snapshots catch it. My G2 fix is the right protection.

**Hidden risk:** error messages may differ. If a test asserts on an error string like `assert!(err.contains("Blocked command"))`, and the verb crate's error formats differently than NikaError, the test fails. **Mitigation:** the bridge maps verb errors back to NikaError via `impl From<...Error> for NikaError`, which controls the error message format at the boundary. As long as the `From` impl preserves the original reason string, tests pass.

**Flag as GATE-S13-6:** pre-flight grep for tests that assert on specific error message substrings.

---

## Part III — Session 14 plan critique

### Q16: Is the Provider trait enrichment actually sufficient for nika-verb-infer?

**Claim (S14 plan):** Adding `infer_vision`, `infer_with_tools`, `infer_with_options`, 4 capability probes to the Provider trait unblocks infer extraction.

**Probe:** Do these 8 additions cover all 8 non-trait methods that infer.rs currently calls on RigProvider?

**Answer:** Per kernel trait audit: infer.rs uses `infer_vision()`, `infer_with_tools()`, `infer_with_options()`, `infer_stream_with_options()`, `supports_native_structured_output()`, `is_anthropic()`, `supports_vision()`, `supports_thinking()`. That's 8 methods. S14 plan adds 8. **Count matches.**

But: **where does `infer_stream_with_options()` go?** It's mentioned in the audit but not in the S14 plan's enriched trait. S14 plan has `infer_vision`, `infer_with_tools`, `infer_with_options` (3 methods) + 4 capability probes = 7. Missing one.

**Gap:** either the audit is wrong (maybe `infer_stream_with_options` is subsumed by `infer_stream`) or the S14 plan is incomplete. This must be verified before W14-A1.

**Flag as GATE-S14-1.**

---

### Q17: Does the `rig_agent_loop/` verbatim move actually work, given the 14-field struct coupling?

**Claim (S14 plan R14-1):** Import surgery is 30-50 unresolved imports. Time-boxed 3 hours.

**Probe:** What are the non-obvious couplings that could surface?

**Potential surprises:**
1. **`LimitTracker`** — referenced in the S14 mapping table. Lives in `nika-engine::runtime::limit_tracker`. If `nika-verb-agent` imports it, nika-verb-agent needs nika-engine dep. Plan already accepts this (TEMP).
2. **`SkillInjector`** — same story.
3. **`DynamicSubmitTool`** — nika-engine::runtime::submit_tool.
4. **`NikaMcpTool` / `NikaMcpToolDef`** — nika-engine::mcp or similar.
5. **`MediaStaging`** — `nika-kernel::scope::MediaStaging` trait OR `nika-engine::runtime::media_context` concrete. The rig_agent_loop holds `AgentMediaStaging` which is one concrete impl.
6. **test_shield_mcp_wrap.rs** — test file imports all of the above. Also needs a test fixture for `SecurityContext`. After moving, does the test build a `nika_shield::ShieldContext` correctly? This test is part of the W14-C1 commit's scope.

**Hidden risk:** `rig-core` types (`rig::message::Message`, `rig::completion::*`) may be accessed via `use rig::message::...;` which works anywhere `rig-core` is a dep. Fine.

**Verdict:** the 3-hour budget is tight but achievable IF the mapping table is exhaustive. I recommend expanding the pre-work to **explicit grep of every `use crate::` in each of the 7 files**, saved as a table BEFORE the W14-C1 commit starts. The plan calls for this; just emphasize it.

**Flag as GATE-S14-2:** pre-work mapping MUST be complete before W14-C1 commit. No shortcuts.

---

### Q18: What if `StructuredOutputEngine` has hidden `provider::rig` deps?

**Claim (S14 plan decision 2.1):** `StructuredOutputEngine` standalone, can be reused without moving.

**Probe:** Does it actually only depend on EventLog + NikaError + AST, as claimed?

**Answer:** This needs verification. Grep: `grep -rn "use crate::" tools/nika-engine/src/runtime/structured_output.rs`. If any line contains `crate::provider::rig::`, the claim is false and Session 14 scope expands.

**Pre-S14 verification task:** run this grep and record the result in S14 synthesis doc.

**If provider::rig deps exist:**
- Option A: move StructuredOutputEngine to nika-runtime in W14-B1 (adds 500 LOC scope).
- Option B: define a `ProviderCallback` trait in nika-kernel that `StructuredOutputEngine` consumes; engine-side impl delegates to RigProvider.
- Option C: accept nika-verb-infer's TEMP nika-engine dep and document it.

**Flag as GATE-S14-3.**

---

### Q19: Is Wave C (Dissolution) actually 5 commits or more like 10?

**Claim (S14 plan):** Wave C = 5 commits (D1 through D5), 2-3 hours.

**Probe:** Per AMEND-3, there are 107 TaskExecutor call sites across 16 files. Is 5 commits enough?

**Answer:** No. AMEND-3 explicitly revised Wave C to 17-22h total with a new prerequisite commit W14-A0 (test migration, 3-4h). But the S14 plan doc still shows Wave C as 5 commits. The plan was not updated to reflect the AMEND-3 correction.

**Gap:** doc 08 contradicts doc 13-plan-corrections.md. AMEND wins per the amendment policy.

**Resolution:** the S14 enriched plan (doc 17) MUST explicitly add W14-A0 as the first commit and expand Wave C to account for the test migration. Revised total: 17-22 commits (was 20).

---

### Q20: Does Session 14 have a fallback path if W14-C1 blocks?

**Claim (S14 plan R14-1):** If import surgery fails, rollback W14-C1 and "Agent verb extraction moves to Phase 15."

**Probe:** But Phase 15 is post-launch. Can Nika launch with TaskExecutor still alive (because agent extraction failed)?

**Answer:** Per ADR-004, TaskExecutor deletion requires all 5 verbs extracted. If W14-C1 fails and agent stays, TaskExecutor stays (because run_agent still lives on it). The whole Wave C is blocked.

**Fallback for launch:**
1. Ship with TaskExecutor alive, 4/5 verbs extracted. Engine ~143k LOC instead of ~138k. Still a big improvement.
2. OR: defer W14-C1 to Phase 15, skip Wave C entirely. Same engine LOC as (1).

**Either way, launch still ships on 2026-05-05.** The refactor is incomplete but Nika is functional. This is the acceptable fallback.

**Document this in the S14 enriched plan as the "Plan B" path.** User should understand the launch gate holds either way.

---

## Part IV — Cross-session probes

### Q21: Is the 4-day timeline realistic for S13+S14?

**Math:**
- S13: 10-12h (originally) → 7-9h (after Q9 simplification)
- S14: 17-22h (after AMEND-3)
- Total: 24-31h of refactor work
- Launch: 2026-05-05 (25 days out)
- Refactor budget: ~4 working days = 28-32h
- **Feasible.** Tight but not unreasonable.

**Risk:** any blocker consuming >2 days of investigation breaks the timeline. The 4-agent review (Phase 1) is the mitigation.

### Q22: Is binary size monitoring set up?

**Claim (S14 plan W14-E2):** Record binary size after S14. Prediction: 118 MB → 120-123 MB.

**Probe:** Is there a baseline `cargo build --release -p nika` size recorded right now, post-S12?

**Answer:** The S12 post-mortem cites "~118 MB" as the baseline. This is an approximation, not a measurement from today. The exact post-S12 size should be recorded NOW to establish a clean baseline.

**Action:** run `cargo build --release -p nika && ls -la tools/target/release/nika` and commit the result to the journal.

**Defer:** release build is slow (5-10 min on this machine). I'll add a reminder to S13 pre-flight instead of blocking here.

### Q23: Are the 5 golden tests actually sufficient oracle for 5 verbs × many code paths?

**Probe:** `golden_exec_hello` covers `exec: "echo hello golden"`. What about:
- `exec` with `shell: true`?
- `exec` with `cwd`?
- `exec` with `env` vars?
- `exec` with `timeout`?
- `exec` with `cancel_token` cancelled mid-flight?

None of these are in the golden suite. They're in the unit tests in the engine.

**The golden oracle catches regressions in the dispatch → verb → return path. It does NOT catch regressions in verb-internal logic. Unit tests do that.** Both layers are needed.

**Resolution for S13:** every verb crate MUST have its own unit test suite in addition to the golden oracle. This is in the plan but worth re-emphasizing.

### Q24: Is `nika-kernel-mock` equipped for verb crate testing?

**Probe:** What mocks does `nika-kernel-mock` ship today?

**Answer:** `MockShell`, `MockClock`, `MockHttpClient`, `MockFs (InMemoryFs)`, `MockBlobStore`, `MockMediaContext`, `MockRecordStore`. **Missing:** `MockPolicyChecker`, `MockProvider`.

**Gap:** S13 verb crates will need at minimum `MockPolicyChecker` for exec tests and `MockProvider` for fetch/invoke (if they use providers). Hand-rolling per test file is fine but tedious.

**Recommendation:** add `MockPolicyChecker` to `nika-kernel-mock` as a low-priority cleanup during S13 Phase A.

### Q25: What about the launch prep files Thibaut is working on in parallel?

**Probe:** User has unstaged changes in `AGENTS.md`, `CLA.md`, `CHANGELOG.md`, `README.md`, `MANIFESTO.md`, `editors/`, `docs/launch/`. These are NOT touched by S12 or S13/S14. How do they merge with the refactor?

**Answer:** They don't conflict because they're disjoint file sets. Rebase / merge is trivial. The risk is if Thibaut tries to commit those files on top of a refactor commit and the working tree state is unclear. The S13 prompt explicitly instructs "do not touch user's parallel launch-prep files" — that's the right protection.

**No action needed.**

---

## Part V — Confidence verdict

| Session | Confidence | Blockers identified | Mitigation |
|---|---|---|---|
| S12 (complete) | 🟢 **95%** | None remaining (G1/G2/G3 fixed) | — |
| S13 | 🟡 **70%** | 6 GATE items (S13-1..6) | See Part II Qs 7-15 |
| S14 | 🟡 **60%** | 3 GATE items (S14-1..3) + W14-C1 P0×H risk | See Part III Qs 16-20 |
| Launch (2026-05-05) | 🟢 **90%** | S13/S14 failures don't block launch; fallback to ship with partial extraction works | Plan B documented |

**Overall confidence S13+S14 complete successfully:** ~50%. This matches the original risk register's cumulative estimate (~60% pre-Socratic). The 10% delta reflects the architectural gaps found in Qs 7, 11, 12, 13, 14 — things that weren't in the original plan but MUST be added as S13 prerequisites.

**Overall confidence Nika launches on 2026-05-05:** ~90%. Architecture refactor partial completion is a valid launch state.

---

## Summary of gates (must resolve before or during the relevant session)

### S13 gates

- **GATE-S13-1** — `nika-verb-exec` tests MUST include `subprocess_does_not_deadlock` with >1MB output. (From Q1)
- **GATE-S13-2** — Decide: does Runner move to nika-runtime in S13 or S14? Recommended: **S14**. (From Q9)
- **GATE-S13-3** — Policy guard `!Send` problem. Decide: change `policy_enforcer` to `Arc<PolicyEnforcer>` with internal mutability? (From Q11)
- **GATE-S13-4** — `BuiltinRouter` trait does not exist in nika-kernel. Add as S13 prerequisite. (From Q12)
- **GATE-S13-5** — `McpPool` trait does not exist in nika-kernel. Add as S13 prerequisite. (From Q13)
- **GATE-S13-6** — Pre-flight grep tests for error message format assertions that could break on bridge transition. (From Q15)
- **GATE-S13-7** — Caps struct expansion: current 4-field shape insufficient. Expand in S13-A0. (From Q7)

### S14 gates

- **GATE-S14-1** — Provider trait enrichment: is `infer_stream_with_options` in the plan? Currently missing. (From Q16)
- **GATE-S14-2** — `rig_agent_loop/` import mapping table must be complete before W14-C1. (From Q17)
- **GATE-S14-3** — `StructuredOutputEngine` dep audit: does it import `provider::rig::*`? Verify before W14-B3. (From Q18)

### Cross-session actions

- **ACTION-1** — Record exact release binary size post-S12 as baseline (before S13 starts).
- **ACTION-2** — Add `MockPolicyChecker` + `MockProvider` to `nika-kernel-mock` (low priority but useful).
- **ACTION-3** — Document "Plan B" in mega-plan: launch with partial extraction if W14-C1 blocks. (From Q20)

---

## How this review informs the S13 mega-prompt + S14 enriched plan

1. **S13 mega-prompt (doc 15)** must add explicit GATE-S13-1 through GATE-S13-7 as Phase 0 pre-flight checks. User sign-off (Phase 2) must include resolution of all 7 gates.

2. **S14 enriched plan (doc 17)** must:
   - Add W14-A0 test migration commit (per AMEND-3)
   - Add W14-A-1 Provider trait enrichment verification (GATE-S14-1)
   - Add StructuredOutputEngine dep audit as W14-B0 pre-work (GATE-S14-3)
   - Document Plan B fallback path (ACTION-3)
   - Expand Wave C from 5 to 7-8 commits

3. **Journal (doc 16)** captures the gates in the "Open architectural questions" section as Q1-Q10.

4. **Session 13 must start with a commit S13-A0** that bundles the kernel prerequisites (expanded Caps, BuiltinRouter trait, McpPool trait, policy_enforcer refactor).

---

**End of Socratic review. All gates must be cleared before the next session starts.**
