# Crate spec — `nika-service-execution`

| | |
|---|---|
| Status | **WORKSPACE WIP** — descended out of `nika-runtime` at the 15k prod-LOC wall; behaviour is unchanged and already exercised by CLI/ARM, but the crate stays in the canonical `wip` set until its own admission ceremony closes. |
| Layer | L3 — execution driver over an already-admitted world |
| Design | One driver, two surfaces. The same composition root, child resolution, closure hashing, and capability intersection serve the local CLI/ARM surface and the service surface; only the local stderr projection and the trace lane differ. |
| LOC budget | ≤2,000 source lines; ≤15,000 hard crate cap. |
| File cap | ≤1,500 lines. |
| Function cap | ≤100 lines. |
| Crate version | tracks workspace |
| License | `AGPL-3.0-or-later` |
| Publish | `false` — engine-internal driver |
| Dependencies | `nika-runtime` · `nika-execution` · `nika-check` · `nika-event` · `nika-schema` · `nika-types` · `nika-providers` · `nika-harness` · `nika-verb-infer` · `serde_json`; dev: `nika-fs` · `tempfile` · `tokio`. |
| NIKA codes | none allocated — child refusals reuse the spec-plane `NIKA-COMP-001` and the runtime's own typed codes; no new registry range. |

## 1. Purpose and boundary

`nika-service-execution` is the ONE production execution driver over an
admitted, byte-owned world. It joins the two L3 peers that own the halves it
needs and owns neither itself:

- `nika-execution` owns byte admission — the immutable `ExecutionSnapshot` and
  the unforgeable `ExecutionContext` that binds snapshot bytes, parsed
  workflow, check report, resolved skills, and execution identity.
- `nika-runtime` owns generic wave-ordered execution and the production
  composition root (`compose::production_runtime` / `compose::service_runtime`).

The driver is deliberately **filesystem-blind**. An L4 adapter readmits an
owned snapshot through `nika-execution`; from that point every definition read
— root, nested `workflow:` child, Agent Skill, closure digest — is served from
the snapshot's in-memory map. The crate holds no `OwnedDir`, opens no path, and
takes no reader callback. A permanent census in
`crates/nika-cli/src/verbs/run/extinction_tests.rs` pins that: the driver source
must contain no `std::fs::read(`, no `read_to_string(`, and no
`nika_fs::OwnedDir`.

### Why a separate crate, and why same-layer

`nika-runtime` measured **16,024 production LOC against the 15,000 hard cap**
once the shared driver landed inside it (base: 14,931). The driver is the
natural seam: it is the only part of the runtime that knows about snapshot
admission, and it was the only reason `nika-runtime` depended on
`nika-execution` at all.

The split follows the `nika-proof` (2026-07-29) and `nika-secret` (2026-08-06)
precedents — descend a cohesive unit rather than shave lines. It sits at **L3,
the same layer as both peers**, which the layer contract permits (same-layer
deps are legal; only *upward* deps are refused — see
`scripts/ci/check-layering.sh`). The mechanical sort agrees: this crate
consumes two L3 crates and no interface crate, so it cannot be L2, and it is
not an interface, so it is not L4.

### What the descent removed from `nika-runtime`

- the `service_driver` module and its eight public types;
- the `nika-execution` dependency (the driver was its only user);
- the `tempfile` dev-dependency (same).

It added exactly **one** seam: `compose::service_runtime` widened from
`pub(crate)` to `pub`. That is a net public-surface *reduction* for
`nika-runtime`, and it puts the service composer beside the already-public
`compose::production_runtime` it mirrors.

## 2. Public API

```text
ServiceExecutionDriver     — the driver; new() = service surface,
                             for_local_interface() = CLI/ARM surface
AuthorizedRuntime          — a ProdRuntime sealed to one admitted
                             workflow/report pair
ServiceExecutionOptions    — caller-supplied inputs, origins, model, ceiling
ServiceExecutionResult     — redacted status + metadata-only event projection
ServiceExecutionStatus     — Succeeded | Failed | Paused | Refused
ServiceEvent               — one metadata-only projected event
ChildTrace                 — one nested run's injected trace lane
ChildTraceFactory          — factory for those lanes
ChildTraceMetadata         — what a lane commits back to its parent
```

Every public type is `#[non_exhaustive]` where it is a struct or enum with
fields (FCI-002).

## 3. Sealed authority (the invariants this crate exists to hold)

1. **No caller-forgeable pair.** `ServiceExecutionDriver` is constructed only
   from an `ExecutionContext`. There is no constructor taking a workflow, a
   report, skills, a snapshot, a source hash, or a closure map. `with_task_scope`
   lets a caller *select* a task cone; the narrowed workflow and its fresh
   report are then derived internally.
2. **`AuthorizedRuntime` cannot be re-pointed.** It exposes no constructor and
   no setter for workflow, report, skills, closure digests, or source hashes.
   `run()` always executes the internally bound pair. The `with_*` knobs refine
   a run (inputs, origins, ceiling, model, pause, approval, resume plan) and
   nothing else.
3. **Metadata-only service projection.** `ServiceEvent` carries id, timestamp,
   kind slug, run, execution, and correlation — and nothing else. Task output,
   provider/tool payloads, prompt text, failure detail, and filesystem paths are
   absent by construction, so a future runtime field cannot leak by default.
   `ServiceExecutionStatus` collapses the runtime's typed error to `Refused`.
4. **No filesystem reopen after admission.** See §1.
5. **Child composition is default-deny.** `effective_permits` intersects the
   child's declared permits with the parent's; an absent parent block caps every
   child at zero authority. A child that fails its own `check_composed` is
   refused before any effect. Registry children are refused, not resolved.
6. **CLI/ARM parity.** Both surfaces run the *same* `base_runtime` selection,
   the same child runner, the same closure digests, and the same source hashes.
   `DriverSurface` changes only (a) whether builtin `nika:log`/`nika:emit`
   payloads are projected to stderr and (b) which trace lane is injected.

## 4. Tests

`src/tests.rs` (in-crate, `#[cfg(test)]`) covers:

- `service_event_projection_drops_every_runtime_field` — a `TaskCompleted`
  event carrying a secret-shaped `output` field projects to metadata only.
- `absent_parent_caps_every_child_at_zero` — the permit-intersection lattice.
- `service_driver_runs_a_child_from_the_owned_snapshot` — a real nested
  `workflow:` run out of the captured world, every event stamped with the
  execution identity.
- `independently_parsed_workflow_and_report_cannot_replace_the_admitted_pair` —
  a separately parsed, deliberately unclean workflow cannot displace the
  admitted one.
- `service_result_never_exposes_secret_shaped_output_material` and
  `service_result_never_exposes_pause_material` — redaction across `Debug`, the
  accessors, and `into_parts`.

The structural census lives with the CLI (`extinction_tests.rs`) because it
also pins the adapter side of the same contract.

## 5. Gate exemptions (WIP)

| Gate | State |
|---|---|
| 5 MUTATION | owed at admission — the crate is WIP. |
| 7 BENCHMARKS | N/A — the driver is orchestration; the hot paths it calls (parse, check, hash, the verb crates) carry their own benches. |
| 9 CANARY | covered transitively — every `nika run` canary drives this driver through the CLI adapter. |
| 10 PARITY | N/A — no legacy counterpart; the code is a byte-preserving descent of `nika-runtime::service_driver`, whose behaviour the CLI suite already pins. |

Gates 1/3/4/8/12 hold at the descent commit; gates 2/6/11 are owed with the
admission ceremony.

## Finite unknown-cost Run shape

The filesystem-blind `run_cost` module owns three observations, none an
execution or a spending grant (the third, `declared_free_shape`, follows):

- `request_bound(workflow, access_plan, unknown_routes)` returns the finite
  physical-request upper bound for static sequential direct text inference.
  Each schema task includes `1 + nika_verb_infer::DEFAULT_SCHEMA_RETRY_BUDGET`
  requests. It rejects parallel inference, task retries, fan-out, recovery,
  external secrets, exec, agent, vision and explicit thinking. Local read/write,
  jq and the builtin pure boolean assertion remain subject to Check/permits.
- `project_file_path(consts, action)` resolves only literals or bare immutable
  string constants into a project-relative path. It performs no I/O; it cannot
  attest filesystem containment or grant access.

Both refuse with the typed `RunShapeError` (`#[non_exhaustive]`, thiserror):
one variant per refusal condition, among them the route, overflow, unsupported
tools and actions, and dynamic or unconfined project paths. `Display` is the
unchanged refusal wording hosts render. It owns no NIKA registry range: it is a
host-side static observation that never enters the workflow or verb plane (the
`transport-surface` exemption of the error one-voice gate).

`dispatch_bound(workflow, access_plan, unknown_routes, bindings)` (B12 ·
2026-09-28) widens `request_bound` to finite fans and authored retries and
returns the typed `DispatchBound`: the worst-case total of physical requests,
the requests in flight at once, and one `TaskDispatch` row per infer task
(items, authored attempts, calls per attempt, width, requests). It judges the
workflow as the run seats it (`nika_runtime::effective_workflow` over the
validated bindings: an operator's value before the declared default). The
counts are the check's own cost law on that seat (`iterations × attempts`),
each attempt carrying the stock schema re-asks, every product and sum checked
(`Overflow`). A fan must iterate a literal list or a bare input/const array whose
value the seat knows; a task output, a computed or navigated expression, or an
input with no value or default refuses as `Cardinality`, even under a
`max_items` cap. `on_error`, a fan or retry on a non-infer step, exec, agent and
nested workflows stay refused. A task's width is its declared `max_parallel`
(all items when absent, never more than its items). Waves run in order, so the
Run's in-flight bound is the widest task. A zero total is a value, never an
allowance: hosts route it to their no-paid-dispatch observer.

`DispatchBound::authored_retry()` is true only when a task authored
`retry.max_attempts` above one. It is the sole source of a choice's
authored-retry law. Fan cardinality, the total and schema re-asks never imply
it; a schema re-ask is an extra call inside one attempt, never a transport
resend. `lines()` is the breakdown a fresh choice shows, one line per infer
task, and is empty for a single sequential Run, which keeps its historical
words.

`declared_free_shape(workflow, access_plan, providers_config, model_override)`
(C2 · 2026-09-28) says whether an admitted API lane is an exact
catalog-declared-free route and refuses, with the typed `FreeShapeRefusal`
(`#[non_exhaustive]` struct: task, model, shape), the first task on such a route
that its observation cannot admit: an `agent:` loop, enabled thinking, vision,
or a `max_tokens` missing, zero or over the tariff's output bound. Check's
readiness mirror and the host's Run observer consume the same judgment before
any effect, so an unsupported shape is never run-ready and never a known zero.
The provider wire guard still refuses what it alone can see (the rendered body
over 1 MiB).

`run_time_models(workflow, access_plan, providers_config, overrides)` (C4 ·
2026-09-28) covers the `model:` values a plan never sees. Each infer/agent task
whose own `model:` is an expression is judged at the value
`nika_runtime::resolve_model_expr` gives it before any effect (`--var` or
`--inputs-json` over the declared default, const, a `with:` alias), exactly as
that literal would be. A declared-free route in a shape its observation cannot
admit refuses as `RunTimeModelRefusal::FreeShape`. An API route whose USD cost
`unknown_cost_route` calls unknown refuses as `UnknownCost`: a fresh
unknown-cost choice binds only a literal `model:`. A dynamic finite-call review
door is an open follow-up. A seated or local value never reaches the registry
and is not judged. `Ok(true)` means such a task exists, or a nested `workflow:` invoke whose
routes no root plan sees, and the host binds the Run observer. A value only the run decides (an upstream task output, a loop
item, CEL beyond the walk) is judged by that observer at dispatch: refused
before provider bytes, but after the effects of the tasks that ran before it.

The L4 host owns descriptor-rooted input observations, fresh source/route-bound
consent and the live monetary account. The provider account meters every actual
request, including schema re-asks, and refuses exhausted or uncertain authority.
The new L3-to-L2 dependency on `nika-verb-infer` reads its existing production
retry constant; it introduces no alternate counter or composition path.

## Host-configured monetary admission

Three more static observations moved here from the host at its 15k wall
(C6), each beside the laws it composes, none a grant:
`bound_files(workflow)` lists, in task order, the project files a Run's
`nika:read`/`nika:write` tasks bind (`BoundFile { path, write }`, where a
write's literal `create_dirs: true` alone lets a missing parent through), or
that task's `project_file_path` refusal; `observes(...)` decides whether a Run
binds the per-Run observer (an exact declared-free lane or a run-time
`model:`, judged against its inputs; without inputs any doubt binds), in the
Run's refusal words; `observer(workflow)` builds that observer's account and
configuration, keeping the run's jitter seed. The descriptor-rooted file
observation itself lives in DAP (`Cleared::observe_file`).

`ServiceExecutionOptions::with_runtime_config(config)` (C6) hands the same
host-bound configuration to `execute`, which composes through
`compose_configured` exactly as `compose_with_config` does; absent, `execute`
composes as before. The builder lives in `run_cost` beside the finite-call
analysis; the options keep one private field, so the closed struct stays
additive. It grants no effect: the host owns the evidence, the review and the
account's settlement.

`compose_with_config` selects service metadata-only versus local stderr
projection at the same `production_runtime_with_emitter` seam. The host must
have scope-bound the live account in `RuntimeConfig` before composition; this
does not grant effects, replace the admitted workflow/report, or replay a
persisted observation. The driver remains filesystem-blind. No configured
composition wrapper or Runtime-to-driver dependency is introduced.

Since C4 (2026-09-28), a configured composition uses the same emitter and
sandbox root as the unconfigured one. The service surface uses its display
root; the local surface uses `production_runtime`'s launch cwd, where it once
used the display root. An account therefore never moves where exec effects
land. A nested `workflow:` child of a configured Run composes with the root's
own account, a clone of the same handle, never a fresh one. An uncertain charge
in the parent then still refuses the child's calls on that account, and the
child's terminal frame carries the shared receipt. This is enforcement at the
child's dispatch boundary, after the root's earlier effects. It does not judge
a child's readiness or cost multiplicity before the Run. A child's unknown-cost
review and a full closure inspection remain open follow-ups: such a refusal
never certifies the whole workflow.

## Input binding (C5 · 2026-09-28)

`inputs` is the one `--var KEY=VALUE` coercer. It descended verbatim from
`nika-cli-host::var_inputs` at the 15k crate wall, and the host re-exports
`ValidatedInputs` and `parse_var_overrides` at their old paths, so the Run, the
golden test, `arm fire` and schedule readiness bind through this one door.
Two E16 fixes landed here, once for every door:

- **E16-3** · a value that reached the coercer through `@env:VAR` and does not
  fit the declared type is withheld: `--var locale=@env:VAR: expects
  \`integer\` — the value of VAR does not fit (withheld)`. A literal keeps its
  wording (the operator typed it).
- **E16-4** · `check_bindings(pairs, workflow)` judges every pair on its own
  and names the input with its `BindingFault` (`malformed`, `unknown_input`,
  `env_name_missing`, `env_undeclared_in_ci`, `env_unset`, `type_mismatch`),
  never the value. A correct binding is never blamed for a neighbour.
  `BindingFault` and `BindingCheck` are `#[non_exhaustive]`.

The literal law under `--var` is shared with the resident's schedule binding
(C6, approved P1): `inputs::declaration(workflow, key)` finds the input a key
names verbatim, or refuses `UndeclaredInput { declared }` (with the shared
`teaching()`), and `inputs::coerce_literal(declaration, text)` binds the text
by its declared type (a `string` keeps its raw text) or keeps the untyped
JSON-or-string guess, or refuses `Misfit { why, expects }`. Each door applies
its own `@env:` policy between the two halves, so an undeclared key refuses
before its text is judged and a channel before any type; each keeps its own
words (`--var` for the CLI, with env values withheld, `inputs.` and HTTP codes
for the resident).

A transport caller's bindings have their own provenance door, `caller`
(C6, descended from Serve's resident door):
`ServiceExecutionDriver::caller_origins(inputs, origin)` gives each supplied
key the caller's origin (Serve passes `ApiCaller`), and each declared input
left unbound keeps what `nika_runtime::input_origins` derives with no CLI
channel (a default is the file's; an input with no default has no entry).
Nothing is read from the executing process, and the inputs themselves are
checked before, by the door's literal law.

## Scheduled program readiness (C5 · 2026-09-28)

`scheduled_program(workflow, report, providers_config, bindings, ceiling_usd)`
judges an unattended fire read-only (`nika arm fire`: the beat's pairs, its
`plafond`, no review channel). It returns `ScheduledProgram`
(`#[non_exhaustive]`): `required_inputs_ready`, `model_cost_ready`
(`Some(false)` on a blocker, `None` when a route is judged only at dispatch
or only a harness seat could serve it), value-free `ReadinessBlocker`s and
the document the receipt carries (`required_inputs`, `optional_inputs`,
`undeclared_bindings`, `unbound_inputs`, `model_summary`,
`authority_summary`).

- A refused binding is one `input_refused` blocker with its reason; a
  required input with no source (literal, declared environment or workflow
  default) is `input_unbound` under the registered `NIKA-1708`, never also
  refused. `unbound_inputs` is R4 71's set: required minus bound.
- The model law reuses the Run's owners: the plan over key presence only
  (`collect_provider_probes`, no harness spawn), `unknown_routes` (descended
  from the host Run review: the host re-exports it), `declared_free_shape`,
  the run-time routes `resolve_model_expr` decides from the bindings, and
  `budget_floor_refusal_seated` under the plafond (`NIKA-1709`). A task whose
  model only the run decides, a nested `workflow:` whose routes no root plan
  sees, and a plan refusal stay unknowns.
- A model the bindings decide is written into its task as the literal it
  renders to. Access over the same key-presence rows,
  `nika_execution::model_admission_findings` (resolution, thinking,
  capacity: `model_admission_refused`), the unknown-cost and free-shape laws
  and the budget floor then judge that literal world, exactly as a literal
  `model:` is judged at capture. Key presence reads this process's
  environment (a host assumption the receipt states).
- `authority_summary` lists what an unattended fire needs (permits, secret
  sources, human gates) and states `activation` and `monetary` as
  `not_acquired`.

## Probe rows only when a plan reads them (C5 · 2026-09-28)

The driver no longer collects `access_probes_env()` at construction. Its rows
live in one shared lazy cell (clones and child runners read one snapshot) and
every plan goes through `lazy_plan`: without a pin, a plan with no static
model lane is identical for any rows (no lane, no seat, no pin refusal), so no
row is collected and no harness CLI is spawned for it; a pin, or a static
model lane in the root or in any child of the captured world, collects exactly
as before. `with_access_probes` presets the cell. Dynamic run-time routes are
unchanged: the runtime judges them with its own composition probes.

**Open gap:** `nika-runtime` composition (`compose.rs`,
`collect_access_probes_env`) still probes eagerly, so a model-free fire still
spawns the harness CLIs there (E16 side observation). This change removes one
of the two spawn sites; it does not claim no-spawn parity.

`run_cost::readiness` owns the monetary readiness observation used by Check.
It shares the route, declared-free shape and finite-request laws with Run;
unknown-cost work still needs a fresh choice. The observation admits no effects
and obtains no spending authority. The Host adapter supplies its provider configuration.
Since B12 it reads the same `dispatch_bound` at declared defaults: a fan or an
authored retry names its total and in-flight bound. A zero total needs no choice,
because the Run takes its no-paid-dispatch observer. A single sequential Run
keeps its historical sentence.
