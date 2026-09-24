# Crate spec — `nika-compile-cognition`

| | |
|---|---|
| Status | **MEMBER** — implemented size-cap split of the admitted `nika-onboard` unit under the authorized V7 consolidation, ADR-140 and D-2026-07-09-N1 |
| Layer | L4; production edges to core, Reader and Fidelity, never a core-to-cognition edge |
| Design | Explicit seat orchestration: COLD proposals, bounded decisions, native/revision/sketch authoring, verified transforms and knowledge recall |
| LOC budget | ≤15k production lines per crate; ≤1500/file; existing function-length ratchets follow their moved functions |
| Version / edition / license | Workspace version / 2024 / AGPL-3.0-or-later |
| Publish | `false`; member of the same architectural unit |
| Diagnostics | Existing `CompileOutcome` and `CompileError` contracts; no new NIKA codes |

The member reads `nika_compile::surface` and the core's public request/outcome types.
`CompileRequest` and `AuthoringPolicy` remain core-owned with their builders; `Cognition`
and decision seats live here. `nika-onboard::compile` combines both members at the existing
consumer paths. Provider choice and admission context remain host-owned and explicit.

`cognition` owns the orchestration ladder; its children own proposal decoding, native
answers/judgment/repairs, sketch filling, verified transforms and knowledge references.
`compose`, `predicate` and `decide` move with their complete tests. The deterministic
native record application and replay stay in core and are shared by accepted candidates
and answer rounds. Candidate, fidelity and sketch laws come from `nika-compile-fidelity`
(ADR-141); no Reader implementation is duplicated.

A revision in words is judged against the original request and its change together, so a
path the original states and the change leaves behind (« Copie entree.txt dans a.txt. »,
then « Finalement, utilise b.txt. ») must not refuse every faithful revision, nor vanish
silently. `native::revision` waives such a path for the path law only when the seat names it
in its `gaps` and the change words never name it; creations and the sketch door keep the
law whole. The accepted gap is then either superseded on proof — the change words state a
replacement (« utilise », « plutôt », « instead ») and no addition (« aussi », « also »),
state exactly one path, and the candidate is the base with the old path replaced by it and
nothing else changed (the workflow's name aside) — recorded as `superseded` and an applied
diagnostic, or it stays a pending gap the human disposes of before READY. An omitted path
plus a seat's gap is never, by itself, a removal the requester asked for; a change that names
the old path (« n'écris plus dans a.txt ») still refuses a candidate that drops it.

An observed world states names; it does not answer a choice the request leaves open. The
judge refuses a question for a column, field, key or value an observed file states, except
one: a `const.<x>_column` · `_col` · `_field` · `_header` question when the request speaks of
« une colonne » / « a column » and names none of the columns of the one file the host
observed with several. It is admitted as a closed choice whose offers are that file's observed
columns, verbatim (never a name the seat's label proposes); the record keeps them, and the
replay bakes an offered key only — any other answer is a finding and the choice stays asked.
The authoring card still tells the seat never to ask an observed name; its amendment for the
open column is a separate Spec proposal, and the seat that picks a column silently is not
detected by this law.

The spec pin and canonical expression laws are read from core, including by the retained
knowledge/transform tests. Kernel inference, bounded Tokio timeouts and jaq/capability
verification belong to this member; they do not add a production dependency back from core.

The core integration suites retain all test bodies and fixtures, using a development edge
to this member for seat entry points. Qualification requires their execution on the
composed workspace plus member unit tests, clippy, docs, API and hygiene checks. Admission
is inherited from the unit; mutation and property attestations remain pending, never
claimed by this split. Existing public-type ratchet debt is unchanged.

## Embedded authoring resources

The native authoring and sketch cards live in this crate's `assets/`, beside
its output conventions. They describe compiler fidelity and prompt behavior,
not normative language rules; syncing the canonical Spec pack must not delete
them. Native-call receipts keep the actual card SHA-256 alongside the distinct
Spec pin and pack version. A missing card is a build error, never empty text.
