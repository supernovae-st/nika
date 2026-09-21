---
id: ADR-137
title: "nika-onboard size-cap member split: the Compile core descends to nika-compile"
status: accepted
date: "2026-09-21"
phase: "pre-1.0 · compiler war room"
deciders: ["@ThibautMelen"]
tags: ["architecture", "crates", "split", "size-cap", "compile"]
affects_crates: ["nika-onboard", "nika-compile"]
affects_layers: ["L4"]
supersedes: []
superseded_by: []
related: ["ADR-110", "ADR-115", "ADR-138"]
requires: []
enables: []
amends: []
fci: []
inv: []
shadow_zones: []
nika_codes: []
timeline: "v0.120"
follow_ups: ["mutation and property attestations of the compile core", "a `nika-compile` L1 descent once the compiler no longer emits through the onboarding surface"]
---

# ADR-137: nika-onboard size-cap member split — nika-compile

## Context

The compiler war room of 2026-09-20/21 grew `nika-onboard::compile` from a bounded
skeleton door into the stateless Compile core: a frozen deterministic reader, a typed
semantic plan with typed computations, a finite composer with fourteen feasibility rules, a
deterministic assembler and a recorded-plan replay. Vector 24 measured `nika-onboard` at
**17,318 prod LOC against the 15,000 Diamond invariant**; vector 12 measured two of its
files above the 1,500-line wall. The cap is a locked maintainability budget
(`nika-invariants.md`), not advisory, and the push gate blocks on it.

## Decision

Per **D-2026-07-09-N1** (a size-cap split is ONE architectural unit in TWO workspace
members, the ADR-110 and ADR-115 precedents), the Compile core descends to a new L4 member
crate `nika-compile`. `nika-onboard` re-exports it at its **historical path**:

```rust
pub use nika_compile as compile;
```

so every call site inside and outside the unit keeps writing `nika_onboard::compile::…`.
The boundary moved; the surface did not.

### Why this boundary, measured

| direction | edges |
|---|---:|
| the surface (`nika init`, gallery, briefs) → `compile` | **1** (the module declaration) |
| `compile` → the surface | **2** (`intent::STOPWORDS`, `banner::sentence`) |

The two shared text helpers move into `nika_compile::text`; the surface re-imports them.
The member never depends back on the surface, exactly as `nika-check-analyzer` never
depends back on `nika-check`.

### What travels with the member

- `crates/nika-onboard/src/compile/*` → `crates/nika-compile/src/*` (`mod.rs` → `lib.rs`).
- The nine `compile_*` integration suites and the `compile_parity_v1.json` fixture.
- `assets/pattern_families.json` (the recall index's family table).
- The `rules` unit batteries move to `rules/tests.rs`; `lexicon.rs` carries a
  `lookup-table` LOC-EXEMPT because the frozen reader is made of head, cue and marker
  tables (the freeze itself is a season-2 decision: every new law lives in the typed plan).

## Consequences

- `nika-onboard` returns well under the 15k wall and loses eleven dependencies the surface
  never used.
- `nika-compile` inherits the admission of its unit; mutation and property attestations for
  the compile core are pending evidence, tracked, never claimed (the same posture ADR-115
  took for the analyzer).
- A later descent of `nika-compile` toward L1 (so the MCP oracle, Serve and the SDK depend
  on the compiler without the onboarding surface) is enabled, not decided.
