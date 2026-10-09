# Crate spec — `nika-compile-behavior`

| | |
|---|---|
| Status | **MEMBER** (size-cap split of `nika-compile-fidelity`, itself a member of the admitted `nika-onboard` unit · ADR-149 · D-2026-07-09-N1 · 2026-10-09) · it inherits the admission of its unit (the ADR-141, ADR-145 and ADR-146 posture); its gates are measured in §6 for this crate, none inherited as a pass, and mutation stays pending evidence, never claimed |
| Layer | L4 — a library surface; lateral L4→L4 edges `nika-compile-fidelity → nika-compile-behavior → nika-compile-reader`, never back |
| Design | the behavioural contract a request states, independent of any candidate, and its typed judgment over what a round of rehearsals consumed and wrote (`behavior`), with the date-time shape classifier the judge and the candidate laws both read (`instant_shape`); kept at their historical paths `nika_compile_fidelity::behavior` and `nika_compile_fidelity::fidelity::instant_shape` |
| IMPL | measured by `scripts/crate-metrics.sh nika-compile-behavior` at each freeze; the crate carries what `nika-compile-fidelity` held in `behavior.rs`, `behavior/` and the classifier of `fidelity/instants.rs` on 2026-10-09, with their tests (6,962 prod LOC at the split, `scripts/ci/prod-loc.py` over the gate's file set: the moved 6,866, the classifier module's 61 and the crate root's 35 · 110 unit tests moved with their 13 files and the classifier's 2) |
| LOC budget | ≤15k crate · ≤1500/file · ≤100/fn |
| Crate version | tracks workspace |
| License | `AGPL-3.0-or-later` |
| Edition | 2024 (workspace-inherited) |
| Publish | `false` — member of the `nika-onboard` unit |
| Dependencies | **read from `Cargo.toml`, which is authoritative** · `nika-compile-reader` (the plan, its rules and aggregates, the shape and structure laws, the HOT door, the gates, the paths and the lexicon a contract is stated from) · `serde_json` (a rule's JSON record) · `csv` (the reading `nika:convert` uses) · `sha2` (every reading bound to the sha256 of its bytes) · dev: `proptest` |
| NIKA codes | none minted here — a judgment is a typed report (`Report`, `Verdict`, `Choice`); the doors that read it speak through their own outcomes |

## 1. Purpose

`nika-compile-fidelity` measured **15,050 prod LOC** on 2026-10-09 (`55ad41642`, the gate's own
counter: Rust plus its two embedded jq laws), against the 15,000 wall, before the candidate-law
work then under way (the composition endpoint witness and its successor) could land. Its
behavioural contract is a family apart: the contract a request states and the judgment of a
round of rehearsals read the reader, `serde_json`, `csv` and `sha2`, and nothing of the laws, the
sketch or the candidate plan but one scalar classifier, the date-time shape. Per
D-2026-07-09-N1 a size-cap split is ONE architectural unit in several workspace members: the
family descends here, below the candidate laws, which keep it at its historical path and measure
8,145 (ADR-149). Moving the candidate laws back into the reader instead would undo ADR-141.

The moved files keep their bytes but for the two imports of the classifier
(`behavior/values.rs`, `behavior/evaluate.rs`: `crate::fidelity::instant_shape` became
`crate::instant_shape`). No judgment, contract, record, wire, budget or Stop behaviour changes.

## 2. The contract and its judgment (`behavior`)

The surface is unchanged by the descent; the description below moved verbatim from the
fidelity spec, where it was recorded when the family was added (« the member » in its last
sentence now reads as this crate, which declares those two dependencies itself).

- Added: the behavioural contract (`behavior`), pure like the laws. `contract_of` reads the
  reader's plan of the request (its operations, effects with their policies and the value
  written alone, its unknowns, each rule's typed fields), the paths the request names and the
  human's answers; it never reads a candidate, and never the jq a rule lowers to. Each
  obligation names its file, whether the request wants it written and what it must hold: a
  relation of filters, duplicates by key, groups or totals, a sort with or without the tie
  rule, the first N rows, projection, renames and duplicates, in the order the request states
  its steps. A fact the plan does not carry stays open: an automatic write is `Unproven` (no
  plan field states whether a condition governs it), policy words the plan does not type leave
  it `Undecided`, and an aggregate's output name is `Naming::Unknown`; `Required`, `When`,
  `OnlyWhen` and `Naming::{Stated, Free}` are for a lower layer that proves them. `judge` first
  establishes what each rehearsal can show (a contradictory observation, a source missing,
  malformed or outside the stated domain, or a result with no read-back is an invalid harness),
  then classifies its end once: completed, a failure an established cause or the contract
  explains as a defect, or a failure the evidence does not settle. An established cause (an
  engine failure observed on a valid fixture, a file the request never names, a named source the
  host copied whole and the run then lost, a well-formed source the run could not parse) is a
  defect even where a stated rule predicts a stop; a named source with no recorded copy, or only
  a partial one, settles nothing (a fixture that lacked it is the host's to attest with its own
  invalid-harness end). A stop
  is never certified: a structured `StopFact` (source, operation, field, value) consistent with
  the stop a stated rule predicts names no data or policy independent of the workflow, so it
  stays unattested, and one no stated rule predicts is a defect. An error code or message is
  never a stop, and a host time bound is no run. Values compare
  exactly (strict JSON with exact decimal numbers, the `nika:convert` CSV reading, each bound to
  the sha256 of its bytes): `70` and `70.0` agree, rows compare as a multiset or block by block
  where a sort orders them, a cut through tied rows admits any of them unless ties keep file
  order (under a stated number policy a tied-cut stop is admitted too, never certified), and an output
  name the request does not state is matched by its value under a key no other output reserves,
  certified only when proven free; two outputs under one name (a group key and an aggregate, or
  two aggregates) are never reduced to one, the relation stays unverified. A stop never hides a
  wrong value or a forbidden write already observed. Outcomes are not ordered: each obligation and the report carry a `Tally`, and
  `Verdict` states its dominance (invalid harness, defective, not run, incomplete, certified);
  `failed()` keeps the defects valid fixtures showed and `scorable()` says no fixture was
  invalid. The report keeps the requested result, the assumptions and the observed proof apart,
  with the round and turn budget (fixtures, attempts, bytes copied and read back, elapsed time)
  the host reports. A join, rows per group with no named aggregate, outputs defined as
  arithmetic over aggregates, a seat-written program, the lines of a text source and several
  rules or writes the plan does not pair stay explicit unsupported obligations. No runtime
  record yields a `StopFact` yet, and none carries the provenance an attested stop needs: a stop
  stays incomplete. `contract_of_request` (with `read_request`, `Provenance`, `Production`)
  proves more only for a sentence of a small closed language (`read SOURCE`, then `count the
  rows where FIELD is VALUE` or `keep the rows where FIELD is VALUE`, then `write the count|it to
  TARGET`, `write the count as LABEL to TARGET` or `write them to TARGET`, joined by `, `,
  `, and ` or ` and `), matched over the caller's own bytes (only the reader reads its
  apostrophe-folded copy), whose identities are kept byte for byte (one terminal period is
  punctuation, never part of TARGET), whose plan agrees with it byte for byte, that the strict HOT
  door admits and that is the whole request: its write is `Required` and its count's name free
  (no name slot) or the stated label. One more closed production, `copy SOURCE as is to TARGET`
  over a plan of exactly one read of SOURCE and one automatic write of TARGET (two distinct files
  of a text suffix, no rule, no other binding), proves the write `Required` and its content
  `Requirement::CopyText`: exactly the text the run consumed from SOURCE, byte for byte. The
  suffix bounds the production and proves no encoding: only the host's complete receipt proves a
  text. A cut or non-text source or a cut result is incomplete, a result not published by the
  run or holding other text fails, a missing source receipt on an attempt is an invalid harness,
  and only a completed run passes. No word list is consulted; every other request, an identity
  folding would alter included, keeps `Unproven` and `Unknown`. `select` judges several
  candidates (`Candidate`: the identity of the bytes rehearsed and one run per world) against one
  such contract, each as one round of the same turn's budget, and selects the first one certified
  (`Choice::Selected`); otherwise every candidate defective is `RejectAll`, a spent turn with
  candidates left is `Spent`, and anything else is `Unproven`: no absence of defect selects.
  `select` runs nothing and verifies no identity: its turn counts only the candidates judged,
  so a door rehearses and judges one candidate at a time, carries the turn, and stops at the
  first one selected or at a spent turn; `Selected(k)` is an index into the slice given, which
  the door keeps bound to the bytes rehearsed. `targets` names, as an iterator, the paths a host
  reads back for a contract. The member gains two workspace dependencies already in the lock:
  `csv` (the reading `nika:convert` uses) and `sha2`.

## 3. The date-time shape (`instant_shape`)

`instant_shape(text)` returns the form of an ISO-8601 date-time (every digit masked as `9`) and
its offset (`Z`, `+02:00`, `+0200`, `+02`, or empty), or `None` for any other text, a bare date
included. Text order is time order only between date-times that share both. It is lexical: no
calendar, zone or instant is validated, normalized or parsed. The judge reads it before it lets a
sort or an order of values stand for time order (`behavior/evaluate`, `behavior/values`); the
candidate laws read the same function for Law 25 (`TEXT ORDER ON INSTANTS`) and the masked
temporal observation, at `nika_compile_fidelity::fidelity::instant_shape`, a `pub use` of this
one. Law 25 itself (the orders a `nika:jq` expression makes, the evidence it reads, its
diagnostic), `Diagnostic` and the record-scope walk stay in `nika-compile-fidelity`. The function
and its two unit tests moved byte for byte.

## 4. The boundary, measured

Edges on the tree of `55ad41642`, production code only (the shared `rs_prod_files` set):

| direction | edges |
|---|---|
| this crate → the rest of `nika-compile-fidelity` | **0** — the two uses of `fidelity::instant_shape` (`behavior/values.rs`, `behavior/evaluate.rs`) read the classifier, whose definition moved here |
| this crate → the reader | `plan::{Effect, EffectPolicy, EffectVerb, Op, Plan}` · `rules::{Rule, Comparator, Junction, NumberPolicy, keeps_order, synthesize}` · `aggregate::{AggOp, ArithOp}` · `hot::{stated_sources, rejections}` · `gates::backstop` · `lexicon::{read, fold_apostrophes}` · `paths::single_file` · `shape::{prohibits, promote_stated_rules}` · `structure::{only_function_words, binds_no_operation}` |
| `nika-compile-fidelity` → this crate | `behavior`, re-exported whole (`#[doc(inline)] pub use nika_compile_behavior::behavior`), read by `sketch/record.rs` (`Contract`, `Presence`, `Requirement`, `contract_of_request`: the request basis and its partial contract projection) · `instant_shape` (Law 25, and `observed/temporal.rs` through the fidelity path) |
| outside the unit → this crate | through `nika_compile_fidelity::behavior` only: `nika-compile` (`assemble/read`), `nika-compile-cognition` (`rehearsal`, `sketch/evidence`), `nika-compile-seats` (`rehearse/*`), `nika-onboard` (`compile/copy` and its children) |

The contract projection matched `Presence` exhaustively inside its defining crate. Across the
boundary the `#[non_exhaustive]` enum takes a wildcard arm that records `unknown`, never a kind
it knows (never `required`): the seven known kinds keep their words, pinned by
`sketch::record::caller_tests::the_projection_keeps_the_word_of_every_known_presence`. A type's
run-time name (`std::any::type_name`) and the API snapshots now name this crate; derived `Debug`
output and every record carry no path.

## 5. Module map

`behavior` (the contract and the report types) · `behavior/requested` (the contract a plan
states) · `behavior/provenance` (what a sentence of the closed language proves) ·
`behavior/composed` (a computation the reader states in pieces) · `behavior/pipeline` (the
relation's ordered steps) · `behavior/evaluate` (the relation over the records a run consumed) ·
`behavior/verdicts` (the judgment; `verdicts/copied`, the copy relation) · `behavior/selection`
(the choice among candidates) · `behavior/{numbers, values, formats}` (exact numbers, value
comparison and the canonical readings) · `behavior/accounting` (the round and turn budget) ·
`instants` (the date-time shape, private, `instant_shape` at the crate root).

## 6. Admission evidence (ADR-003, measured 2026-10-09)

The member inherits the admission of its unit (D-2026-07-09-N1, the ADR-141, ADR-145 and ADR-146
posture). Every gate below was measured for this crate at its split, on macOS with the
checkout's toolchain (1.91), a private target, `--locked --offline` and no signing key or
keychain; none is inherited as a pass. Mutation (Gate 5) stays pending evidence, tracked in
ADR-149's follow-ups, never claimed.

| Gate | Verdict | Evidence |
|---|---|---|
| 1 SPEC | ✅ | this document |
| 2 TDD | ✅ for the one new behaviour; N/A for the moved code | the wildcard arm of the contract projection was forced by a red witness: without it `nika-compile-fidelity` fails with E0004 (`&_` not covered, `sketch/record.rs:297`) once `Presence` is foreign, and passes with it. The moved code is no new algorithm: it keeps its own 110 + 2 tests, with the same names and outcomes before and after (Gate 10) |
| 3 IMPL | ✅ | `cargo test -p nika-compile-behavior -p nika-compile-fidelity`: 270 passed, 0 failed (this crate's lib 112, `tests/properties.rs` 7, fidelity's lib 108 and its integration suites 43, `behavior_contract` 3 and `behavior_reexport` 2 included); the facade's consumers, `cargo test --lib`: `nika-compile` 130, `nika-compile-cognition` 280, `nika-compile-seats` 169 (1 ignored), `nika-onboard` 296 (5 ignored), all passed |
| 4 CLIPPY | ✅ | `cargo clippy --all-targets -- -D warnings`, exit 0, for this crate and fidelity, and for `nika-compile`, `nika-compile-seats`, `nika-compile-cognition` and `nika-onboard` |
| 5 MUTATION | ⏳ pending | `cargo mutants --list -p nika-compile-behavior` lists 1,271 mutants (numbers 200, verdicts 188, evaluate 168, provenance 139, composed 123, formats 117, requested 102, `behavior.rs` 75, values 46, instants 37, pipeline 29, accounting 27, `verdicts/copied` 12, selection 8); the run (`scripts/ci/check-mutation-floor.sh nika-compile-behavior`) is granted a separate slot, the ADR-141, ADR-145 and ADR-146 member-split posture. No score is claimed |
| 6 PROPERTY | ✅ | `tests/properties.rs`, 7 properties at proptest's default 256 cases each, all passed: an integer's text reads as the integer (order, sum, opposite, magnitude), the plain form reads back as the same number, the spellings of one value name one number, an exact quotient and a rounded integer are the integer; a date-time shape masks exactly a prefix and keeps the rest as its offset over arbitrary text, every admitted spelling is read with its form and offset, and another separator or a bare date has no shape |
| 7 BENCHMARKS | N/A | no hot path: the judge runs once per rehearsal round of a compile door, over records whose bytes the host's budget bounds (`Limits`); the descent changes no code path, and the family carried no benchmark before it |
| 8 DOCS | ✅ ADR-003's check; one gap disclosed | `RUSTDOCFLAGS="-D warnings" cargo doc --no-deps`, public and `--document-private-items`, exit 0 for this crate and fidelity; the public API coverage vector is green with this crate's snapshot. Measured beyond the gate: `cargo rustdoc -p nika-compile-behavior --lib -- -D missing_docs` names 81 public struct fields and enum variants with no doc of their own (each type and function is documented). They moved unchanged from fidelity, where the workspace lint set never asked for them; documenting them is a follow-up, not part of this byte-preserving split |
| 9 CANARY | N/A as a workflow canary | a library member with no workflow surface of its own (no verb, builtin or command); its end-to-end path is the compile door's rehearsal, run in this admission through the fidelity path: `nika-onboard` `observed_room_copy` 5 (the real compiler, real rehearsal rooms, this crate's judgment) and `observed_semantic_repair` 2, `nika-compile` `compile_semantic_repair` 9, all passed |
| 10 PARITY | ✅ | no legacy counterpart: the behavioural contract was written for the Diamond compiler after v0.80. Split parity on the same inputs: the base export (`55ad41642`, plus the projection pin) ran 261 tests; after the split all 261 ran with the same outcome (112 in this crate, 0 missing, 0 changed), and 9 were added. The pin passes before and after, so the seven projection words are unchanged. This crate's API snapshot equals, as an ordered sequence, the base fidelity `behavior` section renamed, minus the 482 blanket-impl rows of crates outside its dependency closure |
| 11 REVIEW | ✅ | three independent read-only perspectives on the settled source, none building. The Rust and compile-correctness lens returned ADMIT with no P0 or P1. It checked the moved bytes, the module paths, the `#[non_exhaustive]` boundary, the lints and the lock, and verified by hand that all seven properties hold. The architecture and public-API lens returned ADMIT with no P0 or P1 (direction, no cycle, an exact facade, the snapshots, the size caps). The Nika-contracts lens returned two P1s, both resolved before the commit: the inheritance wording now states the ADR-146 posture, and ADR-149's status is decided (accepted under the decider's standing mandate) |
| 12 ATOMIC | ✅ | the admission commit, this crate with its manifest, lock entry, registries, ADR and the fidelity facade |

## 7. Related

- ADR-149 (this split) · ADR-141 (the candidate laws, where the family was added) · ADR-146 (the
  re-export precedent) · ADR-137, ADR-138, ADR-145 · D-2026-07-09-N1
- `docs/crate-specs/nika-compile-fidelity.md` · the candidate laws, which keep this crate at its
  historical paths
