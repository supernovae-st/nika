# 13 · Outcomes

> Every task settles in exactly one of **four terminal classes** —
> that much has been law since the graph chapter. What was missing is
> the **why**: a timeout, an exhausted retry budget, a recovered
> failure and a plain success all looked alike one level down. The
> Outcome IR names the second axis: `Outcome = TerminalClass × Cause ×
> Payload` — **gates stay simple on the CLASS, precision lives on the
> CAUSE, and no ad-hoc flag ever grows again**.
>
> One table is normative, machine-readable in `canon.yaml`
> (`outcome_transitions`), and consumed by the runtime, the trace, and
> the assertion layer — the SAME table, parity-tested, never three
> private copies.

---

## The Outcome IR (normative)

```
Outcome = (class, cause, payload)
```

- **`class`** — the four terminal classes, unchanged from
  [03](./03-dag.md): `success · failure · skipped · cancelled`.
  **Every edge predicate and gate reads the class alone** (the W2
  pass-sets are untouched — this chapter adds no admission power).
- **`cause`** — the closed second axis (below). A cause never
  influences an edge: two tasks settling `failure(timeout)` and
  `failure(retry_exhausted)` open exactly the same edges.
- **`payload`** — what the record carries, by class (below).

## The transition table (normative · the ONE source is `canon.yaml`)

| class | legal causes | meaning |
|---|---|---|
| `success` | `normal` | the verb completed |
| `success` | `recovered` | the task failed, its `on_error: recover` chain settled it — the record keeps `recovered_from` (the original error) |
| `failure` | `verb_error` | the verb refused/errored and no retry remained (attempts = 1 when no `retry:`) |
| `failure` | `timeout` | the task's `timeout:` budget elapsed ([03](./03-dag.md) · `NIKA-TIMEOUT-001`) |
| `failure` | `retry_exhausted` | every attempt failed and the policy admitted no more |
| `skipped` | `gate` | the `when:` gate (or the default gate's decision) evaluated false — a decision, not a defect; `.error` reads defined-`null` |
| `skipped` | `error_skip` | `on_error: skip` fired — the PRESERVED error rides `.error` ([03 §fields](./03-dag.md)) |
| `cancelled` | `upstream` | the default gate became unsatisfiable (an upstream failure/cancellation propagated) |
| `cancelled` | `operator` | the run was cancelled from outside (user signal · `NIKA-CANCEL-001`) |
| `cancelled` | `budget` | the run crossed `--max-cost-usd`: this task was UNSTARTED when the cap hit (in-flight work completes and is counted · [05](./05-errors.md) `NIKA-RUN-1704`) |

**Any (class, cause) pair outside this table is an engine bug, never a
state** — the reference evaluator refuses it, the engine's own table
is parity-tested against the canon's, and the trace validator (the
proof wave) will refuse a trace that carries one.

## Payload (normative · by class)

| class | payload |
|---|---|
| `success` | `value` · `attempts` · `recovered_from?` (the original error when cause = `recovered`) |
| `failure` | `error` (code · message · transient) · `attempts` |
| `skipped` | `error?` (present iff cause = `error_skip` — the preserved error; defined-`null` under `gate`) |
| `cancelled` | `reason` (the cause, spelled) |

`attempts` counts every attempt including the settling one. No other
field exists — a new fact about a settle is a new CAUSE row (one
table), never a boolean beside the record.

## `trace_format: 2` (normative)

The cause axis changes what a trace line MEANS: a format-1 consumer
ignoring an additive `cause` field would read `failure(timeout)` and
`failure(retry_exhausted)` as the same old failure — semantic
incompatibility under readable JSON, the
[`graph_format: 2` precedent](./03-dag.md). So the trace format bumps:

- format-2 lines carry `outcome: {class, cause}` (+ the payload per
  class) on every terminal task event;
- format-1 stays what it was — a photograph of the pre-W5 world;
  engines do not emit it past this chapter;
- machine formats version independently — **the language stays
  `nika: v1`**, forever.

## One obvious way (normative for linters)

- Branching on *why* something failed reads the record's `cause`
  (`${{ tasks.x.cause }}` is a terminal-observation field, same
  pass-set as `.status` · [03 §fields](./03-dag.md)) — never a string
  match on an error message.
- A gate that only needs *whether* reads `.status` / the `after:`
  predicates, as always — the class is the boundary, the cause is the
  detail.

## What v1 deliberately does not do

- **No new gate predicates.** `after:` keeps its four (succeeded ·
  failed · skipped · terminal) — cause-level admission would couple
  scheduling to diagnostics (the separation this chapter exists to
  protect).
- **No cause taxonomy growth without a wave.** The set is closed; a
  new cause is a spec amendment with its transition row, its payload
  law, and its parity fixture.
- **No resource algebra, no `stop:` sugar, no remote-agent lifecycle.**
  Owned by their own windows (the receipt names them); this chapter is
  the outcome NOYAU the rest consumes.

## Related

- [03 · DAG](./03-dag.md) — the four classes · the pass-sets this
  chapter leaves untouched
- [05 · Errors](./05-errors.md) — the error registry the failure
  payload carries
- [11 · Decision](./11-decision.md) — the receipt discipline
  (determination provenance) the outcome record follows
