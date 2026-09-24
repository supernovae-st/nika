# nika-session — crate spec

| Field | Value |
|---|---|
| Status | **WIP → ADMISSION** (One Door · wave 4 · ADR-125). In `workspace.metadata.diamond.wip` until the 12 gates land (Gate 5 mutation and Gate 11 swarm owed). |
| Layer | **L4 — interface** (the human's terminal) · a host runtime over the installed engine · **sync** on the terminal, one current-thread runtime per inference · lateral `nika-session → nika-cli-host` for the ONE probe and the ONE oracle facade (the ADR-124 precedent). |
| Sub-tier | L4-surface — bare `nika` on an interactive terminal. The first run asks how Nika should think with the human (an AI app they already have · an API · a local engine · none · in that order, in human words) and keeps the answer at `~/.nika/session-intelligence.json`; the session observes the project once, answers Nika facts from the engine, hands the chosen intelligence a minimal typed bundle, and reads every reply through the hallucination guard. |
| Design | Eight modules, one law each: `identity` (the six laws + the language digest) · `snapshot` (the proven root · the project file · the ONE walker) · `intelligence` (the census · the persisted choice · the resolution that refuses, never replaces · the data locus) · `reasoner` (ONE inference over the seat, the provider registry, or none — never a temporary workflow) · `broker` (the bundle: named files inside the root, bounded, redacted, with provenance · the environment never injected) · `guard` (builtins · models · codes · MCP servers · verbs · fields · claimed ignorance, corrected under the reply) · `facts` (the workflows · the builtins · the providers · a verdict through the facade · a code through the ladder · a shape through the ONE router) · `change` (ADR-126 · the typed change set a reply proposes: previewed from the exact bytes the apply consumes · witnessed against stale targets · landed atomically only on the consent line · the real check after it lands · a run requested only on a clean check · the pending gate read from a paused trace) · `runtime` (the loop · the proposal · the consent · the run observed). Owns nothing the engine owns. |
| LOC budget | ≤15k crate · ≤1500/file · ≤100/fn (Diamond caps) |
| IMPL | ~1900 LOC src (2026-09-03 live · `scripts/crate-metrics.sh nika-session`) |
| Crate version | tracks workspace · License `AGPL-3.0-or-later` · Edition 2024 · Publish `false` (Foundation crate · ADR-022) |
| ADRs | ADR-003 (12-gate admission) · **ADR-125 (the native session)** · **ADR-126 (project changes from the session)** · ADR-124 (the oracle facade the facts read) · ADR-122 / ADR-123 (the access plan and the layered verdicts the verdict fact carries) |
| Error range | **none user-facing** — `ReasonError` is the reasoner's refusal (no intelligence · the seat · the provider · the runtime) and `ChangeError` a change set's (outside the root · unnamed · stale · the file system), both spoken in the session as a refusal with its fix; the engine's own codes travel through the facts (`explain`) untouched. |
| Reference | the one-door pack 08 (the session runtime) · 09 (knowledge and grounding) · 13 (the first run) · 15 (project changes · preview == apply · consent) · 27 (the system contract) · 37 (the context firewall) · `crates/nika-cli/src/verbs/session.rs` (the door) · `crates/nika-cli/tests/session_pty.rs` (the door on a real terminal) |

---

## What it must NOT own

The workflow grammar · the builtin catalog · the model catalog · the error definitions · the check semantics · the runtime · the ARM semantics · the trace verification · the project file grammar. It queries those authorities (`nika_pack` · `nika_builtin` · `nika_catalog` · `nika_error` · `nika_cli_host::oracle` · `nika_dap::inventory` · `nika_vocab::project` · `nika_onboard::routing`).

## Exact schedule activation

Saving a scheduled candidate activates nothing. Activation consumes Compile's
exact unbound `requested_trigger.cron`, never its coarse cadence label or a
re-parsed source hint. Missing/unsupported/conflicting requirements refuse
before a declaration proposal, naming the need to restate a supported period
and explicit daily/weekly time; there is no implicit 08:00 or Monday.

The existing activation questions still require timezone, missed-run policy
and positive per-occurrence ceiling. The full `TZ=...` expression is validated
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

The selected reasoner declares its subscription authoring capability separately
from a provider model or catalog-backed admission. A supported harness sends
native Compiler messages through `nika-harness::authoring::HarnessAuthoring`,
which implements the kernel completion seam over the existing infer-grade
transport. Session does not call the CLI compile adapter or select an API as a
fallback. Supported adapters retain their explicitly selected adapter and model;
an absent model retains the harness default. Unsupported model namespaces or
unavailable/unsupported harness capabilities refuse visibly. No intelligence
continues to compile deterministic requests without calling a model.

Initial authoring, recorded clarification continuations and revisions use the
same native Compiler and the same pinned authoring context. The adapter passes
the whole returned answer to Compiler validation, exposes no workflow tools,
and accepts no tool-bearing answer. Codex authoring currently refuses before
any call: its existing infer-grade boundary only rejects observed tool events
after return, which does not prove pre-execution tool disabling. It may be
admitted when that capability is attested; neither deterministic success nor
an API is substituted. Supported one-shot adapters pass an explicit empty tool
list. The transport retains its binary/version
attestation, isolated scratch, tool restrictions and child cleanup. A call has
a finite deadline capped at 600 seconds; Session defaults to 300 seconds for a
subscription authoring call. Explicit thinking budgets are unsupported and
refused; the native CLI does not enforce Compiler's requested token ceiling,
which is recorded without claiming enforcement. Compiler's own bounded call
and repair policy still applies. Cancellation or timeout accepts no answer.

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
Subscription authorization is not billed-provider admission. Existing explicit
monetary refusals remain in force; this connection adds neither an account nor
an exemption from those guards. A proposal still requires fresh review and
consent, revisions expire the old identity, and authoring grants no Save or Run.

The hermetic `subscription_authoring` integration suite injects only a fixture
executable: actual public Session, native Compiler, continuation, knowledge
consumer evidence and proposal identity remain real. These fixtures are not
live subscription qualification; real Codex/Claude execution must be reported
separately with its source and executable identities.

## Monetary admission

Session reads explicit monetary intent before the compiler, classifier or
reasoner sees new work, including Prepare. Currency or a monetary anchor gives
a number that meaning; times, quantities, quoted data and path tokens do not.
Amounts must be finite and nonnegative. Decimal comma is accepted, explicit
zero is distinct from unset, and conflicting amounts refuse. Compact forms
such as `budget=0`, `budget:0,50` and `2USD` accept sentence punctuation;
malformed amounts refuse before cognition. Explicit money
replaces a project default; the default is not an independent cap. A stated
default being replaced must match the observed default. Project discovery
errors remain visible and refuse admission instead of silently becoming an
absent default. The original intent reaches the compiler unchanged.
Once attached currency is recognized, amount validation cannot fall back to
filename handling: `budget=0.5oopsUSD` refuses, while `budget=0.txt` and quoted
or explicit path data retain their data meaning.

`SessionRuntime::monetary_decision()` exposes the actual decision, original
intent, monetary input, amount token, effective USD amount, source, project
default/file, inference enforcement and proposal identity. Rejected money has
no effective amount. `/status` and `/meaning` show the same observation.
Independent policy and machine caps are **unknown**: Session has no observation
seam for them and does not invent either limits or proof of absence. Billed
cost also remains unknown without a receipt, including subscription paths.

An explicit positive ceiling can opt into **catalog-backed admission** on a
qualified endpoint/model. One shared account covers classifier factories,
conversation, semantic restatement, compilation, repair and revision. Each
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
fallback. Zero and invalid amounts remain guarded. With no explicit amount,
existing unbounded behavior is unchanged. Project/Session defaults apply to Run
without pretending to meter authoring.

Amendments change the total allowance without erasing settled or held exposure;
questions, new factory instances, model changes and repairs never reset it.
An uncertain account cannot reopen. After restart the previous aggregate cannot
be proved: restored paid work requires an explicit new-scope reconfirmation and
its previous invoice remains unknown. A bounded session never automatically
changes the selected authoring model to a stronger one.

A prepared decision participates in the exact proposal preview and identity.
A monetary-only amendment can revise Session's own ceiling without changing
workflow bytes or calling a model; it creates a new proposal requiring fresh
consent. Invalid amendments expire pending authority. Save retains the ceiling
against that proposal and the exact saved workflow bytes within the current
runtime. A separate Run carries it in `RunRequest::max_cost_usd`; an explicit
Run ceiling can replace it, and changed bytes require a fresh decision. Neither
money nor Save grants a Run. File lookup resolves symlink aliases to the saved
path and checks the exact byte witness. Ambiguous or changed identities refuse;
independent files with identical bytes do not inherit each other's decisions.

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
- an authoring value said in words binds through its typed reading (`runtime/answer_tests.rs`): a JSON literal or one token is the value as typed, with no call; several words are read once through the metered label seat and bind only as whole tokens copied verbatim from the human's line (a piece cut out of a token — an extension, the name under a folder, the local part of an address — binds nothing), said beside the outcome; whole tokens bound the copy but do not prove its meaning — which tokens are the value is the model's reading, reviewed by the human before consent; no value, several values, an invented or partial copy, a failed call or a spending limit bind nothing and the question waits, saying why; with no intelligence chosen the reply is the value as typed and the question says so; a choice among offered keys (an open column among the observed `montant · autre`) binds an offered key typed alone — or written as its JSON string — with no call, and under a chosen intelligence the same one bounded reading is shown the exact keys offered now and binds only a copy that IS one of them, verbatim and whole in the human's line (« La colonne montant. » → `montant`, said beside the proposal); a line carrying no offered key is not read, and a key the line does not carry, a word not offered, a piece of a word, a trailing second answer, an empty or failed reply or a spending limit bind nothing and the choice waits; which offered key a line chooses — one of several, not the one it rejects — is the reading's, never the first match's; without an intelligence the line is the answer as typed and the compiler, the final authority, keeps the choice asked with its offered keys; the seat's `model`, the replacement request, a clause's disposition and a rule asked in words keep their own doors; a reading is never a consent;
- a run that paused at a human gate (exit 4) returns to the session as the gate's own question, read from the trace's pause event; the human's line becomes the resume the door runs (`--resume <trace> --answer <task>=<value>`), an empty line is refused and nothing answers for them, a gate is answered once; the repair round lands a witnessed update from « fix it » and the on-disk check reads clean.
- the controlled episode over the public seams (`episode_tests`): a local brief proposed from fixture pages lands on a consent that names it, byte for byte, the run handed back as data (the session never executes); the destination's preimage appearing, changing or disappearing after the preview — alone or as the second file of a set — refuses as stale with not one byte of the tree moved and the proposal undecided, and the next revision witnesses what is there now; a restarted host holds no consent from before; a missing grant is named at preview from the checker's own finding and stops the run, never the preparation; the player's prompt carries the bundle and never the oracle outside the root, an unnamed page body or the environment, and is not consulted between the preview and the consent.
