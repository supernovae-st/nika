---
id: ADR-090
title: "Structural doctrine enforcement — gates project the SSOT"
status: accepted
date: "2026-06-10"
phase: "Phase 2 (M2 · process)"
deciders: ["@ThibautMelen", "Olympus self-audit"]
tags: ["process", "quality", "gates", "security", "supply-chain", "antifragile"]
affects_crates: []
affects_layers: ["L0", "L0.5", "L1", "L2", "L3", "L4", "L5"]
supersedes: []
superseded_by: []
related: ["ADR-003", "ADR-005", "ADR-081", "ADR-091"]
requires: ["ADR-003"]
enables: []
amends: []
fci: []
inv: []
shadow_zones: []
nika_codes: []
timeline: "v0.80.0, Phase 2 M2"
follow_ups: ["mutants.toml exclude_re for cross-platform L1 crates (BUDGET-mode calibration)", "cargo-semver-checks install + public-API surface lock gate"]
---

# ADR-090: Structural doctrine enforcement — gates project the SSOT

## Context

ADR-003's thesis is **"enforcement must be mechanical, not social"**. Yet
several load-bearing invariants were still honor-system — recorded in prose
(an ADR, an audit table, an ownership matrix) but with no gate that fails when
the prose is violated. A self-audit (2026-06-10) found four such gaps:

1. **Gate 5 (mutation ≥90%)** — checked only that the crate-spec *mentioned*
   "Mutation". No `cargo-mutants` was actually run/floored at admission.
2. **ADR-081 computer-use guards** — "MANDATORY-at-admission" for the L1
   crates that touch screen capture / accessibility secure-fields, but nothing
   failed if a guard-owner crate shipped without its guard.
3. **Supply-chain policy** — `deny.toml` + `cargo-deny` were configured but
   unwired; only `cargo audit` (advisories) ran, not bans/licenses/sources.
4. **Error one-voice** — every error enum was migrated to the canonical
   `NikaErrorCode` trait (recorded in a completeness audit table), but a new
   error enum could skip the trait silently.

The common shape: a **doctrine with a written SSOT** (audit table · ownership
matrix · `deny.toml` policy · the 12-gate list) but **no projection of that
SSOT into a pass/fail gate**.

## Decision

**Every doctrine invariant that can be mechanically checked gets a structural
gate, and the gate PROJECTS an existing SSOT rather than hardcoding a parallel
list.** This makes the doctrine evolutive (edit the SSOT, the gate follows) and
antifragile (a regression that violates the doctrine turns a gate RED).

Four gates landed under this principle (2026-06-10):

| Gate | SSOT it projects | Invariant |
|------|------------------|-----------|
| `scripts/ci/check-mutation-floor.sh` | crate-spec Gate-5 row | real Gate 5 — `cargo-mutants` kill-ratio ≥ floor; BUDGET mode reads a `<!-- GATE5-EXEMPT: N -->` spec marker (survivors ≤ N) |
| `scripts/hygiene/check-adr-081-guards.sh` (vector 35) | the ADR-081 ownership matrix | every MANDATORY guard whose owner-crate is a workspace member has an impl-binding (`scripts/ci/adr-081-guard-manifest.tsv`) + impl/test markers |
| `scripts/hygiene/check-cargo-deny.sh` (vector 34) | `deny.toml` | full `cargo deny check` (advisories + bans + licenses + sources) |
| `scripts/hygiene/check-error-one-voice.sh` (vector 37) | `error-trait-completeness-2026-06-10.md` → `error-one-voice-allowlist.tsv` | every thiserror enum in admitted-crate src impls `NikaErrorCode` or is a documented exemption |

Each gate carries an **orphan/parity check** so its projected SSOT can't rot
(a manifest/allowlist row must map to a real matrix row / real enum). Vector 36
(`check-unused-deps.sh`, cargo-machete) shipped in the same arc.

## Consequences

### Positive
- The four doctrines are now structurally enforced — a future under-pressure
  dev can't let them rot silently (ADR-003's thesis, completed).
- Evolutive: adding a guard / error enum / banned dep updates one SSOT; the
  gate auto-extends. Zero parallel hardcoded lists.
- The ADR-081 guard gate is a **forcing-function for the next crate**: when
  `nika-input` (M2.4) is admitted, its Guards 1+2 become owed-and-present and
  the gate goes RED until they ship + test.

### Negative
- `check-mutation-floor.sh` and `cargo deny check` are minutes-slow →
  admission/CI-tier, not pre-commit. The 34-vector `check-all.sh` stays fast
  (the slow gates are invoked per-crate at admission).
- BUDGET mode is not reproducible for cross-platform L1 crates: a naive
  `cargo mutants -- --lib` mutates the cfg'd-out other-OS arms too, so the
  spec's reachable exempt-count (e.g. nika-a11y's 7) does not match the raw
  macOS survivor count (~31). Calibration needs a per-crate `mutants.toml`
  exclude_re — a deferred-with-trigger follow-up. Until then those crates carry
  no exempt marker and are not FLOOR-gated.

### Neutral
- The self-audit that found these gaps (and two bugs in the mutation gate as it
  was first shipped) is itself the antifragile pattern: gates are audited like
  any other code, and a gate that would false-RED a legit admission is a bug
  caught before it blocks.

## Evidence

- `scripts/hygiene/check-all.sh` — 34 live vectors (34-37 added this arc)
- `scripts/hygiene/README.md` — vector catalog 34-38
- `docs/adr/adr-003-12-gate-admission.md` — amendment recording the Gate-5 +
  ADR-081 closure
- `docs/architecture/error-trait-completeness-2026-06-10.md` — the error
  one-voice SSOT
- `docs/adr/adr-081-l1-effect-crate-guard-contract.md` — the guard ownership
  matrix
- `scripts/ci/adr-081-guard-manifest.tsv` — the projected guard bindings
- `scripts/ci/error-one-voice-allowlist.tsv` — the projected error-voice allowlist

## Alternatives considered

- **Leave it social (memory + reminders)** — rejected: ADR-003 already proved
  the legacy codebase rotted under social enforcement (29 broken MCP aliases
  lived for months).
- **Hardcode the lists in each gate** — rejected: a parallel list drifts from
  the SSOT. Projection (gate reads the matrix / table / policy file) keeps one
  source.
- **One mega-gate** — rejected: four orthogonal drift classes, four scripts,
  single responsibility (mirrors the existing vector design).

## Notes

This ADR records a *principle*, not a frozen list. As new mechanically-checkable
doctrines appear (e.g. `#[must_use]` discipline, public-API surface lock via
`cargo-public-api`, semver-checks), they fall under the same rule: find the
SSOT, project it into a gate, add an orphan check. Gate count is not sacred —
the projection-from-SSOT discipline is.
