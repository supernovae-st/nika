# Crate spec — `nika-cli-host`

| | |
|---|---|
| Status | **MEMBER** (size-cap split of the admitted `nika-cli` unit · ADR-110 · D-2026-07-09-N1 · 2026-07-31) |
| Layer | L4 — interface member (one unit, two members; the bin + dispatch stay in `nika-cli`) |
| Design | the host-integration plane: probes · wire writers · doctor receipts · vendored client matrix · context envelope · retention · metrics · the output contract |
| IMPL | Project with `scripts/crate-metrics.sh nika-cli-host` from the qualified tree. |
| LOC budget | ≤15k crate · ≤1500/file · ≤100/fn (Diamond caps) |
| Crate version | tracks workspace |
| License | `AGPL-3.0-or-later` |
| Edition | 2024 |
| Publish | `false` — internal member of the `nika-cli` unit |
| NIKA codes | none minted here — findings ride the surfaces they serve |

---

## 1. Purpose

`nika-cli-host` carries the **integration control plane** of the operator
surface — everything that answers "where are we, what host is present,
what is wired, what is honestly active":

- `probe` — client/kit probes, capability levels, `HostCapabilityReceipt`
- `wire` — the writers (JSON · JSONC · TOML families), preview/detected,
  the clap `WireTarget` seam
- `doctor` — the per-component receipt surface
- `welcome` — the front-door glance + the checkable `SAMPLE`
- `clients_registry` + `data/clients.registry.yaml` — the vendored
  byte-copy of the agents-repo matrix (H6: one truth, machine-checked)
- `context_envelope` — the single workspace resolution (chat_only law)
- `context_redaction` — the pure text filter for secrets in model context;
  Session retains its historical `broker::redact` path through a re-export.
- `retention` · `metrics` · `text` · `output` (spec §4 exit codes ·
  `VerbOutput` · the OSC-8 link seam)

This is the third 15k-wall descent (display 2026-07-10 · run composer
2026-07-22): compute descends, the operator crate keeps the bin, the
dispatch, and the run/guard authority path.

## 2. Surface law

`nika-cli` re-exports every public item at its historical path
(`verbs::{doctor, probe, welcome, wire}` · `verbs::{VerbOutput, exit}` ·
`metrics` · `verbs::trace::retention` · …). Existing callers keep those historical paths; `public-api.txt` is the split's
receipt. The native TUI consumes the narrow typed child lane directly, with
default features off and no reverse host dependency on Session or TUI.

## 3. Discipline

- No workflow execution here: `guard`, admitted Run execution, permits,
  secrets and traces stay with their existing owners. Host monetary review
  reads configuration, obtains a fresh decision and consumes provider admission;
  it cannot grant workflow effects or reconstruct authority from a receipt.
- The vendored matrix is engine-mirror material: corrections happen in
  the agents repo (`clients.yaml`), then re-vendor byte-exact.
- `local-infer` forwards from `nika-cli` (feature parity across the
  unit's members).
- Tests coupled to cli-side fixtures (retention×trace-store ·
  SAMPLE×check) live cli-side and exercise the re-exported paths.

## Compile transport and materialization

The Compile CLI adapter lives in this existing interface member and is
re-exported as `nika_cli::verbs::compile`. It owns clap arguments, explicit
source/destination I/O, atomic publication of Ready candidates, and human/JSON
rendering. The acyclic lateral dependency on `nika-onboard` consumes the one
typed Compile core; this adapter does not own compiler semantics or runtime
admission. Shell-word quoting is shared through `output::sh_word` so the
Compile next step and Run resume teaching use the same escaping law.

Only a Ready compile writes. When a compile that is not Ready was given a
destination that already holds a file or link (R4 A6), the JSON document adds
`existing_destination` (that caller-named path, beside `written: null`) and
the human text ends « existing destination remains at <path>; this compile did
not write or remove it ». Presence is read with no-follow metadata: the file is
never read, followed, removed or rewritten (`--force` included), never shown
as this round's output or judged a workflow, and status, exit code and the
absent `nika run` next step are unchanged.

The operator's words carry its money (R4 B15). `build_request` marks the
request `with_stated_money`: the compile core reads every monetary directive of
the request (of a replacement request on the seats' door; of a revision's
`--change` and of the positional request its `--base` answered) with the law of
`nika_compile::money`, blanks it from the business reading and records it in
`decision.money`; a malformed or conflicting one refuses. This door meters no
authoring seat, so any stated ceiling, zero or positive, keeps
`--authoring-model` and `--decision-model` closed: nothing is prepared or sent,
a HOT-settled request stays READY, and other work stays incomplete with an
`authoring_money` finding. Naming a model grants no budget. The ceiling never
enters the workflow bytes and grants no Run authority; `nika run` keeps its own
`--max-cost-usd`.

The binary and all dispatch remain in `nika-cli`. Existing CLI integration
tests exercise the re-exported surface and actual process boundary; the
quoting law itself is unit-tested here, beside `output::sh_word`. No new
crate, authority boundary, or exception to the size budget is introduced.

`serve_args` owns the resident command's clap vocabulary, including its optional
native authoring seat. `nika_cli::verbs::serve::ServeArgs` and
`nika_serve::NativeAuthoringArgs` retain their public paths through re-exports;
the defining Rust type names now originate in this member. The existing closed
native-seat flag structure retains its fields and literal-construction API.
The resident arguments provide `Default`; dispatch and seating remain in their
existing owners. `compile::authoring_http` shares the single-attempt authoring
transport with the resident door.

`nika compile <intent> --observe-only` (an `ObserveFlags` field beside
`CaptureFlags` on `CompileCommand`; the `CompileArgs` literal is unchanged)
prints `nika_onboard::remote_door::document`: `{observation_version: 1,
intent_sha256, observed_world, trial_inputs}` — `compile::observe::world` under
the working directory, unchanged, and the text of the files it read — then
stops: no compile, no seat, nothing written, refused beside a destination, a
base, answers or a seat. A remote door admits it by
`nika_onboard::compile::remote` (the seats crate's law).
`serve_args::NativeAuthoringArgs` carries `--decision-model` (requires
`--authoring-model`); `compile::decision_seat_note` is the receipt wording both
doors share; `typesafe::seat` opens a System One seat for a door that keeps it
for many rounds, with its key to withhold, never to print.

Trials. `CompileCommand.trials` (an `ObservedRoom`, `#[arg(skip)]`; the `nika`
binary names the working directory's room with itself as the jq helper, as the
Session does) makes a seated free-intent compile pass the core
`compile_with_cognition_rehearsed` that room: each final candidate is tried on
a scratch copy of its stated inputs (no provider, network, process or gate; the
originals untouched), the report reaches the judge and the repairs, and a
failed or missing trial is never READY. The library entries `run` and
`run_with` name no room and try nothing.

`compile::config` preserves the authoring configuration API as a re-export of
`nika_onboard::compile_config`. The shared parser, constants, typed errors and
their tests have one owner beside the authoring facade; existing import paths
and resolution behavior remain available without a reverse dependency.

## Private authoring capture

`nika compile --private-authoring-capture --authoring-model ...` opts into
bounded diagnostic capture of application Text returned by the authoring
seat. The default is off. The CLI selects `.nika/compile/capture` below its
process working directory; this is not a project-root resolution contract.
The model and HTTP callers cannot select a capture directory. The existing
`CompileArgs` literal remains unchanged; `CompileCommand` carries the new
`CaptureFlags` field. Its existing `new(args, authority)` constructor keeps
capture off; the command remains `non_exhaustive`.

The host checks its held container's ownership and private mode, then creates
or verifies an exact `*\n` ignore marker before reserving a round. It uses
`nika-fs::ReservedLog`: 1 MiB per round, with 16 KiB reserved for closing
metadata, a container allowance below 16 MiB and a 256-entry inventory bound.
Finished and failed reservations remain charged until operator removal.
Concurrent cooperating reservations share the filesystem mechanism's lock.
Storage refusal does not refuse compilation or discard its normal outcome.

A `CapturePolicy` combines explicit context admission with withheld values.
It refuses oversized policy input: at most 128 initial values, 32 KiB per
value, 256 retained raw/escaped needles and 512 KiB aggregate under the
constructor's documented counting law. Known resolved authoring and TypeSafe
credentials are withheld in raw and JSON-escaped forms. The CLI does not
admit a harness seat's Text merely because capture was requested. This is an
explicit host policy, not a claim to recognize every possible secret.

`CaptureContext` carries a trusted held directory, policy and local report;
`fresh` gives another round its own status. The observer runs around the
actual compile future. The capture holds response framing, counts, digests
and bounded selected metadata; withheld payloads retain their disposition.
An `outcome_returned` flag distinguishes a returned compile result from a
best-effort close on interruption. Capture observes returned authoring work,
not all physical provider sends. It is never a workflow, permit, semantic
record or replay input. Public status contains bounded facts, not raw Text
or paths. Separate cognition diagnostics remain subject to their own
contracts; capture does not sanitize those pre-existing public paths.

The CLI process tests in `tests/compile_cli/capture.rs` cover explicit opt-in,
public-output parity, marker refusals and resolved-key withholding with local
synthetic seats. Host and filesystem unit tests cover the shared mechanisms.
These results do not establish paid-provider, native TUI or Serve consumption
of the context seam, or program-wide private-capture qualification.

## Knowledge door

`compile::knowledge` composes, per intent, the bounded authoring pack a seat reads
from a Foundry release the strict door admitted against a trusted identity
(builder `nika-compile/knowledge-door-v4`; `--no-knowledge` turns it off; a
release `--knowledge` names carries no identity and is refused until one is
wired; `nika serve --no-knowledge` is the seat's explicit off). Its
selection is its own, Rust BM25 over the snapshot's graph, and is not the Foundry
producer's selection. Patterns and blocks are taken in relevance order, each
recalled family and then the direct text match in turn, never by row id; each
source's best-covering block is taken before any second. The receipt records,
per kind, `available`, `candidates`, `selected`, `presented` and `excluded` (with a
reason: the byte cap, a missing row or file), and `no_match` when nothing is
recalled. A block is presented with its row's holes, effects, authority,
capabilities, callables, known failure modes and version, within 1 KiB beside
its code. The caps stay finite: 3 · 8 · 4 · 3 · 1 rows, 6 KiB per file and
40 KiB per pack.

## Literal input binding

`literal_inputs::{read, validate}` owns bounded native API value decoding and
declared-input validation beside the `var_inputs` operator binder (owned by
`nika_service_execution::inputs` since C5 · 2026-09-28 and re-exported here at
the same paths).
The reader consumes at most 1 MiB + 1 bytes; JSON grammar/depth/numbers belong
to serde_json, with duplicate map keys rejected at every nesting level. Types
use canonical `parse_type` / `fits`, required values use the runtime refusal,
and provided/default origins are `api-caller` / `file`. No environment lookup,
expression interpolation, source mutation or execution lives in this seam.
The CLI owns stdin selection, conflicts and structured refusal rendering.

`run_protocol` carries the existing pure output/error/pause-envelope, carry and
resume-hint formatters. The carry quotes through the one shared `output::sh_word`;
no second quoting law exists. CLI rendering retains its tests and reuses those
formatters; literal payloads are never copied into the resume command.

## Run monetary review and observations

`run_cost` owns the interactive local unknown-price question and its
descriptor-rooted observation journal together. `nika-providers::admission`
owns the shared route/evidence/review contract; `nika-runtime::RuntimeConfig`
carries the confirmed live account to the existing execution composer. The
host uses the existing event source SHA-256 interface for an ephemeral candidate
witness over exactly the same source, inputs and project-default observation.
No dependency on Session or restoration of a persisted decision is permitted.

`run_budget` carries the normal operator budget preflight beside that review,
and `run_protocol::emit_diagnostic` preserves its existing stream projection.
CLI retains thin compatibility exports and still owns dispatch. Since B11
(2026-09-28) the preflight is priced over the workflow as the run binds it:
`preflight_bound` hands `--model` and the invocation's validated input values
to the runtime's one floor law (`nika_runtime::budget_floor_refusal_bound`), so
a fan over a bound input is priced at the items given, never at its declared
default, and the unbounded warning describes that same effective workflow.
`plan_gate` is the CLI's former cost gate over the frozen plan (the harness
predicate, then `preflight_bound`), descended here as the same monetary
adapter. `preflight` keeps its signature and prices no bindings.
The project default is read through `nika-vocab`; the journal uses `nika-fs`
`OwnedDir`. Neither move changes default/cap precedence, request/token/time
bounds, route binding, uncertain-charge stop, or subscription separation.
Since 2026-09-28 the journal's writer lease, strict fold (a killed Run's
UNKNOWN, recorded once) and torn-tail append live in `nika_dap::cost_journal`
(descended at the 15k wall); `run_cost` keeps the question, the live account
and its rows (`prepared` · `settled` · settle-on-drop). Since C3 the review
takes the lease and folds (`fold_as`, deriving and appending a killed Run's
UNKNOWN) before any refusal of its channel or the workflow's shape, so a
host that cannot ask still leaves the disposition on record and is refused
with it.

A Run with no unknown-cost route but an exact catalog-declared-free API route
(C2 · 2026-09-28) gets no question, lease or journal row. `review_with_model`
also takes the Run's `--model` (the lane its model-less tasks ride; `review`
keeps its signature and forwards none), refuses
any task the route's observation cannot admit through the shared
`nika_service_execution::run_cost::declared_free_shape` (Check's readiness
mirror consumes the same judgment), then binds a per-Run
`InferenceAdmission::observe_declared_free()` with the run's own jitter seed.
That observer never replaces the Run's `--max-cost-usd` preflight or runtime
budget; the trace's terminal frame carries its receipt, and `finish` only
closes it. A plan that mixes such a route with an unknown-cost one stays on the
unknown-cost review, which refuses it before any question.

Since C4 (2026-09-28), a `model:` rendered at run time is admitted as the route
it renders to. `readiness::unknown_routes` (descended to
`nika_service_execution::run_cost` in C5 and re-exported `pub(super)`) reads the provider predicate
`admission::unknown_cost_route`, which the registry applies to rendered routes.
With no unknown-cost route in the plan, the Run binds `InferenceAdmission::observe_run()`
whenever a static declared-free lane exists, any infer/agent task's own
`model:` is an expression, or it invokes a nested workflow, whose routes no
root plan sees; its children share that one account. Before that, `run_time_models` judges every value its
inputs, defaults or const already decide, exactly as the literal would be judged.
An unsupported free shape or an unknown-cost route is refused before any effect
(exit 3).
- Values decided later: undecidable values are refused at dispatch by the
  observer, before provider bytes and after earlier effects.
- Unknown-cost routes: a dynamic unknown-cost route has no review door; that
  gap is recorded as open, not supported.
- Answered legs: an inline answered leg binds `leg_observer`, the same fresh
  observer a manual `--resume` gets, never unknown-cost authority.
- Bookkeeping: the observer still has no lease or journal, and the budget
  preflight and cap are unchanged.

Since C6 (2026-09-28) the review is one shared path every host drives:
`prepare` runs the evaluator (`RunCostPlan::Unneeded` · `Observer` ·
`Review`), clearing the project (DAP `cost_journal::clear`) before the host's
own `ask` gate speaks, then frames one `ReviewedRun` under the host's
`CostHostEvidence` and its `launch` root. `ReviewedRun::confirm(answer,
source)` re-observes everything it bound before any authority: the source as
the host reads it now, the held project root and `.nika/` (a directory
replaced at the same path refuses), the journal's exact bytes after the
review's own clear, the project configuration, the bound files (read through
the held root: DAP `Cleared::observe_file` over Service `bound_files`) and the
route. Only then is the account confirmed, the Run's `RunJournal` (DAP) bound
to the held `.nika/` descriptor, and the `prepared` row written.
The host's `RunAccount` adapter gives DAP the provider-owned durable
`nika/inference-cost-observation@2` projection for each new prepared or settled
row. The outer journal schema and custody remain unchanged. If the observation
cannot be projected, it refuses before append; no exact-endpoint fallback is
written. The live account keeps its exact route and authority. This changes
new Run observations only: existing rows, their derivation and reconciliation,
and Session history retain their current contracts.
`review_with_model` is that path with the terminal as `ask` and the process's
current directory as `launch`; its refusals and their order are unchanged.
Serve's cost-review door (`--cost-review`) drives the same path with its own
startup-composed evidence. `serve_args` gains `--cost-review` and
`--run-cost-ceiling <USD|none>` (both require `--bind`). The observer decision
and account now come from Service (`observes` · `observer`).
Zero vetoes unknown spend before any question and no review can override
it: `--max-cost-usd 0` refuses in `prepare`, and a project `ceiling:` must
be a positive real (nika-vocab refuses 0, negative and NaN at the project
read, so no review is framed). A positive project ceiling stays an
overridable default; a model-free plan never reads it (pinned in
`run_cost::tests`).

Since B12 (2026-09-28) `prepare` frames the review from Service's typed
`dispatch_bound` over the Run's validated bindings (the operator's value before
the default) instead of `request_bound`. The review confirms its total
(`for_run`), its in-flight bound (`with_concurrency`), its per-task breakdown
(`with_breakdown`) and, only when a task authored `retry.max_attempts` above
one, the retry law (`with_authored_retry`). `ReviewedRun::dispatch_bound()`
exposes the same typed value, the single positive-work value. The review's question
(which the CLI challenge binds) and the Serve witness therefore cover exactly the
reviewed limits. The order is unchanged: clear, then the host's `ask`, then a
shape refusal in its own words (`Cardinality` for a count only the run decides).
One addition comes before `ask`: a zero total returns its own plan,
`RunCostPlan::Zero(observer)`. That is the no-paid-dispatch observer
(`observe_run`: no allowance, no lease kept), which grants no unknown-cost
authority, so an empty fan runs and sends nothing, as B11 left it. The variant,
never a field on the account, tells it from a declared-free `Observer`. It is
decided before any cap judgment, because sending nothing needs none.
`review_with_model` and Serve's unreviewed jobs bind it as they bind an observer.
Serve's version-2 door answers it as no review; version 1 refuses it. Right before the review is
framed, host evidence whose own `unknown_cost_refusal()` refuses every
unknown-cost choice returns `RunCostPlan::HardCapped(words)`. The words are
unchanged; that variant is the one refusal a host may teach its cap's remedy
for. `review_with_model` returns it as its refusal, as before.

`lines::fresh_terminal` is the fresh-input boundary of both local spending doors:
the plain session before its `continue once? ›` prompt, and `ReviewChannel::Terminal`
before each answer (again after `details`). It flushes the question, then switches
stdin to non-canonical `VMIN=0`/`VTIME=0` and discards, without blocking, everything
typed so far: this process's Rust stdin buffer, the terminal queue and an
unterminated line. It restores the saved mode and flushes the queue again (nix
`term`). It is not atomic with typing: queued input is discarded before the prompt,
but keys arriving between the final drain and prompt display may survive. A non-terminal
stdin, a failed mode change or typeahead over 1 MiB fails closed: no answer,
nothing sent. The negotiated Stdio channel keeps its one-response nonce document.

## Native TUI Run review transport

`lane::drive_reviewed_child` keeps one live CLI child, its PID cancellation slot,
normal event story and reply pipe across a fresh Run question. The visible
`--cost-review-stdio` option negotiates `nika/run-cost-challenge@1` /
`nika/run-cost-response@1`; it is never approval. A real controlling terminal and
local invocation scope are observed before this child may ask. Normal JSON,
Serve and ARM retain default refusal. One bounded response followed by EOF must
match the unpredictable challenge exactly; decline, malformed/duplicate input,
expiry or changed source/configuration/route refuses before composition.
`PendingRun::question` is the challenge's first screen; `PendingRun::details`
shows the complete evidence of the same challenge and writes nothing to the
child. The terminal channel prints those details once on `details`.

Providers admission owns the pending review, exact challenge and consuming
confirmation. The host owns configuration reads and the existing observation
journal; neither message nor receipt carries callable authority. The TUI owns
only rendering and the fresh human reply. `RunStory` is a compatibility export
of the pure `nika-display::run_story` projection; no rendering IO moves there.

## Bounded compound unknown-cost Run

For unknown-price API inference, `run_cost::shape` proves an upper bound over
one clean Check DAG: direct text `infer` calls on one literal admitted model,
at most one infer per Check wave, each with explicit `max_tokens: 1..8192`.
The fresh S85 question shows the derived request maximum and the existing
8192-output-token / 120-second per-request bounds. Conditional skips reduce
observed calls; they never increase the reviewed maximum. Provider admission
independently enforces that maximum and one request in flight.

Surrounding steps are exactly `nika:read`, `nika:write`, and `nika:jq`.
File paths are literal relative files confined to the launch project;
output parent directories must already exist and are opened without following
symlinks. Pre-existing read inputs are limited to 1 MiB each and fingerprinted through
descriptor-rooted no-follow opens. Their fingerprint joins source, bound input
values and the reachable project configuration in the pending review and is
re-observed after yes, before composition. Check, tool permits and the existing
runtime filesystem boundary still independently authorize effects.

Unsupported shapes refuse before inference: multiple/overridden/dynamic routes,
same-wave parallel infers, nested workflows, agents, exec, other tools, fan-out,
retry, recovery, schema re-asks, thinking, vision, and external secret bindings.
Read files must already exist; this initial shape does not support workflows
that first generate their read inputs. No-inference, catalog-priced and
subscription lanes retain their existing paths. Observation persistence records
actual partial/uncertain attempts; a bound is never reported as observed usage,
and monetary choice never creates execution permission or a reusable grant.

The shared knowledge implementation lives in `nika-onboard::knowledge`, beside
the authoring facade. `compile::knowledge` remains a compatibility re-export;
CLI flags, host observation, admission and decision-seat adapters stay here.

`compile::observe` keeps the observation's I/O: resolving a stated path under
the project root and refusing a link out of it, the bounded peek, the whole
read of a small JSON file. Its pure half (a CSV header's columns and delimiter,
the records' keys, the short categorical values, the raw kind of every sampled
value) is the compile unit's law, `nika_onboard::compile::observation` (R4 A5),
so the CLI and the Session, which share this observer, share one law. The world
carries the kinds beside the rows (`kinds`, keyed by path: counts only, never a
value). A small JSON file parsed whole within the bound states its keys as
complete evidence, keyless records included, so a request that asserts a field
no record carries is not READY; the kinds still come from the first 200 sampled
values, and nothing here validates every value. An empty array or no records, a
failed whole read, a head over the bound and JSONL remain partial evidence.
That completeness is the one change a row of a whole-parsed JSON file carries;
otherwise a row keeps the bytes it had, so a plan or a verified transform bound
to a recorded row stays bound to it.

`--authoring-reasoning low|high|max` (else `NIKA_AUTHORING_REASONING`, read only
when a seat is named) is the reasoning effort every authoring and decision call
asks; the flag needs `--authoring-model`, `--decision-model` or both (one clap
group). The shared Onboard producer (`compile_config`) judges the word for every
door: the flag outranks the environment and never falls back to it, and any
other word refuses the configuration (exit 3) before any request. It also owns
the per-call caps (`call_bounds`: the operator's, else 8192 tokens and 120 s,
300 s for a harness) and the policy (`AuthoringConfig::policy`), which Serve's
native seat builds from too; the effort never moves a cap. With a level, the
decision seat asks it under the declared authoring cap; without one it keeps its
256-token request. `nika serve --authoring-reasoning` (`NativeAuthoringArgs`,
requires `--authoring-model`) reads the flag, else the environment, once when
the seat is named; an unknown word refuses the seat before the listener binds.
The provider adapter sends a level only on a catalog-qualified route and refuses
a gateway or base-URL override before a byte leaves; the receipt separates the
configured level from the keys read back from the dispatched body.

`compile::sidecar` records the plan an answer round replays. A replay whose outcome
re-anchors the plan to a changed source (its observation or the keys asked again moved, R4 A6)
replaces the record atomically once the compile succeeded, so the next `--answer` binds against
the observation the question showed; a write failure keeps the old record and names the error,
and a verified or pending transform is never re-recorded over another source. A round whose
candidate its judge could not judge (the core's Applied `verify_resume` finding) is recorded
with `resume: true`, and a plain re-run replays it, so the judge is asked again on the same
bytes with no authoring call; once such a round is judged and leaves nothing to record, its
record is removed. An outcome carrying the core's Applied `verify_held` finding (its judge
answered the candidate and did not accept it) is never recorded, and this intent's record is
removed whether this invocation replayed it, resumed it or left it unread, so no later
`--answer` round replays those bytes to that judge. The human text says « plan record removed ·
`<path>` · its judge did not accept the candidate: no later round replays it, the next compile
authors again »; a removal that fails is named (« plan record not removed · `<path>` ·
`<error>` · a later --answer round would replay the candidate its judge did not accept: remove
the file, or compile with --fresh ») and carried in the machine document's
`plan_record_error`. A held outcome is INCOMPLETE: as for any outcome that is not READY,
nothing is written to the destination and the compile exits 2. An answer round with neither
`--authoring-model` nor `--decision-model` replays the record through the core alone: every
record but the reader's own HOT plan keeps its whole request pending, so such a round is
INCOMPLETE with the core's pending-clause finding, never READY on bytes no judge of that round
carried.

Beside the plan record, `.nika/compile/<intent sha256>.declined.json` keeps the verdicts that
rejected candidate bytes of the intent in earlier rounds: each `semantic_verification` attempt
`nika_onboard::compile::round::rejections` lists (declined, rejected and not settled, never a
repeat; an abstention is never kept), the latest per candidate digest, judge, request and
context (`keep_rejections`), with the
same `compile_version`, `engine` and `intent_sha256` guard as the plan record (an absent,
unreadable, foreign-engine or foreign-intent file is not read, and the next rejection rewrites
it). Every compile of the intent carries them, `--fresh` or not
(`CompileRequest::with_declined`), so a round that authors those bytes again for the same
request never asks the same judge on them: its rejection stands with no call, and the human
text says « declined verdicts · `<path>` · its judge rejected these bytes in an earlier round:
it was not asked again (--fresh keeps them) ». A compile that adds a rejection rewrites the
file atomically in the same self-ignoring directory; a write that fails is named (« declined
verdicts not kept · `<path>` · `<error>` · a later round could ask a judge again on bytes it
rejected ») and carried in the machine document's `declined_record_error`.

`compile::typesafe::session` owns the operator-selected typed decision adapter
and its observation journal beside the one TypeSafe transport. Session supplies
an explicit admission verdict and persists the observations; conversation text
is never such a verdict. Each finite compiler need makes one attempt, with no
implicit call-count cap or transport retry. The observation's `role` states what
the seat decides: clause reading, feasible-plan ranking and semantic verification
(the whole request, then each part alone, the task a missing part points to, extra
operations and the questions over a trial run), NONE allowed, never Foundry or knowledge
selection, never authority. Each attempt records the question id and its offered keys
(`task-<id>`, `part-<k>`), never the question's state, so a trial run's texts sent to the
seat are not journaled here. Each attempt that left also records `elapsed_ms`, the time
its transport took as measured here (the System One partition reads no clock).
`SessionSeat::finish` consumes the scope; Drop performs the same conservative closure on
errors or cancellation.
The @2 observation records `scope_ended`; only fully answered or unsent attempts
close as Closed. A send with no observed result stays Uncertain. Cost remains
unknown after either state, never assigned to the author's account or priced0.
An unavailable journal refuses before transport. Old @1 observations are retained
unchanged. Pure status wording is projected by `nika-display::model_scope`.
The Session public selection path remains a compatibility re-export; the former
implicit-cap constant `MAX_DECISION_CALLS` no longer describes this behavior and
is removed. Explicit CLI Compile and workflow limits are unchanged.

## Doctor local protocol evidence

`doctor --ping` keeps TCP reachability separate from a compatible model list. The
opt-in probe makes one GET to each effective local-protocol base plus `/models`,
with a one-second request timeout, 256-KiB response bound, no redirects or retries,
no credentials, inference, download or cloud-profile probe. URL userinfo, query
and fragment are removed. Responses use the provider-owned strict listing decoder.
An empty compatible list, incompatible protocol and failed observation are distinct
from advertised models. JSON exposes `local_model_probes` and `inference_tested:false`;
human rows state that inference was not tested. No opt-in leaves observations absent.
The existing `Probe` and render function signatures remain compatible.

The direct-API authoring backend records `cost_basis: unpriced; billing_unverified`. Its usage completeness and requested/observed model evidence are separate observations; this door does not calculate a catalog valuation or certify billing. Harness-provided descriptors retain their own basis.

`redact_authoring_error` re-exports the provider-owned authoring error projection. It preserves local AdmissionDenied with an engine-authored remedy and removes provider text from all other failures. Serve consumes this shared host boundary; the compiler keeps its typed refusal distinction.

Direct API authoring endpoint metadata comes from the exact seated registry: `host` strips user info, path, query and fragment; `base_url_overridden` compares the effective URL with its profile seed when available. `endpoint_basis: operator_configuration` distinguishes this configuration from an authenticated remote identity or an observed model. Session host diagnostics use the same redaction.

`probe::operator_account()` reads the process account from the OS: its uid,
and its login name when the account database names one. It never reads a
flag or the environment. This is the principal `nika trace cost reconcile`
records: attributable, not authenticated, through the existing `nix` user
feature.

## Passive views and the repair prepass

The welcome and choice views render through `nika_display::front_door`; the host
still collects facts, resolves the context envelope, redacts endpoints and selects
next actions. Doctor keeps diagnosis, row visibility, link preparation, receipt
projection and exit codes here; `Finding` and `Level` remain compatibility
re-exports of the passive human/JSON view types.

`fix_ladder` calls the four pure prepass helpers in `nika_migrate` and re-exports
the repair records and renderers from `nika_display::repair_render`. Applying,
judging and rolling back repair rounds remain host responsibilities.
