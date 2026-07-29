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
follow_ups: ["the run-level metered budget cut (needs a metered provider · not hermetic · the static floor covers every bounded case)", "give the MODELS rung a spec code (NIKA-PROVIDER family) so a conformance harness can match it — parity class B", "a tier-scoping rule for the protocol third-party mode — parity class C", "semantic resume across a composition (law 10 · rides W6 semantic IR)", "render a composition-family verdict in the rust-python differential (spec scripts/oracle-differential.py)"]
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
This engine claims composition is **specified, and demonstrated on 8 of the 9
conditions**. It does **not** claim "proven" — that word stays locked until
the last open condition closes (semantic resume · law 10 · it rides W6), and
this ADR is re-issued.

> **Amended twice on 2026-07-29** (this receipt is meant to move; each
> amendment names what changed and why). Condition **9** was *open* — a
> parity nobody had run, because the adapter the spec's protocol had
> described for months did not exist. Written, run, and the composition
> family reads 9/9 across two independent oracles; the suite-wide residue
> is classified rather than rounded away, and the run found one real
> engine defect (a templated `model:` refused as a bare id). Condition
> **8** stays the single open one.
>
> **Amended 2026-07-29 (same day)** — condition 4 was published *partial*
> ("the cost half is implemented, the run-level test is owed"), and the
> owed test found something better than itself: the cost half is enforced
> **statically**, at the parent's pre-flight, by law 5's own summation —
> a child's floor refuses the parent's run before a single token. The
> run-level metered cut survives as a named, non-hermetic residual rather
> than a gap. The original wording stays above this line in git; the table
> below carries the current claim.

| # | condition (spec 14) | status | evidence |
|---|---|---|---|
| 1 | static resolution | **demonstrated** | `templated_target_is_refused_at_check` (`NIKA-COMP-001` · law 1); real relative-path resolution in every green test; the released 0.106.1 prints a dedicated `COMPOSITION` check line ("static, typed, contained and acyclic") |
| 2 | typed I/O | **demonstrated** | `child_runs_for_real_and_typed_outputs_remount` (law 2, both halves, on a real `echo` subprocess: child typed output → parent task value → parent output); `missing_required_child_input_is_refused_at_check` (`NIKA-COMP-004` names the missing input) |
| 3 | effect containment | **demonstrated** | `child_effect_outside_the_parent_boundary_is_refused` (`NIKA-COMP-002` · child `exec` under a net-only parent · laws 3/4) |
| 4 | inherited budgets | **demonstrated (static tier) · one residual named** | time half: `parent_timeout_bounds_the_real_child` (the child future is dropped at the parent task's deadline). Cost half: `the_childs_floor_bounds_the_parent_budget_before_any_token` — a parent with NO priced task of its own is refused BEFORE IT STARTS because the child's floor exceeds `--max-cost-usd` (rc 2 · no trace written · no provider touched · hermetic at $0), and the child alone under the same budget reports the SAME floor to the cent, so that floor is the child's own summed into the parent (law 5 at work). An unpriced child under the same tiny budget runs green — the gate reads the floor, never the mere presence of a budget. **Residual**: the run-level metered cut (`remaining_budget_usd` handed to the child runtime · `child_runner.rs:210`) is the backstop for spend the static floor cannot bound (an uncapped `infer:`), and demonstrating it requires a METERED provider — it cannot be shown hermetically, and is not claimed here |
| 5 | cycle detection | **demonstrated** | `static_cycle_is_refused_at_check` (a two-file cycle · `NIKA-COMP-003` at check AND `run` refusing through the same gate · law 7); `acyclic_chain_beyond_the_depth_bound_fails_closed_at_run` (`NIKA-SEC-003` backstop at real 10-deep nesting — the case static acyclicity cannot cover) |
| 6 | trace forest | **demonstrated** | `trace_forest_two_chains_and_the_parent_commits_to_the_child`: two journals on disk, each hash chain intact under an INDEPENDENT re-walk (the test's own sha256, genesis `nika-trace-v1`); live lesson run: 2 traces, `nika trace verify` rc=0 on both |
| 7 | nested receipts | **demonstrated (chain-commit tier)** | the parent's hash-chained terminal frame embeds the child's chain head at commit time plus the child's `def_hash` — tamper with any earlier child line and the committed head no longer matches (law 9's Merkle commitment at the journal tier; the spec-15 receipt ladder above it is `nika-proof` territory) |
| 8 | semantic resume | **open** | law 10's cache identity is the W6 semantic IR; today the child row records the pre-W6 identity (`def_hash`, asserted in the e2e). `resume_e2e.rs` has no composition coverage yet — owed with W6 |
| 9 | reference-model parity | **demonstrated (composition family)** | rendered 2026-07-29 through the adapter the protocol had only described (`spec conformance/adapters/nika-engine.py` · the Bowtie pattern): the nine `tests/deep/composition` fixtures are judged by BOTH oracles and agree **9/9**, and the whole `deep` tier is 37/37. Corpus-wide the two-oracle differential reads 49/49 · 0 unexplained · ledger **0** (`spec scripts/oracle-differential.py`). Suite-wide parity is 200/215 with the fifteen divergences classified in the protocol doc — **none of them composition** (they are: a category-only expectation the adapter cannot match · the codeless MODELS rung · tier-scope, a full engine judging more layers than a tier-scoped fixture binds · and one documented spec-vs-engine doctrinal disagreement on open-schema soundness). The measurement also FOUND and fixed an engine defect: the MODELS rung refused a templated `model:`, i.e. refused the parameterization pattern spec 08 §H8 recommends by name |

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
