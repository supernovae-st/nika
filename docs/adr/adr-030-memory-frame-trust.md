---
id: ADR-030
title: "MemoryFrameRef.trust reservation (Shield gate seed)"
status: proposed
date: "2026-04-17"
phase: "Phase D — Wave 4A R2"
deciders: ["@ThibautMelen"]
tags: ["l0", "cortex", "shield", "trust-level", "reservation", "forward-compat"]
affects_crates: ["nika-types", "nika-kernel"]
affects_layers: ["L0", "L0.5"]
supersedes: []
superseded_by: []
related: ["ADR-028", "ADR-033", "ADR-034"]
requires: ["ADR-028", "ADR-033"]
enables: []
amends: []
fci: ["FCI-035"]
inv: []
shadow_zones: []
nika_codes: ["NIKA-380", "NIKA-381", "NIKA-389"]
timeline: "v0.81.0-alpha.4, Wave 4A R2 seed (prose pending Phase C)"
follow_ups:
  - "Flesh out full rationale + alternatives during Phase C ADR prose sweep"
  - "Pin trust-default policy (UNTRUSTED vs TRUSTED) in the prose version"
---

# ADR-030: MemoryFrameRef.trust reservation (Shield gate seed)

> **STUB — prose pending Phase C.** Decision is load-bearing in code; this
> file exists so vector 19 (adr-orphan-proposed) can surface it at day 31
> and force full prose authoring.

## Context

Wave 4A R2 (commit `41e8a1467`, 2026-04-17) adds `trust: TrustLevel` as a
reserved field on `MemoryFrameRef` (re-exported from `nika-kernel` via
`nika-types`). The field lets Shield (NIKA-380..389) gate every recall by
the trust level of the source memory frame without a v0.95+ breaking
change.

## Decision (preliminary)

`MemoryFrameRef.trust` defaults to `TrustLevel::UNTRUSTED` (safe-by-default
per ADR-033 rev.2). Field is `#[non_exhaustive]`-protected and populated by
the memory store impl when real recall arrives in v0.95 Cortex. Shield
downstream reads the field to decide spotlight / block / allow. Full
rationale (trust lattice propagation, default inversion justification,
migration path for pre-v0.95 stubs) to be authored during Phase C ADR
sweep before v0.90.

## See also

- ADR-028 — Forward-compat reservation policy (this ADR is an instance).
- ADR-033 — `TrustLevel` lattice + `UNTRUSTED` default inversion.
- ADR-034 — `MemoryStore` trait (will be sealed at verb admission).
- FCI-035 addendum — Wave 4A/4B reservation catalog.
- NIKA-380..389 — Shield capability / trust violation error codes.

🦋
