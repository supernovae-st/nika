---
id: ADR-032
title: "WasmPluginError OutOfFuel + Trap + PluginCallContext reservation"
status: proposed
date: "2026-04-17"
phase: "Phase D — Wave 4A R4"
deciders: ["@ThibautMelen"]
tags: ["l0.5", "wasm", "plugin", "fuel", "trap", "trust-propagation", "reservation", "forward-compat"]
affects_crates: ["nika-kernel", "nika-kernel-mock"]
affects_layers: ["L0.5"]
supersedes: []
superseded_by: []
related: ["ADR-020", "ADR-028", "ADR-033"]
requires: ["ADR-020", "ADR-028"]
enables: []
amends: ["ADR-020"]
fci: ["FCI-035"]
inv: []
shadow_zones: []
nika_codes: ["NIKA-700", "NIKA-701", "NIKA-702", "NIKA-703", "NIKA-704"]
timeline: "v0.81.0-alpha.4, Wave 4A R4 seed (prose pending Phase C)"
follow_ups:
  - "Flesh out full rationale + alternatives during Phase C ADR prose sweep"
  - "Pin TrapKind enum closed vs. #[non_exhaustive] + discriminator strategy"
  - "Decide caller_trust vs plugin_trust lattice propagation semantics in prose"
---

# ADR-032: WasmPluginError OutOfFuel + Trap + PluginCallContext reservation

> **STUB — prose pending Phase C.** Decision is load-bearing in code; this
> file exists so vector 19 (adr-orphan-proposed) can surface it at day 31
> and force full prose authoring.

## Context

Wave 4A R4 (commit `368820e42`, 2026-04-17) extends the WASM plugin stubs
shipped under ADR-020 with three additive reservations in
`crates/nika-kernel-plugin/src/wasm.rs`:

1. `WasmPluginError::OutOfFuel { fuel_consumed }` — fuel-based cancellation
   variant, complementing the wall-clock `Timeout` variant.
2. `WasmPluginError::Trap { kind: TrapKind }` — runtime trap surface
   (out-of-memory, stack overflow, unreachable, division by zero, ...).
3. `PluginCallContext { cancel, caller_trust, plugin_trust }` — per-call
   context DTO replacing the previous bare-cancel argument; carries the
   trust lattice of both the caller and the plugin for Shield gating.

## Decision (preliminary)

All three additions are `#[non_exhaustive]`. The `TrapKind` enum is
`#[non_exhaustive]` as well so wasmtime and wasmer can contribute
engine-specific variants without breaking v0.8x consumers. `PluginCallContext`
is the replacement for future `PluginCall` callsite evolution; the existing
`cancel: CancelCtx` trait-method parameter continues to work during the
transition. Full rationale (dual-timeout policy rationale, trap taxonomy,
trust propagation model, fuel-budget default) to be authored during Phase
C ADR sweep before v0.90.

## See also

- ADR-020 — WASM plugin boundary + Sandbox. This ADR amends ADR-020's
  trait surface with three additive reservations.
- ADR-028 — Forward-compat reservation policy (this ADR is an instance).
- ADR-033 — `TrustLevel` type used by `PluginCallContext`.
- FCI-035 addendum — Wave 4A/4B reservation catalog.
- NIKA-700..749 — WASM plugin error code range (reserved v0.80).

🦋
