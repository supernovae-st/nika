---
id: ADR-141
title: "nika-compile-reader size-cap member split: the candidate laws ascend to nika-compile-fidelity"
status: accepted
date: "2026-09-24"
phase: "pre-1.0 · compiler war room · V6 authoring"
deciders: ["@ThibautMelen"]
tags: ["architecture", "crates", "split", "size-cap", "compile"]
affects_crates: ["nika-compile-reader", "nika-compile-fidelity", "nika-compile"]
affects_layers: ["L4"]
supersedes: []
superseded_by: []
related: ["ADR-137", "ADR-138", "ADR-140"]
requires: []
enables: []
amends: []
fci: []
inv: ["INV-019"]
shadow_zones: []
nika_codes: []
timeline: "v0.120"
follow_ups: ["mutation and property attestations of the member, tracked with the unit's pending evidence", "the `#[non_exhaustive]` ratchet on the member's public types (`fidelity::Diagnostic` and the sketch's `Sketch`, `SketchTask`, `Edge`, `Verb`, `Hole`, `Fill`)", "canonical public API snapshots of reader, fidelity and the composed compiler unit"]
---

# ADR-141: nika-compile-reader size-cap member split — nika-compile-fidelity

## Context

ADR-138 (2026-09-21) descended the frozen deterministic reader and the typed plan it produces
to `nika-compile-reader`, a crate that depended on `serde_json` alone. While `nika-compile`
stood at its own 15,000 wall (ADR-140), the V6 authoring lane placed three modules in the
reader that do not read an intent: the fidelity laws that judge a candidate document (moved
from `nika-compile` on 2026-09-22), the sketch a seat proposes (2026-09-23) and the plan a
candidate states by its structure (2026-09-23). On 2026-09-24 the approval laws (Law 3 and
Law 3b, with `nika-schema` and `nika-check-analyzer` as new dependencies of the reader), the
raw-text-records law (Law 23) and the multiword file names of the copy family brought the
reader to **15,266 prod LOC against the 15,000 Diamond invariant**. The cap is a locked
maintainability budget, not advisory.

## Decision

Per **D-2026-07-09-N1** (a size-cap split is ONE architectural unit in several workspace
members — the ADR-110, ADR-115, ADR-137 and ADR-138 precedents), the candidate-judgment
family ascends from `nika-compile-reader` to a new L4 member crate `nika-compile-fidelity`,
placed ABOVE the reader and BELOW `nika-compile`: `nika-compile → nika-compile-fidelity →
nika-compile-reader`, never back. `nika-compile` binds the modules at its crate root, so every
`crate::fidelity::…` and `crate::sketch::…` path inside it is unchanged:

```rust
use nika_compile_fidelity::{fidelity, sketch};
```

Nothing of the member is re-exported by `nika-compile` or by the reader. Existing
`nika_onboard::compile` entry points remain available, but direct imports of
`nika_compile_reader::{candidate, fidelity, sketch}` must move to
`nika_compile_fidelity`. This is a Rust source break at the old reader paths, even
though tracked consumers have been migrated. The member binds the reader's
`plan`, `hot` and `lexicon` at its own root under the names the moved files have always used,
and re-exports nothing of the reader.

### Why this boundary, measured

Production edges on the tree of 2026-09-24, from `use` trees and inline paths:

| direction | edges |
|---|---:|
| the reader → the family | **0** |
| the family → the reader | `plan` · `hot::{fold, stated_sources, stated_destinations}` · `lexicon::GATE_WITHOUT_EFFECT` |
| `nika-compile` → the family | `fidelity` and `sketch` through two root bindings (the assembler's emit, the native and sketch doors) · `candidate` twice in the native door |
| outside the unit → the family | **0** |

The reading core — `lexicon` and its tables, `hot`, `gates`, `objects`, `plan`, `rules`,
`rule_tokens`, `rule_cues`, `stages`, `aggregate`, `columns` and `words` — is one strongly
connected region over the `paths` leaf; the family sits outside the forward closure of
`lexicon::read` and nothing inside that closure reaches it.

### What travels with the member

- `crates/nika-compile-reader/src/{fidelity.rs, fidelity/final_gate.rs, fidelity/records.rs,
  sketch.rs, candidate.rs}` → `crates/nika-compile-fidelity/src/`, and
  `assets/record_forms.txt` (Law 23's measured forms) with them.
- Their 34 unit tests travel in the same files; the one integration assertion that named the
  reader's path (`compile_native_fidelity`) now names the member's.
- The files keep their bytes, except the two helpers of `candidate`, which build `Step` and
  `Effect` through their constructors (INV-019): a struct expression of a `#[non_exhaustive]`
  type compiles only inside the crate that defines it. The values are identical.
- The dependencies `nika-schema` and `nika-check-analyzer`, which only the approval laws read.

## Consequences

- `nika-compile-reader` measures 13,736 prod LOC and `nika-compile-fidelity` 1,555 (the gate's
  own counter · 2026-09-24); `nika-compile` measures 14,999, one line for the new binding.
- The reader depends on `serde_json` alone again: deterministic, keyless and offline by
  construction, as ADR-138 stated and the dependency graph proves.
- The member inherits the admission of its unit (the ADR-115, ADR-137 and ADR-138 posture);
  mutation and property attestations stay pending evidence, tracked, never claimed.
- The member's public types are not yet `#[non_exhaustive]`; they moved as they were, and
  the ratchet is a follow-up, not a claim.
- The fidelity-only split does not reduce core size. In the composed ADR-140 boundary,
  now accepted and implemented, the cognition member reads `fidelity`, `sketch` and
  `candidate`; core retains deterministic fidelity checks and native replay.
- `nika-onboard` → `nika-compile` → {`nika-compile-fidelity` → `nika-compile-reader`} is ONE
  architectural unit for the count invariant (D-2026-07-09-N1); the lateral edges stay acyclic.

## Alternatives considered

### Alt A -- the sketch alone, below the reader

The reader would re-export it at its old path, but the sketch reads `hot::fold` and
`fidelity::strings`: a cycle, or a second copy of a normative fold (the reader already holds
two that differ on `ß`). No lower crate carries an equivalent primitive.

### Alt B -- the fidelity laws alone

The sketch reads `fidelity::strings`: the reader would depend on the member above it.

### Alt C -- the sketch alone, above the reader

Acyclic, but it leaves the laws, their dependencies and the candidate plan in the reader and
buys about 300 lines; the family is one concept.

### Alt D -- also the plan-level laws (`shape`, `structure`, `cardinality`, `unknowns`, `trigger_words`)

Acyclic and larger, but those are structural laws over the plan, which the frozen-reader
contract homes with the plan; the member would become a collection of unrelated helpers.

The dependency table and size figures above describe the fidelity-only split. The
composed ADR-140 graph also includes onboarding → cognition and cognition → core,
fidelity and reader. Canonical Linux API snapshots and complete compiler/transport
suites must verify that composed boundary before release.

## Related

- ADR-137 (the Compile core out of nika-onboard), ADR-138 (the reader), ADR-140 (accepted ·
  the seats' doors), D-2026-07-09-N1.
