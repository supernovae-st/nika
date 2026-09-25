# Crate spec — `nika-compile-fidelity`

| | |
|---|---|
| Status | **MEMBER** (size-cap split of `nika-compile-reader`, itself a member of the admitted `nika-onboard` unit · ADR-141 · D-2026-07-09-N1 · 2026-09-24) |
| Layer | L4 — a library surface; lateral L4→L4 edges `nika-compile → nika-compile-fidelity → nika-compile-reader`, never back |
| Design | the laws a candidate `.nika` document is judged by, pure over (request · plan · projected document) (`fidelity`), the constrained sketch a seat proposes and the document it states (`sketch`), the plan a candidate document states by its structure and a revision's delta (`candidate`) |
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
| this crate → the reader | `plan` (the typed plan and its elements) · `hot::{fold, stated_sources, stated_destinations}` · `lexicon::GATE_WITHOUT_EFFECT` |
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

## 4. Module map

`fidelity` (the laws and their diagnostics · the approval guard and Law 3b in
`fidelity/final_gate` · Law 23 in `fidelity/records`, with its measured forms in
`assets/record_forms.txt`) · `sketch` (the constrained intermediate, its structural laws, its
typed holes, its document) · `candidate` (the plan a candidate states, a revision's delta).
