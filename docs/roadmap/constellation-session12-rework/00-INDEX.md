# Constellation Rework — Canonical Index

> **Read this file first.** Single source of truth for the state of every doc in this folder.
>
> **Gitignored** — local planning workspace, safe to modify freely.
>
> **Last updated:** 2026-04-11 (post-S14.5)
> **Current HEAD:** `12407d125` docs(constellation): ARCHITECTURE.md update for Session 14 + 14.5
> **Live metrics:** 35 crates · ~146,600 engine LOC · ~10,900 workspace tests · 0 clippy warnings
> **Next session:** S15 — read `23-session15-mega-prompt.md`

---

## Sessions shipped

| Session | Status | Date | Commits | Record |
|---|---|---|---|---|
| S12 Foundation | ✅ SHIPPED | 2026-04-10 | 22 (incl. G1/G2/G3 hotfixes) | `16-session-journal.md` lines 25–110, `14-session12-handoff-postmortem.md` |
| S13 Extraction Pass 1 | ✅ SHIPPED | 2026-04-10 | S13-A/B/C/D/E (3 verb crates + nika-runtime) | `16-session-journal.md` lines 113+, `.claude/rules/architecture.md` S13 block |
| S14 Wave A–B + S14.5 | ✅ SHIPPED | 2026-04-11 | 5 (α,β,γ,δ,ε) + 2 hotfix (53513e5ee, 144f5abeb) + 1 docs (12407d125) | `16-session-journal.md` lines 287–350, `20b-session14-scope-correction.md` |
| S15 (McpPool + infer bridge) | ⏳ NEXT | — | ~8 (A0–A7) | `23-session15-mega-prompt.md` |

---

## Document status

### ACTIVE (read these for next session)

| File | Status | Description |
|---|---|---|
| `00-INDEX.md` | ACTIVE | **This file.** Canonical index + doc status board. |
| `16-session-journal.md` | ACTIVE | Chronological journal of every S12/S13/S14/S14.5 commit + bug + fix + lesson. Append S15 here on completion. |
| `23-session15-mega-prompt.md` | ACTIVE ⭐ | **Canonical S15 plan.** 444+ lines, post-review enriched. Baseline HEAD `12407d125`. |
| `22-agent-v2-design.md` | ACTIVE (DRAFT) | Wave C / nika-verb-agent design doc. Separate concern, targets S15+/S16. Reviewed by 4 agents, verified against real code. |
| `20b-session14-scope-correction.md` | ACTIVE (REFERENCE) | S14 Phase 0/1 postmortem. Why 20-mega-prompt was stale + what actually shipped. |

### HISTORICAL (frozen records, do not modify)

| File | Status | Description |
|---|---|---|
| `01-architecture-vision.md` | HISTORICAL | Original end-state architecture vision (diamond crates, VerbCapabilities, dispatch). Still valid blueprint. |
| `02-adr-001-enum-dispatch.md` | HISTORICAL (frozen) | ADR-001: enum TaskAction + match beats trait Verb for closed 5-verb set. |
| `03-adr-002-typed-contexts.md` | HISTORICAL (frozen) | ADR-002: per-verb borrowed `*Caps<'a>` beats monolithic `VerbCtx`. |
| `04-adr-003-nika-extract.md` | HISTORICAL (frozen) | ADR-003: `extract.rs` becomes its own pure L2 crate. ✅ Shipped S12-F7. |
| `05-adr-004-delete-task-executor.md` | HISTORICAL (frozen) | ADR-004: TaskExecutor deleted, not refactored. Status: DEFERRED to S15+ (not yet executed). |
| `09-risk-register.md` | HISTORICAL (frozen) | Every known landmine. Check against S15 before execution. |
| `11-kernel-trait-audit.md` | HISTORICAL | Pre-S12 kernel trait inventory. Reference baseline. |
| `12-review-synthesis.md` | HISTORICAL | Pre-S12 4-agent review synthesis. |
| `13-plan-corrections.md` | HISTORICAL | S12 plan corrections (AMEND-1/2/3). |
| `14-session12-handoff-postmortem.md` | HISTORICAL (frozen) | 784-line S12 postmortem. Canonical S12 lessons record. |
| `19-socratic-review.md` | HISTORICAL | Cross-session Socratic review (7 GATE-S13 + 3 GATE-S14 items). All resolved. |

### SUPERSEDED (banner added, kept for context)

| File | Status | Superseded by |
|---|---|---|
| `00-mega-plan.md` | SUPERSEDED | `16-session-journal.md` for actual commits. Keep for original top-level roadmap thinking. |
| `06-session12-foundation.md` | SUPERSEDED (implicit) | `14-session12-handoff-postmortem.md` + journal S12 entry. Historical plan, S12 shipped. |
| `07-session13-extraction-1.md` | SUPERSEDED (implicit) | S13 shipped; see `.claude/rules/architecture.md` S13 block. |
| `08-session14-extraction-2.md` | SUPERSEDED (banner) | `20b-session14-scope-correction.md` + `23-session15-mega-prompt.md`. Original ambitious 20-commit S14 plan. |
| `10-migration-verification.md` | SUPERSEDED (implicit) | Per-session plans now carry their own verification. S12 proved the pattern. |
| `15-session13-mega-prompt.md` | SUPERSEDED (banner) | Journal S13 entry + `.claude/rules/architecture.md`. S13 shipped. |
| `17-session14-enriched-plan.md` | SUPERSEDED (banner) | `20b-session14-scope-correction.md` + `23-session15-mega-prompt.md`. Written pre-S13, assumed different scope. |
| `20-session14-mega-prompt.md` | SUPERSEDED (banner) | `20b-session14-scope-correction.md`. Handoff was stale (written vs HEAD `a3e8d8ab8`, 11 commits drifted). |
| `21-session15-handoff.md` | SUPERSEDED (banner) | `23-session15-mega-prompt.md`. Draft v1, enriched into 23. |
| `MEGA-PROMPT-S12-FOUNDATION.md` | HISTORICAL (banner) | Original S12 copy-paste prompt. S12 shipped long ago. |
| `README.md` | HISTORICAL | Original folder index. Superseded by this `00-INDEX.md`. |

---

## Quick navigation for next session

**If you're starting S15:**
1. Read `23-session15-mega-prompt.md` (canonical plan)
2. Read `16-session-journal.md` Session 14 entry (lines 287+) for context on what shipped + what was deferred
3. Read `20b-session14-scope-correction.md` for the Phase 0 drift lesson
4. Read `.claude/rules/architecture.md` invariants #1–#25 (especially #17–#25 from S14/S14.5)
5. Run pre-flight: `git log --oneline -10` — verify HEAD is `12407d125` or later
6. Follow S15 Phase 0/1/2/3 protocol from the mega-prompt

**If you're looking at S14:**
- What actually shipped: `16-session-journal.md` lines 287+
- Why scope was corrected: `20b-session14-scope-correction.md`
- Original ambitious plan (for curiosity): `08-session14-extraction-2.md` / `17-session14-enriched-plan.md`

**If you're looking at Wave C / agent extraction:**
- Design: `22-agent-v2-design.md` (reviewed, verified against real kernel code)
- Deferred from S14 to S15+/S16

**If you're looking at S12/S13 history:**
- S12: `16-session-journal.md` lines 25–110 + `14-session12-handoff-postmortem.md`
- S13: `16-session-journal.md` lines 113+ (truncated) + `.claude/rules/architecture.md` S13 block

---

## Invariants checklist (from `.claude/rules/architecture.md`)

Session-specific sacred rules codified across S12/S13/S14:

- **#1–#10** — original L0/L1/L2 layering rules
- **#11** kill_on_drop(true) on every tokio::process::Command (G1 lesson)
- **#12** tokio::try_join! for pipe drain, never sequential wait→read (G1 lesson)
- **#13** >1MB output regression test for subprocess (G1 lesson)
- **#14** Golden test oracle asserts BOTH lifecycle AND output (G2 lesson)
- **#15** `cargo check --no-default-features` mandatory for feature-gated crates (G3 lesson)
- **#16** `parking_lot::RwLockReadGuard` is !Send — never hold across `.await`
- **#17** Unified `InferRequest` is the kernel's single LLM call shape (S14 W14-A0)
- **#18** Capability queries on Provider trait, not on ModelCapabilities (S14 W14-A0)
- **#19** Per-crate `new()` constructors on all `#[non_exhaustive]` structs (S14 W14-A0)
- **#20** Verb-crate minimum extraction is valid architecture (S14 W14-B1)
- **#21** StopReason ↔ FinishReason mapping lives at verb-crate/event boundary (S14 W14-B1)
- **#22** TEMP engine deps declared with `# TEMP` comment + clearance condition (S14 W14-B1)
- **#23** Kernel-adjacent helpers stay primitive-typed (no reqwest/tokio leaks) (S14.5)
- **#24** Exactly one `EventKind::*` emit site per file (S14.5)
- **#25** All verb-crate errors `#[non_exhaustive]` from day one (S14.5)

---

## Inventory count

27 files total in this folder (including this INDEX):

- 1 canonical active plan (23)
- 1 active journal (16)
- 2 active reference (20b, 22)
- 12 historical/frozen (01, 02, 03, 04, 05, 06, 07, 09, 10, 11, 12, 13, 14, 19)
- 6 superseded with banner (08, 15, 17, 20, 21, MEGA-PROMPT-S12-FOUNDATION)
- 2 index/README (00-INDEX, README — original README kept for historical context)
- 1 top-level mega-plan (00-mega-plan)
