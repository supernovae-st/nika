---
id: ADR-026
title: "workspace.toml single source of truth + auto-generated crate.md"
status: accepted
date: "2026-04-16"
phase: "Phase B — hygiene foundation"
deciders: ["@ThibautMelen"]
tags: ["hygiene", "ssot", "metadata", "workspace", "auto-gen"]
affects_crates: ["all"]
affects_layers: ["L0", "L0.5", "L1", "L2", "L3", "L4", "L5"]
supersedes: []
superseded_by: []
related: ["ADR-022", "ADR-023", "ADR-027"]
requires: []
enables: ["ADR-027"]
amends: []
fci: []
inv: []
shadow_zones: []
nika_codes: []
timeline: "v0.81"
follow_ups: ["implement scripts/refresh-crate-readmes.sh"]
---

# ADR-026: workspace.toml SSOT + auto-generated crate.md

## Context

Phase A baseline reset (commit cd9602ca0) revealed that workspace metadata is
scattered across 5+ files:
- `Cargo.toml [workspace.metadata.diamond.layers]` (per-crate layer)
- `docs/architecture/forward-compat-invariants.md` (FCI references)
- `docs/architecture/crate-layer-registry.md` (layer rules)
- `scripts/refresh-status.sh` (status block generation)
- Per-crate `Cargo.toml` (LOC budget, gates, owner)
- Per-crate `README.md` (when present, often missing)

Each manual edit drifts independently. Phase A added `refresh-status.sh` as
a partial fix. Need a deeper consolidation.

## Decision

### `docs/workspace.toml` — single source of truth

One file declaring per-crate metadata machine-readable:

```toml
[crates.nika-core]
layer = "L0"
tier = 0
leaf = true
publish = false
gates_passed = ["spec", "tdd", "impl", "clippy", "mutation", "property",
                "benchmarks", "docs", "canary", "parity", "review", "atomic"]
loc_budget = 5000
file_loc_warn = 800
file_loc_fail = 1500
public_api_baseline = "docs/api-baselines/nika-core.txt"
sibling_deps = []
owners = ["diamond-team"]

[crates.nika-error]
layer = "L0"
tier = 1
leaf = false
publish = false
gates_passed = [...]
sibling_deps = ["nika-core"]
# ...
```

### Auto-generated `crates/<name>/README.md`

Hygiene vector reads `docs/workspace.toml` + `Cargo.toml` description +
`lib.rs` `//!` docs and emits `crates/<name>/README.md`:

```markdown
# nika-core

**Layer**: L0 (tier-0 leaf)  •  **Publish**: false  •  **LOC budget**: 5000
**Owners**: diamond-team
**Sibling deps**: (none — leaf crate)

## Purpose

Foundation value types — IDs, Cost, Trust, Baggage, Resource, Hash,
RetryConfig, TokenUsage, etc.

## Gates passed (12/12)

- ✅ spec ✅ TDD ✅ impl ✅ clippy ✅ mutation ✅ property
- ✅ benchmarks ✅ docs ✅ canary ✅ parity ✅ review ✅ atomic

## API baseline

`docs/api-baselines/nika-core.txt`

## Reference

- Spec: `docs/crate-specs/nika-core.md`
- Layer registry: `docs/architecture/crate-layer-registry.md`
```

### Hygiene vector 33 — workspace.toml ↔ Cargo.toml ↔ README sync

Reads `docs/workspace.toml`, asserts:
- Every workspace member has an entry
- Every `Cargo.toml` `version`/`publish` matches
- Every `README.md` matches the auto-gen template
- `[workspace.metadata.diamond.layers]` matches `workspace.toml.crates.*.layer`
- `scripts/refresh-status.sh` output matches `workspace.toml`-derived counts

## Consequences

- ✅ Drift eliminated by mechanical enforcement
- ✅ Adding/renaming a crate = ONE edit (workspace.toml), README + status block
  + Cargo.toml metadata regenerated
- ✅ ADR-027 (cargo timings) reads from same SSOT
- ❌ +1 hygiene vector (33), +1 generator script
- ❌ Initial migration: write `docs/workspace.toml` for all 7 admitted + 7
  planned crates (~2h, mechanical)

## Reference

- ADR-022 (foundation crate layout)
- ADR-023 (file modularity — values populated from workspace.toml)
- ADR-027 (cargo timings — budgets from workspace.toml)
