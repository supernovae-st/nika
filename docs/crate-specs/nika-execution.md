# Crate spec — `nika-execution`

| | |
|---|---|
| Status | **WORKSPACE WIP · ADMISSION CANDIDATE** — the C0/C1 owned-byte carrier is callable and reviewed, but remains in the canonical `wip` set until the full admission ceremony closes. CLI and ARM migration are later consumer waves. |
| Layer | L3 — execution admission and orchestration boundary |
| Design | Descriptor-rooted transitive capture into one immutable snapshot; parser, checker, skill resolver, and injected runner all consume that same owned world. |
| LOC budget | ≤5,000 source lines; ≤15,000 hard crate cap. |
| File cap | ≤1,500 lines. |
| Function cap | ≤100 lines. |
| Crate version | tracks workspace |
| License | `AGPL-3.0-or-later` |
| Publish | `false` — engine-internal service |
| Dependencies | `nika-types` · `nika-fs` · `nika-schema` · `nika-check` · `sha2`; dev: `tempfile` · `proptest`. |
| NIKA codes | none allocated — capture/admission defects are an internal typed boundary; interface adapters project checker findings or transport errors. |

## 1. Purpose and boundary

`nika-execution` owns the executable byte world once for every interface. An
`OwnedDir` project capability opens the root, transitive child workflows, Agent
Skills, and explicitly declared project imports descriptor-relatively with
`O_NOFOLLOW`. Capture finishes before static judgment. The resulting
`ExecutionSnapshot` owns every byte, logical path, unit digest, aggregate
digest, and format version.

`ExecutionService::admit` parses and checks only snapshot text. Skill resolution
uses the same in-memory map. `ExecutionService::execute` consumes the admitted
value and gives the injected runner an `ExecutionContext` containing no
`OwnedDir`, absolute path, or reader callback. Filesystem reopening after
admission is therefore absent by construction and guarded by a counted-reader
test.

The injected runner is a one-shot closure so an interface adapter may capture
its private request (CLI flags, transport metadata, or an ARM claim) without a
global or thread-local side channel. Those adapter fields stay outside
`ExecutionContext`: the context remains capability-free and exposes only the
admitted IDs, snapshot, workflow, check report, and resolved skills.

This crate does not own HTTP, UI rendering, durable jobs, ARM cadence, provider
selection, sandbox implementation, runtime verb semantics, or trace storage.
Those remain in their existing layers and will consume this service through
later adapters.

## 2. Public surface

- `SnapshotLimits` bounds child depth, unit count, per-unit bytes, and aggregate
  bytes before admission.
- `ExecutionSnapshot::capture` owns the workflow/child/skill closure.
  `capture_with_imports` adds opaque project-level imports without inventing an
  `import:` workflow key; the current language grammar has no such field.
- `CapturedUnit` exposes kind, contained logical path, exact bytes, UTF-8 view,
  and SHA-256 digest through read-only accessors.
- `ExecutionService::admit` mints one `ExecutionId`, derives its root `TraceId`
  directly from the same 128 bits, and returns `AdmittedExecution` only after
  parser, composed checker, and skill resolution are clean.
- `ExecutionService::execute` consumes that admission and returns a generic
  `ExecutionVerdict<T>` carrying execution ID, trace ID, snapshot digest, and
  the injected runner's typed outcome.

All public structs and enums are non-exhaustive. Response fields remain private
behind constructors or accessors.

## 3. Security and determinism laws

1. Every filesystem read flows through one held `OwnedDir`; absolute paths,
   root escapes, non-UTF-8 workflow/skill paths, and symlinks fail closed.
2. Capture is eager and bounded. No lazy child, skill, or import reader survives
   into check or run.
3. A workflow may only cause a skill read already admitted by its own
   `permits.fs.read` boundary. Refused paths are never opened.
4. Child cycles, alias duplicates (`x` versus `./x`), and one path carrying two
   unit roles are refused before admission.
5. Snapshot identity hashes a domain tag, format version, root identity, and
   every BTree-ordered unit role, logical path, length, and exact bytes. Wall
   time and host pathname never participate.
6. `ExecutionId`, `RunId`, and `TraceId` remain distinct types. The direct
   `ExecutionId -> TraceId` relation requires no timestamp or directory scan.

Pinned registry workflow references are refused in this carrier because the
descriptor-rooted source cannot yet provide an atomic registry view. Accepting
one while omitting its bytes would make the snapshot's completeness claim
false.

## 4. Falsification suite

Inline `--lib` tests cover:

- child, skill, and import mutation after capture;
- barrier-interleaved root replacement and skill-registry reload after their
  reads but before admission;
- child pathname replacement with an out-of-root symlink after capture;
- workflow cycles and normalized-path duplicates;
- depth, unit-count, per-unit-size, and aggregate-size ceilings;
- stable snapshot digests across separate project roots and map insertion
  orders;
- a counted source proving admission/check/run perform zero reads after the
  capture boundary;
- deletion of the admitted root before execution, proving the runner still
  observes the exact admitted bytes;
- property tests for digest order independence and dot-segment normalization.

The tests use no sleeps and inspect the bytes presented to the runner, not only
an announced digest.

## 4bis. Executable-input reader census (W02.D)

This is the source-level census at `2026-08-22` for bytes that can define an
execution. State/evidence readers in `nika-arm` are excluded: they do not parse
workflow, child, skill, or import bytes. `follow` says whether the current read
can follow a replaced link; `owned` says whether the exact bytes survive into
all later phases.

| reader | root | follow | owned | phase | consumer | disposition |
|---|---|---:|---:|---|---|---|
| `nika-execution::ByteSource for OwnedDir` | held project descriptor | no | yes | capture | `ExecutionService` check + runner | canonical; W03 carrier |
| `nika-cli::verbs::RunSource::capture` | caller pathname/stdin | yes for file | primary only | CLI pre-check | current CLI runtime | migrate to `ExecutionService` in W04; its owned bytes already feed the signature gate |
| `nika-cli::verbs::load_checked_run_source` composed reader | caller pathname | yes | no | static check | CLI checker | migrate to snapshot reader in W04 |
| `nika-cli::verbs::resolve_workflow_skills` | workflow parent pathname | yes | no | pre-run | CLI agent skills | P0 legacy reader; remove in W04 |
| `nika-cli::verbs::run::child_runner::load_child` | parent pathname | yes | no | dispatch | child runtime | P0 legacy reader; remove in W04 |
| `nika-cli::verbs::run::child_runner::closure_digest` | canonicalized pathname | yes | digest only | resume-key build | CLI resume | replace with snapshot unit digests in W04 |
| `nika-arm::RunShot` primary source | held project descriptor | no | yes | claim/firing | ARM adapter | primary safe; W04 must route its children/skills through the service |
| `nika-cli::run::resume_setup` trace reader | held trace-parent descriptor | no | yes | pre-compose evidence fold | CLI resume | closed in W02; not executable source |
| `nika-dap::sign::check_workflow_bytes` | sidecar beside logical workflow path | yes for sidecar only | workflow yes, sidecar used once | pre-effect trust gate | `run --require-signature` | closed in W02: verifies `RunSource` bytes and never reopens the workflow pathname |
| `nika-dap::sign::check_workflow` | workflow + adjacent sidecar pathnames | yes | no | standalone `sign --check` | signing operator | allowed non-execution reader; cannot feed `nika run` |
| `nika-dap::sign::sign_workflow_with` | workflow + adjacent sidecar pathnames | yes | signs that read only | standalone `nika sign` | signing operator | allowed minting reader; cannot feed `nika run` |
| `nika-runtime::EnvFileSecretResolver` | authored secret pathname | yes | value only | composition | provider/tool secret namespace | separate secret-store boundary; remote disclosure is governed by W02.E/W06 |

The structural gate is three-layered rather than a textual search: (1)
`ExecutionSnapshot::capture_from` is the only `ByteSource` consumer, (2)
`ExecutionService::admit_snapshot` is crate-private and `execute` accepts a bare
function over `ExecutionContext` with no root or reader capability, and (3)
`digest_check_skills_and_run_read_zero_sources_after_capture` injects a reader
spy and proves the read count cannot change during parse, composed check, skill
resolution, digest, or run. The committed `public-api.txt` additionally locks
the absence of a reader/root accessor from `ExecutionContext`. The DAP
`signature_gate_verifies_captured_b_not_reread_pathname_a` barrier proves a
pathname replaced with signed bytes A cannot authorize already captured bytes
B. Any W04 adapter that keeps one of the executable pathname readers above has
not completed migration.

## 5. Admission gates

The pre-admission readout on 2026-08-22 reported gates 1, 2, 3, 4, 6, and 8
passing. The full workspace run observed 6,294 library tests passing with zero
failures and clippy with zero warnings. Gates 5, 11, and 12 remain ceremony
work; gates 7, 9, and 10 carry the scoped exemptions below but are not promoted
to an admission claim by the heuristic reporter. Until those receipts close,
the workspace metadata keeps this crate WIP.

| Gate | Evidence |
|---|---|
| 1 SPEC | this document |
| 2 TDD | `nika-types`, `nika-event`, and `nika-execution` tests were observed RED before implementation, then GREEN |
| 3 IMPL | `cargo test -p nika-execution --lib` plus touched-crate library suites |
| 4 CLIPPY | targeted `cargo clippy -p … --all-targets -- -D warnings` for every touched crate |
| 5 MUTATION ≥90% | admission run recorded in the PR evidence |
| 6 PROPERTY | `snapshot::tests` exercises digest ordering and path normalization with `proptest` |
| 7 BENCHMARKS | exempt: admission is bounded, one-shot filesystem I/O; no throughput contract or hot loop |
| 8 DOCS | `RUSTDOCFLAGS='-D warnings' cargo doc -p nika-execution --no-deps` |
| 9 CANARY E2E | exempt in C0/C1: no interface consumer is changed in this carrier; migration owns its real-binary canary |
| 10 PARITY | exempt: no predecessor service exists; existing CLI behavior remains untouched |
| 11 REVIEW | independent architecture, security, and adversarial review findings resolved before commit |
| 12 ATOMIC | one lowercase commit with the Nika co-author trailer |

## 6. Non-goals and next consumers

No CLI/ARM migration · no Serve route · no durable job/event store · no
cancellation state machine · no registry fetch · no artifact custody · no
runtime provider/sandbox construction. Those surfaces can only consume the
admitted snapshot; they may not recreate a parallel composition reader.
