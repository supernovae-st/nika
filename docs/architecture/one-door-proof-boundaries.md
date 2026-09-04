# One Door: authority and proof boundaries

This review accompanies the closing corrections to
[ADR-122](../adr/adr-122-execution-access-plan-authority.md) and
[ADR-128](../adr/adr-128-one-run-settlement.md). It records engineering
obligations, not a claim that Nika is formally verified or architecturally
perfect. A release, a test suite and a research citation are different
kinds of evidence.

## One owner, several adapters

| Question | Authority | What an adapter may do |
|---|---|---|
| Which bytes are admitted? | `nika-execution::ExecutionService` | Submit a served name or engine-captured snapshot; never invent a digest algorithm |
| Which model constraints apply? | Provider and checker judges at admission | Render findings; never decide a separate runtime policy |
| Which access path serves a workflow? | Frozen `ExecutionAccessPlan` | Project lanes and billing; never select a different path at dispatch |
| What happened during this leg? | Runtime `RunSettlement` | Preserve the settlement, outputs and error; never refold task events into a competing result |
| Who may change a job's state? | Resident owner and leased `JobStore` transitions | Request cancellation; never manufacture its requested outcome |
| What can be replayed? | Hash-bound stored event and receipt | Resume observation by sequence; never re-execute a completed admission to recover its answer |
| What does a trace prove? | Trace verifier and writer-liveness checks | Report the proof tier; never infer a sealed trace from a receipt alone |
| What does the budget bound? | Admission floor and runtime spend ledger | Distinguish output estimates, recorded spend, in-flight exposure and unpriced work; never promise an invoice ceiling |

Zero legacy means removing a replaced decision path and migrating its owned
callers. It does not mean deleting unique operator capabilities because
they have different command names. Reading historical evidence also does
not authorize an old execution policy: missing fields stay absent and an
unknown state is not promoted to success.

## Research that sharpens the design

**Refinement rather than similar-looking outputs.** IronFleet combines
state-machine refinement with implementation verification, including safety
and liveness. For Nika, the useful design inference is to make each adapter
a projection of the same transition and result. Comparing status strings
alone is too weak: identities, outputs, failure attribution and spend are
also observable. Nika's differential tests exercise that obligation; they
are not IronFleet-style proofs. [IronFleet, SOSP 2015](https://www.microsoft.com/en-us/research/publication/ironfleet-proving-practical-distributed-systems-correct/).

**State the fault model.** Verdi makes network faults explicit and verifies
systems under selected semantics. Our corresponding review obligation is
to distinguish a cooperative cancel, a result racing that request, a worker
that never returns, a server restart and a disconnected observer. One
successful cancellation test cannot establish all five. Nika does not add
a consensus layer merely because the public door is HTTP.
[Verdi, PLDI 2015](https://homes.cs.washington.edu/~mernst/pubs/verify-distsystem-pldi2015-abstract.html).

**Persist the answer with the operation's authority.** RIFL couples durable
completion records and results to the operation's state so retries recover
the original answer without repeating the operation. The narrow application
here is durable job settlement and idempotent admission replay before a
fresh registry capture. This does **not** prove exactly-once external LLM,
HTTP or shell effects: those effects are not atomically committed with the
job store. Retention, store capacity and ownership loss remain explicit
parts of the contract. [RIFL, SOSP 2015](https://web.stanford.edu/~ouster/cgi-bin/papers/rifl.pdf).

**Memory safety is not protocol correctness.** RustBelt proves safety for a
realistic Rust subset and states obligations for unsafe library extensions.
The engineering inference is to keep ownership and capability boundaries
in types while separately validating semantic invariants at deserialization
and transition boundaries. Well-typed Rust can still accept a fake pause or
overwrite a successful result with a cancellation request.
[RustBelt, POPL 2018](https://plv.mpi-sws.org/rustbelt/popl18/).

## Concrete review obligations

- Reject known model/capacity failures throughout the captured workflow
  closure before the root's first effect; do not describe dynamic unknowns
  as checked facts.
- Mint fresh identities for answered execution legs over the original
  owned bytes. A fixed clock or deleted pathname must not alias journals.
- Serialize queued cancellation and execution claim under the same lease.
  Once a runtime owns the job, preserve its returned result.
- Distinguish a pause observation boundary from a final job transition.
  Generic event append must not create or replace an authoritative pause.
- Recover the same settlement through live SSE, a consumed terminal cursor,
  durable GET and idempotent replay. Preserve additive fields; reject
  malformed known fields and contradictory states.
- Compare complete settlements exactly when observing the same job again.
  Only independent executions may normalize elapsed time and diagnostic
  wording; applying that normalization to replay hides lost stored facts.
- Exercise nonempty outputs and a named failing task in cross-door tests.
  Empty outputs or a status-only comparison leave material divergence
  invisible.
- Bound proof subprocesses and clean them up after failure. A hanging test
  harness cannot be evidence of runtime liveness.
- Repeat installed-package tests on the exact published engine and SDK.
  Source and debug-binary evidence do not prove a registry artifact.
- Keep agent teaching aligned with the spend ledger: crossing the measured
  budget stops new admissions, not already-started calls. Unpriced work and
  input costs omitted by an output-only estimate are not proved free or capped.

The focused Rust regressions are in `nika-execution`,
`nika-service-execution`, `nika-serve` and the CLI answered-leg tests.
The client SDK owns transport-shape and installed-package differential
tests. CI owns platform-specific executable tests. Each verification report
must name which of those actually ran; no single green substitutes for the
others.
