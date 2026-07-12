---
id: ADR-032
title: "WasmPluginError OutOfFuel + Trap + PluginCallContext reservation"
status: accepted
date: "2026-04-17"
phase: "Phase D — Wave 4A R4"
deciders: ["@ThibautMelen"]
tags: ["l0.5", "wasm", "plugin", "fuel", "trap", "trust-propagation", "reservation", "forward-compat"]
affects_crates: ["nika-kernel", "nika-kernel-mock"]
affects_layers: ["L0.5"]
supersedes: []
superseded_by: []
related: ["ADR-020", "ADR-028", "ADR-030", "ADR-033"]
requires: ["ADR-020", "ADR-028"]
enables: []
amends: ["ADR-020"]
fci: ["FCI-035"]
inv: []
shadow_zones: []
nika_codes: ["NIKA-700", "NIKA-701", "NIKA-702", "NIKA-703", "NIKA-704"]
timeline: "v0.81.0-alpha.4, Wave 4A R4 seed · prose authored + accepted 2026-07-12 (merged-code rule)"
follow_ups:
  - "Wire real wasmtime fuel metering + trap mapping when the wasm host lands (v0.100 horizon) — the shapes are reserved, the engine is not"
---

# ADR-032: WasmPluginError OutOfFuel + Trap + PluginCallContext reservation

Status flipped `proposed → accepted` 2026-07-12 under the merged-code
rule (the #474 ADR ruling): all three reservations ship in
`crates/nika-kernel-plugin/src/wasm.rs` (the kernel's plugin member —
the ADR predates the kernel descent; `nika-kernel` re-exports). The
stub's two open follow-ups were RESOLVED in the shipped code; the
prose records those resolutions.

## Context

Wave 4A R4 (commit `368820e42`, 2026-04-17) extended the WASM plugin
stubs shipped under ADR-020 with three additive reservations, so the
v0.100 wasmtime integration lands without breaking every v0.8x plugin
consumer: a fuel-cancellation error, a structured trap surface, and a
per-call context that carries trust + budgets.

## Decision

All three shapes are `#[non_exhaustive]`, shipped in
`nika-kernel-plugin/src/wasm.rs`:

1. **`WasmPluginError::OutOfFuel { consumed, budget }`** — fuel-based
   cancellation beside the wall-clock `Timeout`. The seed sketched a
   single `fuel_consumed`; the shipped shape carries BOTH sides of the
   ledger — an error that says how much was spent but not what was
   allowed teaches nothing.
2. **`WasmPluginError::Trap { kind: TrapKind }`** — guest-level aborts
   (unreachable · divide-by-zero · stack overflow · …) distinct from
   host errors. **Follow-up resolved**: `TrapKind` is
   `#[non_exhaustive]`, mirroring wasmtime's `TrapCode` taxonomy at the
   nika layer — engine-specific variants land without a major AND
   without a direct wasmtime dep in L0.5.
3. **`PluginCallContext { trust, input_trust, cancel, fuel_budget,
   wall_timeout_ms }`** — the per-call DTO replacing the bare-cancel
   argument. **Follow-up resolved**: the seed's `caller_trust` /
   `plugin_trust` pair became `input_trust` (propagated from the verb
   that constructed the call) and `trust` (granted to the plugin's
   OUTPUT — lower than `input_trust` by host policy: plugin output is
   always untrusted data regardless of what it was given, the ADR-030
   sticky-taint model applied at the plugin boundary). Both budget
   knobs ride the context (`None` = unmetered), so the dual-timeout
   policy (fuel + wall) is per-call, not global.

## Alternatives considered

- **Single timeout (wall-clock only)** — rejected: wall time cannot
  distinguish a slow host from a hot guest loop; fuel is deterministic
  per-op and survives host load. Both ship.
- **Closed `TrapKind` enum** — rejected (the stub's open question):
  wasmtime/wasmer taxonomies evolve; a closed enum turns every new trap
  class into a major.
- **Trust pair as caller/plugin** (the seed's sketch) — rejected at
  hardening: what gates downstream consumption is the trust of the
  OUTPUT, not the identity of the caller; input/output naming states
  the dataflow directly.

## Empirical validation (what makes this "merged code")

- `nika-kernel-plugin/src/wasm.rs` — `OutOfFuel` (line ~146), `Trap` +
  `TrapKind` (lines ~162-192, with `Display`), `PluginCallContext`
  (line ~217) with the untrusted-default constructor.
- The reservations are exercised by the plugin trait's mock host in
  `nika-kernel-mock` (the ADR-020 test seam).

## See also

- ADR-020 — WASM plugin boundary + Sandbox (amended by this ADR).
- ADR-028 — Forward-compat reservation policy (this ADR is an instance).
- ADR-030 — sticky-taint trust model (applied here at the plugin edge).
- ADR-033 — `TrustLevel` lattice used by `PluginCallContext`.
- FCI-035 addendum — Wave 4A/4B reservation catalog.
- NIKA-700..749 — WASM plugin error code range (reserved v0.80).

🦋
