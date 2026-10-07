# Crate spec — `nika-compile-seats`

| | |
|---|---|
| Status | **MEMBER** (size-cap split of `nika-compile-cognition`, itself a member of the admitted `nika-onboard` unit · ADR-146 · D-2026-07-09-N1 · 2026-10-07) |
| Layer | L4 — a library surface; lateral L4→L4 edges `nika-compile-cognition → nika-compile-seats → nika-compile`, never back |
| Design | the two capabilities a host lends a preparation of the Compile core: the bounded decision seats (`decide`) and the rehearsal port (`rehearse`), with the reasoning record their calls share (`reasoning`); kept under their historical paths `nika_compile_cognition::{decide, rehearse}` |
| IMPL | measured by `scripts/crate-metrics.sh nika-compile-seats` at each freeze; the crate carries what `nika-compile-cognition` held in `decide.rs`, `rehearse.rs`, `rehearse/{judged, observed}.rs` and the two reasoning helpers of `cognition/receipt.rs` on 2026-10-07, with their tests (the gate's own counter: 1,523 prod LOC at the split) |
| LOC budget | ≤15k crate · ≤1500/file · ≤100/fn |
| Crate version | tracks workspace |
| License | `AGPL-3.0-or-later` |
| Edition | 2024 (workspace-inherited) |
| Publish | `false` — member of the `nika-onboard` unit |
| Dependencies | **read from `Cargo.toml`, which is authoritative** · `nika-compile` (`AuthoringReasoning`) · `nika-kernel` (the provider seam) · `nika-compile-fidelity` (`behavior`, the judge's run) · `serde_json`, `thiserror`, `tokio` |
| NIKA codes | none minted here — `DecisionError` is a seat's failure the doors record and fall back from; a rehearsal states its refusal in its report |

## 1. Purpose

`nika-compile-cognition` stood at 16,632 prod LOC on 2026-10-07 (16,346 once the clause readings
ascended to `nika-compile-clauses`, ADR-145), against the 15,000 wall. Its `decide` and
`rehearse` modules are the two capabilities a host lends a preparation; the seats' doors read
them, and they read nothing of the doors but the two reasoning helpers the decision call shares.
Per D-2026-07-09-N1 a size-cap split is ONE architectural unit in several workspace members: they
descend here, below the doors, which keep them at their historical paths and measure 14,887
(ADR-146). `crates/nika-compile-cognition/tests/seats_reexport.rs` compiles against those paths
as an external consumer.

## 2. Surface

- `decide` (the WARM strategy) — `ChoiceQuestion` (options plus NONE), `ChoiceOption`,
  `ChoiceAnswer`, `DecisionError`, the object-safe `DecisionSeat` and `ProviderChoice` (a
  generative provider constrained to a closed enum, one physical call, no retry). The seams the
  doors read across the boundary are public: `closed_choice` (the two messages and the answer
  schema), `answer_text` (the sole final Text of a completed answer), `decoded` (the option an
  answer chooses), `admit` (the answer revalidated against its question) and `record` (the
  provenance projection of one settled question).
- `rehearse` — the port a host answers (`Rehearse`, `RehearsalFuture`), the report
  (`RehearsalReport`, `Rehearsal`, `Attempt`, `RoomEvidence`, `EffectCounts`, `RehearsedOutput`),
  the host's observation of the copied world (`observed`: copies, final states, the ledger, the
  bounds, the refusal, the failure) and `judged_run`, which maps a report to the behavioural
  judge's run (`targets_of` names the results a contract reads back).
- `reasoning` — `effort` (the provider level an authoring level names) and `reasoning_record`
  (one call's reasoning, each fact apart), shared with every authoring call of the doors.

## 3. Boundary

- Neither capability grants authority: a seat answers one closed choice the compiler
  revalidates, a room runs a candidate where no effect escapes; the doors decide what an answer
  or a report means for a candidate.
- The rehearsal's enums stay `#[non_exhaustive]`; across the member boundary the doors match them
  with a wildcard arm that records an unknown kind as `unknown` and stops on an unknown outcome,
  never proceeds.
- This crate never depends on `nika-compile-cognition`.

## 4. Related

- ADR-146 (this split) · ADR-140 (the seats' doors) · ADR-145 (the clause readings) · ADR-144
  (the re-export precedent) · D-2026-07-09-N1
- `docs/crate-specs/nika-compile-cognition.md` · the doors, the owner of the orchestration
