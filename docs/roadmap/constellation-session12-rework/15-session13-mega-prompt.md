# Session 13 Mega-Prompt — Nika Constellation Verb Extraction

> ⚠️ **SUPERSEDED 2026-04-11** — Session 13 HAS SHIPPED. All S13 commits are on `origin/main` (exec/fetch/invoke verb crates + nika-runtime skeleton). This doc is preserved for historical reference to the original plan-of-record that drove execution.
>
> **For the real S13 record, read `16-session-journal.md`** (Session 13 entry).
>
> **Historical value only.**
>
> ---

> **Purpose:** Drop-in prompt for a **fresh Claude Code session** to execute Session 13 of the Nika Constellation refactor. This prompt assumes the new session starts with **zero memory** of Session 12 — every piece of context required is embedded or referenced by absolute path.
>
> **Expected duration:** 10–12h wall clock, 15–18 commits
> **Launch gate:** 2026-05-05 (J-25 as of 2026-04-10)
> **Reasoning effort required:** **HIGH (ultrathink)** — no shortcuts
>
> **How to use this file:** copy the ENTIRE content into a fresh Claude Code session and start the conversation with it. The first four phases are non-negotiable: context re-absorption, parallel agent review, plan synthesis, execution.

---

## STOP — Read this before doing ANYTHING

You are **NOT** starting fresh on "implement Session 13 from the plan doc." You are stepping into a multi-session refactor where **Session 12 is complete, pushed, and your job is to extend it safely**. The previous session (me, Claude, acting under the same rules you now have) did the foundation work, had an independent code review, caught **2 critical bugs plus 1 warning** (pipe deadlock + zombie children in TokioShell, weak golden test oracle, unused variable on `--no-default-features`), fixed them, wrote a 784-line post-mortem, AND ran a cross-session Socratic review that found **7 S13 GATE items** and **3 S14 GATE items** that must be resolved before execution. All **21 S12 commits** are on `origin/main`.

**New docs written after the post-mortem (you MUST read them in addition to the post-mortem):**

- `16-session-journal.md` — chronological journal of every S12 commit + bug + fix + lesson learned + 10 open architectural questions
- `17-session14-enriched-plan.md` — enriched Session 14 plan with W14-A0 test migration (per AMEND-3) + Plan B fallback
- `19-socratic-review.md` — 25 Socratic questions across all sessions with findings, confidence verdicts, and the 10 GATE items

**If you do not read these docs, you will hit the gaps they identify. Budget 30 extra minutes for them.**

**Your Session 13 has 4 phases and you MUST execute them IN ORDER:**

| Phase | Name | Duration | What |
|---|---|---|---|
| **0** | Context re-absorption | ~45 min | Read full post-mortem + S12 code + G1/G2 lessons |
| **1** | Parallel agent review | ~20 min | Dispatch 4 agents to audit S12 state + critique S13 plan |
| **2** | Plan synthesis | ~30 min | Integrate agent findings, update plan, get user sign-off |
| **3** | Execution | 10–12h | 15–18 commits with TDD + golden test verification |

**Skipping phases 0, 1, or 2 = guaranteed to ship bugs.** The previous session proved this: I shipped 2 P0 bugs that were caught only because a reviewer agent ran after the work. Do the review UP FRONT this time — it's cheaper.

**Ultrathink is mandatory.** Session 13 is where the diamond layering either holds or breaks. Do not batch-paste code; reason about each type signature, each lifetime, each dyn-object dispatch. If you find yourself writing boilerplate without thinking about ownership, stop and reason.

---

## Project coordinates

- **Working directory:** `/Users/thibaut/dev/supernovae/nika`
- **Workspace root:** `/Users/thibaut/dev/supernovae/nika/tools`
- **Main branch:** `main` (HEAD is also `origin/main` after S12 push)
- **Launch date:** 2026-05-05 (Tuesday, 14h Paris)
- **Today (at time of handoff):** 2026-04-10 (J-25)
- **Git user:** Thibaut Melen
- **Co-author trailer:** `Nika 🦋 <nika@supernovae.studio>` (NEVER Claude/Anthropic)

---

# PHASE 0 — Context re-absorption (MANDATORY, ~45 min)

## 0.1 — Environment verification (run these FIRST, in order)

Run each check and compare against the expected output. If anything fails, STOP and report to the user.

```
cd /Users/thibaut/dev/supernovae/nika

# Git state
git log --oneline -5
# EXPECT top line: 304b1d3c2 test(runtime): golden tests now assert output content (S12-G2)
git rev-parse HEAD
# EXPECT: 304b1d3c2856648fcb0f9cf00276faead170f7fe
git rev-parse origin/main
# EXPECT: same as HEAD (pushed)
git status
# EXPECT: some files modified (user's launch-prep — DO NOT TOUCH)

# Workspace state
cd tools
cargo test --workspace --lib 2>&1 | grep -E "^test result" | awk '{s+=$4} END{print s}'
# EXPECT: 10805
cargo clippy --workspace --lib -- -D warnings
# EXPECT: clean

# S12 deliverables in place
ls nika-policy/src/lib.rs
ls nika-extract/src/lib.rs
ls nika-kernel/src/caps.rs
ls nika-kernel/src/policy.rs

# Diamond layering intact
cargo tree -p nika-policy  | grep nika-engine   # EXPECT: empty
cargo tree -p nika-extract | grep nika-engine   # EXPECT: empty
cargo tree -p nika-kernel  | grep nika-policy   # EXPECT: empty
cargo tree -p nika-kernel  | grep nika-extract  # EXPECT: empty

# Golden regression suite (your kill criterion for every S13 commit)
cargo test -p nika-engine --lib runner::tests_golden_verbs
# EXPECT: 5 passed

# Engine LOC baseline
find nika-engine/src -name "*.rs" | xargs wc -l | tail -1
# EXPECT: around 146,473

# Crate count
grep -c '^    "nika' Cargo.toml
# EXPECT: 28
```

**If ANY of these checks fail: STOP. Do not continue. Ask the user.**

## 0.2 — Mandatory reading list (in this order)

The handoff post-mortem is **gitignored and local-only** — it lives in the `docs/plans/constellation-session12-rework/` folder which is not tracked. You must read it from the filesystem.

### Tier 1 — Session 12 handoff + Socratic review (READ FULLY, ~45 min)

1. **`docs/plans/constellation-session12-rework/14-session12-handoff-postmortem.md`** (784 lines) — the single most important document. Read sections 5 (architecture inventory), 6 (known issues), 8 (S13 scope), 9 (commit plan), 10 (traps), Appendix A (lessons learned).

2. **`docs/plans/constellation-session12-rework/15-session13-mega-prompt.md`** — this file.

3. **`docs/plans/constellation-session12-rework/16-session-journal.md`** (450+ lines) — chronological journal with 10 open architectural questions (Q1-Q10). Read the "Open architectural questions" section carefully — these block S13-A1.

4. **`docs/plans/constellation-session12-rework/17-session14-enriched-plan.md`** (500+ lines) — enriched S14 plan. Read because S14 plans may force S13 design decisions (especially GATE-S13-2: Runner move timing).

5. **`docs/plans/constellation-session12-rework/19-socratic-review.md`** (800+ lines) — 25 Socratic questions across all sessions with 10 GATE items. Read Parts I (S12 audit), II (S13 critique, 9 questions), IV (cross-session), and V (confidence verdict). The 7 GATE-S13 items are the gates you MUST clear.

### Tier 2 — Original S13 plan + authoritative amendments (READ FULLY, ~20 min)

3. **`docs/plans/constellation-session12-rework/README.md`** — index and sacred invariants.

4. **`docs/plans/constellation-session12-rework/13-plan-corrections.md`** — authoritative amendments. Where this file contradicts any other plan doc, this wins. Read AMEND-1 (golden tests), AMEND-2 (Caps structs in nika-kernel, not nika-runtime), AMEND-4 (dispatch is parallel not live during S13).

5. **`docs/plans/constellation-session12-rework/07-session13-extraction-1.md`** — the original S13 plan. Do not execute it blindly — validate it against the handoff post-mortem and the agent findings from Phase 1 of this session.

6. **`docs/plans/constellation-session12-rework/01-architecture-vision.md`** — end-state architecture. Know what you are building toward.

7. **`docs/plans/constellation-session12-rework/02-adr-001-enum-dispatch.md`** — why NO `trait Verb`. Enum + match. Sacred.

8. **`docs/plans/constellation-session12-rework/03-adr-002-typed-contexts.md`** — why per-verb `Caps<'a>` structs. Rigorous lifetime discipline.

9. **`docs/plans/constellation-session12-rework/09-risk-register.md`** — known landmines across all sessions.

10. **`docs/plans/constellation-session12-rework/11-kernel-trait-audit.md`** — kernel trait priority list. Informs what S13 verb crates can rely on.

### Tier 3 — S12 deliverables to study BY HAND (~10 min)

These are the building blocks S13 consumes. Read them, don't just `cat` them — understand shapes and lifetimes.

11. **`tools/nika-kernel/src/caps.rs`** — the 5 per-verb `Caps<'a>` structs. Note: `provider` is `Arc<dyn Provider>` while everything else is `&'a dyn Trait`. Note `#[non_exhaustive]`. Note `AgentCaps` composes `InvokeCaps<'a>`.

12. **`tools/nika-kernel/src/policy.rs`** — `PolicyChecker` trait (object-safe). 4 methods.

13. **`tools/nika-kernel/src/filesystem.rs`** — `FsRead`, `FsWrite`, umbrella `Filesystem`. Blanket impl.

14. **`tools/nika-kernel/src/shell.rs`** — `ShellCommand` with `cancel: Option<CancellationToken>`. `ShellError::Cancelled` variant.

15. **`tools/nika-kernel/src/http.rs`** — `HttpClient::send` plus `send_streaming` (default returns `Unsupported`). `HttpStreamResponse` shape. `HttpError::TooLarge`, `HttpError::Unsupported`.

16. **`tools/nika-policy/src/lib.rs`** — `PolicyEnforcer` concrete, `impl PolicyChecker for PolicyEnforcer`, `map_decision` helper, SSRF helpers, `TokenReservation` RAII guard.

17. **`tools/nika-extract/src/lib.rs`** — pure 9-mode extraction, `extract()` entry point, `ExtractError`.

18. **`tools/nika-exec-runner/src/lib.rs`** — **STUDY THIS CAREFULLY**. It contains the G1 fix (tokio::try_join! + kill_on_drop). Every verb crate that spawns subprocesses MUST follow this pattern.

19. **`tools/nika-engine/src/runtime/runner/tests_golden_verbs.rs`** — YOUR regression oracle. Run it after every verb extraction commit. If it fails, something observable changed. Revert and re-do.

20. **`tools/nika-engine/src/runtime/runner/snapshots/*.snap`** — the 5 golden snapshots. Each captures lifecycle AND output.

### Tier 4 — Project rules

21. **`nika/CLAUDE.md`** — project rules.
22. **`nika/.claude/rules/architecture.md`** — diamond layering.
23. **`nika/.claude/rules/git-workflow.md`** — 1 fix = 1 commit.

## 0.2bis — The 7 GATE-S13 items from the Socratic review (MUST resolve before Phase 3)

From `19-socratic-review.md`. These are BLOCKERS — Session 13 cannot proceed until each is decided.

- **GATE-S13-1** — nika-verb-exec tests MUST include a `subprocess_does_not_deadlock` regression test with >1 MB output (G1 lesson applied). Add to S13-B3 commit plan.
- **GATE-S13-2** — Decide: does `Runner` move to `nika-runtime` in S13 or S14? Socratic recommends **S14** (simplifies S13, deferred cost). User sign-off needed.
- **GATE-S13-3** — Policy guard `!Send` problem. Bridge pattern `&*self.policy_enforcer.read() as &dyn PolicyChecker` held across `.await` won't compile. Fix: change `policy_enforcer: Arc<RwLock<PolicyEnforcer>>` to `policy_enforcer: Arc<PolicyEnforcer>` with interior mutability for `add_allowed_host`. ~5 files affected. **This is the biggest blocker.**
- **GATE-S13-4** — `BuiltinRouter` trait does not exist in `nika-kernel`. S13 plan assumes it does. Either add as prerequisite commit S13-A0 or use concrete type in InvokeCaps.
- **GATE-S13-5** — `McpPool` trait does not exist in `nika-kernel`. Same question. Recommended: add as S13-A0 prerequisite.
- **GATE-S13-6** — Pre-flight grep tests for error-message format assertions (e.g., `assert!(err.contains("Blocked command"))`) that could break on bridge transition. If found, adjust bridge `From` impls to preserve message format.
- **GATE-S13-7** — `Caps` struct expansion: current 4-field shape (shell, policy, clock, fs_read) is insufficient. Must add `event_log`, `cancel_token`, `workflow_base_dir`, `working_dir_mode`, `project_root` at minimum. But `event_log` requires nika-kernel to depend on nika-event (layering violation) OR an `EventEmitter` trait in nika-kernel::events (clean). Recommendation: Alternative 2 (trait in kernel, impl in nika-event). Prerequisite commit S13-A0.

**Phase 2 user sign-off MUST include explicit resolution of ALL 7 gates.**

## 0.3 — Study the G1/G2/G3 corrections as cautionary tales

The S12 post-mortem documents 2 bugs I shipped that were caught in review. Study them — the lessons apply DIRECTLY to Session 13 verb extraction.

### G1 — Pipe buffer deadlock + zombie children (TokioShell)

**Root cause 1 (deadlock):** I unified all paths through a single `child_fut` that read stdout/stderr **sequentially after** `child.wait()`. For any command producing more than ~64 KB stdout, the child blocks on the pipe buffer → `wait()` blocks forever → deadlock. Pre-refactor the no-timeout path used `wait_with_output()` which drains correctly; my unification regressed it.

**Lesson for S13:** when `nika-verb-exec` spawns anything, use `tokio::try_join!(child.wait(), drain(stdout), drain(stderr))`. Never `wait()` before reading.

**Root cause 2 (zombies):** `TokioShell` never called `cmd.kill_on_drop(true)`. When `tokio::select!` cancel/timeout arm fires, `Child` is dropped — but tokio does NOT kill the process on drop unless you opt in.

**Lesson for S13:** every `tokio::process::Command` in any verb crate must set `kill_on_drop(true)` before spawn. No exceptions.

### G2 — Golden oracle was too weak

**Root cause:** AMEND-1 required "assert output content AND event sequence". My initial F11 only snapshotted the workflow lifecycle. A verb extraction that corrupted output while preserving the lifecycle would have passed silently.

**Lesson for S13:** when the plan says "assert X AND Y", do not drop Y for convenience. The golden snapshots NOW capture both lifecycle AND output. Keep it that way. Do not weaken the oracle.

### G3 — Unused variable warning on `--no-default-features` (nika-extract)

**Root cause:** F7 `apply_extract_with_base` takes `base_url: Option<&str>`. When all fetch-* features are disabled, no match arm uses the parameter. Workspace clippy with default features masks the warning. Found during S13 readiness audit by running `cargo check -p nika-extract --no-default-features` — not part of S12's standard ritual.

**Lesson for S13:** add `--no-default-features` check to the verification ritual for any new crate that has feature flags. The workspace default-features suite is NOT sufficient. This is now Sacred Invariant #15.

### Meta-lesson

Independent review catches what author confidence masks. Session 13 must bake review into Phase 1 — BEFORE execution, not after. The Socratic review (`19-socratic-review.md`) is the evolution of this practice: instead of reviewing after the work, probe the plan BEFORE the work. S13 must do this with 4 agents in Phase 1.

---

# PHASE 1 — Parallel agent review (MANDATORY, ~20 min wall clock)

Before writing a single line of code, dispatch **four agents in parallel** to audit the state and critique the Session 13 plan. Use a single message with 4 `Agent` tool invocations to parallelize.

## 1.1 — Dispatch strategy

All four agents are research-only (they read, do not write code). Run them in parallel. Each returns a synthesis. You then reconcile findings.

**Why 4 agents and not just 1:** each has a different lens. A reviewer looks for bugs. A rust-pro looks for idiom violations. A code-explorer maps coupling. An architect challenges the high-level design. Missing any one lens = risk of shipping a flaw.

## 1.2 — Agent 1: feature-dev:code-reviewer

**Purpose:** audit the current S12 state and critique `07-session13-extraction-1.md` against the handoff post-mortem and AMEND rules.

**Prompt to pass:**

> You are reviewing the Nika Constellation refactor for Session 13 readiness.
> Repository: /Users/thibaut/dev/supernovae/nika
>
> Session 12 just completed (19 commits, HEAD 304b1d3c2). Your job is to audit the S12 state and critique the Session 13 plan before execution.
>
> MUST READ FIRST:
> - docs/plans/constellation-session12-rework/14-session12-handoff-postmortem.md
> - docs/plans/constellation-session12-rework/13-plan-corrections.md (AMENDs override everything)
> - docs/plans/constellation-session12-rework/07-session13-extraction-1.md
> - docs/plans/constellation-session12-rework/09-risk-register.md
> - tools/nika-kernel/src/caps.rs
> - tools/nika-exec-runner/src/lib.rs (G1 pattern verb crates must follow)
>
> DELIVER:
> 1. S12 state audit — is the workspace actually ready for S13? Any latent bugs besides G1/G2 that could bite S13? Check especially:
>    - nika-policy PolicyChecker trait impl disambiguation (inherent vs trait)
>    - nika-extract feature flag interaction with engine Cargo.toml
>    - kernel::caps struct lifetimes (will they compose cleanly in nika-runtime?)
> 2. S13 plan critique — is 07-session13-extraction-1.md still correct given AMEND-1..5? Any contradictions? Any missing commits? Any commits that should be split or merged?
> 3. Known risks you find — rank P0/P1/P2 with file:line and suggested fix.
> 4. Go/no-go verdict — is Session 13 cleared to start, or must fixes land first?
>
> Confidence ≥ 80% to flag an issue. No style nits. Under 1500 words.

## 1.3 — Agent 2: spn-rust:rust-pro

**Purpose:** deep Rust idiom audit of Session 12 deliverables before Session 13 extraction.

**Prompt to pass:**

> You are performing a rigorous Rust idiom audit of Session 12 deliverables before Session 13 extraction begins.
> Repository: /Users/thibaut/dev/supernovae/nika
>
> MUST READ:
> - tools/nika-kernel/src/caps.rs
> - tools/nika-kernel/src/policy.rs
> - tools/nika-kernel/src/filesystem.rs
> - tools/nika-kernel/src/shell.rs
> - tools/nika-kernel/src/http.rs
> - tools/nika-kernel/src/provider.rs
> - tools/nika-policy/src/lib.rs (trait impl pattern)
> - tools/nika-extract/src/lib.rs (pure module shape)
> - tools/nika-exec-runner/src/lib.rs (G1 tokio::try_join! pattern)
> - docs/plans/constellation-session12-rework/02-adr-001-enum-dispatch.md
> - docs/plans/constellation-session12-rework/03-adr-002-typed-contexts.md
>
> EVALUATE:
> 1. Caps struct shapes — is `Arc<dyn Provider>` plus `&'a dyn Trait` for other fields an acceptable asymmetry? Any lifetime issues when nika-runtime's VerbCapabilities method returns `ExecCaps<'_>` borrowed from `&self`?
> 2. AgentCaps composition — `AgentCaps<'a> { invoke: InvokeCaps<'a>, ... }`. Does this composition hold at call sites? Any lifetime inference traps?
> 3. PolicyChecker trait impl — `impl PolicyChecker for PolicyEnforcer` with name collision against inherent methods. Is disambiguation correct? Will `enforcer.check_exec("...")` hit inherent or trait?
> 4. FsRead + FsWrite split — is the blanket impl safe for existing `dyn Filesystem` usages? Any object-safety traps for S13?
> 5. HttpStreamResponse body — `Pin<Box<dyn Stream<Item = Result<...>> + Send>>`. Usable from async contexts? Requires Unpin to poll?
> 6. ShellCommand cancel + G1 pattern — is tokio::try_join! the correct concurrent drain pattern for all subprocess spawning in S13 verb crates?
> 7. enum TaskAction + dispatch — ADR-001 mandates enum + match + free functions per verb. Sketch the idiomatic shape. Should TaskAction be #[non_exhaustive]? (NO — closed sum of 5 verbs forever.) How should dispatch extract the right Caps slice per arm?
> 8. VerbCapabilities accessor pattern — does returning `ExecCaps<'_>` from `fn exec_caps(&self)` compose cleanly?
>
> DELIVER: idiomatic pattern for each point, concrete code sketches where helpful, and any gotchas that would delay S13 if unfixed. Under 2000 words.

## 1.4 — Agent 3: feature-dev:code-explorer

**Purpose:** map the TaskExecutor verb-method coupling surface so S13 bridges are clean.

**Prompt to pass:**

> You are mapping the coupling surface of nika-engine::runtime::executor::TaskExecutor to inform Session 13 verb crate extraction.
> Repository: /Users/thibaut/dev/supernovae/nika
>
> MUST READ:
> - tools/nika-engine/src/runtime/executor/mod.rs
> - tools/nika-engine/src/runtime/executor/exec.rs
> - tools/nika-engine/src/runtime/executor/fetch.rs
> - tools/nika-engine/src/runtime/executor/infer.rs
> - tools/nika-engine/src/runtime/executor/invoke.rs
> - tools/nika-engine/src/runtime/executor/agent.rs
> - tools/nika-engine/src/runtime/task_dispatch.rs (if exists)
> - docs/plans/constellation-session12-rework/07-session13-extraction-1.md
>
> DELIVER a precise map of:
> 1. TaskExecutor field inventory — list all fields. Mark each: thread into Caps / stay engine-internal / delete.
> 2. Verb method signatures — dump exact sigs for run_exec, run_fetch, run_invoke (S13 targets) and run_infer, run_agent (S14).
> 3. Per-verb dependency map — what does each verb ACTUALLY need? Validate against nika-kernel::caps.
> 4. Bridge pattern — sketch what engine's verb methods look like after extraction (delegate to nika_verb_X::run).
> 5. Hidden coupling — anything called via task_dispatch or Runner that reaches into TaskExecutor internals and would break the bridge.
> 6. Test surface impact — grep for TaskExecutor::new, ::with_*, etc. Count test sites that construct TaskExecutor directly.
> 7. Recommendation — for each of exec/fetch/invoke, is bridge-then-extract feasible in S13 without touching Runner? Which verb should be extracted FIRST (easiest first)?
>
> DELIVER: tables, file:line references, concrete blockers. Under 2500 words. No speculation — only what the code actually says.

## 1.5 — Agent 4: spn-rust:rust-architect

**Purpose:** validate the final architecture against idiomatic Rust before S13 commits.

**Prompt to pass:**

> You are validating the Nika Constellation S13 target architecture against idiomatic Rust patterns.
> Repository: /Users/thibaut/dev/supernovae/nika
>
> MUST READ:
> - docs/plans/constellation-session12-rework/01-architecture-vision.md
> - docs/plans/constellation-session12-rework/02-adr-001-enum-dispatch.md
> - docs/plans/constellation-session12-rework/03-adr-002-typed-contexts.md
> - docs/plans/constellation-session12-rework/07-session13-extraction-1.md
> - docs/plans/constellation-session12-rework/14-session12-handoff-postmortem.md (sections 5, 8, 9)
> - tools/nika-kernel/src/caps.rs
> - tools/nika-kernel/src/lib.rs
>
> REFERENCE PATTERNS TO COMPARE AGAINST:
> - Restate SDK Rust (Context<'ctx> pattern)
> - Ruff (Checker god-context + enum Rule)
> - uv (free functions per subcommand)
> - Dagster (typed resource injection)
>
> EVALUATE:
> 1. nika-runtime L3 crate design — `VerbCapabilities` bundle, `TaskAction` enum, `dispatch()` function. Is the downward dependency flow clean? Will it compile without cycles?
> 2. Per-verb crate shape — nika-verb-exec as a crate that ONLY depends on nika-kernel plus nika-core. Achievable? What if a verb needs a side-effect not in Caps today?
> 3. Error propagation strategy — each verb crate defines its own error; engine converts via From impls. Right split? Or should nika-runtime own a unified RuntimeError?
> 4. Capability extension protocol — when S14 needs to add `shield: &'a ShieldContext` to ExecCaps, will #[non_exhaustive] plus VerbCapabilities accessor updates be sufficient, or will verb crates need edits?
> 5. Async trait + Send + Sync bounds — are the Caps structs all Send + Sync? Do dyn trait objects propagate correctly across .await?
> 6. Testability — can each verb crate be unit-tested in isolation using nika-kernel-mock? Any infrastructure missing?
> 7. Flaws in the architecture — anything that looks right on paper but will be painful to implement? Concrete blockers with suggested alternatives.
>
> DELIVER: a verdict (GREEN / YELLOW / RED), flaws list, and any architectural improvements that should land before S13 starts. Under 2000 words.

## 1.6 — Synthesize findings

After all 4 agents return, reconcile:

- **P0 blockers** — must fix before S13 starts (add "S12-G3" commits if needed).
- **P1 improvements** — update the S13 commit plan before execution.
- **P2 nits** — note for S14+ cleanup.

**If any agent returns a RED verdict or a P0 blocker, STOP and report to the user.** Do not proceed to Phase 2 until blockers are resolved.

---

# PHASE 2 — Plan synthesis + user approval (~30 min)

## 2.1 — Write synthesis document

Create `docs/plans/constellation-session12-rework/16-session13-synthesis.md` (gitignored, local-only). It contains:

1. **Agent findings table** — one row per agent, verdict, top 3 issues each.
2. **Updated S13 commit plan** — incorporate fixes. If 07-session13-extraction-1.md needs amendments, document them as `AMEND-S13-*` entries.
3. **Risk register delta** — what new risks did the agents find?
4. **Code sketches for the 5 hardest type signatures** — at minimum: `VerbCapabilities` fields, accessor methods, `TaskAction` enum, `dispatch()` function, the first verb crate's `run()` signature.
5. **Your confidence level** (GREEN / YELLOW / RED) for proceeding to Phase 3.

## 2.2 — Present to user

Show the user:
- Synthesis summary (under 500 words)
- Any required pre-execution fixes (with commit plan if needed)
- Updated commit plan for Phase 3
- Request explicit authorization to start execution

**Do not proceed to Phase 3 without explicit user sign-off.**

---

# PHASE 3 — Execution (10–12h, 15–18 commits)

This is the actual S13 work. Follow the commit plan exactly. TDD every commit. Run the golden suite after every verb extraction commit.

## Commit ritual (apply to EVERY commit)

1. Mark TodoWrite item in_progress
2. Write failing test(s) first (TDD)
3. Write minimal implementation
4. Verify locally (ALL of these):
   - `cargo test -p <crate> --lib` (crate-level)
   - `cargo test -p nika-engine --lib runner::tests_golden_verbs` (oracle)
   - `cargo test --workspace --lib` (full suite)
   - `cargo clippy --workspace --lib -- -D warnings`
   - `cargo tree -p <new crate> | grep nika-engine` (diamond check if new crate — must be empty)
5. `git add <specific files>`
6. `git commit` with co-author trailer `Co-Authored-By: Nika 🦋 <nika@supernovae.studio>`
7. Mark TodoWrite item completed

## Phase 3.A — nika-runtime foundation (4 commits, ~2h)

**A1:** Create `tools/nika-runtime` crate with `VerbCapabilities` bundle (Arc-shared fields) and 5 accessor methods returning borrowed `Caps<'_>` slices. Add to workspace members.

**A2:** Add `TaskAction` enum (closed sum of 5 verbs, NOT `#[non_exhaustive]`) plus `dispatch()` function with 5 arms (3 todo! initially — filled as verb crates land). Per AMEND-4, dispatch is NOT live during S13.

> **Important trap:** `ExecParams`, `FetchParams`, etc. currently live in `nika-engine::ast`. `nika-runtime` cannot depend on `nika-engine` (diamond violation). Phase 1 agents must answer: can these param types move to `nika-core::ast` cleanly, or does S13 need a temporary generic trait bound? Resolve before A2.

**A3:** Unit tests for `VerbCapabilities` construction + accessor Send+Sync assertions + one smoke test using nika-kernel-mock.

**A4:** Workspace wire-up and placeholder verb crates (empty lib.rs) so nika-runtime can reference them without cycles. Actual verb content lands in phases B/C/D.

## Phase 3.B — nika-verb-exec (4 commits, ~2h)

**B1:** Create `nika-verb-exec` crate with `pub async fn run(params, caps: ExecCaps<'_>) -> Result<Value, VerbExecError>`. Dep on nika-kernel + nika-core only. NO nika-engine, NO nika-policy. TDD with MockShell + hand-rolled MockPolicyChecker.

**B2:** Engine bridge — `TaskExecutor::run_exec` builds `ExecCaps` from `&self` fields and delegates to `nika_verb_exec::run`. Add `impl From<VerbExecError> for NikaError` in engine error.rs.

**Critical verification after B2:** run `cargo test -p nika-engine --lib runner::tests_golden_verbs`. All 5 golden snapshots MUST match byte-for-byte. If not, STOP and investigate.

**B3:** Comprehensive unit test suite using nika-kernel-mock. Cover: happy path, policy denial, shell error propagation, timeout, cancel.

**B4:** Wire dispatch `Exec` arm in `nika-runtime::dispatch` (remove `todo!()`, add `nika-verb-exec` as nika-runtime dep).

## Phase 3.C — nika-verb-invoke (4–5 commits, ~2.5h)

Same pattern as Phase 3.B but for `nika-verb-invoke`. Extra complexity:
- Handles BOTH `nika:*` builtin routing AND MCP tool routing.
- Builtins via `nika-builtin` direct dep.
- MCP via trait injection (may need a new `McpClient` trait in `nika-kernel` if none exists — check first).

**Golden test check after every commit:** the `golden_invoke_builtin_log` snapshot must still match.

## Phase 3.D — nika-verb-fetch (5–6 commits, ~3h)

**D1:** Implement `ReqwestClient::send_streaming` in `tools/nika-http/src/lib.rs` with 50 MB early-abort. Use reqwest's `bytes_stream()` and wrap in a futures stream that tracks cumulative bytes + returns `HttpError::TooLarge` when threshold crossed. Accumulate size per chunk BEFORE yielding.

**D2:** Create `nika-verb-fetch` crate using `nika-extract` (for extraction) + `nika-policy` (for SSRF redirect policy).

**D3:** SSRF redirect policy wiring via `nika-policy::ssrf_safe_redirect_policy`.

**D4:** Engine bridge — `TaskExecutor::run_fetch` delegates. Golden test check.

**D5:** Wiremock-based unit tests for the verb crate (not integration tests — keep them under `--lib`).

**D6:** Wire dispatch `Fetch` arm.

## Phase 3.E — Close (2 commits)

**E1:** Update `tools/nika-engine/ARCHITECTURE.md` with the 4 new crates, updated LOC, updated dependency diagram. Update session memory file.

**E2:** Final regression verification — run full golden suite, full workspace test suite, clippy workspace-wide, all diamond checks. Commit any residual cleanup.

---

# Sacred invariants (NEVER violate)

1. **AGPL-3.0-or-later header** on every new `.rs` file.
2. **Co-author trailer:** `Co-Authored-By: Nika 🦋 <nika@supernovae.studio>` — NEVER Claude/Anthropic.
3. **Tests via `cargo test --workspace --lib` only** — no integration tests in `tests/` dirs (keychain risk).
4. **Zero `.unwrap()` / `.expect()`** in new production code. Tests may use them. `expect()` requires a `// REASON:` comment documenting the invariant.
5. **Diamond layering:** new verb crates MUST NOT depend on `nika-engine`. Verify: `cargo tree -p nika-verb-X | grep nika-engine` → empty. Every commit that adds a verb crate MUST be followed by this check.
6. **1 fix = 1 commit.** No batching unrelated changes.
7. **Push only after explicit user authorization.**
8. **No `trait Verb`.** Enum dispatch only (ADR-001). The verb set is 5, closed, forever.
9. **Verb crates receive Caps by reference** (`ExecCaps<'_>`), never own them.
10. **NEVER touch user's parallel launch-prep files:** `AGENTS.md`, `CLA.md`, `COMMERCIAL_LICENSE.md`, `CHANGELOG.md`, `README.md`, `MANIFESTO.md`, `CONTRIBUTING.md`, `CONVENTIONS.md`, `editors/*`, `docs/launch/`, `.github/SECURITY.md`.
11. **Every `tokio::process::Command` MUST set `cmd.kill_on_drop(true)`** before spawn. G1 lesson.
12. **Every concurrent pipe-reading code MUST use `tokio::try_join!`** with drain futures. NEVER sequential wait-then-read. G1 lesson.
13. **Every subprocess code MUST be regression-tested with >1 MB output.** No more dormant deadlocks. Applies to nika-verb-exec (GATE-S13-1).
14. **Golden test oracle MUST capture BOTH lifecycle AND output.** G2 lesson. Never weaken for convenience.
15. **Verification ritual MUST include `cargo check --no-default-features`** for crates with feature flags. G3 lesson.
16. **`parking_lot::RwLockReadGuard` is !Send** — NEVER hold it across `.await`. Drop the guard, then await, then reacquire. Policy enforcer must be `Arc<PolicyEnforcer>` with interior mutability, not `Arc<RwLock<PolicyEnforcer>>` held across await (GATE-S13-3).
17. **Phase 0 (context re-absorption) is NOT optional.** Budget 75 minutes (45 for handoff + 30 for new S13 gate docs).
18. **Phase 1 (4 parallel agents) is NOT optional.** Dispatch code-reviewer + rust-pro + code-explorer + rust-architect BEFORE writing code.
19. **Phase 2 (user sign-off) is NOT optional.** All 7 GATE-S13 items must be explicitly resolved with user approval before Phase 3.

---

# Traps, gotchas, landmines

## Rust-specific

- **`tokio::try_join!` pattern** — the G1 fix. Study `tools/nika-exec-runner/src/lib.rs` before writing any subprocess code.
- **`biased;` in `tokio::select!`** — priorities first branch. Decide per-use-case whether cancel-first or child-first is correct.
- **`Arc<dyn Trait>` vs `&'a dyn Trait` in Caps** — the asymmetry in S12 is intentional. Provider is Arc because it lives for the whole run; other capabilities are scoped to a task.
- **`#[non_exhaustive]` construction** — external crates cannot build the struct, but internal `nika-runtime` code CAN. Verb crates receive the struct by value or ref; they never construct it.
- **Feature flag propagation** — `nika-extract` has default features ON. Do not double-forward from engine or you get cfg confusion.
- **Workspace member ordering** — new crate members can be added anywhere in the `members` array but the corresponding `[workspace.dependencies]` entry must also be added.
- **`Pin<Box<dyn Stream + Send>>`** — for `HttpStreamResponse::body`. Use `futures_util::StreamExt` if available, else hand-roll. Needs Unpin for simple polling.
- **Blanket impls** — safe for umbrella traits (like `Filesystem` on `FsRead + FsWrite`). Watch for coherence if ever adding a second blanket impl.
- **Lifetime elision** — `fn exec_caps(&self) -> ExecCaps<'_>` works because Rust elides the lifetime. Making it explicit: `fn exec_caps<'a>(&'a self) -> ExecCaps<'a>`.

## Nika-specific

- **`cargo test --workspace --lib`** is the ONLY safe test command. Bare `cargo test` triggers macOS Keychain popups.
- **Engine's `pub use nika_policy as policy;` re-export** in `runtime/mod.rs` — `crate::runtime::policy::*` paths in engine still work. New verb crates should use `nika_policy::*` directly.
- **`provider: mock` is the only zero-config provider** — use it in all lib tests.
- **`quiet()` on Runner** — chain before `.run()` in tests to suppress output pollution.
- **`parse_analyzed` vs `parse_workflow`** — Runner consumes `AnalyzedWorkflow` from `parse_analyzed`.
- **Golden test snapshots** — in `tools/nika-engine/src/runtime/runner/snapshots/`. They capture lifecycle + output. `cargo insta accept --workspace` ONLY if you intentionally changed behaviour AND the user approves.
- **PolicyEnforcer name collision** — inherent `check_exec` + trait `PolicyChecker::check_exec`. Rust prefers inherent methods. In verb crates, you hold `&dyn PolicyChecker` so calls resolve through the trait (correct).

## Process

- **Phase 0, 1, 2 are NOT optional.** Read the handoff, run the agents, sync with user.
- **Run the golden suite AFTER EVERY verb extraction commit.** Not at the end of the phase. EVERY commit.
- **Report after each phase.** User needs checkpoints.
- **Pre-commit hooks** run format + clippy. Don't bypass with `--no-verify`.
- **Avoid amending commits.** Each fix is a new commit.
- **If a test fails, revert to green BEFORE investigating.** Never commit a red state.

---

# Done criteria (verify inline)

```
# All 15-18 commits landed
git log --oneline 304b1d3c2..HEAD | wc -l   # EXPECT: 15-18

# 4 new crates exist
ls tools/nika-runtime/Cargo.toml
ls tools/nika-verb-exec/Cargo.toml
ls tools/nika-verb-invoke/Cargo.toml
ls tools/nika-verb-fetch/Cargo.toml

# Workspace member count
grep -c '^    "nika' tools/Cargo.toml   # EXPECT: 32 (28 + 4)

# Diamond layering — all 4 new crates must be clean
cd tools
for c in nika-runtime nika-verb-exec nika-verb-invoke nika-verb-fetch; do
  echo -n "$c: "
  cargo tree -p $c 2>/dev/null | grep nika-engine && echo "BROKEN" || echo "clean"
done

# Engine LOC down
find nika-engine/src -name "*.rs" | xargs wc -l | tail -1
# EXPECT: around 143,800 (-2,600 from S13 start of 146,473)

# Tests
cargo test --workspace --lib 2>&1 | grep -E "^test result" | awk '{s+=$4} END{print s}'
# EXPECT: >= 10,805 plus N new verb crate tests

# Clippy
cargo clippy --workspace --lib -- -D warnings  # clean

# Golden regression oracle
cargo test -p nika-engine --lib runner::tests_golden_verbs  # 5 passed

# Per-verb unit tests
cargo test -p nika-verb-exec --lib
cargo test -p nika-verb-invoke --lib
cargo test -p nika-verb-fetch --lib
cargo test -p nika-runtime --lib

# Docs updated
grep -q 'nika-runtime' tools/nika-engine/ARCHITECTURE.md
grep -q 'nika-verb-exec' tools/nika-engine/ARCHITECTURE.md
grep -q 'nika-verb-invoke' tools/nika-engine/ARCHITECTURE.md
grep -q 'nika-verb-fetch' tools/nika-engine/ARCHITECTURE.md
```

**ALL must pass before you ask for push authorization.**

---

# Skills to announce and use

- **`spn-powers:executing-plans`** — MANDATORY, you are executing a documented plan
- **`spn-powers:test-driven-development`** — before writing any implementation code
- **`spn-powers:verification-before-completion`** — before every `git commit`
- **`spn-powers:systematic-debugging`** — if any test fails
- **`spn-rust:rust-core`** — trait design, error handling, ownership
- **`spn-rust:rust-async`** — tokio patterns, Send+Sync across await, Arc<dyn>

Announce skill usage clearly in chat: "I'm using the executing-plans skill to implement this plan."

---

# Reading list — quick reference (with absolute paths)

## Session 12 handoff (Tier 1 — MANDATORY)

- `/Users/thibaut/dev/supernovae/nika/docs/plans/constellation-session12-rework/14-session12-handoff-postmortem.md`
- `/Users/thibaut/dev/supernovae/nika/docs/plans/constellation-session12-rework/15-session13-mega-prompt.md`
- `/Users/thibaut/dev/supernovae/nika/docs/plans/constellation-session12-rework/13-plan-corrections.md`

## Original S13 plan + ADRs (Tier 2)

- `/Users/thibaut/dev/supernovae/nika/docs/plans/constellation-session12-rework/07-session13-extraction-1.md`
- `/Users/thibaut/dev/supernovae/nika/docs/plans/constellation-session12-rework/01-architecture-vision.md`
- `/Users/thibaut/dev/supernovae/nika/docs/plans/constellation-session12-rework/02-adr-001-enum-dispatch.md`
- `/Users/thibaut/dev/supernovae/nika/docs/plans/constellation-session12-rework/03-adr-002-typed-contexts.md`
- `/Users/thibaut/dev/supernovae/nika/docs/plans/constellation-session12-rework/09-risk-register.md`
- `/Users/thibaut/dev/supernovae/nika/docs/plans/constellation-session12-rework/11-kernel-trait-audit.md`

## S12 deliverables (Tier 3)

- `/Users/thibaut/dev/supernovae/nika/tools/nika-kernel/src/caps.rs`
- `/Users/thibaut/dev/supernovae/nika/tools/nika-kernel/src/policy.rs`
- `/Users/thibaut/dev/supernovae/nika/tools/nika-kernel/src/filesystem.rs`
- `/Users/thibaut/dev/supernovae/nika/tools/nika-kernel/src/shell.rs`
- `/Users/thibaut/dev/supernovae/nika/tools/nika-kernel/src/http.rs`
- `/Users/thibaut/dev/supernovae/nika/tools/nika-policy/src/lib.rs`
- `/Users/thibaut/dev/supernovae/nika/tools/nika-extract/src/lib.rs`
- `/Users/thibaut/dev/supernovae/nika/tools/nika-exec-runner/src/lib.rs` (G1 pattern reference)
- `/Users/thibaut/dev/supernovae/nika/tools/nika-engine/src/runtime/runner/tests_golden_verbs.rs` (oracle)
- `/Users/thibaut/dev/supernovae/nika/tools/nika-engine/src/runtime/runner/snapshots/` (snapshot dir)

## Project rules

- `/Users/thibaut/dev/supernovae/nika/CLAUDE.md`
- `/Users/thibaut/dev/supernovae/nika/.claude/rules/architecture.md`
- `/Users/thibaut/dev/supernovae/nika/.claude/rules/git-workflow.md`

---

# Closing instructions

1. **Do not skip Phase 0.** Read the full post-mortem. Read the ADRs. Read the S12 code. Budget 45 minutes minimum.

2. **Do not skip Phase 1.** Dispatch 4 agents in parallel. Study their findings. The previous session shipped 2 P0 bugs that would have been caught in Phase 1 if it existed then.

3. **Do not skip Phase 2.** Synthesize. Get user sign-off. Write it down.

4. **Phase 3 is mechanical if phases 0–2 were thorough.** Each commit is TDD + minimal impl + golden check + diamond check + commit.

5. **If you get stuck, stop and ask.** Don't guess at unclear type signatures. Don't fight the borrow checker for more than 10 minutes — step back and ask what the ownership story should be.

6. **Report after each phase.** Phase 0 report, Phase 1 report, Phase 2 synthesis, then Phase 3 per sub-phase (A, B, C, D, E).

7. **Push only after explicit authorization.**

8. **Remember the stakes:** launch is 2026-05-05. Session 13 is one of 3 refactor sessions remaining. A bug here doesn't just delay S13 — it delays S14 and risks the launch. Ultrathink. Ship clean.

---

## TL;DR for a fresh Claude Code session

> **You are continuing Session 13 of the Nika Constellation refactor after Session 12 completed and was pushed to origin/main. Do NOT start writing code immediately. First: re-absorb full context (Phase 0), then launch 4 review agents in parallel (Phase 1), then synthesize findings into a final plan (Phase 2), then execute with TDD and run the S12 golden regression suite after every verb extraction commit (Phase 3). Follow `docs/plans/constellation-session12-rework/15-session13-mega-prompt.md` exactly. Ultrathink. Do not skip phases.**

---

**End of Session 13 mega-prompt. Use it as-is or enrich further before handing off.**
