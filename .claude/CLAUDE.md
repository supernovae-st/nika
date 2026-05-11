# Nika Diamond — Claude Code rules

**Branch** : `main` — DEFAULT working branch (production · renamed 2026-05-06 from `nika-diamond`).
**Brouillon** : `830aa6154` (legacy v0.79.3 anchor) — read-only reference via `git show brouillon:path`. NEVER checkout, NEVER modify, NEVER push. Access legacy code ONLY via `git show`.
**Legacy binary** : `~/bin/nika-legacy` — pre-built v0.79 for parity tests (Phase 5+).
**This is NOT extraction. This is CRAFT.** Each crate rewritten from scratch, guided by legacy. User learns Rust in parallel.

## ⚙️ Hook & settings model

`.claude/settings.json` (this repo, public) loads at Claude Code
**process startup** — edits to hooks do **not** take effect until the
next session restart. Pair with `.claude/settings.local.json`
(gitignored) for HQ-coupled / private overlay hooks; Claude Code
merges both at load time.

## 🔐 Authority hierarchy

1. `~/.claude/projects/-Users-thibaut-dev-supernovae-hq/memory/POST_AUDIT_REVISIONS.md` — SUPREME AUTHORITY, overrides all other docs.
2. `~/.claude/.../PRE_LAUNCH_GATES.md` — 7 shadow zones mandatory before v0.90.
3. `~/.claude/.../HANDOFF_PHASE_1_REVISED.md` — current execution plan.
4. `.claude/rules/*.md` (this directory) — project-specific enforcement.
5. `~/.claude/.../project_ai_velocity_north_star.md` — WHY diamond (decision filter).

If any doc contradicts another, **POST_AUDIT_REVISIONS wins**.

## 🎯 What we're doing

Nika Diamond = 40-42 crates architecture (cap 100). Building on fresh
orphan branch. Each crate passes 12 gates before admission to workspace.
Count finalized by POST_AUDIT_REVISIONS 2026-04-14 — includes pck + natives.

Timeline honnête : 11-12 mois total. No deadline pressure — quality > speed.
Current: Phase D (parser scaffolding, Round 2c+2d+2e-part-1 DONE). HEAD `ee74d97e0` — 7 crates in workspace (6 admitted + 1 WIP), **905 lib tests**, 32 providers, 49 capability rules. See `scripts/refresh-status.sh` for the canonical block.

## 🚫 Interdits stricts

- ❌ Co-Authored-By: Claude (always Nika 🦋 `<nika@supernovae.studio>`)
- ❌ Copy-paste from brouillon verbatim (rewrite propre requis, brouillon = reference only)
- ❌ git checkout brouillon or modify brouillon in any way
- ❌ Admit crate to workspace without all 12 gates passing
- ❌ `.unwrap()` or `.expect(` in src/ (use `?` propagation)
- ❌ `#[allow(dead_code)]` (delete or pub(crate))
- ❌ Files >1500 LOC (split into modules)
- ❌ `git add -A` or `git add .` (stage by explicit path)
- ❌ `cargo test --test` (macOS Keychain popup — use `--lib` only)
- ❌ `--no-verify` on commits
- ❌ Push without explicit user GO

## ✅ Mandatory patterns

- ✓ TDD : tests first, implementation second
- ✓ Mutation testing ≥90% killed per crate (cargo-mutants)
- ✓ Review swarm (3 agents) before each crate admission :
  spn-nika:code-reviewer + spn-rust:rust-pro + feature-dev:code-reviewer
- ✓ Atomic commit : 1 crate admission = 1 commit
- ✓ `#[non_exhaustive]` on all public error enums + response structs
- ✓ Every I/O behind kernel trait (MemoryStore, ShellExecutor, etc.)
- ✓ workspace.lints.clippy `unwrap_used = "deny"` enforced
- ✓ Commit message : `feat(nika-X): admit to workspace — all 12 gates passed`
- ✓ Tout refactor/rename touchant un symbole Rust → grep callers + impact analysis AVANT edit

## 📋 12 Gates per crate admission

Read full spec in `docs/adr/adr-003-12-gate-admission.md` + `docs/architecture/forward-compat-invariants.md`. Summary :

1. SPEC — `docs/crate-specs/nika-X.md` exists (purpose, layer, LOC budget, public API)
2. TDD — tests written before impl, RED then GREEN
3. IMPL — minimal, compiles, tests pass, no `# TEMP` without removal plan
4. CLIPPY 0 — `cargo clippy --workspace --all-targets -- -D warnings`
5. MUTATION ≥90% — `cargo mutants -p nika-X`
6. PROPERTY — proptest if sensitive (security, parsers, encoding)
7. BENCHMARKS — `benches/` if hot path
8. DOCS — `cargo doc --no-deps` 0 warnings, pub items documented
9. CANARY E2E — `tests/canary-X.nika.yaml` passes (or exemption)
10. PARITY LEGACY — golden test vs `git show brouillon:...` output
11. REVIEW SWARM — 3 agents parallel, P0/P1 fixed same session
12. ATOMIC COMMIT — 1 commit, co-authored Nika 🦋

## 📐 Architecture invariants

- L0 crates : zero I/O, zero async, ≤15k LOC
- L0.5 crates : traits only (nika-kernel, nika-kernel-mock)
- L1 effect crates : 1 trait impl each (clock/fs/http/blob/process/etc.)
- L2 domain crates : verb-*, service crates, memory stubs
- L3 orchestration : runtime + daemon
- L4 interfaces : cli, lsp, serve, sdk, init, lints
- L5 binary : nika (<500 LOC composition root)

Strict downward dependencies only. No upward imports. `cargo-deny` enforces
via `[[bans.deny]] + wrappers` per layer.

## 🔧 Tools installed / mandatory

```
cargo-nextest       — test runner (process-per-test isolation)
cargo-insta         — snapshot testing
cargo-deny          — license + advisories + layer enforcement
cargo-machete       — unused deps
cargo-public-api    — API surface diff
cargo-semver-checks — breaking change detection
cargo-mutants       — mutation testing
dylint + nika-lints — custom architectural lints (Phase 4+)
```

## 🎯 Current state

> Single source of truth: `bash scripts/refresh-status.sh`. The block
> below is regenerated by that script and parity-enforced by hygiene
> vector 23 (`check-status-claims-sync.sh`).

<!-- AUTO-GENERATED by scripts/refresh-status.sh — do not edit by hand -->
<!-- Status drift between this block and any quoting doc is caught by
     scripts/hygiene/check-status-claims-sync.sh (vector 23). -->

| field            | value                                          |
|------------------|------------------------------------------------|
| branch           | `main`                                      |
| HEAD             | `6af1b7ced` (post BLUEPRINT v1.3 + W3 prep · nika-bm25 scaffold)             |
| workspace        | v0.80.0                                  |
| crates (workspace)| 10                                              |
| crates (admitted)| 9 / 40-42                                   |
| crates (WIP)     | 1 — nika-schema                                  |
| L0               | 5                                              |
| L0.5             | 2                                              |
| L1               | 0                                              |
| L2               | 0                                              |
| L3               | 0                                              |
| L4               | 1                                              |
| lib tests        | 1031 passed, 0 failed                              |
| clippy           | 0 warnings                              |

Narrative context (manually maintained):

- L0 admitted: nika-types, nika-error, nika-catalog. WIP: nika-schema (parser scaffolding).
- L0.5 admitted: nika-kernel (with prelude hub, Q7), nika-kernel-mock.
- L4 admitted: nika-catalog-verify.
- 0 unwraps in `src/`, Gate 8 GREEN, Invariant #19 FULL.
- 32 providers, 49 capability rules, 7-axis ModelPricing, scope.providers canonical.
- Q1-Q13 L0/L0.5 architecture decisions LOCKED 2026-04-16
  (`docs/architecture/l0-l05-architecture-decisions.md`).
- 8 new ADRs (021-028 + ADR-006 amendment) lock Foundation v0.81 constellation.
- 5 stub ADRs (029/030/031/032/035) mark Wave 4A/4B reservations — prose lands Phase C.
- **Active arc: Phase B (hygiene vectors 22-33, P0 ratchets) → Phase C (ADR
  prose) → Phase D (14-crate envelope refactor with rename, facade drop,
  schema split).** See `~/.claude/.../memory/MEGA_HANDOFF_FOUNDATION_LOCK_V081.md`.

🦋 Nika — workflow engine for AI, AGPL, SuperNovae Studio.
