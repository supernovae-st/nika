# Crate spec — `nika-compile-trigger`

| | |
|---|---|
| Status | **MEMBER** (size-cap split of `nika-compile`, itself a member of the admitted `nika-onboard` unit · ADR-142 · D-2026-07-09-N1 · 2026-09-30) |
| Layer | L4 — a library surface; lateral L4→L4 edges `nika-compile → nika-compile-trigger → nika-compile-reader`, never back |
| Design | the trigger reading of a request, pure over its words: the words of a folded phrase, its coarsest cadence and time of day (`reading`), the period multiples five cron fields cannot hold (`multiple`), the cron fields a phrase states whole (`schedule`), the form of a trigger clause (`form`) and the words only this reading reads (`words`) |
| IMPL | measured by `scripts/crate-metrics.sh nika-compile-trigger` at each freeze; the crate carries what `nika-compile` read in `trigger.rs`, `trigger/multiple.rs` and `trigger/schedule.rs` on 2026-09-30 and the eleven word tables of `nika_compile_reader::trigger_words` the reader never read, moved with their unit tests |
| LOC budget | ≤15k crate · ≤1500/file · ≤100/fn |
| Crate version | tracks workspace |
| License | `AGPL-3.0-or-later` |
| Edition | 2024 |
| Publish | `false` — member of the `nika-onboard` unit |
| NIKA codes | none minted here — the reading returns words, labels and proposals; `nika-compile` speaks through `CompileOutcome` |

## 1. Purpose

On 2026-09-30 `nika-compile` stood at **14,949 prod LOC** and `nika-compile-reader` at
**14,986**, against the 15,000 cap, while the compiler still owed the lowering of the two
cadence forms the arming grammar holds beyond plain fields (the last day of a month and an
interval of weeks from its start date) and the reader owed the heads that keep them whole.
Not every line of `nika-compile`'s trigger module binds a requirement: the reading of the words
(which cadence, which time of day, which multiple, which cron fields, which clause form) is pure
over folded words and needs nothing of the compiler. Eleven of the reader's trigger tables
(`AT`, `BETWEEN`, `TIME_UNITS`, `NAMED_TIMES`, `WEBHOOK`, `SEQUENCE_HEADS`, `EVENT_HEADS`,
`COMPLETION_WORDS`, `TIME_WORDS`, `ARRIVAL_WORDS`, `MANUAL`) were read by that reading alone.
Per D-2026-07-09-N1 a size-cap split is ONE architectural unit in several workspace members:
the reading and those tables ascend here, above the reader and below `nika-compile`, which binds
them at its historical paths (`crate::trigger::{classify, arriving, TriggerForm}` are
re-exported by its `trigger` module).

## 2. Surface

- `phrase_words(folded) -> Vec<&str>`: the words of a folded phrase, a clock keeping its `:`.
- `stated_cadence(words) -> (Option<&'static str>, Option<String>)`: the coarsest cadence label
  (`weekdays`, `weekly`, `monthly`, `daily`, `hourly`, `minutely`) and the time of day `HH:MM`.
- `time_of_day(words)`: the time after an introducer or by name, with the words it consumed.
- `multiple::unbindable(words)`: a period multiple five cron fields cannot hold.
- `schedule::fields(phrase)`: the five cron fields a FR or EN phrase states whole, or none.
- `classify(phrase) -> TriggerForm` and `arriving(phrase)`: the form of a trigger clause.
- `words`: the word tables only this reading reads.

The cadence tables the reader reads itself (`DAILY`, `WEEKDAYS`, `WEEKLY`, `MONTHLY`, `HOURLY`,
`MINUTELY`, `RECURRENT`) stay in `nika_compile_reader::trigger_words`.

## 3. Laws

The reading states what the words state and nothing they do not: no default hour or day, no
narrowed period, no guessed cron. A phrase with neither a cadence nor a time of day is not a
schedule; a multiple keeps no coarse label; a phrase the fields cannot state whole proposes
none. The binding of what is read (the questions, the answers, the zone, the policies, the
ceiling) stays in `nika-compile`, and `nika-cadence` alone validates and executes a bound
expression.
