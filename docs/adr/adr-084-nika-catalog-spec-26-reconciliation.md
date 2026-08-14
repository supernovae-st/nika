---
id: ADR-084
title: "nika-catalog builtin set · reconciliation to spec 26 (42→26 stdlib collapse + json_merge cut)"
status: accepted
date: 2026-05-27
phase: "Phase 2 M2"
deciders: ["@ThibautMelen"]
tags: ["nika-catalog", "nika-schema", "builtins", "stdlib", "spec-canon", "trust", "refactor"]
affects_crates: ["nika-catalog", "nika-schema"]
affects_layers: ["L0"]
supersedes: []
superseded_by: []
related: ["ADR-003", "ADR-082", "ADR-085", "ADR-086", "ADR-087", "ADR-088", "ADR-089"]
requires: []
enables: []
amends: []
fci: ["FCI-001"]
inv: ["INV-019", "INV-025"]
shadow_zones: []
---

# ADR-084 · nika-catalog reconciliation to spec 26

## Context

`nika/spec/stdlib/builtins-v0.1.md` (the canonical workflow language stdlib
per ADR-082 single-version-marker envelope `nika: v1`) ships **26 builtins
across 5 categories** as of D-2026-05-22-N6 stdlib-collapse + 2026-05-27
`nika:json_merge` cut. The engine's `nika-catalog::ALL_BUILTINS` carried
**63 entries across 10 categories** as a fossil of pre-D-N6 era (engine
inventory grew during Sprint 2 + media planning before the spec
minimalism doctrine locked).

A monorepo-side §2.6 Signal 7 verify-the-auditor catch (2026-05-27 ·
internal design review · private DX surface ·
§10 BLOCKER) surfaced the drift empirically · engine first-5 names were
`aggregate · assert · chart · chunk · compare` vs spec first-5
`sleep · log · emit · assert · prompt`. The drift gates the future
schemars-adoption G1 codegen (builtin-enum closed set in
`workflow.schema.json` for LSP-perfect completion · proposed-blocked
until catalog reconciliation lands).

This ADR locks the catalog reconciliation as the structural close.

## Decision

`nika-catalog::ALL_BUILTINS` and `nika-catalog::types::builtin::BuiltinCategory`
reconcile to spec source-of-truth `nika/spec/stdlib/builtins-v0.1.md` ·

- **`ALL_BUILTINS`** · 63 → 26 entries (46 legacy removed · 9 spec-only
  added · 17 keeps recategorised to the simplified 5-category enum).
- **`BuiltinCategory`** · 10 → 5 variants ·
  - KEEP · `Core` · `File` · `Data` · `Introspection`
  - ADD · `Network` (for `fetch` + `notify` · spec category)
  - REMOVE · `DataSprint2` · `CostRecords` · `Agent` · `MediaAlwaysOn` ·
    `MediaCore` · `MediaOptIn` (6 variants · the 46 legacy entries that
    used them are all cut · zero workspace consumer matches on these
    variants per `grep -rn "BuiltinCategory::<v>" crates/`).
- **`nika-schema::trust.rs`** · 67 hardcoded `"nika:<name>"` strings
  reduced to 26 across 3 trust categories (PROPAGATING 7 · PURE 14 ·
  EXTERNAL 5 · the legacy REFERENCE category consolidated into
  PROPAGATING since they were semantically equivalent per the existing
  trust.rs comment · REFERENCE re-instates when media DEFERRED builtins
  land per LOCK-031).
- **Doc-comments** · `63 tools across 10 categories` → `26 tools across
  5 categories` (line 6 of `types/builtin.rs`).

## Per-spec-category mapping (the canonical 26)

| Category | N | Builtins |
|---|---|---|
| Core | 7 | `sleep` · `log` · `emit` · `assert` · `prompt` · `done` · `wait_until` |
| File | 5 | `read` · `write` · `edit` · `glob` · `grep` |
| Data | 8 | `jq` · `json_diff` · `validate` · `json_merge_patch` · `csv_to_json` · `uuid` · `date` · `hash` |
| Network | 2 | `fetch` · `notify` |
| Introspection | 4 | `cost` · `records` · `dag_info` · `threads` |

Total · 26. Alphabetically sorted in `ALL_BUILTINS` (binary-search
invariant preserved · unit-test `sorted_order` GREEN).

## Trust-category mapping (per `nika-schema::trust.rs`)

| Trust class | N | Builtins | Output trust |
|---|---|---|---|
| PROPAGATING | 7 | `csv_to_json` · `glob` · `grep` · `jq` · `json_diff` · `json_merge_patch` · `read` | input |
| PURE | 14 | `assert` · `cost` · `dag_info` · `date` · `done` · `emit` · `hash` · `log` · `records` · `sleep` · `threads` · `uuid` · `validate` · `wait_until` | Trusted |
| EXTERNAL | 5 | `edit` · `fetch` · `notify` · `prompt` · `write` | Untrusted |

Total · 26. Conservative classification · file `edit` is EXTERNAL (writes
to disk · observable side-effect) · file `read`/`glob`/`grep` are
PROPAGATING (input-trust passes through · no observable side-effect).
`prompt` is EXTERNAL because human/API input is by definition Untrusted
floor.

## Consequences

### Positive

- **Spec ↔ engine parity** · `nika-catalog` now mirrors `nika/spec/stdlib`
  exactly · the catch-class « engine carries phantoms vs spec » closes
  structurally · future schemars-adoption G1 codegen (ADR-085 reserved)
  unblocks · `workflow.schema.json` `builtin_name` can ratchet from
  permissive pattern to closed enum derived from `ALL_BUILTINS`.
- **Trust surface lean** · 67 → 26 hardcoded refs in `trust.rs` · -61%
  surface area · easier to audit + verify the trust-floor invariant.
- **SemVer-clean enum simplification** · `BuiltinCategory` removes 6
  variants but `#[non_exhaustive]` is on the enum AND zero workspace
  consumers match on the removed variants · the breaking change is
  theoretical (`cargo public-api` will flag but no actual call-site
  breaks).
- **Forward-compat** · `Network` variant added per `#[non_exhaustive]`
  ratchet · safely consumable when downstream wants per-category trust
  defaults. REFERENCE class consolidation into PROPAGATING is reversible
  if media builtins re-emerge (current LOCK-031 DEFERRED).

### Negative

- **Public API surface change** · `BuiltinCategory` enum membership
  reshuffles · `cargo public-api` diff will show removed variants.
  Acceptable per ADR-002 (amended D-2026-06-20-N1 · real semver toward 1.0) ·
  the workspace is pre-1.0 · breaking MINOR ratchets are allowed pre-launch.
- **Trust.rs REFERENCE category gone** · the documentation-only split
  between PROPAGATING and REFERENCE (both passed input trust) is now
  collapsed · reintroduces when media DEFERRED builtins land. Cost ·
  ~0 (semantically equivalent today · just a code-organization signal).

### Neutral

- **Tests · +2 net new** · `legacy_builtins_unknown_post_d_n6` (trust)
  + `category_totals_match_spec_26` (trust) · plus `builtin_count`
  + `category_counts` + `is_known_builtin_works` updated for the new
  totals. 1170 baseline → 1172 tests GREEN.

## Alternatives considered

### A · Keep 63 catalog · LSP codegen filters to spec 26 at build time

REJECTED · the source-of-truth divergence stays · spec ↔ engine parity
is never structural · drift recurs at each spec update · two truth
sources is the anti-pattern this ADR closes.

### B · Defer to L2 `nika-builtin` admission (the MEMORY-pointer framing)

REJECTED post user-explicit override (« ship reconciliation now »
AskUserQuestion 2026-05-27). The L2-future framing remained valid as
long as no consumer needed the spec-26 set today · the schemars-adoption
G1 plan + the canon coherence work surfaced an immediate consumer signal
that overrode the LOCK-031 spirit · the user redirected accordingly.
Documented per `dx/.claude/rules/socratic-research-discipline.md` §5c
(Option D · don't pick least bad).

### C · Mass-rename engine to keep 63 set + mark legacy as deprecated

REJECTED · violates `dx/.claude/rules/no-legacy-no-back-compat.md`
NUKE-LEGACY doctrine + Class 1 single-canonical-set discipline ·
deprecated-alias drift accumulates · git history is the archive.

## Gate verification

This is a refactor of admitted-crate data + types (not a new crate
admission) · the ADR-003 12-gate ceremony reduces to applicable gates ·

- **Gate 2 TDD** · ✓ existing tests updated to spec 26 · 2 new tests
  added · 1172 lib tests GREEN.
- **Gate 3 IMPL** · ✓ `cargo check --workspace` GREEN.
- **Gate 4 CLIPPY** · ✓ `cargo clippy --workspace --all-targets -- -D
  warnings` 0 warnings (post 8 doc_markdown backtick fixes).
- **Gate 8 DOCS** · ✓ `cargo doc --no-deps -p nika-catalog -p nika-schema`
  0 warnings.
- **Gate 12 ATOMIC** · ✓ single commit · co-authored Nika 🦋.
- `cargo fmt --check` ✓ GREEN.
- `cargo deny check` ✓ advisories/bans/licenses/sources ok.
- Gates 1 / 5 / 6 / 7 / 9 / 10 / 11 · N/A for data refactor (no new
  crate · no new logic surface beyond data shape).

## Companion artefacts

- `nika-catalog/src/data/builtins.rs` · 63 → 26 entries · 5-category
  reorganization · tests updated.
- `nika-catalog/src/types/builtin.rs` · `BuiltinCategory` 10 → 5
  variants · doc-comment refreshed.
- `nika-catalog/src/lib.rs` · `all_builtins_non_empty` test 63 → 26.
- `nika-schema/src/trust.rs` · 67 → 26 hardcoded refs · 4 → 3 trust
  category lists · 2 new tests.
- Internal design review · private DX surface
  · §10 BLOCKER inventory + reconciliation scope (the design + audit
  trail · monorepo-side).
- `nika/spec/stdlib/builtins-v0.1.md` · the canonical source-of-truth
  (unchanged · this ADR mirrors it into the engine).

## References

- `nika/spec/stdlib/builtins-v0.1.md` · spec canonical 26 + cuts
- ADR-082 · envelope `nika: v1` single-version-marker
- D-2026-05-22-N6 · stdlib-collapse 42 → 27 (monorepo decision)
- 2026-05-27 `nika:json_merge` cut · `jaq` source-verified
- `dx/.claude/rules/no-legacy-no-back-compat.md` · NUKE-LEGACY discipline
- `dx/.claude/rules/cross-source-validation.md` §2.6 Signal 7 verify-the-auditor
- `dx/.claude/rules/round-1-ship-pattern.md` §6 LOCK-031 trigger-gated

🦋
