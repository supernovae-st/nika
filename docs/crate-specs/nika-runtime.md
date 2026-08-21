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
  unbounded) · settle in input order (same ordered-settlement
  pattern) · `retry:`/`timeout:`/`on_error:` apply per iteration.
- `fail_fast: true` (default) · first settled error drops the
  remaining stream (in-flight cancelled · unspawned never start) ·
  `false` · all iterations run · failed slots contribute `null` at
  their index (positional alignment survives · spec §null-at-index).
- Task output = the array of per-iteration outputs in input order ·
  task status = failure if ANY iteration failed unrecovered.
- Events: ONE task-level Started/Completed/Failed pair (iterations are
  internal · the note carries `for_each · N items`) — the event
  grammar has no per-iteration id space at v0.1.

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
`outputs:` resolve after the terminal event from the records (an
unresolvable output is omitted · the verdict unchanged).

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
   delay fields) · non-transient never retries · `on_codes` filter ·
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
