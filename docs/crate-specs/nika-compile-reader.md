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
- Every public type is `#[non_exhaustive]` (the forward-compatibility ratchet of the
  boundary); the composer builds plan elements through `Step::new`, `Effect::new`,
  `Obligation::new`, `Binding::new`, `Clause::new`, `Aggregation::new` and `Derived::new`
  (INV-019) and matches the reader's enums with a wildcard arm. Five public types added
  after the split (`cardinality::{Bound, Measure}`, `shape::{LiteralLookup, Shape}`,
  `structure::Law`) are not yet `#[non_exhaustive]`: that ratchet is owed, not claimed.
- `provenance.plan` (the recorded plan a sidecar replays with zero provider calls) is the
  reader's `Plan::to_json` / `Plan::from_json` pair, byte-identical.
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
