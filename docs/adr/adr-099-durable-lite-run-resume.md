---
id: ADR-099
title: "Durable-lite run resume — the trace IS the checkpoint"
status: accepted
date: 2026-07-05
phase: ""
deciders: ["@ThibautMelen"]
tags: [resume, durability, trace, cache-hit, human-gate, prompt, cli, conformance]
affects_crates: [nika-cli, nika-runtime, nika-event]
affects_layers: [L2, L4]
supersedes: []
superseded_by: []
related: ["ADR-100", "ADR-106", "ADR-109"]
requires: []
enables: []
amends: []
fci: []
inv: []
shadow_zones: []
nika_codes: []
timeline: ""
follow_ups: []
---

# ADR-099 — Durable-lite run resume · the trace IS the checkpoint

- **Status**: Accepted (2026-07-05) — implemented by the engine
  `feat/resume-impl` arc: `nika-runtime` resume keys (JCS + blake3) +
  the `--resume` skip fold + the pause rider · `nika-cli run --resume`
  / `--from` / `--answer` · `EventKind::{TaskCacheHit, WorkflowPaused}`
  · exit code `4` (`paused`) · the 3 conformance fixtures as engine
  e2e tests (`crates/nika-cli/tests/resume_e2e.rs`).
- **Trust amendment (2026-08-08)** — §5. The resume path's trust
  assumption, stated at last and ENFORCED: the chain is walked before
  the fold. Measured 2026-08-07 on the 0.108.0 binary, a chain-broken
  journal resumed silently (exit 0), propagated a forged output into a
  live task, and emitted a fresh journal whose own chain verified
  clean — laundering. This ADR held zero occurrences of verify / trust
  / tamper: the assumption was never stated, which is how it was never
  questioned.
- **Surface**: CLI + trace/event vocabulary ONLY. Zero envelope change ·
  zero new YAML · the 4-verbs and `nika: v1` locks are untouched.
- **Home**: the CANONICAL prose lives in the `nika-spec` repo's `adr/`
  (language-surface decisions — the relative spec links below resolve
  THERE); this copy registers the number in the shared Nika ADR series
  (the engine `docs/adr/` registry is the series' index).

## Context

Durability is the largest capability gap the spec names about itself.
[08-out-of-scope](../spec/08-out-of-scope.md) holds four interlocking
deferrals: §Persistence defers workflow checkpointing/resumption (with a
`checkpoint:`-block sketch), §Idempotency defers step dedup "together
with the durable-execution waypoint", H1 states plainly that **crash =
re-run from the top**, and H5 admits the human gate's pause-state is
**live** ("the run keeps a process while blocked · durable pauses arrive
with H1"). Across the 2026 workflow-runner landscape, crash-resume and
the durable human gate are the two most-asked-for capabilities this
class of tool gets measured on — Nextflow's `-resume` (content-addressed
task skip) is that ecosystem's single most-loved feature, and it taxes
authors with nothing.

Meanwhile the checkpoint artifact already exists. A conformant Runtime
engine MUST emit the workflow event stream
([07 §Runtime-4](../spec/07-conformance.md#level-2--runtime-conformance)):
`task.completed` records carry per-task outcome, and the reference
engine journals every run as an NDJSON trace (`crates/nika-event/src/kind.rs`
serializes the kinds snake_case: `task_completed` · and pre-reserves
`checkpoint_written`, "a durable run state snapshot"). What is missing
is not a new runtime — it is a **reader**.

The trap to avoid is equally documented: Temporal-class durable
execution buys resume by imposing replay-determinism constraints on
workflow authors (code must be deterministic · side effects must be
wrapped · histories version with the code). That tax is the single most
hated property of the durable-execution class, and this spec's whole
posture (audit-before-run · no hidden magic · the engine owns the how)
forbids exporting it to authors.

## Decision

**`nika run wf.nika.yaml --resume <trace>` re-executes a workflow,
skipping every task whose identity matches a completed record in the
given trace.** The trace is the run's own NDJSON journal — no second
store, no new artifact, no daemon.

### 1 · The skip rule — content-addressed · two hashes · both MUST match

A task is skippable iff the trace holds a `task.completed` record whose

- **task-definition hash** — a canonical serialization of the task's
  behavior-bearing fields (the verb body · `with:` · `output:` ·
  `retry:` / `on_error:` / `on_finally:` · `when:` · `for_each:`) —
  matches the task as now written, AND
- **resolved-input hash** — the values its `${{ }}` references resolved
  to at execution time (upstream outputs · `vars` · `with` · `env`) —
  matches the values the resumed run resolves.

Edit a prompt → that task re-runs. Change a var an early task consumes →
it re-runs, and the mismatch cascades through resolved-input hashes
exactly as far as the data actually flows — untouched siblings still
skip. `for_each` tasks record per-iteration (the item participates in
the input hash), so a partially-completed fan-out resumes at the
iteration level.

**Secrets participate by declared reference identity (name · declared
source path), never by value** — masking is normative
([01 §envelope MUSTs](../spec/01-envelope.md)) and a trace MUST NOT
carry secret-derived material, including value hashes (a hash of a
low-entropy secret is an oracle). Stated limit: a rotated secret does
NOT invalidate the cache — force the affected node with `--from`.

### 2 · The skip is VISIBLE — `task.cache_hit` · never silent

Each skipped task emits a **`task.cache_hit`** event (additive to the
[07 §Runtime-4](../spec/07-conformance.md#level-2--runtime-conformance)
event set · reference-engine trace kind `task_cache_hit`) carrying
`task_id` + a reference to the matched trace record. The task's output
is rehydrated from the record; downstream tasks observe `status:
success` and the bound output exactly as if it had run live. The task
state enum stays CLOSED ([03 §task states](../spec/03-dag.md#task-states)
· no new state): the cache/live distinction rides the event stream and
the run report, never the `${{ }}` surface. Anti-hidden-magic is the
design line — a resumed run's console and trace say `cache_hit` per
skipped task, loudly.

### 3 · `--from <task_id>` — the manual override

Forces re-execution from a node: the named task and its transitive
downstream re-run even on hash match (tasks upstream of it may still
cache-hit). An unknown task id is refused pre-run (the same contract as
`nika run --var` unknown-key refusal). This is the lever for "the world
changed in a way the hashes cannot see" (rotated secret · external
state · an `infer:` output you want re-rolled).

### 4 · Non-deterministic tasks replay their recorded output

An `infer:`/`agent:` task that hash-matches replays the recorded output
— that is the point (crash-resume without re-spending tokens).
Determinism is never the author's problem: there are no replay rules,
no "workflow versioning", no side-effect wrappers. A task that does not
hash-match simply re-runs live, side effects included (see Non-goals).

### 5 · The resume VERIFIES the chain before trusting the records (trust amendment · 2026-08-08)

The trust assumption this ADR left unstated, stated now: **a `--resume`
trace is an authority claim.** Its recorded successes are served as
cache hits, and live tasks run on the values those records carry — so
the journal is one artifact with two roles the literature keeps apart
(the tamper-evident audit record · the resume checkpoint), and the
second role MUST consult the first. Measured 2026-08-07 on the 0.108.0
binary, it did not: a chain-broken journal resumed silently (exit 0),
propagated a forged output into a live task, and emitted a fresh
journal whose own chain verified clean — one resume laundering a
journal that FAILS verification into one that PASSES it.

The law, enforced by the reference engine:

- The trace's tamper-evidence chain is walked BEFORE the fold (the same
  walk `nika trace verify` runs). `Intact` proceeds; `Incomplete` and
  `TornTail` proceed (a crash leaves an honest journal — crash
  resumption is the use case).
- `Broken` — edited, inserted, dropped, reordered, unreadable
  mid-journal, or over the line bound: the run REFUSES (exit 2 · the
  FILE class, one voice with `trace verify`), naming the finding,
  `nika trace verify`, and the opt-out below.
- **`--resume-unverified`** is the NAMED opt-out (a trace the operator
  edited or truncated by hand): the run proceeds loudly, and the NEW
  run's boot manifest journals `resume_unverified: declared` +
  `resume_unverified_finding` (the walk's finding). A laundered trace
  can never claim a clean ancestry silently. The flag is consumed only
  when the walk objects; a verified trace journals no claim.
- A chainless journal (a pre-chain `--json` capture · a pre-0.96
  journal) carries nothing to verify: the resume proceeds with a
  printed notice — trusted WITHOUT verification, said, never silent —
  AND attested on the new run's boot manifest (`resume_unverified:
  unchained` + the reason). The strip-the-chain forgery (tamper, then
  delete every `chain` field) converts the walker's `Broken` into
  `Unchained` and lands exactly here: it cannot convert the refusal
  into a SILENT proceed. Retired 2026-08-09: the `--json` stream
  carries the chain (the sinks share one ChainState · the journal file
  is the stream byte for byte), so a fresh capture verifies and its
  forgery is refused — the compat now covers only pre-chain journals
  and stripped forgeries, and it stays attested, never silent.

### Rider — the durable human gate (`nika:prompt` pause)

Today's normative non-interactive contract
([stdlib §nika:prompt](../stdlib/builtins-v0.1.md)): with `default:`
the engine MUST answer with it; without, it MUST fail
`NIKA-BUILTIN-PROMPT-001` — never hang. The rider occupies **exactly
that failure branch**, changing nothing else:

- Under a non-interactive surface (`--json` · `nika serve`), a blocking
  `nika:prompt` with no usable `default:` journals a **`workflow.paused`**
  event (trace kind `workflow_paused`) carrying the prompt payload
  (`task_id` · `mode` · `message` · `choices` · secret-masked like every
  journal surface) and **exits cleanly** with run state `paused` instead
  of failing.
- **`--resume <trace>` re-arms the prompt**: completed upstream work
  cache-hits, the run continues AT the prompt — interactively it asks;
  resumed non-interactively with an answer supplied (CLI arg · serve
  webhook payload — delivery ergonomics are the engine's) it binds it;
  resumed non-interactively with no answer it pauses again
  (idempotent · a paused trace can be resumed any number of times).
- This resolves H5's noted limitation ("pause-state is live · durable
  pauses arrive with H1") with **zero new syntax** — "ONE construct · a
  tool under `invoke:` · forever" stays exactly as locked.

The one spec-surface delta this rider needs: the run-report/event
vocabulary gains **`paused`** as an additive workflow-state value
([05 §workflow-level semantics](../spec/05-errors.md#workflow-level-error-semantics))
plus the two events above. Like `pending`/`running`, `paused` exists in
run reports and events, never inside `${{ }}`.

## Non-goals

- **No author-facing determinism constraints** (Temporal-style replay
  rules). Side-effectful tasks re-run whenever their hashes mismatch —
  durability is the engine's problem, never the author's. An author who
  needs a side effect to be exactly-once under resume marks the node
  with `--from` discipline or makes the tool idempotent via its own
  args (today's H1 guidance · unchanged).
- **Idempotency keys stay deferred** — the NEXT deferral in this ladder
  ([08 §idempotency](../spec/08-out-of-scope.md#idempotency-keys--step-deduplication)).
  Resume shrinks the re-run window (completed work never re-fires) but
  a crash mid-side-effect can still double-fire on re-run; at-least-once
  dedup remains the full durable-execution waypoint's job.
- **No envelope/syntax change.** The `checkpoint:`-block sketch in
  [08 §Persistence](../spec/08-out-of-scope.md#workflow-checkpointing--resumption)
  REMAINS deferred exactly as written: a workflow never declares
  persistence. This ADR is CLI flags + trace/event vocabulary.
- **No daemon.** `--resume` is a reader of a file. No supervisor · no
  run-store service · no background process · no wake-on-event (durable
  timers ride a later design).
- **Not general memoization.** `task.cache_hit` fires only under
  `--resume`; a fresh `nika run` executes every task. The H17
  `cache_key:` field stays deferred on its own terms.

## Consequences

### Positive

- Crash-resume · re-run-from-a-node · no-token-re-spend land behind ONE
  flag, riding an artifact every Runtime-conformant engine already emits.
- The durable human gate closes the biggest practitioner ask with zero
  new syntax and zero change to answered-prompt behavior (the pause path
  occupies only what is a hard error today — strictly additive).
- Sovereignty intact: the trace is a local NDJSON file · grep-able ·
  portable · no vendor run store, no cloud dependency.
- The determinism tax is structurally refused, not just avoided: there
  is no replay engine to feed, so no replay rules can ever leak to
  authors.

### Negative

- The `task.completed` record fields resume reads (definition hash ·
  input hash · output) become a compatibility surface: they MUST evolve
  additively, and an engine facing a trace it cannot read MUST say so
  and re-run fully — never guess a partial skip.
- The trust amendment (§5) turns a previously silent acceptance into a
  refusal: a chain-broken trace no longer resumes (exit 2), so an
  operator who hand-edits or truncates a journal MUST name
  `--resume-unverified` — attested on the new run's boot manifest. A
  deliberate behavior change, landed with its conformance fixture.
- Secrets-by-name hashing means a rotated secret silently cache-hits;
  the limit is documented and `--from` is the override, but it is a
  real sharp edge to teach.
- `paused` is one more workflow-state value every run-report consumer
  must render.

### Neutral

- Resume conformance is behavioral-tier — it lands with the runtime
  fixtures, post-announce
  ([07 §suite status](../spec/07-conformance.md#suite-status--v01-honest)).
- Another engine MAY keep a different trace file layout: the
  conformance contract binds the EVENTS and the skip/pause semantics,
  not the bytes of the journal.

## Conformance sketch — 3 fixtures · `tests/runtime/resume/`

Behavioral-tier shape per
[conformance/tests/runtime/README.md](../conformance/tests/runtime/README.md)
(`input.nika.yaml` + `run.json` + `expected-run.json` · `mock/echo`) ·
resume fixtures add the two-phase invocation (run → interrupt → resume)
to the runner protocol:

- **(a) `001-kill-midrun-resume-completes-remainder`** — first
  invocation is interrupted after task `a` completes; the resumed run
  MUST cache-hit `a` (`events_include: ["task.cache_hit:a"]`), execute
  only the remainder live, and end `workflow_state: success` with
  outputs identical to an uninterrupted run.
- **(b) `002-input-change-rehashes-and-reruns`** — same trace, one
  `vars` value changed in `run.json`: the consuming task's
  resolved-input hash mismatches → it re-runs (no `task.cache_hit` for
  it) · an untouched sibling branch still cache-hits.
- **(c) `003-paused-prompt-rearms`** — a non-interactive run hits a
  default-less `nika:prompt` → exits `workflow_state: paused`, trace
  carries `workflow.paused` with the prompt payload; resumed with an
  answer supplied → upstream cache-hits, the prompt binds, the run
  completes; resumed without one → it pauses again (idempotent).

## Evidence / Affected surfaces

- [`spec/08-out-of-scope.md`](../spec/08-out-of-scope.md) — §Persistence
  checkpointing (the deferral this ADR lifts at the durable-lite tier) ·
  §Idempotency (the next deferral) · H1 · H5 · H17 (amended this arc).
- [`stdlib/builtins-v0.1.md`](../stdlib/builtins-v0.1.md) §`nika:prompt`
  — the `NIKA-BUILTIN-PROMPT-001` branch the rider occupies.
- [`spec/07-conformance.md`](../spec/07-conformance.md) §Runtime-4 —
  the event vocabulary `task.cache_hit` / `workflow.paused` extend
  additively.
- Reference engine `crates/nika-event/src/kind.rs` — snake_case trace
  kinds (`task_completed`) + the pre-reserved `checkpoint_written`
  ("durable run state snapshot"): the seam was planted before the design.
- External poles · Nextflow `-resume` (the adopted shape ·
  content-addressed task skip · zero author tax) · Temporal workflow
  determinism constraints (the rejected shape · the tax this ADR
  structurally refuses).

## Alternatives considered

### Alt A — full durable-execution runtime (Temporal-class)

Event-sourced replay: re-execute workflow code against recorded
histories, requiring deterministic authorship, wrapped side effects,
and code-version pinning. Rejected — it exports the runtime's problem
to every author forever, contradicts the no-hidden-magic spine, and
buys nothing this design does not already deliver for a finite DAG
whose journal carries every task boundary.

### Alt B — sqlite checkpoint store

A dedicated `checkpoints.db` beside the trace (the shape the old 08
sketch reserved). Rejected for v1 — a second stateful artifact to
create, locate, version, and explain, duplicating data the NDJSON trace
already journals. One artifact · plain text · portable. An engine MAY
later keep an internal index (sqlite or otherwise) as a pure
optimization — invisible to this contract.

### Alt C — do nothing

Rejected — crash-resume and the durable human gate are the top two
practitioner asks of this tool class, and H5's live-pause limitation is
flagged in the spec's own text. Waiting for the full durable-execution
waypoint gates the 90% capability on the 10% problem (idempotent
delivery) that finite single-run DAGs mostly do not have.

## Related

- [`spec/08-out-of-scope.md`](../spec/08-out-of-scope.md) §Persistence ·
  §Idempotency · H1 · H5 · H17
- [`spec/07-conformance.md`](../spec/07-conformance.md) §Runtime
  conformance · §suite status
- [`spec/03-dag.md`](../spec/03-dag.md) §task states (closed enum ·
  unchanged) · [`spec/05-errors.md`](../spec/05-errors.md)
  §workflow-level semantics (`paused` additive)
- [`stdlib/builtins-v0.1.md`](../stdlib/builtins-v0.1.md) §`nika:prompt`

## Notes

- **Registration** · DONE (this file · the `feat/resume-impl` arc) —
  ADR-099 is registered in the engine `docs/adr/` index (the series
  registry).
- **Status flip** · DONE · `proposed → accepted` with the engine
  implementation; the behavioral fixtures of the Conformance sketch
  ship as `crates/nika-cli/tests/resume_e2e.rs`. (The spec-repo copy's
  frontmatter flip is a spec-repo commit — tracked there.)
- **Revisit trigger** · if implementation shows secrets-by-name hashing
  is too blunt (rotated-secret staleness bites in practice), a keyed
  commitment (HMAC under an engine-local key · never a bare hash)
  re-opens HERE first — the trace-carries-no-secret-material invariant
  is not negotiable.
