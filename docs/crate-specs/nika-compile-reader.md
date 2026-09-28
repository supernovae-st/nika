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
  newline still ends a sentence, and the same words outside the quotes still govern. The
  waiver shapes and the few indecision phrases read by substring are not yet quote-aware.
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
