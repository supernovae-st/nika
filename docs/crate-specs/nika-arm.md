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
the canary; a `.nika.yaml` canary cannot safely manufacture kernel contention,
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
