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
| Dependencies | `nika-runtime` · `nika-execution` · `nika-check` · `nika-event` · `nika-schema` · `nika-types` · `nika-providers` · `serde_json`; dev: `nika-fs` · `tempfile` · `tokio`. |
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
