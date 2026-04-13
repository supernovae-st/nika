# Constellation V2.3 — Session 12 Rework Plan

> **Status:** Authoritative blueprint for Sessions 12/13/14 of the Nika Constellation refactor.
> **Supersedes:** `docs/plans/2026-04-10-constellation-session12-handoff.md` (the original Phase 13 plan, which 4 parallel research agents proved architecturally suboptimal).
> **Date:** 2026-04-10 · **Launch gate:** 2026-05-05 (J-25).

---

## Why this folder exists

The original Session 12 handoff proposed a straightforward "move verb files into new crates" approach to Phase 13. Before executing, four parallel research agents (`feature-dev:code-explorer`, `spn-rust:rust-architect`, `spn-search:web-researcher` studying Restate/Ruff/uv/Dagster, `spn-rust:rust-pro` auditing kernel traits) were dispatched to validate the approach against the cleanest possible Rust architecture.

**The research converged — unanimously — on a radically different design.** The original plan would have:
- Left the 10 existing kernel traits as dead code
- Duplicated `tokio::process::Command` and raw `reqwest::Client` into the new verb crates
- Preserved `TaskExecutor` as a god object
- Created a `trait Verb` that Rust idioms reject for a closed 5-verb set
- Missed the biggest architectural win: `extract.rs` (1327 LOC) is pure and belongs in its own crate

This folder contains the corrected plan. It is larger in scope (3 sessions vs 1) but produces the architecture the Constellation V2.3 targets document describes, not a LOC-shuffling exercise.

---

## Document index

### Vision & decisions

| File | Purpose |
|---|---|
| [`00-mega-plan.md`](00-mega-plan.md) | Top-level roadmap across Sessions 12/13/14 — metrics, timeline, gate criteria |
| [`01-architecture-vision.md`](01-architecture-vision.md) | The end-state architecture — diamond crates, VerbCapabilities, dispatch, verb crates |
| [`02-adr-001-enum-dispatch.md`](02-adr-001-enum-dispatch.md) | **ADR-001:** `enum TaskAction + match` beats `trait Verb` for a fixed 5-verb set |
| [`03-adr-002-typed-contexts.md`](03-adr-002-typed-contexts.md) | **ADR-002:** Per-verb borrowed `ExecCaps<'a>`/`FetchCaps<'a>`/... beats monolithic `VerbCtx` |
| [`04-adr-003-nika-extract.md`](04-adr-003-nika-extract.md) | **ADR-003:** `extract.rs` (1327 LOC) becomes its own pure L2 crate |
| [`05-adr-004-delete-task-executor.md`](05-adr-004-delete-task-executor.md) | **ADR-004:** `TaskExecutor` is deleted, not refactored |

### Implementation plans (one per session)

| File | Session | Scope | Commits | Hours |
|---|---|---|---|---|
| [`06-session12-foundation.md`](06-session12-foundation.md) | **S12** (this session, continuation after D/P/E) | Kernel trait extension, `nika-policy`, `nika-extract`, `*Caps` types | 10 | ~4 |
| [`07-session13-extraction-1.md`](07-session13-extraction-1.md) | **S13** (next session) | `nika-runtime`, `nika-verb-exec`, `nika-verb-invoke`, `nika-verb-fetch` | 15-18 | 10-12 |
| [`08-session14-extraction-2.md`](08-session14-extraction-2.md) | **S14** (final session) | `nika-shield`, `nika-verb-infer`, `nika-verb-agent`, **delete TaskExecutor** | 20 | 14-18 |

### Operations

| File | Purpose |
|---|---|
| [`09-risk-register.md`](09-risk-register.md) | Every known landmine across all 3 sessions with mitigation plans |
| [`10-migration-verification.md`](10-migration-verification.md) | How to prove each commit is safe — golden tests, layer guards, LOC checks |
| [`11-kernel-trait-audit.md`](11-kernel-trait-audit.md) | Priority-ranked audit of nika-kernel traits (from `rust-pro` research) |

### Review & corrections (critical — read before execution)

| File | Purpose |
|---|---|
| [`12-review-synthesis.md`](12-review-synthesis.md) | Independent review by `feature-dev:code-reviewer` — 🟡 YELLOW verdict, 5 concerns |
| [`13-plan-corrections.md`](13-plan-corrections.md) | **Authoritative amendments** to docs 00-11. Where this file contradicts them, this wins. |

---

## Quickstart — What runs tonight

**Session 12 continues.** 6 commits already landed (D1-3 security, P1-3 polish, see Phase D/P/E in the original handoff). The remaining 10 commits are in [`06-session12-foundation.md`](06-session12-foundation.md). They:

1. Add `PolicyChecker` trait to `nika-kernel` (~50 LOC, 4 methods)
2. Add `HttpClient::send_streaming` + `HttpStreamResponse` (additive)
3. Add `CancellationToken` to `ShellExecutor::ShellCommand`
4. Split `Filesystem` → `FsRead` + `FsWrite` splinters
5. Create `nika-policy` L1 crate (move `PolicyEnforcer` verbatim, ~1263 LOC)
6. Delete `PolicyEnforcer` duplicate from `nika-engine`
7. Create `nika-extract` L2 crate (move `extract.rs` verbatim, ~1327 LOC)
8. Delete `extract.rs` from `nika-engine`
9. Add `ExecCaps<'a>`/`FetchCaps<'a>`/`InferCaps<'a>`/`InvokeCaps<'a>`/`AgentCaps<'a>` structs (types only, not wired)
10. Update `ARCHITECTURE.md` + Session 12 memory file

**Engine LOC impact S12:** 148,792 → ~146,200 (−2,590). No verb extraction yet. No TaskExecutor deletion yet.

**Session 13 starts next.** Creates `nika-runtime` + 3 verb crates (exec, invoke, fetch). Engine → ~143,800.

**Session 14 finishes.** Creates `nika-shield` + 2 verb crates (infer, agent). Deletes `TaskExecutor`. Engine → ~138,500. Full target `<=100k` met in Phase 15+.

---

## The 6 unanimous research findings

1. **No `trait Verb`.** Use `enum TaskAction` + `match` + free `pub async fn run()` per crate. Rust idioms for closed sums. (ADR-001)
2. **Per-verb typed contexts.** `ExecCaps<'a>` borrowed from `VerbCapabilities`. Compile-time capability enforcement. (ADR-002)
3. **`nika-extract` is its own crate.** 1327 LOC of pure byte→structured output — not a side effect, not a verb. (ADR-003)
4. **Delete `TaskExecutor` entirely.** 22-field god object replaced by `VerbCapabilities` + free functions. (ADR-004)
5. **`nika-runtime` is the new L3.** Dispatcher lives there. `nika-engine` dissolves into a thin shim.
6. **Extend kernel traits BEFORE extraction.** The existing 10 traits bypass the raw `tokio::process` / `reqwest` calls. Foundation-first.

---

## Sacred invariants (unchanged across all sessions)

1. **AGPL-3.0-or-later** header on every new file
2. **Co-author** line: `Nika 🦋 <nika@supernovae.studio>` — NEVER Claude/Anthropic
3. **Tests only via** `cargo test --workspace --lib` (no keychain popups)
4. **Zero** `.unwrap()` / `.expect()` in new production code
5. **Diamond layering**: every `nika-verb-*` crate must compile WITHOUT `nika-engine` in its dep graph (TEMP exceptions documented inline)
6. **1 fix = 1 commit** (atomic refactors that break compile if split = exception)
7. **Push only after explicit user authorization**
8. **No new verbs.** 5 verbs sacred forever.
9. **No `trait Verb`.** Enum dispatch only.
10. **Never mask bugs.** Test, don't delete-to-pass.

---

## Research sources (inline citations)

- **Restate SDK Rust** (Context<'ctx> trait-sliced pattern): https://github.com/restatedev/sdk-rust/blob/main/src/context/mod.rs
- **Ruff** (Checker god-context + enum Rule): https://github.com/astral-sh/ruff/blob/main/crates/ruff_linter/src/checkers/ast/mod.rs
- **uv** (free functions per subcommand): https://github.com/astral-sh/uv/blob/main/crates/uv/src/commands/mod.rs
- **Dagster** (typed resource injection): https://docs.dagster.io/concepts/resources
- **Rust API Guidelines** (C-OBJECT for open sums, enums for closed): https://rust-lang.github.io/api-guidelines/

Full research synthesis available in conversation transcripts of the 4 parallel agents dispatched 2026-04-10.
