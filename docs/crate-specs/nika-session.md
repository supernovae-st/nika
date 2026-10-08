# nika-session — crate spec

| Field | Value |
|---|---|
| Status | **WIP → ADMISSION** (One Door · wave 4 · ADR-125). In `workspace.metadata.diamond.wip` until the 12 gates land (Gate 5 mutation and Gate 11 swarm owed). |
| Layer | **L4 — interface** (the human's terminal) · a host runtime over the installed engine · **sync** on the terminal, one current-thread runtime per inference · lateral `nika-session → nika-cli-host` for the ONE probe and the ONE oracle facade (the ADR-124 precedent) · lateral `nika-session → nika-trace` for the run facts behind the result, gate and `/proof` views (never back) · `nika-session → nika-session-change` for the change set, its review, its outcome and the consent record (size-cap member · ADR-144 · never back). |
| Sub-tier | L4-surface — bare `nika` on an interactive terminal (the `nika-tui` renderer by default · the plain loop with `--plain` or `NIKA_TUI=0`, and as the renderer's fallback · a pipe gets the concierge). The session opens on the human's request; the deterministic compiler and the engine facts answer with no choice made. The first line only an intelligence can answer asks, in context, how Nika should think with the human (an AI app they already have · an API · a local engine · none · in that order, in human words), resumes that line exactly as typed once chosen, and keeps the answer at `~/.nika/session-intelligence.json`. The session observes the project once, answers Nika facts from the engine, hands the chosen intelligence a minimal typed bundle, and reads every reply through the hallucination guard. |
| Design | Eight modules, one law each: `identity` (the six laws + the language digest) — owned by `nika-onboard` since 2026-09-30 and re-exported here unchanged · `snapshot` (the proven root · the project file · the ONE walker) · `intelligence` (the census · the persisted choice · the resolution that refuses, never replaces · the data locus) · `reasoner` (ONE inference over the seat, the provider registry, or none — never a temporary workflow) · `broker` (the bundle: named files inside the root, bounded, redacted, with provenance · the environment never injected) · `guard` (builtins · models · codes · MCP servers · verbs · fields · claimed ignorance, corrected under the reply) — owned by `nika-onboard` since 2026-09-29 and re-exported here unchanged · `facts` (the workflows · the builtins · the providers · a verdict through the facade · a code through the ladder · a shape through the ONE router) · `change` (ADR-126 · the typed change set a reply proposes: previewed from the exact bytes the apply consumes · witnessed against stale targets · landed atomically only on the consent line · the real check after it lands · a run requested only on a clean check · the pending gate read from a paused trace) — owned by the size-cap member `nika-session-change` since 2026-10-06 with `review`, `outcome` and `consent`, and re-exported here unchanged (ADR-144) · `runtime` (the loop · the proposal · the consent · the run observed). Owns nothing the engine owns. |
| LOC budget | ≤15k crate · ≤1500/file · ≤100/fn (Diamond caps) |
| IMPL | projected, never hand-typed: `scripts/crate-metrics.sh nika-session` (src LOC · largest file · unit and integration tests) |
| Crate version | tracks workspace · License `AGPL-3.0-or-later` · Edition 2024 · Publish `false` (Foundation crate · ADR-022) |
| ADRs | ADR-003 (12-gate admission) · **ADR-125 (the native session)** · **ADR-126 (project changes from the session)** · ADR-144 (the change set's size-cap member) · ADR-124 (the oracle facade the facts read) · ADR-122 / ADR-123 (the access plan and the layered verdicts the verdict fact carries) |
| Error range | **none user-facing** — `ReasonError` is the reasoner's refusal (no intelligence · the seat · the provider · the runtime) and `ChangeError` a change set's (outside the root · unnamed · stale · the file system), both spoken in the session as a refusal with its fix; the engine's own codes travel through the facts (`explain`) untouched. |
| Reference | the one-door pack 08 (the session runtime) · 09 (knowledge and grounding) · 13 (the first run) · 15 (project changes · preview == apply · consent) · 27 (the system contract) · 37 (the context firewall) · `crates/nika-cli/src/verbs/session.rs` (the door) · `crates/nika-cli/tests/session_pty.rs` (the door on a real terminal) |

---

## What it must NOT own

The workflow grammar · the builtin catalog · the model catalog · the error definitions · the check semantics · the runtime · the ARM semantics · the trace verification · what a run's trace proves · the project file grammar · the round's pure law and its codec · the compiler's reasons in a human's words. It queries those authorities (`nika_pack` · `nika_builtin` · `nika_catalog` · `nika_error` · `nika_cli_host::oracle` · `nika_dap::inventory` · `nika_vocab::project` · `nika_onboard::routing` · `nika_trace::run_view` · `nika_onboard::compile::round` · `nika_onboard::compile::reading`).

## Run facts: read here, owned by the trace reader

The workspace also consumes `RunFacts::of` and the canonical captured verifier
through its own `nika-trace` dependency. Session re-exports only `KeptRun` for
`SessionRuntime::observe_run_leg`; `kept_run` and `kept_turns` expose retained
observations. Session remains the sole history owner. Existing `observe_run`
keeps its signature and supplies no inferred execution identity. See
[history and downgrade limits](../architecture/session-history.md).

The views of a run the session observed are read from the run's own
journal, never from what the run printed. The reading and its three views
(the result after exit 0 or 1, the gate at exit 4, `/proof` on request)
live in `nika_trace::run_view` since 2026-09-24. The session owned no part
of it: frames in, text out, the chain judged by the ONE verify door that
crate already hosts. What stays here is the session's side of the
boundary: when a view is shown, which trace is under the root, the gate's
question and the tasks a yes lets happen (read from the workflow's bytes),
and the observation line that stands alone when no journal can be read.
The seam is five read-only doors, `RunFacts::{read, result, gate, proof,
pause_gate}`; the facts' fields stay private to `nika-trace`. A paused run's
gate (`change::PendingGate::from_trace`) is read through `pause_gate`: the
first pause's task, message and mode, as the session always read them, since
C9 (2026-09-28) no longer parsed a second time here. A private
`use nika_trace::run_view` in `lib.rs` keeps the session's one path,
`crate::run_view`. The paused/resumed trace fixtures moved with their
tests; `tests/fixtures/traces/copy.ndjson` stays here too, because the
runtime's observation test reads it.

## Meaning: owned here, its only reader

The Meaning view (what survived of a request, clause by clause, from the compiler's own
obligation ledger) is a pure projection of that ledger. The session is its only reader, so it
is owned here, `nika_session::meaning`: it sat beside the ledger in
`nika_onboard::compile::meaning` from 2026-09-28, and moved back on 2026-10-07, once the
onboarding surface stood at its 15k prod-LOC wall and this crate had room again (ADR-144). It
reads the compiler's outcome and the two verdict readings it shares with the onboarding surface
(`compile::reading::judged_not_accepted`, `compile::round::last_verification`) from there; the
candidate's bytes are read by the same strict `nika_schema` law as the review's. `/meaning`
shows the view and holds a waiting proposal, and a revision's delta rides beside the revised
proposal. The « unavailable » line names no protocol; the session adds its own way on (the
review above and `/show`).

`meaning` projects an outcome's obligation ledger
(`provenance.decision.ledger`) into the Meaning view: each clause's fate
(represented · needs an answer · external · not expressible · refused ·
contradicted), its assurance (read from the task that carries it in the
candidate's bytes, parsed by the one strict `nika_schema` law: strict mode, one
anonymous file, and bytes it refuses carry no verb), the rendered view and a
revision's delta. It is pure (an outcome or a ledger in, words out), never
certifies coverage (a clause the compiler did not read is not listed, and the
view says so). With neither a ledger nor a semantic record it renders
« unavailable » — words that name
no host's protocol (`UNAVAILABLE`; a host adds its own way on). A ledger that is
not a list, or an entry of an unknown or missing state, is never guessed and
never silently dropped: the view says it could not read it and counts none of
it as done (no « no clause » or « 0 waiting » over unread entries), and a delta
over unread entries says they were not compared. It sat in
`nika_onboard::compile::meaning` from 2026-09-28 and moved back here on 2026-10-07.
Its tests and their three recorded outcomes live beside it (`src/meaning/`), and
`tests/meaning_owner.rs` reads it as an external consumer.

For a closed semantic record, `render` instead projects the reading ledger at
`basis.read.ledger`: clauses are shown as read, without inventing a carrying task,
a represented clause or a pending question. Gaps remain explicit; unreadable
entries are disclosed. A Ready outcome states that the program was judged
against the whole request, without assigning that judgment to each clause: the
parts a doubted verdict asks alone locate defects as evidence, and the view
does not present their answers as clause judgments.
`clauses` and the existing delta API retain their realization-ledger contract.

A candidate its verifier answered and did not accept (the outcomes `nika_onboard::compile::reading::held_words`
reads so: the compiler's Applied `verify_held` finding, or a last
verification contested with no defect, the candidate still shown) is said « the
program was judged against the whole request: judged, not accepted » on the
ledger's count line and in place of the reading's judgment. When the last
verification repeats a rejection carried from an earlier round (`carried`), the
view adds « the verdict comes from an earlier round, which judged these same
bytes: the verifier was not asked again ». When the compiler kept neither a
ledger nor a record (it dropped the record, so no round replays those bytes),
`render` gives the request as the verifier judged it (« Meaning · your request as
the verifier judged it »), from the verification that judged those bytes: the
last attempt, or the earlier attempt of the compile it repeats with no call (its
`same_bytes_as` index, on the same candidate digest). Each part that verification
asked alone appears once, in order, with what it left: « missing » with the
verifier's reason; for a contested part, « broken in the candidate's bytes
(`<note>`), but the trial run shows it done for its inputs: nothing decided it »
(without the note when the attempt kept none and a trial-run question answered
`carried`), else « missing, then no task named that fails it: nothing decided
it »; « not settled »; else its answer, a trial run's first (« carried in the
trial run », « not exercised by the trial run », « carried », « superseded by a
later part », « asks no operation of the workflow »). Each other finding follows
once (an extra operation, a part never asked once a call got no answer), the
whole request aside. The view ends with the count of parts asked alone, « judged,
not accepted; nothing was written » and, for a carried verdict, which asked
nothing in this compile, the earlier-round words; it lists what the verifier asked
and claims no realization.

## The change set: proposed here, owned by its member

The project change a proposal writes (`change`), its factual review (`review`), the typed
outcome a host renders (`outcome`) and the consent record (`consent`) live in the size-cap
member `nika-session-change` since 2026-10-06 (ADR-144), placed below the session:
`nika-session → nika-session-change`, never back. They formed a closed cluster: they named only
each other inside this crate, apart from two aliases the move resolved (`crate::run_view` became
`nika_trace::run_view`, `crate::ProposalId` the member's `outcome::ProposalId`). The move
brought this crate back under its production-LOC wall. `nika_session::{change, consent, outcome,
review}` are documented re-exports of the member's modules and the root re-exports
(`ProjectChangeSet`, `ProposalId`, `ConsentRecord`, …) keep their paths:
`tests/change_reexport.rs` compiles against them as an external consumer. What stays here is the
session's side: when a set is proposed, previewed, consented to and applied, the rounds, the
money gate and the history. The seams it reads across the boundary (the apply attempt, the
check under a pinned access, the revision proposal, the candidate parse, the question's
incarnation) were crate-private and are now public items of the member.

## Source basis at the yes (C9 · F4)

A proposal is consented to as the program its recorded source facts justified.
`runtime/fresh.rs` binds what the compile outcome recorded of its sources (its
decision record and the request it read) to the proposal's identity and exact
bytes where it is proposed (`propose`, which a revision's proposal goes
through too); a money-only amendment names the same bytes anew and keeps it.
At the yes, before any write, consent record or money effect, the session
observes exactly the recorded sources again through the host's one bounded
observer and the compiler judges them for the request that compiled the bytes
(`nika_onboard::compile::basis_for`, the grounding law that admitted them). The
proposal keeps that exact request beside its bytes: its answers, the plan it
continued with that plan's earlier observation, and apart from them the
observation its own round was given (`compile::round::compiled`: the world the
outcome's plan recorded when the session's record names that very observation by
its identity, `world_sha256`; none when it was given none; no basis at all when
an attached observation was not recorded or its record names no identity). A field the human named by answering is recovered by the
compiler's zero-call replay of that request, never from a recorded label, and
the observation made at the yes is only ever the fresh side (C10). A moved basis (a column renamed or removed,
a source gone or unreadable, a key some sampled records now lack, a number
field no longer all numbers) or one that cannot be judged withdraws the
proposal: nothing lands, the reason names the changed dependency, the goal
stays and the request said again grounds itself on the project as it is. New,
removed or reordered rows, another column order and a new peek hash hold it,
and the report says the facts were judged. A proposal no compile bound (a kept
draft proposed again) takes its basis from a zero-call deterministic compile
of its request only when that gives its exact bytes; otherwise it is withdrawn
when its workflow reads project files (the check facade's own permits), and
lands with its freshness said unjudged when it reads none. A decision that
records no source fact is said so when its workflow reads files, never
presented as fresh. A field an answer asserted over a JSONL source, or a JSON file the
observer could not read whole, holds while nothing observed whole disproves it:
such a sample is partial, and a bounded sample disproves nothing an answer
asserted (a CSV header, or a JSON file read whole, does).
The run's own guards (`compute_admit`, the number policy)
stay the check at use: the observation is bounded, and nothing is atomic
between the yes and the run.

## One waiting state, one routing, one work snapshot

`SessionRuntime::waiting` is the one precedence every host reads for the next line: the one-time
cost decision, the choice of intelligence, a proposal's consent, a run's gate, then the value an
authoring question, a run input or an activation asks, else a new turn. `submit(line, shown)`
routes that line to what waits, by the identity the host displayed: a consent answers the
proposal shown (`consent_to`), a gate answer names the gate shown (`answer_gate_for`). With no
proposal shown, a declining or leaving line still declines and any other line is refused with
`NOTHING_SHOWN`: nothing is consented that was not seen. `work()` builds the typed snapshot
(`nika_session_change::work`) from the session's own state; it audits nothing, reads no file and
decides nothing. The reach it shows for the requested run and the saved workflow comes from the
check that cleared the run request or landed the consent, kept beside them when they happen. Its
`authoring` is the last compile outcome the session keeps, read as the compiler wrote it, so a
failed or unfinished creation stays explainable after its card. The CLI plain loop and the
terminal renderer route through these since 2026-10-08. A requested run's cost review is part
of them: the host announces the child's challenge (`run_review_asked`), `waiting` names it first
(`Waiting::RunReview`, its identity only), and `submit` reads the line with the one spending
grammar: one shown yes returns `RunReviewed { approve: true }` and the host answers its child
once; a decline, leaving or `decline_run_review` (an interruption) sends nothing; the evidence,
`/help` and `/status` answer beside it. The host keeps only the live child and its pipe.

## Exact schedule activation

Saving a scheduled candidate activates nothing. Activation consumes Compile's
exact unbound `requested_trigger.cron`, never its coarse cadence label or a
re-parsed source hint. Missing/unsupported/conflicting requirements refuse
before a declaration proposal, naming the need to restate a supported period
and explicit daily/weekly time; there is no implicit 08:00 or Monday.

The existing activation questions still require timezone, missed-run policy
and positive per-occurrence ceiling. The zone answer is judged by `nika-cadence`
itself when it is given (a name its bundled IANA base lacks, or `/Europe/Paris`,
is refused at once and the question keeps waiting), never by a second shape
rule. The full `TZ=...` expression is validated
by `nika-cadence`, shown with its field meanings, zero interval phase and local
clock/DST semantics, and recorded unchanged only after explicit declaration
consent. The canonical overlap/after-skip defaults are unchanged. A recognized
revision discards the old pending declaration and asks for a restated, saved
candidate followed by fresh activation; it cannot hold old consent as authority.
Cancel, stale project bytes and restart never authorize that declaration.
The lifecycle reports Declared, never proven Active or Run from this gesture.

`tests/schedule_activation.rs` drives the real deterministic Compiler through
public Session and asserts persisted registry bytes and typed lifecycle stages.
It has no model or firer and does not qualify live scheduled execution.

## Subscription authoring

An explicit `1 acp:claude-code/<model>` choice uses ACP for conversation,
classification and native Compiler authoring. The persisted harness transport
is `acp`; older preferences without that field retain `native`. The selected
transport must match the reasoner's authoring capability. No native or API
fallback is permitted. Workflow execution access remains a separate Run choice.

The initial ACP completion profile admits only the maintained
`@agentclientprotocol/claude-agent-acp` 0.81.1 identity on the active connection,
before `session/new`. Its audited options disable built-in tools, external MCP
configuration and filesystem settings, allow one SDK turn and disable transcript
persistence. The session uses a fresh scratch directory. Tool, permission,
non-text output and incomplete stop reasons refuse the answer; EOF, timeout or
cancellation never become a completed candidate. An error answer the adapter
types as a lost sign-in (`error.data.errorKind` `authentication_failed`) is told
as « ACP authoring unavailable: the app's sign-in has expired or was revoked; sign
in again in that app, then retry; no fallback », in Nika's words, never the
adapter's; any other error answer stays « transport ended before a complete
answer ». This is adapter-specific
capability admission, not a generic claim about ACP read-only modes. Codex
over ACP runs under its own completion profile (`@agentclientprotocol/codex-acp`
1.13.1 with its bundled codex 0.156.1), which is not an empty-tools profile.
Before the adapter starts, every tool-bearing feature, the plugins and every
MCP server the user configuration defines are disabled through `CODEX_CONFIG`
and read back on the exact bundled binary (`CODEX_PATH`). The session mode
codex-acp names `read-only` is applied and read back before the prompt; its
actual per-turn sandbox is `workspaceWrite` limited to the scratch directory,
with the network off and every approval denied. `apply_patch` stays callable
inside that scratch, which is removed after the call, and any tool beat
refuses the answer; the receipt names that residue. The whole answer reaches
Compiler validation; no JSON prefix is extracted.

ACP uses the same selected Compiler policy as API authoring. Interactive preparation
has no implicit total call, repair or monetary bound; callers using the historical
bounded Session surface retain its policy and subscription monetary guards. The
existing `harness_infer` receipt class carries an explicit
`transport: acp`, preserving clarification replay. Configured/accepted session
models are recorded separately from a responding model, which remains unknown.
Token usage and the subscription invoice remain unknown; no zero-priced API
allowance is created and no retained API account is erased. The requested token
ceiling is recorded without claiming that ACP enforces it.


The selected reasoner declares its subscription authoring capability separately
from a provider model or catalog-backed admission. A supported harness sends
native Compiler messages through `nika-harness::authoring::HarnessAuthoring`,
which implements the kernel completion seam over the existing infer-grade
transport. Session does not call the CLI compile adapter or select an API as a
fallback. Supported adapters retain their explicitly selected adapter and model;
an absent model retains the harness default. With multiple answer-capable apps
present, `1` keeps the choice and any pending request open until the human names
`1 <app>` or `1 <app>/<model>`; presence order and sign-in evidence never choose
between them. `1` alone still chooses the app when exactly one can answer; only
when several can, or none can, is it asked again as a question (which app, or
route 2, 3 or 4) that keeps the pending request, never an execution refusal. An
app named explicitly that cannot answer is still refused. The apps line labels each app `sign-in seen` or `no sign-in seen`: a
home-file witness proves presence, not a live login. Unsupported model namespaces or
unavailable/unsupported harness capabilities refuse visibly. No intelligence
continues to compile deterministic requests without calling a model.

Initial authoring, recorded clarification continuations and revisions use the
same native Compiler and the same pinned authoring context. An answer round adopts
the plan the Compiler re-anchored to a changed source and drops every answer the outcome asks
again (`AuthoringRound::absorb`, R4 A6): the same goal keeps its round, only a fresh explicit
answer binds, and a verified or pending transform is never carried to another source. Revisions retain
the exact base bytes, original request and raw change. A failed edit keeps the
previous proposal or saved workflow; Session never substitutes a model
paraphrase as the source of a fresh Create request. A saved-file revision binds
its destination and original byte witness before compilation. Its proposal must
update that same file over those bytes; a moved base refuses without proposing
a new sibling file. Pending updates retain this target through further
corrections and question continuations. Save still requires fresh consent.
For an unsettled work line beside a saved workflow, Modify or Mixed revises it,
NewWork starts a new creation, and failed, absent or unknown classification
keeps the current state without authoring. This routing does not reinterpret
lines already settled by the deterministic reader. The adapter passes
the whole returned answer to Compiler validation, exposes no workflow tools,
and accepts no tool-bearing answer. Native Codex authoring runs `codex exec`
under a measured pre-execution empty-tools profile: every tool-bearing feature
is disabled and hosted web search, sub-agents, skills and MCP servers are
configured off before the turn starts. The profile is admitted only on the
codex-cli minors where it was measured (0.160); at spawn the binary must name
`codex-cli` on such a minor and `codex features list` under the same flags
must read every disabled feature as off, otherwise the call refuses before
any prompt, with no provider fallback. Observed tool events still reject the
answer as a second check. The CLI's `codex/<model>` authoring seat uses this
same transport, never the ACP agent. Supported one-shot adapters pass an
explicit empty tool list. The transport retains its binary/version
attestation, isolated scratch, tool restrictions and child cleanup. A call requires
a positive finite deadline. The legacy bounded Session policy uses 300 seconds
for subscription authoring; continuous preparation currently uses 600 seconds
per call, not as a transport-wide maximum. Explicit thinking budgets are
unsupported and refused; the native CLI does not enforce Compiler's requested
token ceiling, which is recorded without claiming enforcement. Explicit request
and repair limits still apply where selected; continuous preparation has no
implicit count. Cancellation or timeout accepts no answer and does not prove
that a sent request was unbilled.

The Compiler's existing deterministic first step remains available even when
the chosen harness cannot author. A settled deterministic request makes no
model call and is not evidence that the harness worked. Only work requiring
cognition reaches the harness refusal; an unchosen or unavailable intelligence
can still be reselected in context, and ordinary conversation keeps its own door.

A subscription receipt names the adapter, requested and separately observed
model, attested binary version and usage-marker evidence. Unknown responding
identity stays unknown. Infer-grade does not expose numeric usage or an invoice;
Compiler token totals remain absent rather than zero. `/meaning` displays this
subscription evidence without labelling it a direct API or catalog price.
Clarification replay carries the originating subscription receipt even without
a knowledge snapshot and explicitly states that replay made zero calls.
Subscription authorization is not billed-provider admission. An explicit
subscription selection suspends a retained API allowance without clearing its
identity, settlements, reservations or uncertainty. Only that selected subscription
uses its normal non-API door; its invoice remains unknown and no API fallback is
admitted. A saved selection has the same separation after reopening. For the
historical bounded Session surface, returning to API requires a fresh total on
the same complete account; historical uncertainty
still cannot become a new numeric allowance. A USD ceiling stated while the
subscription is selected blocks its cognition and is kept as a monetary restriction
across reopening. Explicitly reselecting the subscription explains its unknown
invoice and clears only this restriction, never API exposure. Invalid money,
project zero and a pending gate still block cognition on that bounded surface.
Continuous preparation observes the selected subscription's unknown invoice and
retained API exposure without those monetary gates; this supplies no credit and
changes no Run constraint. This adds no subscription account or numeric price.
A proposal still requires fresh review and
consent, revisions expire the old identity, and authoring grants no Save or Run.

Every authoring round observes the files its request names under the session's
own project root, with the shared bounded observer (never the process's working
directory, never through a link outside the root), the deterministic rounds
included: the initial ladder, an answer or a revision compiled again, and the
deterministic fallback while money blocks cognition (R4 S1). One observation
site serves every seat. A round compiled on the session's own seat roots the
session's context in place, as before; the initial ladder, a request read again
with its change and the money fallback compile under a rooted copy instead, so
they never change the session's context nor the identity of the questions it
asks. The compiler grounds a rule's keys in that observation: a key the file
holds is ready with no call, a word it does not spell is a closed choice of its
keys, and an answer given for another revision of the file is asked again; the
decision records both (`decision.session.observed`, `decision.grounding`).
`compile_in` observes on a deterministic seat only when its context roots a
project. `compile_through` (the default context) and the public
`compile_deterministic` keep their pure contract: with no root nothing is
observed, a key a request merely names is then asked, and a library caller
supplies its own world as knowledge.

The hermetic `subscription_authoring` integration suite injects only a fixture
executable: actual public Session, native Compiler, continuation, knowledge
consumer evidence and proposal identity remain real. These fixtures are not
live subscription qualification; real Codex/Claude execution must be reported
separately with its source and executable identities.

## Bounded label calls

Provider-backed label calls keep a finite first-call output ceiling: 1,024 tokens
for ordinary or catalog-unknown models, 4,096 for catalog-known reasoning models
whose reasoning shares that output allowance. The mock provider keeps the
ordinary ceiling. The selected provider and model remain unchanged; caller-set
infer limits keep their meaning. No blank or truncated label triggers a retry
with a larger ceiling. Existing monetary admission must cover the selected
ceiling before transport; a failed or empty answer remains a failed reading,
not a fallback classification. Hermetic wire tests prove these limits and
zero-call monetary refusals; they do not establish real-model routing quality.

A persisted explicit `none` choice has no conversational reasoner, even when
the host installs a factory for changing that choice later. It stays on the
protocol fallback: a typed model answer can reach review without calling that
factory again. An injected classifier retains its route, and a real classifier's
failed or unknown answer still binds nothing. Reaching review grants no consent.

## Monetary admission

The Session inference rules in this section describe the historical bounded
embedding surface. The interactive CLI and TUI use continuous preparation instead:
earlier exposure remains evidence, while monetary constraints govern the separate
Save/Run review and execution. The Run rules below apply to both surfaces. See
[Continuous interactive preparation](#continuous-interactive-preparation) for the
current interactive defaults.

Session reads explicit monetary intent before the compiler, classifier or
reasoner sees new work, including Prepare. Currency or a monetary anchor gives
a number that meaning; times, quantities, quoted data and path tokens do not.
Amounts must be finite and nonnegative. Decimal comma is accepted, explicit
zero is distinct from unset, and conflicting amounts refuse. Compact forms such
as `budget=0`, `budget:0,50` and `2USD` accept sentence punctuation; malformed
amounts refuse before cognition. Explicit money replaces a project default; the
default is not an independent cap. A stated default being replaced must match
the observed default. Project discovery errors remain visible and refuse
admission instead of silently becoming an absent default. The original intent
reaches the compiler unchanged. A work request, a consent line and an answer
line are read by their monetary directives only (R4 A6 · B15): a word inside a
business clause, a field named `budget`, a business amount, a negative value, a
quoted value or a path is never money and never refused as money, whatever its
currency. Run and gate lines keep the whole-line reading. A consent line
changes money only when the whole line is a money amendment, which gets a fresh
proposal identity; a business revision never changes money by a numeric
coincidence. The unknown-cost review reads its zero and its default the same
way. The seat of a metered round reads the request with its admitted directives
blanked, never as work. The directives the
gate admitted ride with the authoring round (`AuthoringRound::money`), so the
compiler's deterministic door reads the rest of the request without them: « ….
Budget: $0. » keeps the business round (READY or a field question, zero calls)
where the unresolved clause used to need a seat the zero ceiling refused. A
line that still does not settle is routed as written (conversation stays
conversation, unread work stays work); UNKNOWN, exposure and reconfirmation are
never reset by it. When the ceiling refuses the seat, such a line is refused
with the compiler's reasons, never the bare ceiling: « budget=0 » over a file
with a `budget` column reads both ways and is the human's to restate. The
unknown-cost gate's own deterministic check reads a line the same way: on a
route whose USD cost the catalog cannot qualify (a gateway), a stated ceiling
never turns deterministic work into a cost review, a route refusal or a
zero-constraint refusal; a malformed ceiling is left to the money gate. An
explicit zero stages no cost review at all, since nothing can be sent under it
on any route: the deterministic reading, a question, or a refusal with the
reader's reasons follows.

At a live authoring question, an offered choice key alone or a value that binds
without interpretation needs no unknown-cost review. A question ending in `?`
keeps its local answer. Other words meet the selected seat's one-time cost
review before any classification or value reading; declining keeps the pending
question, and restored exposure refuses the review without sending a request.
A live round with no current question exempts only the local `?` path.
Once attached currency is recognized, amount validation cannot fall back to
filename handling: `budget=0.5oopsUSD` refuses, while `budget=0.txt` and quoted
or explicit path data retain their data meaning.
Compact anchors also open trailing currency directives attached to the work («
… stars and budget=0.5USD »), never inside a relative clause: « … rows whose
status is open and budget=1500USD » is data.
An unanchored currency needs its own sentence or comma segment (`…, 2USD`);
`and 2USD` or `et 2USD` could finish a business range and is never stripped as
a ceiling. Currency inside a business predicate remains business data. A
standalone malformed currency segment is refused.
Monetary conjunctions are read together, so `budget 1 USD and cap 2 USD`
refuses instead of selecting the last amount. An empty monetary anchor and
an invalid non-path amount between an anchor and currency also refuse.
An explicit old-default reference remains part of the admission decision
and must match the observed project default.
The lexical reader is the compile unit's `money` module (R4 A6), moved
unchanged from `runtime/money_parse.rs` and re-exported as
`nika_onboard::compile::money`: the compiler and Session read one law;
admission, its account and its refusals stay Session's.

`SessionRuntime::monetary_decision()` exposes the actual decision, original
intent, monetary input, amount token, effective USD amount, source, project
default/file, inference enforcement and proposal identity. Rejected money has
no effective amount. `/status` and `/meaning` show the same observation.
Independent policy and machine caps are **unknown**: Session has no observation
seam for them and does not invent either limits or proof of absence. Billed
cost also remains unknown without a receipt, including subscription paths.

An explicit positive ceiling can opt into **catalog-backed admission** on a
qualified endpoint/model. One shared account covers classifier factories,
conversation, compilation, repair and revision. Each
physical request reserves full-context input plus its explicit output limit at
the pinned tariff. Complete validated usage settles a catalog estimate; failed,
missing or contradictory usage retains the reservation and closes admission.
This is a local token-cost estimate, not a hard external billing cap or invoice.
The provider invoice remains unknown. Authoring and Run have separate scopes.
`SessionRuntime::inference_receipt()` exposes the live aggregate and attempt
provenance; `MonetaryDecision.admission` snapshots it when a proposal is bound.
Observation changes do not change a proposal's identity; changed monetary terms do.

The first profile covers nonstreaming text at the exact native DeepSeek endpoints
and exact wire models in `nika-catalog/data/inference-admission.toml`. Other
providers, custom gateways (including unsupported Scaleway overrides), streaming,
media, tools, extra billing axes, subscriptions and unpriced local computation
refuse before a paid effect. Custom reasoners/classifiers/HTTP effects must
explicitly implement the bounded seam; their old methods are never called as a
fallback. Zero and invalid amounts remain guarded.

With no explicit amount there is no Session allowance, cap or ceremony, and the
chosen model keeps answering. A qualified priced route (the profile above) is
observed, never admitted: conversation, labels, routes, compilation and
revisions ride one no-budget account (`InferenceAdmission::unbudgeted`) through
the same bounded seam — exact route, text only, explicit output bound, single
attempt, no retry or redirect — so each physical request records its
reservation, usage and catalog estimate of complete usage, or stays
charge-unknown; never an invoice. The record carries a « may have been sent »
line before transport and the settled observation after it. A contradicted or
unsettled request freezes that account: the same work's continuations (an
answer, a revision at consent, a repair) send nothing more, only new work starts
a fresh account, and the frozen one stays in the record. A restart names a
leftover line, replays nothing and demands no reconfirmation: a no-budget
observation restores as history, never as an account or a restriction, while
any other observation, including one without the `unbudgeted` mark, restricts
as before. A later explicit amount starts its own allowance; earlier no-budget
observations stay recorded and are never counted as covered. Other routes
(native non-compatible APIs, local engines, subscriptions, a door's own
classifier) keep their existing unobserved path. Project/Session defaults apply
to Run without pretending to meter authoring.

Amendments change the total allowance without erasing settled or held exposure;
questions, new factory instances, model changes and repairs never reset it.
An uncertain account cannot reopen. An old observation cannot prove the complete
aggregate: its previous invoice remains unknown. A complete concordant checkpoint
may restore a closed ledger as described below; the narrow legacy review below
admits only a fresh unknown-cost invocation, never a recovered numeric allowance. A ceiling stated after the restart (a
request's own directive, or a kept round's read again at `/restore`) belongs to
that round: its answers state no money and keep it, never refused as the
restored exposure, while numeric Session inference stays blocked and new work stating
no ceiling is still refused outside a fresh legacy cost review (C11). A bounded session never automatically
changes the selected authoring model to a stronger one.

Restoring a kept authoring round may replay its deterministic reading without
an allowance and without a model call, including under restored uncertain
exposure. This exception belongs only to that replay: it neither clears the
exposure nor admits fresh work or paid cognition. A null or otherwise unreadable
durable cost observation remains uncertain. Every Session inference status
states how many such observations are unreadable and never treats them as settled.

A prepared decision participates in the exact proposal preview and identity.
A monetary-only amendment can revise Session's own ceiling without changing
workflow bytes or calling a model; it creates a new proposal requiring fresh
consent. Invalid amendments expire pending authority. A line that states no
money at a consent prompt whose cognition is blocked (a restored exposure, a
zero or closed allowance, a held gate amendment) has nothing to admit and
nothing reads it: the proposal waits with its identity, answered from its own
observed effects and that reason; only `yes` applies it. Save retains the ceiling
against that proposal and the exact saved workflow bytes within the current
runtime. A separate Run carries it in `RunRequest::max_cost_usd`; an explicit
Run ceiling can replace it, and changed bytes require a fresh decision. Neither
money nor Save grants a Run. File lookup resolves symlink aliases to the saved
path and checks the exact byte witness. Ambiguous or changed identities refuse;
independent files with identical bytes do not inherit each other's decisions.

A saved-file revision without a new amount uses the same in-memory binding
only when the resolved file and exact compiled base bytes agree. Ambiguous,
stale or unreadable bindings refuse before cognition. With no binding, revision
keeps the existing change-money/default law; this does not restore a saved
ceiling across restart or alter Run's journal-based refusal below.

The existing consent journal proves which files Save wrote but does not persist
their monetary constraints. When an in-memory binding is unavailable, including
after reopening, Run checks that journal even if the host omitted `restore_state`.
A previously saved file or its resolved alias requires an explicit Run ceiling
or a fresh Prepare/review; Session never substitutes the default. An unreadable
or unsupported journal also requires reconfirmation. An explicit Run override
applies to that request only. The journal and state schemas are unchanged; this
is refusal on unavailable evidence, not restoration of execution consent or a
claim to recover constraints after the durable evidence itself has been removed.

Open-language replies to confirm gates pass monetary admission through both
`answer_gate` and `answer_gate_for` before classification. Zero, positive and
invalid monetary replies invoke no cognition, issue no Resume and retain the
waiting gate identity, with the supplied money visible in the observation.
A rejection expires pending authority but remains a cognition guard for the
continuation. Ordinary questions at the same gate retain the rejected decision,
including after zero followed by an invalid amendment. A valid replacement is
admitted afresh; deliberate new work still receives its own monetary decision.
This does not revise a paused execution's budget: that capability is unavailable.
Text and choice gate values remain typed data, including monetary-looking text.

The host must still admit and execute a Run. The CLI forwards the amount to
the existing runtime spend gates; those bound metered spend and can report
unbounded exposure or subscription costs outside that accounting. A Session
decision is neither proof of execution nor proof that all downstream costs are
bounded. Public deterministic tests and observed loopback protocol doubles
verify mechanics; they are not live provider or subscription qualification.

## Question identity

A host that is not at the keyboard answers an authoring question by identity,
as it consents to a proposal or answers a gate (ADR-133).
`SessionRuntime::pending_question_id()` names the question the next line
answers: a witness of the question, of the request revision it belongs to
(intent, answers, recorded plan, open questions), of the ordinal of its asking
and of the intelligence, seat and authoring context that read its answer,
held with the session that asked it. It stays the same while the question
waits — an aside, a refusal, a reply that bound nothing — and changes when the
request is read again or revised, when the question is asked again, when the
intelligence changes and when the session restarts, even for the same key and
the same words. `answer_question_for(&id, line)` refuses before any classifier,
reading, compiler call, record or effect, and whatever waits keeps waiting:
another question waits (`stale_revision`, also for an identity another session
asked), the question was answered or dropped in this session
(`already_consumed`), none waits (`wrong_state`), a cost review waits
(`stale_revision`, as for a proposal's consent), or the intelligence choice, a
proposal or a paused run owns the next line (`wrong_state`). The waiting
question takes the line exactly as `turn` gives it at the keyboard, and the
terminal and the TUI keep answering the question they show that way; the
`Question` outcome is unchanged. `CompileQuestion.key` stays the compiler's
semantic hole. The identity lives in memory only: it is never persisted, never
restored and grants no consent and no Run. Its text names the question and its
revision, not the session — the session is told apart by the value itself, with
no clock, randomness or shared counter — so a host keeps the value the session
handed out; a wire host needs ADR-133's session identity follow-up.

`runtime/inference_tests/question_identity.rs` drives real Session → Compiler
clarifications: the deterministic compiler's `model` question, and a native
destination question over the loopback seat in DIALOG-11's shape, where the
same key asked again for the revised request is another identity. These are
mechanics, not a live provider qualification.

## The tests that admit it

- a chat turn writes nothing (no temp workflow · no `.nika/` · no trace);
- the reasoner receives only the bundle (the identity core · the facts · the file the human named, redacted · never the environment · never an unnamed file);
- the pack's adversarial corpus (an invented builtin · model · code · MCP server · verb · field · a claim of ignorance) is corrected before the human sees it;
- an explicit intelligence this machine cannot serve is refused with its fix and never replaced;
- the facts answer without any model;
- the preference round-trips under the home and a corrupt file is « never chosen »;
- the first screen speaks the atelier order in human words, never a class name;
- on the real binary (`session_pty.rs`): a pipe is the concierge, the TTY is the session, the first run asks once, the kept choice never asks again, `nika thread` is the parser's own refusal;
- a reply carrying a file is a proposal and nothing is written before the consent line (`no` discards · a classified cancellation discards the whole pending proposal without effects · questions keep the proposal pending · `yes` lands the exact bytes the preview printed, byte for byte); an update is witnessed and a stale target applies nothing; a path outside the root or a file the human never named is refused before any preview; the fix ladder's prepass repairs the reply's dead forms before the preview and says so; the preview's effect rows come from the report's own permits and requirements; the real check follows every workflow written; « create and run it » requests the run ONLY on a clean on-disk check and findings stop it; the door's observation of the run is a fact; the last run is read from its trace, never from memory;
- an authoring value said in words binds through its typed reading (`runtime/answer_tests.rs`): a JSON literal or one token is the value as typed, with no call; several words are read once through the metered label seat and bind only as whole tokens copied verbatim from the human's line (a piece cut out of a token — an extension, the name under a folder, the local part of an address — binds nothing), said beside the outcome; whole tokens bound the copy but do not prove its meaning — which tokens are the value is the model's reading, reviewed by the human before consent; no value, several values, an invented or partial copy, a failed call or a spending limit bind nothing and the question waits, saying why; with no intelligence chosen a value binds only as it stands alone — one token, a JSON literal, a longer value in quotes (its content, never its quotes) — and the question says so, while a sentence (« tell me more » at a path) binds nothing and the question waits; a stated trigger's cadence, which the compiler reads again in words, keeps its words; a choice among offered keys (an open column among the observed `montant · autre`) binds an offered key typed alone — or written as its JSON string — with no call, and under a chosen intelligence the same one bounded reading is shown the exact keys offered now and binds only a copy that IS one of them, verbatim and whole in the human's line (« La colonne montant. » → `montant`, said beside the proposal); a line carrying no offered key is not read, and a key the line does not carry, a word not offered, a piece of a word, a trailing second answer, an empty or failed reply or a spending limit bind nothing and the choice waits; which offered key a line chooses — one of several, not the one it rejects — is the reading's, never the first match's; without an intelligence the line is the answer as typed and the compiler, the final authority, keeps the choice asked with its offered keys; the seat's `model`, the replacement request, a clause's disposition and a rule asked in words keep their own doors; a reading is never a consent;
- a run's declared input takes the line as typed (words and paths alike, no call), and a value in quotes is its content, never its quotes — the escape for a word the protocol would take (`"why"` binds `why`, where `why` alone explains);
- a closed line (`why`, « qu'as-tu compris ? », « what happened? », a greeting) is the same line whatever its typography — a no-break or narrow no-break space before its mark, a typographic apostrophe — and reads from state with nothing routed; the closed sets gain no word;
- a turn that fails leaves a recovery card, repeated from memory with no call by « what happened? » and, when nothing else waits, by `why`;
- a run that paused at a human gate (exit 4) returns to the session as the gate's own question, read from the trace's pause event; the human's line becomes the resume the door runs (`--resume <trace> --answer <task>=<value>`), an empty line is refused and nothing answers for them, a gate is answered once; the repair round lands a witnessed update from « fix it » and the on-disk check reads clean.
- the controlled episode over the public seams (`episode_tests`): a local brief proposed from fixture pages lands on a consent that names it, byte for byte, the run handed back as data (the session never executes); the destination's preimage appearing, changing or disappearing after the preview — alone or as the second file of a set — refuses as stale with not one byte of the tree moved and the proposal undecided, and the next revision witnesses what is there now; a restarted host holds no consent from before; a missing grant is named at preview from the checker's own finding and stops the run, never the preparation; the player's prompt carries the bundle and never the oracle outside the root, an unnamed page body or the environment, and is not consulted between the preview and the consent.

## Bounded decision seat

An operator can select `NIKA_SESSION_DECISION_MODEL=typesafe/<model>` when Session
opens. Session and CLI compilation share the same TypeSafe adapter. Only finite
compiler choices consult it; deterministic work makes no call, and NONE remains a
valid outcome. The compiler validates the answer against its offered options.

Each finite choice makes one attempt with a 20-second deadline and no transport
retry. Interactive preparation has no implicit three-call ceiling. The selected
service accompanies API, native subscription and ACP authoring through the same
compiler capability. It can read ambiguous clauses, rank feasible plans and judge semantic
fidelity: the whole request first and, only when it doubts it, each part of the request
alone, why a part it finds missing is missing (the task it points to, an operation no task
performs, or no task after all), each part still open over a trial run of the same bytes when
the sketch door made one in this Session, any extra operation, and then the whole request
over that run (the [whole-request judgment](nika-compile-cognition.md)). A verdict is
therefore one consultation when it carries the request; otherwise it is one per part of the
request plus those follow-up questions, asked in sequence, each its own attempt under the
deadline, and an attempt that fails or is refused ends that verdict's questions. Only a defect
it locates goes to the author for repair, with the reason it gave; a doubt it does not locate
is never repaired from and never proposed, and the candidate is held (below). Within one
compile, the service is not asked again on bytes it already declined: its earlier verdict
stands with no consultation. Session also carries the service's rejections into every later
compile of the same goal (below), where a rejection of the same bytes for the same request
stands with no consultation too; an abstention is not carried. The questions over a trial run
carry the run's texts to the configured service as preparation context. The current adapter
does not perform Foundry retrieval; that role belongs to the accepted
[cooperative target](../architecture/ARCHITECTURE-0.123.md) and needs separate
implementation and qualification. It grants no execution authority. In the current
implementation, a caller that explicitly supplies a monetary account keeps its
existing admission rules; an unpriced decision never consumes that allowance.
Billing units remain separate from tokens, with cost and invoice unknown.

`nika/session-decision-seat@2` retains every attempt and closes the scope when its
compile ends; its `role` states what the seat decides, the whole-request judgment's
questions included. Each attempt keeps its question id and offered keys (`task-<id>` and
`part-<k>` among them), never the question's state or a trial run's texts.
`scope_ended: true` means no further consultation of that scope, not
that a price or successful workflow was proved. An unresolved send remains
Uncertain even on cancellation. Old @1 records stay unchanged and visible. A new
scope never reconciles an earlier uncertain charge, and reading observations
never dispatches. The pre-dispatch durable marker remains the host's responsibility;
an unavailable in-memory journal refuses before the decision transport starts.

The configured verifier receives preparation trial observations by default. Session's
sketch rounds may try a candidate in a sealed room on copies of the observed files. Every
question over the run (a part asked again over it, its pointer, the whole request over it and
its pointer) receives the texts the run read and the outputs it read back (currently at most
64 KiB per file, with whole/partial and written-by-run markers); the verifier asks over a run
only when it proves whole outputs. The decision service is used when selected, otherwise the
authoring provider. The compiler binds the evidence to the exact candidate and records
paths, sizes and sha256 rather than duplicate texts. The copy door continues to perform
its deterministic qualification; making evidence available does not add a judge call.
Workflow execution permissions belong to `.nika` and Run, not to this preparation context.

The adapter and journal remain in `nika-cli-host::compile::typesafe::session`;
`authoring::DecisionSetup` preserves its public path. Session owns persistence and
the selected author/decision composition. No new provider, permission, retry or
execution path is introduced. The obsolete `MAX_DECISION_CALLS` constant is removed;
explicit limits of workflow execution and the CLI Compile callers are unchanged.

## Project context and preparation bounds

Every seated authoring round observes the named files under the Session project
root through the shared `compile::observe::world` reader. It carries bounded
headers, keys and categorical values into the full original request, answers
and revision context. The receipt separates attachment from presentation in a
model call; a deterministic result or replay does not claim a presentation.

The legacy bounded Session surface starts native output at 16384 tokens, with
a 32768 ceiling, 180 seconds per API call and three repair rounds. Its
`CostReview::for_session` compatibility review allows at most seven requests;
this is an enforced allowance, not the continuous compiler's theoretical worst
case, and it may stop preparation before completion. Reported truncation may
spend a repair to widen output; an uncertain transport is not retried. Numeric
allowances never widen themselves.

The interactive CLI and TUI instead enable continuous preparation. Its selected
route supplies the technical output ceiling and per-call deadline; no implicit
request-count, repair-count or monetary review gates preparation. A stated
workflow budget remains part of the separate Save/Run review and execution.
Historical accounts, reservations and unknown charges remain observations,
never new credit. Missing usage or prices stay unknown, and a configured model
name is not evidence of the identity actually served. Explicit limits retained
by bounded callers and workflow execution are unchanged.

## Configuration read when a door opens

A host door reads this configuration once, when it opens the session. Model
selection and credentials are operator configuration; none of it is taken from
a workflow, a reply or retrieved context.

| Source | Meaning |
|---|---|
| `NIKA_<PROVIDER>_API_KEY` or the catalog's variable (`DEEPSEEK_API_KEY` …) | Presence only, for the census; the provider client reads the value when it calls |
| `~/.nika/session-intelligence.json` | The kept choice (kind · model · time); a corrupt file reads as never chosen |
| `NIKA_AUTHORING_STRATEGY` · `NIKA_KNOWLEDGE` · `NIKA_KNOWLEDGE_EXCLUDE` | The shared authoring configuration: a trusted named release or the embedded default is admitted and pinned when the context opens. Knowledge off attaches nothing; strategy off with no source is unread. `NIKA_KNOWLEDGE_PACK` is refused: a pack was composed for one request |
| `NIKA_AUTHORING_REASONING` | The explicit reasoning effort every seated authoring call asks (`low` · `high` · `max`), through the same parser; a host's typed word outranks it |
| `NIKA_AUTHORING_SOURCE_RECOVERY` | An optional explicit finite count of source recovery rounds, as a nonnegative integer. Positive rounds are refused under `off` or `only`; invalid words refuse. With a bounded repair policy, `0` or unset supplies no recovery rounds. Continuous preparation (`repairs: None`) can enter source recovery after an eligible structured failure even with `0` or unset, without an aggregate recovery count; provider failure, truncation and repeated diagnostics still end that attempt. Recovery uses the same selected seat and records its route in `/details`; its observed costs do not add a monetary confirmation. Explicitly bounded callers retain their allowance and count. |
| `NIKA_SESSION_DECISION_MODEL` with `TYPESAFE_API_KEY` | The optional decision seat (`typesafe/<jev>` only) |
| `NIKA_TUI` | `0` · `off` · `false` · `no` · `plain` keep bare `nika` on the plain loop |

There is no provider credential store. The default knowledge needs no filesystem
location: `AuthoringContext::default()` resolves and pins the release embedded
in the binary, with an independently trusted build identity. The current release
contains three patterns linked to three blocks. Matching words recall patterns
and their blocks beside the language card, request, answers and observed world;
an intent with no lexical match adds no reference. There are no examples or
repair principles. Records distinguish composed references from those presented
in a native authoring instruction; neither proves better generation.
Deterministic authoring presents no knowledge to a model. Explicit knowledge
off and strategy off with nothing named compose none.
A named source must be admitted against the host's trusted identity; a missing
identity or invalid release refuses without falling back to the embedded one.
See the [shared knowledge door](nika-onboard.md#shared-authoring-knowledge-door)
and [pin migration](nika-onboard.md#the-knowledge-pin-and-its-records-read-by-session)
for the typed choices and disk/embedded origins. Operator overrides are read
from the login environment when the door opens.

## Explicit authoring reasoning (R4 B16 · C11)

The session resolves an explicit reasoning effort once, through the shared parser
(`compile_config::reasoning`), apart from the rest of the configuration: another refusal
(a strategy word, a snapshot) never drops a valid level. A host's typed word
(`AuthoringSettings::with_reasoning`) outranks `NIKA_AUTHORING_REASONING` and never
falls back to it. Any word other than `low`, `high` or `max` is refused at the first
seated turn, as an unknown strategy is, and every label and conversational turn
refuses it before any reasoner, dispatch record or byte: a refused word is never read
as no level. `AuthoringContext::reasoning()` keeps the level. Its Debug appends it only
when one is named, so every question identity and unknown-cost binding hashed without
one keeps its bytes, and a named level binds them. `/status` says « reasoning effort
<w> asked of every LLM call ». With the operator-selected decision seat, it adds that this
TypeSafe seat is a separate backend and no effort is sent to it (B19).

On a provider seat every authoring call asks it through the one policy
(`AuthoringPolicy::with_reasoning`): the plan, its evidence repair, the native
candidate and its repairs, a sketch, a transform. The provider writes `thinking`
enabled and `reasoning_effort` only on a direct route whose catalog lists the level
(`deepseek/deepseek-v4-pro`), and refuses before any byte elsewhere (another model or
provider, a gateway, a base-URL override). The caps stay the policy's: an effort never
changes a cap and a cap never chooses an effort. A subscription seat refuses a named
level before any call, since its adapter cannot carry it. The TypeSafe/Jev decision
seat is a separate backend, outside the level's scope (B19). Its contract carries no
reasoning effort, so none is sent to it or claimed for it. With a named level, its receipt
(`decision.session.decision_seat`) says `reasoning_effort: not applicable …`. The receipt
records, per call, the level configured, the keys read back from the bytes sent, and
`served: unknown`: what the provider spent internally is not observable here.

The conversation's own calls ask it too: every turn-routing label and every
conversational turn. A label asks the level through the classifier that sends it
(`TurnClassifier::carry_effort`, B19):
- the fresh `ReasonerClassifier` the factory builds for each routed turn carries it
  (`asking` is its builder form), and so does a door's classifier (`with_classifier`) that can;
- the trait's default refuses a named level with the reasoner's typed `ReasonError` (never a
  bare string), so a classifier that cannot carry it is never called with it;
- the no-call `ConservativeFallback` accepts it.

A refused word, or a level the classifier cannot carry, fails the route before any record or
byte. Each call reads the level the session holds when it is made, so a
host's `set_authoring_context` after open takes effect at the next call. They go through
`SessionReasoner::reason_effort`. `ProviderReasoner` makes the same call it makes
without a level, with the same ceiling, temperature and words, and carries the level
through the verb's conduit (`InferInput.reasoning_effort` in `nika-verb-infer`), so the
provider's route qualification above applies unchanged. Any other reasoner refuses a
named level before calling (the trait's default): the level is never dropped. A session
naming none makes the calls it made before.

`/details` reads the receipt: every authoring call that asked a level gets one line, each
fact apart. The line gives the level configured and the keys read back from the body
sent (`unobserved` when none was read back, never assumed from the level). It also says
the served effort is unknown, and gives the reasoning tokens, the usage reported (or why
no answer came) and the model the response named. A call that asked no level adds
nothing, and the subscription lines are unchanged. The headline counts only what the
receipt shows (B19): a call refused before sending is never said to be sent, and a call whose
record shows no response is unobserved, never answered. When every attempt was refused, it reads
« nothing was sent to ». The knowledge-record lines of
`/details` come from Onboard's `pin::knowledge_lines` (B19, descended with their bytes
unchanged).

## Persisted state and version compatibility

| Path | Contents |
|---|---|
| `~/.nika/session-intelligence.json` | the intelligence choice |
| `~/.nika/sessions/<project-digest>/events.ndjson` | the private conversation history (`docs/architecture/session-history.md`) |
| `<root>/.nika/session-state.json` | the project's structured record (#1464) |
| `<root>/.nika/consents.ndjson` | the consent journal (#1465): which files Save wrote |

The record's `inference_observations` keep each new account observation in its
durable form (`InferenceReceipt::durable_observation`,
`nika/inference-cost-observation@2`, E35): origins replace endpoints, and the
accounting fields its restore reads keep their meaning. The in-flight line names
the route by its origin. An entry recorded earlier is carried as written, never
rewritten or migrated. The live account keeps its exact route and authority.

Since `4c728c980`, a closed Run request is journaled as its own `run`
operation, apart from a conversation turn, so its ceiling never amends Session
inference. This reader accepts earlier histories and keeps their legacy
restrictions. Executables that predate the operation, v0.120.3 included,
refuse a history that contains it: an unknown operation is refused, never
guessed. Rolling an installation back restores the executable and its
configuration, not the history format. There is no reverse migration, and
deleting history is not a way to reset spending uncertainty. The user-facing
account is in `docs/usage/conversational-session.md`.

The Session direct-API authoring descriptor uses `cost_basis: unpriced; billing_unverified`. Token observations and endpoint diagnostics do not establish a tariff or provider invoice.

Direct API authoring endpoint metadata comes from the exact seated registry: `host` strips user info, path, query and fragment; `base_url_overridden` compares the effective URL with its profile seed when available. `endpoint_basis: operator_configuration` distinguishes this configuration from an authenticated remote identity or an observed model. Session host diagnostics use the same redaction.

On reopening, an input round is not restored; an authoring round whose
question waited is kept (C7). The home History is its one durable copy
(`Saved.round`, the schema-1 record of `nika_onboard::compile::round`); the
project's structured record keeps none, and the unanswered labels the kept
round owns are not announced as expired (any other label still is). The
recovery notice, `/meaning`, `/why` and `/status` name the kept round
read-only: its request as typed and, when a clause answered in words rebuilt
it, as rebuilt (the goal saved beside the round; `/restore` keeps it as typed),
its settled answers and the question that waited.
Opening rewrites neither store, calls no model and grants no consent; the
live intent's `unresolved` stays empty, so no line is bound to the kept
question before `/restore`. `/restore` continues it only when asked: the
money gate reads the request again (no admitted span, account, review or
consent is restored; a revision's words are its change, read as the live
revision reads them, never the goal it was kept with), a saved workflow's
revision whose base moved or
vanished is held with nothing compiled, and the recorded plan is replayed by
the deterministic compiler against the observation it recorded — no provider
call, no workflow executed — so the question is asked again under this session's
identity (one minted by the closed session is refused). The next answer is an
ordinary answer round against the project as it is then, and any provider
call passes the current admission. A round the redactor changed, one over its
bound, another schema or a malformed record stays kept byte for byte, is
named, and is never continued. New work replaces a kept round. Conservative
monetary restrictions retain their existing restoration laws.

The round's pure law lives beside its codec (C10): `AuthoringRound` keeps its
path, fields, constructor and every method with its signature — `compile` and
`compile_with_admission` stay inherent — and delegates the typed request, what
an outcome settled, the questions it leaves, a re-anchored plan and a carried
receipt to `nika_onboard::compile::round`. The kept round's read-only lines
are built from that codec's `RoundWords`. A kept draft and a kept round share
one refusal: something else waits, this engine cannot read the kept value, or
none is kept; either stays kept.

The preview's rows of a check report (the first findings and hints, the effect rows and
the spend) are read from `nika_display::check_render::review` (C10): strings, order and
limits are unchanged, and the change primitive keeps the verdict, the path and the audit's
authority. A saved workflow's revision carries the monetary directives the gate admitted in
the line that said the change, as the money law reads the change the EDIT holds
(`compile::round::change_money`, B15); a revision said at the consent prompt carries none,
and a money-only consent line keeps its fresh-proposal path.
A complete Create replacement (`intent.clarification`) becomes the request text and drops
what the earlier intent was answered and planned with, including its knowledge and receipt.
The chosen seat and aggregate account stay. Lexical budget spans stay only for identical
bytes; changed text carries only directives read from those exact replacement bytes.
A replacement without a directive retains the account ceiling but carries no old spans.
A restatement binds both original and added directives against the combined text and
refuses conflicting amounts. A clause restated in words also verifies that the budget
in the rebuilt request agrees with the ceiling admitted for the answer; a changed
clause cannot leave the account and compiled request naming different amounts.
A revision keeps its change and does not become a Create.

A gate restored at open is offered only as its journals stand (C7b §3.4,
`nika_trace::lineage`): with no continuation it waits again; a continuation
that settled, still runs or cannot be judged is said and nothing waits; one
that paused again offers its own gate. The same standing is read again before
any resume, and an answer to a gate a continuation overtook is not sent. Before
any answer, the gate's question names the completed tasks a resume is sure to
run again, live (`run_view::live_again`, judged by the resume's own fold);
nothing is said when the fold's plan carries every completion, which promises
nothing more: the run serves a carried completion only while its definition and
inputs are unchanged (C10 · Q8).

## Passive candidate plan presentation

`review::{plan_lines, plan_lines_in_order}` re-export the pure projections in
`nika_display::check_render::review`, which also owns their task-face rendering.
Session retains candidate identity, destination selection, consent, application
and history; rendering candidate text does not approve or save it.

## Semantic programs across revisions and reopen

The proposal boundary retains the compiler's byte-bound semantic or source-revision
record as evidence in the existing HOME conversation history (`Saved.programs`).
Pending edits retrieve records by proposal identity and exact base bytes; saved-file
edits require the saved relative path and those bytes. Equal bytes at distinct paths
do not alias a request, and an unaccepted proposal cannot replace a saved record.
The compiler reconstructs and judges the record under the current observation. A saved
revision reads its original request from that record, not a later conversation goal.
Records are bounded and redacted as a whole by `compile::program_records`; an
unknown envelope is kept unchanged and grants nothing. A missing record keeps the
compiler's explicit historical-source limitations. No second project store is added.

An EDIT's base record is input to a fresh revision, not a replayed answer round:
knowledge is composed again, current monetary admission applies, and the revision's
settled record replaces the base as its continuation. History can restore the last
file actually saved as a conversational selection, but no proposal, consent, Run
permission or clean-check assertion. The separate concordant-checkpoint path below
can restore numeric accounting closed; program evidence cannot. Save still targets the same
file under its original byte witness. Hermetic loopback tests in
`runtime/semantic_basis_tests.rs` exercise Save/reopen/revise and pending failure
retention; these are protocol checks, not live provider qualification. The test also
preserves the negative case where a reopened session has prior monetary exposure:
a new allowance must not erase or replace that exposure. Retaining semantic evidence
does not itself implement durable monetary-account resumption.

The closed conversational acts (`is_cancel`, `is_why`, `is_meaning`,
`is_what_happened`, `is_greeting`) are owned by
`nika_onboard::routing::conversation` and re-exported under their existing Session
paths. Their vocabulary and whole-line matching remain unchanged.

### Resuming complete numeric accounting

A new-format inference checkpoint is kept in both the project record and the
completed conversation boundary. After `enable_history`, `restore_state` may
restore its accounting CLOSED only when both values match, their project binding
and exact cost observation validate, the exclusive history lease is held, and
no interrupted operation or dispatch marker remains. Older observation-only
records, corruption, missing/divergent copies and uncertain history still refuse
paid continuation. The read never sends a request or renews a proposal or consent.

Before another inference the human must restate a TOTAL Session ceiling. The
provider owner's `amend` conserves prior settlement, holds and request identity;
a default (including zero) cannot reconfirm it. Run admission remains separate.
The exact observation superseded by the restored ledger is removed from the
historical display list to avoid counting those same attempts twice.

A turn without a recognized fresh ceiling keeps this restored account closed.
Session records a fixed monetary-refusal reason on the account; the human
diagnostic stays separate, so a private route in that diagnostic cannot make
the otherwise complete numeric checkpoint unwritable. Dropping and reopening
after this refusal preserves the same prior costs and request identity. It
still requires a fresh total, for example `Budget: 10 USD.` as its own sentence;
no amount is inferred from an ambiguous phrase and uncertain costs stay unknown.

### Fresh invocation after a readable legacy cost report

A restored pre-checkpoint numeric report is not a reconstructed account. In the narrow
legacy form (one readable, internally consistent uncertain numeric observation, followed
only by completed explicitly reviewed scopes), the selected HTTPS API may offer the
existing one-time unknown-cost review even when its current route has a catalog tariff.
The report's old allowance, original attempts, known estimate and retained unknown-charge
reservation remain unchanged and visible. The quote is not a final charge or proof of the
historical request bound; no TOTAL dollar guarantee covers that earlier charge.

A stated budget alone never authorizes this exception. Only an explicit answer to the
shown unknown-cost question admits its new invocation under the existing request, output,
timeout, route and host-policy bounds. Zero, malformed/contradictory evidence, a gate,
an old in-flight marker, a present checkpoint or unreadable host cap still refuses. The
review witness includes the current project record and observations as well as the exact
input, project, source bytes and selected route; a changed record cannot confirm the old
question. Reconfirmation remains a durable restriction outside the confirmed invocation.

The bounded account closes after that invocation and its observation joins the same
project report exactly once. A subsequent request, revision or reopening needs a fresh
review. A newly uncertain scope blocks this exception; it is never absorbed into the old
report. Save and Run still require their separate acts. This is not numeric-ledger
migration, invoice reconciliation or authority restored from conversation history.

## Routing diagnostics without a compiled workflow

`/details` exposes the current session's recorded routes even when no compiler
outcome exists yet. A Failed route means that no usable label was obtained;
it does not establish that a model was called, returned blank, or was billed.
The visible route retains phase, act, method and the input hash. Private failure
notes remain internal: the display projects only closed engine-authored guidance
for missing intelligence, local admission refusal, AI-app/provider failure or a
runtime failure. Only the exact native timeout form emitted by the harness is
named as a timeout; quoted client stderr cannot establish that category. The
routing diagnostic displays no raw stderr, prompt, endpoint, credential or
private failure text.

The compiler decision's route/seat/ledger/knowledge wording is the pure
`nika_onboard::compile::reading::decision_words` projection beside the existing
receipt wording. Session still owns when to display it and keeps every existing
consent, accounting and dispatch boundary; diagnostics trigger no automatic retry.

### Reopening a completed unknown-cost invocation

For completed unknown-cost API scopes, Session may retain a
`nika/completed-cost-report@1` witness in both durable stores. With the same exclusive
history and concordance checks as numeric restoration, it enables only a new cost
review. It restores no account, allowance or consent; monetary reconfirmation remains
set. The review names the unchanged earlier exposure, and confirmation binds the full
project record, observations, exact input and selected route. A stale record refuses
before transport. Each later request or reopening needs a fresh explicit review, and
only that invocation receives the existing bounded unknown-cost account.

The old exact numeric-codec refusal string is compatible only when every observation
is a complete CLOSED unknown-cost scope and both stores agree. Unknown/incomplete
attempts, zero, gates, interrupted boundaries, arbitrary errors, missing history and
mixed observation families remain refused. In particular this path does not interpret
Jev's decision-seat records or discard no-budget observations. Qualification of the
API-only TUI journey is separate from any decision-seat continuation. Save and Run
remain separate human acts; previous observations are neither reset nor duplicated.

### Continuous interactive preparation

The plain interactive CLI and fullscreen TUI activate `enable_continuous_preparation` before
opening history. Preparation has no implicit request-count, repair-count or monetary gate.
The selected connection remains explicit; no API fallback is granted. Historical accounts,
reservations and unknown charges remain evidence and never become fresh credit. Monetary
directives in a proposed workflow still belong to its separate Save/Run review and execution.
Callers using the existing bounded Session surface without this opt-in retain its old policy.

Preparation observes physical API requests through the provider dispatch journal, including
failed and cancelled requests, and persists its privacy-projected records. Missing prices or
usage remain unknown; subscription and decision-service invoices are not fabricated. The
configured direct DeepSeek route starts at 131072 output tokens and may widen to the pinned
393216 technical limit; the existing transport timeout is 600 seconds per call. Other routes
use their own qualified limits, never DeepSeek's because of a similar model name.

### Interactive preparation stop and activity

The host obtains `begin_preparation_turn()` before moving the conversation into a worker.
Its fresh `CancelCtx` applies only to preparation; Run keeps its own cancellation and effects.
The driver drops an API/subscription compile future when Stop is observed, retains the Session
and cost journals, and accepts no result from that cancelled future. A sent remote request may
still be billed. A new turn uses a new token without resetting historical exposure.
`on_activity(ActivityHook)` carries the producer's typed `Activity`; `on_progress` remains a
compatible text projection. This does not infer a model actually served from a configured name.

A host calls `withdraw_cancelled_preparation` immediately after a stopped worker returns and
before displaying or accepting its proposal. It withdraws only a new proposal or changed
preparation round from that turn; the proposal/question that already waited, gates and Run
authority are preserved. Stop wins at the proposal boundary and again at the host handoff.
`TurnOutcome::Cancelled` keeps the Session available for the next human instruction.
Each activity may carry a `CallMark` with the compiler ordinal, role, requested model and
Started/Finished/Cancelled state. Finished means returned, not semantically accepted or billed.

### Unjudged candidate continuation

An INCOMPLETE result carrying the compiler's Applied `verify_resume` finding is kept as an authoring round, never as a Save proposal or Run permission. `continue` or `retry` replays its exact semantic record and asks the judge without regenerating the author candidate. A new correction discards that candidate and enters the existing request-restatement path. The round's answers, obligations and provenance remain durable under the existing exact-digest record. Reopening makes no model call; `/restore` arms this questionless round without proposing it, and an explicit continuation asks the judge. An altered or withheld record still refuses. Ordinary clarification rounds retain their existing replay protocol.

The compiler leaves that finding only when the verifier's call on the whole request returned
no admitted choice (the judge answered nothing), and the kept round says the candidate could
not be judged (`JUDGMENT_KEPT`); an empty line, a run verb, `yes` and `save` repeat those
words and change nothing.

A candidate the verifier answered and declined (it rejected the bytes or abstained) is held
instead when no defect was located: the compiler keeps its bytes as the INCOMPLETE outcome's
preview, drops its replayable record and adds the Applied `verify_held` finding, worded by what
held it; COLD, WARM and answer rounds add the same finding to a candidate whose located
defects stayed unsettled. The authoring round then drops the plan it replayed, with the
knowledge and receipt of the call that authored it, and keeps every answer
(`AuthoringRound::absorb`; a revision's base record, its input, stays), so no later compile of
the round replays that record. A later compile of the same request may author the same bytes
again: the same verifier's rejection of them stands there with no consultation, because
Session carries the goal's rejections (below), while an abstention may be decided by a new
round. Session words it
through `nika_onboard::compile::reading::held_words`, by what the last verification found: a
part missing that the repairs did not settle, an abstention, or a rejection with no defect
located (« The workflow is built but not proposed: the verifier did not accept it and located no
defect a repair could start from; nothing was written. »); then a correction, or
`/intelligence` for another authoring model (which also judges unless a decision model is set),
with `/meaning` for what was understood. No `continue` is offered, and nothing is proposed to
save or run. `Reading::of` reads a held outcome as unsettled before its questions and provider
findings, so a held candidate whose judge call timed out or was refused while the verifier was
locating its doubt still gets these words, never those of a provider failure or an exhausted
budget.

The Session keeps, in memory, the verdicts that rejected candidate bytes in its current goal's
compiles (`nika_onboard::compile::round::rejections`, the latest per candidate digest, judge,
request and context by `keep_rejections`; an abstention is never kept). Every later compile of
that goal carries them
(`CompileRequest::with_declined`): an authoring or answer round, the write-again after a
located defect, the stronger seat's retry, a continuation, and a request read again with the
human's words or a correction, whose restated goal takes the kept rejections with it. The
compiler repeats a carried rejection, with no consultation (`carried: true` on its attempt),
only for the same verifier, the same bytes, the same whole request and the same answers and
observed world; its located defects are what the round's repair or reopening starts from, and
a localization it left unfinished resumes with no question asked again: a restated or
corrected request is another request, so the verifier is asked again
on it, even on bytes it rejected under the earlier words. A compile of new work keeps its own
verdicts and drops the earlier goal's; a reopened Session carries none from before it closed. A
deterministic compile, when money blocks cognition, asks no verifier and carries none.

An answer round replays and judges, never repairs. The replay of a model's record (COLD,
WARM, native or sketch) leaves the whole request pending on the bytes it emits, so the round's
verifier judges the whole request again. When the last verification of the replayed candidate
holds a located defect (`defects`: a part the verifier pointed to a task for or found no task
performing, or an extra operation it pointed to a task for), Session forgets the replayed plan
once and compiles the request again under every answer already given, where the authoring
round's own judgment and repairs run. A verifier that declined the replay without locating a
defect gives no defect to write again from: the compiler drops the replay's record and its
questions (`verify: doubted, not replayable`) and adds `verify_held`, Session forgets the
replayed plan, and the human gets the words above, never a question about rejected bytes. A
verifier that answered nothing leaves the whole request pending: the compiler keeps the
record, and the human is told the workflow is built but not proposed because no judgment made
in this round settled it. When money blocks cognition, an answer round compiles
deterministically: the core's replay keeps the whole request pending for every record but the
reader's own HOT plan, so such a round is INCOMPLETE, never READY. An unsettled round in an
unmetered Session whose model the human did not name is tried once more with the provider's
stronger model; after a held replay, that retry writes the workflow afresh under the answers
already given instead of replaying the held bytes, and carries the goal's rejections.

After a stopped creation with no saved workflow or live round, the retained goal remains
context for a classified correction. The existing turn classifier sees the earlier goal:
Modify/Mixed restates its exact words after « Original request: » and a line break, then
« Correction (it takes precedence over the original where they differ; every other
requirement stands): », a line break and the exact correction, so the correction takes
precedence only over what it changes. The frame names no keep or exclusion lead and no
restriction word, so the verifier's part it labels, the correction's first phrase, is judged
for what it asks, never as a restriction; the restated goal takes the earlier goal's kept
rejections, though its new words are judged afresh (above). NewWork is an independent CREATE;
an unknown route preserves the goal and starts no authoring call. No cancelled candidate,
Save consent or Run authority is restored. The combined request remains the goal even if
its authoring fails, and the existing history restores it without asking a model. Compiler
validation and fidelity still decide whether the resulting candidate answers that request.

The intelligence census retains the same host probe's effective provider endpoints
for the reply guard. Conversation and proposal discussion both pass these facts to
`KnownWorld::audit_over`; missing context keeps the profile-default check. These
facts grant no authority, contain no API key values, and are not model context.
