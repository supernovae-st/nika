# Crate spec — `nika-compile-reader`

| | |
|---|---|
| Status | **MEMBER** (size-cap split of `nika-compile`, itself a member of the admitted `nika-onboard` unit · ADR-138 · D-2026-07-09-N1 · 2026-09-21) |
| Layer | L4 — a library surface; lateral L4→L4 edges `nika-compile → nika-compile-reader` and `nika-compile-fidelity → nika-compile-reader` (ADR-141), never back |
| Design | the frozen deterministic reader and the typed semantic plan it produces: the multilingual head, cue and marker tables (`lexicon`), the structural laws (`objects` · `gates` · `paths` · `columns` · `hot`), the closed rule grammar and its typed computations (`rules` · `aggregate` · `rule_tokens` · `rule_cues` · `stages`), the plan and its provenance record (`plan`), the text helpers the compiler and the onboarding surface share (`text`) |
| IMPL | measured by `scripts/crate-metrics.sh nika-compile-reader` at each freeze; the crate carries what `nika-compile` read on 2026-09-21 (the gate's own counter: 8,432 prod LOC at the split · 46 unit tests · the multilingual sentence battery runs in `nika-compile` as `compile_reader_sentences`, where both members are in reach) |
| LOC budget | ≤15k crate · ≤1500/file (`lexicon.rs` carries a `lookup-table` LOC-EXEMPT: the frozen reader's head, cue and marker tables) · ≤100/fn |
| Crate version | tracks workspace |
| License | `AGPL-3.0-or-later` |
| Edition | 2024 |
| Publish | `false` — member of the `nika-onboard` unit |
| NIKA codes | none minted here — the reader produces a plan, never a finding; `nika-compile` speaks through `CompileOutcome` |

## 1. Purpose

`nika-compile` reached **18,028 prod LOC against the 15,000 cap** the day ADR-137 descended
it out of `nika-onboard` (2026-09-21): the product-convergence line and the season-2 typed
computations landed on the same member. Per D-2026-07-09-N1 a size-cap split is ONE
architectural unit in TWO workspace members: the reader and the plan descend here, the
composer, the assembler and the preview stay in `nika-compile`, which reads this crate at
its historical module paths:

```rust
use nika_compile_reader::{columns, hot, lexicon, objects, paths, plan, rule_tokens, rules};
pub use nika_compile_reader::text;
```

Every caller of `nika-compile` keeps its whole public surface (`nika_compile::text`
included). The boundary moved; the surface did not.

## 2. The boundary, measured

| direction | edges before the split |
|---|---:|
| `nika-compile` (composer) → the reader | 7 module paths |
| the reader → the composer | 2 clusters of pure text helpers (`shape::fold` and its unit tables · `network::page_facet` and the carried back-reference), now in `rule_tokens` and `objects` |

`serde_json` is the only dependency: the reader is deterministic, keyless and offline by
construction, and the dependency graph proves it.

The laws a candidate document is judged by (`fidelity`), the sketch a seat proposes
(`sketch`) and the plan a candidate states (`candidate`) were placed here on 2026-09-22/23
while `nika-compile` stood at its wall; they ascended to `nika-compile-fidelity` on
2026-09-24 (ADR-141) with the two dependencies their approval laws had brought
(`nika-schema`, `nika-check-analyzer`), so the statement above holds again.

## 3. Contracts kept

- The deterministic reader is FROZEN: no cue or head is added; every new law is a
  structural one (path boundaries, anaphora, the shape of a human gate, the carrier of a
  constraint) or lives in the typed semantic plan. The reading of every intent is
  byte-identical to `nika-compile` before the split (216 `#[test]` before, 216 after).
  The recurrence-fidelity correction preserves an explicitly recurring request.
  Previously, « Fais-moi un rapport des trucs importants régulièrement » could become
  READY with the recurrence silently missing:
  the closed FR · EN table `trigger_words::RECURRENT` of recurrences stated without their
  cadence (« régulièrement », « de temps en temps », « regularly », « from time to time »),
  found whole-word and folded by `words::recurrence`. It adds no operation: the phrase
  becomes the plan's `trigger` (cut as a head when it leads its sentence, left in its
  clause otherwise, never over a head the sentence already states), `nika-compile` asks
  the cadence as a mandatory `trigger.cadence` question (`manual` is an answer), and
  `hot::rejections` refuses a reading whose trigger another head took, so the recurrence
  never vanishes under an event or a distribution. The adjectives (« un rapport
  régulier », « a regular report ») and the frequency words (« souvent », « often ») stay
  out: they also name kinds of things (« une expression régulière », « regular
  customers »); a recurrence stated only that way is still not read.
  The reader also preserves a cadence stated at the end of a sentence (« Fais-moi
  un rapport des trucs importants chaque lundi », « … tous les matins », « … every Monday »),
  which the head-only reader left in the draft's words, a one-shot READY with no trigger
  (e6bc576b witness). No new parser: a quantifier of the closed tail list (« chaque », « tous
  les », « toutes les », « every », « each ») not first in its sentence, followed by the head
  grammar itself (`head_bounds`: cadence words, their small words, clock tokens) naming a
  period and ending the sentence. It is cut off as a head is, so the clause reads exactly as
  without it, and settled once every sentence is read: the plan's trigger when no head
  recorded one, the completion of a recurrence head that stated no cadence, nothing more
  when the head already says it. An event, a distribution, a sequence or a different cadence
  beside it, in either sentence order, or two different tails, is the unknown work
  `lexicon::TWO_TRIGGERS` naming both, never one kept in silence. A quantifier after a
  grouping, negation or exception word (« de chaque mois », « pour chaque mois », « sales of
  every month », « mais pas chaque lundi »), inside quotes, after a colon that opens content
  (« Écris dans note.txt : réunion chaque lundi »), or in a negated sentence opens no
  schedule. The French plural weekdays (« tous les lundis ») join the cadence tables
  (`cadence_words.txt`, `trigger_words::{WEEKLY, TIME_WORDS}`): the head « Tous les lundis, … »
  was read as an event.
  A recurrence is never a one-shot READY claim wherever the request places it (R4 S0). « each »
  opens a head only over cadence words (`CADENCE_HEAD_PREFIXES`, `cadence_bounds`: « Each weekday
  at 8, … » is a schedule; « Each row whose status is open, … » states no trigger, the comma
  fallback of the other prefixes is not taken). A cadence that ends an earlier clause of its
  sentence (« Read ./tickets.json every weekday at 8, keep … ») is cut from that clause as the
  sentence-final one is (`cut_clause_tail`, the same guards), except in a clause that opens on
  a prohibition (`opens_negated`, the negated-sentence law), and settled with the tails. The
  six placements of the schedule fixture's cadence give the same candidate bytes and the same
  `requested_trigger`, which stays `requires_binding`: a schedule is never claimed bound.
  A head keeps the words of the two cadence forms beyond plain fields whole, so the compiler
  can lower them: a month end opens a head only over cadence words (« On the last day of every
  month at 18:00, … », « Le dernier jour de chaque mois à 18h, … »; « On the last day of the
  sprint, … » states no trigger), `on` and `last`/`dernier` are small words of a head (« Every
  month on the last day at 18:00, … »), and an ISO start date `YYYY-MM-DD` after « from »,
  « starting », « beginning », « à partir du », « à compter du » or « dès le » ends the head it
  anchors, also right after the comma that closes it (« Every 2 weeks on Monday at 9:00,
  starting 2026-10-05, … »). Anything else after that comma (a range « from … to … », « starting
  with the oldest ticket ») stays out of the head.
- The unnamed-destination floor keeps an output the request asks for without naming it.
  Before it, « Résume mes notes dans un fichier. » compiled READY after the model answer alone:
  one draft, no effect, the transformation ledgered as realized (S98 J02 on 53f8c640, through
  the real TUI and the zero-provider CLI alike). No cue or head is added.
  `objects::unnamed_destination` reads the destination grammar `destination_at` states (its
  connector tables, now shared), an indefinite singular determiner (the module's own law: an
  indefinite object is new), at most two modifiers that are no function word, and a file noun of
  `paths`' table, with no file name after it in its clause. Inside quotes, or after a colon that
  opens content, it reads nothing: the sentence-final cadence's guard (`cadence::quoted`).
  `hot::unnamed_destination_floor` turns such a destination inside a producing step's object
  into a write, whose target is the noun phrase and whose evidence is the connector and the
  phrase; the assembler then asks its exact path (`const.output_path`) and grants nothing
  before the answer. The floor is idempotent, and `nika-compile` applies it again when it
  replays a HOT record, so a record an earlier engine wrote cannot make that request READY.
  A definite or possessive file (« dans le fichier », « in my file ») stays a locative. A
  plural one (« dans des fichiers ») is not read: a compiled workflow writes one named file per
  request, and that case is unchanged.
- A ban of every write names the write effect (R4 S0, the 2026-09-27 A07 audit case « write
  'hello' to ./a.txt but do not write anything », which a seat resolved into a write).
  `effects::universal_write` reads a write head and one universal object (« anything »,
  « rien », « any file », « quoi que ce soit »; « nothing » in the positive form « write
  nothing »), in either order (« ne rien écrire »), after at most the negation particles, with
  nothing after them but a terminal qualifier (« at all », « to disk », « du tout »). Beside a
  requested write, `push_effect` merges it into the `Conflict` effect whose evidence keeps both
  clauses as exact excerpts. A ban scoped to what the content says (« nothing about
  salaries »), to the rest of the shape (« nothing else ») or to another literal file is no
  universal ban, and a ban alone stays `Forbidden`.
- Quoted content is what the workflow writes, reads or matches, never an instruction to it.
  `cadence::quoted_at` extends the sentence-final guard to single-quoted literals: a straight
  quote opens at the start or after a space or an opening bracket and closes before a space,
  a punctuation mark or the end, so an apostrophe inside a word (« don't », « n'écris »)
  neither opens nor closes. A listed policy marker inside quotes (ban, gate, stop,
  indecision, revision, deduplication) is skipped by `earliest`, as is a shaped final or named
  gate, and a connector, a full stop or a semicolon inside quotes cuts no clause and no
  sentence: « write 'do not write anything' to ./a.txt » and « Read ./rules.txt, which says
  'never email anyone', and email … » read as their neighbours with neutral quoted words. A
  mark after a backslash is content (« "she said \"hi\"" »). A newline inside a quote that
  closes after it is content; past the last closed quote a newline still ends its sentence,
  so an unclosed mark never carries one. A rooted path inside quoted prose (« 'delete
  ./b.txt' ») is no path literal (`paths::located`, the bindings, the clause's own path), and
  a head inside quotes asks for no element (`hot::cue_coverage`). The laws that read words by
  substring read only what is stated outside quotes (`cadence::unquoted`, the `quoted_at` law
  blanking quoted content byte for byte): the waiver (`gates::waiver_polarity`), the approval
  bound of a ban, the bypass and refund backstops, the indecision phrases and their companion,
  and the contradiction-marker sentence. A line filter over 'no need to ask me' keeps its
  filter (it was read as a waiver and dropped, READY-wrong), and a ban whose quoted object says
  « until I approve » stays a ban. The same words outside the quotes still govern.
- Constant work stays deterministic (R4 S0, G2). A write whose object is exactly one quoted
  literal, to a prose destination it names (« write 'hello' to ./a.txt », « écris « bonjour »
  dans ./a.txt »), states that literal as its content: a `content` binding (the verbatim
  quoted span, marks included) instead of a draft, so no model and no question.
  `text::quoted_literal` is the one law of that literal: its characters as written (newlines,
  Unicode, instructions and template-shaped text included); a backslash before the closing
  mark or before a backslash is that character; guillemets drop their typographic inner
  spaces; a single quote closes as `quoted_at` reads it, never inside a word. `Plan::content_of`
  pairs a write with the one literal its evidence holds, never a guess between two, and the
  HOT admission asks no producer for it. At a named gate (« ask me before writing 'hello' to
  ./a.txt ») the gated write reads its quoted object by the same law, and its target keeps
  the request's spelling; one quote-aware path finder (`objects::stated_path`) serves the
  clause, the target and the gated object. Anything else keeps its draft or its question: an
  unquoted object, two literals, a literal with more words (« 'hello' in French »), a
  transformation of it, or a structured destination (how a text sits in JSON or CSV is not
  stated).
- A conversion (`lexicon/convert.rs`: a head, then two structured files of two formats) is the
  identity over the parsed records, the whole clause its rule's text. A clause whose segments
  also state a rule or a stage the grammar reads, or whose words compare (`rules::compares`:
  « …, keeping only the rows whose amount is above the agreed threshold », a value no grammar
  types), is no plain conversion: the identity swallowed it (READY, every row written). A
  participle stage (« …, sorted by amount ») is read by no law of the frozen reader and is not
  covered. The identity is a complete recorded rule (`Rule::from_json`); a flag over no clause
  and no stage is not.
- A removal of duplicates over the rows is no dedup obligation.
  `ObligationKind::Dedup` means no second effect for the same incoming
  identifier, and it asks for a state file. « Déduplique par customer et invoice_id,
  première occurrence conservée » was read as that obligation.
  - A dedup head or marker is read on as an operation when all three hold. The clause stays
    unresolved for the seat, whose typed `distinct_by` or program then carries it; it is never
    swallowed (the HOT admission's dedup cue is satisfied by the obligation alone). The reading
    is recorded as an `in_data_dedup` binding holding the clause, and the compiler tells it in
    an Applied finding (`dedup`: no state across runs is asked; say so if earlier runs must be
    skipped).
    - The clause scopes it by a field: a scope word (par, by, por, per) followed by a word that
      is not « défaut », « default », « exemple » or their kin.
    - The clause states the occurrence kept (« première occurrence », « the first »,
      « la dernière »).
    - The request holds no cross-run cue: no phrase such as « already processed », « déjà
      traité », « never twice », « between runs » or « state file », and no event-shaped
      word such as event, événement, callback, webhook or incoming.
  - Any other removal keeps the obligation and its state-file question. That covers a missing
    kept occurrence (« Déduplique par invoice_id »), a missing scope, and a cross-run or event
    cue (« dédoublonne le callback par identifiant »).
  - The tables are `assets/dedup_words.txt`, the dedup heads and markers included (they read
    as before).
  - Measured on 21 deduplication requests (keyless): French and English requests satisfying
    these conditions lose the obligation; the other requests retain it. Nothing else moves.
  - Not covered: a kept occurrence stated in another clause of the sentence
    (« …, même si des doublons sont éloignés ; première occurrence conservée »), and a
    removal that names its keys with no kept occurrence. These keep the
    obligation.
- Every public type is `#[non_exhaustive]` (the forward-compatibility ratchet of the
  boundary); the composer builds plan elements through `Step::new`, `Effect::new`,
  `Obligation::new`, `Binding::new`, `Clause::new`, `Aggregation::new` and `Derived::new`
  (INV-019) and matches the reader's enums with a wildcard arm. Five public types added
  after the split (`cardinality::{Bound, Measure}`, `shape::{LiteralLookup, Shape}`,
  `structure::Law`) are not yet `#[non_exhaustive]`: that ratchet is owed, not claimed.
- `provenance.plan` (the recorded plan a sidecar replays with zero provider calls) is the
  reader's `Plan::to_json` / `Plan::from_json` pair. A canonical record replays every
  provided owned field without loss or coercion. Present malformed `rules` or `slots`,
  a defective element beside a valid one, and nested values that would otherwise be
  dropped or defaulted refuse the entire plan. Historical absent optional fields may
  still default; transport metadata such as `strategy` remains its caller's to read.
  The rule record always includes its junction, including for one clause. Its computed
  observational fields must agree with the decoded rule if they are present.
  Recorded aggregation rounding stays within the generated 0..=6 precision domain,
  and a non-count aggregation must name a source column. Numeric operands have closed
  numeric syntax and must match whole numeric tokens in the request, including sign.
  A program cannot override typed clauses or shape stages; produced names are unique.
  Slots require declared `const.<identifier>` keys and an explicit numeric flag.
  An effect's `alone` (E38: the plan states a write holds one computed value by itself) is
  recorded only when true, so a plan recorded before it replays unchanged; a recorded
  `false` or any other value is refused as a field no record writes. No reading of the
  request sets it: only a plan states it.
  Required original rule fields (value kind, summary, shape, multi-clause junction)
  cannot default; absent lines/program remain compatible with their historical windows.
  A closed-grammar rule is anchored by its own words: the request states them, runs of
  whitespace and typographic quotes aside (a seat that unwraps a line or drops a double
  space names the same words; E14 seat r2), never a changed letter (« inactive » for
  « active » is other words, unlike the evidence path's near-miss citation).
  A verified program's text is the detail of the compute step it realizes (the seat's words
  for it, or a detail a stated rule joined with « ; »): it is anchored through that step,
  whose evidence stays an excerpt (E14 F7: demanding the program's text verbatim refused
  every paraphrased or joined continuation after its paid regeneration).
  This replay check does not establish full clause-to-intent fidelity, arbitrary program
  correctness, runtime permission, or business-result correctness. The single-clause
  junction is serialized but means nothing (« and » or « or » over one clause is that
  clause), so the identity a pending or verified transform binds (`plan_sha256`,
  `verified_rule`, in `nika-compile`) keeps the historical canonical form without it:
  the records the fcf290a7b compiler wrote (`nika-compile/tests/fixtures/historical`)
  replay as they did there, and a record written here carries the identity that compiler
  computes. A change that means something still changes the identity, and a changed
  identity never authorizes a changed context.
- One number law (R4 A5, `rules/numbers.rs`): a value a rule reads as a number is a finite
  JSON number or a text `text::NUMBER_TEXT` accepts (the JSON number grammar: an optional
  minus, `0` or digits without a leading zero, an optional fraction, an optional exponent
  since R4 A6, blanks around; no plus in front, bare or trailing point, comma, `Infinity`,
  `NaN` or empty text) whose value is finite (`1e999` is no number), parsed; anything else
  follows the field's stated `NumberPolicy`. FAIL stops the run naming the field and the
  value; SKIP makes the comparison false for that record and leaves it out of a ranking or a
  total, and a ranking or total left with no number stops the run. Under either policy an
  average, a minimum or a maximum over no number is no value, never 0 or null: it stops the
  run, from an empty input too, and an average divides by the numbers it kept. A rule with no
  stated policy renders exactly as a recorded plan always rendered it (`tonumber`, `tonumber?
  // .` for a sort), so every plan record stays canonical under the strict replay and its
  identity unchanged; the compiler states a policy on the rule it binds (`nika-compile`). The
  bounded canonical-spelling expansion of a text equality (`Rule::with_spellings`) matches
  exactly the extra spellings the compiler grounded; it is never a normalization at run.
  Policies and spellings are serialized only when stated, and a plan record carrying either
  is refused, never read without them.
- **Exact order where a policy binds (R4 A8).** A number the compiler binds under a stated
  policy compares, ranks and sorts by its exact decimal key. The key is `dkey`, one of the
  decimal laws `nika-compile` emits in front of the rule; jq's own equality and ordering are
  unchanged.
  - A comparison reads `(<law> | dkey) <op> (<other> | dkey)`. The other side is the literal
    as the request states it (the text `Operand::Number` keeps, never a jq number literal),
    another column's law, or a slot (`numbers::compared`).
  - A ranking sorts by `<law> | dkey`. Descending is still the ascending order reversed, so
    equal values keep their documented order: source order ascending, reverse source order
    descending.
  - A tie rule the request states (`Shape::ties_first_in_file`, the plan's `ties:
    first_in_file`, E38) keeps source order both ways: a descending sort orders the reversed
    rows stably and reverses them back (`reverse | sort_by(K) | reverse`), an ascending one is
    the stable sort itself, and no `dtie` cut follows, since the request chose among the tied
    rows. It holds only over a sort of rows still in file order: with no sort, a grouping or
    a join, the shape is refused, read back or merged. Unstated, the lowering is byte for byte
    as before.
  - A ranking that keeps n rows passes through `dtie` before its own `.[:n]`, unless the
    request states the tie rule. When rows n and n+1 tie and the tie holds distinct records as
    written (after any projection), input order would choose, so the run stops. JSON-equal
    copies, and ties the cut does not separate, pass.
  - A projected column the request writes as a JSON number (`Shape::numbers`, E38) is read
    through the number law with FAIL: a value that is no number stops the run naming its
    column, never SKIP, which would drop rows no filter states. Each is a column the rows are
    projected on, named once, never over totals, or the shape is refused. It is then written
    through the exact output law (`dnum_out`, R4 A8): the value itself where a JSON number
    carries it exactly, else the run stops naming the column and what the value would have
    become (`0.12345678901234567891`, an integer past 2^64), never a rounded number.
  - `ties` and `numbers` are recorded only when stated, so a record written before them reads
    and replays as it did.
  - A plain sort bound over observed numbers sorts by the key too.
  - An aggregate over a bound number (a sum, an average, a minimum, a maximum, with the
    rounding the request states) is lowered through the exact arithmetic laws
    (`dsum_out`, `davg_out`, `dmin_out`, `dmax_out`, `laws/arithmetic.jq` in `nika-compile`):
    the result is exact within a 1000-digit bound, an average with no finite expansion asks
    for a rounding, and a result no JSON number carries stops the run naming it. A count is
    unchanged.
  - Unbound reads render as before, byte for byte, so every plan record keeps its canonical
    `jq` and replays under the strict check. This covers a rule with no stated policy, a sort
    with no policy, and a ranking over a produced value (a count or a total per group). Such
    a ranking breaks ties by the group key's order, never by input order.

  The unbound code is retained for that reason, and it is not the candidate's computation:
  the compiler binds a policy on every number field of every synthesized rule.
- **Closed word tables are data (V9 A10, A0).** The 30 word tables of the stage grammar live
  in `assets/stage_words.txt`, one `[name]` section per table, read into statics under their
  old names; `EXCLUSION_LEADS` lives in `assets/exclusion_leads.txt`. They moved unchanged
  from `stages.rs` and `rules.rs` at 4bddf8a14 (`GROUP_PHRASES`, pairs, stays in Rust), and
  two tests pin every table against that frozen pre-edit list, word for word and in order.
  The assets hold words only, no executable string; their raw lines (523 + 48) are reported
  beside the production count, which they do not enter. The six word tables of `shape.rs`
  (headings, distributive cues and leads, leading quantifiers, structural and supplied cues,
  135 entries) moved the same way to `assets/shape_words.txt` (146 raw lines) at 161e9e649,
  pinned by their own frozen test; one section reader (`rule_tokens::section`) serves
  `stage_words.txt` and `shape_words.txt`. The line grammar's two tables (`ONE_OR_MORE`, its
  quantity words, and `BY_CONSTRUCTION`, its by-construction tails, 82 production lines of
  `rules/lines.rs`) moved exact and in order to `assets/line_words.txt` (81 raw lines) at
  fc8a0feba (V9 A11), read through the same section reader and pinned with the section count
  by `lines::tests::tables_are_the_frozen_lists`: 14,974 production lines before, 14,901
  after (`scripts/ci/prod-loc.py`). The grammar reads the same words.
- **A stated sort orders every row; only a ranking asks its count (V9 A11).** A descending
  sort under a ranking word with no stated count asks `const.top_n`
  (`Rule::ranking_without_count`): « the top-selling items by units » keeps n rows, and n is
  the requester's to state. « sort them by amount_usd, most expensive first » used to ask it
  too, a count the request never implied, and the correct descending plan stayed INCOMPLETE.
  Since bbbc721e6 a clause led by a sort verb that states no keep lead and no only word
  (`stages::sorts_every_row`, over the grammar's own `SORT_VERBS`, `KEEP_LEADS` and
  `ONLY_WORDS`) orders every row: its superlative is the direction, and no count is asked.
  « sort them by amount and keep the most expensive » still asks it. Limits: the reading is
  this closed form, the clause's first word a sort verb; it is no general understanding of
  rankings, and a superlative the grammar does not lead with a sort verb keeps its question.
- **A clause's lead is read, never dropped (R4 F1, V9 A10).** The words before a clause's
  field (before its relative marker, or before its last word) used to be discarded whole:
  « count the rows where status is paid » read as the filter alone, and the workflow wrote
  the rows, READY. `rules::lead_reading` now accounts for every one of them.
  - A count or an aggregate the stage grammar reads whole over the rows a relative clause
    keeps (`stages::lead_stage`: « count the rows where … », « the number of rows whose … »,
    « the total of the amount column where … », « compte les lignes dont … ») runs after every
    clause of its segment. The rows' own noun (« count the orders where … ») stands for them
    in a count, as a row word does.
  - The clause's own verb (the words before the first function word: « filter », « show
    me »), the function words (`stages::lead_word`, `ARTICLES`) and the rows' noun state
    nothing the filter drops: those leads read the same filter, byte for byte.
  - Any other word stating a stage (`stages::operation_word`, `SUMMARY_CORE`), or a word after
    the first function word that is not one (a modifier: « the paid rows where … », « les
    lignes payées dont … »), leaves the clause unread. HOT is never READY on it: the clause
    goes to cognition, or stays unresolved, never to a narrower filter.
  - The stage a lead states runs after the clauses of its own segment, and the segments
    around it keep their stated order (the next entry).
  - The fused rule is an ordinary rule record (clauses and a shape). A plan recorded with
    the old filter-only reading is refused on replay as not what its words say; a fresh
    compile recovers.
  - Known limits: an adjective with no determiner before the rows' noun (« paid orders
    where … ») still reads as the clause's verb; « how many … where … » and a sort or a top-N
    stated before the relative clause are outside the closed forms and go to cognition.
- **Stages run in the order the request states (R4 F5, V9 A10).** A rule is a list of
  steps: its own filter and shape, then `Rule::then`, each `Then` a filter and a shape over
  the rows the step before it wrote. `Rule::jq` lowers every step through the one lowering
  (`Shape::lower`, and `map(select(…))` for a later step's filter), and every number any step
  reads falls under the one number law (`number_fields`, `with_number_policy`).
  - `synthesize` keeps the segments in the stated order. `stages::place_clauses` and
    `stages::place_stage` put a segment's clauses or stage into the last step only when the
    fixed per-step order (join, distinct by, grouping and aggregates, derived, sort, limit,
    projection, renames, distinct) reproduces the stated order, or when a stated commutation
    holds: a filter after whole-row duplicates removed or after a projection keeping every
    column it reads (stages that neither drop a row nor read a number); a sort or a cut after
    a projection keeping its key. Otherwise a later step opens on the rows the last step
    wrote. A filter stated after a sort, a grouping or a total runs after it, so a number
    the stated order reads under a FAIL policy still stops the run: moved first, the filter
    dropped that row and the run wrote (reproduced on the emitted program, then fixed).
  - « keep the 2 rows with the highest amount ; keep the rows where status is paid » ranks,
    then filters the two kept rows; the reverse order filters, then ranks, in one step; « keep
    the 2 … ; count them » counts the two rows.
  - Nothing a later step cannot read is composed, and nothing is reordered without its
    precondition: a filter after totals, a rename, a derived value or a produced name (a
    HAVING), a filter on a source column after a grouping on another key, the same stage
    twice, a join after the first step, « or » clauses followed by another segment, or steps
    over the lines of a text file leave the text unread: HOT is never READY on it, cognition
    takes it. A source-field filter stated after an aggregation is never read as its scope.
  - The rule record gains the optional key `then` (a list of `{clauses, junction, shape}`),
    written only when a rule has a later step: every other record keeps its bytes, a binary
    before this one refuses the key instead of replaying part of the reading, and `fields`
    lists what any step reads.
- **A selection of the rows is a demand, never context (V9 A10).** A clause led by a keep
  or an exclusion lead of the rule grammar over a noun of the material
  (`structure::selection_demand`: « keep the rows whose status is a », « garde les lignes
  dont … », « ignore the rows where … ») demands an operation even where the grammar cannot
  read its predicate. `structure::context_statement` never calls it context, and the compile
  ledger names it unresolved work unless the compute step states each of its clauses (the
  constraint then restates the computation): such a clause compiled READY with its filter
  dropped, taken for a description the material realizes. The leads are the grammar's own
  tables (`stages::keep_lead`, `rules::exclusion_lead`); the context sentences of every
  language keep their reading. The lexicon files such a clause as unresolved beside its
  constraint (« keep the tone formal » stays a constraint alone): HOT is rejected and a
  configured cognition carries it, its proposal bound beside what the grammar reads; with
  none, the door names the clause and needs cognition. The constraint stays the policy floor a
  proposal joins, so a recorded plan keeps its identity, and the deterministic ledger names
  the clause once. `structure::only_function_words` (the reader's closed function
  words) tells the compile witness which words between two read clauses state nothing.
- **A restriction is never a clause that asks for nothing (V9 A11).** The compile core lets a
  judge settle a clause no element of a seat's plan names as asking for no operation, except
  where `structure::restricts` holds: a keep or an exclusion lead of the grammar, a structure
  law of this module (« nothing else », « no other file », no model, a single request), or a
  word of `[restriction_words]` in `assets/stage_words.txt` (75 folded words over the six
  languages: negations with their pronouns and determiners, « only », exceptions and
  conditions), pinned with the other tables by `stages::tests::tables_are_the_frozen_lists`.
  A path or a URL is a literal, never a word of the clause, so the `out` of
  `./out/result.json` is no exclusion lead. `structure::only_a_compute_head` (the words around
  one computation head, « then compute », « calcule », function words aside) lets the compile
  witness keep such a head beside the object the grammar read instead of filing it as an
  unread part; `lexicon::compute_head` is its crate-private reading. Limits: closed word
  lists; a restriction phrased with none of these words reads as no restriction, and the
  judge's `no_operation` stays admissible for it.
- **The named gate asks with `request`, `seek` and `solicit` too.** Its asking
  verbs (`assets/gate_ask_verbs.txt`) hold « request human confirmation before writing » as
  « ask for human confirmation before writing » does: the write waits for a human's yes. The
  shape is unchanged: an asking verb, then an approval word or a person, then a `before` or
  `until` connector that binds the effect after it; « request the file before writing » and
  « request a refund before the deadline » stay no gate, and a waiver is read first.
- The 12 ADR-003 gates were passed by `nika-onboard` at its admission; this member inherits
  them as the third member of the same unit (the ADR-115 and ADR-137 precedent). Mutation
  and property attestations for the reader are owed as pending evidence, tracked with the
  season-2 debt wave, never claimed.

## 4. Module map

`lexicon` (the reader · `read` · the EN/FR/IT/ES tables under `lexicon/`) · `plan` (the
typed plan and its provenance record) · `rules`, `aggregate`, `rule_tokens`, `rule_cues`,
`stages` (the closed rule grammar and its typed computations) · `objects`, `gates`,
`paths`, `columns` (the structural laws: what a clause names, where a human gate sits, what
a literal token is, which words are columns) · `hot` (the strict HOT admission over the
reader's own vocabulary) · `text` (the shared text helpers).
