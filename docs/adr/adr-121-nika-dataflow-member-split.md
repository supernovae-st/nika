---
id: ADR-121
title: "nika-runtime size-cap member split: the run's dataflow descends to nika-dataflow"
status: accepted
date: "2026-08-25"
phase: "pre-1.0 · post-P2-submission hardening"
deciders: ["@ThibautMelen"]
tags: ["architecture", "crates", "split", "size-cap", "dataflow", "expressions"]
affects_crates: ["nika-runtime", "nika-dataflow"]
affects_layers: ["L1", "L3"]
supersedes: []
superseded_by: []
related: ["ADR-115", "ADR-110", "ADR-108", "ADR-003", "ADR-027"]
requires: []
enables: []
amends: []
fci: []
inv: []
shadow_zones: []
nika_codes: []
timeline: "v0.114"
follow_ups: []
---

# ADR-121: nika-runtime size-cap member split — nika-dataflow

## Context

`nika-runtime` measured **14,787 prod LOC against the 15,000 Diamond
invariant** — 213 lines of headroom on the crate every feature lands in.
The wall surfaced through #1171, a written pull request that could not
land: branch and `main` were each under the cap while their **merge**
crossed it. Only the merge commit can see that, and no local gate runs on
one.

What made it a decision rather than an inconvenience is **how the budget
was being paid**. #1171 had already compressed three explanatory doc
blocks to one-liners in the same diff that added its feature — and still
did not fit. A comment is the cheapest line to delete, so it is the first
line deleted, which means a *maintainability* budget gets satisfied by
discarding exactly the thing that makes the crate maintainable. The number
stays honest while the property it stands for erodes.

`nika-runtime` is itself the crate that ABSORBED one of these moves: the
production composition descended here from `nika-cli` at the same wall on
2026-07-22, and the secret-resolution half left for `nika-secret` on
2026-08-06. The cap is a locked maintainability budget
(`nika-invariants.md`), not advisory.

### The measurement that qualified the wall

Qualifying the number first found the **counter** wrong — in both
directions at once (fixed in the same arc · `scripts/ci/prod-loc.py`):
braces inside string literals ended a `#[cfg(test)]` module early (412
lines of test body charged to production in `expr.rs`), `#[cfg(test)] mod
foo;` swallowed whichever block came next and *hid* production lines, and
a phantom trailing line was charged per file. It was wrong for 69 of 71
crates.

So the honest headroom was 213, not the 6 first reported, and #1171 would
have merged at ~14,813 and fit. **That specific PR was blocked by an
instrument defect.** The wall itself was not fiction: 14,787 is 98.6% of
the cap, and the next feature pays the same tax in comments. The split
stands on the wall, not on the blocked PR.

## Decision

Per **D-2026-07-09-N1** (a size-cap split is ONE architectural unit in TWO
workspace members), the **run's dataflow** descends to a new L1 member
crate `nika-dataflow`. Two questions live there, and they are one question:

- **What a task record IS** — `TaskStatus` · `TerminalCause` ·
  `TaskErrorRecord` · `TaskRecord`, the spec-13 transition law (`legal`),
  the failure-cause triage (`failure_cause`), the Outcome IR
  (`outcome_json`), the canonical value rendering (`render_value`), and
  `TIMEOUT_CODE` — the wire code the triage reads.
- **How a value referencing those records resolves** — `Scope`, `${{ }}`
  island rendering (`render` · `render_json`), `cel-subset/0.1` gate
  evaluation (`eval_when` · `resolve_expr`), and `output:` named jq
  bindings (`eval_binding`) — plus the four evaluation error classes as
  `DataflowError`.

They descend together because they are not separable: `expr` projects a
`TaskRecord` into the CEL object a `${{ tasks.x.output }}` island reads,
and renders values back out through `record::render_value`. A seam between
them would cut one concept in half.

### Why this module and not another

The cut was chosen by **coupling, not line count**. Measured before the
move, the trio had exactly **three** intra-crate edges —
`crate::errors::RuntimeError`, `crate::task::TIMEOUT_CODE`, and
`expr → record` (internal to the trio) — against **ten-plus** for the next
plausible cluster (approval / pause / resume / recover, woven through
`task`, `settle`, `integrity`, `agent_events`, `proof`, `witness` and
`stamp`). It also needs nothing the runtime owns: no `EventSink`, no
clock, no compose ladder, no session state. It is pure — zero I/O, zero
async — which is why the executor keeps the effects and this keeps the
evaluation.

### The seam does not move

`nika-runtime` re-exports `TaskRecord`, `TaskStatus`, `TerminalCause`,
`TaskErrorRecord` and `legal` at their historical `nika_runtime::…` paths,
and keeps `crate::{expr,jq,record}` as module aliases so all fifteen
calling files read exactly as before. `RuntimeError` keeps its four
evaluation classes through a transparent
`Dataflow(#[from] DataflowError)` — `Display`, `Diagnostic`, `spec_code()`
and `nika_code()` all delegate — so the wire form a consumer sees
(`NIKA-VAR-001` · `-002` · `-004` · `-005` · `-006`) is byte-identical.
`RuntimeError::from_cel` still exists and delegates.

The delegation tests stay in `nika-runtime`, rewritten to construct
through the wrapper, because the delegation IS the risk the descent
introduces. Moving them to the new crate would have tested the enum and
left the seam unproven.

### Layer

**L1**, mirroring `nika-secret` (the previous descent from this crate).
L0 caps a crate at three sibling `nika-*` dependencies (ADR-027) and this
one needs five — `nika-cel` · `nika-tmpl` · `nika-schema` · `nika-types` ·
`nika-cap` (+ `nika-error` for the code registry). Taking an exemption to
sit at L0 would buy nothing: `nika-runtime` is L3 and depends downward on
L1 without ceremony.

## Consequences

- `nika-runtime` **14,787 → 13,699** prod LOC. The doc comments #1171
  compressed can be restored; they were paying rent for a wall that is
  not theirs.
- One more workspace member, under the ADR-037 horizon (50-90 · cap 100 ·
  projected, never a gate · D-2026-07-21-N1).
- `Scope::workflow` and `Scope::workflow_with_secrets` — the
  empty-namespace TEST constructors — become reachable across the crate
  boundary and are therefore gated behind a `testing` feature that
  `nika-runtime` enables in `dev-dependencies` only. Plain `pub` would
  have blessed a constructor that silently turns a real `secrets.X` into a
  `NIKA-1702`; `#[cfg(test)]` would have hidden it from the sibling tests
  that need it.
- The next crate at the wall is **`nika-check` at 14,697** (corrected
  counter). It descended once already, on 2026-07-21. This ADR does not
  decide that move; it names it so the next reader does not rediscover it
  at merge time.

## Alternatives considered

- **Compress more prose.** Rejected: it is the defect, not the fix.
  #1171 had already done it once and still did not fit.
- **Raise the cap.** Rejected without a rule change. The cap is locked in
  `nika-invariants.md`; re-ruling it is a separate, deliberate act with
  its reason written down — not something a blocked PR does in passing.
- **Descend approval / resume instead.** Rejected on the measurement: ten
  or more intra-crate edges, and it needs the event lane. It would have
  dragged half the crate or created a cycle.
- **Fix only the counter.** Rejected as the whole answer, though it was
  done alongside. Corrected, the crate is still at 98.6% of the cap.
