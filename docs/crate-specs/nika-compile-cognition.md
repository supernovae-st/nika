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

A seat's admitted typed rule joins the reader's reading of the same clause
(`proposal::seat_rules`, E38). A clause the reader holds no rule of takes it. A seat rule that
adds stages (a projection, numbers, an order, a limit, a grouping) over the reader's plain filter
with the same clauses, junction and lines replaces it, and an Applied finding says so. A seat rule
that adds stages over a plain reader filter whose clauses, junction or lines differ keeps the
reader's rule and records the disagreement as work no model settles, so the request is
incomplete. A reader rule that already carries stages binds unchanged (R4 A3), and a seat rule
with no stage of its own changes nothing. Before, stages a
seat stated over a clause the reader also read were dropped with no finding, leaving only the
judge between an incomplete workflow and READY.

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
treats that unmatched text (a byte comparison treats the rows spelled the other way as unmatched:
it drops them, or keeps the rows a negation excludes; the refusal says so in neutral words, never
that rows are dropped, B21 T4), nor return
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

A program that stops with an error on U+2400 shows no treatment of an unmatched text to compare
(B21 D1, a labelled adversarial double: it erred on every status holding no ASCII letter and
compared the stated bytes otherwise, and was READY summing 0 where 42 was due). The law then
reads that treatment from the first value the host observed in the bound column that the clause
does not state (never within the clause's text, case aside, never a canonical spelling the clause
states at token boundaries, never either spelling of a literal bound there), probed the same way:
D1 is refused naming both spellings, repaired, and sums 42. When the program errs on that value
too, or the host observed none, the law cannot judge the program and refuses nothing on that
ground: the transform record (`unjudged`), an applied `authoring_transform` finding and the
decision's `unjudged_spellings` name the clause, the program, the column, both spellings, their
code points and every text tried, and the verifier puts the notes on the plan's own programs in
the state of every question (`unjudged_spellings`), so the clause and the whole request are
settled by judges that read them, never READY silently; a field answer's regeneration records
the same (`transform_regeneration.unjudged`). A note on a program a repair replaced is not shown.
The observed value is a stand-in, not a sentinel that solves the case: a program that treats the
chosen value specially still escapes, and a judge's approval stays bounded evidence.

Every text tried that the program answers is compared, the observed value too when U+2400 is
answered: B23 F3, a program answering `.status == "␀"` itself and comparing the stated bytes
otherwise, was READY summing 0 where 42 was due; it is now refused on the observed value, then
repaired. When no answered text shows either spelling treated as it, yet the program treats the
two spellings apart, the law cannot tell whether the request means that difference: a requested
transformation of the value (a label, a case, a length, an encoding) does, B23 F2 (erring on
U+2400, answering the observed value with neither spelling's output) does not. It refuses
neither: the note records the program with its own reason, `treated_apart`, apart from
`every_probe_errs`, and goes to the judges the same way, so the third way the law leaves to the
verifier is no longer silent.

Each unmatched text is paired with that text repeated twice. The synthetic U+2400 probe is
paired too: a one-character literal's requested length can collide with that probe. A spelling
is treated as dropped only when both members of a pair answer alike and exactly one spelling
shares their output. One operation-level counterfactual then decides the drop's cause (B24,
`transform/spelling/relations.rs`): a private copy of the program, printed back from the
execution parser's tree, in which every string relation compares canonical (NFC) forms while
every value operation keeps its exact bytes. Relations are equality and ordering (with the
orderings of `sort`, `unique`, `group_by`, `min`, `max` and their `_by` forms), containment,
prefix and suffix, trimming, position, separators, keys and lookups by a key that is not a
constant ASCII name, and regular expressions; values are everything else (a length, a code
point, a slice, a case, an encoding, arithmetic, what the program builds). No literal is edited
and the emitted program keeps its bytes.

Proof domain: when the drop disappears in the canonical copy, a relation told canonically
equivalent texts apart on the probed row, and the program is refused, however the compared text
is written (split or concatenated with or without parentheses, interpolated, a fragment of the
spelling, or one literal bound to a variable that also inspects its own bytes, which the earlier
literal exchange could not confirm). Signal domain: when the drop persists, a value the program
computes decides it: a requested length, threshold or encoding, and a value used as a proxy for
equality alike. The law records it (`relation_unconfirmed`) for the judges, never as a pass. An
identity copy must reproduce the program's own answers on the probed texts; a construct without
a faithful copy, a definition shadowing a relation, an error or an unpaired answer is recorded
`relation_inconclusive`. A dead comparison whose branches perform the same value transformation
leaves the drop in place. Requests about the encoding itself (« the rows spelled with a
combining accent ») stay refused under the law's premise: canonical equivalents are one text
unless the request says otherwise.

The verifier and its probes share one jq language (`transform/engine.rs`): the runtime mirror
of `nika:jq` (jaq core, the capability-filtered std, jaq-json, the runtime's std shadows
`nika_cap::JQ_STD_SHADOWS`, the fixed run-start clock and the input-bound variables), assembled
once. The shadows are the builtin's own text, not a copy: the global `scan`, and a `tonumber`
that emits one finite number and refuses NaN and the infinities by name before a predicate,
sort or aggregate reads them. The verifier runs the shared probe set
`nika_cap::JQ_STD_SHADOW_PROBES` with the builtin, output bindings and the checker. The probe
adds two private natives through `run_with`; they are never installed for an emitted program,
and a program that names one does not compile in the verifier.

Agreement across unrelated pairs is not required: a special case of the synthetic probe must
not hide a drop exposed by the observed stand-in and its companion. A text named by the program
as a string of its own remains a special case, excluded from that comparison. Repeating the
stand-in also avoids putting the same synthetic marker in every companion. B24 S3's two property
cases and the original D1 and F3 controls remain regression obligations.

B23 R2, every status's code-point length, was refused because « annulé » counts 6 code points
like the decomposed « livré ». A requested rounded length can collide on both members of a pair
too. Canonical relations leave these differences in place, so these programs are left to the
judges, with `unmatched_varies` or `relation_unconfirmed` as observed; a constant output label
does not change that judgment. The ordinary length program is due [6, 6, 6], and the rounded length is due
[3, 3, 3]. These are distinct user intents, not evidence that arbitrary byte comparisons are safe.

The record names every text actually tried. A singleton answer whose companion fails or is
excluded cannot establish a drop (`probe_unconfirmed`); `probe_named`, `every_probe_errs`,
`unmatched_varies` and `treated_apart` retain their separate reasons. These finite observations
do not prove semantic equivalence: the probed rows and the host's sample bound them, NFC is the
only equivalence (no compatibility forms), regular-expression patterns are canonicalized on a
best-effort basis, orderings go unexercised on one-row probes, and the canonical copy's JSON
round trip may reorder object keys. A value used as a proxy for equality with the stated literal
is not refused by this law; it reaches the judges with its note. The law never rewrites the
requested literal. It refuses a requested transformation only when a relation, not the value,
separates the two spellings; the earlier literal exchange also refused requested values of the
literal itself (B24 S8C: a per-row code-point threshold, refused by the exchange), which this
law leaves to the judges.

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
never a question for what the request already says. A duty the core names that no element of
the plan carries (no candidate is emitted and no judge is asked) is told as the core's: its
finding names the duty and its kind and says no judge was asked, the clarification the core
asks stays its next action, and a repair from it is told the compiler named it, never that a
judge compared the workflow (B21 T3). A count or a size stated inside the clause a seat's
verified program was read from (« return each item with its status as one line ») is no such
duty: the core's `realize` has the compute task claim it unverified, and the judges settle it
with the rest (B21 T2); a bound stated as its own constraint keeps its law. A clause several
pending duties hold is asked once, since one judgment of it at its statements settles them all. An answer round replays its record through
`replay_judged`: deterministically closed duties replay as they are, with no call; the remainder
the core names is judged by the round's judge, or stays INCOMPLETE when the round has none; a
field answer's regeneration is the first candidate of its plan and is judged whole. Nothing a
record or a request carries is read as a judgment. The judge is a model: its approval is bounded
evidence, not proof; a clause a line break splits across two named elements is judged, not read.

A candidate the native or the sketch door finishes READY is judged against the whole request
before READY too (`verify::judged_native`, R4 A11, E39 C3). The seat writes the workflow itself
(the sketch door's seat its tasks and program holes), so no law of the core reads its programs:
the parser, Check and the fidelity laws only admit it. Once the door's conclusion is READY, the
authoring provider answers the whole-request question (`faithful` · `unfaithful`, then the part)
over the candidate's actual final bytes, with the state and reference every verifier question
carries, through the journaled authoring call under the authoring policy's caps and the same
physical ceiling; the attempt is recorded under `semantic_verification` and the route says
`verify: judged (authoring_provider)`. The native doors receive no decision seat, so a seat the
caller permits does not judge a native candidate. A candidate found unfaithful, or not settled
(an abstention, a failed call, a call the ceiling refuses), is withdrawn with its questions, its
requested boundary and its replayable record, and the request stays INCOMPLETE naming the part;
no repair round follows the judgment. A native outcome that is not READY in its authoring round
(a business question or the `model` placeholder open) is not judged there, and an answer round
that finishes a recorded native candidate (`native_replay`, then `native_apply`) is READY with no
call and no judge: that door is not judged yet.

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
synthesizes, and the value it returns is written as it is to a json or a prose file (the COLD
transform writes `${{ tasks.compute.output }}` to json and md alike), while to a csv, yaml or
toml file it passes first through a `nika:convert` stage from json, whose accepted input shapes
apply (no scalar conversion is promised). A shape the request names
overrides both as an obligation, and no other wrapper, key or field is added; the deterministic
compile does not read an explicit bare-number request (« write only the number »), which stays
INCOMPLETE, never claimed. Tests pin the text against the candidates the compiler emits, typed
and synthesized. A plan states a bare value itself (E38): a write effect's optional `alone`
(the plan schema and the seat's instruction) writes the one total the engine types to a json file
as its value alone (`${{ tasks.compute.output.<name> }}`), the computation staying typed under the
number law instead of a seat's program outside it. Several totals, a total no template selects,
or a csv, yaml or toml destination are refused with a finding and a clarification; `alone`
changes nothing where no object names the value (rows, a prose file, a seat's program), and it is
read on a write only. It is a plan claim like the others: the whole-request judge still reads
the candidate's bytes, and nothing reads it from the request's words. The typed computation
states, the same way, a tie rule (`ties: first_in_file`: rows with equal sort keys keep their
file order) and the output columns written as JSON numbers (`numbers`), in its strict schema
and the seat's instruction (E38 C3); `nika-compile-fidelity` admits them only where they hold
and the reader lowers them (a stable order, the number law).

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

The COLD plan and its evidence-citation repair send `opening(intent)` (the instructions
and the request text). The semantic verifier's repair also carries the bound observed world,
answers, original request and candidate in its untrusted state, and the compiler-owned
reference in its system message, as described above. The native opening carries the observed
world, answers, knowledge references and an edit's original request. Reasoning effort alone
does not add context to any of these calls; the COLD plan's context gap remains open.

### Response identity evidence

The shared authoring `Seat` counts an absent, empty or whitespace-only provider
model identity as unreported. It never substitutes the requested model. Nonblank
reported identities remain exact and deduplicated in first-observed order; the raw
provider response is preserved. This observation is not independent provider or
invoice verification. CLI, Session and native Serve use this same owner.

## Numeric conversion cardinality

Transform examples execute the same numeric conversion definition as the runtime; successful parsing alone never establishes semantic fidelity.

`tonumber` preserves a numeric input or parses one numeric value from a text input.
Empty or whitespace-only text, several JSON values in one text, and non-numeric values fail;
an enclosing aggregate cannot silently omit or double-count that operand. An explicitly
authored `try` or `?` still controls error handling. `fromjson` retains its stream semantics.
This cardinality correction does not promise arbitrary decimal arithmetic or a field name in
the generic error; typed numeric laws remain responsible for those contracts.
