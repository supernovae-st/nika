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
number, or one field holding one), it returns exactly 0 in that shape on the source with no
row, and a number in that shape on each one-row source: null or a stated error there is no
sum. That identity is measured on the source with no row only; a one-row source proves a
number, not which. An average, a minimum or a maximum may stop with a stated error there; rows
and groups of no row are an empty list. A refusal by these laws goes back to the seat once
with its program and the stated defect (role `transform_repair`), within the policy's
repairs: one allowance per request, shared by every transform step and a field answer's
regeneration, never one per clause; zero buys no call. The attempt is told and recorded under
`transform_repairs` by what the receipt shows of its call (answered; refused by the call
ceiling before any transport; timed out; failed; not sent), never by the request for it, and a
repair that got no answer keeps the defect named. The laws judge values on the seat's own
rows. They cover neither
every predicate that keeps no row nor what the request asks (a program that omits the
clause's predicate, or returns another shape, holds them), which stays the whole-request
verifier's; the scalar reading rests on the example the seat chose, so it proves neither that
the request asked a scalar nor that the sum is complete.

A seat's program also answers for the spelling the host observed (`transform::spelling`, R4
A11), under the bounded canonical-spelling law of R4 A5, defined once
(`nika_compile::surface::observed::equivalent_spellings`) for the typed equalities and the
seat's programs. A binding is a column of the request's one stated source whose host-observed
categorical values spell, with other bytes, a literal the clause states at exact token
boundaries (`stated_spellings`: never inside another word, never a column name, never a
byte-identical spelling; no case or compatibility folding). A program must not drop either
spelling of a bound column, whatever columns its `columns_read` declares (a bracket read escapes
a declaration, and a column the program never reads moves none of its outputs): on each one-row
source of its own example with the column set to the stated literal, to the observed spelling,
and to a text neither spells (U+2400), it must not treat exactly one of the two spellings as it
treats that unmatched text (a byte comparison drops the rows spelled the other way), nor return
a value on one spelling and fail on the other. Every string value and key exactly equal to a
probe's own text reads back to one placeholder, so echoing or grouping the value is no
difference; an error on both spellings is none (the row errs whatever the spelling, and the
value laws and the run own that error). What the program makes of the value itself (a label,
ASCII uppercase, a code-point length, an encoding of its bytes) may differ between the spellings
by its very definition: that is the request's to ask and the whole-request verifier's to judge,
never this law's to refuse (those four valid intents were refused by an earlier output
comparison, measured with a frozen RED). A refused program is named with the column, both
spellings and their code points, and goes back to the seat within the same one allowance
(`transform_repair`); with none left the request stays INCOMPLETE. The stated literal stays the
request's, the program's bytes are never rewritten, and the observed spelling of a stated
literal is no invented literal. The transform state carries the host's categorical values
(`observed_values`), so a first program can compare the source's own spelling. The offline
counterexample « …status is livré » over an observed e + U+0301 was READY and summed nothing
(H1, RED frozen before the fix); it is now refused, then repaired or INCOMPLETE, and a program
matching both spellings is no longer refused as an invented literal. The law covers the seat's
own example rows and the host's bounded sample: a spelling the sample did not show, a column
without categorical values and a literal the clause does not state bind nothing, a program
treating the observed spelling some third way (neither as the stated one nor as unmatched) is
left to the verifier, and passing it proves no equivalence of meaning.

A candidate a model's plan shaped is judged against the whole request before READY
(`cognition::verify`, R4 A11). The core names what no law reads from the bytes: a duty a
step's words only restate (a label), a task carrying words no law reads (unverified), a clause
no element of the plan names, and, for the first candidate of a WARM or COLD plan, the whole
request. The judge answers one closed question per pending clause at each place the request
states it (`carried` · `missing` · `no_operation` when no element claims it, or NONE): the core
names every statement's span under `decision.pending.open[].spans` and settles a clause the
request repeats only when each statement is judged. It then answers the whole request (`faithful` ·
`unfaithful`, then the part it misses, located over the request's own text cut where
punctuation ends a phrase, so a path, a URL or a decimal reaches the repair whole). Each question
shows the request as compiled and as first stated, its answers, the observed world and the
candidate's own bytes; labels, task names, comments and generator confidence are claims, never
evidence. A judgment is admitted by the core only under the binding it recomputes (request,
original request, answers, observed world, stated plan, candidate bytes, clause and span), so a
judgment bound to another context or other bytes, or naming another clause or span, settles
nothing, and `no_operation` never settles a claimed, whole or restrictive clause. The binding is
that context, not a round nonce: it does not date a judgment. Cognition passes the core only the
judgments its own judge calls returned in the compile at hand; a serialized plan or answer is
never read as one. The judge is the caller's decision seat,
else the authoring provider asked through the journaled authoring call: its calls, usage and
failures ride the authoring receipt with every other call, under the same physical ceiling,
and `usage_complete` covers them; each attempt records its own usage under
`semantic_verification`. A part found missing is a concrete defect: COLD repairs from it within
the policy's repairs, the repair call carrying the judge's own state and, after its instructions,
the judge's reference (below), and a repaired plan's
computations go through the transform seat again with the judge's defects, so no program of the
plan it replaced survives; WARM makes no proposal and stays INCOMPLETE. An abstention, a failed
judge or exhausted repairs leave the request INCOMPLETE naming the clause and the next action,
never a question for what the request already says. An answer round replays its record through
`replay_judged`: deterministically closed duties replay as they are, with no call; the remainder
the core names is judged by the round's judge, or stays INCOMPLETE when the round has none; a
field answer's regeneration is the first candidate of its plan and is judged whole. Nothing a
record or a request carries is read as a judgment. The judge is a model: its approval is bounded
evidence, not proof; a clause a line break splits across two named elements is judged, not read.

Every verifier question and the COLD repair carry one compiler-owned reference, apart from the
untrusted state (`verify::grounding`, R4 A11, E36): the engine's output conventions whole, the
card's language section and the whole stdlib section of each tool the candidate reaches by the
checker's own capability inference over the parsed workflow (`nika_check::infer_permits`: an
invoke in any task form and the tools an agent may call, never a denied one), never cut to a
size (the write, convert and fetch sections exceed the 2,000 characters the native door's
callables keep). An MCP tool or a glob with no embedded section, a child workflow's tools and a
candidate that does not parse are named, never described. Each question opens with the same
reference bytes, then asks; the repair reads them after its instructions, over the candidate the
judge read. The verdict records the engine identity, the sha256 and size of the reference text
sent and each piece's receipt; each judge call through the authoring provider and the repair
journal those receipts beside their instruction digest. The reference describes the engine; it
proves nothing about a candidate. The conventions it carries state both write laws of a
computation (below), the typed total's and the synthesized program's, so a judge reads each
candidate against the law its compute follows.

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
The output conventions state the compiler's written-total law (R4 A11, E36): a total the engine
types (a named total over every row) goes to a structured file (json, csv, yaml, toml) as the
compute's object and to a prose file as its value alone when it is the only total, several
totals keeping the object; a computation the engine does not type is the jq program a seat
synthesizes, and the value it returns is written as it is, whatever the destination (the COLD
transform writes `${{ tasks.compute.output }}` to json and md alike). A shape the request names
overrides both as an obligation, and no other wrapper, key or field is added; the deterministic
compile does not read an explicit bare-number request (« write only the number »), which stays
INCOMPLETE, never claimed. Tests pin the text against the candidates the compiler emits, typed
and synthesized.

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
