# Crate spec — `nika-dap`

| | |
|---|---|
| Status | **ADMITTED 2026-07-09** — Gate 1 authored at the split (a descent, not a greenfield: every line arrived tested from `nika-cli`). |
| Layer | L4 — interface crate (stdio protocol server + the forensic read seams) |
| Design | The trace-forensics plane: the DAP replay debugger (`nika dap`) plus the seams every forensic reader shares — the tolerant NDJSON reader (`recover`), the tamper-evidence chain walk (`chain`), the forensic statistics (`stats` — the Prior honesty ladder + Hyndman-Fan-7 quantiles every learned-truth reader speaks · descended from nika-cli at the 15060-LOC wall, the same session as the crate itself), and since the W0 descent (§5) the forensic half of the trace family — the OTLP projection (`otel`), the reproduce comparison (`reproduce`), the store scan (`store`) and the retention policy (`retention`). One home so the sink that WRITES the chain and every walker that CHECKS it share one genesis tag and one hash primitive (the hash itself re-homed to `nika-event::source_id` 2026-07-22 — the run-composer descent needed it at L≤3). |
| LOC budget | the 15k-prod workspace ratchet governs (≤1,500/file · ≤100/fn as everywhere) — admitted at ~1,450 src incl. in-file tests; the 2026-07-09 W0 trace descent (§5) added the four forensic trace modules (~1.1k prod) with headroom for live DAP sessions intact |
| File cap | ≤1,500 LOC each (max at admission: `replay.rs` ~490) |
| Function cap | ≤100 lines each |
| Crate version | tracks workspace (`0.98.0` at admission) |
| License | `AGPL-3.0-or-later` |
| Edition | 2024 |
| Publish | `false` — internal L4 interface crate, same stance as `nika-cli` |
| Extraction source | `crates/nika-cli/src/verbs/dap/{mod,protocol,replay}.rs` (1,202 LOC · git-mv, history preserved) + `run/source_id.rs` (moved) + `run/resume.rs::recover_events` (moved) + `verbs/trace_verify.rs::{walk, Verdict}` (moved) — `nika-cli` re-exports every seam at its old path (zero call-site churn). **W0 trace descent (2026-07-09 · architecture review v2 §1)**: `verbs/trace_otel.rs` → `otel` · `verbs/trace_reproduce.rs` → `reproduce` · `verbs/trace/{store,retention}.rs` → `store` + `retention` — the compute descends, the render stays (the CLI keeps `export`/`reproduce` file plumbing, the report/line renderers, `fmt_age`/`fmt_bytes` display vocabulary and the `nika run` GC hook as shims). Per D-2026-07-09-N1 the descent is ONE architectural unit in TWO members — this crate spec names the parentage; the unit stays `nika-cli`'s. |
| NIKA codes | **none** — the DAP wire speaks the protocol's own error responses; the forensic seams return typed verdicts/Results (the trace surface stays non-coded, the same stance the trace verbs hold) |

---

## 1. Purpose

`nika-dap` is the **trace-forensics plane**:

1. **`run_stdio()`** — the Debug Adapter Protocol server behind `nika dap`:
   a READ-ONLY replay debugger over a recorded run journal. Breakpoints
   map to task lines, stepping walks task settles, `stepBack` is free
   because the log is total — replay = re-render, NEVER re-execute.
2. **`recover`** — the ONE tolerant NDJSON reader (`--resume` · `trace
   show` · the store scan · the forecast gather · the replayer all fold
   through it): a torn tail keeps its valid prefix, a dead first line
   refuses.
3. **`chain`** — the tamper-evidence walk (`walk(raw) -> Verdict`) and
   the ONE `CHAIN_GENESIS` constant the sink imports to write the same
   chain the walk verifies.
4. **`source_id`** — `sha256_hex` + `lf_normal_form` (a CRLF re-encode
   is not an edit — the 0.96.0 dap-review lesson lives here).
   **Re-homed to `nika-event::source_id` 2026-07-22** (the run-composer
   descent: `nika-runtime`'s child runner hashes the child source at
   L≤3, and dap is L4 — the taxonomy owner keeps the one primitive;
   every reader below rewired, zero behavior change).

## 2. Why a crate (and why now)

`nika-cli` sat at 14,828/15,000 prod LOC (98.9%) before the forecast
feature landed (+947): the crate-size ratchet blocked every push. The
2026-07-09 gates audit (§3) weighed three options — compacting was
insufficient (−200 for a needed −775+), bumping the cap is forbidden
(it has paid four times), and the dap module was the cleanest cut:
an external protocol surface whose only inbound edge was ONE match arm
in `main.rs`. The compiler then surfaced the real inverse coupling —
five forensic symbols — and the split became the chance to give them
one home (three private `sha256_hex` copies and two `CHAIN_GENESIS`
tags unified). Precedent: `nika-cap` absorbing `builtin_shape` at the
same wall (2026-07-07).

## 3. Public API (the whole surface)

```text
pub fn run_stdio() -> u8
pub mod recover    { RecoveredTrace · RecoverError · recover_events }
pub mod chain      { CHAIN_GENESIS · Verdict · walk }
pub mod stats      { Prior (#[non_exhaustive]) · BANDS_MIN_N · quantile_h7 · ConformalUpper · conformal_upper }
pub mod otel       { project (journal + chain Verdict → one OTLP/JSON line) }
pub mod reproduce  { Verdict · Row · Report · compare · workflow_of }
pub mod store      { TRACE_DIR · TraceState · TraceMeta · scan · fold_facts · locate_trace }
pub mod retention  { RetentionConfig · Reason · GcReport · plan · newest_per_workflow · collect }
pub mod journal    { TraceFileSink (· settle_sealed · interrupt) · JsonSink · Tee · seal_journal }   // the WRITE half (descended 2026-07-22)
pub mod resume     { ResumeRequest · PlanFold · fold_plan · apply_from · parse_answers · summary_line }   // ADR-099 (descended 2026-07-22)
pub mod cost_journal { JOURNAL · Writer · Lease · Taken · take · Exposure · Blocker · Exposures · fold · refusal · append_row · clear · Cleared · Blocked · JournalWitness · RunAccount · RunJournal }   // billing evidence (descended 2026-09-28 · Run custody C6)
```

Consumers: `nika-cli` (the bin's `Command::Dap` arm + the re-exported
seams). The DAP protocol/replay internals stay private.

## 5. The W0 trace descent (2026-07-09)

`nika-cli` hit the 15k wall a second time the same day (99.8% ·
14,966/15,000 — two open PRs blocked at the push gate). The
architecture review v2 §1 designed the descent: **the forensic half of
the trace family comes home to the forensics plane** — `trace_otel`'s
projection (embedder-useful without the CLI: OTLP export of any
recorded journal), `trace_reproduce`'s comparison taxonomy (the
replayer competency), and the `store` scan + `retention` math. The
render half STAYS cli-side (`trace/mod.rs` readers · `trace manage` ·
the report/line renderers · the `Theme`/display vocabulary) — compute
descends, render stays, the `trace_verify` shim pattern throughout.
Deps stay L0-only (nika-event · nika-types · sha2 · serde/serde_json ·
thiserror — the absorption is L4-legal). Every moved type follows
FCI-002/FCI-016 (`#[non_exhaustive]` + `new()` per invariant #19);
the two cli-side exhaustive `TraceState` matches gained honest
wildcard arms.

## 6. The cost journal descent (2026-09-28)

`nika-cli-host` crossed the 15k wall with the paid Run's durable settlement
(P3): the writer lease beside `.nika/inference-cost-observations.ndjson`, the
strict fold that records a killed Run's UNKNOWN once, and the torn-tail append.
They come home beside `liveness` (the same ADR-129 lease law, applied to billing
evidence) as `cost_journal`: descriptor-rooted through `nika-fs` `OwnedDir`
(the one new dependency edge), `std::io::Result` at the boundary (invalid
journal data is `InvalidData`, never a bare `String`), every public type
`#[non_exhaustive]` with `new()` where it is constructed. The host keeps the
question, the live account and its own rows (`prepared` · `settled` ·
settle-on-drop); the fold, the lease and the refusal wording moved unchanged,
their tests with them.

C3 hardening (2026-09-28, the E4 P3 review). The journal is read as bytes: a
line that is not UTF-8 or not JSON is torn, named by the sha256 of its exact
bytes. Rows no longer win by being latest: each Run moves only by legal
transitions, `prepared`, then `settled` by the same lease writer or the
`unknown` derived from that exact row (writer and `prior_sha256` match), and
nothing after either but a byte-identical repeat. A lease-less `prepared` or
`settled` row is legacy evidence only before the first leased row. Any other
row (orphan, foreign or late settlement, second preparation, mismatched
unknown) is a `Conflict`: it never changes its Run's standing, it blocks by
itself, and the refusal names it by digest (journal text is escaped). A
reconciled resolution needs its own append-only evidence, principal and
digest contract (P4); none is accepted here. A derived `unknown` row carries
the prepared-time observation as `prior_observation`, so its `Open` state and
zero counters never read as the unknown Run's current state (rows an earlier
engine derived with `observation` still read, as prior). The lease record
also carries the running kernel's boot identity where the platform proves
one (Linux `/proc/sys/kernel/random/boot_id`: a bounded read of exactly one
lowercase UUID, otherwise none). A lease holder derives a killed Run only for
a writer on its own nonempty hostname (the historical heuristic, never proof
of one machine) or on its boot identity (the same kernel's lock table, e.g.
a container restarted under a new hostname); absent or empty identities
never match. `fold_as` takes the holder's `Writer` with that identity;
`fold` keeps its source-compatible signature and judges by hostname only. On macOS judgment stays hostname-only: `kern.bootsessionuuid`
needs a safe `sysctl` owner (a dependency or kernel wrapper), which is a
recorded follow-up, not implied here. The refusal names each blocking Run
by its journal identity and, when a trace recorded it, the trace file (a
store name of that trace id whose first frame names the same execution,
never a same-suffix decoy), escapes journal text, and says only that a
writer "no longer holds the cost lease": the lease proves no more, and the
pid may be this very process's.

Terminal consistency (2026-09-28, root's independent review of C3). A
`prepared` or `settled` row the transitions admit must also be an observation
its account could have written, per nika-providers
`InferenceReceipt::observation`, the same serializer since the journal's
first writer:
- `unknown_calls` equals the number of sent attempts, in either attempt list,
  that carry no estimate;
- every estimate is a nonnegative decimal on a sent attempt, and the known
  subtotal is their sum;
- `prepared` is the untouched account, Open with no attempt, because the host
  writes it right after the review confirms the choice;
- `settled` is never Open, because the host closes the account before it
  settles.

Any other admitted row is a `Conflict` named by digest and reason. The
transition law's own reasons come first, so a foreign or late row keeps its
reason. A conflict changes no standing: a contradicted settlement leaves its
Run prepared, and a review that can judge that writer derives the Run's
unknown once, append-only. A consistent Closed settlement still clears,
including a completed unknown-cost call whose USD price stays unknown
(`unknown_calls` 1 with that sent attempt, as in the TUI's own Run), and
Uncertain still blocks. Malformed rows still fail closed as unreadable. This
checks what a row says, never who wrote it: a consistent row forged by a
copied writer remains the open P4 authentication work.

Reconciliation door (P4, 2026-09-28). The first supported recovery for an
unknown exposure lives here, in the same journal and the same fold:
`cost_journal::reconcile`. An operator runs `inspect`, which takes the cost
lease like a review, may record a Run's unknown once as every review does,
and reports that in `derived`. Its output is the journal as data: per blocking
or reconciled Run, its state, the digest of its latest row, whether this door
can resolve it and why not, the writer, the trace, the facts its own rows
record, and its history. `submit` then appends one `phase: "reconciled"` row
inside `nika/run-cost-observation@1`. That row carries `reconciliation`
(`nika/cost-reconciliation@1`) with these fields:
- `prior_sha256`, the exact bytes of the Run's latest row;
- `resolution`, one of `billed`, `not_billed` or `still_unknown`;
- `evidence`, the one supported class, `operator_attestation`, with
  `verified: false` and the operator's reference;
- `route`, `provider_request_ids` and `window`, copied from the Run's rows and
  never typed. The window is the `UUIDv7` time of the invocation and of the
  deriving review, labeled `uuidv7-execution-ids`, never a measured request
  time;
- `principal`, the local OS account the host read;
- `project`, a host-local binding: the sha256 of the held `.nika`
  descriptor's device and inode under `nika/cost-project@1`, not an
  authenticated or globally unique identity;
- `observed_at`.

The row also carries the reconciler's `lease`.

Only an Uncertain settlement, a recorded unknown, or an earlier
`still_unknown` can be reconciled, and only through its latest row.
`still_unknown` keeps blocking and becomes that latest row. `billed` and
`not_billed` end the exposure and authorize nothing: the next unknown-cost Run
still meets its own fresh review. Everything else is a named conflict:
- a stale or foreign prior;
- a clean or prepared Run;
- unsupported or "verified" evidence;
- a copied fact that does not match;
- no lease;
- anything after a final resolution;
- an unknown resolution.

A reconciliation that does not read is unreadable, as every reconciliation is
to an engine older than this phase, so an old engine fails closed.

`submit` judges everything before its one write: the binding, the Run's
standing and latest row, the evidence, and the event itself through the same
fold. A Run whose unknown is not on record yet is refused with "inspect first".
Every preflight refusal leaves every journal byte untouched: busy, no journal,
another project, an unknown Run, not reconcilable, stale, invalid, and a double
resolution. After attempting an append, an I/O failure means its effect is
uncertain; inspect before another action. A successful receipt requires both
byte-prefix preservation and the fold naming that exact event as the Run's
current standing, without refusing that event.

The cost lease holds independent project-directory, journal-directory,
journal-inode and legacy lock-file flocks for its lifetime. The project guard
is acquired first through a fresh parent-directory descriptor and survives
replacing `.nika` inside that same project. Journal-directory custody survives
replacing the journal or lock names; journal custody ties the writer to the opened bytes. The locked
files must still be the named, singly linked inodes after acquisition. A
hard-linked journal or lock is refused rather than sharing project custody.
Taking the first lease may create an empty journal; it does not create a cost
observation. The legacy lock remains for interoperation with older writers.
These are cooperating-writer guarantees: an old or arbitrary process holding
only an unlinked legacy lock cannot retroactively acquire the new project,
directory and journal guards. Replacing the project root itself changes the
custody object, and filesystem access can still forge unauthenticated rows.

Unjudged writers, refused rows and torn rows stay blocking and are shown as not
reconcilable. The journal is still not authenticated: a consistent
reconciliation written by hand reads like one this door wrote, and a copied
journal's rows, reconciliations included, still apply in the copy. Serve and
SDK transport of the same documents is a later slice.

## 7. The Run custody descent (C6, 2026-09-28)

A second host (Serve's cost-review door) reviews unknown-cost Runs, so the
Run's side of the journal leaves `nika-cli-host` for the child module
`cost_journal::run`, on the laws above unchanged (`take_at`, the lease's
`read`, `fold_as` and `append_row`, and `refusal` are called, never edited).
`clear(root, observer)` creates `.nika/` below the held root, takes the lease
from that root descriptor (`take_at`) and folds through the locked journal
inode: `Cleared`
holds the root and `.nika/` descriptors, the writer and the lease; `Blocked`
is `Busy { pid }` (the host's words) or `Exposed(Exposures)` (the refusal).
`Cleared::journal()` is the journal's exact length and sha256 read through
the locked journal inode (a review's prior-journal witness), and
`Cleared::same_place(path)` re-observes, by path, that the root and its
`.nika/` are the very directory objects held (device and inode): a directory
moved away or a copy put back at the same path is not the same place, even
with byte-identical journal bytes. `RunJournal` then appends one Run's rows
through the lease's locked journal inode, never by reopening a name (E21: the
old host journal followed a replacement when settling; a renamed or replaced
journal or `.nika/` never receives a row, and a journal with a second link
refuses the append); every row
reads the live `RunAccount` (the host's account), so a `settled` row, and the
settle-on-drop of a Run that ends without one, record the account's own last
observation, never a stale `prepared` snapshot. Row shape and schema are
unchanged (`nika/run-cost-observation@1`). Settlement is attempted once: the
first `settle` owns the account's closure and the `settled` row, a later call
succeeds only after that attempt succeeded and otherwise reports it pending or
unverified, and Drop settles only a Run whose settlement was never attempted. A
failed or uncertain closure or append (a short write, a failed sync) never earns
a second row; what it left is what the next review records as unknown.

`Cleared::observe_file(path, write)` observes one project file the Run binds
through the same held root, never a path reopened: a read input answers the
sha256 of its bytes (at most 1 MiB); a write target re-observes its contained
parent component by component (a symlink or a file refuses; a missing one
passes only under the write's literal `create_dirs: true`) and is never
created before the answer. It moved from the host's file witness with its
tests; which files a Run binds is the static half, Service's `bound_files`.
The boundary is `std::io::Result` throughout, the account included.

`cost_journal::Reviews<H>` (C6, descended from Serve at its 15k wall with its
words unchanged) is a cost review's custody between its framing and its one
admission, for a host that answers across requests, generic over what a live
review holds `H` (Serve's holds the framed review and so its `Cleared`
lease). It keeps the states (pending · approved · admitting · consumed ·
refused · failed · declined · expired), the 300 s monotonic lifetime
(`REVIEW_TTL`, swept on every access), the one-winner `take`, the first
verdict that stands (`settle` after an end is a no-op), the newest 256 ended
reviews (`REVIEWS_RETAINED`) and the review-local idempotency keys that leave
with them, and a constant-time witness comparison; every refusal is a typed
`ReviewRefusal` with its wire code and words. `review_witness(nonce, binding)`
(descent L) is the witness it compares: the hex SHA-256 of the host's fresh
private nonce followed by the exact binding the host composed. The host draws
the nonce and composes the binding; a witness names one review's binding and is
never authenticated journal evidence. Ending a review drops what it
held, which releases the lease. `Claims<V>` is an admitted job's held
authority until its first run claims it (a duplicate run never gets it).
Wall clock stays with the host (`decided_at`), the account view is the host's
closure. Tests hold a real `Cleared` and observe the lease on every ending.

Two trace-journal operations also came home from Serve (C6, at its 15k wall):
`store::locate_trace(dir, execution, trace)` finds a run's journal by the
sink's own naming law (the short or full trace suffix) and settles a shared
short id by the first line's `execution.uuid`, read bounded; and
`TraceFileSink::settle_sealed(seal)` (the seal first, then the durability
point, the head the receipt names, `None` for a journal never opened) and
`TraceFileSink::interrupt(execution, operator)` (the living writer's
`run_settled interrupted` END, `cause: operator` only when asked). Serve keeps
the HTTP projection and the seal's custody; behavior and bytes are unchanged.

The resident door's seal teardown fold came home as well (C6, descent F):
`seal::SealTeardown::served(workflow, report, outcome, settlement,
sdk_receipt, memory_root)` takes existing plain types only. It folds the
receipt inputs (proves, the certificate, the caller's outcome word), the
budgets from the settlement's own spend (`spent_usd` only when metered, no
budgets without a settlement), the caller's SDK receipt binding and the
signed-memory fold. The effects and quarantine folds stay out, because a
service boundary redacts their per-task records. The caller keeps the
outcome-word mapping and the binding's claims; `seal/served_tests.rs` pins
the fold against a real parsed workflow and typed settlements.
`seal::workflow_hash(workflow)` (descent M) is the hash such a run's seal is
taken under, the per-task Merkle root of the admitted workflow (the CLI's
`seal_hash`).

## 4. Gates at admission (2026-07-09)

| Gate | Name | Verdict |
|---|---|---|
| 5 | MUTATION | ✅ 98.6% killed (144/146 viable · 142 caught + 2 timeouts · 21 unviable) — survivors: the two `run_stdio` stubs (the seamless stdio composition root) |

- Tests: 34 in-crate — 25 moved WITH their code (chain walk 8 · replay 12 ·
  protocol 3 · recover 1 · source_id 1) + 9 mutation-killers added at
  admission (the documented cap boundaries · every terminal-kind fold arm ·
  defensive-min totality on a wild cursor · outgoing-seq monotony · the
  serve arms a client probes) — `cargo test -p nika-dap --lib`.
- Clippy: 0 warnings (`--all-targets -- -D warnings`, pedantic-clean).
- Mutation (Gate 5): **98.6% killed** (144/146 viable — 142 caught +
  2 timeouts · 21 unviable) at admission, 2026-07-09. The two survivors
  are the `run_stdio -> u8` stubs: the 4-line stdio composition root has
  no injectable seam by design (its body is exercised through `serve`,
  which the in-crate session tests drive over cursors). The first run
  scored 77% — the 9 killer tests above were written at admission to
  close the gap, not waved through (+ 2 line-number pins on the recover
  fold after the post-review re-run).
- Property (Gate 6): N/A as a dedicated suite — the chain walk's
  adversarial cases (torn tail · dropped line · blank renumbering ·
  single-line garbage) are the property set, exercised as units.
- Benchmarks (Gate 7): N/A — protocol server + linear walks, no hot path.
- Canary (Gate 9): N/A — the `nika dap` verb is exercised by the editor
  extension's F5 flow; the CLI e2e suite covers the seams.
- Parity (Gate 10): N/A — no brouillon ancestor (the DAP server was born
  in Diamond, 2026-07-06).
- Review (Gate 11): 3-agent swarm at admission (2026-07-09) on top of the
  moved code's shipped lineage (PR #225 · the 0.96.0 dap review · the
  rust-pro hardening batch). Verdict FIX-THEN-ADMIT, all resolved same
  session: `Verdict` + its struct variants gained `#[non_exhaustive]`
  (+ the wildcard arm in `trace verify`'s render), `RecoveredTrace`
  gained `#[non_exhaustive]` + `new()` (invariant #19), and
  `recover_events` returns a typed `RecoverError` (thiserror · FCI-019 —
  never a bare `String` in public API).

## Original-project cost custody

Project-facing hosts retain the project descriptor before opening `.nika` and
use `cost_journal::take_at(project, nika, writer)`. Acquisition fresh-opens and
locks that original project, then checks the held child against its current
`.nika` before writing child files. Moving an already-open child to a sibling
therefore cannot change the project whose live writer blocks acquisition.
The compatibility `take(nika, writer)` locks the child's current parent; it
cannot recover the caller's original project identity after a move and does
not establish that stronger claim. CLI review, inspection and reconciliation
use the explicit-project entry.

Lease acquisition may create empty administrative journal and lock files. A
fresh refusal before a question appends no preparation or consent row and
makes no provider request. A prior killed writer may still acquire an UNKNOWN
observation before that refusal. Neither property means local metadata is
byte-inert, and the journal remains unauthenticated filesystem evidence.

`Lease::read`, `Lease::fold_as` and `Lease::append_row` use the locked journal
descriptor. Replacing the journal name or moving `.nika` cannot redirect them
to a replacement file. The held cursor is serialized locally; append uses
`O_APPEND` and one write containing any torn-tail separator, row and newline.
A short write or sync failure remains uncertain and is never retried by that
operation. The compatibility free functions still resolve names under the
supplied directory; project-facing review and reconciliation use the lease.
These guarantees preserve acquired file custody, not authenticity against an
owner capable of rewriting the journal's contents.
