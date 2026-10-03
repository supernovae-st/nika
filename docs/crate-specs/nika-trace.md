# Crate spec — `nika-trace` (descended member)

| | |
|---|---|
| Status | **DESCENDED 2026-08-11** from `nika-cli` — NOT a fresh admission: a size-cap member split of an already-admitted unit per D-2026-07-09-N1 (one architectural unit · two workspace members · the ADR-110 `nika-cli-host` precedent). `nika-cli` measured 15,040 prod LOC at the vector-24 gate (cap 15,000); the trace-reading plane was the clean seam (every consumer reaches it through `verbs::trace*`, and the compute half — chain walk · anchor wire · recover · store scan — had already descended to `nika-dap` 2026-07-09). |
| Layer | **L4** — the operator surface's read half: renders + routes, every effect already below. |
| Design | The **flight-recorder reader** — every surface that READS `.nika/traces/` (the NDJSON journals a run records): `trace show\|replay\|outputs\|peek\|flow` (the fold's render), `trace ls\|rm` (store management · ADR-100), `trace verify` (tamper-evidence chain · minisign signature · anchor tiers), `trace anchor` (Rekor v2 · RFC 3161 notary), `trace reproduce`, `trace export` (OTel), the `evidence` pack, the `receipt` explainer, the learned-truth `forecast` behind `explain --forecast`, the run facts (`run_view::RunFacts`) behind the native session's result, gate and `/proof` views (moved in 2026-09-24 · §2), the read-only `lineage` view of which journals continued a paused run (derived from DAP's `store::survey` · §5), and the bin's `trace` dispatch arm (`dispatch.rs` — the replay loop + door routing descended verbatim from `main.rs`, which keeps a one-line arm). `nika-cli` re-exports every public item at its historical `verbs::` path — call sites, the 32 integration suites and the clap tree read unchanged. |
| Name | `nika-trace` — the plane it reads, named after the verb it serves. Descent precedent: `nika-cli-host` (2026-07-31). |
| LOC | ~6087 LOC src (`scripts/crate-metrics.sh --loc nika-trace` · ±15% band per vector 6) — ≈3.3k of it prod (the counter's cfg(test) scope); the descent lifted `nika-cli` from 15,040 to 11,851 prod. |
| Deps | `nika-dap` (the forensics compute), `nika-cli-host` (VerbOutput/exit · retention config), `nika-display` (Theme · RunView · frame), `nika-event`, `nika-types`, `clap`, `serde`, `serde_json`. dev: `uuid`. |
| Publish | `false` — internal member of the `nika-cli` unit (the binary ships, the lib doesn't). |

## 1 · Why this crate exists

Two reasons, one mechanism (the same two as every descent):

1. **The cap held.** `≤15k prod LOC/crate` is a hard law; the trust-experience
   arc's trace surface (verify tiers · anchor · evidence · forecast) tipped
   `nika-cli` over it. The sanctioned move is a member split, not an
   exemption — the unit (the operator surface) stays ONE architectural
   unit; the workspace gains one member.
2. **The seam was real.** The plane is read-only over the journals, its
   compute had already descended (`nika-dap` hosts chain/anchor/recover/
   store/stats/seal), and its only in-workspace consumer is `nika-cli`
   itself (re-export) — plus `explain --forecast`, which reads the
   descended `forecast` module through the same re-export. Nothing else in
   the workspace imports these paths.

## 2 · Known residue (owned)

- **The staged-journal test fixtures are duplicated cli-side.** The
  `#[cfg(test)]` fixture sets (`trace::store::tests` · `forecast::gather::
  tests`) cannot cross a crate boundary, and `explain_file`'s staged-history
  integration tests stay with their subject in `nika-cli`. They carry a
  local twin of the six fixture fns (`temp_store` · `stage_trace` · `ev` ·
  `run_body` · `task_done` · `done`) — the established per-crate fixture
  pattern (`nika-dap`'s store tests already twin the same helpers). If a
  third consumer ever needs them, the honest home is a `testkit` seam, not
  a third copy.
- **The `store`/`retention` shims narrowed, deliberately.** They were
  `pub(crate)` inside `nika-cli`; rather than widen them to `pub` through
  the re-export, the two remaining cli-side consumers (`run`'s start-GC ·
  `explain_file`'s census) now read the descended homes directly
  (`nika_dap::store` · `nika_cli_host::retention`). `nika-cli`'s public
  surface did not gain an item.
- **The run facts moved in (2026-09-24).** `run_view` came verbatim from
  `nika-session`. It reads a run's journal frames (tasks · permit decisions ·
  approvals · the pause · the seal · the terminal word and cost counters) and
  says three views of them: the result after a run ended, the gate when it
  paused, and the proof through the ONE verify door (`trace_verify::verify`).
  It is the same plane as the rest of this member, read-only over
  `.nika/traces/`, and it needed no new dependency (`nika-event` for the
  digests, `serde_json` for the frames). The session owned none of it; it
  kept only when a view is shown, the gate's question and the gated tasks it
  reads from the workflow's bytes. The public seam is deliberately four
  read-only doors, `RunFacts::{read, result, gate, proof}` (`#[non_exhaustive]`,
  every field and every per-task, permit, approval, pause or seal fact
  crate-private); `nika-session` reads it laterally (L4→L4, never back)
  through a private `use nika_trace::run_view`, so its `crate::run_view` path is
  unchanged, and `nika-cli` does not re-export it. The move answers the size
  cap the same way the 2026-08-11 descent did: `nika-session` stood above
  15,000 prod LOC, and `nika-trace` stays far below it. Its tests moved with
  it. Their paused/resumed fixtures (and `gated.nika`, the workflow they
  record) now live in `tests/fixtures/traces/` here. `copy.ndjson` is
  copied, because the session's own observation test still reads its copy.
  Their temporary directories use `std::env::temp_dir()` like this member's
  other suites, not a new `tempfile` dev-dependency.
- **A fifth door, the pause's gate (C9, 2026-09-28).** `RunFacts::pause_gate`
  says the gate a host answers: the task, message and mode of the journal's
  FIRST `workflow_paused` frame, each the first value its fields give that key,
  with the pause's defaults (« the run awaits your answer » · `text`); `None`
  when the journal never paused or that first pause names no task (absent, not
  text, or empty — a later pause never stands in for it). It is a separate
  owned fact beside `pause`, which the result and proof views keep reading as
  the last pause. `nika-session`'s `PendingGate::from_trace` reads it instead of
  parsing the journal a second time.
- **What a resume runs again (C10 · Q8).** `run_view::resumed_live(trace)` names
  the completed tasks a resume of a paused journal is sure to run again, live:
  each completion the resume plan does not carry (no resume identity, or an
  output that does not read back), judged by `nika_dap::resume::fold_plan`
  itself, in journal order and once each. `live_again` is the host-neutral line
  a host says before the answer that resumes, `None` when the plan carries every
  completion; the gate view says it too. Silence promises nothing more: the run
  serves a carried completion only while its definition and inputs are
  unchanged, which the run judges, not the journal fold.
- **The size formatter (C10).** `run_view::human_size` is public: the session's
  produced-files line reads it instead of keeping an identical copy.

### Captured journal and kept run

`trace_verify::verify_captured(trace, raw, opts)` applies the same verifier to
already captured journal bytes, within `JOURNAL_BOUND`; it refuses replay and
never reopens the journal. Custody keys, anchor sidecar and liveness remain
separate reads by their existing owners. `RunFacts::of` folds the same bytes
without I/O or a verification claim. Its execution/start/hash observations let
a host bind that verdict; `terminal` returns no terminal word when none was
observed, and names `paused` for a journal ending at a gate.

`run_view::KeptRun` is a pure, closed, versioned observation of workflow, exit,
trace, execution, source hash and receipt head/length. Missing parts stay absent.
Its owner persists it; this crate adds neither history storage nor authority.

## 3 · Cost door (P4, 2026-09-28)

`nika trace cost` is the operator's door onto the cost journal. The law and
the documents belong to `nika_dap::cost_journal::reconcile`; this member only
supplies what the host has (the launch directory, the OS account from
`nika_cli_host::probe::operator_account`, the clock, a fresh execution id) and
renders.
- `nika trace cost [--json]` inspects. Its document is
  `cost_inspect_version: 1`, and it names any UNKNOWN it recorded.
- `nika trace cost reconcile <INVOCATION> --project <BINDING> --prior <SHA256> --resolution billed|not-billed|still-unknown --reference <TEXT> [--evidence operator-attestation] [--json]`
  appends one resolution. Its document is `cost_reconcile_version: 1`. There
  is no prompt, and no class other than the operator's own, unverified
  attestation parses.
- Exit codes: `0` printed or appended; `2` the request refused; `3` the
  environment refused (busy lease, unreadable journal).
- JSON always goes to stdout, including a refusal (`{"refused": {kind,
  message, …}}`). The suggested command shell-quotes the invocation, because an
  earlier engine recorded `ExecutionId { uuid: … }`.

The door hangs off a new `#[non_exhaustive]` `TraceCommand`: `Cost` plus
`Legacy(TraceAction)`, flattened. `TraceAction` keeps its exact variants, so
its constructors and any exhaustive match compile unchanged, and every old
`trace` command line parses to the same variant (unit-pinned). The bin changes
only its field type and dispatch call, and `trace_verb` is untouched.

## 4 · Fan-out item words (B8, 2026-09-28)

Spec 03/17 add `cancelled` (the iteration began and was abandoned without a
recorded terminal) beside `never_started` (it never began). This is a
closed-vocabulary extension for the next engine MINOR after 0.121.
- `trace outputs --json` and `peek` project each row's status verbatim.
- The `show` companion tallies `cancelled` on its own line item, apart from
  `never_started`.
- A word outside the vocabulary stays uninterpreted data: it is printed, and
  never tallied as a known outcome.
- Paged tables complete only through the `nika-display` fold, which enforces
  the `items_cancelled` law (see that crate's spec).
- Neither word is a billing verdict or proof about physical requests.

## 5 · The lineage view (C7c, 2026-09-28)

`lineage::fold(survey, paused)` and `lineage::lineage_of(dir, paused)` answer
one question for the session and trace readers: which journals of ONE trace
directory continued a paused run (the `resumed_from` link, #1462).

The facts come from `nika_dap::store::survey`: reading, recovery, identity
and every skip or doubt stay in DAP, the compute. This module only folds
them, the way `run_view` folds one journal. The view is not runtime
admission, cryptographic verification or continuation authority, and it
never verifies a chain, seal or signature.

The verdicts:

- `NoneObserved`: a complete survey in which no journal names the paused run.
  It records what one read of one directory held. It is never an
  authorization.
- `Chain { links, head }`: one linear chain whose links all agree. The head
  is one of:
  - settled;
  - paused again, at its own gate;
  - running, with its ADR-129 liveness.
- `Indeterminate(reasons)`, with every reason accumulated:
  - unsurveyed entries or listing errors;
  - an unidentified paused journal, or one no longer paused;
  - a torn suffix on a journal the lineage depends on;
  - any journal whose link is unknown or ambiguous;
  - a duplicate identity;
  - a fork (siblings are never ranked by time or name);
  - a cycle;
  - a disagreeing continuation;
  - a chain longer than `MAX_LINKS` (64), where the walk terminates.

Limits stated by the view itself: a continuation before its first frame or
after the survey, a pruned or removed journal, a copied or tampered journal
and an unverified resume are all out of sight.

Neither atomicity nor exactly-once is claimed. The engine's resume admission
(the approval ticket's single-use claim, per claim store and TTL) still
decides a race, within its own scope.

`Lineage::standing(paused, own_doubt_stands)` (C7) says, for a host that would
offer the pause again, where it stands: it stands (no journal continued it),
a single chain paused again at a named journal, a continuation settled (its
state and journal file name), one has not settled (its liveness), or the
journals cannot decide (every reason in words: `Undecided` implements
`Display`). `own_doubt_stands` is for a host that observed the pause itself:
when the only doubt is that pause's own journal (it names no run, or it is the
one entry that could not be folded) or no trace store exists, nothing could be
followed and it stands; any other doubt beside it decides nothing. The words
name no host protocol; the host says where it stands and keeps its own way on.

## Task outcome projection compatibility

`trace outputs --json` emits `outputs_version: 2`. Its task rows, also
returned by `trace::tasks_json`, separate the current recorded terminal
error from recovery provenance:

- `cause` is the recorded outcome cause, or null when its class/cause pair
  is absent, invalid, unknown, or inconsistent with the event kind.
- `error_code` and `error_message` read `payload.error` for failure and
  skipped/error_skip only. Missing or non-string leaves stay null.
- `recovered_from` remains a string or null: the original recovery code.
  A recovered success has null terminal error fields.

Version 1 used `error_code` as an alias for the recovery code. Consumers
that read that alias must use `recovered_from`; consumers of terminal
errors should require version 2. The command syntax and Rust function
signature are unchanged; `tasks_json` itself has no version envelope.
The trace event format, `peek`, and the human output table are unchanged.

Only the current task occurrence supplies these facts. The latest relevant
event in journal order wins, regardless of timestamps. A new start,
schedule, retry, recovery-in-progress, matching pause, or workflow start
prevents borrowing an earlier terminal outcome. A missing or malformed
current outcome never falls back to an earlier terminal or display prose.
A cache hit reads only a recorded normal success.

For legacy traces, a completion with **no** outcome field may use the
nearest explicit recovery marker within the same occurrence, stopping at
an earlier terminal, start, schedule, retry, pause or workflow start. A
present but malformed outcome does not enable that fallback. This marker
can establish recovery without fabricating a cause or terminal error.
Recovery status is reported only on the current successful observation;
a sticky display flag cannot override a newer failure or running state.
Messages preserve recorded text, including Unicode and newlines; no code
or cause is inferred from prose. These are projections of recorded facts,
not trace-integrity verification or a claim of fresh execution.
