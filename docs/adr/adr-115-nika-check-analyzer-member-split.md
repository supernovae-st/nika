---
id: ADR-115
title: "nika-check size-cap member split: the analysis substrate descends to nika-check-analyzer"
status: accepted
date: "2026-08-18"
phase: "pre-1.0 · post-P2-submission hardening"
deciders: ["@ThibautMelen"]
tags: ["architecture", "crates", "split", "size-cap", "static-judgment"]
affects_crates: ["nika-check", "nika-check-analyzer"]
affects_layers: ["L0"]
supersedes: []
superseded_by: []
related: ["ADR-110", "ADR-022", "ADR-023", "ADR-027"]
requires: []
enables: []
amends: []
fci: []
inv: []
shadow_zones: []
nika_codes: []
timeline: "v0.108"
follow_ups: []
---

# ADR-115: nika-check size-cap member split — nika-check-analyzer

## Context

Vector 24 measured `nika-check` at **14,995 prod LOC against the 15,000
Diamond invariant** — five lines from a push that blocks. `nika-runtime`
sat at 14,929 (seventy-one). Neither was visible: the hygiene dashboard
renders one line per vector, and that line said only `crate(s) in
[12000, 15000) LOC range` — the same sentence at 12,001 as at 14,999. A
3,000-wide band reported as a single bit (fixed separately · the vector
now names the tightest crate and its headroom).

`nika-check` is itself the product of this exact move: it descended from
`nika-schema` at the same wall on 2026-07-21 (parser there, judgment
here). The cap is a locked maintainability budget (nika-invariants.md),
not advisory.

## Decision

Per **D-2026-07-09-N1** (a size-cap split is ONE architectural unit in
TWO workspace members), the **analysis substrate** descends to a new L0
member crate `nika-check-analyzer`:

- `analyze` + `AnalyzedWorkflow` (the Core conformance pass — envelope ·
  duplicate ids · `NIKA-DAG-002` edge-target resolution · `NIKA-DAG-001`
  cycle detection over `G_p` · the `tasks.*` reference boundary ·
  namespace resolution · `when:` shape · output binding rules · the full
  `TypeExpr` on io `type:`)
- `edges` (the derived DAG edges every surface projects) + `dag`
  (topological waves)
- `scan` · `builtin_shape` · `jq_lint` (the jaq compile-check) ·
  `schema_lint` (the jsonschema meta-check) · `schema_paths` ·
  `types_contract`

`nika-check` re-exports the member at its **historical path** —
`pub use nika_check_analyzer as analyzer;` — so every call site inside
and outside the crate keeps writing `nika_check::analyzer::…` and
`crate::analyzer::…` unchanged. The boundary moved; the surface did not.

### Why this boundary, measured

The seam was not chosen by theme, it was measured on the module graph:

| direction | edges |
|---|---:|
| the rest of `nika-check` → `analyzer` | **33** |
| `analyzer` → the rest of `nika-check` | **0** |

`analyzer` is the substrate everything reads and it reads nothing back —
a pure leaf downward, which is exactly what can descend without breaking
a cycle. The only back-references from inside the subtree to the ladder
(`crate::analyze`) live in `#[cfg(test)]` blocks and became internal
`crate::` paths on the move.

## Alternatives rejected

- **`LOC-EXEMPT` markers** · the whitelist (codegen · lookup-table ·
  enum-mega) does not cover organic judgment growth; exempting erodes
  the ratchet.
- **Descending the authority plane instead** (`permit_taint` ·
  `permits_fit` · `permits_infer` · `certificate` · `secrets` ·
  `consent` · `declass` · `data_journey`) · measured as a leaf on the
  inbound side (nothing but `lib.rs` references it) but it *depends on*
  `analyzer`/`analysis`/`reach`/`hints` — nine edges from three files.
  Extracting it while the ladder stays in `nika-check` makes
  `nika-check` → authority → `nika-check` a **cycle**. Cargo forbids it.
  Descending the substrate first is precisely what removes that cycle.
- **A logic split inside one crate** · moves no prod LOC out of the
  crate; churns module bodies for no budget gain.

## Consequences

- `nika-check` **14,995 → 12,455 prod LOC** · headroom **5 → 2,545**.
  The crate is off the wall.
- `nika-check-analyzer` ≈ 2,540 prod (5,688 raw across 9 files — the
  difference is `#[cfg(test)]` mass the counter correctly subtracts).
- **Still YELLOW** (>12,000): this buys a planning window, it does not
  end the descent. The next unit to leave is the authority plane, and
  ADR-115 is what makes it extractable — with the substrate in its own
  member, `nika-check-authority` would depend on `nika-check-analyzer`,
  not on its own parent.
- New `crates/nika-check-analyzer/public-api.txt` baseline joins the
  diff gate; `crates/nika-check/public-api.txt` re-baselines (the
  `analyzer` items become re-exports).
- `nika-check-analyzer` is a member of the `nika-check` unit, not a new
  architectural unit: same L0 row, `publish = false`, one public surface
  re-exported by the judgment crate. The ADR-037 count horizon is
  unaffected (a size-cap split adds a member, never a unit).

## Security boundary

**None moved.** The authority model stays whole in `nika-check` —
permits fit, capability escape, secret-leak IFC, consent, declassify,
the trifecta and the `RunCertificate` are untouched, and the substrate
carries no verdict of policy. The one authority-adjacent item that
travels is `jq_lint`'s read of `nika_cap::is_withheld_jq_native` — the
withheld-native list (`env`, refused since 2026-08-15) — which is a
*compile-check* over the jq program, not a permits decision.

## Rollback

Reverse `git mv` + drop the member from workspace `members`, the
`layers.` entry and the `nika-check` dep. The re-export shim keeps the
surface stable in both directions, so no call site changes either way.
