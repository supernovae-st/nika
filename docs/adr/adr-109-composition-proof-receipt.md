---
id: ADR-109
title: "Publish the composition proof receipt — what this engine may claim on spec 14"
status: accepted
date: 2026-07-29
phase: ""
deciders: ["@ThibautMelen"]
tags: ["composition", "receipt", "proof", "conformance", "spec-14", "trace"]
affects_crates: ["nika-runtime", "nika-check", "nika-cli"]
affects_layers: ["L0", "L3", "L4"]
supersedes: []
superseded_by: []
related: ["ADR-099", "ADR-108"]
requires: []
enables: []
amends: []
fci: []
inv: []
shadow_zones: []
nika_codes: ["NIKA-COMP-001", "NIKA-COMP-002", "NIKA-COMP-003", "NIKA-COMP-004", "NIKA-SEC-003"]
timeline: "2026-07-29"
follow_ups: ["demonstrate the cost half of law 6 at run level (a child that would outspend min(parent remaining, child declared) is cut)", "semantic resume across a composition (law 10 · rides W6 semantic IR)", "render a composition-family verdict in the rust-python differential (spec scripts/oracle-differential.py)"]
---

# ADR-109: Publish the composition proof receipt — what this engine may claim on spec 14

## Context

Spec `14-composition.md` opens with a word of discipline: an engine may say a
composition is *specified* the moment it parses and checks; it may only say
*proven* once it has demonstrated, **together, on real child execution**:
static resolution · typed I/O · effect containment · inherited budgets ·
cycle detection · the trace forest · nested receipts · semantic resume ·
reference-model parity. The external audit of 2026-07-29/30 asked for the
receipt, condition by condition, partial allowed — because a claim without a
receipt is exactly the class of drift the contract forbids.

The evidence base is executable, not prose: `crates/nika-cli/tests/composition_e2e.rs`
(8 tests against the REAL binary — real files, real subprocesses, an
independent sha256 chain re-walk that does not trust the engine's own
verifier), plus a live run of the taught lesson
(`crates/nika-pack/pack/examples/10-compose-pipeline.nika.yaml` · offline on
`mock/echo`) whose two journals pass `nika trace verify` (rc=0 each).

## Decision

Publish the per-condition receipt below and bind the claim vocabulary to it.
This engine claims composition is **specified, and demonstrated on 6 of the 9
conditions (one more partial)**. It does **not** claim "proven" — that word
stays locked until the three named owes close, and this ADR is re-issued.

| # | condition (spec 14) | status | evidence |
|---|---|---|---|
| 1 | static resolution | **demonstrated** | `templated_target_is_refused_at_check` (`NIKA-COMP-001` · law 1); real relative-path resolution in every green test; the released 0.106.1 prints a dedicated `COMPOSITION` check line ("static, typed, contained and acyclic") |
| 2 | typed I/O | **demonstrated** | `child_runs_for_real_and_typed_outputs_remount` (law 2, both halves, on a real `echo` subprocess: child typed output → parent task value → parent output); `missing_required_child_input_is_refused_at_check` (`NIKA-COMP-004` names the missing input) |
| 3 | effect containment | **demonstrated** | `child_effect_outside_the_parent_boundary_is_refused` (`NIKA-COMP-002` · child `exec` under a net-only parent · laws 3/4) |
| 4 | inherited budgets | **partial** | time half demonstrated: `parent_timeout_bounds_the_real_child` (the child future is dropped at the parent task's deadline · law 6). Cost half **implemented, not yet demonstrated at run level**: `nika-runtime/src/dispatch.rs` hands `child_budget` = the run ledger's remaining USD, the child runs under `min(this, its declared budget)` (`workflow_call.rs`) — the run-level cut test is owed |
| 5 | cycle detection | **demonstrated** | `static_cycle_is_refused_at_check` (a two-file cycle · `NIKA-COMP-003` at check AND `run` refusing through the same gate · law 7); `acyclic_chain_beyond_the_depth_bound_fails_closed_at_run` (`NIKA-SEC-003` backstop at real 10-deep nesting — the case static acyclicity cannot cover) |
| 6 | trace forest | **demonstrated** | `trace_forest_two_chains_and_the_parent_commits_to_the_child`: two journals on disk, each hash chain intact under an INDEPENDENT re-walk (the test's own sha256, genesis `nika-trace-v1`); live lesson run: 2 traces, `nika trace verify` rc=0 on both |
| 7 | nested receipts | **demonstrated (chain-commit tier)** | the parent's hash-chained terminal frame embeds the child's chain head at commit time plus the child's `def_hash` — tamper with any earlier child line and the committed head no longer matches (law 9's Merkle commitment at the journal tier; the spec-15 receipt ladder above it is `nika-proof` territory) |
| 8 | semantic resume | **open** | law 10's cache identity is the W6 semantic IR; today the child row records the pre-W6 identity (`def_hash`, asserted in the e2e). `resume_e2e.rs` has no composition coverage yet — owed with W6 |
| 9 | reference-model parity | **open (harness exists)** | the spec reference model carries composition generator blocks (`reference/README.md`) and the two-oracle differential runs the whole corpus (`spec scripts/oracle-differential.py` · 48/49 agree · 1 named ledger row, unrelated to composition) — but no composition-family differential verdict has been rendered yet |

## Consequences

### Positive
- "Proven" stays a load-bearing word: the public claim is now exactly as
  strong as the executable evidence, per the spec's own discipline note.
- The three gaps are named, greppable follow-ups (`follow_ups:` above) —
  the next session picks them up without re-deriving this audit.
- The receipt is replayable: every row cites a test that runs in ~1s
  (`cargo test -p nika-cli --test composition_e2e` · 8/8 green 2026-07-29)
  or a command (`nika trace verify`) — no trust in this document required.

### Negative
- The engine's public posture is deliberately weaker than "it all works"
  feels from the green suite. That is the cost of the vocabulary; we accept
  it.
- The receipt is dated evidence and will rot if not re-issued when the
  composition surface moves — re-issue triggers listed in Notes.

### Neutral
- Condition 7 is demonstrated at the journal chain-commit tier; the full
  spec-15 receipt composition (SEALED/ANCHORED tiers over nested runs) rides
  the `nika-proof` ladder and is not claimed here.

## Evidence / Affected code

- `crates/nika-cli/tests/composition_e2e.rs` — the 8-test demonstration suite
- `crates/nika-runtime/src/child.rs` — child-workflow execution (the composition seam)
- `crates/nika-runtime/src/dispatch.rs` — `child_budget` law-6 doc + the workflow-call dispatch
- `crates/nika-runtime/src/workflow_call.rs` — min(parent remaining, child declared)
- `crates/nika-check/src/composition.rs` — the static half (`NIKA-COMP-001..004`)
- `crates/nika-pack/pack/examples/10-compose-pipeline.nika.yaml` + `10-compose-child.nika.yaml` — the taught lesson (spec `examples/`, vendored)
- Live run 2026-07-29: parent+child journals under `.nika/traces/`, `nika trace verify` rc=0 each, typed output `{"brief":{"chars":76,…}}` flowed child→parent

## Alternatives considered

### Alt A — claim "proven" now
Rejected: two conditions have zero demonstration (semantic resume ·
reference parity). The spec's discipline paragraph exists precisely to
forbid this.

### Alt B — put the receipt in the spec repo
Rejected: the receipt is an ENGINE artifact about ONE implementation. The
spec defines the conditions and stays engine-agnostic; each implementation
publishes its own receipt.

## Related

- ADR-099 (durable-lite resume — the surface law 10 will compose with)
- ADR-108 (nika-proof split — the receipt ladder above condition 7)
- `spec 14-composition.md` — the contract this receipt answers
- `spec scripts/oracle-differential.py` — the two-oracle harness for condition 9

## Notes

Re-issue this receipt (same id, new date, updated table) when any of: the
cost-half run-level test lands (condition 4 → demonstrated) · resume learns
compositions (condition 8) · the differential renders a composition family
(condition 9) · any `NIKA-COMP` semantics change.
