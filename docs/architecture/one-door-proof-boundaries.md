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
| Which custody bytes may be public? | Decoded public-key projection in `nika-dap::seal` | Show or retire only reconstructed public-key material, not arbitrary text from a public-named slot |

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
- A queued replay is neither execution ownership nor resume consent. Claim
  and admission refusal both require `Queued` under the store lease; an old
  queue entry cannot reopen a paused leg. Only a successful claim arms the
  interruption guard and owns cancellation registration. A read-side check
  avoids reopening known stale worlds but is not the transition authority.
  This lifecycle observation uses the reserved store control lane, so a full
  HTTP ingress queue cannot turn that optimization into a fatal execution.
  The explicit paused-leg store transition remains available and tested.
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
- Keep readiness and authority separate in agent teaching. `paid_ready` can
  remain true on a report with a permits error: it is not a replacement for
  `clean`, native-strict findings, resolved-child coverage or execution consent.
  A mock envelope does not replace per-task model pins or disable real tools.
- Retire teaching workarounds when their source defect is fixed. The composed
  cost tests in `nika-check` and global-scan tests in `nika-builtin` own those
  behaviors; authoring instructions must not describe the historical defects
  as current. A rehearsal compares concrete values and negative outcomes,
  not only a successful exit or well-formed JSON.
- Treat a public-key storage slot as untrusted input. A minisign box wrapper
  is not validation: decode the key and reconstruct its public representation
  before trust output or retirement. Discard untrusted comments and trailing
  payloads; normal engine-generated boxes retain their bytes and fingerprint.
  Refuse unknown public-key algorithms. Before signing, compare the public
  projection to the public key derived from the opened secret, including the
  minisign key number; library key equality alone omits that number.
- Distinguish invalid custody from absent custody. Partial explicit file
  configuration refuses; a broken explicit pair never selects another signing
  identity. Non-forced initialization must preserve existing file entries,
  including corrupt or orphaned material, and exclusive creation must catch a
  concurrent file writer after the initial presence check.
- Reject known aliases between explicit private and public file slots before
  initialization, including resolved parent aliases and existing Unix hard
  links. Distinct path spellings do not establish distinct custody slots.
- Parse verification ledgers through one public-box decoder, keeping their
  two-line records whole. Workflow signatures, trace seals and evidence packs
  use the same fingerprint function; a retired key remains a candidate after
  rotation. Historical comments retain their recorded fingerprint and are
  not copied into new live trust output.
- Route native keyring construction through one guarded function, including
  verifier and public-only readers. A source inventory must recurse into
  submodules and must not stop at the first test-only item: production code can
  follow that item.

The custody corrections do not make a two-file key pair or the OS keychain a
transaction. A failed second file write can leave a new private half; its
presence blocks non-forced retries. File creation refuses a concurrent winner,
but the keychain has no compare-and-set guarantee here. Path-alias checks do
not lock directories against later replacement. Verification surfaces still
select their existing enrolment sources; this does not unify that authority
policy. Historical verification
still reads its recorded key boxes; these changes do not rewrite old evidence.
Reconstructed live boxes no longer incorporate custom comment bytes into new
fingerprints. An imported key whose older seals bind a custom comment needs
its original public enrollment record to verify those seals: canonical
retirement cannot preserve that old fingerprint by copying arbitrary metadata.
Engine-generated public boxes are unchanged. This is a public-data boundary,
not proof of complete secret custody or global effect exactly-once execution.
Key-pair correspondence is checked at signing load; public-only trust reads
do not decrypt the secret and therefore do not prove that correspondence.

The focused Rust regressions are in `nika-execution`,
`nika-service-execution`, `nika-serve`, `nika-dap` and the CLI answered-leg tests.
The client SDK owns transport-shape and installed-package differential
tests. CI owns platform-specific executable tests. Each verification report
must name which of those actually ran; no single green substitutes for the
others.
