---
id: ADR-145
title: "nika-compile-clauses size-cap member: how one clause of a request reads ascends from the reader, the verifier and the core's surface"
status: accepted
date: "2026-10-07"
phase: "pre-1.0 · compiler architecture"
deciders: ["@ThibautMelen"]
tags: ["architecture", "crates", "split", "size-cap", "compile"]
affects_crates: ["nika-compile-clauses", "nika-compile-reader", "nika-compile-cognition", "nika-compile"]
affects_layers: ["L4"]
supersedes: []
superseded_by: []
related: ["ADR-137", "ADR-138", "ADR-140", "ADR-141", "ADR-142", "ADR-144"]
requires: []
enables: []
amends: []
fci: []
inv: []
shadow_zones: []
nika_codes: []
timeline: "v0.123"
follow_ups: ["mutation and property attestations of the member, tracked with the unit's pending evidence", "the Linux public API job confirms the member's snapshot and the reader's, written from a macOS render"]
---

# ADR-145: nika-compile-clauses size-cap member — how one clause of a request reads

## Context

At the integration head `75073eb0c` (2026-10-07) the pre-push hygiene gate is red on the 15,000
prod-LOC wall (`scripts/ci/check-crate-size.sh`, the gate's own counter): `nika-compile-cognition`
measures **16,632**, `nika-compile-reader` **15,614** and `nika-compile` **15,014**. The cap is a
locked maintainability budget, not advisory, and every next change to the verifier, the reader's
structural laws or the core needs room inside it.

Three readings of one clause of a request live in these crates although none of them needs the
crate that holds it: the reader's prohibition reading (`structure::{negated_demand,
states_operation, pure_prohibition}` and the private `Clause` reader they share), which only the
verifier reads; the verifier's parts of a request (`verify/parts.rs`) with the two readings of a
part it shares with the whole-request verdict (`restricts`, `asks_an_operation`); and the core
surface's stated spellings (`surface::observed::stated_spellings`), which only the seats'
spelling law reads. The proposal merge's word tables and classifiers in the reader's `words.rs`
(the key a clause is folded by, a draft that is no language work, a format kept by
construction, the content words of a clause) are read by the merge alone, as that module's own
documentation says.

## Decision

Per **D-2026-07-09-N1** (a size-cap split is ONE architectural unit in several workspace
members, the ADR-137, ADR-138, ADR-141 and ADR-142 precedents), these readings ascend to a new
L4 member crate `nika-compile-clauses`, placed ABOVE the reader and BELOW the seats' doors:
`nika-compile-cognition → nika-compile-clauses → nika-compile-reader`, never back. `nika-compile`
does not depend on it.

- `prohibition` · the reader's `structure.rs` from `NEGATIONS` through `pure_prohibition`, with
  its three unit tests, bytes unchanged but for its paths. The words it reads stay the reader's:
  `stages::{restriction_word, keep_lead, operation_word}` and `rules::exclusion_lead` become
  public (each documented, `#[must_use]`), and the head table is asked through a new predicate,
  `lexicon::reads_a_head(word)`, so no `Head` type crosses the boundary. The rest of
  `structure.rs` (the laws, the context statement, the selection demand, `restricts`, the
  function-word readings) stays in the reader, which reads it itself.
- `parts` · `crates/nika-compile-cognition/src/cognition/verify/parts.rs` (`git mv`) with its
  tests (`verify/tests/cuts.rs` → `parts/tests.rs`, `git mv`) and the faithful verdict's
  `restricts`, `asks_an_operation` and `read_contractions`. `parts`, `restricts` and
  `asks_an_operation` become public; the verifier imports them.
- `spellings` · `stated_spellings` with its private `stated` and `boundary`, from
  `nika_compile::surface::observed`, and its test (`crates/nika-compile/tests/observed_statement.rs`
  → `crates/nika-compile-clauses/tests/`, `git mv`). The bounded canonical-spelling law the core
  itself reads (`equivalent_spellings`) stays in the core.
- `words` · the proposal merge's tables and classifiers of the reader's `words.rs`
  (`clause_key`, `fold_words`, `SERIALIZE_VERBS`, `DATA_WORDS`, `LANGUAGE_WORDS`,
  `serialization_draft`, `NUMBER_WORDS`, `ONLY_WORDS`, `CONVERSION_WORDS`, `SAME_COLUMNS`,
  `only_format_words`, `content_words`). The cognition binds its historical `crate::words` path
  to the member, so the merge reads them unchanged; the day-part compounds and the recurrences the
  reader, the core and the trigger reading read stay in the reader.

The member binds the reader's modules at its root under the names the moved prohibition reading
and the merge's classifiers have always used (`super::stages`, `super::rules`, the fidelity
precedent of ADR-141).

### Why this boundary, measured

Edges on the tree of `75073eb0c`, production code only (the shared `rs_prod_files` set):

| direction | edges |
|---|---|
| the reader → the member | **0** |
| the member → the reader | `stages::{restriction_word, keep_lead, operation_word}` · `rules::{exclusion_lead, by_construction_tail}` · `lexicon::reads_a_head` · `rule_tokens::fold` · `structure::{laws, restricts, only_function_words}` |
| `nika-compile` → the member | **0** |
| `nika-compile-cognition` → the member | `parts::{parts, restricts, asks_an_operation}` (the verifier) · `spellings::stated_spellings` (the spelling law) · `words::{clause_key, fold_words, serialization_draft, only_format_words, content_words, CONVERSION_WORDS, LANGUAGE_WORDS}` (the proposal merge, through `crate::words`) |
| outside the unit → the member | **0** |

The design first moved the prohibition reading alone out of the reader (615 lines). Widened
predicates carry `#[must_use]` under the workspace's pedantic `must_use_candidate`, and the head
predicate adds its own lines: the reader measured **15,010**. The clause key alone would have left
it at 14,999, and `fold_words` is shared with `serialization_draft`; the merge's vocabulary as a
whole is the closed block the reader never reads.

## Consequences

- `nika-compile-reader` measures **14,645** prod LOC (from 15,614), `nika-compile` **14,954**
  (from 15,014) and `nika-compile-clauses` **1,395**; `nika-compile-cognition` loses the parts and
  the three part readings (16,632 → 16,346), and reaches the wall with ADR-146.
- The reader's public API loses `structure::{negated_demand, pure_prohibition, states_operation}`
  and the twelve merge items of `words`, and gains `stages::{restriction_word, keep_lead,
  operation_word}`, `rules::exclusion_lead` and `lexicon::reads_a_head`. `nika-compile`'s loses
  `surface::observed::stated_spellings`. No consumer outside the unit read the removed items; a
  direct import of them must move to `nika_compile_clauses` (a Rust source break at the old
  paths, the ADR-142 posture).
- The tests move with their code: the reader's three prohibition tests, the verifier's fourteen
  cut tests and the core's stated-spelling integration suite (three tests).
- The member inherits the admission of its unit (the ADR-137, ADR-138, ADR-141 and ADR-142
  posture); mutation and property attestations stay pending evidence, tracked, never claimed.
- The workspace registries name it: the members, the layer table, the crate spec, the public API
  coverage floor and the generated status blocks.

## Alternatives considered

### Alt A — the clause key alone beside the prohibition reading

It leaves the reader at 14,999, one line below the wall, with every structural law still owed
blocked again by the next change.

### Alt B — the readings into the seats' doors

`nika-compile-cognition` is itself over the wall; the readings would add to it, and the core's
stated spellings would climb two members for one reader.

### Alt C — raise the cap

The 15,000 invariant is a locked maintainability budget, not advisory.

## Related

- ADR-137 (the Compile core out of nika-onboard), ADR-138 (the reader), ADR-140 (the seats'
  doors), ADR-141 (the candidate laws), ADR-142 (the trigger reading), ADR-144 (the session's
  change set), ADR-146 (the seats a host lends a preparation), D-2026-07-09-N1.
