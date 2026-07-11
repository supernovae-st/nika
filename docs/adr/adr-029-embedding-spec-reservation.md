---
id: ADR-029
title: "EmbeddingSpec value-type reservation (Cortex seed)"
status: accepted
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
timeline: "v0.81.0-alpha.4, Wave 4A R1 seed · prose authored + accepted 2026-07-11"
follow_ups:
  - "Re-read the field set when the first embedding provider prototypes (the #[non_exhaustive] reservation absorbs additive evolution — ADR-028)"
---

# ADR-029: EmbeddingSpec value-type reservation (Cortex seed)

> Accepted 2026-07-11 (operator-delegated ADR ruling): the decision has
> been load-bearing in code since v0.81 and the reservation held its
> shape for three months — a `proposed` status no longer described
> reality. This prose replaces the Phase-C stub.

## Context

The memory/Connectome era needs embedding storage whose index layout and
distance kernel are decided by three axes — **dim**, **dtype**, **metric** —
plus the model identity that produced the vector. The embedding landscape
moves monthly (Matryoshka truncation; 256→3072 dims; f32/f16/bf16/i8/binary
dtypes; cosine/dot/euclidean/hamming metrics), so whatever type we reserve
must be able to grow fields without a breaking-change migration.

Wave 4A R1 (commit `001ae0b6f`, 2026-04-17) added `EmbeddingSpec` to
`nika-types` (`crates/nika-types/src/embedding.rs`) as that reservation,
per the ADR-028 forward-compat reservation policy.

## Decision

`EmbeddingSpec` ships in `nika-types` (L0) as `#[non_exhaustive]` with a
`::new()` constructor, carrying the (dim, dtype, metric, model_id) tuple:

- **dim / dtype / metric** — exactly what HNSW/BM25/RRF backends need to
  pick index layout and distance kernel.
- **model_id optional** — catalog-free consumers (tests, tools) construct
  a spec without materialising a provider name.
- **`#[non_exhaustive]` + `new()`** — additive field evolution stays
  non-breaking by construction (INV-019 idiom); the reservation absorbs
  the monthly landscape drift instead of chasing it.

Field shape pinned in the commit body of `001ae0b6f` and in the FCI-035
addendum of
[`docs/architecture/forward-compat-invariants.md`](../architecture/forward-compat-invariants.md).
`EmbeddingProvider` (ADR-034, L0.5) consumes the spec.

## Alternatives considered

- **No reservation — add the type when the first provider lands.** Rejected:
  the Connectome consumers (index rehydration · cross-provider portability)
  would then force a breaking L0 change mid-era; ADR-028 exists precisely
  to pay this cost early and additively.
- **A wider spec (normalization · truncation · pooling fields now).**
  Rejected: speculative fields rot; `#[non_exhaustive]` makes adding them
  when a real provider needs them a patch-level change.
- **String-typed axes.** Rejected: dim/dtype/metric are closed vocabularies
  the kernel must match on — typed enums keep the distance-kernel dispatch
  total.

## Empirical validation (why accepted now)

Three months in production shape: the type is consumed, the field set never
needed a change, and zero migrations were forced on dependents. The
reservation did exactly what ADR-028 promised.

## See also

- ADR-028 — Forward-compat reservation policy (this ADR is an instance).
- ADR-033 — L0 foundational types (lives in the same crate).
- ADR-034 — L0.5 trait expansion (`EmbeddingProvider` consumes `EmbeddingSpec`).
- FCI-035 addendum — Wave 4A/4B reservation catalog.

🦋
