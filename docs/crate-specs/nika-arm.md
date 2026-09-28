# Crate spec — `nika-arm`

| | |
|---|---|
| Status | **ADMISSION CANDIDATE** — extracted from the proven ARM custody code in `nika-cli`; named-beat tick policy lives in `nika-cadence`; behavior remains guarded by the `nika-arm` library suite plus the real CLI `arm_fire` and Serve tests. |
| Layer | L4 — interface-shared custody library |
| Design | Descriptor-rooted `.nika/arm/` state, verified replay/rotation, kernel leases, and the one injected firing transaction. Interfaces inject execution and waiting; they never reinterpret the ledger or discover a trace globally. |
| LOC budget | ≤5,000 source lines for this custody unit; ≤15,000 hard crate cap. W04 measurement: 4,732 lines including inline tests. |
| File cap | ≤1,500 lines; W04 maximum 1,263 in `state/tests.rs`. |
| Function cap | ≤100 lines. |
| Crate version | tracks workspace |
| License | `AGPL-3.0-or-later` |
| Publish | `false` — engine-internal interface library |
| Dependencies | `nika-cadence` (pure schedule/ledger authority) · `nika-execution` (shared owned-byte admission/execution service) · `nika-fs` (`OwnedDir`) · `jiff` · `nix` · `serde_json`; dev: `tempfile`, `sha2`. |
| NIKA codes | none allocated — failures are typed I/O results or the existing ARM process exit contract rendered by the calling interface. |

---

## 1. Purpose and boundary

`nika-arm` owns the effectful half of ARM once for every interface. It keeps a
beat's kernel lease from decision through terminal receipt, holds one project
directory capability across sidecars, workflow capture, and execution, appends and fsyncs the verified
ledger, rotates legacy evidence without erasure, and derives projections only
from replay. `nika-cadence` remains the L0 authority for registry grammar,
slots, the named-beat tick classifier (`tick_decision`), firing transitions, hashes, and borrowed-text ledger semantics.

The CLI and resident Serve loop are adapters. They may discover `nika.yaml`,
render a verdict, supply an execution closure, or replace the default sleep
with a signal-aware wait. They may not own ARM locks, path traversal, journal
repair, source pinning, receipt fencing, or trace attribution.

This split closes two previously measured faults: firing a captured workflow
under a temporary pathname rebased its relative children and skills, while
finding the newest trace by directory scan could attribute a concurrent run's
trace. The shared transaction now admits the complete descriptor-rooted workflow
world once through `ExecutionService`, executes that immutable snapshot under
the declared logical path, and accepts the exact trace identity returned by the
same in-process service.

## 2. Public surface

- `ArmState::open` binds a fallible project capability; `at_project` retains
  the ergonomic constructor but stores any root refusal so every later operation
  fails closed. Both root every operation at `<project>/.nika/arm` and
  exposes verified projections, tallies, unsettled claims, orphan labels,
  migration inspection, healing, and lifecycle folding.
- `FireCtx::new(..., RunSeam)` derives the label and state from one root plus
  registry index; callers cannot pair a workflow with another label or sidecar.
  `with_wait` is the resident signal seam. Fields stay private and the registry
  returns through `into_registry` after the borrow ends (or through
  `FireCtxError` when the index is invalid).
- `fire_beat(&FireCtx) -> FireVerdict` performs lock → re-read → decide → claim
  → injected run → fenced receipt → release. `FireVerdict::into_parts` is the
  interface projection.
- `RunShot` exposes request metadata: the held project capability, display root,
  declared workflow path, generation, and spend ceiling. `RunSeam` receives the
  service-issued `ExecutionContext` beside that request, so it reads the complete
  immutable snapshot and its direct execution/trace identity rather than reopening
  workflow inputs. `RunUpshot::new` returns the process exit and escaped trace path.
- `HealOutcome`, `Rotation`, and `Folded` expose read-only accessors; public
  structs are non-exhaustive and carry no constructible public fields.
- `ArmState::inspect(label, now)` returns a non-exhaustive `ArmInspection` with
  last firing, folded lifecycle, and tallies from one verified journal snapshot.
  This observation opens existing paths beneath the held descriptor and never
  creates, locks, heals or rewrites a projection. Absent evidence yields empty
  projections; unsafe paths or corrupt evidence refuse. It grants no firing
  lease and makes no guarantee against a later concurrent change. The existing
  `last` repair-capable API remains unchanged for its mutation-authorized callers.
- `unit_io` owns the existing explicit emission environment-path persistence,
  unit-file writes, and printed load instructions. It preserves the prior CLI
  filesystem behavior and refusal text, exposes a typed input/host error, and
  never loads a unit or acquires schedule authority. Parsing, human rendering,
  and exit-code projection remain in the CLI. Emitted OS files use the existing
  operator-selected paths; this descent does not claim additional path custody
  or change the separately descriptor-rooted firing journal.

No default API accepts an arbitrary sidecar path or raw ledger mutation. The
one production mutation outside firing is the typed `record_disarm`, which takes
the beat then ledger lease. Labels remain single contained components; paths
are opened descriptor-relatively through `nika_fs::OwnedDir`, every workflow
component uses `O_NOFOLLOW`, and PID text is diagnostic only: the kernel lease
is authority. Cross-crate fixtures use a non-default `test-support` feature;
that surface is absent from normal builds and the committed public API snapshot.

## 3. Determinism and durability laws

1. Time, wait, process id, and execution are injected at the interface edge.
2. Before the claim, `ExecutionService::admit` captures the descriptor-rooted
   primary workflow, transitive child workflows, and skill files once. The
   generation binds the validated beat and admitted root bytes; later mutations
   cannot change the execution world.
3. The service allocates `ExecutionId` before the claim. The durable claim and
   terminal receipt carry the same `exe-<uuid>` plus its direct 32-hex trace ID,
   along with slot, generation, and fencing authority; a crash leaves that exact
   association on the unsettled claim for replay.
4. The beat lease spans the entire decision and run. A queued wait always
   re-reads and re-decides after sleeping.
5. Replay validates the full archive/live snapshot and durable head before any
   projection or append. A cache never overrides the chain.
6. Rotation is first-event-only and commits the ordered archive bundle.
7. The execution seam returns its exact trace path from the same typed context;
   directory scans are not an attribution authority. ARM neither shells to the
   CLI nor calls a localhost HTTP adapter.

## 4. Tests and parity

The 81 targeted library tests observed after W04.B cover kernel
lease overlap/release, descriptor and symlink refusals, source replacement,
captured relative bases, claim-before-run ordering, fencing, orphan settlement,
tamper/reorder/truncation refusal, archive commitment, crash-window migration,
replay projections, queue re-decision, signals, DST-facing decisions, and
stable labels. `nika-cli --lib` keeps interface rendering and migration guards.

Real-binary parity remains authoritative:

- `cargo test -p nika-cli --test arm_fire -- --test-threads=1` exercises due,
  missed, catch-up, unknown/refused policies, exact one-line output, paused
  runs, concurrent exact traces, relative children/skills, broken pipes, and
  terminal claim settlement.
- `cargo test -p nika-cli --test serve -- --test-threads=1` proves the resident
  loop uses the same firer, never fires cloud beats, and stops on SIGTERM.

The extraction is a `git mv` plus a thin CLI adapter; the legacy parity oracle
is the pre-extraction `nika-cli::verbs::arm::{fire,state}` behavior guarded by
those unchanged binary tests.

Property testing belongs to the pure state/ledger machines in
`nika-cadence`; this effect adapter has no independent algebra to duplicate.
Benchmarks are not applicable: filesystem durability and process execution
dominate, and no throughput claim is made. The real CLI integration tests are
the canary; a `.nika` canary cannot safely manufacture kernel contention,
symlink swaps, or receipt crash boundaries.

## 5. Admission gates

| Gate | Evidence |
|---|---|
| 1 SPEC | this document |
| 2 TDD | 89-test suite plus existing CLI/Serve binary regressions |
| 3 IMPL | `cargo check -p nika-arm` and `cargo test -p nika-arm --lib` |
| 4 CLIPPY | `cargo clippy -p nika-arm --all-targets -- -D warnings` |
| 5 MUTATION ≥90% | `271 mutants`: 228 caught, 3 missed, 40 unviable · 228/231 viable caught (98%) · no exemption marker |
| 6 PROPERTY | pure properties remain in `nika-cadence`; effect boundary covered by adversarial fixtures |
| 7 BENCHMARKS | not applicable; no performance contract |
| 8 DOCS | `RUSTDOCFLAGS='-D warnings' cargo doc -p nika-arm --no-deps` |
| 9 CANARY E2E | real `arm_fire` and Serve binary suites are the stronger canary |
| 10 PARITY | unchanged real-binary matrix against the pre-extraction CLI owner |
| 11 REVIEW | three independent admission reviewers; every P0/P1 fixed before commit |
| 12 ATOMIC | one signed admission commit with the Nika co-author trailer |

## 6. Non-goals

No registry parsing or schedule calculation · no workflow composition or
runtime execution implementation · no HTTP or authentication · no job API · no
general cancellation · no artifact authority · no resume · no exactly-once
claim. Those contracts belong respectively to `nika-cadence`, the shared L3
execution service, and the separately threat-modeled Serve boundary.

## 7. W04 migration closure

W04.B closes transitive ARM custody: the registry, primary workflow, child
workflows, and skill files are captured through one held project capability and
the ARM adapter executes only the admitted `ExecutionContext`. Mutation after
the durable claim, including a child or skill pathname swap, cannot change the
bytes executed. The claim-to-receipt execution identity is replay-verifiable.

W04.C removes the broader compatibility composition path: the production child
runner has one snapshot constructor, no pathname reader, and no optional world.
CLI stdin enters `ExecutionService` through owned root bytes, so file, stdin, and
ARM execution share the same admitted closure. Structural ratchets keep ARM free
of CLI dependency, subprocess/localhost bridging, and latest-trace discovery.
Resident Serve's once/dry/reload/signal behavior remains owned by its existing
adapter and is not changed by this extinction pass.

## 8. Schedule readiness receipt (C5 · 2026-09-28)

`readiness::ScheduleReadinessReceipt::assemble(registry, index, now, identity,
program, evidence)` judges one beat read-only, for R4 71 · 89 · 112. Its three
inputs have three owners:

- **Schedule** — judged here with the firer's own pure law: `Cadence::parse`
  (zone from the expression, embedded tzdb, `next_fire`), `v0_unsupported`
  (a policy every fire refuses is `policy_unsupported`, E16-2),
  `tick::expiry_passed` (`schedule_expired`), the locus and the webhook form
  (both unknown: another executor fires them), and `tick_decision` over the
  same `last` the firer reads (`run_now`).
- **Program** — `ProgramFacts`, judged by its owner
  (`nika_service_execution::run_cost::scheduled_program` over the admitted
  world) and passed in unchanged; a `None` axis is unknown.
- **Evidence** — one `ArmState::inspect` read, bound to the beat by what the
  record holds (`ProofBinding`): its slot identity against
  `SlotId::derive(workflow, cadence, instant)`, its instant against the
  cadence, its generation against the current `ArmGeneration`. Current
  generation and slot → `current_generation`; this beat's slot at another
  generation → `historical_generation` (E16-6); no generation → `legacy`; a
  slot another workflow derived or an instant the cadence never produces →
  `unattributed` (E16-1); a refused replay → `refused`. A beat's own record
  from before a cadence change is also `unattributed` (its slot identity
  hashed the old cadence, and records carry no declaration to tell it from a
  foreign one): the report exits 3 until the next fire under the new cadence
  lands a derivable record. Records carry no
  project or label, so the proof scope is `generation_and_slot` and
  `project_authenticity` is `not_proven`: a byte-identical copy from an
  identical project still reads `current_generation`, never
  project-verified. No journal format changed.

**Status law.** `DORMANT` when `actif: false`. `READY` only when
`program_ready`, `trigger_binding_ready`, `required_inputs_ready` and
`model_cost_ready` are all proven true with no blocker and no unknown;
otherwise `UNREADY` (false OR unknown). `STATUS_SCOPE` states the scope in
every receipt: the current configuration, never OS activation, acquired
authority or the current window. `run_ready_now` is the tick law's `Fire`
AND `READY`, so an out-of-window skip leaves a READY schedule READY.
`arm_ready` is `false` on any blocker or dormancy and otherwise `null`, never
`true`. `trigger_requirement_ready` is `null`: the compile-side
`requested_trigger` bridge (A3) is open, and an arm entry is never read as a
semantic trigger requirement. `activation.status` is `not_verified`:
`nika arm --emit` prints a unit and `--write` writes its file, neither loads
it, and a resident serve is not observed.

**Identity.** `ReceiptIdentity` carries the existing digests (root
`workflow_sha256`, world `snapshot_digest`, `generation` over the canonical
beat and the world) and, apart, `ProjectBinding` (`root_fingerprint` from
`nika_runtime::project_root_fingerprint`, project file, label). No new digest:
a workflow, child, input binding, input source, cadence or policy change moves
the generation, so an older receipt's identity no longer matches.

**Rendering.** `to_json` keeps the version 1 keys with their meaning
(`program_ready`, `required_inputs_ready`, `arm_ready`, `authority`,
`binding_status`, the digests, `required_inputs`, `firing_evidence.status`)
and adds the R4 89 fields (`status`, `project_identity`, `timezone`,
`next_fire`, `unbound_inputs`, `model_summary`, `authority_summary`,
`host_assumptions`, `blockers[].kind`). A program key the owner could not
judge is `null`, never an empty list. `human_lines` renders the same status
word, blockers and unknowns; the proof line says `✓ PROUVÉ (génération +
créneau · ni projet ni hôte)` only for the current generation and names
history, legacy and foreign records as such. The plural helper mirrors the
CLI's `count` for its two nouns (`saut`, `tir`).

**Codes.** Blockers keep the registered `NIKA-1708` (missing required input)
and `NIKA-1709` (budget floor) plus stable kind slugs. R4 89's
`NIKA-SCHEDULE-INPUT-UNBOUND` is illustrative: it maps to
`kind: input_unbound` with `code: NIKA-1708`; no registry code was invented.

**Tests.** 12 `readiness::tests` over real ledgers (claim plus fenced receipt,
legacy receipt, torn append): READY needs every axis proven, an unknown model
cost stays UNREADY, dormancy is never computed, `manqué: rattraper` is a
schedule blocker, the current, historical, legacy, unattributed and refused
bindings, bytes unchanged by inspection, the project binding kept apart from
the generation, one verdict in JSON and human, and a cadence change leaving
the earlier record unattributed. Disabling the slot-identity or the
cadence-membership check fails the foreign-evidence test.
