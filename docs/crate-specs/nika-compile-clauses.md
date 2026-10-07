# Crate spec — `nika-compile-clauses`

| | |
|---|---|
| Status | **MEMBER** (size-cap split of the admitted `nika-onboard` unit · ADR-145 · D-2026-07-09-N1 · 2026-10-07) |
| Layer | L4 — a library surface; lateral L4→L4 edges `nika-compile-cognition → nika-compile-clauses → nika-compile-reader`, never back · `nika-compile` does not depend on it |
| Design | how one clause of a request reads above the reader, pure over its words: the parts a verifier asks alone and how each reads (`parts`), the prohibition reading of a clause (`prohibition`), the proposal merge's word tables and classifiers (`words`) and the literals a clause states that the host observed spelled with other bytes (`spellings`) |
| IMPL | measured by `scripts/crate-metrics.sh nika-compile-clauses` at each freeze; the crate carries what the reader read in `structure.rs` (the prohibition reading) and `words.rs` (the merge's vocabulary), what the verifier read in `verify/parts.rs` and `verify/faithful.rs` (the part readings), and what the core's surface stated in `observed::stated_spellings` on 2026-10-07, moved with their tests |
| LOC budget | ≤15k crate · ≤1500/file · ≤100/fn |
| Crate version | tracks workspace |
| License | `AGPL-3.0-or-later` |
| Edition | 2024 (workspace-inherited) |
| Publish | `false` — member of the `nika-onboard` unit |
| Dependencies | **read from `Cargo.toml`, which is authoritative** · `nika-compile-reader` (the restriction, keep, exclusion and operation words, the heads, the fold, the structure laws) · `unicode-normalization` (canonical decomposition) |
| NIKA codes | none minted here — the readings return booleans, excerpts and pairs; the seats' doors speak through `CompileOutcome` |

## 1. Purpose

On 2026-10-07 `nika-compile-cognition` stood at **16,632 prod LOC**, `nika-compile-reader` at
**15,614** and `nika-compile` at **15,014**, against the 15,000 cap. Four readings of one clause
of a request lived where they were not read: the reader's prohibition reading, read by the
verifier alone; the verifier's parts of a request and the two readings of a part it shares with
the whole-request verdict; the core surface's stated spellings, read by the seats' spelling law
alone; and the proposal merge's word tables, read by the merge alone. Per D-2026-07-09-N1 a
size-cap split is ONE architectural unit in several workspace members: they ascend here, above
the reader and below the seats' doors (ADR-145). The cognition binds its historical
`crate::words` path to this crate.

## 2. Surface

- `parts::parts(intent) -> Vec<String>`: the parts of a request, each an exact excerpt of it,
  cut where a phrase ends and never inside a closed literal; a short phrase, a label, a condition
  and a phrase of function words join a neighbour, so no word is dropped.
- `parts::restricts(part)`: whether a part restricts, read as the reader reads a restriction,
  English negative contractions read as their « not », a negated demand restricting nothing.
- `parts::asks_an_operation(part)`: whether a part may ask an operation of its own (no pure
  prohibition, no structure law without an operation of its own).
- `prohibition::{pure_prohibition, negated_demand, states_operation}`: whether a clause forbids
  by negation alone, demands by a negation of forgetting, or states an operation of its own.
- `words`: the key a clause is folded by (`clause_key`, `fold_words`), a draft that is no
  language work (`serialization_draft`), a format kept by construction (`only_format_words`),
  the content words of a clause (`content_words`) and their tables.
- `spellings::stated_spellings(clause, observed, columns)`: the literals a clause states that the
  host observed spelled with other bytes, as `(stated, observed)` pairs.

The reader keeps the words it reads itself and exposes the four predicates the prohibition
reading asks (`stages::{restriction_word, keep_lead, operation_word}`, `rules::exclusion_lead`)
and `lexicon::reads_a_head`, never its head type.

## 3. Laws

The readings state what the words state and nothing they do not: a negation forbids only what
its phrase holds, a demand by a negation of forgetting is a demand, a part is the request's own
bytes, a stated spelling is canonically equivalent at exact token boundaries. Nothing here asks a
judge, reads a candidate, calls a model or grants authority; the verifier, the spelling law and
the proposal merge in `nika-compile-cognition` decide what a reading means for a candidate.

## 4. Related

- ADR-145 (this split) · ADR-138 (the reader) · ADR-140 (the seats' doors) · ADR-142 (the
  trigger reading, the precedent) · D-2026-07-09-N1
- `docs/crate-specs/nika-compile-reader.md` · `docs/crate-specs/nika-compile-cognition.md`
