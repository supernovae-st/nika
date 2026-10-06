# Crate spec — `nika-compile-cognition`

> Target direction amended 6 October 2026: [cooperative intent resolution](../architecture/ARCHITECTURE-0.123.md).
> The implementation descriptions and dated measurements below retain their actual scope.
> Historical HOT/WARM/COLD paths, BM25 selection and single-judge behavior are migration
> seams, not requirements to preserve as the target product architecture. This note does
> not claim that cooperative retrieval or incremental verification is implemented.

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

A completed seat answer may carry separate `Thinking` blocks beside exactly one final `Text`.
Only that Text reaches proposal, sketch/fill, source-recovery, transform and closed-choice
judgment decoders. Thinking never supplies missing answer bytes; multiple Text blocks, tools,
images and any other block kind remain refused, as do non-terminal stop reasons. The shared
projection borrows the response without changing the raw observation, receipt or usage.

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

The R4 A11 / E39 C3 whole-request check is the historical basis of native and
sketch verification. Its earlier author-only account, without a post-judgment
repair round, no longer describes all current paths. The selected decision seat
now reaches sketch verification and supported recovery/revision paths; the
[Semantic CREATE and resolved questions](#semantic-create-and-resolved-questions)
section below specifies selection, abstention, defect localization and repair.
A judgment reads the candidate's final bytes with the compiler-owned reference;
structural admission alone does not establish semantic fidelity. Preserve the
whole-request coverage and binding laws while qualifying the cooperative target.

The answer round of a native record is judged too (R4 A11, step 2). The native and the sketch
doors both record strategy `native`. The core's `native_replay` bakes the round's answers into
the recorded source with zero calls and keeps the whole request pending on a finish the laws
admit (`decision.pending`); a finish they refuse is returned as it is, and no judge is asked of
it. `verify::replayed` binds such a record to the reader's plan of the request,
as the core does. It then asks the round's judge the whole-request question, at most
`WHOLE_QUESTIONS` requests: a decision seat the caller permits, else the authoring provider
through the journaled call when the round's policy is bounded.
- A faithful verdict under the recomputed binding is READY, and the route says
  `verify: judged (<kind>)` after the replay.
- An unfaithful or unsettled verdict stays INCOMPLETE naming the part, with the candidate kept as
  the preview. Nothing is repaired in an answer round.
- With no judge permitted, the round is INCOMPLETE and the core's finding names the judge to
  permit.

A native revision's answer round (`revise`) takes the same door. A judgment is never read from a
record, so a transport whose native answer rounds made no call now permits a judge in them (one
request for a faithful verdict, two for an unfaithful one) or gets INCOMPLETE.

The current estimate is `authority::worst_case_of`. It counts author requests,
including an author-provider verifier, when the configuration alone supplies a
representable finite upper bound. With an explicit repair count `r`:

- a revision using a model: unknown (`None`), because its retained representation
  determines link, fill and repeated judgment work; `3 + r` was not an upper bound
  for semantic revisions;
- the sketch door: `2 + r + 2(1 + r) = 4 + 3r`, with each new candidate judged afresh;
- COLD creation (`off` or `escalate`): unknown (`None`), including with an explicit
  repair count, because the request determines its clause and transform work;
- a creation under `only`: zero requests; the core refuses the retired source-authoring route.

An arithmetic overflow or an absent repair count where repairs add requests also
returns `None`, never a saturated number presented as an upper bound. Paths with
no possible call retain zero. The deprecated `authority::worst_case` preserves
the old capped engine's formula and saturating arithmetic solely for compatibility;
current consumers do not use it to estimate uncapped work.
An absent request bound still counts calls, without inventing a numeric limit or a
monetary ceiling. A separately selected decision seat retains its own consultation and cost
observations; these author-call formulas do not price or count that service's requests.

Verification asks every pending clause, transform synthesis considers every unstated
computation, and native decoding retains every admitted business question. Positive
sample counts run as typed, with no hidden five-sample ceiling; zero is invalid. The
composer retains every distinct signature and feasibility result, with no eight-plan
ceiling; only feasible candidates reach selection, together with `NONE`. Its existing
structural expressibility law is unchanged. A real request-authority refusal stops
further sampling while retaining the attempts already recorded. The whole-request
protocol still has at most two questions: verdict, then localization if unfaithful.
Localization offers every qualifying distinct phrase of the request, including
phrases beyond the former sixteen-part cutoff, and maps the selected index back
to that complete phrase.
The former eight-clause and two-transform caps are historical behavior, not active
limits. Typed repairs under `off` are the verifier's and count, never ignored.
A typed strategy is honored in full or refused, needing
`authority::least_requests`:
- `escalate`: two requests (the plan, then its judgment);
- `sketch`: three (the sketch, its fills, then their judgment).
The refusal names that number. A fresh creation under `only` is refused by the core
with a migration to the semantic doors; no larger grant can enable it.

This removal does not claim unrestricted model context or complete R5 cooperation.
The current native Check diagnostic projection still keeps up to six conformance
violations and six error findings, and authoring knowledge excerpts retain up to
2000 characters per builtin section. The whole-contract verifier reference described
below is separate from those authoring excerpts. Their remaining context selection
needs its own coverage qualification; no complete-context claim follows from an
uncapped number of preparation calls.

Source recovery retains an explicit operator count (`AuthoringPolicy::source_recovery`,
default 0). When the policy has no repair count, an eligible stalled CREATE can also
open it without a separate recovery count; an explicitly bounded policy requires a
positive recovery count. It is not the retired `only` creation route: when the sketch door ends a
CREATE INCOMPLETE with no candidate and no open question (its structured rounds spent, no
progress, an answer that is not the sketch wire's, the evidence's defect with no round left, or
a candidate the whole-request judgment withdrew past the last round for a demonstrated missing
part; an unsettled or abstained judgment is not recovered), the same seat, in the same conversation,
is told the previous findings, the evidence defects no reopening carried and the last refused
candidate, and answers the whole source on the retired source wire
(`assets/native_answer_schema.json`, instruction `assets/native_source_recovery.md`). Each answer
faces `native::judge` (strict parser, pure Check, fidelity laws, admitted questions); a refusal is
repaired within any stated rounds; a repeat is no progress under the current recovery
law. An accepted source settles as a native
record (`native_apply`, zero-call replay, `native_pending` on answer rounds) and faces the
rehearsal and the whole-request judgment like any candidate; a defect reopens it while
the configured policy permits. Every request is charged to the same authority and
receipt (no reset, no other model). `authority::recovery_requests` describes an
explicit reservation (`3` per stated round); an unknown total remains unknown.
The fallback is recorded: route `native: source recovery after structured exhaustion`,
`decision.native.recovery` (rounds, spent, accepted by the laws, the structured findings) and an
`authoring_recovery` finding that the recovery opened; only a READY outcome adds the finding that
the recovered source passed every check (a refused request, a refusal, an open question or a
withdrawal never claims it). A failed or cut seat, a refusal, an open question and every edit
are never recovered: a whole-source rewrite would escape the revision's preservation laws. A
recovered workflow carries a native record, never a semantic record (none is fabricated), so a
later revision in words is kept or source-anchored (one destination), never semantic.

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

The COLD plan, its evidence-citation repair and the semantic verifier's repair carry the
admitted pack's references and callable descriptions, embedded recall, the reader's floor,
observed world and answers as untrusted context after the compiler's instructions. The
request remains the user's message and the merge anchors evidence on that request. The
verifier's repair also carries the original request and candidate. Each prepared Plan call
records its references and `semantic_context` pack/world digests; preparation is not proof
of delivery, reading, trust or benefit. The native opening keeps its corresponding context
and an edit's original request. Reasoning effort alone adds none of this context.

### Response identity evidence

The shared authoring `Seat` counts an absent, empty or whitespace-only provider
model identity as unreported. It never substitutes the requested model. Nonblank
reported identities remain exact and deduplicated in first-observed order; the raw
provider response is preserved. This observation is not independent provider or
invoice verification. CLI, Session and native Serve use this same owner.

### Forensic record (`decision.forensic`, v1)

A cognition compile offered a seat or provider projects one summary after Rehearsal
from the journals it already keeps: the door and its reason, the source owner, the
routes tried, request digests apart from proposals, the proposal kind and state
(`NOT_CAPTURED` for source-direct and replay), and the finite candidate universe
or `not_applicable`. Foundry references distinguish attached, prepared and
confirmed-presented: only an answered call confirms delivery; a provider failure
is `delivery_unknown`, and a local refusal is not sent. Calls retain their roles,
delivery and unknown usage as null. Evidence names the exact final bytes checked
or rehearsed, while the semantic judge lacks a candidate binding and behavioral
satisfaction remains `UNKNOWN` (`behavioral_judge: not_run`).

Each authoring call journals its response digest and, for a plan, the decoded
object. Accepted sketches and declared fills retain the closed form consumed by
the compiler and a count of ignored keys. Refused plans and sketches, and
undeclared or repeated fills, retain digest, shape and reason only. Transform
program proposals and an external DecisionSeat's own usage remain explicitly
`not_captured`. The record grants no authority and changes no routing decision.


Sketch, Fill, judged native and native ask round notes retain their digest and
shape through the existing `receipt::withheld` form. Refused Sketch and native
question keys and gaps use the same form, including refused native asks. Accepted typed question
keys and gaps remain available to their existing compiler consumers. A schema
(`Data`) decode failure is reported publicly by its fixed class and position;
its detailed error remains available to the internal repair. Syntax and EOF
errors retain the decoder's wording. Response digests, usage and refusal/error
classification are preserved.

This is a bounded public-journal rule, not global sanitization. Structural
findings may still quote task identifiers and literals; the native question
admission refusals and whole candidate remain separate paths. Admitted native
asks preserve the question keys and gaps needed by the human.
Private capture is host-owned and grants no authority to replay these records.
The `compile_forensic` sentinel tests exercise the covered paths with capture
disabled, beside a positive control that preserves a valid typed question.

## Sketch fills are validated before emission

The sketch door judges each fill round before a candidate is serialized or journaled: the fidelity
`fills_from_json` and `complete_document` laws over the accepted sketch, then each emitted
invoke's builtin contract (`nika_cap::builtin_shape_findings`, e.g. the closed `nika:fetch`
extract modes), with any fill string echoed by a finding replaced by `<proposed value>`. A
refused round records named `fill` diagnostics, the fills by digest only and no candidate or
candidate digest, and enters the same bounded repair loop as a refused candidate (same
`policy.repairs` cap, no reset; a repeated refusal still ends the talk). The schemas and the
sketch card state the same closed shapes and fill rules. Routing, defaults and the
assembler's output for a lawful fill set are unchanged.

Each emitted invoke's filesystem arguments are then bound to the stated reach of its own task
(0.123 A2): `nika_cap::unbound_fs_args(tool, args, task.reads, task.writes)`, the effect owner's
query, refuses a read slot not in the task's `reads`, a write slot not in its `writes` (both
for `edit`), and any present slot that is not a literal path, covering the primary
`builtin_effect` path plus `image_fx.input`, `chart.data.path`, `image_generate`
`image`/`images[]`/`mask`, a string `decide.bundle` and `fetch.multipart[].path`. Another
task's path is refused even where the sketch's derived permits admit it. This binds and refuses;
it does not make Rust derive those arguments, and its slot list proves names
(`nika-builtin` parity test), not that every runtime read or write is listed.

A reach no fill can repair is judged at the sketch phase instead (0.123 A2b), so the sketch
repair loop can fix the graph: each side `nika_cap::required_fs_directions` says a builtin always
reaches must be stated in its task's `reads`/`writes` (an `edit` in both, a `chart` in
`writes`), and every path the sketch derives for a task (the partial projection's arguments) is
bound by the same `unbound_fs_args` query. Optional slots and inline data are never required at
that phase.

## Sketch graph semantics (0.123 slice B)

The sketch answer accepts `outputs` (and the task controls `max_turns`, `tools`, `fail_fast`);
the schema, the card and the fidelity decoder state the same closed shapes. `consumed_sketch`
projects the outputs and every control as consumed, with a per-task `defaulted` list naming the
controls left to the historical emission, so a receipt never presents a default as requested.
An agent's `tools` must be effect-free for every call (`nika_cap::pure_internal_for_all_calls`);
a tool that can reach a file, a host or a process is refused at the sketch phase. Effectful
agent tools are the named remaining part of slice B, not supported by this lot.

A COLD proposal is classified before `Plan::push_step` folds its operations
(`cognition/proposal/occurrences.rs`): the merge runs exactly as before, and a proposal it
refuses stays refused; a lawful proposal whose source steps (`read`, `fetch`) each name a
distinct stated path that no write targets, two or more of them, whose write targets name two or
more distinct stated paths (a written path comes from the target only: a write's evidence that
cites the source it copies leaves that source a read source), and whose own cited texts pair
every read source with exactly one written path and back (a step's detail and evidence, or a
write's target and evidence, naming one of each) is `NeedsSketch` (private `Merged`): independent branches the plan's one step per
operation would merge. Path counts alone are not branches: two sources merged into one result
written twice carry no pairing and keep their plan. Several reads feeding one destination, typed
rule sequences and every other unpaired proposal keep exactly their prior merge result, which is
not thereby claimed sound. The pairing is lexical over stated paths, not semantic. When no sample yields a candidate and one
carried such a composition, the existing sketch door continues with the same intent, answers,
reading floor and receipt (the paid plan calls stay first), never source generation. Under
`native: off` the composition is named and nothing is sent. Escalate continues through the
sketch door with one repair less (checked); with no repair allowance the budget is named
and no sketch request is sent. `authority::worst_case_of` leaves this request-dependent
continuation unknown; the shared authority counts and enforces any explicit request limit.
The forensic summary names this door `sketch` with the
reason `plan_composition_requires_sketch` (from its route step); a composition stopped before any
sketch request (native off, no repair allowance) keeps the plan round's own record, door `none`
with `cold_plan_without_candidate`, and the finding that names the stop. Remaining limits: the
classification is lexical over the request's stated paths (a host, a URL or an unstated path is
never a branch, and a target naming two stated paths cannot pair); a lawful proposal without the
pairing keeps its plan, which is not thereby judged faithful; agent tools with effects are
refused at the sketch phase, not carried.

## Numeric conversion cardinality

Transform examples execute the same numeric conversion definition as the runtime; successful parsing alone never establishes semantic fidelity.

`tonumber` preserves a numeric input or parses one numeric value from a text input.
Empty or whitespace-only text, several JSON values in one text, and non-numeric values fail;
an enclosing aggregate cannot silently omit or double-count that operand. An explicitly
authored `try` or `?` still controls error handling. `fromjson` retains its stream semantics.
This cardinality correction does not promise arbitrary decimal arithmetic or a field name in
the generic error; typed numeric laws remain responsible for those contracts.


## Semantic replay and request basis (0.123 slice C-core)

`compile_with_cognition_rehearsed` reads the caller basis on the raw request before money is
blanked (`surface::semantic::caller`), refusing with no call a semantic record its caller cannot
replay, and binds the captured basis into the record a door just produced before any rehearsal
(`sketch::bind_caller`, which keeps the record only when the core's own replay of it reproduces
its final binding; a replayed record keeps its original basis forever). The sketch door computes
`request_basis` before its first proposal, carries the accepted graph and fills from `fill`
itself (never from journals) and builds the closed record; when it cannot be built or does not
replay, the round is INCOMPLETE with a finding and no retry. An answer round of a semantic record
dispatches before the hello, template and no-seat shortcuts: the core rebuilds it, then the
reach laws only this crate reads (`nika_cap`) are re-run on its graph and fills — a refusal is
static, nothing is judged — and the whole request is judged in that round by an admitted judge,
a counted call, or stays pending. Serialized judgments are never read. The rehearsal reads a
semantic record's answered paths through the core's validated rebuild. Effectful agent tools,
raw rejected-text capture on disk, host adapters and semantic EDIT remain open work.

## Authoring answer observation (0.123 slice C-core)

`observe::observe_authoring(sink, future)` scopes a host's observer (tokio task-local) around its
own compile future. `call_with_schema` emits one `AuthoringObservation` per authoring or repair
call (plan, native, sketch, fill, their repairs, transform; never a judge) after the response and
before any decode: a scope-local ordinal, the role, the prompt and schema by identity only, and
either every Text block in order as received at `compiler_provider_response` with the count of
other blocks and a private identity over the length-framed blocks (distinct from the public
digest of their concatenation), or `NoResponse(Timeout | AdmissionRefused | ProviderError)`. A
response with no Text block is an empty block list, never confused with a failure; a request
never built before dispatch makes no call and no observation. The observation is borrowed, not
`Debug` or `Serialize`; the public receipt keeps withholding refused text and unknown keys,
and candidate, authority, call counts and outcome are identical with or without a sink. Received
is not persisted: no file, cap, redaction or retention lives here, and no crash durability is
promised. Open (C is not closed): the common private writer and the CLI, Session and Serve
adapters, each with an exact rejected→repair artifact and privacy/cap/isolation evidence.

The same observation carries the ordered prompt role/block shape by digest and
optional counts (messages, Text/other blocks and UTF-8 Text bytes), plus the
existing instruction identity. An unrepresentable shape remains unknown. Usage
presence, input/output/reasoning token counts, returned model and closed stop
reason are optional facts of that response; missing usage is not reported zero.
Provider model text and an unknown stop's detail still require host admission.
`TextBlocks` exposes only borrowed Text iteration, length and emptiness, with no
public constructor, `Debug` or serialization. `NoResponse` is not evidence of
zero spend, and a dropped scope promises no closing observation. These additions
do not persist raw answers or implement the private writer and host adapters.

## Semantic CREATE and resolved questions

Fresh Escalate proposes Plan and, when no candidate remains, Sketch; Rust assembles the
ordinary workflow. Whole-source authoring remains available for supported EDIT and
historical replay, and as the eligible CREATE source recovery described above. Its
success does not prove that structured composition succeeded. After a Sketch passes structural laws, the existing admitted-question
check removes questions already resolved by the request or observation before fills are
requested. The full check after filling remains. This adds no repair grant or second compiler.

The selected decision seat also reaches the sketch whole-request verifier, its source
recovery, and semantic or source-anchored revisions. Generation and repair remain with
the author. The existing `verify-request` choice (`faithful`, `unfaithful`, `none`)
and, after `unfaithful`, `verify-locate` name the defect consumed by the existing repair
loop. An absent decision seat retains the journaled author-provider judge. An explicitly
selected seat that fails, abstains or returns an unoffered option never falls back to it:
the candidate stays incomplete. Deterministic refusals still ask no seat.

Each consultation stays in `decision.semantic_verification`: selected judge identity/kind,
compiler-owned reference, question IDs and offered choices, returned choice or failure,
attempted/returned/consumed counts and reported usage. Questions read the final candidate bytes. A repair or source recovery
retains prior verification attempts. Unknown usage stays incomplete, never a zero-cost
claim; the host decision observation owns its separate settlement. These are typed semantic
judgments, never permission to Save or Run. The current record does not include Foundry retrieval decisions; wiring that role belongs to the cooperative target, and requires actual selection and consumption evidence.
