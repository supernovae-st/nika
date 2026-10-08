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
| Dependencies | **read from `Cargo.toml`, which is authoritative** · `nika-compile` (`AuthoringReasoning`) · `nika-kernel` (the provider seam) · `nika-pack` (the skeletons and stdlib page of the shelf) · `nika-compile-fidelity` (`behavior`, the judge's run) · `serde`, `serde_json`, `thiserror`, `tokio` |
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
  provenance projection of one settled question). Independent questions are asked together
  (A1): `ChoiceBatch` holds them (`ChoiceBatch::of` keeps, as the batch's, the longest common
  prefix of their instructions up to its last paragraph break and the state entries they all
  hold alike; each `BatchItem` keeps the question as asked alone, what it asks beyond the shared
  words and what it adds to the shared state). `DecisionSeat::choose_each` answers a batch,
  one answer per item in item order: by default each item asked alone, all in flight together
  (`each_alone`: one physical request per question, no retry, a failure failing only its own
  item); `ProviderChoice` settles a batch in ONE request (`closed_choices`: the shared words and
  state once, an answer schema keyed by item id; `decoded_each`: each item's key bound by its
  id, an item left without one of its keys failing alone, a failed request failing every item,
  the request's usage and reasoning riding the first item's answer only).
- `compose` — the candidate composer (ADR-147): the distinct admissible
  COLD plans in first-seen order, each judged by the deterministic `feasibility` filter against
  the reading's floor and the request before any seat sees it, the topology dimensions recalled
  candidates suggest (`Dimension`), the structural `signature`, the seatless `rank`, `describe`
  and `classify_disagreement`; nothing calls a provider or grants authority.
- `rehearse` — the port a host answers (`Rehearse`, `RehearsalFuture`), the report
  (`RehearsalReport`, `Rehearsal`, `Attempt`, `RoomEvidence`, `EffectCounts`, `RehearsedOutput`),
  the host's observation of the copied world (`observed`: copies, final states, the ledger, the
  bounds, the refusal, the failure) and `judged_run`, which maps a report to the behavioural
  judge's run (`targets_of` names the results a contract reads back). A completed run as a
  verifier is shown it (`shown`, descended from the cognition's verifier at its size cap):
  `trial_shown` (the candidate's sha256, each input read and output read back with its text,
  `read_whole` and `written`), `trial_whole` (at least one output, each written by the run, every
  text read whole) and `trial_receipts` (each text's role, path, size and sha256, never the text).
  A host may also offer a read-only composition check, `Rehearse::compose`. It answers
  `Composed`: `Unoffered` (the default), `Unresolved` or `Refused` with the reason, or a clean
  `Closure`. The closure binds the candidate's sha256, the project-relative path it was checked at,
  the snapshot identity and format, and every captured unit's digest. `composed` asks the host
  about a candidate that holds a child workflow source-only (`held_children`, which reads the
  core's `UNJUDGED_DEPENDENCY` finding on a parsed `invoke.workflow` task) and records the answer
  on `decision.composition`. `discharge_children` lifts that hold only for a clean closure of
  these exact bytes and then settles READY by the core's own law (`ready_by_law`). An MCP or
  skill hold, a mandatory question or a refusal stays as it is. `arguments::evaluated` is the
  law a host's room screen applies last, held by `nika-onboard` until its size cap (2026-10-08):
  in every task (its bindings, condition, fan-out collection, arguments and recovery value),
  the outputs and the model, it refuses a value the run would build before the room's write
  budget sees it (two template islands or more in one string, a CEL list holding a value
  reference, an array or an object holding a template that reads a value, or a template the
  scanner cannot read), in the words of its field; the screen states each as a data bound.
  `record::report` and `record::usage` state one report (with the decision a preparation took
  on it) and a preparation's rehearsal spend as data in `decision.rehearsal`, held by
  `nika-compile-cognition` until its size cap (2026-10-08); no field grants authority.
- `reasoning` — `effort` (the provider level an authoring level names) and `reasoning_record`
  (one call's reasoning, each fact apart), shared with every authoring call of the doors. The
  receipt of an authoring call is stated here too, held by `nika-compile-cognition` until its
  size cap (2026-10-08): `authoring_request` (the bounded JSON-schema request at an output
  limit never above the policy's ceiling, with its explicit reasoning effort, or `None` when
  that effort has no provider level), `context_entry` (a call's role, the digests of its
  instruction and answer schema, the bytes of its messages), `response_identity` (the text
  blocks an answered call returned, by digest and length; `null` for nothing) and `withheld`
  (a refused or ignored payload by digest, length and shape with its reason, never its text).
- `judge` — the untrusted state a judging seat reads, apart from the compiler-owned reference
  every question also carries, held by `nika-compile-cognition` until its size cap
  (2026-10-08): `state` (the request as compiled and as first stated, its answers, the observed
  world and the candidate's bytes; a revision in words is shown its change beside the request of
  the base it revises, as history, never as its first statement) and `over_document` (a revision
  applied over the complete document, by its decision or by the record a round replays, shows
  the base whole). The verifier that asks the questions and weighs the answers stays there.
- `objects` — the JSON objects of a seat's text (`first_json_object`, `answer_objects` and
  `Objects`, `answer_shaped`, `syntax_target`), descended from the doors on 2026-10-07.
- `shelf` — the references an authoring seat reads beside its card (`Reference`, `references`:
  the embedded recall's skeletons and families; `callables` and `builtins_of`: the stdlib
  contracts they name; `rendered`), receipted by digest.
- `foundry` — recalled Foundry knowledge qualified by a decision seat before an author reads it
  (R3 · A1): `question` (one closed choice per reference, `applies` · `unrelated` · NONE),
  `verdict`, `qualify` (all references asked together; the unrelated leave the pack, the
  unqualified stay as hypotheses), `qualified` (the embedded recall folded into the attached
  pack, then qualified; no seat: shown unqualified and said so), `trace` (which shown code
  lines a candidate kept, a lexical trace, never causal proof) and `traced` (the record on
  the outcome, `decision.knowledge_qualification`). `foundry::document` applies the
  operations a revision states over a complete base no semantic record binds (document edits
  made by the `nika-schema` document editor, each re-read and byte-proven: `set`, `insert`,
  `insert_text`, `push`, `remove`, `rename`; `compose` an admitted component, `rebind` one
  through its receipt; or a whole `replace` that claims no preservation) and states their
  record, whose preservation claim says what each operation proved; the revision door that
  asks for them and judges the result stays in `nika-compile-cognition`.

`remote` is what a door that holds no project admits from its caller's
engine: `admit_observation` (the host observer's document only, rows about
paths the request states or files directly inside a stated folder,
`OBSERVATION_ROWS` 64 and `OBSERVATION_BYTES` 256 KiB at most),
`admit_trial` (`{files: [{path, text}]}`, only files the observation marks
`observed`, `TRIAL_BYTES` 1 MiB in all), `Observed::admit` (both, from JSON
texts, a repeated key at any depth refused by `repeats_a_key`) and `Refusal`
with its door code and words. Pure: no file, no network.

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
