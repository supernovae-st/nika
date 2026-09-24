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
