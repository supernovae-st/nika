---
id: ADR-147
title: "Descend the candidate composer and the document operations to nika-compile-seats"
status: proposed
date: "2026-10-08"
phase: "pre-1.0 · compiler architecture"
deciders: ["@ThibautMelen"]
tags: ["architecture", "crates", "size-cap", "compile"]
affects_crates: ["nika-compile-cognition", "nika-compile-seats"]
affects_layers: ["L4"]
supersedes: []
superseded_by: []
related: ["ADR-140", "ADR-145", "ADR-146"]
requires: []
enables: []
amends: ["ADR-146"]
fci: []
inv: []
shadow_zones: []
nika_codes: []
timeline: "v0.123"
follow_ups: ["decider review of this size-cap application of D-2026-07-09-N1", "the Linux public API job confirms the member's snapshot"]
---

# ADR-147: Descend the candidate composer and the document operations to nika-compile-seats

## Context

`nika-compile-cognition` measured **14,998** prod LOC at the 0.123 integration base, two lines
under the 15,000 wall (`scripts/ci/check-crate-size.sh`, the gate's own counter), and **15,618**
once the document revision route over a record-less base landed (`df9468b6e`). The cap is a
locked maintainability budget, not advisory. ADR-146 already placed the decision seats, the
rehearsal port and the Foundry beside the doors, below them, in `nika-compile-seats`.

## Decision

Under the same size-cap rule (**D-2026-07-09-N1**) and the same measure as ADR-146, two pieces
with no edge into the doors descend to `nika-compile-seats`, bytes unchanged but for the paths and
the visibilities the move changes:

- `foundry::document` · the operations a revision states over a complete base no semantic record
  binds (`set` a literal, `compose` or `rebind` an admitted component, a whole `replace` that
  claims no preservation), their record and their answer schema. They use `foundry` itself,
  `nika-compile` and, for every document edit, the `nika-schema` document editor (re-read and
  byte-proven); the revision door and the forensic summary read `apply`, `nodes`,
  `components`, `carried`, `record`, `answer_schema`, `ROUTE` and `OPERATIONS`.
- `compose` · the candidate composer: the admissible options a WARM decision seat is offered, the
  distinct COLD plans judged by the deterministic feasibility filter before any seat sees them.
  It reads the plan and the reading of `nika-compile-reader` and `Hit` of `nika-compile`; only
  `cognition.rs` reads it (`compose`, `Candidate`, `Dimension`, `signature`, `rank`, `describe`,
  `classify_disagreement`).

This adds one lateral edge, `nika-compile-seats → nika-compile-reader`, never back. The
composer's types become `#[non_exhaustive]` public items the doors only read; its functions are
documented, `#[must_use]` where they return a plain value, with `# Errors` where they return a
`Result`.

## Consequences

- The gate's counter reads **14,377** for the cognition and **7,177** for the member.
- The library tests move with their files (seven document tests, the composer's sixteen); the
  three tests of the document route stay beside the revision door. Behaviour is unchanged.
- The member's public API snapshot gains the two modules; the cognition's is unchanged (neither
  piece was public there).
- The layer registry names the new lateral edge.

## Alternatives considered

### Alt A — keep the document operations in the cognition and trim elsewhere

The cognition held two lines of headroom: any feature in it needs a relocation, and the
operations are Foundry edits with no seat call, already described by the member's charter.

### Alt B — raise the cap

The 15,000 invariant is a locked maintainability budget, not advisory.

## Related

- ADR-146 (the member and its precedent measure), ADR-140 (the seats' doors), ADR-145 (the clause
  readings).
