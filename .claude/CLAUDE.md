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

Nika Diamond = 42 crates architecture (cap 100). Building on fresh
orphan branch. Each crate passes 12 gates before admission to workspace.
Count finalized by POST_AUDIT_REVISIONS 2026-04-14 — includes pck + natives.

Timeline honnête : 11-12 mois total. No deadline pressure — quality > speed.
Current: Phase 2 M2 (L1 computer-use effect crates) in progress — the M2
trio **nika-screen + nika-ocr + nika-a11y ADMITTED** (ADR-003 12 gates ·
ADR-081 guard contract · error one-voice via `NikaErrorCode` +
`nika_error::codes` NIKA-1000..1206). Kernel 4-way split EXECUTED
2026-06-10 (nika-kernel-{core,ai,runtime,plugin} + facade hub · see
`docs/architecture/kernel-split-census-2026-06-10.md`). M2.4 `nika-input`
ADMITTED 2026-06-10 (ADR-003 12 gates · ADR-081 Guards 1+2 password-redaction
+ ConsentProof-TTL · 3-lens review folded in · InputDeviceDyn Send-variant
canon now uniform across ALL L1 effect crates) — the M2 computer-use
see→read→locate→ACT loop is closed. Next: M2.5 `nika-browser` (Guard 5) or
M3 per roadmap. Announce-ladder slice runs in parallel
(D-2026-06-10-N6) · s4 nika-fs + s5 nika-http + s6 nika-blob + s7 nika-exec-runner ADMITTED 2026-06-10 · s8.5 nika-providers ADMITTED 2026-06-11 (L1.5 · 14/14 wired · gemini s8.6 done) · s8 nika-policy design LOCKED (impl gated on kernel-migration) · next s9 verb-infer. NO live counts in this paragraph — crate ·
test · provider · capability-rule numbers live ONLY in the auto-generated
block below (`scripts/refresh-status.sh` · vector 23 parity-enforced ·
hand-typed counts here are the drift class that said 14 crates while the
block said 18).

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
- L0.5 crates : traits only (nika-kernel facade hub + core/ai/runtime/plugin siblings, nika-kernel-mock)
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
| HEAD             | `4ab88adeb` (`4ab88adeb221dd55b0a1a9490ea0c38051d91604`)             |
| workspace        | v0.80.0                                  |
| crates (workspace)| 33                                              |
| crates (admitted)| 29 / 42                                   |
| crates (WIP)     | 4 — nika-schema nika-infer-local nika-cli nika-builtin                                  |
| L0               | 7                                              |
| L0.5             | 6                                              |
| L1               | 11                                              |
| L1.5             | 3                                              |
| L2               | 4                                              |
| L3               | 0                                              |
| L4               | 2                                              |
| lib tests        | (skipped — pass --no-quick to compute)                              |
| clippy           | (skipped)                              |

Narrative context (manually maintained):

- L0 admitted: nika-types, nika-error, nika-catalog, nika-catalog-codegen, nika-event, nika-pack. WIP: nika-schema (parser scaffolding).
- L0.5 admitted: nika-kernel (facade + range-registry hub post 4-way split 2026-06-10), nika-kernel-core, nika-kernel-ai, nika-kernel-runtime, nika-kernel-plugin, nika-kernel-mock.
- L1 admitted: nika-clock, nika-bm25, nika-screen, nika-ocr, nika-a11y, nika-input (M2.4 · Guards 1+2), nika-browser (M2.5 · Guard 5 + occlusion hit-test), nika-fs (atomic write · s4), nika-http (reqwest+rustls · 3-layer SSRF + cross-origin cred-strip · s5), nika-blob (blake3 CAS · sidecar mime · s6), nika-exec-runner (shell/process effect · s7).
- L1.5 admitted: nika-providers (s8.5 · 14/14 providers wired across 3 wire formats incl gemini s8.6 · kernel http seam). WIP: nika-builtin (s16 · the 22 stdlib builtins behind ONE dispatcher · the 3 tool seams ToolExecuteDyn+ToolBatchDyn+ToolDefinitionProviderDyn · the agent's first real tool source).
- L2 admitted: nika-verb-infer (s9 · FIRST verb crate · one-shot infer · structured-output floor · NIKA-430..433), nika-verb-exec (s10 · shell exec · kernel ShellRunDyn seam · capture one-obvious-way split · NIKA-440..442), nika-verb-invoke (s11 · builtin/MCP tool call · kernel ToolExecuteDyn seam · closed nika:/mcp: namespace validated · NIKA-450..452), nika-verb-agent (s12 · the 4th+LAST verb · multi-turn ReAct loop · 3 injected seams ProviderInferDyn+ToolExecuteDyn+ToolDefinitionProviderDyn · default-deny whitelist · NIKA-460..466).
- L4 admitted: nika-catalog-verify. WIP: nika-cli (operator-surface seed 2026-06-11 · display fold + trace replay|show + the e2e L3-rehearsal suite · S6 build grows the full first-15-min verb tree per D-2026-06-10-N6).
- 0 unwraps in `src/`, Gate 8 GREEN, Invariant #19 FULL.
- 32 providers, 49 capability rules, 7-axis ModelPricing, scope.providers canonical.
- Q1-Q13 L0/L0.5 architecture decisions LOCKED 2026-04-16
  (`docs/architecture/l0-l05-architecture-decisions.md`).
- 8 new ADRs (021-028 + ADR-006 amendment) lock Foundation v0.81 constellation.
- 5 stub ADRs (029/030/031/032/035) mark Wave 4A/4B reservations — prose lands Phase C.
- **Active arc: Phase 2 M2 (L1 computer-use effect crates).** M1 (kernel L0.5
  sealed traits · 6 effect domains io::{screen,ocr,a11y,input,browser} +
  ai::vision · ADR-081 7 guards · NIKA-1000..1599 reserved) COMPLETE. M2.1
  nika-screen ADMITTED (capture · guards 6+7 · `xcap` · NIKA-1000..1009).
  M2.2 nika-ocr ADMITTED 2026-05-25 (text extraction · pure-Rust `ocrs` 0.12
  + `rten` 0.24 · `spawn_blocking` · NIKA-1101..1109 · mutation 93.1 % +
  Rule-2 model-inference exemption · `with_models` sovereign local-path load).
  M2.3 nika-a11y ADMITTED 2026-05-25 (accessibility-tree query · macOS `AXUIElement`
  via safe `accessibility` 0.2 · `spawn_blocking` walk · NIKA-1201..1206 ·
  MANDATORY Guard 3 AX-secure-field redaction (pure tree-transform) · mutation
  82.9 % + Rule-2 walk exemption · `MAX_WALK_DEPTH` untrusted-input cap).
  M2.4 nika-input ADMITTED 2026-06-10 (synthetic input write-side · `enigo` 0.6
  cross-platform · NIKA-1301..1305 · ADR-081 Guards 1+2 MANDATORY — type-state
  consent + monotonic fail-closed TTL + TypedText un-formattable wrapper ·
  mutation 98.8 % + Rule-2 press_chord-executor exemption · 3-lens
  adversarially-verified review, 9 findings folded same-session). See
  `docs/adr/adr-081-l1-effect-crate-guard-contract.md` + `docs/crate-specs/nika-input.md`.

🦋 Nika — workflow engine for AI, AGPL, SuperNovae Studio.
