# Constellation Session Journal

> **Purpose:** Chronological log of every commit, bug, fix, test added, and improvement across the Constellation V2.3 refactor. Seeded with Session 12 in full; Session 13 and 14 are filled in as they happen.
>
> **Why a journal and not just git log:** git log captures what. This journal captures **what, why, what it cost, what was learned**. Future sessions read this before touching related code.
>
> **Status:** Session 12 complete (**22 commits**, all pushed to `origin/main`). Session 13 pending fresh-session start. Session 14 planned.
> **Last updated:** 2026-04-10 (post-Socratic review + G3 push)
> **HEAD:** `d7ac1031b` (S12-G3)
> **Docs written after the post-mortem:** `16-session-journal.md` (this file), `17-session14-enriched-plan.md`, `19-socratic-review.md`

---

## Legend

- 🟢 **Clean** — landed without incident
- 🟡 **Warning** — landed but a caveat was found (documented)
- 🔴 **Bug shipped** — caught in review after commit (G-series fixes)
- 🐛 **Bug fix** — corrects a prior commit
- 🛡️ **Security** — security-relevant change
- 📏 **Metrics** — LOC / test / binary changes

---

## Session 12 — Foundation

**Goal:** extend kernel trait surface + extract 2 pure crates + define per-verb Caps + add golden regression oracle.
**Plan reference:** `06-session12-foundation.md` + `13-plan-corrections.md`
**Outcome:** 21 commits, all green, pushed to origin/main. Post-review found 2 P0 bugs + 1 P2 warning; all fixed.

### Phase D/P/E (pre-Foundation, 2026-04-09)

| SHA | Subject | Status | Notes |
|---|---|---|---|
| `3897870e8` | `fix(engine): hard error on agent file tools cwd lookup failure (S12.D1)` | 🟢🛡️ | Eliminated silent fallback on cwd resolve failure |
| `c92075fe2` | `fix(builtin): reject '..' in glob patterns (S12.D2)` | 🟢🛡️ | **TDD reproduced real vault.enc leak** via `../../../.nika/secrets/vault.enc` glob. Hardened path validation. |
| `2381c491c` | `perf,fix(builtin): cache canonicalized working_dir + hard error (S12.D3+E1)` | 🟢🛡️ | Removed fallback + amortized repeated canonicalize cost |
| `ffdfa770d` | `refactor(kernel,builtin): current_is_tainted() + BuiltinError::denied() (S12.P1)` | 🟢 | Extracted helper, unified error construction |
| `30f7a67a5` | `refactor(builtin): file/limits.rs — unify MAX_*_BYTES on u64 (S12.P2)` | 🟢 | Type unification |
| `1c5d48954` | `refactor(builtin): file/test_util.rs shared run_as() test helper (S12.P3)` | 🟢 | DRY test helpers |

### Phase Foundation (2026-04-10)

| SHA | Subject | Status | LOC Δ | Tests | Notes |
|---|---|---|---|---|---|
| `870571f74` | `feat(kernel): add PolicyChecker trait (S12-F1)` | 🟢 | +94 | +4 | Object-safe trait, 4 methods. First foundation commit. |
| `2b5908bca` | `feat(kernel): HttpClient::send_streaming + HttpStreamResponse (S12-F2)` | 🟢 | +100 | +3 | Additive default → `HttpError::Unsupported`. No existing impl breaks. |
| `f235b9913` | `feat(kernel): cancellation support in ShellExecutor (S12-F3)` | 🔴 | +142,-45 | +2 | **Two P0 bugs shipped, caught in G1:** (a) sequential pipe drain → deadlock on >64KB stdout, (b) no `kill_on_drop` → zombie children on cancel/timeout. See G1 below. |
| `82d43cc17` | `feat(kernel): split Filesystem into FsRead + FsWrite splinters (S12-F4)` | 🟢 | +172,-56 | +2 | Blanket impl preserves existing `dyn Filesystem` (zero consumers found). |
| `2d9ae1a3a` | `feat(policy): create nika-policy L1 crate (S12-F5)` | 🟢 | +1,409,-48 | +36+1 | 1263-LOC PolicyEnforcer move. PolicyConfig migrated to nika-core. `impl PolicyChecker for PolicyEnforcer` via `map_decision` bridge (kernel `PolicyDecision` vs local `PolicyDecision` distinct types). |
| `f2632a0b9` | `chore(engine): delete duplicated policy code (S12-F6)` | 🟢 | +7,-1,264 | -36 | `pub use nika_policy as policy;` trick → 2-file change instead of rewriting 9 consumers. Very clean. |
| `719b26522` | `feat(extract): create nika-extract L2 crate (S12-F7)` | 🟡 | +1,450,-1,307 | +42+3 | Pure 9-mode pipeline. Thin wrapper kept in engine temporarily. Had `--no-default-features` warning (P2), fixed in G3. |
| `f5068768b` | `chore(engine): delete runtime/executor/extract.rs (S12-F8)` | 🟢 | +17,-53 | 0 | Rewired fetch.rs call sites to `nika_extract::extract()` directly. `.map_err(Into::into)` at one site where `merge_link_hreflang` needs `NikaError`. |
| `c6a0d78bc` | `feat(kernel): add 5 per-verb Caps structs (S12-F9)` | 🟡 | +118 | +5 | Types only, not wired. `Arc<dyn Provider>` asymmetry vs `&dyn Trait` (intentional). `#[non_exhaustive]` on all 5 structs. **Caveat:** shapes do NOT match the original S13 plan — the plan wanted event_log/cancel_token/workflow_base_dir fields; my S12 Caps have minimal fields only. This MUST be resolved in S13 Phase 1 review. See Journal section "Open architectural questions". |
| `f288baa43` | `docs(constellation): ARCHITECTURE.md + session12 memory update (S12-F10)` | 🟢 | +37,-8 | 0 | Updated engine ARCHITECTURE.md with new crates + commit summary. |
| `2cb4f0bcb` | `test(runtime): golden e2e regression tests for 5 verbs (S12-F11)` | 🔴 | +254 | +5 | AMEND-1 golden suite. **Shipped too weak** — lifecycle only, no output. G2 strengthened. |

### Phase Gap fixes (post-review, 2026-04-10)

| SHA | Subject | Type | LOC Δ | Tests | Root cause |
|---|---|---|---|---|---|
| `8ef810b9f` | `fix(exec-runner): pipe buffer deadlock + kill_on_drop (S12-G1)` | 🐛🔴 | +59,-17 | +1 | **Two bugs in one commit.** (1) F3 refactor unified timeout/no-timeout paths through one `child_fut` that read stdout/stderr sequentially after `wait()`. Pre-F3 the no-timeout path used `wait_with_output()` which drains concurrently. Any command producing >~64KB output → pipe buffer full → child blocks on write → `wait()` blocks forever → deadlock. (2) `TokioShell` never called `cmd.kill_on_drop(true)`. On cancel/timeout arm fire in `tokio::select!`, `Child` drops without killing → zombie process. Found by feature-dev:code-reviewer agent in independent review. Fix: `tokio::try_join!(child.wait(), drain(stdout), drain(stderr))` + `cmd.kill_on_drop(true)`. Regression test: 1 MB pipe via `yes \| head -c 1048576`. |
| `304b1d3c2` | `test(runtime): golden tests now assert output content (S12-G2)` | 🐛🔴 | +128,-85 | 0 | AMEND-1 of the plan required "assert output content AND event sequence". F11 initial commit only snapshotted the workflow lifecycle (WorkflowStarted → Task* → WorkflowCompleted). A verb extraction commit that produced correct lifecycle but corrupted output (wrong value, missing field, shape change) would have passed silently. Fix: `golden_snapshot()` helper now captures BOTH lifecycle AND `runner.datastore().get(task_id).output_str()`. Snapshots updated (5 deleted, 5 new). Real output now visible: exec → `"hello golden"`, invoke → `{"logged":true,...}`, infer → full mock JSON structure. |
| `d7ac1031b` | `fix(extract): silence unused_variables on all fetch-* features disabled (S12-G3)` | 🐛🟡 | +15 | 0 | `nika-extract` with `--no-default-features` warned on unused `base_url` parameter. Workspace clippy with default features masks it. Found during S13 readiness audit via explicit `cargo check -p nika-extract --no-default-features`. Fix: conditional `#[allow(unused_variables)]` gated on all fetch-* features being off. |

### Session 12 final metrics

| Metric | Pre-S12 (S11 HEAD) | Post-S12 + G1/G2/G3 | Δ |
|---|---|---|---|
| Workspace tests | 10,780 | **10,805** | +25 |
| nika-engine LOC | 148,792 | **146,473** | −2,319 |
| Workspace crate count | 26 | **28** | +2 (nika-policy L1, nika-extract L2) |
| Clippy warnings | 0 | **0** | — |
| `cargo check -p nika-extract --no-default-features` | — | clean | new invariant |
| Binary size | 118 MB | ~118 MB | unchanged |
| New trait methods in nika-kernel | 0 | 5 | PolicyChecker(4) + HttpClient::send_streaming(1) + cancel field + filesystem splinter |
| Sessions complete | 11 | 12 | 1 |

### What went right (keep doing)

1. **4-agent parallel research before F1** — caught the original Phase 13 plan's fatal flaw (moving code without extending kernel traits).
2. **`pub use nika_policy as policy;` trick in F6** — 2-file change instead of rewriting 9 consumer files.
3. **TDD with insta snapshots** — G2 was caught because the snapshot was visible at accept time.
4. **Independent code review after completion** — caught G1 that my own self-review missed.

### What went wrong (lessons for S13/S14)

1. **F3 regressed the no-timeout path silently.** When unifying code paths, understand what DIFFERENT paths were doing, not just one. Lesson: diff pre-refactor vs post-refactor behavior on edge cases (1 MB output, 10 min runtime, etc.).
2. **F11 initial oracle was weak.** When the plan says "X AND Y", do not drop Y for convenience. Lesson: re-read the acceptance criteria BEFORE writing tests, not after.
3. **Missing `kill_on_drop`.** Focus on the happy path hid the resource leak. Lesson: for any subprocess spawning code, add `kill_on_drop(true)` to the checklist.
4. **Didn't run `--no-default-features`.** S12 only verified default-features workspace compile. G3 would have been caught pre-commit with `cargo check --no-default-features` in the ritual. Lesson: add to ritual.
5. **Caps struct shape mismatch with S13 plan.** My F9 `ExecCaps` has 4 fields (shell, policy, clock, fs_read). The S13 plan expects event_log, cancel_token, workflow_base_dir, working_dir_mode, project_root. This is a GATE-2 contradiction the plan tried to resolve but I didn't apply fully. Must be addressed in S13 Phase 0/1.

### Open architectural questions discovered during S13 readiness audit

These are flagged for S13 Phase 1 resolution:

| # | Question | Severity | Resolution needed by |
|---|---|---|---|
| Q1 | **Caps struct shape** — current 4-field minimal shape insufficient for S13 bridge. Need to add: `event_log`, `cancel_token`, `workflow_base_dir`, `working_dir_mode`, `project_root`. Does adding them break diamond? | P0 | S13-A1 |
| Q2 | **ExecCapsOwned / borrow() pattern** — S13 plan needs it for `for_each` + `tokio::spawn` crossing. S12 didn't add it. Who defines it (nika-kernel or nika-runtime)? | P0 | S13-A1 |
| Q3 | **ExecParams/FetchParams location** — still in `nika-engine::ast`. nika-runtime cannot depend on nika-engine (cycle). Move to `nika-core::ast`? When? | P0 | S13-A1 or prerequisite commit |
| Q4 | **BuiltinRouter trait** — S13 plan uses `&dyn BuiltinRouter` but no such trait exists in nika-kernel. Define in S13 prerequisite? | P1 | S13-C1 |
| Q5 | **McpPool trait** — same question. | P1 | S13-C1 |
| Q6 | **FetchAux 4 traits** (CookieJar, ResponseCache, DomainRateLimiter, RobotsChecker) — S13 plan proposes adding them as commit 4.1. Are they in scope for S13 or S12 completion? | P1 | S13-D start |
| Q7 | **RunContext location** — currently in `nika-engine::store`. S13 plan (commit 1.3) proposes moving to `nika-core::store`. Is that still valid? Any dependencies that break? | P0 | S13-A3 or prerequisite |
| Q8 | **Runner location** — S13 plan (commit 1.3) proposes moving Runner from nika-engine to nika-runtime. This creates a circular dep (nika-runtime ↔ nika-engine) resolved by moving RunContext. Is the circular resolution still needed given AMEND-4 (dispatch parallel, not live)? | P1 | S13-A3 |
| Q9 | **RigProvider bridge** — `nika-engine::provider::rig::kernel_bridge.rs` implements `Provider for RigProvider`. Who constructs `Arc<dyn Provider>` for nika-runtime in S13? The plan says TaskExecutor's `get_dyn_provider()` — but TaskExecutor is deleted in S14. Temporary bridge in S13? | P1 | S13-A2 |
| Q10 | **`HttpClient` vs raw reqwest in nika-verb-fetch** — kernel audit said fetch owns reqwest directly for the SSRF redirect closure. S13 plan says verb-fetch has `reqwest` as a direct dep (the "exception"). Does S13 need `HttpClient::send_streaming` at all, or is it dead code during S13? | P1 | S13-D2 |

---

## Session 13 — Extraction Pass 1 (SHIPPED 2026-04-10)

**Goal:** create nika-runtime L3 + extract exec/invoke/fetch into verb crates.
**Plan references:** `07-session13-extraction-1.md`, `14-session12-handoff-postmortem.md`, `15-session13-mega-prompt.md`.
**Status:** ✅ SHIPPED. S13-A through E landed 2026-04-10. 3 verb crates extracted (exec/fetch/invoke) + nika-runtime skeleton + BuiltinRouter/McpPool kernel traits. Crate count 28 → 32 (+4). For the full commit list, see `git log` and the `.claude/rules/architecture.md` Session 13 block. The "to be filled" checklist and table below are the ORIGINAL pre-execution plan, preserved for historical reference only.

> **Note:** Actual commits landed under phases S13-A/B/C/D/E matching the pre-flight table below. Status column was never back-filled — the journal focus shifted to S14 immediately after. The `ARCHITECTURE.md` block in `.claude/rules/architecture.md` is the authoritative S13 record.

### Pre-flight checklist

- [ ] Phase 0: read all S12 handoff docs (45 min)
- [ ] Phase 1: dispatch 4 review agents in parallel (20 min)
- [ ] Phase 2: write synthesis doc + user sign-off (30 min)
- [ ] Resolve all 10 open architectural questions (Q1-Q10 above)
- [ ] Phase 3 start: commit S13-A1

### Commits (to be filled as they land)

| # | SHA | Subject | Status | Notes |
|---|---|---|---|---|
| A1 | — | `feat(runtime): create nika-runtime L3 crate + VerbCapabilities` | pending | |
| A2 | — | `feat(runtime): TaskAction enum + dispatch skeleton` | pending | |
| A3 | — | `feat(runtime): VerbCapabilities accessors + unit tests` | pending | |
| A4 | — | `chore(workspace): wire nika-runtime` | pending | |
| B1 | — | `feat(verb-exec): create nika-verb-exec crate` | pending | |
| B2 | — | `feat(engine): TaskExecutor::run_exec bridges` | pending | |
| B3 | — | `test(verb-exec): comprehensive unit tests` | pending | |
| B4 | — | `chore(runtime): wire dispatch Exec arm` | pending | |
| C1 | — | `feat(verb-invoke): create nika-verb-invoke crate` | pending | |
| C2 | — | `feat(verb-invoke): MCP routing` | pending | |
| C3 | — | `feat(engine): TaskExecutor::run_invoke bridges` | pending | |
| C4 | — | `test(verb-invoke): unit tests` | pending | |
| C5 | — | `chore(runtime): wire dispatch Invoke arm` | pending | |
| D1 | — | `feat(http): ReqwestClient::send_streaming with 50MB cap` | pending | (optional per Q10) |
| D2 | — | `feat(verb-fetch): create nika-verb-fetch crate` | pending | |
| D3 | — | `feat(verb-fetch): SSRF redirect policy via nika-policy` | pending | |
| D4 | — | `feat(engine): TaskExecutor::run_fetch bridges` | pending | |
| D5 | — | `test(verb-fetch): wiremock integration` | pending | |
| D6 | — | `chore(runtime): wire dispatch Fetch arm` | pending | |
| E1 | — | `docs(constellation): ARCHITECTURE.md + S13 memory` | pending | |
| E2 | — | `test(regression): full golden suite verification` | pending | |

### Target metrics

| Metric | Start (post-S12) | Target (post-S13) | Δ |
|---|---|---|---|
| Workspace tests | 10,805 | ~10,830 | +25 |
| nika-engine LOC | 146,473 | ~143,800 | −2,673 |
| Workspace crates | 28 | 32 | +4 (runtime, verb-exec, verb-invoke, verb-fetch) |
| Clippy | 0 | 0 | — |
| Golden suite | 5 passing | 5 passing | — (kill criterion: every commit must keep this green) |

---

## Session 14 — Extraction Pass 2 + TaskExecutor Dissolution (SUPERSEDED — S14 shipped with different scope)

**Goal (ORIGINAL):** enrich Provider trait + create nika-shield + extract verb-infer + verb-agent + delete TaskExecutor.
**Plan references:** `08-session14-extraction-2.md`, `17-session14-enriched-plan.md` — BOTH NOW SUPERSEDED.
**Actual outcome:** S14 shipped a radically reduced scope (5 Wave A–B commits + S14.5 hotfix). See the **Session 14 (2026-04-11)** entry further down for the real record. W14-B2 (infer.rs bridge) + Wave C (agent extraction) + TaskExecutor dissolution all deferred to S15+.
**Status:** ✅ SHIPPED (with scope correction) — see the real entry below.

### Target metrics

| Metric | Start (post-S13) | Target (post-S14) | Δ |
|---|---|---|---|
| Workspace tests | ~10,830 | ~10,850 | +20 |
| nika-engine LOC | ~143,800 | ~138,500 | −5,300 |
| Workspace crates | 32 | 35 | +3 (shield, verb-infer, verb-agent) |
| TaskExecutor | 22-field god | **DELETED** | — |
| Binary size | 118 MB | 120–123 MB (predicted) | +2–5 MB |

See `17-session14-enriched-plan.md` for the detailed commit plan, code sketches, and traps.

---

## Bugs log (chronological)

| ID | Session | Commit introduced | Commit fixed | Severity | Title |
|---|---|---|---|---|---|
| BUG-001 | S12 | `f235b9913` (F3) | `8ef810b9f` (G1) | P0 | Pipe buffer deadlock in TokioShell — any command producing >64KB stdout hangs forever |
| BUG-002 | S12 | `f235b9913` (F3, latent pre-existing) | `8ef810b9f` (G1) | P0 | Zombie children on cancel/timeout in TokioShell — no `kill_on_drop(true)` |
| BUG-003 | S12 | `2cb4f0bcb` (F11) | `304b1d3c2` (G2) | P1 | Golden oracle too weak — lifecycle only, no output assertion. Violates AMEND-1. |
| BUG-004 | S12 | `719b26522` (F7) | `d7ac1031b` (G3) | P2 | `nika-extract --no-default-features` warning — unused `base_url` parameter when all fetch-* features off |

**Total S12 bugs caught in review:** 4 (2× P0, 1× P1, 1× P2). **All fixed before next session.**

**Bugs shipped to production:** 0.

---

## Improvements log

| ID | Session | Commit | Title |
|---|---|---|---|
| IMPR-001 | S12 | `870571f74` (F1) | Object-safe PolicyChecker trait in nika-kernel — verb crates can mock policy in tests without pulling nika-policy |
| IMPR-002 | S12 | `2b5908bca` (F2) | HttpClient::send_streaming surface ready for 50MB early-abort (impl deferred to S13 if needed) |
| IMPR-003 | S12 | `f235b9913` (F3) | ShellCommand::cancel field — cooperative cancellation via CancellationToken |
| IMPR-004 | S12 | `82d43cc17` (F4) | FsRead + FsWrite splinters — verb crates can depend on narrowest capability |
| IMPR-005 | S12 | `2d9ae1a3a` (F5) | `nika-policy` L1 crate — PolicyEnforcer out of the engine monolith |
| IMPR-006 | S12 | `719b26522` (F7) | `nika-extract` L2 crate — pure 9-mode extraction pipeline |
| IMPR-007 | S12 | `c6a0d78bc` (F9) | 5 per-verb Caps structs in nika-kernel — types only, wired by S13 |
| IMPR-008 | S12 | `2cb4f0bcb` (F11) | Golden regression suite via insta — S13/S14 safety net |
| IMPR-009 | S12 | `8ef810b9f` (G1) | `tokio::try_join!` concurrent pipe drain pattern in TokioShell — becomes reference for all subprocess spawning |
| IMPR-010 | S12 | `8ef810b9f` (G1) | `cmd.kill_on_drop(true)` — enforced via invariant #11 for all S13+ subprocess code |
| IMPR-011 | S12 | `304b1d3c2` (G2) | Golden snapshots capture BOTH lifecycle AND output — oracle strong enough for verb extraction |
| IMPR-012 | S12 | `d7ac1031b` (G3) | `cargo check --no-default-features` added to verification ritual (pending S13 adoption) |

---

## Tests added log

| ID | Session | Commit | Count | Type | What |
|---|---|---|---|---|---|
| T-001 | S12 | F1 | 4 | unit | PolicyChecker trait + PolicyDecision + PolicyError |
| T-002 | S12 | F2 | 3 | unit | HttpClient::send_streaming default + HttpError display + HttpStreamResponse debug |
| T-003 | S12 | F3 | 2 | async | Pre-cancel + mid-flight cancel of shell |
| T-004 | S12 | F4 | 2 | compile | FsRead narrowing + umbrella Filesystem blanket |
| T-005 | S12 | F5 | 36+1 | unit | Policy enforcement (moved from engine, +1 new PolicyChecker trait impl test) |
| T-006 | S12 | F7 | 42+3 | unit | Extraction pipeline (moved from engine, +3 new: error display + entry point + smoke) |
| T-007 | S12 | F9 | 5 | compile | Send+Sync assertions for all 5 Caps structs |
| T-008 | S12 | F11 | 5 | snapshot | Golden lifecycle tests for all 5 verbs |
| T-009 | S12 | G1 | 1 | async | `large_output_does_not_deadlock` — 1MB pipe regression test |
| T-010 | S12 | G2 | 0 (5 updated) | snapshot | Golden snapshots now capture output |

**Total new tests added in S12: ~100.** Net delta: +25 after dedup of moved tests.

---

## Verification ritual evolution

Initial S12 ritual (4 commands):
```
cargo test -p <crate> --lib
cargo clippy --workspace --lib -- -D warnings
cargo test --workspace --lib
git commit (with co-author trailer)
```

**Upgraded S13 ritual** (learned from G-fixes):
```
1. cargo test -p <crate> --lib                                  # crate unit tests
2. cargo test -p <crate> --no-default-features --lib            # feature flag hygiene (G3 lesson)
3. cargo check -p <crate> --no-default-features                 # minimal build (G3 lesson)
4. cargo test -p nika-engine --lib runner::tests_golden_verbs   # oracle (G2 lesson)
5. cargo test --workspace --lib                                 # full suite
6. cargo clippy --workspace --lib -- -D warnings                # lint
7. cargo tree -p <new crate> | grep nika-engine                 # diamond (required for verb crates)
8. git commit (with co-author trailer, specific file paths only)
```

---

## Cross-session invariants discovered

1. **Sacred invariant #11 (G1):** every `tokio::process::Command` MUST set `cmd.kill_on_drop(true)` before spawn.
2. **Sacred invariant #12 (G1):** every concurrent pipe-reading code MUST use `tokio::try_join!` with drain futures. NEVER sequential `wait().then().read_to_end()`.
3. **Sacred invariant #13 (G1):** every subprocess spawning code MUST be regression-tested with >1 MB output.
4. **Sacred invariant #14 (G2):** golden test oracle MUST capture BOTH lifecycle AND output. Never weaken for convenience.
5. **Sacred invariant #15 (G3):** verification ritual MUST include `--no-default-features` check for crates with feature flags.

---

## References

- [`14-session12-handoff-postmortem.md`](14-session12-handoff-postmortem.md) — S12 post-mortem (784 lines)
- [`15-session13-mega-prompt.md`](15-session13-mega-prompt.md) — S13 drop-in mega-prompt (615 lines)
- [`17-session14-enriched-plan.md`](17-session14-enriched-plan.md) — S14 enriched plan (to be written, see next)
- [`19-socratic-review.md`](19-socratic-review.md) — Socratic cross-session review (to be written)
- [`13-plan-corrections.md`](13-plan-corrections.md) — authoritative amendments
- [`09-risk-register.md`](09-risk-register.md) — landmine catalog

---

**End of Session Journal. Fill in Session 13 commits as they land.**

---

## Session 14 (2026-04-11)

### Wave A–B (5 commits, Phase-1-reviewed)

**Phase 0 finding:** the S14 mega-prompt v3 was stale. Actual HEAD was 11 commits ahead of the handoff baseline `a3e8d8ab8`. Labels W14-A0/A1/B0/E1 collided semantically with already-landed commits. Two flagged "P0/P1 bugs" turned out to be phantoms.

**Phase 1 review** (4 agents pre-code):
- rust-architect: validated InferEvent::Done struct variant, blocked McpPoolAdapter (trait too thin)
- rust-pro: re-counted fetch helper LOC (340 → ~253 actual)
- code-explorer: confirmed McpPool gap analysis
- rust-async-expert: confirmed cancel semantics, audited fetch.rs Send (clean)

**Commits landed:**

| Commit | Hash | Summary |
|--------|------|---------|
| S14-α | `c96dec861` | kernel: `InferEvent::Done` struct variant + `#[non_exhaustive]` enum (5 sites updated atomically, 1 new test in kernel-mock) |
| S14-β | `9f384e07a` | verb-fetch: migrate `safe_backoff_delay` + `parse_retry_after` + `is_html_content_type` + hreflang family from engine. Engine fetch.rs −277 LOC. New mod `retry.rs` (16 tests) + `hreflang.rs` (4 tests). Engine gains dep on nika-verb-fetch. |
| S14-γ | `935658eae` | verb-fetch: `RetryExhausted` + `DeadlineExceeded` error variants + `#[non_exhaustive]` (S15 retry-loop prep) |
| S14-δ | `aebea1cd9` | verb-infer: golden oracle test asserts all 8 `ProviderResponded` fields (request_id, ttft_ms, cost_usd, cache_read_tokens). S12-G2 compliance. |
| S14-ε | `acf9d1784` | verb-exec: pre-spawn `caps.cancel.is_cancelled()` short-circuit. New test with empty MockShell + pre-cancel proves zero subprocess fork. |

**Test deltas:** kernel-mock 51→52, verb-fetch 5→25 (+20), verb-fetch 25→28 (+3 Display), verb-infer 9→10, verb-exec 12→13. Engine 3873 (−16 migrated to verb-fetch). Total ~10,900 lib tests.

**Engine LOC:** 146,473 → 146,196 (−277).

### S14.5 hotfix (post-review, 2 commits)

**4-agent post-execution review** (code-reviewer + rust-architect ×2 + code-explorer):

| Finding | Severity | Fix |
|---------|----------|-----|
| `f64::EPSILON` assertion in S14-δ mathematically wrong (ULP at 0.0042 ≈ 9.3e-19, accepts ~5.3e-14 silent drift) | P1 correctness | `assert_eq!` exact (S14.5-A) |
| Only `VerbFetchError` is `#[non_exhaustive]`; other 3 verb errors aren't | symmetry violation | Retrofit on Exec/Invoke/Infer + wildcard arms in engine match sites (S14.5-A) |
| Missing `# TEMP` markers on engine→verb deps | invariant #22 violation | Retrofit with clearance conditions (S14.5-A) |
| `parse_retry_after(&reqwest::header::HeaderMap)` leaks L1 type into verb-crate signature | new invariant #23 | Codified as #23 in architecture.md, fix deferred to S15-A0 |
| `infer.rs` emits `ProviderResponded` from 7 sites (not 2 as Phase 1 thought) | new invariant #24 | Codified as #24 in architecture.md, collapse deferred to W14-B2 |
| `#[non_exhaustive]` asymmetry | new invariant #25 | Codified as #25 in architecture.md (S14.5-B) |
| `finish_reason_raw` plumbed but never consumed by mapping fn | dead carriage | Wire deferred to W14-B2 |
| Crate count ARCH said 33; actual is 35 | doc drift | Corrected in S14.5-B |

**Commits:**
- `53513e5ee` fix(s14): post-S14 review findings hotfix A
- `144f5abeb` docs(constellation): codify invariants #23/#24/#25 + correct crate count
- `12407d125` docs(constellation): ARCHITECTURE.md update for Session 14 + 14.5

### Lessons codified

- **Sacred invariant #23** — kernel-adjacent helpers stay primitive-typed (no `reqwest::*` / `tokio::*` leaks). Triggered by `parse_retry_after(&HeaderMap)`.
- **Sacred invariant #24** — exactly one `EventKind::*` emit site per file. Triggered by `infer.rs` 7-site `ProviderResponded`.
- **Sacred invariant #25** — verb-crate errors `#[non_exhaustive]` from day one. Triggered by S14-γ asymmetry.

### Meta-lessons (apply to S15+)

- **Phase 0 pre-flight is mandatory.** S14 caught a stale handoff in Phase 0. Without it, would have spent hours implementing phantom bugs.
- **Phase 1 review BEFORE code, then AGAIN AFTER code.** Pre-execution catches design flaws; post-execution catches implementation drift. S14 had both — would have shipped a P1 math bug without post-review.
- **Drift in Phase 1 review is real.** Async-expert's pre-S14 finding "2 ProviderResponded sites" was already drift — actual was 7 by post-execution measurement. ALWAYS re-grep at session start.
- **`#[non_exhaustive]` cross-crate is a compile-time trap.** Adding it to a verb error breaks engine match sites unless you ALSO add wildcard arms in the same commit. S14.5-A had to fix this mid-build.

### S15 setup

- `23-session15-mega-prompt.md` (444+ lines, post-review enriched) — full S15 plan with 8-commit Option C sequence, DTOs ready-to-paste, top traps, anti-goals.
- `21-session15-handoff.md` — superseded banner pointing to 23.
- `22-agent-v2-design.md` — Wave C / nika-verb-agent design (separate concern, S15+/S16).
