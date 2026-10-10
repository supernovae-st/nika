---
id: ADR-150
title: "nika-session size-cap member split: the intelligence, its reasoners, the line router and the authoring door descend to nika-session-intelligence"
status: accepted
date: "2026-10-09"
phase: "pre-1.0 · native session"
deciders: ["@ThibautMelen"]
tags: ["architecture", "crates", "split", "size-cap", "session"]
affects_crates: ["nika-session", "nika-session-intelligence"]
affects_layers: ["L4"]
supersedes: []
superseded_by: []
related: ["ADR-003", "ADR-125", "ADR-133", "ADR-141", "ADR-144", "ADR-145", "ADR-146", "ADR-148"]
requires: []
enables: []
amends: []
fci: []
inv: ["INV-019"]
shadow_zones: []
nika_codes: []
timeline: "v0.123"
follow_ups: ["mutation and property attestations of the member, tracked with the unit's pending evidence", "the Linux public API job confirms the member's snapshot and the member paths in the session and nika-tui snapshots, written from a macOS render", "one shared test-support seam for the loopback peer the session's suites and the member's suites each carry a copy of"]
---

# ADR-150: nika-session size-cap member split — nika-session-intelligence

## Context

`nika-session` (ADR-125) measures **14,968 prod LOC** at `189a01857` (2026-10-09), against the
15,000 wall (`scripts/ci/check-crate-size.sh`, the gate's own counter). ADR-144 bought 1,484 lines
four days earlier; they are spent. The Session work queued next lands where the conversation
reads and authors: grouped answers to a compile round, a real multiple choice, a clarification
paused and resumed apart from a cancel, and typed, role-scoped intelligence settings (the author,
the decision seat, the routing classifier and the run). None of it fits in 32 lines. The cap is
a locked maintainability budget, not advisory.

Four modules stand below the runtime as one cluster: `intelligence` (which reasoning path the
human chose, the census of what this machine can serve, the resolution that refuses a choice it
cannot serve), `reasoner` (one inference over that choice and the provider plane's transport),
`turn` (the bounded router of a free line through the same reasoner) and `authoring` with its
`context`, `decision` and `harness` modules (the door to the ONE compiler: the seat the choice
permits, the authoring context pinned at open, and the round, `AuthoringRound`). Together they
hold 2,885 prod LOC. They name nothing else in the session; the runtime reaches them downward.

## Decision

Per **D-2026-07-09-N1** (a size-cap split is ONE architectural unit in several workspace
members, the ADR-110, ADR-137, ADR-138, ADR-140 to ADR-146 precedents), the four modules descend
from `nika-session` to a new L4 member crate `nika-session-intelligence`, placed BELOW the
session: `nika-session → nika-session-intelligence`, never back.

- `crates/nika-session/src/{intelligence.rs, reasoner.rs, turn.rs, authoring.rs}` move to
  `crates/nika-session-intelligence/src/` (`git mv`) with `reasoner/{test_transport.rs,
  label_tests.rs}` and `authoring/{context.rs, context/tests.rs, decision.rs, harness.rs,
  money_restatement_tests.rs}`. The files keep their bytes except the paths, the visibilities
  and the test seam's `cfg` the move changes, and the INV-019 constructors below.
- The session keeps every public path: `#[doc(inline)] pub use
  nika_session_intelligence::{authoring, intelligence, reasoner, turn};` replaces the four
  `pub mod` lines, and its root re-exports (`AuthoringRound`, `IntelligenceCensus`,
  `ReasonError`, `ScriptedReasoner`, …) are unchanged. `nika-session-host`, `nika-cli`,
  `nika-tui` and `nika-serve` compile without an edit; `crates/nika-session/tests/
  intelligence_reexport.rs` compiles against the session paths as an external consumer.
- The member has no root re-exports: its own code names `crate::reasoner::SessionReasoner`.

### Why this boundary, measured

Edges on the tree of `189a01857`, production code only (the shared `rs_prod_files` set, test
items stripped):

| direction | edges |
|---|---|
| the four modules → the rest of `nika-session` | **0** module edges · one alias, resolved by the move: `crate::change::Witness` (the session's re-export of `nika_session_change::change`) → `nika_session_change::change::Witness` |
| the rest of `nika-session` → the four modules | 18 production files of the runtime · `authoring::{AuthoringRound, AuthoringSeat, AuthoringContext, AuthoringError, Reading, compile_deterministic, compile_in, gateway_host, …}` · `intelligence::{IntelligenceCensus, IntelligenceKind, ResolvedSessionIntelligence, UserIntelligencePreference, DataLocus}` · `reasoner::{ReasonError, Reply, SessionReasoner}` · `turn::{TurnAct, SessionPhase, RoutingMethod, TurnContext, TurnDecision, TurnClassifier, RouteRecord, ReasonerClassifier}` · and the crate-private items below |
| the four modules → other crates | lateral L4: `nika-onboard` (the typed Compile surface, the conversation grammar, the pinned knowledge), `nika-cli-host` (the probe, the observed project, the authoring backend, the authoring settings, the TypeSafe decision seat · default features off), `nika-compile-cognition`, `nika-compile-seats`, `nika-display` (`front_door::DataLocus`), `nika-session-change` (`change::Witness`) · `nika-runtime` (`compose::config_from_env`) · `nika-providers`, `nika-verb-infer`, `nika-http`, `nika-kernel`, `nika-types`, `nika-harness` (optional) · `blake3`, `serde`, `serde_json`, `thiserror`, `tokio`; none of them depends on the session or the member |
| outside the unit → the four modules | through `nika_session::` paths only: `nika-tui`, `nika-session-host`, `nika-cli`, `nika-serve` |

`intelligence` and `authoring` read each other (the census's key line names a gateway host
through `authoring::gateway_host`; the seat is built from the resolved intelligence), and the
authoring door reads the reasoner's transport: none of the three can descend alone. The
runtime's own round, question, answer, protocol and durability pieces cannot stand below it at
all: 7,433 of the session's 14,968 lines are `impl SessionRuntime` blocks.

The crate-private items the session's runtime read become public items of the member, the crate
boundary leaving no narrower visibility, each documented: `AuthoringSeat::has_model`,
`AuthoringRound::{edit, target, money}` and `AuthoringRound::{replays, forget_plan, retain_held,
compile_rehearsed, asks, restate_clause, replacement}`, `authoring::compile_in_rehearsed`,
`AuthoringContext::reasoning_asked`, `intelligence::now_rfc3339`,
`IntelligenceCensus::provider_context`, `reasoner::{provider_config, block_on}`. The
crate-private source-recovery count the unknown-cost review reserves is read through a new
accessor, `AuthoringContext::recovery()`, the field itself staying crate-private.
`AuthoringRound` takes `#[non_exhaustive]`: its crate-private fields made it unconstructible
outside its crate, and with every field public the attribute keeps that law exactly. No
consumer outside the unit names a widened item.

### The provider transport's test seam

The reasoner's provider transport carries a test substitution (`reasoner::test_transport`): under
a test the provider client is wrapped so a loopback peer answers the canned mechanics and the
configuration holds a test key. Thirty-three of the session's test files install it. A
`cfg(test)` of the member is not set when the session's tests build it, so the seam is compiled
under `cfg(any(test, feature = "test-support"))`: the member's non-default `test-support`
feature, enabled only by the session's dev-dependency (the `nika-onboard` and `nika-arm`
precedent). Production builds never enable it and are unchanged; under it, `install`,
`set_config` and `Installed` are public. The default public API does not carry them. A
workspace-wide test or lint build unifies the feature into every binary it builds; the hook
stays inert there, since nothing installs it.

Two suites stay with the code they test because they read its privates: the reasoner's label
suite calls the private `ProviderReasoner::infer` and its reply purpose, and the authoring
context's suite reads its crate-private source-recovery count. Both use the loopback peer of the
session's inference suites (`runtime/inference_tests/wire.rs`). A peer behind `test-support`
would put its `.expect` and `thread::spawn` in production scope, so the member carries a
`cfg(test)` copy of it (`reasoner/wire.rs`, the same bytes but for its judge-approval constant,
now local): 122 duplicated test-only lines, against widening three internals. One shared
test-support seam can remove the copy later.

The decision seat's suite, whose System One peer four runtime suites script, re-homes to the
session as `runtime/inference_tests/authoring_decision.rs`; only its use lines change.

### Exhaustiveness and construction across the boundary

`IntelligenceKind`, `AuthoringSeat`, `AuthoringError`, `ReasonError`, `TurnAct`, `SessionPhase`
and `RoutingMethod` stay `#[non_exhaustive]`. Across the member boundary ten of the runtime's
matches take a wildcard arm that decides nothing new: a seat the session does not know never
authors (it is unavailable, and never « nothing was sent »), a machinery failure it does not know
is refused, an act it does not know at a question binds nothing, a phase it does not know shows
no last prompt and reads the line as no work, and the work snapshot names an unknown intelligence
kind `other` (the contract's own degradation word, as `Question.answer_type` does) and an
unknown seat `unavailable`. Where a wildcard would repeat an existing arm, that arm became the
wildcard; no current variant takes a different path.

The structs the session builds take their INV-019 constructors in the member:
`ResolvedSessionIntelligence::new`, `IntelligenceCensus::new`, `SeatSeen::new`, `Reply::new` and
`TurnContext::new`. Two production literals (a kept choice the history cannot read, the
classifier's context) and 53 test literals call them with identical values; no assertion
changed.

## Consequences

- `nika-session` measures **12,091** prod LOC (headroom 2,909) and `nika-session-intelligence`
  **3,110** (the 2,885 lines that moved, the test seam's file now counted under its feature cfg,
  the crate doc, the constructors, the recovery accessor and the documentation of the widened
  items), the gate's own counter.
- The library tests split: 61 move with their files to the member, 607 stay with the session
  (the 668 of the base), the decision suite among them. The integration suites of the session
  are unchanged; `intelligence_reexport.rs` adds two.
- A type's run-time name (`std::any::type_name`) now names the member; derived `Debug` output,
  the persisted choice (`~/.nika/session-intelligence.json`), the history and the work snapshot
  carry no path and are unchanged.
- The session's manifest forwards `access-harness` to the member's, so `nika-cli`,
  `nika-session-host` and `nika-tui` keep forwarding the session's. It drops the dependencies
  only the member reads (`nika-harness`, `nika-http`, `nika-verb-infer`,
  `nika-compile-cognition`, `thiserror`); `nika-kernel` and `tokio` become dev-dependencies of
  its suites. `Cargo.lock` gains the member's package; no version moves.
- The public API snapshot of `nika-session` loses the four modules' sections, which become
  `pub use` lines; signatures that carry their types name `nika_session_intelligence::…`. The
  member's snapshot carries the moved sections, the widened items, the five constructors and
  `AuthoringRound`'s `#[non_exhaustive]`. The snapshot of `nika-tui` names the member's path in
  the one signature that carries the census and the preference.
- The member depends on `tokio` directly, so it joins the `tokio` wrappers of `deny.toml`; the
  session keeps its row for the dev-dependency of its suites.
- The error one-voice exemptions of `ReasonError`, `AuthoringError` and `AuthoringContextError`
  (`scripts/ci/error-one-voice-allowlist.tsv`) move with the enums to the member; their classes
  and triggers are unchanged.
- The member joins the workspace as WIP with its unit, as `nika-session` is; its admission
  evidence stays pending with the unit (mutation and property attestations tracked, never
  claimed).
- The workspace registries name it: the members, the WIP list, the layer table, the crate spec,
  the public API coverage floor and the generated status blocks.

## Alternatives considered

### Alt A — the observation and record leaves (meaning, snapshot, broker, facts, state, money)

They are closed (about 1,800 prod LOC, `meaning` alone 620 with no edge at all), but they are
what the session is: its grounding, its facts, its state. Together they do not reach the target,
and none of the queued work changes them.

### Alt B — the intelligence alone

`AuthoringSeat::from_reasoner` reads the resolved intelligence and the census's key line reads the
authoring door's gateway host: the member would depend on the session above it.

### Alt C — the runtime's round, question, answer and protocol

They are `impl SessionRuntime` blocks: they cannot leave the crate that defines the runtime.

### Alt D — raise the cap

The 15,000 invariant is a locked maintainability budget, not advisory.

## Related

- ADR-125 (the native session), ADR-133 (the portable session machine), ADR-144 (the change set's
  member, the re-export precedent), ADR-148 (the session over a wire), ADR-141, ADR-145 and
  ADR-146 (the size-cap member precedents), ADR-003 (admission), D-2026-07-09-N1.
