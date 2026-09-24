---
id: ADR-138
title: "nika-compile size-cap member split: the frozen reader descends to nika-compile-reader"
status: accepted
date: "2026-09-21"
phase: "pre-1.0 · compiler war room"
deciders: ["@ThibautMelen"]
tags: ["architecture", "crates", "split", "size-cap", "compile"]
affects_crates: ["nika-compile", "nika-compile-reader"]
affects_layers: ["L4"]
supersedes: []
superseded_by: []
related: ["ADR-137", "ADR-141"]
requires: []
enables: []
amends: []
fci: []
inv: []
shadow_zones: []
nika_codes: []
timeline: "v0.120"
follow_ups: ["mutation and property attestations of the reader, tracked with the season-2 debt wave", "the season-2 descent of `lexicon.rs` below the 1,500-line wall (it keeps its `lookup-table` LOC-EXEMPT until then)"]
---

# ADR-138: nika-compile size-cap member split — nika-compile-reader

## Context

ADR-137 descended the Compile core out of `nika-onboard` on 2026-09-21. The same day the
product-convergence line (bare `nika` → Session → Compile → Check → Run) and the season-2
typed computations grew `nika-compile` to **18,028 prod LOC against the 15,000 Diamond
invariant** (vector 24 · `crate-size-15k`), and the push gate refused the branch. The cap
is a locked maintainability budget (`nika-invariants.md`), not advisory.

## Decision

Per **D-2026-07-09-N1** (a size-cap split is ONE architectural unit in TWO workspace
members, the ADR-110, ADR-115 and ADR-137 precedents), the frozen deterministic reader and
the typed plan it produces descend to a new L4 member crate `nika-compile-reader`.
`nika-compile` keeps composing, assembling and previewing; it depends on the reader and
reads it at its historical module paths:

```rust
use nika_compile_reader::{columns, hot, lexicon, objects, paths, plan, rule_tokens, rules};
pub use nika_compile_reader::text;
```

`nika_compile::text` keeps the exact surface ADR-137 gave it (`STOPWORDS`, `banner_lines`,
`is_label`, `banner_sentence`); nothing else of the reader is re-exported publicly. The
member never depends back on the composer.

### Why this boundary, measured

The reader was already a closed region: it reads words into a plan and never sees a
`CompileOutcome`. Two clusters of pure text helpers had grown on the wrong side of it and
travel with the reader; they were the only edges that pointed from the reader into the
composer.

| direction | edges before the split |
|---|---:|
| the composer → the reader | 7 module paths (`plan` · `lexicon` · `rules` · `paths` · `objects` · `hot` · `columns`) |
| the reader → the composer | 2 clusters: `shape::{fold, SIZE_UNITS, ATTEMPT_UNITS, DISTRIBUTIVE_OPENERS, without_distributive_tail}` (five text tables, now in `rule_tokens` and `objects`) and `network::{Facet, page_facet, carried}` (the facet an object names and the back-reference before a destination, now in `objects`) |

`predicate` stays in `nika-compile`: its only caller is `cognition`, it decodes the proposal
shape `cognition` owns, and it reaches the reader's typed rules through their constructors.

### The forward-compatibility ratchet

Every public type of the reader is `#[non_exhaustive]`. The composer builds plan elements
through `Step::new`, `Effect::new`, `Obligation::new`, `Binding::new`, `Clause::new`,
`Aggregation::new` and `Derived::new` (INV-019: a per-crate constructor on every
`#[non_exhaustive]` struct) and matches the reader's enums with a wildcard arm that names
what an unknown element means (a policy the composer does not know leaves its effect
unresolved, never bound; a retrieval it does not know emits nothing). The reader may grow a
variant without breaking the member above it.

### What travels with the member

- `crates/nika-compile/src/{plan, lexicon (+ its tables), gates, objects, paths, columns,
  hot, rules (+ its unit battery), aggregate, text, rule_cues, rule_tokens, stages}.rs` →
  `crates/nika-compile-reader/src/`. `lexicon.rs` keeps its `lookup-table` LOC-EXEMPT.
- The multilingual sentence battery that reads a sentence AND compiles it
  (`lexicon/tests.rs`) becomes the `compile_reader_sentences` integration suite of
  `nika-compile`, where both members are in reach. 216 `#[test]` before, 216 after.

## Consequences

- `nika-compile` measures 9,778 prod LOC and `nika-compile-reader` 8,432 (the gate's own
  counter · 2026-09-21); both sit under the 12,000 descent window.
- `nika-compile-reader` depends on `serde_json` alone: the reader is deterministic,
  keyless and offline by construction, and the dependency graph now proves it.
- The reader inherits the admission of its unit (the ADR-115 and ADR-137 posture);
  mutation and property attestations stay pending evidence, tracked, never claimed.
- `nika-onboard` → `nika-compile` → `nika-compile-reader` is ONE architectural unit for
  the count invariant (D-2026-07-09-N1); the lateral edges stay acyclic.
