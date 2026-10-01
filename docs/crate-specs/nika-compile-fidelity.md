# Crate spec — `nika-compile-fidelity`

| | |
|---|---|
| Status | **MEMBER** (size-cap split of `nika-compile-reader`, itself a member of the admitted `nika-onboard` unit · ADR-141 · D-2026-07-09-N1 · 2026-09-24) |
| Layer | L4 — a library surface; lateral L4→L4 edges `nika-compile → nika-compile-fidelity → nika-compile-reader`, never back |
| Design | the laws a candidate `.nika` document is judged by, pure over (request · plan · projected document) (`fidelity`), the constrained sketch a seat proposes and the document it states (`sketch`), the plan a candidate document states by its structure and a revision's delta (`candidate`), the behavioural contract a request states and its typed judgment over rehearsals (`behavior`) |
| IMPL | measured by `scripts/crate-metrics.sh nika-compile-fidelity` at each freeze; the crate carries what `nika-compile-reader` held on 2026-09-24 (the gate's own counter: 1,555 prod LOC at the split · 34 unit tests, moved with their files) |
| LOC budget | ≤15k crate · ≤1500/file · ≤100/fn |
| Crate version | tracks workspace |
| License | `AGPL-3.0-or-later` |
| Edition | 2024 |
| Publish | `false` — member of the `nika-onboard` unit |
| NIKA codes | none minted here — a law returns a structured diagnostic; `nika-compile` speaks through `CompileOutcome` |

## 1. Purpose

`nika-compile-reader` reached **15,266 prod LOC against the 15,000 cap** on 2026-09-24, once
the approval laws (Law 3 and Law 3b) and the raw-text-records law (Law 23) joined the fidelity
laws and the multiword file names of the copy family joined the reading. Not every module of
the reader was reading: the fidelity laws judge a candidate document, the sketch is the form a
seat proposes, and the candidate plan is read back from a document's bytes. They sat in the
reader because `nika-compile` stood at its own wall when they were written. Per
D-2026-07-09-N1 a size-cap split is ONE architectural unit in several workspace members: they
ascend here, above the reader and below `nika-compile`, which reads them at its historical
module paths:

```rust
use nika_compile_fidelity::{fidelity, sketch};
```

The typed computation law (`predicate::typed_rule`) descended here from
`nika-compile-cognition` on 2026-09-28, verbatim: the part-by-part validation of
a seat's typed computation against the request and its deterministic lowering. Creation and
replay must run one law — a seat's computation is admitted by it, and a recorded rule is
re-derived by it when a record comes back — and the replay door in `nika-compile` cannot
reach cognition. The seat's wire stays decoded in cognition, strictly and once; the meaning
crosses as the same JSON, read here with the same field rules (listed keys only, a missing or
null optional read as empty, required fields present). No struct is exposed and no manifest
changed; the walk kept its length (272 lines, the same fn-length ceiling, relocated).
`predicate::rederives` is the replay's fixpoint of that law: a recorded seat-typed rule read
back as its meaning (the inverse of the lowering: a slot by its recorded label, a truth value
by a word of the request that spells it) must validate and lower to exactly the recorded
rule, every slot it asks recorded. It binds the rule to the law, not to its meaning (the law
grounds a comparator, an aggregate, a junction or a direction nowhere): that hole is open.
A literal of the computation (a compared value, a number, a derived number, a limit) must be
stated in the reader's own clause that holds its evidence (`clause_scope`: the request's
sentences cut by `lexicon::split_clauses`, never the seat's citation boundary): a number the
request states only in another clause, such as a schedule's hour, is not admitted and the rule
is asked (option 2, measured: with the seat's citation as the scope one legitimate positive
broke, a threshold stated in the same clause just outside the citation; with the clause, none).
Output names stay words of the whole request and fields stay among its columns.

Two additional stages are admitted beside the walk (`with_order`, one line in it: the walk
measures 272 lines, its documented hard ceiling, never above): a tie rule (`ties`, `first_in_file` or empty) settles a
stated sort over rows still in file order, never a grouping; the output columns written as JSON
numbers (`numbers`) are projected columns, each named once, never over totals. Any other word,
or either where it cannot hold, is no rule. `meaning_of` reads both back (absent as unstated), so `rederives` holds a rule
that states them. Neither is grounded in the request's words: a seat that states a tie rule
the request never states is a claim the judges read, as for a direction.

The fidelity move preserves the existing onboarding facade entry points. Direct Rust
imports of `nika_compile_reader::{candidate, fidelity, sketch}` must instead name
`nika_compile_fidelity`; these reader paths are removed. Nothing of this crate is
re-exported by core or reader. In the composed ADR-140 boundary, core reads `fidelity`
and cognition reads `fidelity`, `sketch` and `candidate`. The reader cannot re-export
the new member without introducing a production dependency cycle.

## 2. The boundary, measured

| direction | edges at the split (production code) |
|---|---:|
| the reader → this crate | **0** |
| this crate → the reader | `plan` (the typed plan and its elements) · `hot::{fold, stated_sources, stated_destinations}` · `lexicon::GATE_WITHOUT_EFFECT` · for `behavior`: `rules::{Rule, Comparator, Junction, NumberPolicy, keeps_order}`, `aggregate::AggOp`, `paths::single_file` |
| `nika-compile` → this crate | `fidelity` and `sketch`, bound once at the crate root (the assembler's emit, the native and sketch doors) · `candidate` in the native door |

The approval laws read a gate on the AST the parser gives Check (`nika-schema`) and reuse
Check's refusal substitution (`nika-check-analyzer`, NEP-0020): those two dependencies came
into the reader with the laws and leave it with them, so the reader depends on `serde_json`
alone again (the ADR-138 property).

The guard of a stated approval is stricter than the analyzer's consent predicate: it counts a
confirm gate only when its `nika:prompt` declares no `default:` (`final_gate::blocking`), the
blocking human gate of Check's trifecta and of ADR-099's pause rider. A `default: false` is a
consent gate for `human_confirm`, and stays one there, yet unattended it answers « no » by
policy and the run exits 0 without asking (AUTH-01/02/06, 2026-09-24: « Demande-moi
explicitement avant de l'envoyer »). The refusal teaches the repair: omit the default; an
unattended « no » is the invocation's `--answer <id>=false`. A prompt no stated approval
needs keeps its default.

## 3. Contracts kept

- The laws are unchanged: the five moved files keep their bytes, except that the two helpers of
  `candidate` build plan elements through `Step::new` and `Effect::new` (INV-019) — a struct
  expression of a `#[non_exhaustive]` reader type compiles only inside the reader. The values
  are identical (`categories` empty, `policy_literal` absent).
- Added after the move, Law 22b (`fidelity::unnamed_writes`, private, run by `laws`): a
  planned write whose target names no single file must be carried by a `nika:write` task. Such
  a write comes from the reader's unnamed-destination floor or from an unsettled copy. A
  candidate that drafts and writes nothing is refused by name (`UNWRITTEN DESTINATION`), at the
  native and sketch doors as at the assembler's emission. Law 1 witnesses stated paths only,
  and Law 22 leaves writes to their paths, so such a write was judged by neither.
- Added after the move, Law 1's observed placement (`fidelity::laws_observed`, the door that
  also takes the host's observation; `laws` runs it with none and is unchanged). It applies to
  a bare file name the request states (`orders.csv`: one component, with no directory, home,
  glob or placeholder) that no `permits.fs` entry covers. Its source occurrences are realized
  by the one file the compile request's knowledge places under that name, when three things
  hold:
  - exactly one positive row (`state: observed`) has a path whose last component is that name,
    byte for byte (`./data/orders.csv`);
  - `permits.fs.read` covers that file;
  - a `nika:read` path or a `nika:glob` pattern opens it, literally or through a bare
    `${{ const.<name> }}`.

  The refusal (`UNREALIZED PATH`, unchanged) stands otherwise: no observation, only absent,
  unreadable or outside rows, two observed files of that name, another name (`orders_old.csv`,
  `Orders.csv`), or no task opening it. A destination occurrence keeps its own law. The law
  reads only the rows it is given (the knowledge the host's observer builds, `nika-cli-host`
  `compile/observe.rs`), never the disk. The native door passes its observation. The sketch
  door's structural law admits only stated paths, and the deterministic door reads the stated
  literal, so neither ever reads an observed path, and both keep `laws`.
- Added after the move, Laws 24 and 25 (`fidelity/record_scope` and `fidelity/instants`,
  private, run by `laws_observed` only: without the host's observation they judge nothing).
  They read a `nika:jq` expression, parsed and never run, with `jaq-core` at the workspace pin
  the analyzer compiles with. The records of each key of the task's `args.input` object, or of
  the whole input, are traced through `with:` bindings and task outputs (`nika:convert` to
  JSON, a `fromjson` `nika:jq`) to a `nika:read` of an observed path. The walk follows jq's
  scope: `.K[]`, `map`, `select`, the `*_by` builtins, `any`, `all` and `E as $x | F`, which
  keeps `.`. Only a task without `for_each` whose expression parses is judged.
  - Law 24 (`RECORD SCOPE`): on one record, a path whose first key is a key of the input
    object and no observed column of the records' file reads null. One finding per task and
    iterated key names the keys, the file, its columns and the repair (`. as $doc` before the
    iteration, then `$doc.<key>`). A column of the same name, a variable bound before the
    iteration and a read at document level stay admitted.
  - Law 25 (`TEXT ORDER ON INSTANTS`): on one record, `<`, `<=`, `>`, `>=`, `sort_by`,
    `min_by` or `max_by` over a field the host observed as ISO-8601 date-times in more than one
    offset or form (`Z` and `+00:00` are two), or against a string bound that is a date-time
    in another offset or form. The repair compares `fromdateiso8601` instants. The evidence is
    the categorical values of the observed row, or the `instants` a kinds entry carries per
    field (`{"offsets": […], "forms": […]}`), which the observer does not record yet; the
    public `fidelity::instant_shape` classifies a value. A date-only bound, a bound that is no
    date-time and a converted field stay admitted.
- The reader's modules are bound at the crate root under the names the moved files always
  used (`crate::plan`, `super::hot::fold`, `crate::lexicon::GATE_WITHOUT_EFFECT`); the member
  re-exports nothing of the reader.
- The 12 ADR-003 gates were passed by `nika-onboard` at its admission; this member inherits
  them as a member of the same unit (the ADR-115, ADR-137 and ADR-138 posture). Mutation and
  property attestations for the moved laws are owed as pending evidence, tracked with the
  unit's, never claimed.
- The public types (`fidelity::Diagnostic`, `sketch::{Sketch, SketchTask, Edge, Verb, Hole,
  Fill}`) moved as they were and are not yet `#[non_exhaustive]`: the ratchet is owed, not
  claimed.

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

## 4. Module map

`fidelity` (the laws and their diagnostics · the approval guard and Law 3b in
`fidelity/final_gate` · Law 23 in `fidelity/records`, with its measured forms in
`assets/record_forms.txt` · Law 24 and the jq scope walk in `fidelity/record_scope` · Law 25
in `fidelity/instants`) · `sketch` (the constrained intermediate, its structural laws, its
typed holes, its document) · `candidate` (the plan a candidate states, a revision's delta) ·
`behavior` (the contract and the report types; `behavior/requested` the contract a plan
states, `behavior/provenance` what a sentence of the closed language proves, `behavior/evaluate`
the relation, `behavior/verdicts` the judgment, `behavior/selection` the choice among candidates,
`behavior/{numbers, values, formats}` exact numbers, value comparison and the canonical
readings, `behavior/accounting` the round and turn budget).
