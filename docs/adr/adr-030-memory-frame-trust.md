---
id: ADR-030
title: "MemoryFrameRef.trust reservation (Shield gate seed)"
status: accepted
date: "2026-04-17"
phase: "Phase D — Wave 4A R2"
deciders: ["@ThibautMelen"]
tags: ["l0", "cortex", "shield", "trust-level", "reservation", "forward-compat"]
affects_crates: ["nika-types", "nika-kernel"]
affects_layers: ["L0", "L0.5"]
supersedes: []
superseded_by: []
related: ["ADR-028", "ADR-031", "ADR-032", "ADR-033", "ADR-034", "ADR-080"]
requires: ["ADR-028", "ADR-033"]
enables: []
amends: []
fci: ["FCI-035"]
inv: []
shadow_zones: []
nika_codes: ["NIKA-380", "NIKA-381", "NIKA-389"]
timeline: "v0.81.0-alpha.4, Wave 4A R2 seed · prose authored + accepted 2026-07-12 (merged-code rule)"
follow_ups:
  - "Populate `trust` from real ingest provenance when the Connectome recall path ships (the field is wired, the writer is not)"
---

# ADR-030: MemoryFrameRef.trust reservation (Shield gate seed)

Status flipped `proposed → accepted` 2026-07-12 under the merged-code
rule (the #474 ADR ruling): the decision is load-bearing in shipped
code — `MemoryFrameRef.trust` exists, defaults safely, and rides the
serde wire today. An ADR whose decision the workspace already executes
does not stay "proposed"; the prose below documents what IS.

## Context

Wave 4A R2 (commit `41e8a1467`, 2026-04-17) added `trust: TrustLevel`
as a reserved field on `MemoryFrameRef` so that Shield (the
NIKA-380..389 code family) can gate every recall by the trust level of
the source memory frame **without a breaking change** when real recall
arrives. Per ADR-028 (forward-compat reservation policy), the field
ships years before its writer: reserving the slot is cheap; retrofitting
it onto a `#[non_exhaustive]` struct consumed across layers is not.

The trust model this seeds is **sticky taint**: trust is assigned at
ingest and never upgrades on recall — a frame that entered the store
untrusted can never launder itself into a trusted context by being
remembered.

## Decision

- `MemoryFrameRef` carries `pub trust: crate::trust::TrustLevel`
  (`nika-types/src/memory.rs` — re-exported through `nika-kernel`).
- **Wire default**: when the field is absent on the wire, serde fills
  `TrustLevel::UNTRUSTED` (50) via `default_trust_untrusted()` — a
  legacy or unknown-origin frame is conservatively untrusted, keeping
  new-consumer/old-producer AND old-consumer/new-producer strictly
  fail-safe.
- **No language-level `Default`**: `TrustLevel` deliberately does NOT
  implement `Default` (removed in Wave 3 · P1-2 rust-security). A
  blanket `default()` returning UNTRUSTED (50) sits ABOVE SANDBOXED
  (10) in the lattice — a silent inversion of safe-by-default for any
  capability gate written as `is_at_least(SANDBOXED)`. Trust must be a
  deliberate construction at every call site; only the serde
  wire-compat path carries a default, and that one is scoped, named,
  and documented at the field.
- The lattice itself (SANDBOXED 10 < UNTRUSTED 50 < TRUSTED 150 <
  ELEVATED 200 < SYSTEM 255 · `meet`/`join`/`is_at_least`) is ADR-033's
  decision; this ADR consumes it.

## Alternatives considered

- **`Option<TrustLevel>`** — rejected: an absent trust level forces
  every reader to invent a policy at the read site; the whole point is
  ONE conservative policy at the boundary.
- **Defaulting to TRUSTED for engine-internal frames** — rejected: the
  writer (real ingest) does not exist yet, so nothing can vouch for
  provenance; safe-by-default (ADR-033 rev.2) wins until it does.
- **Deferring the field to the Connectome era** — rejected per ADR-028:
  adding a field later to a serialized, cross-layer struct is the
  exact breaking change the reservation policy exists to prevent.

## Empirical validation (what makes this "merged code")

- `nika-types/src/memory.rs` — the field, its serde default fn, and the
  constructor defaulting to UNTRUSTED, all shipped and exercised by the
  crate's serde round-trip tests.
- `nika-types/src/trust.rs` — the lattice with the no-`Default`
  invariant documented at the impl site.
- The `#[non_exhaustive]` + `new()` discipline (FCI invariant #19)
  holds on both types, so the v0.95+ writer lands without a major.

## See also

- ADR-028 — Forward-compat reservation policy (this ADR is an instance).
- ADR-033 — `TrustLevel` lattice + `UNTRUSTED` default inversion.
- ADR-034 — `MemoryStore` trait (will be sealed at verb admission).
- FCI-035 addendum — Wave 4A/4B reservation catalog.
- NIKA-380..389 — Shield capability / trust violation error codes.

🦋
