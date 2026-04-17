---
id: ADR-029
title: "EmbeddingSpec value-type reservation (Cortex seed)"
status: proposed
date: "2026-04-17"
phase: "Phase D — Wave 4A R1"
deciders: ["@ThibautMelen"]
tags: ["l0", "cortex", "embedding", "reservation", "forward-compat"]
affects_crates: ["nika-types", "nika-kernel"]
affects_layers: ["L0", "L0.5"]
supersedes: []
superseded_by: []
related: ["ADR-028", "ADR-033", "ADR-034"]
requires: ["ADR-028"]
enables: []
amends: []
fci: ["FCI-035"]
inv: []
shadow_zones: []
nika_codes: []
timeline: "v0.81.0-alpha.4, Wave 4A R1 seed (prose pending Phase C)"
follow_ups:
  - "Flesh out full rationale + alternatives during Phase C ADR prose sweep"
  - "Decide final field set once first embedding provider (v0.95) is prototyped"
---

# ADR-029: EmbeddingSpec value-type reservation (Cortex seed)

> **STUB — prose pending Phase C.** Decision is load-bearing in code; this
> file exists so vector 19 (adr-orphan-proposed) can surface it at day 31
> and force full prose authoring.

## Context

Wave 4A R1 (commit `001ae0b6f`, 2026-04-17) adds the `EmbeddingSpec` value
type to `nika-types` at `crates/nika-types/src/embedding.rs` as a
forward-compat reservation for v0.95 Cortex. The type captures the
(dim, provider, model, dtype, schema) tuple that Cortex will need for
vector index rehydration and cross-provider embedding portability.

## Decision (preliminary)

`EmbeddingSpec` ships in `nika-types` at v0.81 with `#[non_exhaustive]`
and a `::new()` constructor. Field shape pinned in the commit body of
`001ae0b6f` and in the `FCI-035` addendum of
[`docs/architecture/forward-compat-invariants.md`](../architecture/forward-compat-invariants.md).
Full rationale + alternatives to be authored during Phase C ADR sweep
before v0.90.

## See also

- ADR-028 — Forward-compat reservation policy (this ADR is an instance).
- ADR-033 — L0 foundational types (lives in the same crate).
- ADR-034 — L0.5 trait expansion (`EmbeddingProvider` consumes `EmbeddingSpec`).
- FCI-035 addendum — Wave 4A/4B reservation catalog.

🦋
