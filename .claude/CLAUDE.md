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

1. Studio-internal authority docs (POST_AUDIT_REVISIONS · PRE_LAUNCH_GATES ·
   HANDOFF · the velocity north-star) — private overlay on the operator's
   machine, wired via `.claude/settings.local.json` (gitignored) — SUPREME
   AUTHORITY when present locally, overrides all other docs.
2. `.claude/rules/*.md` (this directory) — project-specific enforcement.

If any doc contradicts another, **POST_AUDIT_REVISIONS wins**.

## 🎯 What we're doing

Nika Diamond = layered crate architecture (L0→L5 · the count is
projected, never a gate — ADR-037 horizon 50-90 · cap 100 · ruled
D-2026-07-21-N1). Building on fresh orphan branch. Each crate passes 12 gates before admission to workspace.

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
     scripts/hygiene/check-status-claims-sync.sh (vector 23). No branch
     row since 2026-08-26 (#1240): it stamped the generation branch —
     always a deleted feature branch under PR flow — never main. -->

| field            | value                                          |
|------------------|------------------------------------------------|
| HEAD             | `f5f8de3b3` (`f5f8de3b39b3b454db3de454b0079c6dee12c914`)             |
| workspace        | v0.118.7                                  |
| crates (workspace)| 73                                              |
| crates (admitted)| 64                                             |
| crates (WIP)     | 9 — nika-chart nika-fx nika-proof nika-store nika-harness nika-execution nika-service-execution nika-serve nika-session                                  |
| L0               | 22                                              |
| L0.5             | 6                                              |
| L1               | 17                                              |
| L1.5             | 4                                              |
| L2               | 5                                              |
| L3               | 4                                              |
| L4               | 15                                              |
| lib tests        | 7230 passed, 0 failed                              |
| clippy           | 0 warnings                              |

Narrative context (manually maintained):

- L0 admitted: nika-types, nika-error, nika-catalog, nika-catalog-codegen, nika-event, nika-pack, nika-cel, nika-schema (THE PARSER — AST + raw + error + keysets · admitted 2026-06-18 · Gate-5 budget 290≤300 · O(n²) when-gate DoS fixed + origin's gate-list cap, both integrated · parser-only since the nika-check descent 2026-07-21), nika-check (the static judgment: analyzer + the `nika check` ladder · split from nika-schema 2026-07-21 at the 15k wall · the nika-graph/nika-dap precedents · L0 per the mechanical sort + three L0 consumers), nika-cap (the permits: capability boundary as pure data + fits predicate + set-algebra · extracted from nika-schema · admitted 2026-07-03 · Gate-5 100% · the ONE host matcher consolidated, third copy removed), nika-tmpl (the ONE ${{ }} island lexer — quote/escape-aware byte-spans shared by checker AND runtime · check⇄run parity by construction · admitted 2026-07-08 · Gate-5 46/51 = 90.2% with the 5 survivors certified unkillable · parity pre-proven on ~337k exhaustive inputs).
- L0.5 admitted: nika-kernel (facade + range-registry hub post 4-way split 2026-06-10), nika-kernel-core, nika-kernel-ai, nika-kernel-runtime, nika-kernel-plugin, nika-kernel-mock.
- L1 admitted: nika-clock, nika-bm25, nika-screen, nika-ocr, nika-a11y, nika-input (M2.4 · Guards 1+2), nika-browser (M2.5 · Guard 5 + occlusion hit-test), nika-fs (atomic write · s4), nika-http (reqwest+rustls · 3-layer SSRF + cross-origin cred-strip · s5), nika-blob (blake3 CAS · sidecar mime · s6), nika-exec-runner (shell/process effect · s7).
- L1.5 admitted: nika-providers (s8.5 · 14/14 providers wired across 3 wire formats incl gemini s8.6 · kernel http seam), nika-infer-local (sovereign local inference sidecar · ADR-091), nika-builtin (s16 · the 23 stdlib builtins behind ONE dispatcher · the 3 tool seams · the agent's first real tool source), nika-extract (the 9 fetch extract modes · article Trafilatura cascade · feed/sitemap/metadata/microdata · admitted 2026-06-21 · Gate-5 93% · totality proptest · the `nika:fetch` extract step).
- L2 admitted: nika-verb-infer (s9 · FIRST verb crate · one-shot infer · structured-output floor · NIKA-430..433), nika-verb-exec (s10 · shell exec · kernel ShellRunDyn seam · capture one-obvious-way split · NIKA-440..442), nika-verb-invoke (s11 · builtin/MCP tool call · kernel ToolExecuteDyn seam · closed nika:/mcp: namespace validated · NIKA-450..452), nika-verb-agent (s12 · the 4th+LAST verb · multi-turn ReAct loop · 3 injected seams ProviderInferDyn+ToolExecuteDyn+ToolDefinitionProviderDyn · default-deny whitelist · NIKA-460..466).
- L4 admitted: nika-catalog-verify, nika-lsp (stdio language server · `nika lsp`), nika-mcp (in-binary MCP server · `nika mcp`), nika-cli (the operator surface · admitted 2026-06-21 · the first-15-min verb tree check/run/trace/inspect/graph/explain/spec/schema/examples/new/doctor/pack/completions + lsp/mcp · display fold render + the --no-progress/--quiet/--dry-run reduced surfaces · Gate-5 91% · Gate-6 fold property · all 12 gates per docs/crate-specs/nika-cli.md §11).
- 0 unwraps in `src/`, Gate 8 GREEN, Invariant #19 FULL.
- 32 providers, 49 capability rules, 7-axis ModelPricing, scope.providers canonical.
- Q1-Q13 L0/L0.5 architecture decisions LOCKED 2026-04-16
  (`docs/architecture/l0-l05-architecture-decisions.md`).
- 8 new ADRs (021-028 + ADR-006 amendment) lock the L0-Foundation constellation.
- 5 stub ADRs (029/030/031/032/035) mark Wave 4A/4B reservations — prose lands Phase C.
- **Active spine — the announce ladder (D-2026-06-10-N6).** Sequenced
  announce-first · a usable first-15-min vertical slice across layers, not
  strictly bottom-up. SHIPPED · L1 effects (nika-{fs,http,blob,exec-runner} ·
  s4-s7) + the 14-provider L1.5 (nika-providers · s8.5/s8.6) + **all 4 verbs**
  L2 (nika-verb-{infer,exec,invoke,agent} · s9-s12 · the 4-verb tier COMPLETE).
  Phase 2 M2 computer-use (L1) **COMPLETE** · 5 crates admitted
  (nika-{screen,ocr,a11y,input,browser} · ADR-081 7-guard contract ·
  NIKA-1000..1599 · detail in `docs/crate-specs/` + adr-081). The first-15-min
  slice is now CLOSED · nika-cli (operator surface) + nika-builtin (s16 · the
  23 stdlib tools) + nika-infer-local (candle · ADR-091) + nika-extract (the 9
  fetch extract modes) are ALL admitted (2026-06-21) — **the wip array WAS
  empty on that date** (39 admitted then · every crate that existed).
  ⚠️ It said « the wip array **is** empty », in the present tense, thirty-five
  lines under an auto-block reading `crates (WIP) | 6`. One file, two answers
  to the same question. The narrative was TRUE on 2026-06-21 and became a lie
  by outliving its tense — which is why every declaration here carries its
  date, and why the live figure is only ever read from the block above.
- **Last stabilization — 2026-06-16** (origin/main `0b558f7f8`) · the static-check
  layer hardened to runtime-parity. **DEEP_GAPS conformance ledger EMPTIED** ·
  jq compile-check (jaq) + schema meta-check (jsonschema), both in L0 calling the
  SAME crate the runtime uses (zero check↔runtime drift). Conformance harness
  verdicts the full `check()` surface (tier-aware). CEL eval errors now carry the
  spec wire code, never the internal NIKA-17xx (spec 05 §142). one-obvious-way/009
  stream-binding lint. CF-1 for_each positional-null verified spec-correct (not a
  bug · test + spec clarification). Full battery green · 2687 lib + e2e + 503-wf
  corpus check (0 panic / 0 internal-code leak) + hygiene 0-RED.
- **Next** · design-partner `1.0.0-rc.N` → the **1.0.0**
  launch (amended D-2026-06-20-N1). The latest tagged release is whatever the
  CHANGELOG top names (never quote a number here · it rots); `main` carries
  the next dev version so contributor binaries cannot masquerade as the
  Homebrew asset. The crate count follows the ADR-037 horizon (50-90 · cap 100 ·
  projected, never a gate · ruled D-2026-07-21-N1).

🦋 Nika — workflow engine for AI, AGPL, SuperNovae Studio.
