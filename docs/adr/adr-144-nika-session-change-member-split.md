---
id: ADR-144
title: "nika-session size-cap member split: the change set, its review, its outcome and the consent record descend to nika-session-change"
status: accepted
date: "2026-10-06"
phase: "pre-1.0 · native session"
deciders: ["@ThibautMelen"]
tags: ["architecture", "crates", "split", "size-cap", "session"]
affects_crates: ["nika-session", "nika-session-change"]
affects_layers: ["L4"]
supersedes: []
superseded_by: []
related: ["ADR-125", "ADR-126", "ADR-133", "ADR-110", "ADR-137", "ADR-138", "ADR-140", "ADR-141", "ADR-142", "ADR-143"]
requires: []
enables: []
amends: []
fci: []
inv: []
shadow_zones: []
nika_codes: []
timeline: "v0.123"
follow_ups: ["the admission evidence of the member, pending with its WIP unit", "the Linux public API job confirms the zvariant rows of outcome::Incarnation, written in the siblings order from a macOS render that does not show them"]
---

# ADR-144: nika-session size-cap member split — nika-session-change

## Context

`nika-session` (ADR-125) measures **14,998 prod LOC** at the integration head `664b028aa`, against
the 15,000 Diamond wall (`scripts/ci/check-crate-size.sh`, the gate's own counter). Every next
change to the Session needs room inside it. The cap is a locked maintainability budget, not
advisory.

Four modules stand apart. `change` (ADR-126: the typed change set, previewed from the exact bytes
the apply consumes, witnessed against stale targets, landed below the root's descriptor),
`review` (the factual review of a Ready candidate), `outcome` (ADR-133: the proposal, gate and
question identities, the refusal classes) and `consent` (the append-only consent record) name
only each other inside the session (1,489 prod LOC). The rest of the session reaches them
downward.

## Decision

Per **D-2026-07-09-N1** (a size-cap split is ONE architectural unit in several workspace
members, the ADR-110, ADR-137, ADR-138, ADR-140, ADR-141, ADR-142 and ADR-143 precedents), the
four modules descend from `nika-session` to a new L4 member crate `nika-session-change`, placed
BELOW the session: `nika-session → nika-session-change`, never back.

- `crates/nika-session/src/{change,review,outcome,consent}.rs` and the Unix filesystem tests of
  `change` (`change_fs_tests.rs`) move to `crates/nika-session-change/src/` (`git mv`). The
  files keep their bytes except the paths and visibilities the move changes.
- The session keeps every public path: `pub use nika_session_change::{change, consent, outcome,
  review};` replaces the four `pub mod` lines, and its root re-exports (`ProjectChangeSet`,
  `ProposalId`, `ConsentRecord`, …) are unchanged. `nika-tui`, `nika-tui-view` and `nika-cli`
  compile without an edit; `crates/nika-session/tests/change_reexport.rs` compiles against the
  session paths as an external consumer.
- The member has no root re-exports: its own code names `crate::outcome::ProposalId`.

### Why this boundary, measured

Edges on the tree of `664b028aa`, from `use` trees and inline `crate::` paths, production code
only (the shared `rs_prod_files` set, test items stripped by `strip_test_items`):

| direction | edges |
|---|---|
| the four modules → the rest of `nika-session` | **0** module edges · two aliases, resolved by the move: `crate::run_view::{RunFacts::read, live_again}` (the session's private alias of `nika_trace::run_view`) → `nika_trace::run_view`, and `crate::ProposalId::of` (the session's root re-export) → `crate::outcome::ProposalId::of` |
| the rest of `nika-session` → the four modules | 22 production files · `change::{Witness, ProjectChange, ProjectChangeSet, RunRequest, PendingGate, Applied, ChangeError, check_on_disk, compact_hints}` · `review::{propose, render}` · `outcome::{ProposalId, GateId, QuestionId, Refusal, RefusalClass}` · `consent::{CONSENTS_FILE, ConsentDecision, ConsentRecord}` · and twelve crate-private items (below) |
| the four modules → other crates | `nika-cli-host` (`oracle::{audit_source, AuditOptions, Audit}`) · `nika-display` (`check_render::review`) · `nika-onboard` (`compile::{CompileOutcome, DiagnosticKind, TriggerKind, TriggerRequirement, TriggerStatus}`) · `nika-schema` (`parse`, `raw`) · `nika-fs` (`OwnedDir`) · `nika-source` (`is_canonical_program_file_name`) · `nika-trace` (`run_view`, once the alias is resolved: a lateral L4 edge the session already has) · `blake3` · `serde` · `serde_json` · `thiserror` |
| outside the unit → the four modules | through `nika_session::` paths only: `nika-tui` (`change::{ProjectChange, Witness}`, `ProposalId`, `Refusal`, `RefusalClass`), `nika-tui-view` (`review::{plan_lines_in_order, gate_tasks}`), `nika-cli` (tests) |

The twelve crate-private items the session read become public items of the member, the crate
boundary leaving no narrower visibility: `change::{ApplyAttempt, ApplyAttempt::refusal_text,
check_with_access}`, `ProjectChangeSet::{apply_attempt, project_reads}`, `review::{parse,
task_face, propose_over, NOTHING_RAN}` and `outcome::{Incarnation, QuestionId::new,
QuestionId::asked_by}`. No consumer outside the unit names them. A question's identity still
answers only in the session that asked it: `asked_by` compares the incarnation the session holds
by pointer, and no other crate can obtain that `Arc`.

## Consequences

- `nika-session` measures **13,514** prod LOC (headroom 1,486) and `nika-session-change`
  **1,541** (the 1,489 lines that moved, the crate doc and the documentation of the widened
  items), the gate's own counter.
- The library tests of `nika-session` split: 571 stay (its 6 ignored ones unchanged) and 28 move
  with their files under the new crate (`change` 9, its filesystem tests 7, `review` 6,
  `outcome` 3, `consent` 3). The integration suites of `nika-session` are unchanged;
  `change_reexport.rs` adds two.
- `ConsentDecision` stays `#[non_exhaustive]`. Across the member boundary the session's match on
  it (the decision line of a consent) takes a wildcard arm that names nothing the session did not
  decide; the session records only `Applied` and `Partial`.
- A type's run-time name (`std::any::type_name`) now names the member; derived `Debug` output and
  the consent record's serialized lines carry no path and are unchanged.
- `Cargo.lock` gains the member's package and the session's edge to it; no version moves.
- The public API snapshot of `nika-session` loses the four modules' sections, which become
  `pub use` lines, as do the root re-exports of their items; signatures that carry them name
  `nika_session_change::…`. The member's snapshot carries the moved sections under
  `nika_session_change::` (the same rows the session's Linux snapshot held, renamed) and the
  twelve widened items. On Linux, zvariant reaches the member through `nika-cli-host`, and the
  render adds the `NoneValue` rows of `outcome::Incarnation`, its one `Default` type; the
  snapshot carries them in the siblings' order and the Linux job confirms them. The snapshot
  of `nika-tui` names the member's path in the five
  signatures that carry `RunRequest` or `ProposalId`; those of `nika-tui-view` and `nika-cli`
  are unchanged.
- The error one-voice exemption of `ChangeError` (`scripts/ci/error-one-voice-allowlist.tsv`)
  moves with the enum to the member; its class and trigger are unchanged.
- The member joins the workspace as WIP with its unit, as `nika-session` is; its admission
  evidence stays pending with the unit, tracked, never claimed.
- The workspace registries name it: the members, the WIP list, the layer table, the crate spec,
  the public API coverage floor and the generated status blocks.

## Alternatives considered

### Alt A -- the observation cluster (snapshot, facts, broker)

It is also closed (929 prod LOC; its only edges out are the session's aliases of
`nika_onboard::{guard, identity}`), but it is what the session is: the proven root, the engine's
facts and the context bundle. The runtime holds the snapshot as its own state and ten of its
production files name the three modules. It buys about 930 lines against 1,489 and would put
the session's grounding in a library beneath it.

### Alt B -- the intelligence edge (reasoner, turn)

`turn` reads only `reasoner` (923 prod LOC together), but the reasoner carries the
`access-harness` feature in five `cfg` sites and the session's widest fan-out (`nika-providers`,
`nika-verb-infer`, `nika-http`, `nika-harness`, `nika-runtime`, `nika-kernel`): the member would
repeat the feature wiring that `nika-session`, `nika-cli` and `nika-tui` forward, and it buys
fewer lines.

### Alt C -- trim the crate in place

The wall counts comments and blank lines by design; the work queued behind it needs hundreds of
lines at each step, not tens, and the cheapest lines to delete are the doc comments the budget
exists to protect.

### Alt D -- raise the cap

The 15,000 invariant is a locked maintainability budget, not advisory.

## Related

- ADR-125 (the native session), ADR-126 (project changes from the session), ADR-133 (the
  portable session machine), ADR-110, ADR-137, ADR-138, ADR-140, ADR-141, ADR-142 and ADR-143
  (the size-cap member precedents), D-2026-07-09-N1.
