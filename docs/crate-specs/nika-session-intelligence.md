# Crate spec — `nika-session-intelligence`

| | |
|---|---|
| Status | **WIP · MEMBER** (size-cap split of `nika-session`, a WIP crate itself · ADR-150 · D-2026-07-09-N1 · 2026-10-09) · it joins the workspace as WIP with its unit, the `nika-session-change` precedent |
| Layer | L4 — a library surface; lateral L4→L4 edge `nika-session → nika-session-intelligence`, never back · its own lateral edges reach `nika-onboard` (the typed Compile surface, the conversation grammar, the pinned knowledge), `nika-cli-host` (the probe, the observed project, the authoring backend and settings, the TypeSafe decision seat · default features off), `nika-compile-cognition`, `nika-compile-seats`, `nika-display` (`front_door::DataLocus`) and `nika-session-change` (`change::Witness`), none of which depends on it |
| Design | the intelligence a session reasons, routes and authors with: four modules, kept under their historical paths `nika_session::{authoring, intelligence, reasoner, turn}` |
| IMPL | measured by `scripts/crate-metrics.sh nika-session-intelligence` at each freeze; the crate carries what `nika-session` held in `intelligence.rs`, `reasoner.rs`, `turn.rs` and `authoring.rs` with its `context`, `decision` and `harness` modules on 2026-10-09 (the gate's own counter: 3,110 prod LOC at the split, 2,885 of them moved · 61 unit tests, all moved with their files) |
| LOC budget | ≤15k crate · ≤1500/file · ≤100/fn |
| Crate version | tracks workspace |
| License | `AGPL-3.0-or-later` |
| Edition | 2024 (workspace-inherited) |
| Publish | `false` — member of the `nika-session` unit |
| Dependencies | **read from `Cargo.toml`, which is authoritative** · lateral L4 `nika-onboard`, `nika-cli-host` (default features off), `nika-compile-cognition`, `nika-compile-seats`, `nika-display`, `nika-session-change` · `nika-runtime` (`compose::config_from_env`, the ONE env boundary) · `nika-providers`, `nika-verb-infer`, `nika-http`, `nika-kernel`, `nika-types` · `nika-harness` (optional, `access-harness`) · `blake3`, `serde`, `serde_json`, `thiserror`, `tokio` · dev: `nika-catalog`, `nika-error`, `nika-event`, `tempfile` |
| Features | `access-harness` (default; the harness reasoner and the subscription authoring seat, forwarded by `nika-session`'s own) · `test-support` (never default; the provider transport's test substitution, which only the session's loopback suites enable through their dev-dependency) |
| NIKA codes | none owed — `ReasonError` (a reasoner that could not answer), `AuthoringError` (the authoring door's machinery) and `AuthoringContextError` (an authoring configuration that cannot be honored, wrapped by `AuthoringError::Context`) are spoken by the session as refusals with their fix; their exemptions sit in `scripts/ci/error-one-voice-allowlist.tsv` |

## 1. Purpose

`nika-session` stood at 14,968 prod LOC on 2026-10-09, against the 15,000 wall, with the next
Session work (grouped and multiple-choice answers, a paused clarification, role-scoped
intelligence settings) still to land. Its `intelligence`, `reasoner`, `turn` and `authoring`
modules formed a closed cluster below the runtime: they named nothing else in the session, while
the runtime reached them downward. Per D-2026-07-09-N1 a size-cap split is ONE architectural unit
in several workspace members: the cluster descends here, below the session, which keeps 12,091
prod LOC (ADR-150).

The session re-exports the four modules at their historical paths and keeps its root re-exports
(`AuthoringRound`, `IntelligenceCensus`, `ReasonError`, `ScriptedReasoner`, …), so
`nika-session-host`, `nika-cli`, `nika-tui` and `nika-serve` compile unchanged;
`crates/nika-session/tests/intelligence_reexport.rs` compiles against those paths as an external
consumer.

## 2. The four modules

- `intelligence` — which reasoning path the human chose (an AI app they already have · an API ·
  a local engine · none), persisted at `~/.nika/session-intelligence.json`; the deterministic
  census of what this machine can serve now (presence only, read from the ONE probe); and the
  resolution that refuses a choice this machine cannot serve, with its fix, never replaces it.
  Each path names where the project context goes (the data locus).
- `reasoner` — ONE inference over the selected intelligence, never a temporary workflow: a
  harness seat through the same infer-grade adapter `nika run` uses, an API or a local engine
  through the provider registry and the one-shot infer verb, a scripted stand-in, or none. The
  provider plane's HTTP client (SSRF off for the fixed provider profiles, the transport ceiling
  raised) and the providers configuration read at the ONE env boundary are shared with the
  provider authoring seat. A path that cannot carry an explicit reasoning effort refuses it
  before any call.
- `turn` — the semantic act of one free line: a bounded routing decision (discuss, modify, new
  work, answer, run, cancel, mixed, unknown) through the same intelligence, never a lexicon and
  never a consent. The record of a route keeps the line's digest, never the line.
- `authoring` — the session's door to the ONE compiler: the seat the human's choice permits
  (deterministic, a provider, a subscription harness, or unavailable with its reason), the
  authoring context pinned when the session opened (the strategy, the pinned knowledge, the
  decision seat, the explicit reasoning effort), and the round (`AuthoringRound`: the answers by
  stable key, the replayed plan, the questions still open, a revision's base and the admitted
  monetary directives). The context keeps the settings it was resolved from, read once:
  `AuthoringContext::knowledge_named` says what a layer named before admission (a source the
  strict door refused stays named), and `with_embedded_knowledge` resolves the same settings with
  the release this build embeds named on the explicit layer, keeping the strategy, the effort, a
  held-out corpus, the decision seat, the project root and the source word; the session applies
  it only on the conversation's explicit `/knowledge embedded`.

## 3. Boundary

- The member owns no conversation, runtime, history, money gate or durable round: the session
  decides when a line is routed, when a round compiles, what it records and what it keeps.
- The seams the session reads across the boundary are public items of the member:
  `AuthoringSeat::has_model`, `AuthoringRound::{edit, target, money}`,
  `AuthoringRound::{replays, forget_plan, retain_held, compile_rehearsed, asks, restate_clause,
  replacement}`, `authoring::compile_in_rehearsed`, `AuthoringContext::reasoning_asked`,
  `intelligence::now_rfc3339`, `IntelligenceCensus::provider_context` and
  `reasoner::{provider_config, block_on}`. They were crate-private inside the session; the crate
  boundary makes them public. The source-recovery count is read through the accessor
  `AuthoringContext::recovery()`; its field stays crate-private. `AuthoringRound` is
  `#[non_exhaustive]`, so it stays as unconstructible outside the member as its crate-private
  fields made it.
- The structs a host builds have their INV-019 constructors: `ResolvedSessionIntelligence::new`,
  `IntelligenceCensus::new`, `SeatSeen::new`, `Reply::new` and `TurnContext::new`.
- The public enums stay `#[non_exhaustive]`; across the member boundary the session matches them
  with wildcard arms that decide nothing new (ADR-150).
- `reasoner::test_transport` exists only in tests and under `test-support`: the provider client
  of this thread sends to one loopback peer under a test key. The session's loopback suites
  install it; no production build carries it. A workspace-wide test or lint build unifies the
  feature into the binaries it builds, where the hook stays inert: nothing installs it.
- This crate never depends on `nika-session`.

## 4. Tests

The library tests moved with their files: the census, the choice and the resolution, the
reasoners (the label suite drives the real verb and registry over the loopback test seam), the
router, the authoring door and its context (the explicit reasoning effort on the wire). The two
loopback suites use a `cfg(test)` copy of the session's loopback peer (`reasoner/wire.rs`),
ADR-150 records why. The decision seat's suite stays with the session, whose runtime suites
script its System One peer.

## 5. Admission evidence

The member inherits the admission of its unit, as `nika-session-change` does. Gate 5 (mutation)
and Gate 6 (properties) are pending, tracked with the unit's evidence, never claimed. The Linux
public API job confirms the snapshot, which was written from a macOS render.

## 6. Related

- ADR-150 (this split) · ADR-125 (the native session) · ADR-133 (the portable session machine) ·
  ADR-144 (the change set's member, the re-export precedent) · D-2026-07-09-N1
- `docs/crate-specs/nika-session.md` · the session, the owner of the conversation
