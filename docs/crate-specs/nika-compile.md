# Crate spec — `nika-compile`

| | |
|---|---|
| Status | **MEMBER** (size-cap split of the admitted `nika-onboard` unit · ADR-137 · D-2026-07-09-N1 · 2026-09-21) |
| Layer | L4 — a library surface; lateral L4→L4 edges `nika-onboard → nika-compile`, `nika-compile → nika-compile-reader` (ADR-138) and `nika-compile → nika-compile-fidelity` (ADR-141), never back |
| Design | the stateless Compile core: one `CompileRequest` in, one `CompileOutcome` out — deterministic HOT admission, native record application, the assembler, the Check preview, the recorded-plan replay across answer rounds; the deterministic reader (frozen: a safety floor) and the private typed semantic plan (operations · effects · obligations · constraints · typed computations) live in `nika-compile-reader` since ADR-138 |
| IMPL | measured by `scripts/crate-metrics.sh nika-compile` at each freeze; the crate carries what `nika-onboard::compile` carried on 2026-09-21 minus the reader and the plan, descended to `nika-compile-reader` the same day (ADR-138 · the gate's own counter: 9,778 prod LOC after the split · 52 unit tests · 15 integration suites) |
| LOC budget | ≤15k crate · ≤1500/file · ≤100/fn (the frozen reader's tables and their `lookup-table` LOC-EXEMPT live in `nika-compile-reader` since ADR-138) |
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
// nika_onboard::compile explicitly re-exports the supported core API and:
// Cognition, NoProvider, compile_with_cognition, compile_with_provider, decide.
// The member-only surface and outcome-building helpers remain in nika_compile.
```

Every caller keeps writing `nika_onboard::compile::…` (`nika-cli-host`, `nika-serve`, the
MCP integration). Existing explicit facade imports remain available. ADR-140 adds a
shared Rust surface and moves direct core seat exports to `nika_compile_cognition`; see
the current member boundary below. This is not a claim of an identical public API.

## 2. The boundary, measured

| direction | edges before the split |
|---|---:|
| `nika-onboard` (surface) → `compile` | 1 (`lib.rs` declares the module) |
| `compile` → the rest of `nika-onboard` | 2 (`intent::STOPWORDS`, `banner::sentence`) |

The two borrowed helpers moved into `nika_compile::text` (`STOPWORDS`, `banner_sentence`,
`banner_lines`, `is_label`) and `nika-onboard` re-imports them. The member never depends
back on the surface.

## 3. Contracts kept

- The split retains the machine wire projection and recorded-plan format. The Rust
  request surface grows under ADR-140; it is not byte-identical API text. Zero-call replay,
  candidate semantics and provenance remain contracts to verify on the composed source
  with the compiler and transport regression suites.
- The deterministic reader is frozen: no cue or head is added; every new law is a structural
  one (path boundaries, anaphora, the shape of a human gate, the carrier of a constraint) or
  lives in the typed semantic plan.
- The 12 ADR-003 gates were passed by `nika-onboard` at its admission; this member inherits
  them as the second half of the same unit (the ADR-115 precedent). Mutation and property
  attestations for the compile core are owed as pending evidence, tracked with the season-2
  debt wave, never claimed.

## 4. Current member boundary (ADR-140)

`doors` owns deterministic HOT admission, plan/native replay and record application.
Record application completes a native candidate's boundary only from its answers: an
answered endpoint's host (`permits.net.http`), and an answered bare `${{ const.<slug> }}`
path, which replaces the seat's empty `permits.fs` placeholder (`[""]`, flow or block
`- ""`) in the direction the capability inference derives — never a path that escapes the
workspace, never a glob, never a direction the seat declared with any other entry.
A replayed HOT record receives the reader's unnamed-destination floor again (idempotent), so a
record written before that law asks the path of the write it lacked (« … dans un fichier »).
The assembler's `const.output_path` question stays open when the answer names no file:
prose, like a directory, a glob or a placeholder, is refused and asked again, never dropped.
`assemble`, `bindings`, `approval`, `laws`, `ledger`, `network`, `realize`, `support`,
`trigger` and `writes` implement deterministic compilation. `edit`, `edit_source`,
`materialize`, `retrieve`, `types`, `pattern` and `wire` retain their existing roles.
`surface` states the contracts used by the cognition member, including the one spec pin
reader and the assembler laws its tests compare against.

The COLD composer, proposal decoder, predicates, decision seats and native/sketch/transform/
knowledge authoring now live above this crate in `nika-compile-cognition`. Core depends on
Reader for the frozen reading/plan and on Fidelity for deterministic candidate laws
(ADR-141); it never depends on cognition in production. Kernel/Tokio are development
dependencies for the unchanged compiler suites. The supported onboarding facade combines
core and cognition; the direct core crate's seat exports move to the cognition member.

The split is implemented under the authorized consolidation. Compilation, complete suite
execution and canonical API qualification remain integration checks; the inherited
mutation/property attestations remain pending. Historical counts above describe their
recorded revision, not current test or size results.

## Exact schedule requirement

`requested_trigger.cadence` remains a coarse label. Additive nullable `cron`
contains the exact five fields of the existing cadence grammar when a bounded
FR/EN trigger phrase is complete. The original `source_hint` stays evidence;
a `trigger.cadence` answer is projected by the same reader and survives plan
replay. Neither field is a grant, timezone choice, project row or execution.
Older machine documents lacking `cron` remain readable by tolerant consumers;
absence must never be reconstructed from the coarse label for activation.

Supported exact forms are daily/weekday/single named weekday with an explicit
clock, and every N hours/minutes where N divides 24/60 (digits, plus one/two in
FR/EN). Examples: every Tuesday at 09:15, chaque vendredi à 18h30, toutes les
deux heures. The interval phase is zero on the local clock, not elapsed time;
the activation review displays it and the canonical scheduler owns DST. Missing
time/weekday, conflicting periods/clocks, mixed periods, monthly/alternate-week
recurrence and non-divisor intervals retain their words and yield null `cron`.
This does not claim unrestricted natural-language cadence understanding or add
another cron parser. `nika-cadence` alone validates/executes the bound expression.

A period five cron fields cannot hold is no coarse label either (R4 A5 · C1):
an alternating or counted day-or-longer period (« every other Monday », « every
2 weeks », « un lundi sur deux », « tous les quinze jours », « biweekly »), a
frequency (« twice a week », « deux fois par semaine ») or an hour/minute
interval that does not divide its day or hour (« every 5 hours ») keeps a null
`cadence` and a null `cron`, keeps its time of day, and asks the mandatory
`trigger.cadence` question before READY. Only the human's explicit replacement
cadence or « manual » resolves it; another unbindable period is refused as an
answer and the question stays. Weekly, weekday, daily and dividing-interval
schedules are unchanged. A clause the reader does not take as a trigger stays
an unresolved clause, never READY.

## Money words

`money` is the one lexical money reader (R4 A6, moved unchanged from `nika-session`):
what a line states about a USD ceiling, its exact amount token, a default it replaces,
whether it states money only. Quotes and path tokens are data; only a currency or a
monetary anchor gives a number monetary meaning. It recognizes; it never admits: the
caller's money gate decides what an amount allows.

A work request states money only in its directives (`money::directives`, R4 A6 · B15). A directive is an explicit act about the work's own money, in one of two closed forms:
- a whole sentence or comma segment made of money words only (« Budget: $0 », « budget=0 », « Le budget est de 2 dollars », « --max-cost-usd 0 »);
- the phrase that ends a segment and attaches to the work: a connector, its articles and a limit anchor (« … ./out.csv with a budget of $1 », « … avec un plafond de 3 dollars »), or the limit anchor alone (« hello budget 2 USD »), then the amount and its currency and nothing else.
A phrase attaches to the work when its head, past the determiners, is a path or a file, a pronoun, a greeting or consent word, a skeleton name opening its segment, or a conjunction no relative clause governs. Everything else money-shaped is business data, by its role and never by a word or an observed field. That covers a predicate over records (« rows whose budget is 1500 USD », « rows where cost is under 5 USD », « rows with a budget of 1500 USD »), a business amount (« refund the cost of 50 USD »), a negative value, cost wherever it stands, quoted text and paths. Malformed, negative, non-finite and conflicting directives refuse. The French copula links a directive's words only in a sentence of its own. Run and gate lines keep the whole-line reading.

A caller that admitted directives as its own ceiling names their exact spans
(`CompileRequest::with_admitted_money`). The deterministic door then reads the request with
them blanked (same bytes, same offsets), never as business clauses; `decision.money` records
each directive and `decision.intent_sha256` stays the original request's; a diagnostic says the
ceiling is the caller's and that the compiler certifies no cap. A span that is no directive of
the request refuses. A directive with no currency whose anchor names an observed field
(« budget=0 » over a `budget` column) is not blanked and is asked. A request that names a
skeleton only once blanked (« hello budget 2 USD ») is read as written, with its money
record retained; no seat reads that ceiling as work. A door that meters no seat states its
operator's money instead (`CompileRequest::with_stated_money`, the CLI): every directive of
the words the compiler reads is admitted, including a creation's replacement request, never
the words it replaced. A revision's admitted spans index its exact change text. On a door
that states money, its change and the original request its base answered are both read;
records from the latter carry `in: original_intent`. A replacement answer never replaces a
revision's change. `surface::admitted` exposes the same reading to the seats' door, which
applies it before every strategy (see `nika-compile-cognition`).

## Numbers a bound rule reads

Every field a bound rule reads as a number (a numeric comparison, a sum, an average, a
minimum, a maximum, a ranking's key) is read under the reader's one number law (R4 A5): the
binding states its policy on the rule (`observed::numbered`), FAIL unless the request grounds
another, and records each in the decision (`decision.numbers`: rule, field, policy, what
bound it). A candidate therefore never parses a number with jq's lenient `tonumber` (which
reads « 1,5 » as 1, « Infinity » as infinite and an empty text as nothing) nor ranks by jq's
total order (null lowest, any text above every number): a value that is not a number stops
the run with the field and the value named, before any write that depends on it. An average,
a minimum or a maximum over no number stops the run too, never 0 or null, and under SKIP an
average divides by the numbers it kept. The plan record keeps the reader's own reading, so
plans recorded before replay unchanged; the decision's `rule` is the bound computation the
candidate runs.

Over one observed source the policy is grounded in the raw kinds the host counted
(`world.kinds`, `observation.rs`, R4 A5): a numeric field whose sampled values include
anything that is not a number (null, missing, true/false, text, empty, a list, an object)
asks a mandatory closed choice before READY (`const.rule_number_<n>`: `skip` or `fail`),
naming how many sampled records hold what; a field observed as numbers only (a JSON number, a
decimal or exponent text such as « 1.5e2 », zero) asks nothing and reads under FAIL, so a
value the bounded sample did not show stops the run by name. The answer binds the source's
revision: a changed row or changed kinds (a type-only change included) asks it again. That
question also answers what S1's partial-presence obligation would ask for a missing numeric
key, so it is never asked twice. A plain sort over a key observed as numbers only reads the
law; over any other key it keeps its legacy reading. `decision.numbers` records per field the
source, revision, sampled count, kinds, policy and what bound it (`answer`, `pending`,
`observed numbers`, `unobserved`). A recorded plan is grounded again on every replay: over
observed non-numbers it is asked, never READY without a stated policy. A verified seat
program (a pending transform's) keeps its own bytes and reads no policy.

## Canonical spellings of a stated text

Text is compared byte-exact everywhere (jq equality, the candidate, an answer). A text
equality over one observed source is grounded by the bounded canonical-spelling expansion
(`observed/spellings.rs`, R4 A5): where the observed categorical values of the compared field
hold a spelling that differs from the stated literal only by Unicode canonical equivalence
(NFC, « livré » typed precomposed against a file's e + U+0301), the equality also matches
exactly that observed spelling (`Rule::with_spellings`), and the decision records the law, the
field, the literal, the spellings, the source and its revision (`decision.spellings`). Nothing
is normalized at run and jq equality is unchanged: no case folding, no compatibility (NFKC)
folding, no accent stripping. A spelling the bounded sample did not show, or a field the
observer did not find categorical, stays byte-exact and is not claimed matched. The expansion
lives in the binding, never in the plan record: a recorded plan replays with the same bytes and
is expanded again from its fresh observation; a record that carries spellings is refused.

## One record by identifier

A lookup by a literal identifier (« Look up ticket 42 in ./tickets.json », « find ticket 42 » in
a file the request reads) asks which field of the records holds it, among the fields every
observed record carries, and selects through `laws::SELECT_BY_FIELD` (R4 A7). The identity
relation is unchanged: the field equals the identifier as a string, or is a number whose
canonical text equals it. The law returns the ONE record that relation matches:
- copies equal as JSON values (key order aside) are that one record;
- no match yields `null`, which the lookup's admit refuses before any effect;
- matches that differ — two records with id 42, or a string `"42"` and a number 42 — stop the
  run at `lookup_record` with a jq error naming their count, the field and the identifier.

Input order is never a reason to pick one: no first or last winner is inferred, and order words
in the request (« the first », « the latest ») ground no ordering, so a duplicate still stops the
run. Every effect of the workflow (writes, posts) waits for the record and its admit. The check
lives in the run, not the compile: the observation is bounded and quotes no value, so it cannot
certify that a whole file holds one match, and the source may change between the compile and a
run. An object directory yields its keyed entry, and the per-invocation lookup
(`SELECT_BY_KEY`, keyed by `inputs.record_id`) is unchanged.

## Exact order of the numbers a bound rule reads

Every number field of a synthesized rule is bound under a policy: `bind_computation` passes the
rule through `observed::numbered`, which gives FAIL where nothing stated another. This is the
one site that makes a `RuleBinding::Synthesized`, whether the compile is fresh or replays a
recorded plan.

For such a rule, the reader emits exact comparisons, rank keys and rank cuts (R4 A8, reader
spec). `laws::with_decimal` puts `laws/order.jq` in front of any compute that calls these laws;
a rule that reads no number carries none.

The plan record keeps the reader's unbound reading, byte for byte. A plan recorded before R4 A8
therefore replays READY, and its candidate binds the exact laws.

Unbound, as before:
- a seat's verified program;
- an answered `const.rule_expression`;
- a plain sort over a key that is not observed as numbers only.

## Numbers past the parse: exact or stopped

A JSON source's numbers reach the next task through the engine's JSON transport between tasks
(serde_json with `float_roundtrip`, no `arbitrary_precision`). An integer within [−2⁶³, 2⁶⁴−1]
passes as itself; anything else passes as the shortest text of its f64. Before R4 A8 that
changed values in silence, with exit 0, in every candidate that decoded JSON:
- a >u64 identifier (`123456789012345678901234567890`) became `1.2345678901234568e29`;
- a fine decimal (`1.000000000000000001`) became `1.0`.

The decode is now guarded (`laws::guarded_parse`, the `dguard` law of `laws/order.jq`). Every
number the next task may read or write keeps its exact value through the transport, or the run
stops at `parse_source` before any effect, naming:
- the number's path;
- its value;
- what the transport would have made of it.

What is kept is the value, not the spelling: as before, `1.50` passes as `1.5` and `1e2` as
`100.0`.

By default the scope is every number (`Bindings::guard_scope`). It narrows to the source fields
a rule reads (`Rule::source_fields`, the columns it writes included) only when both of these
hold:
- one synthesized rule is the records' only consumer: no per-record classification, no
  endpoint payload, no join;
- the rule fixes what it writes: named columns, groups or totals.

Then a precise payload the rule drops (a >u64 id, a fine-decimal weight) never stops the run.
The check and the name in its refusal obey the same scope, so a field outside it never decides
and never appears.

Whole rows (a rank, a filter), a copy of the records and any payload keep the whole-document
guard. A lossy number in a row that is not kept may therefore stop the run: a conservative
refusal, never a silent change.

Lookups are guarded the same way, over every number of the record they select:
`SELECT_BY_FIELD`, `SELECT_BY_KEY` and the support composition's customer lookup.

Other sources:
- **CSV:** converted to text cells; it needs no guard.
- **YAML or TOML:** the numbers are parsed inside `nika:convert` before any law can see them.
  This is a known limit, owned by the builtin.

The laws are jq that the one runtime runs.
- `laws/order.jq` is readable source, counted with this crate: Rust and jq together stay
  within the crate budget.
- It carries no regular expression.
- It defines nothing global: each guarded expression carries it in front.

## Observed fields and pending transformations

Source observation distinguishes absent, unreadable, empty, unknown and observed
material. An observed field choice is grounded in that source; a partial sample
is not a complete schema. A missing field asks a closed clarification rather than
silently selecting another key or returning an empty result.

Every source key a typed rule reads over one file is grounded by one law
(`observed/grounding.rs`, R4 S1) on creation and replay alike, and the decision
records exactly what it decided (`decision.grounding`: rule, key, source, revision,
grade, whether every sampled record holds it, what binds it, admissibility, an open
obligation). Grades: `declared` (a CSV/TSV header names the column), `observed_complete`
(a host that read the whole artifact), `observed_partial` (a bounded sample shows the key:
presence, never absence, and the revision is the peek's hash, never the unread tail),
`user_asserted` (the request's own column list, or an answer given in the context it was
asked), `inferred` (anything else). A key is admissible only above `inferred` and bound by the
request's own words, an answer or an approval: with nothing observed, a key the request merely
names is asked for its exact spelling, never lowered, unless the request lists its columns. A
word the observed keys do not hold is a closed choice over every observed key (the partial ones
included); no synonym, spelling or similarity maps it. An answer counts only against the
revision it was asked for: a replay whose fresh observation differs from the recorded one
refuses it as stale and asks again. The answer to a question the conversation asked (the recorded observation held
no admissible key for the word, or the plan's `reasked` names its key) is read before the fresh
observation's own words (R4 A6): a column renamed to the request's word is asked again over
the fresh keys, never taken silently, and an answer no question asked stays unowned. The
outcome's plan is re-anchored to the observation the question showed (`observed_world`,
`reasked`): a caller that replays it binds the next explicit answer there, and a later change
asks again. A number-policy answer over a plan recorded before kinds were observed says so
and is asked over them. A key some sampled records lack is grounded, but what the
rule does with those records (missing or null values compare, sort and total differently) is an
operator law the request must state: the rule stays pending with that obligation named, never
a default. The world is an input, never a CLI exception: a host supplies what it observed
(`nika compile` observes the files every free-intent request states, under its working
directory, on the deterministic door as on the authoring one; named skeletons read none), and
a library or Serve caller supplies its own as `knowledge`; with neither, the law above asks.
Not covered: nested paths, joins and folders of several files, the keys of a native-authored
candidate's own jq, and every value or operator meaning (units, types, dates, null comparisons:
a key present with a null value is grounded, and a numeric comparison over the null fails the
run: S2 and S3).

A pending transformation preserves intent, plan and observed source identity,
field choices and bounded attempt lineage across answer rounds. Record replay
rejects changed or malformed context as `PendingTransformError`, rendered through
the existing authoring outcome diagnostic. A verified transform is reusable only
for the same captured context. This record is neither an executable grant nor
permission for a new source or Run. If the host supplies no fresh observation,
replay can validate only its captured source, not assert current filesystem identity.

Each unnamed output keeps a stable path question across rounds. A single unnamed
output retains `const.output_path`; several use numbered keys in plan order. One
answer cannot fill two different outputs, and an answered file already assigned
to another output is refused. Content fidelity checks data edges separately from
ordering or guard edges; this structural floor is not a general proof of semantic
correspondence between every producer and every requested result.

The assembler owns one compute binding. Independent typed computations feeding
several files leave that binding unresolved instead of reparsing their joined
descriptions into one partial rule. The normal authoring escalation can generate
the complete native task graph. A typed rule for the whole detail retains its
deterministic path, as does a single-output pipeline whose parsed rule retains
every recorded typed stage in the recorded order: a part is found only in the
step of the part recorded before it or in a later step (R4 F5), so an inventory
holding every stage in another order is refused.
The operations the request states are witnessed (R4 A3). Each plan rule, step
detail and step evidence whose text is an exact excerpt of the request anchors a
part, read once by the one grammar over
the binding's columns: the widest readable excerpt stands for excerpts inside it,
and an unreadable excerpt counts only where no readable one overlaps it. A rule
holds the parts when their operations (each filter clause, an « or » of clauses
as one, each count with its grouping, each sort with its key and direction, each
cut with its size) appear in its lowered sequence in request order with those
parameters. A proposal that does not hold them yields to a reading that does
(the step's detail, its evidence, or the parts' excerpts joined in request order).
When every part is read, the lowered rule must also contain no extra operation
shaping the rows. A summary computed beside the rows in its own task is judged
only when stated. Without a fitting reading, missing or extra row operations
remain unresolved. With an unread part, extras are not judged and that part
stays unverified. This does not prove arbitrary model-generated computations
semantically correct.

The lexical reader supplies hypotheses for effects it cannot settle: an indirect negation
does not become a ban, and an undecided effect is not an obligation to execute. Native
authoring sees those open readings separately from settled constraints; any realized
uncertain effect is stated in review. Literal-targeted bans retain their own scope,
including relative paths and referenced constant destinations. These readings grant
no permissions: Check, exact-byte review, consent and runtime admission still apply.

A recorded plan is data the caller hands back: where one enters (`doors::replay` after
anchoring, and `PendingTransform::load` before a continuation), every recorded rule must be
re-derived from its words by the law that created it (`binding.rs`, R4 S0; the E14
near-misses of rounds 1 to 5). A rule whose words the closed grammar reads must equal that
reading under the request's column hint, the observed columns or none: every clause, junction,
flag and shape key, so a record that changed a value, comparator, field, junction, clause,
aggregate, key, direction or limit, or dropped a key, is refused and the rule named. A line
filter must be what `line_filter` reads. A verified program may stand only where the grammar
reads no typed rule. A seat's typed computation must be the fixpoint of its admitting law
(`nika_compile_fidelity::predicate::rederives`). No field of the record selects a weaker law,
and a rule nothing re-derives is refused by name. A recorded written literal (a `content`
binding) must be one the reader itself reads from the request as a write's content
(`binding::unread_content`, the reader's own literal law re-run only when a record carries
one): a record never writes text the request only quotes, matches or names. An identity
(no clause, stage, program or flag) is a conversion's and is bound only where the reader reads
its words as that very conversion, never by the seat's law: a conversion replays on its
answer round, and a record that replaced a filter by the identity (every row kept) is refused. **Open, not closed by this law:** a seat's
typed computation is bound to the law that admitted it, not to what its words mean. That law
grounds a value, a number or a limit in the clause that states it (a schedule's hour is
another clause's), a field among the request's columns, and a comparator, an aggregate, a
junction or a direction nowhere. Where the reader reads the rule's own words, the witness
(R4 A3) catches a replaced comparator, direction, cut or step order; an aggregate or a listed
column the law admits can still replace the recorded one unseen (pinned by
`compile_rule_binding`). No meaning closure is claimed.

An effect the request's own words both ask for and prohibit (the reader's `Conflict`) stays
the human's (R4 S0): `surface::assemble::refuse_contradiction` refuses it with both clauses
quoted as excerpts of the request, a `RequiresHuman` finding and one `intent.clarification`
question, no candidate and no unrelated model, endpoint or path question. It is the one
refusal every door states; no seat reads the contradiction to choose a side. The deterministic
door states it also when its reading holds a clause it cannot settle: beside every unresolved
clause, never with an authoring model offered to resolve it. A refusal's decision record
carries the stated ledger (`decision.ledger`): the contradicted effect, the unsupported work and
every clause beside them, typed with their states, not only the plan and the sentence. This covers the
contradictions the reader recognizes (a request and a ban of the same effect, object or
destination, and a ban of every write), not every semantic incompatibility a request can hold.

The READY law realizes typed duties against the emitted computation (R4 A3).
`decision.ledger` states one duty per operation the parts state (`filter`, `count`,
`order`, `limit`, each with `position`, its place in the stated order, and `reads`,
the fields it reads), anchored on the part's excerpt, in place of the compute
step's generic filter duty. `compute` realizes one only when its expression is
the bound rule's lowering byte for byte and the rule holds the operation at its
place; `compute_summary` realizes the summary stage's count only when it runs the
summary law. When every part is read, each extra row operation is an unresolved
duty on the step evidence, with the fields it reads. A task id, record flag or
plan annotation realizes no typed duty.
Words the grammar cannot read state no typed duty: their excerpt stays a
transformation duty carried by the task, noted unverified; an answered program's
filter duty is noted unverified too. `plan.obligations` is unchanged, and no
closure is claimed: aggregates other than a count, groupings, projections,
renames, distinct keys, joins and derived values remain transformation duties.

Constant work stays deterministic (R4 S0, G2). A write the reader states with a quoted literal
as its content (`Plan::content_of`) is lowered to the existing `nika:write`: the literal's
exact text (`text::quoted_literal`) is baked into the candidate as a constant
(`const.output_content`, `const.<stem>_content`), a value the run never renders as a template,
and reaches the write through `with.content`. No draft, no `infer`, no model question; with no
step and only stated literals, no invocation item is declared. A gated write reviews that
exact content and waits alone, after whatever the workflow read or computed; a banned or
contradicted write is never emitted. A structured destination, an unquoted object, two
literals or a transformation of one keeps its question.

With attached authoring references, Escalate tries complete HOT and finite WARM judgments
first, then gives the first open generation the native language card, original request,
answers, observed world and selected references. It avoids a preliminary private-plan
call that cannot consume that context. Native repair progress compares both candidate
identity and diagnostics; changed candidates may use the remaining bounded attempts.
A technical failure retains the request and round candidates instead of requesting a
replacement intent. An optional initial output limit can increase after a reported
truncation, using the same repair count and never exceeding the original hard limit.
Per-call receipts record the output limit, timeout, elapsed time, stop reason and usage.

## Source basis of a candidate

`basis_for(request, decision, fresh)` (`observed/basis.rs`, C9 · F4) judges the source facts a
candidate's decision recorded against a fresh host observation of the same sources, by the
grounding law that admitted them; `Basis::sources(decision)` names the sources to observe again.
The host keeps the actual compile-round `CompileRequest` beside the exact proposal bytes,
including its answers, plan and observation; the fresh observation is a separate input. A
complete creation replacement is folded with the same literal-answer and apostrophe laws;
a revision's change is never replaced by such an answer. The host clears old answers and
continuations when replacing the request, then retains genuine subsequent answer rounds.
A column declaration asserts a key only for the request's one stated source. Answered
assertions are re-derived by an effect-free, zero-call replay of the actual request, using
the existing grounding, plan anchoring and stale-observation laws. Incoming `user_asserted`
or `bound_by` labels supply no assertion. The legacy `basis(decision, fresh, intent)` accepts
source-bound column declarations only. This re-derivation judges recorded source dependencies;
it does not establish that the whole candidate is equivalent to the request.
A host calls it where a proposal is consented to, so the program consented to is still the one
those facts justified. The dependencies are every `decision.grounding` key (whatever its
`grade` or `admissible` labels say: each is graded again, never trusted) and every
`decision.numbers` policy chosen from observed kinds alone (`bound_by: observed numbers`). A key
renamed or removed from a declared header, a source now absent, unreadable, outside the project
or showing no record, a key every sampled record held and some now lack, a key no longer seen in
a partial sample, or a number field whose sampled values are no longer all numbers (`number`,
`number_text`) moves the basis (`Basis::Moved`, each in words: source, key, then and now). New,
removed or reordered rows, another column order and a new peek hash hold it (`Basis::Holds`,
with how many dependencies were judged): that is data the program reads at run. A key the
request asserted over a source never observed holds while the source is still not observed, and
a bounded sample never disproves it. Each dependency is matched to the one fresh row of its
exact source (`./a.csv` and `a.csv` are one path; two rows, no row or another source's row are
no answer): a dependency the fresh observation does not cover, or a record naming no source or
field, is `Basis::Unjudged`, never assumed to hold. A decision with no recorded dependency has
no basis (`Basis::None`), which is not a proof that the candidate reads nothing. The recorded
wire shapes are read as they are; the law adds no field to them. A present numeric record
with no `bound_by` or an unknown discriminator is unjudged, never discarded as an absent
dependency. Recognized policies not chosen from observed kinds keep their existing treatment;
this shape check does not prove the truth of a recorded discriminator. Not covered: the canonical
spellings a text equality matched (`decision.spellings`, a bounded sample law), and anything a
fresh observation cannot see (the unread tail, values that only appear at run): the use-time
guards and number policies of the lowered program stay the last check.

## Door cognition, knowledge and reproducibility

`provenance.cognition` names the cognition an outcome used. `deterministicOnly`
made no provider call; a model-assisted outcome carries its per-call receipts.
`nika compile` permits a model only through `--authoring-model` and a decision
seat only through `--decision-model`; an ambient key is never consent. The
Session reaches the same core under the intelligence the human chose: an API
or a local engine can author, and with no intelligence the door stays
deterministic.

Knowledge is attached only when named: `--knowledge` or `NIKA_KNOWLEDGE` for a
snapshot, and on the CLI `--knowledge-pack` or `NIKA_KNOWLEDGE_PACK` for a pack
composed for one request. There is no default location, and release archives
carry no snapshot. Without one, native authoring composes the embedded
language card with the request, answers and observed world.

A deterministic candidate follows from the request, answers, observed sources
and engine version. On the installed 0.120.3 candidate (`4c728c980`), the same
filter request (« Read ./data/orders.csv, keep only the rows whose status is
paid, and write them to ./out/paid.csv. ») compiled deterministically through
the CLI and through the Session. The two files differed only in the `nika:` id
their destinations named. That is one observation, not a proof for every
request. A two-output variant with a stated total stayed `incomplete` without
an authoring model. A model-assisted candidate is not reproducible across
calls. Its recorded plan replays answer rounds with zero calls, and the
reviewed bytes are what runs.
