# Nika Session 12 — Foundation Phase Mega Prompt

> ⚠️ **HISTORICAL 2026-04-11** — Session 12 HAS SHIPPED (22 commits on `origin/main`, 2026-04-10). This is the original pre-execution mega-prompt, preserved for historical reference. For the real S12 record, read `16-session-journal.md` (Session 12 entry) and `14-session12-handoff-postmortem.md`.
>
> ---

> Copy everything below the line into a fresh Claude Code session.

---

# Nika Session 12 Foundation — Constellation V2.3 Architecture Rework

You are Claude Code starting the **Foundation phase of Session 12** of the Nika Constellation refactor.

**Project:** `/Users/thibaut/dev/supernovae/nika` · **Launch:** 2026-05-05 (J-25)

## What happened before you

Session 12 started with a D/P/E phase (6 commits already landed on local main, NOT pushed):
- S12.D1 hard error on agent cwd (`3897870e8`)
- S12.D2 reject `..` in glob patterns (`c92075fe2`) — TDD reproduced real vault.enc leak
- S12.D3+E1 canonicalize cache + hard error (`2381c491c`)
- S12.P1 `current_is_tainted()` + `BuiltinError::denied()` (`ffdfa770d`)
- S12.P2 `file/limits.rs` u64 unification (`30f7a67a5`)
- S12.P3 `file/test_util.rs run_as()` helper (`1c5d48954`)

**Then:** 4 parallel research agents (code-explorer, rust-architect, web-researcher, rust-pro) were dispatched to validate the original Phase 13 plan. They **unanimously rejected** the original architecture and converged on a cleaner design. A 15-document plan was written and independently reviewed.

**HEAD:** `1c5d48954` (6 commits ahead of `c5ea27438` S11 HEAD). **Release binary baseline: 118 MB.**

## Your job: execute the Foundation phase (11 commits, ~5h)

You are implementing `docs/plans/constellation-session12-rework/06-session12-foundation.md` with corrections from `docs/plans/constellation-session12-rework/13-plan-corrections.md`.

**The 11 commits in strict order:**

1. `feat(kernel): PolicyChecker trait` — 4 methods, ~50 LOC, sealed not needed, 3-4 unit tests
2. `feat(kernel): HttpClient::send_streaming + HttpStreamResponse` — additive, default error impl for existing impls
3. `feat(kernel): cancellation support in ShellExecutor` — `ShellCommand::cancel: Option<CancellationToken>`, update TokioShell in nika-exec-runner, 2 async tests
4. `feat(kernel): split Filesystem into FsRead + FsWrite splinters` — blanket umbrella impl, compile-time narrowing test
5. `feat(policy): create nika-policy L1 crate` — move PolicyEnforcer from engine (1263 LOC), impl PolicyChecker, no nika-engine dep (diamond!)
6. `chore(engine): delete duplicated policy code` — ~20 call sites rewired to nika-policy
7. `feat(extract): create nika-extract L2 crate` — move extract.rs (1327 LOC) verbatim, pure functions, zero async
8. `chore(engine): delete runtime/executor/extract.rs` — engine shrinks −1327 LOC
9. `feat(kernel): add ExecCaps/FetchCaps/InferCaps/InvokeCaps/AgentCaps` — per-verb borrowed-slice context structs (types only, NOT wired yet — Session 13 does wiring)
10. `docs(constellation): ARCHITECTURE.md + session12 memory update`
11. `test(runtime): golden e2e regression tests for all 5 verbs` — via Runner::run with provider:mock, insta snapshots (AMEND-1 from review)

## Mandatory pre-flight

```bash
cd /Users/thibaut/dev/supernovae/nika
git log --oneline -7                                  # expect: 6 S12 D/P/E commits on top of c5ea27438
git status                                            # expect: clean on tools/ (AGENTS.md etc = user's parallel work, DO NOT TOUCH)
cd tools
cargo test --workspace --lib 2>&1 | grep -E "^test result" | awk '{s+=$4} END{print s}'
# expect: ~10785
cargo clippy --workspace --lib -- -D warnings          # expect: clean
find nika-engine/src -name "*.rs" | xargs wc -l | tail -1
# expect: 148792
```

## Files you MUST read in this exact order

**Architecture documents (read in full — these are the SOURCE OF TRUTH):**
1. `docs/plans/constellation-session12-rework/README.md` — index + sacred invariants
2. `docs/plans/constellation-session12-rework/13-plan-corrections.md` — **READ THIS SECOND — authoritative amendments that override all other docs**
3. `docs/plans/constellation-session12-rework/06-session12-foundation.md` — your detailed commit-by-commit plan (47k, from rust-architect agent)
4. `docs/plans/constellation-session12-rework/01-architecture-vision.md` — the end-state you're building toward
5. `docs/plans/constellation-session12-rework/02-adr-001-enum-dispatch.md` — why NO `trait Verb`
6. `docs/plans/constellation-session12-rework/03-adr-002-typed-contexts.md` — why per-verb Caps<'a> structs
7. `docs/plans/constellation-session12-rework/04-adr-003-nika-extract.md` — why extract is its own crate
8. `docs/plans/constellation-session12-rework/11-kernel-trait-audit.md` — kernel trait priority list

**Code files (read strategically):**
9. `nika/CLAUDE.md` — project rules
10. `nika/.claude/rules/architecture.md` — diamond layering rules
11. `nika/.claude/rules/git-workflow.md` — 1 fix = 1 commit
12. `tools/nika-kernel/src/lib.rs` — existing 10 trait modules
13. `tools/nika-kernel/src/shell.rs` — ShellExecutor trait (you're adding cancellation)
14. `tools/nika-kernel/src/http.rs` — HttpClient (you're adding send_streaming)
15. `tools/nika-kernel/src/filesystem.rs` — Filesystem (you're splitting into FsRead+FsWrite)
16. `tools/nika-engine/src/runtime/policy.rs` (skim top 200 — you're extracting to nika-policy)
17. `tools/nika-engine/src/runtime/executor/extract.rs` (skim top 100 — you're extracting to nika-extract)
18. `tools/nika-exec-runner/src/lib.rs` — TokioShell (you're updating for cancellation)
19. `tools/nika-http/src/lib.rs` — ReqwestClient (you're adding send_streaming default)

**Memory (skim):**
20. `~/.claude/projects/-Users-thibaut-dev-supernovae-nika/memory/project_constellation_session12.md`
21. `~/.claude/projects/-Users-thibaut-dev-supernovae-nika/memory/MEMORY.md`

## Skills to announce and use

- **`spn-powers:executing-plans`** — REQUIRED: you are executing a plan from docs/plans/
- **`spn-powers:test-driven-development`** — BEFORE every code change (write failing test first)
- **`spn-powers:verification-before-completion`** — BEFORE every `git commit`
- **`spn-powers:systematic-debugging`** — if any test fails
- **`spn-rust:rust-core`** — trait/sealed/task_local patterns

## Sacred invariants (violate = task fails)

1. **AGPL-3.0-or-later** header on every new file
2. **Co-author: `Nika 🦋 <nika@supernovae.studio>`** — NEVER Claude/Anthropic
3. **Tests only via** `cargo test --workspace --lib` (no keychain)
4. **Zero `.unwrap()` / `.expect()`** in new production code
5. **Diamond layering:** `nika-policy` and `nika-extract` MUST NOT depend on `nika-engine`. Verify with `cargo tree -p <crate> | grep nika-engine` after each new crate.
6. **1 fix = 1 commit**
7. **Push only after explicit user authorization**
8. **No `trait Verb`** — enum dispatch only (ADR-001)
9. **Per-verb Caps structs live in `nika-kernel`** with `&'a dyn PolicyChecker` (trait objects), NOT in nika-runtime with concrete types (AMEND-2 from 13-plan-corrections.md)
10. **Never touch** AGENTS.md, .github/SECURITY.md, CLA.md, docs/launch/ — user's parallel launch-prep work

## Key architectural decisions already taken (DO NOT re-debate)

- **enum + free functions for dispatch** — `TaskAction` + match in nika-runtime, one `pub async fn run()` per verb crate (ADR-001)
- **Per-verb borrowed slices** — `ExecCaps<'a>`, `FetchCaps<'a>` etc. from run-scoped `VerbCapabilities` (ADR-002)
- **`nika-extract` is pure** — zero I/O, zero async, 9 extract modes (ADR-003)
- **TaskExecutor gets DELETED** in Session 14 — replaced by `VerbCapabilities` (ADR-004)
- **`dispatch()` is NOT live during S13** — engine's task_dispatch stays the live path until S14 Wave C (AMEND-4)
- **HttpClient stays thin** — `nika-verb-fetch` owns reqwest directly for redirect policy (kernel trait audit)
- **PolicyChecker is a trait** in nika-kernel, concrete `PolicyEnforcer` impl in nika-policy (resolved debate)

## What this session explicitly does NOT do

- Extract any verb into a new `nika-verb-*` crate (that's Session 13)
- Delete TaskExecutor (that's Session 14)
- Create nika-runtime (that's Session 13)
- Wire ExecCaps into any runtime code (that's Session 13)
- Wire `dispatch()` function (that's Session 13)
- Implement `ReqwestClient::send_streaming` (that's Session 13 when nika-verb-fetch needs it)
- Move `StructuredOutputEngine` (that's Session 14)
- Touch `rig_agent_loop/` (that's Session 14)
- Push to remote (await explicit authorization)

## Done criteria

- [ ] 11 commits landed on local main
- [ ] `cargo test --workspace --lib` green (~10,800 tests)
- [ ] `cargo clippy --workspace --lib -- -D warnings` clean
- [ ] Engine LOC: 148,792 → ~146,200 (−2,590)
- [ ] 2 new crates: `nika-policy` (L1), `nika-extract` (L2)
- [ ] Crate count: 28 → 30
- [ ] Diamond verified: `cargo tree -p nika-policy | grep nika-engine` = empty
- [ ] Diamond verified: `cargo tree -p nika-extract | grep nika-engine` = empty
- [ ] 5 per-verb Caps structs defined in `nika-kernel/src/caps.rs`
- [ ] Golden e2e tests for 5 verbs via Runner::run (AMEND-1)
- [ ] ARCHITECTURE.md updated (crate count, LOC, Phase 13 target section)
- [ ] Session memory file written
- [ ] User authorized push
- [ ] `git push origin main` completed

## Workflow

```
Read plans → Create TodoWrite tasks → Execute commit-by-commit → Report per batch of 3 → Continue → Close
```

Use the executing-plans skill. TDD for every commit. Batch-report every 3 commits for feedback.

**Ultrathink. Ship clean. The architecture rework is the real win — make it count.**
