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

`compile::config` preserves the authoring configuration API as a re-export of
`nika_onboard::compile_config`. The shared parser, constants, typed errors and
their tests have one owner beside the authoring facade; existing import paths
and resolution behavior remain available without a reverse dependency.

## Knowledge door

`compile::knowledge` composes, per intent, the bounded authoring pack a seat reads
from a pinned Foundry snapshot (builder `nika-compile/knowledge-door-v3`). Its
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
value); a row keeps exactly the bytes it had, so a plan or a verified transform
bound to a recorded row stays bound to it.

`compile::typesafe::session` carries the bounded operator-selected decision
adapter and its observation journal beside the one TypeSafe transport. Session
still decides whether its current monetary account admits a call and persists
observations; the host helper accepts that explicit verdict, not conversation text.
The existing Session public selection path remains a compatibility re-export.

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
