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

A projected name absent from observed source columns can require a generated output,
rather than an input-field mapping. When no column list is stated, its computation's
clause names it, and no typed stage reads or produces it, the typed rule is withheld.
The existing verified transform may express that computation; withholding a rule
grants no READY verdict. Observed spellings, copied columns named only by the write,
and fields read by filtering, grouping, sorting or aggregation retain their grounding
questions. Creation and replay share this admission through `predicate::admit`.

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

## Grounding observed source facts

`grounding` owns the pure observation law shared with `nika-compile`: `Grade`,
`Seen`, `Entry`, row matching, grading, revision identity and literal comparison.
It descends from the compiler's private grounding module; stale-answer and
request-witness handling remain in the compiler facade. Existing typed-rule
admission and recorded shapes use the same law.

`grounding::semantic::facts` records a bounded slice of a semantic Sketch
candidate's dependencies. It recognizes literal root key segments and object
shorthand in jq expression fills, outside strings and comments; code in a string
interpolation is inspected. A recognized key is recorded for each declared read
source that the observation places unambiguously and shows that key. The record
uses `bound_by: semantic_reads`, the observed grade and sampled presence.

This is partial source coverage, not a jq dataflow proof. Computed accesses,
unobserved keys, ambiguous paths and unrecognized reads contribute no fact.
An empty result establishes no freshness. Conservative extra facts can withdraw
a proposal after a source change; no recorded set establishes that every read,
value, behavior or unread row has been checked. The host still judges each
recorded dependency again before Save through the compiler's existing basis law.

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
- Law 2 (`fidelity::invented`: no invented path or host) also admits an address composed from
  the request's own words (`nika_compile_reader::paths::origin_and_path`):
  - its origin stated as a whole token, read case-insensitively;
  - its path stated as a whole token, in its own spelling, read against the request and answers
    as written.

  A path's directories and stem are judged by `paths::composed_from`, which moved to the reader
  at this crate's size cap (2026-10-08). An address taken from another, or a longer, origin or
  path is still `INVENTED LITERAL`: a host label, a port, a child segment, a suffix, a path
  character continuing or preceding the stated token (quoted or not), or another spelling.
- Added after the move, Law 22b (`fidelity::unnamed_writes`, private, run by `laws`): a
  planned write whose target names no single file must be carried by a `nika:write` task. Such
  a write comes from the reader's unnamed-destination floor or from an unsettled copy. A
  candidate that drafts and writes nothing is refused by name (`UNWRITTEN DESTINATION`), at the
  native and sketch doors as at the assembler's emission. Law 1 witnesses stated paths only,
  and Law 22 leaves writes to their paths, so such a write was judged by neither.
- Added after the move, Law 1's route witness (`fidelity::role`, private). A rooted literal
  the reader states only as a destination (`hot::stated_destinations`, sentence by sentence)
  is also realized, at a destination occurrence, by a sending `nika:fetch` (any method but
  GET) whose URL is exactly one of the reader's `url` bindings followed by that literal. Each
  bare `${{ const.<name> }}` is read from `const:`, and a query or fragment is ignored:
  `POST /notifications/stock` to a stated `http://127.0.0.1:57468`. A source never is, even
  after a destination sentence, and no fs authority is granted. An unquoted spaced name keeps
  its exact extent: no neighbouring literal, article or last words settle it.
  `stated_paths`, which takes no plan, passes no `url` binding: it realizes no route.
- Added after the move, an open name's typed answer (`fidelity::asked_names`,
  `fidelity::asked_readings`). An open name is the reader's unquoted spaced name holding a path
  whose first word it leaves open (`paths::open_names`: `un payload out/notification.json`).
  Law 1 still owes it exactly, and its finding says how the human settles it. A question asks
  it when its label or why names it verbatim and no other open name, its `const.<slug>` is
  declared blank, and a task reads `${{ const.<slug> }}` whole as its `path`. The native judge
  then passes it as waived, and the admitted question is the closed choice of the name's
  readings (`paths::readings`: the whole name, then from each later word to the path alone),
  the only answers the replay bakes. A name no question names, or two name, stays owed; a
  quoted name is no open name. `literal::answered::grant_paths` completes the side's one empty
  `permits.fs` entry in place with the inferred paths the record's questions answered, every
  other entry kept, as `grant_host` completes an answered endpoint.
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
  `compile/observe.rs`), never the disk. Native and Sketch pass their current observation.
  `structural_laws_observed` also admits a Sketch read placed by this unique bare-name
  witness; writes and hosts never gain observed authority, and `allowed` is unchanged.
  The deterministic door still reads the stated literal and keeps `laws`.
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
- The public types moved as they were at extraction. In 0.123, `Sketch` and `SketchTask`
  are `#[non_exhaustive]` and have constructors (see the source-compatibility note below).
  The ratchet remains owed for `fidelity::Diagnostic` and `sketch::{Edge, Verb, Hole, Fill}`.

### Sketch graph semantics (0.123 slice B)

`Sketch` carries the workflow's named results, `outputs: Option<Vec<Edge>>` (the existing
`{name, from}` type): omitted or `null` keeps the historical single `result` of the last task;
`[]` states no workflow output; a list emits exactly `{<name>: ${{ tasks.<from>.output }}}` and
nothing else. Each output name is a `snake_case` identifier, named once, the result of a task the
sketch has (any task: an output is the workflow's, not a binding, so edge reservations and the
earlier-task rule do not apply). `SketchTask` carries an agent's `max_turns: Option<u32>` (the
language's 1..=1000) and `tools: Option<Vec<String>>` (named `nika:`/`mcp:` tools, no glob or
negation, each once, the agent's own list), and a loop's `fail_fast: Option<bool>`. A control on
the wrong verb or out of range refuses the sketch; omitted or `null` controls keep the
historical emission (`max_turns: 4`, `tools: []`, `fail_fast: false`), never overriding a
stated value; explicit `tools: []` is no tool. A stated agent tool joins the derived tool
requirement (`permits.tools`); it grants nothing else. A write bound by more than one edge
requires its content template (a first edge is never silently the content).

Source compatibility (0.123): `Sketch` and `SketchTask` are now `#[non_exhaustive]`, with
`Sketch::new(name, tasks)` and `SketchTask::new(id, verb, purpose)` building the legacy shape
(outputs and controls omitted, collections empty). An external Rust literal of either type no
longer compiles; function signatures are unchanged. `Edge` is unchanged. The wire is additive:
a sketch JSON without the new keys reads and emits exactly as before.

### Semantic record decode and literal conservation (0.123 slice C-core)

`sketch::record` holds the pure half of a semantic record's replay; the compile core keeps the
caller, the answers' application, the lowering to bytes, grants and the current judgment.
`replayed(record, intent, allowed)` checks the closed, typed format at every level (required
fields present and of their type, `source_is` label, versions), decodes the graph, runs
`structural_laws` under the caller's `intent` and allowed values (never the record's), the fill
laws and `complete_document`, and returns `{document, questions, gaps, trigger}`; a gap's words
are kept only when they are the request's own, else its position. A refusal is a static reason,
never a record value or key name. `bound_answers(record, current)` holds the record's answer
maps to strings and to A0 ⊆ Ak ⊆ Ac; `read_basis(intent, initial)` is the reader's and the
behavior contract's part of a request basis (`contract_projection` keeps each obligation's
identity, target, presence kind, unsupported reason and evidence). These are reconstruction and
consistency checks, never READY, authority or producer authentication.

`binding::unbound(plan, intent, observed)` is the recorded-plan binding law (each recorded
rule re-derived from its words by the law that created it, the first difference named), moved
unchanged from the compile core, whose two callers keep every refusal and continuation.

`literal` holds the literal-conservation helpers the edit door uses (`has_expression`,
`inexact_integer`, `literal_at`, `fill_slot`), moved unchanged from the compile core's
`edit.rs` (the core reached its 15 000-line wall); the answer laws, slot and operation choice,
emission, Check and grants stay in the core. `inexact_integer` reads JSON already validated.

### Sketch emission integrity (0.123 slice A)

`Sketch::from_json` reads closed task and edge objects with exact types: a field outside the
task set (`id verb tool reads writes hosts after with gated_by for_each purpose max_turns
tools fail_fast`) or the edge set (`name from`), a value of another type or a malformed array
item is refused by its path. Absent or null optional fields retain their omission semantics
(the controls are described above), never a filtered remainder. The
structural laws also refuse an empty, non-snake_case or repeated edge name and an edge named
`approved` on a gated task or `items` on a looping one (the names the assembler binds).

`fills_from_json` reads closed `{task, field, value}` objects with `value` present.
`complete_document(sketch, fills)` is the only complete emission: every fill names a declared
hole of that sketch, once, with a value of the hole's kind; every required hole is filled; a
whole `args` object never carries an argument the sketch owns for that task (every argument the
assembler derives for it, plus `path` for read/grep/write/edit and `pattern` for glob even where
no path is stated, and `input` for jq/convert/validate, read only by an edge), while another tool's own argument of the same name (`nika:grep`'s
`pattern`, `nika:hash`'s `content`) stays fillable; a write's content template reads every edge its task is bound to; no fill value references
`tasks.<id>` inside `${{ }}` (a data edge the sketch never stated). Refusals repeat only names
the accepted sketch or a tool contract declares (a task id, a declared hole, an argument the
sketch owns); any other fill is named by its index (`fills[k]`), a key outside a closed object
is never named, and no refused value is repeated (0.123 A2). On success it returns `document(sketch, fills)`
unchanged; `document` itself stays the partial projection the structural judge inspects with no
fill. Kind checks are JSON shape only: they do not prove a jq program, an argv, a URL or a
schema safe or correct, and the builtin argument vocabulary stays the Check's catalog scan (and,
before emission, the cognition door's `nika_cap` contract), which this crate cannot reach.

`complete_document` checks the declared-hole/fill laws; the cognition consumer additionally
checks builtin shapes and task-bound filesystem slots before candidate serialization.
Calling the pure fidelity function alone does not establish those additional contracts or READY.

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

## Observed Sketch replay and exact graph inputs

`structural_laws` and `replayed` retain their no-observation behavior. Their observed
siblings, `structural_laws_observed` and `replayed_observed`, consume only the current
caller's observation. Only read paths can use a unique positive witness for a stated bare
name; missing or ambiguous observations, write destinations and hosts gain no permission.

A `nika:jq` task with two or more input edges receives an object keyed by every edge name,
with each value bound to its corresponding `with` input. Zero and one edge keep their
existing forms; fills cannot replace the graph-owned `input`. A semantic record's trigger
must be a verbatim occurrence of the effective request, found on that text's own character
boundaries. Replay refuses a different non-null trigger or an omitted trigger when the
reader finds a cadence. Older records violating these laws may refuse replay; their stored
source is never used as a fallback.

## Retained program evidence for semantic revision

`sketch::kept` owns the pure envelope used by conversation hosts to retain
compiler records by a proposal identity or saved relative path and exact final
candidate bytes. Proposal and saved-file namespaces are distinct; Save invalidates
the previous record for its path even when the replacement has no retainable plan. The envelope keeps at most
sixteen recent records within 256 KiB; an individual serialized plan above
128 KiB or changed by the host's redactor is withheld. Unknown or oversized
envelopes stay unchanged and yield no usable record. Each entry binds both the
final candidate digest and the complete plan digest. The optional last-saved
project-relative path is a conversational selection only.

This module performs no I/O, invokes no provider, and confers no admission,
consent, freshness or fidelity judgment. A returned record must still pass
Compile's complete reconstruction of the EDIT base and the current observation.
The compiler's semantic record and its source-revision record retain their own
formats; this envelope does not translate either into source-authoring authority.

`observed::basis` binds a semantic reading to the current facts for the source and
destination paths stated by that reading, including files below stated folders.
Canonical ordering and `./` aliases do not change identity; extra destinations
observed for a later EDIT do not change its base. Changed relevant facts or kinds
still refuse replay. The full host observation retains its independent digest.
Older records whose full-world digest differs from this canonical scoped identity
may refuse replay; no stored source fallback or silent authority upgrade is added.

A source revision whose change omitted the new path retains the human's decided
`revision.path` beside that change clause in its resolved request. The original
change and typed links remain unchanged in the record. A subsequent revision can
therefore refer to the destination actually written, including after clarification;
no missing path is guessed and no source formatting outside its proven slots changes.


### Source revision records and answered endpoint projection

`sketch::import::record` owns the pure source-revision record construction, original
request binding, bounded question descriptions and exact replay checks. The core
lends its literal parser and obligation-ledger projection; it retains public
CompileRequest/CompileOutcome types, mandatory typed questions, Check, judgment
and diagnostic orchestration. Record format, hashes, answers and scope stay unchanged.

`literal::answered::grant_host` projects an exact answered HTTP(S) endpoint into
the candidate's declared permit document only when a fetch URL or notify target
reads that literal. The core still chooses and proves the answer, emits the changed
bytes and checks them. This pure document update grants no live execution authority.

`sketch::record::same_caller` compares the core-projected caller facts and initial
answers without granting authority; a new clarification or changed money remains
a new basis, never a replay of the stored caller.

## Exact decimal laws emitted by the assembler

`decimal::{ORDER, ARITHMETIC}` holds the unchanged jq law sources for exact decimal
ordering, rank cuts, source transport, sums, averages and stated rounding. These
are pure text beside the computation laws in this same unit: no runtime or I/O
is introduced. `nika-compile` keeps assembly and reuses the constants at its
existing local names. Rust and both embedded jq sources count toward this
member's 15k limit; the existing numeric precision integration tests remain the
behavioral qualification.

## Masked temporal observation context

`observed::temporal_shapes` counts lexical ISO-like date-time shapes from already sampled
slots. Every digit, including offset digits, is masked as `9`; no value or free text is
retained. Shapes are ASCII and at most 64 bytes. `sampled`, `matched`, and per-format counts
make mixed and unrecognized values explicit. No recognized shape emits no `temporal` entry.
The existing nested walk collects the same metadata at admitted paths, within the unchanged
row, element, path and depth bounds. Nested slots count visited members only; absent members
are not visited. Its existing `complete` flag still describes coverage, not a schema.

This context is not `instants` evidence: masking an offset destroys its numerical identity,
and matching a shape validates neither a calendar nor a timezone. Law 25 is unchanged.
Temporal metadata remains inside the source's kinds and the whole observed-world digest,
so replay never substitutes changed context for retained evidence. The native author's
existing observed-world input receives the metadata with parsing and timezone guidance.
