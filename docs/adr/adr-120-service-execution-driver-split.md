---
id: ADR-120
title: "descend the shared service execution driver out of nika-runtime"
status: accepted
date: "2026-08-23"
phase: "pre-1.0 · execution access"
deciders: ["@ThibautMelen"]
tags: ["architecture", "runtime", "execution", "crate-size", "diamond", "split"]
affects_crates: ["nika-runtime", "nika-execution", "nika-service-execution", "nika-cli"]
affects_layers: ["L3", "L4"]
supersedes: []
superseded_by: []
related: []
requires: []
enables: []
amends: []
fci: []
inv: []
shadow_zones: ["serve-execution-authority"]
nika_codes: []
timeline: "pre-1.0"
follow_ups:
  - "admit nika-service-execution through the full Diamond crate ledger when its WIP surface next changes"
  - "make the first nika-serve execution adapter consume this driver without adding a second composition path"
---

# ADR-120: descend the shared service execution driver out of `nika-runtime`

## Context

W06 adds one shared driver that joins descriptor-rooted, owned-byte admission
from `nika-execution` to the production composition already owned by
`nika-runtime`. The local CLI and future service adapter must use this same
driver so nested workflows, permits, closure digests, redaction, and effect
construction cannot drift between interfaces.

Keeping that driver inside `nika-runtime` raised the crate from 14,931 to
16,024 production lines. `scripts/ci/check-crate-size.sh` enforces a hard
15,000-line budget. Hiding source from the counter, weakening the ceiling,
compressing the implementation, or dropping tests would preserve the wrong
boundary rather than fix it. ADR-108 and the later `nika-secret` descent
establish the workspace precedent: when a coherent concern fills a crate,
move the concern into the lowest honest Diamond layer.

## Decision

Create the WIP L3 crate **`nika-service-execution`** and move the W06 driver
and its focused tests there as one unit.

The new crate depends on the two L3 peers whose contracts it joins:

- `nika-execution` owns immutable snapshot admission and the sealed
  `ExecutionContext` pairing;
- `nika-runtime` owns generic DAG execution and production composition.

`nika-runtime` no longer depends on `nika-execution` or the driver's
`tempfile` test dependency. It exposes one narrow additional seam,
`compose::service_runtime`, beside `production_runtime`; the driver remains
the only owner of its service-safe surface and metadata-only result types.
`nika-cli` imports the moved driver from `nika-service-execution`. No HTTP,
listener, authentication, job-store, SDK, or `nika-serve` code enters this
split.

The moved public types intentionally leave the `nika_runtime` namespace and
live under `nika_service_execution`. Both crates are internal, unpublished
workspace crates, and every current consumer is updated in the same commit.
The regenerated `nika-runtime/public-api.txt` records the removal and the one
new composer seam explicitly.

## Consequences

### Positive

- `nika-runtime` returns below the hard production-LOC ceiling without an
  exemption or behavioral deletion.
- Local CLI/ARM and future remote execution keep one production driver and
  one child-composition implementation.
- Snapshot admission remains absent from the generic runtime crate.
- The service result remains redacted and metadata-only; the move adds no
  transport surface.

### Negative

- The workspace gains a seventieth crate and one same-layer L3 dependency.
- The internal Rust namespace changes for the eight moved driver types.
- `service_runtime` becomes public so a sibling crate can consume the narrow
  production seam; its public baseline must remain floored.

### Neutral

- `nika-service-execution` lands WIP and is not published independently.
- `Cargo.lock`, the Diamond layer registry, and the CLI dependency graph gain
  the new member.
- The split changes ownership, not execution semantics.

## Required evidence

1. `scripts/ci/check-crate-size.sh` passes with every crate below 15,000
   production lines.
2. `scripts/ci/check-layering.sh` accepts all 70 workspace crates.
3. `cargo check --workspace --locked` passes with the regenerated lockfile.
4. Focused library tests pass for `nika-cli`, `nika-runtime`,
   `nika-execution`, and `nika-service-execution`.
5. Workspace all-target clippy passes with warnings denied, and workspace
   formatting is clean.
6. `nika-runtime/public-api.txt` is regenerated deterministically and contains
   `compose::service_runtime` but none of the moved driver types.
7. The CLI extinction fixture reads the new crate and continues to reject a
   second execution/composition path.

## Alternatives considered

### Shrink or exempt `nika-runtime`

Rejected. The 1,024-line excess is a real architectural boundary signal. An
exemption, test deletion, `cfg` trick, or compressed implementation would make
the maintainability gate dishonest.

### Put the driver in `nika-execution`

Rejected. The driver consumes `nika-runtime` composition; making
`nika-execution` depend on runtime would invert the admission boundary and
create a cycle with the pre-existing runtime-to-execution edge.

### Put the driver in an L4 interface crate

Rejected. CLI, ARM, and Serve must share it. An interface-owned driver would
make one interface canonical and force the others to depend sideways on an
adapter rather than on a shared L3 implementation.
