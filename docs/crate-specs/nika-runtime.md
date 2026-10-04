# nika-runtime — crate spec (L3 · orchestration)

> Gate 1 artifact · v2 · 2026-06-12 · the first L3 crate. v1 (admission
> `2e0386d3a`) shipped the conformance-floor executor the nika-cli
> rehearsal pinned. v2 is the **v0.1 spec-parity engine**: the full task
> pipeline of `nika-spec` 03/04/05 (gates · records · `with:` · retry ·
> timeout · `on_error:` · `for_each:` · the unwind cleanup lane) + bounded
> intra-wave concurrency with ordered settlement. Research-grounded —
> every mechanism cites its paper or its spec section (citation law).
>
> **Language note (teaches 0.109 · amended 2026-08-19).** This spec was
> written against the fourteen-key envelope. Two surfaces it names have
> since left the language and this text follows: the task-level
> `on_finally:` list is dead (2026-08-11) — cleanup is an ORDINARY task
> joined by `after: { <parent>: unwind }` (a `finally` node in
> `graph_format: 3`), and `on_error.fail_workflow` is dead — the default
> IS failure, `on_error:` is `recover:` or `skip:`. The mechanisms below
> (sequential best-effort cleanup · swallowed cleanup errors · the
> parent's record visible to the cleanup gate) are unchanged; only the
> spelling an author writes moved.


## Paged fan-out evidence

`emit_items` owns both successful and failed fan-out item emission. Small
arrays retain their inline bytes. Larger tables emit ordered `task_items`
frames before the task terminal, which closes the set with page, row and
status counts. The internal 64 KiB target measures the doubly encoded JSON
text and leaves space for the event envelope. An unpageable individual row
falls back to the full inline representation; the writer still refuses an
oversized record. No item is silently truncated, and journal bounds are not
raised. The wire contract is spec 17's paged item evidence section.


## Pending approval rendering

A direct `nika:prompt` pause renders its message and choices using the same
secret-marker scope and task `with:` bindings as the approval content hash.
Thus headless CLI consumers can review the resolved question before answering;
the resumed decision attests the same question. Secret references remain
markers, not resolved values. An unresolved binding retains the existing raw
argument fallback and does not relax execution errors. The wire fields and
approval content recipe are unchanged. This does not add a remote approval
payload to Serve's conservative service projection.


## 1 · Role

Execute one **checked** workflow wave-by-wave through the four verb
crates, emitting the canonical event stream. The runtime is the ONE
emission site per verb path (INV-024 · the verbs stay event-free) and
the ONE place run state (task records · dataflow) lives.

```text
RawWorkflow + CheckReport (clean)        nika-schema   (audit BEFORE run)
        │
        ▼
nika_runtime::Runtime::run()             THIS CRATE
        │  waves (CheckReport order) · per-wave bounded concurrency
        │  ordered settlement (deterministic event stream)
        │  task pipeline · gate → with → for_each → retry/timeout →
        │                  on_error → unwind cleanup → settle
        ├──▶ infer  → nika-verb-infer
        ├──▶ exec   → nika-verb-exec
        ├──▶ invoke → nika-verb-invoke
        ├──▶ agent  → nika-verb-agent
        ▼
Vec<Event> via EventSink                 nika-event    (display folds it)
```

## 2 · Public API (v2)

```rust
pub struct Runtime<S, T, H, P, D, C> { /* 4 verbs + clock + config */ }

impl<…> Runtime<S, T, H, P, D, C>
where
    S: ShellRunDyn + Sync, T: ToolExecuteDyn,
    H: HttpPostDyn + Send + Sync + 'static,
    P: ProviderInferDyn, D: ToolDefinitionProviderDyn,
    C: ClockDyn + Sync,            // sleep = backoff + timeout (kernel seam)
{
    pub fn new(shell, invoke, infer, agent, clock: C, config: RuntimeConfig) -> Self;
    pub async fn run(&self, wf, report, stamper: &mut dyn Stamper,
                     sink: &mut dyn EventSink) -> Result<RunOutcome, RuntimeError>;
}

pub struct RuntimeConfig {
    /// Per-wave in-flight cap (for_each has its own `max_parallel`).
    /// None = wave-width (unbounded within the wave).
    pub wave_parallelism: Option<NonZeroUsize>,
    /// Seed for the retry full-jitter PRNG (splitmix64 over
    /// (seed, task, attempt) — pure · replay-stable · no RNG state).
    pub jitter_seed: u64,
}

pub struct RunOutcome {
    pub ok: bool,
    pub records: BTreeMap<String, TaskRecord>,   // the result records (04)
    pub outputs: BTreeMap<String, Value>,        // workflow outputs:
}

pub struct TaskRecord {                          // spec 04 §task reference
    pub status: TaskStatus,                      // success|failure|skipped|cancelled
    pub output: Value,                           // Null on skipped/cancelled
    pub error: Option<TaskErrorRecord>,          // present iff failure (+ on_error.skip)
    pub started_at / ended_at: Option<Timestamp>,
    pub duration_ms: Option<u64>,
}

#[non_exhaustive]
pub struct EngineIdentity { /* private compile-bound fields */ }

pub const fn engine_identity() -> &'static EngineIdentity;
```

`EngineIdentity` is the one provenance authority shared by CLI, runtime and
future network adapters: engine version, build stamp, exact spec commit and
remote execution API generation. `spec_sha` names the language source;
`api_version` names the transport protocol and is deliberately a different
clock. The runtime build refuses unless root `SPEC_PIN` equals the generated
`nika-pack/pack/SPEC_SHA`, so conformance and embedded documentation cannot
describe different specs inside one binary.

**Why generic over 6 seams** · the agent tool-defs impl lives in
`nika-builtin` · the clock impl in `nika-clock`
(L1 effect). The runtime CORE (the DAG executor) never names a
concrete effect — the four verbs arrive PRE-CONSTRUCTED and async
rides the injected seams.

**Amended 2026-07-22 (the run-verb descent)** · the production
COMPOSITION descended from `nika-cli` at the 15k wall (compute
descends, render stays): `compose.rs` wires the real effects
(`TokioFs` · `ReqwestHttp` · `SystemClock` · `TokioShell` ·
`ProviderRegistry` with env-resolved keys · the sandbox pair) into
the generic `Runtime` for every embedder (cli today · daemon/serve/
sdk tomorrow), `SystemStamper` joined the stamper family, and the
launch gates grew the `--task` cone + the budget floor beside the
required-input refusal. The core stays seam-generic — the new Cargo
edges (the L1/L1.5 effect crates · all strictly downward, acyclic)
serve the composer module only, and the crate's own code keeps ZERO
tokio edge (the effect crates wrap their own; the executor stays the
embedder's). The `child_runner` production impl deliberately stayed
in `nika-cli`: it speaks the journal's concrete `TraceFileSink` +
`TRACE_DIR`, and the journal's home is L4 (`nika-dap`) — an L3 crate
cannot reach up for it.

## 3 · Execution model (v0.1 · spec 03 §DAG execution model)

### 3.1 Waves + bounded concurrency + ordered settlement

**Emission timing (#1362)**: `task_started` and the task's terminal frame are
stamped together during ordered settlement. Their timestamps are emission time;
the terminal `duration_ms` carries measured task duration. The CLI/SDK event
stream therefore cannot serve as a live provider heartbeat. `trace ls --json`
observes the writer lease separately, which proves writer liveness only. Workflow
`infer:` uses the buffered provider door; the kernel provider streaming API is
separate. This preserves the deterministic event ordering described below.

- `CheckReport.waves` is the schedule (the checker owns topology · the
  runtime never re-sorts · a bad index is NIKA-1701). Wave-barrier
  (BSP) execution is a deliberate v0 trade: bounded makespan loss at
  workflow widths (≤50) vs static auditability — per Graham 1969
  (list-schedule 2−1/m bound) · Nelson & Tantawi 1988 (fork/join
  barrier cost ~H_k) · Buttari et al. 2009 (async DAG gains). Eager
  dispatch is a future seam the settlement contract already permits.
- Within a wave, tasks **dispatch concurrently** (cap =
  `wave_parallelism`) and **settle in wave order**: dispatch is pure
  (returns an `Outcome` · no emission), settlement owns the pens
  (stamper + sink) and runs sequentially in the checker's task order.
  This is the canonical deterministic-parallelism pattern — Blelloch
  et al. PPoPP 2012 (deterministic reservations · commit in fixed
  priority order) · Thomson et al. SIGMOD 2012 (Calvin · sequencer
  fixes order ahead of execution) · Kahn 1974 (the determinism floor).
  Consequence: **the event stream is byte-identical for any cap ≥ 1**
  (the cap-equivalence test pins this).
- Mechanism: `futures_util::StreamExt::buffered(k)` over the wave's
  dispatch futures — polls up to k concurrently, yields in submission
  order (source-verified semantics · futures-util 0.3.31). Send-free
  (single-task concurrency · no spawn). The settle body is sync
  (event emission) — no in-flight stall (the "Barbara" pitfall).
- **In-flight drain** (spec 05 §workflow-level) · a sibling failure
  never aborts a running task — all wave members settle.

### 3.2 The gate (spec 03 §task states · §when)

- **Default gate** (no `when:`) · run iff ALL deps ∈ {success,
  skipped} · else the task is **`cancelled`** (emits `TaskCancelled` ·
  note `upstream failed/cancelled` · propagates downstream — the
  Dead-Path-Elimination pattern · Ouyang et al. SCP 2007 · skipped
  nodes still fire an observable token).
- **Explicit `when:` REPLACES the default gate** · evaluated once deps
  are terminal whatever their status · `true` → run (the
  always-pattern — a notify task runs even in a failing workflow) ·
  `false` → `skipped` (emits `TaskSkipped` · note `when: gate
  closed`). Evaluation error → the task FAILS (NIKA-1702/1703 in the
  detail · cascade · never a run abort).
- v1's `TaskSkipped(upstream failed)` cascade emission is SUPERSEDED
  by `TaskCancelled` per spec 03's closed status enum (the event
  taxonomy's `TaskSkipped` doc comment is amended in lockstep).

### 3.3 Expressions (v0 subset · the CEL seam)

Value-model scope (spec 04): namespaces `vars.*` (envelope defaults ·
typed or untyped · JSON values) · `with.*` (task-local · rendered
per task / per iteration) · `item` / `index` (`for_each` locals) ·
`tasks.<id>.{output,status,error,started_at,ended_at,duration_ms}`
(the result record · closed field set · named jq bindings DEFER with
`output:` below). **Defined-null reads** (04 §branch-join unlock):
record fields of a terminal task never error — absent = `Value::Null`
(skipped/cancelled output → null · error of a non-failure → null).
Unknown reference = NIKA-1702 loud · out-of-subset form = NIKA-1703
loud. Rendering into string positions (04 §value rendering): scalars
natural (`null` → `null`) · objects/arrays compact JSON with **sorted
keys** (deterministic). Single-pass island scan — injected values are
DATA, never re-scanned. `when:` v0 subset: `<ref> == '<lit>'` ·
`<ref> != '<lit>'` · bare `<ref>` truthy (null/false/0/empty/
`"no"`/`"false"` → false · CEL replaces truthiness with bool-typing
at the 03-dag milestone, deliberately).

### 3.4 Retry (spec 05 §retry · schema `RetryConfig`)

- Transient-only (`error.transient` — the verbs' `NikaErrorCode::
  is_transient()` feeds it) unless `on_codes:` whitelists the final
  error's wire code. `max_attempts` strict · last error surfaces.
- Direct `invoke` calls to `nika:fetch` also apply
  `nika_types::net::retry_is_effect_safe` to the resolved method and
  header names at dispatch. A mutating request without a declared
  idempotency key cannot retry a failure, including response extraction
  failures matched by `on_codes`. The failed attempt is debited before
  this veto; its error and commit evidence remain intact. A declared key
  permits retry under the existing contract; it does not prove receiver
  deduplication. This guard does not cover whole-agent or child-workflow
  replay.
- Backoff per the spec's three strategies (`fixed` · `linear` ·
  `exponential`, capped at `backoff_max_ms`) + **full jitter** when
  `jitter: true` (default) — Brooker (AWS Architecture Blog 2015) ·
  cap per Bender et al. JACM 2019 (uncapped backoff loses throughput).
  Delay arithmetic mirrors `nika_types::retry::delay_for_ms`'s
  blend/clamp discipline (the shared semantics) extended with the two
  spec ramps — the adapter graduates into nika-types on a second
  consumer (stress-to-ratchet).
- Jitter randomness: splitmix64 over `(jitter_seed, task-id hash,
  attempt)` — pure · Sync · replay-stable by construction (no RNG
  state · no logged-sleep requirement). The chosen `delay_ms` lands ON
  the `TaskRetrying` event (attempt · max_attempts · delay_ms fields ·
  the display contract's `↻`).
- Sleeps via the injected kernel clock (`ClockDyn::sleep` ·
  cancel-safe · MockClock = instant in tests).

### 3.5 Timeout (spec 03 §timeout)

ONE per-task wall-clock budget covering the whole attempt loop
(retries + backoff sleeps included) — the spec deliberately rejects
Temporal's per-attempt/per-schedule split at v0.1 ("the timeout
already covered the retries by definition"). Implemented as a
`select` race: the task pipeline vs `clock.sleep(timeout)` ·
loser-dropped (drop-cancellation is the futures contract · exec
subprocesses die via the runner's kill_on_drop). On expiry: the task
fails with the spec wire code `NIKA-TIMEOUT-001` (catchable by
`on_error:` · NEVER retryable · `transient: false`) — emitted as
`TaskFailed` (the timeout is an error class, not an operator
cancellation; `TaskCancelled` stays the decision class per the event
taxonomy). On a `for_each` task the budget applies **per iteration**.

### 3.6 `on_error:` (spec 05 · schema `OnError`)

After retries exhaust: `on_codes:` filter (empty = all) → action:
`recover: <value>` (render the value — a `${{ }}` ref or literal —
task becomes **success** with the recovered output) · `skip: true`
(task becomes **skipped** · the original error STAYS readable at
`tasks.X.error` — the one status where both coexist). The default IS
failure and has no keyword (`fail_workflow: true` died 2026-08-11 · an
author who wants the default omits `on_error:`). Unlisted code falls
through to fail. v0 recover-ref resolution: against the records at recovery
time — a ref to a not-yet-terminal task fails the recovery (the task
fails as if `on_error:` were absent) · the spec's step-3 await
arrives with eager dispatch (documented divergence · LOUD over
silent).

### 3.7 `for_each:` (spec 03 §for_each · closed at v1)

- Collection = the rendered single-island expression or literal list ·
  MUST be an array (else the task fails · `NIKA-VAR-006` class) ·
  empty → `skipped`.
- Per-iteration scope: `item` + `index` bound · **every body
  expression re-evaluates per iteration** (`with:` · verb fields) ·
  the only once-evaluated expression is the collection itself.
  (Spec-drift note · 03 §for_each lists `when:` BOTH among the
  per-iteration re-evaluations AND as "evaluated once before the
  fan-out" — the engine implements the second, more specific bullet:
  ONE gate evaluation before the fan-out · `item`/`index` are not in
  scope in a gate. Flagged for a spec erratum.)
- Per-iteration retry jitter rides a DISTINCT stream
  (`task[index]` coordinates) — anti-thundering-herd applies WITHIN
  a fan-out (Brooker 2015) · replay-stable (the index is part of the
  deterministic coordinates).
- Iterations dispatch concurrently capped by `max_parallel` (default
  unbounded) · are collected in COMPLETION order and folded in input
  order (rows · outputs · spend) · `retry:`/`timeout:`/`on_error:`
  apply per iteration.
- `fail_fast: true` (default) · the first error to COMPLETE, whatever
  its index, drops the remaining stream at once (in-flight cancelled ·
  unspawned never start) · `false` · all iterations run · failed slots
  contribute `null` at their index (positional alignment survives ·
  spec §null-at-index).
- B10 · 2026-09-28 · the collector used to read iterations in input
  order (`buffered`), so an error that completed early waited behind a
  slower earlier-index sibling: the fan ran on until that sibling's
  timeout or answer, and the finished error then read `cancelled` (the
  D6 measurement). `run_fan_out` now drives the batch with
  `buffer_unordered`, and each iteration carries its index
  (`started_on_first_poll`). `collect_fan_out` keeps every iteration
  that completed before the stop, and `items_json` fills each index
  that has no row. A completed failure or success is therefore never
  relabelled by a slower sibling. Two iterations completing in the same
  instant as the stop may still leave one unread (`cancelled`): which
  one the collector meets first is not a contract.
- A dropped in-flight iteration, like an attempt its `timeout:` drops,
  has usually sent its provider request already (B7 · 2026-09-28). The
  attempt loop runs each dispatch under
  `nika_providers::dispatch_journal`. A dispatch that returns folds its
  own evidence as before. One dropped first hands the ledger every
  request it sent, once: unanswered ones unpriced (never a known zero),
  answered ones by their own evidence. The drop itself is unchanged
  (spec 03 aborts the remaining iterations immediately).
- Item rows tell `cancelled` from `never_started` (B8 · 2026-09-28 ·
  spec 03/17, a closed-vocabulary extension for the next MINOR after
  0.121). `run_fan_out` wraps each iteration future in
  `fan_out::started_on_first_poll`, which raises that item's flag on its
  first poll (building a future is not execution). After the collector
  stops, an unconsumed item whose flag rose reads `cancelled` (began,
  abandoned without a recorded terminal, including, since B10, only an
  outcome that finished in the same instant as the stop and was never
  read), and one whose flag never rose
  reads `never_started` (a queued item, or one the budget never
  admitted). Recorded rows, outputs and the immediate abort are
  unchanged; nothing is drained, and neither word is a billing or
  physical-request verdict (the ledger owns spend, above). Paged
  terminals always carry `items_cancelled`, including 0.
- Task output = the array of per-iteration outputs in input order ·
  task status = failure if ANY iteration failed unrecovered.
- Events: ONE task-level Started/Completed/Failed pair (iterations are
  internal · the note carries `for_each · N items`) — the event
  grammar has no per-iteration id space at v0.1.
- Call evidence (E33 · 2026-09-28): that one parent frame carries the call
  records of every iteration the collector read, each record once, in input
  order, whatever order they completed in.
  - They ride as `inference_calls` and `cost_unknown_calls`, through the same
    durable projection as any task's.
  - The parent's split holds calls only: no meters, `attempts` or single-route
    `pricing_route` of its own, as for an authored retry's joined attempts.
    Each call element keeps its own pricing.
  - A request dropped in flight (a cancelled sibling, or an attempt its
    `timeout:` cut) returns no transport report, so it has no call record
    here, and none is invented. The ledger and the observed account keep it
    (B7).
  - This is presentation only: the ledger debits at each iteration's dispatch,
    so no call is charged twice.
  - Before E33, the parent wrote `usage: None`, and a fan-out's calls reached no
    frame.

### 3.8 the unwind cleanup lane (spec 03 §`unwind` · ALWAYS runs · was `on_finally:` until 2026-08-11)

For a task that **started**: after its terminal status, run its
cleanup tasks (ordinary tasks declaring `after: { <parent>: unwind }`)
**sequentially in declaration order** · each with its
own `when:` (the scope sees the parent's fresh record — status/error
routing) + `timeout:` (default 30s). Cleanup outcomes are best-effort:
errors are swallowed (the parent's status reflects ONLY the main
verb) — consistent with the cross-engine canon (cleanup never masks
the original error · Sagas '87 lineage · Temporal detached scopes).
Never-started tasks (skipped gate · cancelled) run NO cleanup. Since
`graph_format: 3` every cleanup task is a projected node (`kind:
"finally"` · the author's own task id) — v0's anonymous mini-tasks
(no id grammar · no engine events) are the shape this lane replaced.

Best-effort is never money authority (B11). A cleanup dispatches on the
run's own ledger with the main lane's attempt seams: a seat only the run
decides meets the same pre-send guard (NIKA-1704 before any byte,
journaled on the cleanup lane), every request it sends is debited once
(a served answer at its price; a request that failed, or that the
cleanup's own timer dropped, as an unknown charge through the dispatch
journal), and a cleanup `invoke: workflow:` child runs under the run's
remaining budget (law 6). The remaining budget is a snapshot at call
time, never a reservation. Once the run's budget is crossed, a cleanup
that can spend (a model call, an agent, a child workflow, image or
speech generation, or a verb this runtime does not know) is refused
before dispatch (NIKA-1704, journaled), as the main lane starts no task
after a trip; `exec` and the other builtins are housekeeping and still
run. An unknown-cost route stays refused before
any byte in this lane: the Host's unknown-cost review refuses a workflow
with an `unwind` task, and the Run observer refuses a route rendered at
run time.

### 3.9 Settlement + records + terminal

Settle order = wave order (3.1). Per task: `TaskStarted` (note =
dispatch note) · `TaskRetrying`× (attempt history) · terminal event
(`TaskCompleted` + `duration_ms` + tokens? · `TaskFailed` + detail +
`duration_ms` · `TaskSkipped` · `TaskCancelled`) — `started_at` /
`ended_at` = the two stamps (event identity · settle-time) ·
`duration_ms` = **clock-derived** (the injected `ClockDyn` measures
the actual attempt-loop wall time · 0 under MockClock · the stamps
are NOT the duration source — a settle-time stamp pair would lie
about a task that ran long before its settle slot). Record inserted at settle. Terminal: `WorkflowCompleted`
iff zero unrecovered failures else `WorkflowFailed` (always-pattern
tasks may have run after a failure · the verdict stands · spec 05).
`outputs:` resolve from the records before the normal terminal frame
(an unresolvable output is omitted). A typed-output mismatch can make that
terminal fail under NIKA-VAR-009. The resolved map is recorded beside its
settlement through `nika-runtime-laws::secret::output_fields`: at most 64 KiB
of whole JSON, otherwise an exact byte count or a whole-map withheld marker.
The secret sink withholds any map whose keys or values need scrubbing; the
projection does not promote a failed run or change its ledger.

## 4 · Errors (NIKA-1700 range · Category::Runtime)

| code | when |
|---|---|
| NIKA-1700 | dirty CheckReport handed to run (audit-before-run violated) |
| NIKA-1701 | wave index out of bounds (checker/runtime contract breach) |
| NIKA-1702 | unresolved `${{ }}` reference (silent-literal guard) |
| NIKA-1703 | expression outside the v0 subset (when/render forms) |
| NIKA-1707 | report's boundary lanes ≠ workflow bytes (run-start re-derivation of the pure permits-fit + trifecta subset · the fail-closed backstop for library embedders — a clean report over different bytes is not clean) |
| NIKA-1708 | a `required: true` input reached `run` with neither `default:` nor `--var` (the admission preflight · issue #603 — refuses BEFORE the prologue, zero events zero spend; the CLI gauntlet speaks the same constructor) |

NIKA-1700/1701/1707/1708 abort the RUN. NIKA-1702/1703 inside a task pipeline
fail THE TASK (cascade · the detail carries the code) — a
**system** surface (a corrupt schedule · a report that does not match
the bytes) or a LAUNCH refusal (the unsatisfied `required: true` input)
aborts the run. Verb failures
are `TaskFailed` events carrying the verb's own `nika_code()` wire
form; the timeout class surfaces the SPEC code `NIKA-TIMEOUT-001`.

## 5 · Tests (the floor + the v2 battery)

1. **Conformance floor** (v1 · kept) · diamond fixture byte-stable
   storyboard · cascade (now `TaskCancelled` per 3.2 · the cli e2e
   updates in lockstep) · gates · agent lane · 24-deep / 12-wide.
2. **Cap-equivalence** · same workflow · `wave_parallelism` 1 vs 8 →
   byte-identical event streams (the determinism theorem made a test).
3. **True-concurrency proof** · two same-wave tasks that each await
   the other's start signal (mock handshake) — completes under cap ≥ 2
   · would deadlock sequentially (timeout-guarded).
4. **Drain** · sibling failure mid-wave never cancels an in-flight
   task (spec 05).
5. **Gate matrix** · default-gate cancel cascade · always-pattern
   (`when: true` over a failed dep RUNS) · `when:` false → skipped ·
   eval-error → task failure.
6. **Records** · status/error/duration refs in `when:` + render ·
   defined-null diamond join · skipped→null output.
7. **Retry** · transient×N→success (attempt counts · `TaskRetrying`
   delay fields) · non-transient requires `on_codes` admission ·
   resolved fetch effect veto (including Unicode method normalization) ·
   `max_attempts` strict · backoff table (fixed/linear/exponential ·
   jitter bounds · property: delay ≤ cap forever).
8. **Timeout** · hanging verb killed at budget (`NIKA-TIMEOUT-001` ·
   catchable by `on_error:` · never retried) · fast verb unaffected.
9. **`on_error`** · recover (downstream sees success + value) · skip
   (status skipped + error readable) · filter fall-through.
10. **`for_each`** · literal + upstream-array collections ·
    `max_parallel: 1` ordering · `fail_fast` both ways ·
    null-at-index · empty→skipped · `item`/`index`/`with` per
    iteration · non-array loud.
11. **unwind cleanup** (`after: {x: unwind}`) · runs on success AND
    failure · errors swallowed · parent status visible to cleanup
    `when:` · never-started runs none.
12. **Properties** (proptest) · random DAG schedules: replay
    determinism (run twice ≡) · settle-exactly-once · event
    arithmetic · cap-equivalence over random caps.
13. **Mutation** · `cargo mutants -p nika-runtime` · 0 missed.
14. **Agent telemetry** (`tests/agent_telemetry.rs` · ADR-096) · an
    `agent:` task through the REAL runtime puts its decisions on the
    canonical stream: per-turn `agent_tools_selected` (offered ·
    universe · per-source counts) · `tool_invoked` per dispatched tool
    (the agent path's ONE emission site · INV-024) ·
    `agent_budget_checkpoint` per turn — each task-stamped, ordered
    inside the task's lifecycle bracket; a stalled agent puts
    `agent_nudge` (reason) + `agent_stalled` (period · repeats) on the
    stream and `NIKA-467` on the `TaskFailed` frame. Topology: the
    dispatch pass stays pen-free — decisions are BUFFERED per dispatch
    (`agent_events::BufferingObserver` → `AgentVerb::run_observed` ·
    per-dispatch because a wave dispatches concurrently and a verb-wide
    observer would interleave tasks' streams), ride `Dispatched` →
    `RanTask` across attempts, and the settle pass emits them between
    the retry frames and the terminal frame. Review fold (2-lens audit
    on the wiring): the buffer is OWNED BY `attempt_loop`, OUTSIDE the
    timeout-cancellable region — a timed-out attempt's pre-timeout
    decisions (routing · budget) SURVIVE the drop and reach the stream
    with the `NIKA-TIMEOUT-001` frame (F1 · the timed-out-agent test
    pins it); every emitted event carries `attempt` (and `iteration` on
    fan-out lanes) so a retried agent and a 2-iteration fan-out are
    distinguishable in the flat stream (F3 · joins the `TaskRetrying`
    frames' counter); cleanup mini-tasks dispatch with a throwaway
    buffer (best-effort lane · collecting it is a trigger-gated
    ratchet).

## 6 · Non-goals (v0.1 · tracked)

Full CEL (the 03-dag milestone · replaces `expr` behind the same
seam) · `output:` jq bindings (ONE jq engine law — jaq lives in
nika-builtin · WIP · the record model already reserves the field
space) · `env.*` / `secrets.*` namespaces (envelope features ·
checker-validated · loud 1702 here) · eager (non-wave) dispatch ·
operator cancellation (Ctrl+C · daemon milestone · `TaskCancelled`
/ `WorkflowCancelled` reserved) · checkpoints/resume · streaming
frames (`InferChunk`) · `CostIncurred` (consumer-signal gated) ·
recover-await (3.6) · USL-fitted auto caps (Gunther arXiv:0808.1431).

## 7 · Dependencies

`nika-types` · `nika-error` · `nika-event` · `nika-schema` ·
`nika-kernel` (hub · ClockDyn) · the four `nika-verb-*` ·
`futures-util` (default-features off · std). Dev · `nika-kernel-mock`
(MockClock · seams) · `nika-providers` (mock/echo) · insta · proptest
· tokio (test rt).

## 8 · Research base (the citation law)

Graham 1969 (SIAM J. Appl. Math 17(2)) · Topcuoglu et al. 2002 (HEFT ·
TPDS 13(3) · explicitly NOT implemented · needs duration estimates) ·
Nelson & Tantawi 1988 (IEEE TC 37(6) · barrier cost) · Buttari et al.
2009 (arXiv:0709.1272) · Blelloch et al. 2012 (PPoPP · deterministic
reservations) · Thomson et al. 2012 (Calvin · SIGMOD) · Kahn 1974
(IFIP) · Ouyang et al. 2007 (SCP 67 · BPEL DPE) · Russell et al. 2006
(CAiSE · exception patterns) · Brooker 2015 (AWS · full jitter) ·
Bender et al. 2019 (JACM · saturating backoff) · Bronson et al. 2021
(HotOS · metastable retries — why attempts stay finite + certified) ·
Garcia-Molina & Salem 1987 (Sagas · cleanup canon) · Gunther
arXiv:0808.1431 (USL · future cap fitting) · Schroeder et al. 2006
(NSDI · closed-loop caps) · Temporal/Restate/Azure-DF/SFN/Flyte docs
(determinism + retry + timeout layering cross-engine canon).

## Configured production composition

`RuntimeConfig` remains the shared execution configuration owner. The existing
`production_runtime` and `service_runtime` doors retain their defaults; the
shared `compose::production_runtime_with_emitter` seam also accepts a live,
host-bound config for the admitted service-execution driver. Its service
projection uses `StderrEmitter::metadata_only`; all paths share the same
HTTP, registry, sandbox, and execution composition. The two duplicate configured
wrappers proposed during S81 are replaced by this existing seam before release.

The provider, image and TTS key readers all reuse `ladder_key` at the same
Runtime environment boundary. Existing variable names, precedence, empty-value
handling, cloud endpoint overrides and local URL normalization are unchanged.
The pending cost review contract lives in provider admission; Runtime preserves
the narrow `cost_choice` compatibility path and the configuration binding test.

A host account that observes declared-free routes only
(`observes_declared_free_only`) keeps the normal provider client for the
registry and gets a separate single-attempt client through
`with_inference_admission_http`; any other account keeps the all-bounded
client. Whenever a host account is attached, `run` stamps its receipt's durable
`nika/inference-cost-observation@2` projection as the JSON text field
`inference_admission` on the terminal frame
(`cost_choice::ObservedSink`, inside the secret scrub). A scoped receipt says
`scoped_to_declared_free`: its subtotal is never the whole Run's. A run killed
before its terminal frame leaves no receipt; that lifecycle stays open. An
unreadable account or an observation the provider-owned projection cannot read
is recorded as unreadable, never omitted or passed through with its endpoint.

Task terminal frames write their per-dispatch evidence through the provider
route-identity owner (E32). `inference_calls` is the JSON text of
`nika_providers::durable_calls` over the task's call records. `pricing_route`,
present when every call shares one pricing text, is
`nika_providers::durable_pricing` of it; the field is omitted when that pricing
is withheld whole, and the call elements say why. Neither holds an endpoint
path, query or userinfo. The run ledger's attribution key for an inference call
is `nika_providers::route_label`, `{provider}/{model} @ {origin}`, so the
terminal `cost_by_source`, the settlement's `spend.by_source` and the
`nika:inspect` cost view name origins. Routes of one origin sum under one key:
the key is presentation, each call is still debited by its own known estimate,
and the totals and counters are unchanged. The exact endpoints stay in memory
for pricing and identity. The terminal `inference_admission` projection does
not change that account, its counters or its authority. `workflow_started`
carries no route-identity declaration; legacy journals and the Session entries
recorded before its projection are separate migrations.

`resolve_model_expr` (C4 · 2026-09-28; its body descended to
`nika-check-analyzer`'s `rendered` module in B9, re-exported here at the same
path) exports the run-start cap gate's own
resolved-id walk: a literal, a concatenation, an operator `--var` over the
declared default, const, or a task's `with:` alias. A host judges a `model:`
expression before any effect at the value this resolver gives it; `None` means
only the run decides it (an upstream output, CEL beyond the walk). Dispatch
still renders every `model:` through the `${{ }}` seam, and the provider
registry judges that rendered route under a host-bound Run observer.

B9 (2026-09-28) closes the rendered-route bypass at the launch gates, in the
order trust · required inputs · budget floor · MODELS rung · access plan. The
floor and the MODELS gate judge one effective workflow: the operator's
`--model` in the envelope (a task's own `model:` keeps winning), then every
`model:` the bindings (`--var`, `--inputs-json`), a declared default, a const
or a `with:` alias decide, rendered to its literal by the analyzer's
`rendered_models`. A rendered paid route therefore prices exactly as its
literal twin (NIKA-1709 before the prologue). Since B11 the same effective
workflow also binds fan-out collections (the analyzer's
`rendered_collections`): a `for_each` over an input the invocation binds is
counted from the bound value, as the run binds it (a bound value replaces the
declared default and never falls back to it; a bound non-array is an unknown
count). The MODELS gate applies the
checker's own laws (the resolver's refusal, `thinking_findings`,
`capacity_findings`) to every such seat, literal or rendered, and refuses with
`ReportMismatch` (NIKA-1707) naming the findings. This also closes the
embedder door: `CheckReport::is_clean` leaves the MODELS rung to the CLI, so
a library host used to run a literal reasoning seat under its 256-token floor.
A refusal emits no event and runs no task. A `model:` only the run decides is
not priced at launch; `unbounded_breakdown` names it « decided at run time »,
never « unpriced ». The static reads these gates price with, the priced
builtin floor and the unpriced-cloud class (`priced_builtin_floor`,
`unpriced_cloud_seat`), descended verbatim to the analyzer's `builtin_floor`
module in B9 phase C; the refusals and their wording stay here.

B9 phase C adds a pre-send guard at dispatch for the seats launch cannot
judge: an infer/agent task whose own `model:` the pre-effect resolver leaves
unresolved (a task output, an answer, an item). After the seat renders and
before any provider or harness request, `admit::run_decided_refusal` judges
the task as ONE call in the effective workflow (the envelope and the
operator's `--model` kept, the rendered seat, its `for_each` cleared and its
gate open, every other field kept) with the launch gates' own laws. The
MODELS rung refuses with the code of the failure it prevents (NIKA-INFER-004
for the reasoning seat's cap floor, else NIKA-INFER-001). Under a cap, an
unpriced cloud seat off a harness, or a one-call floor above the ledger's
remaining USD, refuses NIKA-1704. The refusal is a task failure: non-
transient, never replayed (even under `on_codes:`), no ledger debit, no
provider attempt; the effects of tasks that ran before it stand. The remaining
USD is a snapshot at call time, the cap minus KNOWN spend: unknown charges
make it an upper bound, so a refusal is certain and a pass proves no fit.
Siblings started together each read the same snapshot and may cross together
(the ledger's wave-boundary NIKA-1704 and the provider admission keep their
roles); nothing here is a reservation or a hard cap. A child run inherits the
parent's remaining (law 6) and meets the same gates and guard against its own
ledger. Internal retries (schema re-asks, provider re-sends) are judged once
and counted on the ledger as they happen. The `unwind` cleanup lane meets the
same guard, ledger and child budget (§3.8 · B11).
