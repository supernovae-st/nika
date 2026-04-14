# Nika Diamond — Claude Code rules

**Branch** : `nika-diamond` — DEFAULT working branch for the next 11-12 months.
**Main** : `830aa6154` — read-only reference via `git show main:path`. NEVER checkout, NEVER modify, NEVER push. Access legacy code ONLY via `git show`.
**Legacy binary** : `~/bin/nika-legacy` — pre-built v0.79 for parity tests (Phase 5+).
**This is NOT extraction. This is CRAFT.** Each crate rewritten from scratch, guided by legacy. User learns Rust in parallel.

## 🔐 Authority hierarchy

1. `~/.claude/projects/-Users-thibaut-dev-supernovae-nika/memory/POST_AUDIT_REVISIONS.md` — SUPREME AUTHORITY, overrides all other docs.
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
Current: Phase D (catalog scope expansion, Session 2a DONE). 5 crates admitted.

## 🚫 Interdits stricts

- ❌ Co-Authored-By: Claude (always Nika 🦋 `<nika@supernovae.studio>`)
- ❌ Copy-paste from main verbatim (rewrite propre requis, main = reference only)
- ❌ git checkout main or modify main in any way
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

## 📋 12 Gates per crate admission

Read full spec in `RUST_ENFORCEMENT.md`. Summary :

1. SPEC — `docs/crate-specs/nika-X.md` exists (purpose, layer, LOC budget, public API)
2. TDD — tests written before impl, RED then GREEN
3. IMPL — minimal, compiles, tests pass, no `# TEMP` without removal plan
4. CLIPPY 0 — `cargo clippy --workspace --all-targets -- -D warnings`
5. MUTATION ≥90% — `cargo mutants -p nika-X`
6. PROPERTY — proptest if sensitive (security, parsers, encoding)
7. BENCHMARKS — `benches/` if hot path
8. DOCS — `cargo doc --no-deps` 0 warnings, pub items documented
9. CANARY E2E — `tests/canary-X.nika.yaml` passes (or exemption)
10. PARITY LEGACY — golden test vs `git show main:...` output
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

## 🎯 Current state (2026-04-15)

- 5 crates admitted : error + catalog + catalog-verify + kernel + kernel-mock
- 386 tests, 0 clippy, 0 unwrap in src/, Gate 8 GREEN, Invariant #19 FULL
- 15 ADRs Accepted (ADR-001..015) — bidirectional Related: cross-refs landed
- 2026 SOTA toolchain green : machete clean, semver-checks live, typos live, miri+hack+deny in CI matrix (9 jobs)
- Phase D Session 2a DONE : TOML capability rules, api_dialect, proptest, inv #19
- Next : Session 2b — Modality + TokenizerFamily + ParamFlag → Gate 2

Follow `MEGA_HANDOFF_SESSION_2B.md` (memory/) for next session spec.

🦋 Nika — workflow engine for AI, AGPL, SuperNovae Studio.
