---
id: ADR-109
title: "Publish the composition proof receipt — what this engine may claim on spec 14"
status: accepted
date: 2026-07-30
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
follow_ups: ["the run-level metered budget cut (needs a metered provider · not hermetic · the static floor covers every bounded case)", "give the MODELS rung a spec code (NIKA-PROVIDER family) so a conformance harness can match it — parity class B", "a tier-scoping rule for the protocol third-party mode — parity class C", "WITHIN-child resume granularity (law 10's semantic tier · rides the W6 semantic IR — the def_hash tier ships today)", "no shipped verb verifies a trace FOREST: the parent's committed child chain_head is checkable but unchecked (a `nika trace verify --forest` candidate)"]
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
(9 tests against the REAL binary — real files, real subprocesses, an
independent sha256 chain re-walk that does not trust the engine's own
verifier), the four composition cases in `crates/nika-cli/tests/resume_e2e.rs`
(condition 8), plus a live run of the taught lesson
(`crates/nika-pack/pack/examples/10-compose-pipeline.nika.yaml` · offline on
`mock/echo`) whose two journals pass `nika trace verify` (rc=0 each).

The 2026-07-30 pass added a discipline the earlier ones lacked: each row's
claim was probed BY HAND before any test was written, and each probe was
built to make the claim LIE. That is how the two defects below surfaced —
both had green suites over them.

## Decision

Publish the per-condition receipt below and bind the claim vocabulary to it.
This engine claims composition is **specified, and demonstrated on 9 of the 9
conditions — at the tiers each row names**. It still does **not** claim
"proven" flatly: two rows are demonstrated at a NAMED LOWER TIER than spec 14's
ceiling (condition 8 at the `def_hash` tier, its semantic tier owed to W6;
condition 7 at the chain-commit tier, its spec-15 receipt ladder owed to
`nika-proof`), and a tier is not a promotion — the word "proven" unlocks when
those two tiers close, and this ADR is re-issued again.

> **Amended 2026-07-30 — condition 8 CLOSES at the `def_hash` tier, and the
> socratic probe that closed it found a WRONG SKIP.** The row was *open* on
> the assumption that resume across a composition needed the W6 semantic IR.
> Probed by hand first (parent + child · real files · `--resume` after an
> edit), the truth was worse than "missing": resume ALREADY crossed the
> boundary and cache-hit the call — while the CHILD FILE'S CONTENT was in no
> hash at all. An edited child served the OLD child's output as a hit
> (measured: `{"echoed":"hello composition"}` when a fresh run said
> `goodbye`). That is ADR-099 trap 6 — "an edited task NEVER silently skips"
> — leaking through the one edge the key recipe did not cover, the file
> boundary. Fixed by putting the child's transitive source-closure digest
> into the calling task's DEFINITION identity (a Merkle fold, so a
> grandchild edit re-keys the whole chain), with a missing digest making the
> task non-eligible rather than skippable. Condition 8 now ships four e2e
> demonstrations; the SEMANTIC tier (within-child granularity · law 10's own
> words) is named as the residual, not claimed.
>
> **Amended 2026-07-30 (same day · the proof-ladder pass) — two verify
> defects, both in the dangerous direction.** The adversarial sweep over
> `nika trace verify`'s own claims ("the HIGHEST honestly-attained tier" ·
> "a re-written journal now needs the key, not just write access") broke two
> of them: (1) **append-after-seal was invisible** — a line chained after a
> valid `run_sealed` frame verified rc=0, the seal never mentioned, and the
> journal reported either "the run was killed or crashed" (a FALSE statement:
> the journal carries a seal) or a plain "OK · chain intact"; re-chaining
> needs only write access, so this is precisely the forgery the SEALED tier
> exists to catch. Now the `SEAL BURIED` class: exit FILE, the seal located
> and the trailing lines counted, and a torn tail after a seal still reads as
> the crash it is. (2) **an absent key was rendered as forgery** — an intact,
> genuinely sealed journal verified without its public key said "SEAL FORGED"
> and exited 2, an accusation on no evidence (the third-party case the
> artifact exists for). Now `SEAL UNATTRIBUTABLE` · exit ENV: the signature
> is not judged, non-zero so a gate still fails closed. Neither defect
> touched a condition row — they are the LADDER the rows are read through,
> so they are recorded here.
>
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
| 4 | inherited budgets | **demonstrated (static tier) · one residual named** | time half: `parent_timeout_bounds_the_real_child` (the child future is dropped at the parent task's deadline). Cost half: `the_child_floor_bounds_the_parent_budget_before_any_token` — a parent with NO priced task of its own is refused BEFORE IT STARTS because the child's floor exceeds `--max-cost-usd` (rc 2 · no trace written · no provider touched · hermetic at $0), and the child alone under the same budget reports the SAME floor to the cent, so that floor is the child's own summed into the parent (law 5 at work). An unpriced child under the same tiny budget runs green — the gate reads the floor, never the mere presence of a budget. **Residual**: the run-level metered cut (`remaining_budget_usd` handed to the child runtime · `child_runner.rs:210`) is the backstop for spend the static floor cannot bound (an uncapped `infer:`), and demonstrating it requires a METERED provider — it cannot be shown hermetically, and is not claimed here |
| 5 | cycle detection | **demonstrated** | `static_cycle_is_refused_at_check` (a two-file cycle · `NIKA-COMP-003` at check AND `run` refusing through the same gate · law 7); `acyclic_chain_beyond_the_depth_bound_fails_closed_at_run` (`NIKA-SEC-003` backstop at real 10-deep nesting — the case static acyclicity cannot cover) |
| 6 | trace forest | **demonstrated** | `trace_forest_two_chains_and_the_parent_commits_to_the_child`: two journals on disk, each hash chain intact under an INDEPENDENT re-walk (the test's own sha256, genesis `nika-trace-v1`); live lesson run: 2 traces, `nika trace verify` rc=0 on both |
| 7 | nested receipts | **demonstrated (chain-commit tier) · one gap named** | the parent's hash-chained terminal frame embeds the child's chain head at commit time plus the child's `def_hash` — tamper with any earlier child line and the committed head no longer matches (law 9's Merkle commitment at the journal tier; the spec-15 receipt ladder above it is `nika-proof` territory). **Gap (probed 2026-07-30)**: the commitment is CHECKABLE but no shipped verb CHECKS it — `nika trace verify <parent>` walks the parent file only and says nothing about the child (correctly: it never overclaims — measured, a falsified child journal leaves the parent's own verdict rc=0 with zero mention of the child). The demonstration is `trace_forest_two_chains_and_the_parent_commits_to_the_child`'s own independent sha256 re-walk; an operator has no command for it (`--forest` is the follow-up) |
| 8 | semantic resume | **demonstrated (`def_hash` tier) · the semantic tier named** | four e2e in `resume_e2e.rs` on REAL nested runs: unchanged files cache-hit ACROSS the boundary (`resume_across_a_composition_cache_hits_the_call`); an EDITED child re-runs the call instead of serving the stale output (`an_edited_child_reruns_the_call_instead_of_serving_stale_output` — the wrong skip this row's probe FOUND and fixed); a GRANDCHILD edit re-keys transitively through the Merkle closure fold (`an_edited_grandchild_reruns_the_call_transitively`); a run torn mid-composition re-runs the child WHOLE (`a_composition_torn_mid_child_reruns_the_child_whole`). The identity is the child's transitive SOURCE-closure digest, composer-resolved (`child_runner::closure_digests`) and folded into the calling task's definition hash (`nika-runtime/src/resume.rs`); a target the composer cannot resolve yields NO stamp (the task re-runs · never a wrong skip). **Named residual**: law 10's own tier is the W6 SEMANTIC IR — within-child granularity (resuming INSIDE a child, skipping only the child tasks whose semantics are unchanged) is not claimed; today a changed child re-runs whole, and two children differing only in formatting re-run rather than hit |
| 9 | reference-model parity | **demonstrated (composition family)** | rendered 2026-07-29 through the adapter the protocol had only described (`spec conformance/adapters/nika-engine.py` · the Bowtie pattern): the nine `tests/deep/composition` fixtures are judged by BOTH oracles and agree **9/9**, and the whole `deep` tier is 37/37. Corpus-wide the two-oracle differential reads 49/49 · 0 unexplained · ledger **0** (`spec scripts/oracle-differential.py`). Suite-wide parity is 200/215 with the fifteen divergences classified in the protocol doc — **none of them composition** (they are: a category-only expectation the adapter cannot match · the codeless MODELS rung · tier-scope, a full engine judging more layers than a tier-scoped fixture binds · and one documented spec-vs-engine doctrinal disagreement on open-schema soundness). The measurement also FOUND and fixed an engine defect: the MODELS rung refused a templated `model:`, i.e. refused the parameterization pattern spec 08 §H8 recommends by name |

## Consequences

### Positive
- "Proven" stays a load-bearing word: the public claim is now exactly as
  strong as the executable evidence, per the spec's own discipline note —
  and each row that sits below spec 14's ceiling says WHICH tier it holds
  at, so a tier can never be read as a promotion.
- Every gap is a named, greppable follow-up (`follow_ups:` above) — the
  next session picks them up without re-deriving this audit.
- The receipt is replayable: every row cites tests that run in ~2s
  (`cargo test -p nika-cli --test composition_e2e --test resume_e2e` ·
  9/9 + 12/12 green 2026-07-30) or a command (`nika trace verify`) — no
  trust in this document required.
- Two verify defects are closed with the probes that found them kept as
  tests (`a_buried_seal_reports_tampered_and_never_blames_a_crash` ·
  `a_key_id_mismatch_across_all_candidates_is_unattributable_never_forged`),
  each mutation-proven: reverting the production code kills them.

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

- `crates/nika-cli/tests/composition_e2e.rs` — the 9-test demonstration suite
- `crates/nika-cli/tests/resume_e2e.rs` — the 4 composition cases (condition 8)
- `crates/nika-runtime/src/resume.rs` — the child-closure digest in the calling
  task's definition identity (`workflow_targets` · the `child_closure` payload)
- `crates/nika-cli/src/verbs/run/child_runner.rs` — `closure_digests` (the
  composer's transitive Merkle fold over the child sources)
- `crates/nika-dap/src/anchor/tier.rs` — `SealTier::Buried` (the
  append-after-seal class) + `SealTier::Unattributable` (an absent key is a
  missing input, never forgery)
- `crates/nika-trace/src/trace_verify.rs` — the TAMPERED headline that
  replaces the walk's crash reading when a seal is buried
- `crates/nika-runtime/src/child.rs` — child-workflow execution (the composition seam)
- `crates/nika-runtime/src/dispatch.rs` — `child_budget` law-6 doc + the workflow-call dispatch
- `crates/nika-runtime/src/workflow_call.rs` — min(parent remaining, child declared)
- `crates/nika-check/src/composition.rs` — the static half (`NIKA-COMP-001..004`)
- `crates/nika-pack/pack/examples/10-compose-pipeline.nika.yaml` + `10-compose-child.nika.yaml` — the taught lesson (spec `examples/`, vendored)
- Live run 2026-07-29: parent+child journals under `.nika/traces/`, `nika trace verify` rc=0 each, typed output `{"brief":{"chars":76,…}}` flowed child→parent

## Alternatives considered

### Alt A — claim "proven" now (9/9 demonstrated)
Rejected AGAIN on 2026-07-30, for a new reason. The original rejection was
"two conditions have zero demonstration"; that is no longer true. But two
rows hold at a tier BELOW the one spec 14 names (condition 8 at `def_hash`
rather than the semantic IR · condition 7 at chain-commit rather than the
spec-15 receipt ladder), and "9 of 9 demonstrated" would read as the
ceiling. Counting rows is not the same as meeting them.

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
run-level metered cut lands (condition 4's residual) · W6's semantic IR
reaches resume (condition 8 climbs from the `def_hash` tier) · a verb
verifies a trace FOREST (condition 7's gap) · the spec-15 receipt ladder
composes over nested runs (condition 7's tier) · any `NIKA-COMP` semantics
change · any change to the verify ladder's classes (the rows are read
through it).

The word "proven" unlocks when conditions 7 and 8 reach spec 14's own
tiers — not when the row count reaches nine.
