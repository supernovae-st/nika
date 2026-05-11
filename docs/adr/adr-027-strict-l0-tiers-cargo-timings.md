---
id: ADR-027
title: "Strict L0 sublayer tiers + max 3 sibling deps + cargo timings per-crate budget"
status: accepted
date: "2026-04-16"
phase: "Phase B — hygiene foundation"
deciders: ["@ThibautMelen"]
tags: ["architecture", "tiers", "cargo-timings", "compile-budget", "hygiene", "max-deps"]
affects_crates: ["all-L0"]
affects_layers: ["L0", "L0.5"]
supersedes: []
superseded_by: []
related: ["ADR-022", "ADR-024", "ADR-026"]
requires: ["ADR-022", "ADR-026"]
enables: []
amends: ["ADR-007"]
fci: []
inv: []
shadow_zones: []
nika_codes: []
timeline: "v0.81"
follow_ups: ["implement check-l0-dep-fanout.sh + check-cargo-timings.sh"]
---

# ADR-027: Strict L0 sublayer tiers + cargo timings budget

## Context

ADR-022 lays out 14 foundation crates. Without internal discipline, L0 risks
becoming a flat bag of crates with arbitrary cross-dependencies. We want the
strongest possible compile DAG.

## Decision

### Internal L0 tiers (sublayer discipline)

L0 has 4 internal tiers, strict downward-only:

| Tier | Constraint | Crates |
|---|---|---|
| **L0-tier-0** | LEAF — zero `nika-*` deps | `nika-core`, `nika-macros` |
| **L0-tier-1** | depends on tier-0 only | `nika-error`, `nika-envelope` |
| **L0-tier-2** | depends on tier-0+1, single domain | `nika-event`, `nika-binding-transform`, `nika-catalog-codegen`, `nika-schema-ast` |
| **L0-tier-3** | depends on lower L0 tiers | `nika-catalog`, `nika-binding`, `nika-schema` |

### Hard rule: max 3 sibling L0 deps per L0 crate

Hygiene vector 28 (`check-l0-dep-fanout.sh`) reads `docs/workspace.toml` and
asserts every L0 crate's `sibling_deps` array length ≤ 3.

`nika-schema` is at the limit (4 deps including `nika-catalog`); admission
spec must justify each dep.

### `cargo timings` per-crate budget (vector 29)

Each crate has `compile_budget_secs` in `docs/workspace.toml`:

```toml
[crates.nika-core]
compile_budget_secs = 10
[crates.nika-schema]
compile_budget_secs = 30
```

Hygiene vector 29 (`check-cargo-timings.sh`) runs `cargo build --timings` and
asserts no crate exceeds budget. CI ratchet — regression caught at PR time.

### `cargo deny check duplicates` strict (vector 31)

`deny.toml` configured to forbid duplicate crate versions in `Cargo.lock`.
Single-version policy across the workspace.

### `cargo doc --document-private-items` clean (vector 32)

Internal API documented with same rigor as public. No undocumented private
items.

## Consequences

- ✅ Maximum compile parallelism (independent tier-0 + tier-1 crates compile
  concurrently)
- ✅ Forward-compat: adding a tier-4 means promoting one tier-3 first —
  conscious decision
- ✅ Predictable crate placement (mechanical sort test extends to internal
  tiers)
- ✅ `cargo timings` budget catches O(n²) trait-resolution regressions early
- ✅ `cargo deny duplicates` prevents `tokio-util 0.7` + `tokio-util 0.8`
  ecosystem split
- ❌ +5 hygiene vectors (28, 29, 30, 31, 32) — implementation cost ~4h
- ❌ Initial baseline: 14 `cargo timings` measurements committed to
  `docs/api-baselines/timings/<crate>.toml`

## Vector summary (Phase B + E adds)

| # | Name | Tier | Mode | Source ADR |
|---|---|---|---|---|
| 22 | check-no-async-in-l0 | P1 | full | (planned, PRE-DIAMOND) |
| 23 | check-workflow-envelope | P0 | fast | ADR-021 |
| 24 | check-model-format | P0 | fast | ADR-021 |
| 25 | check-kind-closed-set | P0 | fast | ADR-021 |
| 26 | check-status-claims-sync | P1 | fast | (this session) |
| 27 | check-file-loc | P1 | full | ADR-023 |
| 28 | check-l0-dep-fanout | P0 | fast | ADR-027 |
| 29 | check-cargo-timings | P2 | full | ADR-027 |
| 30 | check-public-api-baseline | P1 | full | ADR-024 |
| 31 | check-cargo-deny-duplicates | P1 | full | ADR-027 |
| 32 | check-cargo-doc-private | P2 | full | ADR-027 |
| 33 | check-workspace-toml-sync | P0 | fast | ADR-026 |

Total deployed at v0.81: **21 (current) + 12 (new) = 33 vectors**.

## Reference

- ADR-022 (14-crate layout)
- ADR-026 (workspace.toml SSOT — provides metadata for vectors 28+29)
- `~/.claude/.../memory/feedback_l0_strict_sublayer_tiers.md`
