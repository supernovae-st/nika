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
`compose`, `predicate` and `decide` move with their complete tests; `predicate` keeps the
seat's wire decoding of a typed computation, whose law descended to `nika-compile-fidelity`
(`predicate::typed_rule`, shared with the replay, 2026-09-28). The deterministic
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

A request whose words both ask for an effect and prohibit it never reaches a seat (R4 S0,
superseding the earlier « the seat arbitrates » reading). `route_create` answers every
strategy (Escalate, Only, Sketch) with the deterministic door's refusal of the request as
read: a clarification that replaced the original is that request, never the original. A
revision in words whose change contradicts itself is refused before `native::author` revises
the base, and `native::escalates` never escalates a refused outcome.

Money is read before every strategy (R4 B15). `compile_with_cognition` reads the directives a
host admitted (`CompileRequest::with_admitted_money`) or its operator stated
(`with_stated_money`) through `nika_compile::surface::admitted` before support, HOT, WARM, COLD,
native and sketch: every seat reads the request with those directives blanked, and the outcome
records each one beside the original request's identity (`decision.money`). A revision's change
is read by the same law; on a door that states money so is the request its base answered, and a
creation's `intent.clarification` replacement is read afresh, the words it replaced stating
nothing. On a host-admission door, changed clarification bytes discard the spans admitted on
its earlier request before any money record is attached; unchanged bytes keep their admission
and the same blanked reading inside the answer. Repeating a request must not reintroduce its
admitted ceiling as work or open a needless provider call. A host re-admits a replacement by
submitting its own bytes with their own spans. A request that names a skeleton only once its
directive is blanked is read as written,
never as that skeleton, its money recorded. A ceiling no seat can be held to opens none: an
admitted zero on every door, any stated ceiling on a door that meters no seat, any ceiling of a
request read as written (a seat would read it as work). The generative and the decision seats
both stay closed, the deterministic outcome stands (a HOT READY stays READY) and an
`authoring_money` finding says why no request was sent. A host that meters its seats (Session's
admission account) keeps them open under a positive ceiling it admitted; the compiler certifies
no cap.

A field answer's regeneration (`transform::pending::resume`) is claimed only once the replay of
its verified record kept it: the Applied finding and `transform_regeneration.accepted: true`
ride that record. A replay that refuses it keeps its own findings; the outcome carries no
candidate and no verified record, states `accepted: false` with the refusal as its `why`, and
the provider call stays spent and counted (E14 FRESH-1 saw `accepted: true` beside a refusal).

A seat's program also answers for sources beside its own example (`transform::domain`, R4
A11): the source with no row and each source holding one row of the seat's example. It never
returns null on them, which no write can take (B16's live `… | add` over no shipped row ran
READY into a failed write). When the clause's leading word is a sum or a count by the Reader's
closed `AggOp` words, with no ranking cue, and the program's example value is a scalar (a
number, or one field holding one), it returns exactly 0 in that shape where no row is kept:
null or a stated error is no sum of nothing. An average, a minimum or a maximum may stop with a
stated error there; rows and groups of no row are an empty list. A refusal by these laws goes
back to the seat once with its program and the stated defect (role `transform_repair`), within
the policy's repairs: one allowance per request, shared by every transform step and a field
answer's regeneration, never one per clause; zero buys no call, and each attempt is recorded
under `transform_repairs`. The laws judge values on the seat's own rows. They cover neither
every predicate that keeps no row nor what the request asks (a program that omits the
clause's predicate, or returns another shape, holds them), which stays the whole-request
verifier's; the scalar reading rests on the example the seat chose, so it proves neither that
the request asked a scalar nor that the sum is complete.

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

### Explicit reasoning effort (R4 B16)

A policy that names a reasoning effort (`AuthoringPolicy::with_reasoning`, the closed core
word `AuthoringReasoning`: low · high · max) is asked by every call this member makes through
`receipt::authoring_request`: the COLD plan and its evidence repair, the native candidate and
its repairs, the sketch and the transform. The word maps to the kernel's `ReasoningEffort` of
the same spelling; a word the kernel does not know sends nothing and leaves an
`authoring_provider` finding. The output cap and the timeout are the policy's own and never
move with the effort; a truncated answer stays a failure. `ProviderChoice::with_reasoning`
asks the decision call the same level under the declared authoring cap; without it the
decision call keeps its 256-token request. Whether a route carries a level at all is the
provider adapter's decision: a route the catalog does not qualify refuses before any byte,
recorded as `admission_refused`.

Each call's receipt entry, and each decision answer, records `reasoning` as separate facts:
`configured` (the policy's word, else null), `transmitted` (the reasoning keys the adapter
read back from the body it dispatched, or `unobserved` when no response carried them, a
refused call included), `served: unknown` (the provider's internal effort is not observable
here), `reasoning_tokens` (null when unreported) and `response_model`. The read-back is the
adapter's own observation of its serialized request, not an independent network capture.

The COLD plan call and its repair send `opening(intent)` only (the instructions and the
request text); the observed world, the answers already given, knowledge references and an
edit's original request reach the native opening alone. That gap is recorded for the
semantic-context slice, not closed by the effort.

### Response identity evidence

The shared authoring `Seat` counts an absent, empty or whitespace-only provider
model identity as unreported. It never substitutes the requested model. Nonblank
reported identities remain exact and deduplicated in first-observed order; the raw
provider response is preserved. This observation is not independent provider or
invoice verification. CLI, Session and native Serve use this same owner.
