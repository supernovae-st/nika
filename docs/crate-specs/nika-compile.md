# Crate spec — `nika-compile`

| | |
|---|---|
| Status | **MEMBER** (size-cap split of the admitted `nika-onboard` unit · ADR-137 · D-2026-07-09-N1 · 2026-09-21) |
| Layer | L4 — a library surface; lateral L4→L4 edge `nika-onboard → nika-compile`, never back |
| Design | the stateless Compile core: one `CompileRequest` in, one `CompileOutcome` out — the deterministic reader (frozen: a safety floor), the private typed semantic plan (operations · effects · obligations · constraints · typed computations), the finite composer and its feasibility rules, the deterministic assembler, the Check preview, the recorded-plan replay across answer rounds |
| IMPL | measured by `scripts/crate-metrics.sh nika-compile` at each freeze; the crate carries what `nika-onboard::compile` carried on 2026-09-21 (about 12k prod LOC · 187 unit tests · 9 integration suites) |
| LOC budget | ≤15k crate · ≤1500/file (`lexicon.rs` carries a `lookup-table` LOC-EXEMPT: the frozen reader's head, cue and marker tables) · ≤100/fn |
| Crate version | tracks workspace |
| License | `AGPL-3.0-or-later` |
| Edition | 2024 |
| Publish | `false` — member of the `nika-onboard` unit |
| NIKA codes | none minted here — compile diagnostics speak through `CompileOutcome`; machinery failures keep the canonical `nika-error` voice |

## 1. Purpose

`nika-onboard` reached **17,318 prod LOC against the 15,000 cap** during the compiler war
room (2026-09-20/21): the compile module alone was 16k lines of the crate's 23k. Per
D-2026-07-09-N1 a size-cap split is ONE architectural unit in TWO workspace members: the
Compile core descends here, the onboarding surface (`nika init`, the gallery, the briefs)
stays in `nika-onboard`, which re-exports this crate at its historical path:

```rust
pub use nika_compile as compile;
```

Every caller keeps writing `nika_onboard::compile::…` (`nika-cli-host`, `nika-serve`, the
MCP oracle). The boundary moved; the surface did not.

## 2. The boundary, measured

| direction | edges before the split |
|---|---:|
| `nika-onboard` (surface) → `compile` | 1 (`lib.rs` declares the module) |
| `compile` → the rest of `nika-onboard` | 2 (`intent::STOPWORDS`, `banner::sentence`) |

The two borrowed helpers moved into `nika_compile::text` (`STOPWORDS`, `banner_sentence`,
`banner_lines`, `is_label`) and `nika-onboard` re-imports them. The member never depends
back on the surface.

## 3. Contracts kept

- `CompileRequest` / `CompileOutcome` / `provenance.plan` (the recorded plan a sidecar
  replays with zero provider calls) are byte-identical: the arena's trusted-plan control,
  the clause-drop probe and the canaries are the regression gates.
- The deterministic reader is frozen: no cue or head is added; every new law is a structural
  one (path boundaries, anaphora, the shape of a human gate, the carrier of a constraint) or
  lives in the typed semantic plan.
- The 12 ADR-003 gates were passed by `nika-onboard` at its admission; this member inherits
  them as the second half of the same unit (the ADR-115 precedent). Mutation and property
  attestations for the compile core are owed as pending evidence, tracked with the season-2
  debt wave, never claimed.

## 4. Module map

`lexicon` (reader) · `objects`, `gates`, `hot` (admission laws) · `cognition`, `predicate`
(the typed proposal and its validation) · `plan`, `rules`, `aggregate`, `columns` (the typed
plan and its computations) · `compose` (the finite candidate set and rules 1–14) ·
`bindings`, `shape`, `paths`, `assemble` (the deterministic assembler) · `retrieve`
(recall over the gallery, provenance only) · `decide` (the closed-choice seat) · `edit`,
`edit_source`, `materialize`, `wire`, `support`, `types`, `text`.
