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
- an authoring value said in words binds through its typed reading (`runtime/answer_tests.rs`): a JSON literal or one token is the value as typed, with no call; several words are read once through the metered label seat and bind only as whole tokens copied verbatim from the human's line (a piece cut out of a token — an extension, the name under a folder, the local part of an address — binds nothing), said beside the outcome; whole tokens bound the copy but do not prove its meaning — which tokens are the value is the model's reading, reviewed by the human before consent; no value, several values, an invented or partial copy, a failed call or a spending limit bind nothing and the question waits, saying why; with no intelligence chosen the reply is the value as typed and the question says so; the seat's `model`, the replacement request, a clause's disposition and a choice keep their own doors; a reading is never a consent;
- a run that paused at a human gate (exit 4) returns to the session as the gate's own question, read from the trace's pause event; the human's line becomes the resume the door runs (`--resume <trace> --answer <task>=<value>`), an empty line is refused and nothing answers for them, a gate is answered once; the repair round lands a witnessed update from « fix it » and the on-disk check reads clean.
- the controlled episode over the public seams (`episode_tests`): a local brief proposed from fixture pages lands on a consent that names it, byte for byte, the run handed back as data (the session never executes); the destination's preimage appearing, changing or disappearing after the preview — alone or as the second file of a set — refuses as stale with not one byte of the tree moved and the proposal undecided, and the next revision witnesses what is there now; a restarted host holds no consent from before; a missing grant is named at preview from the checker's own finding and stops the run, never the preparation; the player's prompt carries the bundle and never the oracle outside the root, an unnamed page body or the environment, and is not consulted between the preview and the consent.
