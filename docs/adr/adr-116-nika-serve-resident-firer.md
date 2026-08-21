---
id: ADR-116
title: "nika serve v0 was a local-only resident firer"
status: superseded
date: "2026-08-19"
phase: "pre-1.0 · ARM+Serve"
deciders: ["@ThibautMelen"]
tags: ["architecture", "cadence", "arm", "serve", "lineage"]
affects_crates: ["nika-cli", "nika-cadence"]
affects_layers: ["L0", "L4"]
supersedes: []
superseded_by: ["ADR-117"]
related: []
requires: ["ADR-114"]
enables: []
amends: []
fci: []
inv: []
shadow_zones: []
nika_codes: []
timeline: "v0.110"
follow_ups: []
---

# ADR-116: `nika serve` v0 was a local-only resident firer

## Status and provenance

This is a historical tombstone, not a retroactive claim that this ADR was
previously canonical. The original accepted-form document existed only on the
closed, unmerged PR #1006 (`c224e1bdc`). The local resident-firer code later
landed through PR #1008 without that ADR. Reintroducing the number preserves a
machine-valid decision lineage before ADR-117 supersedes it.

## Historical decision

`nika serve` v0 was the same ARM firer kept resident by a loop. It read the
project's `nika.yaml`, the referenced workflow shelf, and `.nika/arm/` state.
It did not open a port, accept a socket, or expose a remote trigger. Cadence
selection remained in `nika-cadence`; filesystem custody, firing, and rendering
remained in `nika-cli`.

The shipped surface is narrower and more precise than the withdrawn document's
prose:

- `--once` executes every due local beat once, records the result, then exits;
  it is not a rehearsal.
- `--dry` prints the `nika_cadence::due` projection and runs nothing; it does
  not call the full firing decision machine.
- a malformed reload is reported while the last valid registry remains active;
  the current implementation does not poison the served set.

These statements are characterization of the current code, not new behavior.

## Why it is superseded

An HTTP execution surface changes the input, identity, replay, and data-release
boundaries. Treating it as a small extension of this local loop would make the
CLI own transport, authentication, execution composition, job state, and ARM
custody at once. ADR-117 therefore preserves the local default but replaces the
architectural decision for any networked Serve mode.

## Evidence

- `crates/nika-cli/src/verbs/serve.rs` — resident local loop and current option
  semantics.
- `crates/nika-cli/tests/serve.rs` — real-binary once, reload, cloud-refusal,
  and signal behavior.
- `crates/nika-cadence/src/firing.rs` — pure cadence and firing judgment.
- PR #1006 — withdrawn source of this ADR number.
- PR #1008 — carrier that shipped the local resident firer without the ADR.

## Consequences retained by ADR-117

- Bare `nika serve` remains local and opens no listener.
- ARM's verified ledger remains the firing truth.
- Network input cannot appear as an ambient new default.

## Related

- ADR-114 — cadence and verified ledger authority.
- ADR-117 — superseding Serve transport and trust boundary.
