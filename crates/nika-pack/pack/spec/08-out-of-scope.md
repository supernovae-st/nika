# 08 · Out of scope for v0.1

> The 5 pillars are immutable forever. The stdlib evolves. But the
> following items are **deliberately deferred** from the v0.1 spec ·
> they will land in later spec versions OR stay forever out of scope.
>
> Explicit deferral is canon · prevents drift · prevents premature lock-in.

---

## Why explicit defer matters

Every workflow language faces pressure to grow features. Without an
explicit defer list · the spec accretes accidental complexity · the
"forever" commitment becomes "until next quarter".

This document is the canonical defer list. If you find yourself missing
a feature here · it is intentional · not a forgotten gap. Most of these
items are tracked in design discussions and may land in `nika.sh/v2` or
in stdlib v0.x extensions.

---

## Antivalues · what this language is AGAINST, permanently

A defer list is not enough. « Later OR never » is itself accretion
pressure — every deferred item reads as a promise to someone. The list
below is the other kind: **things nika will not do, at any version.** If
you need one of them, nika is the wrong tool and the honest answer is to
say so. (Prior art for publishing this at all: WDL's `Antivalues` section
— the only workflow language that names what it refuses.)

**A1 · Flexibility at all costs.** nika expresses the majority of
workflows, not all of them. Arbitrary execution patterns are out. If you
want them, use a language embedded in a general-purpose host — Nextflow
or Snakemake are good, and they say so themselves.

**A2 · A second way to spell one intent.** Every construct has one
spelling. Argo ships `steps:` and `dag:` for the same thing, and inside
`dag:` ships both `dependencies:` and `depends:` — three ways to say one
thing. The `one-obvious-way/*` lint set exists so that never happens
here. *(Design bet, stated as one: PEP 20's slogan has no empirical
support anywhere we could find.)*

**A3 · Ambient state.** A body reads its declared imports and nothing
else. No implicit environment, no global task namespace, no inherited
scope. This is the [hidden dependencies](https://www.cl.cam.ac.uk/~afb21/CognitiveDimensions/)
dimension of Green's Cognitive Dimensions of Notations (1989/1996) —
« is every dependency overtly indicated *in both directions*? » — and it
is why the graph document lists every edge with `from` and `to`.

**A4 · An unversioned expression layer.** Both expression sublanguages
carry a normative, versioned grammar with a closed callable set
(`cel-subset/0.1` · `jq-subset/0.1`). CWL feature-gates its JavaScript
hatch but does not bound it — and portability breaks exactly there.
GitHub Actions documents its `${{ }}`-into-shell hazard and ships the
canonical injection class anyway. Nextflow is amputating Groovy right
now (strict parser default in 26.04, `for`/`while`/`switch`/`import`
removed) at the cost of a multi-year migration. A hatch you cannot
version is a hatch you cannot keep.

**A5 · Orchestration.** No scheduler, no queue, no cluster. `nika.yaml`
arms a cadence; it does not become a control plane.

**A6 · More than one thread of control per node.** A task settles
exactly once. This permanently refuses the discriminator family
(WCP-9, WCP-28–32 — six members), Multi-Merge (WCP-8), and N-of-M
joins. It is not a
gap to patch — it is the property that makes
[soundness conditions (i) and (ii)](./03-dag.md#what-that-liveness-check-is--soundness-and-what-it-costs-elsewhere)
structural rather than checked.

**A7 · Cancellation of an arbitrary region.** Reset arcs make every
variant of soundness undecidable (LICS 2024). This closes YAWL-style
cancellation regions, BPMN cancel/compensate events, and the general
OR-join — whose accepted formalization *is* a reset net.

### The counter-arguments, stated before a reviewer states them

Honesty costs less than being caught. The strongest published positions
against this design ·

- **The configuration complexity clock** (Hadlow, 2012): config grows a
  rules engine, grows a DSL, and you have rebuilt a bad programming
  language. Folklore, zero data — but it is *the* objection.
- **Snakemake** (Mölder et al., F1000Research 10:33) states it from
  inside the field: purely declarative systems get « concision and
  clarity … [but] can be **more restrictive in the processes that can be
  expressed** ». Their `checkpoint` — re-plan the DAG mid-run — is the
  strongest existing counter to A6/A7.
- **Closedness does not buy comprehensibility**, only analysability. A
  large Starlark codebase is still hard to read.
- **Oozie's rigid XML lost to Airflow's Python.** Being closed is not
  sufficient.

**The rebuttal, in one line:** nika's escape hatches are *declared floors,
not ambient capability* — `exec:` (lint-discouraged, `native-first/*`),
`agent:` (budget-bounded, cap in the file), `invoke: workflow:`
(authority intersected). And the measurement now runs the other way:
custom shell commands carry **4.12× the odds of failure**, environment
variables 1.84×, and **each additional language feature used adds ~19%
to the odds of failure** (Bardsiri, Decan, Mens, arXiv:2605.26825,
2026). Attribution matters and the first version of this paragraph got
it wrong: those odds ratios are regressed over **13,915 workflows across
six months**, not over the paper's larger 260K/six-year usage corpus —
the Gini 0.84 and the 74.6%-of-constructs-at-7.1%-of-use figures come
from the usage study, the odds ratios from the reliability study. Two
populations, one paper.

### The same metric, turned on ourselves

A citation that can only ever agree with us is decoration. So the
authors' own metric was run against **this** language, over 669 files
and 2071 authored tasks ·

| | GitHub Actions | nika |
|---|---|---|
| constructs measured | 197 | **41** |
| usage Gini | 0.840 | **0.700** |
| constructs under 1% of use | 147 (74.6%) | **16** |
| never written | — | **2** |

Read it honestly. Our distribution is **less** concentrated (0.700 vs
0.840), but **we have a long tail too** — 16 constructs sit under 1% of
use, and 2 have never been written. The argument that survives is
therefore **not** « nika has no long tail ». It is the surface size:
**41 constructs against 197.** With ~19% added odds of failure per
feature *used*, the ceiling on what an author CAN reach is the
protection — and that ceiling is what this document defends.

Two limits of that measurement, stated rather than hidden: the corpus is
our own (a first-party corpus is not a population sample), and the
envelope-key count picks up keys from files still written in an older
dialect, so the tail is if anything **understated**.

---

## Workflow composition · the CALL half SHIPPED ([14](./14-composition.md)) · the TEXTUAL half deferred

### Multi-file workflows (`include:` · `import:`)

```yaml
# NOT supported in v0.1
import:
  - ./shared/auth.nika.yaml
tasks: ...
```

**Why deferred** · single-file workflows are simpler · linter/LSP works without resolving paths · examples are self-contained. Textual import (merging FILES into one workflow) may land in a later minor with an explicit import grammar · it is a different mechanism from CALLING a child workflow, which shipped (next section).

### Sub-workflow invocation · lifted by [14-composition](./14-composition.md) (`invoke: workflow:`)

> **Shipped** · a parent calls a child as a typed, bounded,
> authority-contained step. The form is the tagged union on the verb you
> already know · `invoke:` carries **exactly one** of `tool:` or `workflow:`
> ([14 · Composition](./14-composition.md) · `NIKA-COMP-001..004` ·
> reference engine v0.105+).

```yaml
sub:
    invoke:
      workflow: ./subroutine.nika.yaml   # a STATIC target · never templated (NIKA-COMP-001)
      args:
        topic: "${{ inputs.topic }}"     # each key must fit a DECLARED child input
    returns: { object: { summary: string } }
```

The once-proposed **`nika:run` builtin is ABANDONED, not deferred** · a
builtin would have carried the child as a string argument — invisible to the
type system, the effect boundary and the acyclicity check. The field IS the
semantics (the same command/shell law generalized), so composition shipped
as the tagged union instead. Per the
[02-verbs closure argument](./02-verbs.md#the-closure-argument--why-no-case-forces-a-5th-verb)
a sub-workflow call stays the dispatch-and-await model · never a verb — that
half of the original design survived unchanged.

**The sibling-run workaround stays valid** · `exec: shell: "nika run
subroutine.yaml --output json"` launches a sibling workflow PROCESS ·
`--output json` (engine CLI) prints the child's typed `outputs:` as JSON on
stdout · the parent binds it back with `capture: stdout` + a jq `extract:`
(the typed contract survives the process boundary). It remains floor 1 below
and the one form for what a static call graph cannot express (an unbounded
cursor chain · H23) · wherever the target is static, a linter recommends the
native form ([14 §workaround](./14-composition.md)).

**Recursion guard (normative · now carried by [14](./14-composition.md))** ·
the rails this section held while composition was deferred are laws of the
composition chapter ·

- **Cycle detection** · the static call graph MUST be acyclic — a workflow
  file (transitively) calling itself is refused **at check**
  (`NIKA-COMP-003` · law 7): a self-launching workflow is a fork bomb, not a
  recursion scheme.
- **Depth cap** · nested run depth above the engine's limit fails the
  *launching task* fail-closed (`NIKA-SEC-003` · the reference engine
  defaults to a small single-digit depth), never the host process — the
  runtime backstop for what static analysis cannot see (the workaround's
  sibling processes · acyclic-but-deep chains).

### Macros / templates

```yaml
# NOT supported in v0.1
macro retry_with_backoff:
  retry: { max_attempts: 5, backoff_ms: 1000 }

task:
    use_macro: retry_with_backoff
    invoke: { tool: "nika:fetch", ... }
```

**Why deferred** · macros are a powerful but dangerous primitive (expansion order · debugging · drift). Most needs are met by `with:` scope + structured retry. Revisit only if empirical demand emerges.

---

## Control flow · DEFERRED

> **Note** · bounded map iteration (`for_each:`) is **IN v1**: it ships as a
> task field, see [03-dag.md](./03-dag.md#for_each--optional--map-a-task-over-a-collection).
> What remains deferred is **unbounded** iteration (`while:`).

### Unbounded loops (`while:`)

```yaml
# NOT supported in v1 — unbounded iteration
poll_until:
    while: ${{ tasks.check.output.ready == false }}
    exec:
      command: ["./check.sh"]
```

**Not deferred — REFUSED, permanently.** This section previously said
« why deferred », which contradicted two other places in this spec that
call the same construct a structural refusal. One statement, one home:
**the status of `while:` is decided in
[03 §where this sits](./03-dag.md#where-this-sits--deliberately-between-two-named-dataflow-models),
and it is `A1` + `A6` here.** Everywhere else points at those.

The reason is not that loops are hard to analyse. It is that a
data-dependent branch whose decision **re-enters the graph** is exactly
the SDF→BDF line: Boolean dataflow is Turing-complete, and bounded-memory
scheduling plus deadlock-freedom become **undecidable** (Buck & Lee 1993 ·
Lee & Parks, Proc. IEEE 83(5), 1995). Nothing about a future version
changes that; a `while:` that stayed decidable would not be a `while:`.

Bounded iteration already exists and is not a workaround: `for_each:`
over a runtime-computed collection (WCP-14), or `agent:` with a
`max_turns` budget written in the file.

### Goto / jumps

Never supported. Anti-pattern.

### Try / catch blocks

`on_error:` is the v0.1 mechanism. Block-scoped try/catch is deferred.

---

## Streaming · DEFERRED

### Streaming output binding

```yaml
# NOT supported in v0.1
stream_chat:
    infer:
      prompt: "..."
      stream: true
    extract:
      chunks: .stream                 # not in v0.1
```

**Why deferred** · streaming semantics in a YAML/DAG model are tricky · tasks are nominally synchronous. Engines MAY stream internally as an implementation detail · workflows see the final assembled response.

### Pub/sub / event-driven workflows

```yaml
# NOT supported in v0.1
listen:
    subscribe: "events://orders.created"
```

**Why deferred** · v0.1 is finite-DAG. Long-running event listeners belong in the daemon layer or external orchestrators.

---

## Persistence · DEFERRED

### Scheduled execution (cron)

```yaml
# NOT supported in v0.1
schedule: "0 */6 * * *"             # every 6 hours
tasks: ...
```

**Why deferred** · scheduling is an engine-runtime concern · not a language concern. A conformant engine handles cron at runtime · workflows themselves stay schedule-agnostic.

### Trace retention · lifted by ADR-100

> `.nika/traces/` is bounded by default (keep-last-N · age cap · size
> budget) with two absolute exemptions — a `paused` trace (a pending human
> gate) and each workflow's newest trace (the standing resume candidate) —
> and a one-line visible report whenever collection removes anything ·
> [ADR-100](../adr/adr-100-trace-retention.md). The CLI surface (`nika trace
> ls|rm`) and config wiring landed with the engine arc.

### Workflow checkpointing / resumption

```yaml
# NOT supported in v0.1
checkpoint:
  after: [task_a, task_b]
  storage: sqlite://./checkpoints.db
```

**Why deferred** · persistence is an engine-runtime concern. A conformant engine handles this internally · workflows don't declare it.

> **Lifted at the durable-lite tier · [ADR-099](../adr/adr-099-durable-lite-run-resume.md) (accepted · 2026-07-05 · shipped)** ·
> `nika run --resume <trace>` re-executes from the run's own NDJSON trace
> (content-addressed per-task skip · every skip a **visible** `task.cache_hit`
> event · `--from <task_id>` override) — the trace IS the checkpoint · CLI +
> trace format only. The `checkpoint:`-block sketch above **REMAINS deferred**
> exactly as written: a workflow never declares persistence · the language
> surface is unchanged.

### Workflow versioning / migration

A workflow file carries no version pin: `nika:` is the file's name, the
language family is v1 forever and pre-1.0 breaking changes land INSIDE it,
announced in the changelog with their `check --fix` rungs ([01 §nika](./01-envelope.md) ·
[00 §pre-1.0 stability contract](./00-overview.md#pre-10-stability-contract)). Per-workflow stdlib version pinning may land in v0.2.

### Idempotency keys / step deduplication

```yaml
# NOT supported in v0.1
charge:
    idempotency_key: "${{ inputs.order_id }}"   # dedup retried side effects
    invoke: { tool: "mcp:stripe/charge", args: { ... } }
```

**Why deferred** · idempotency keys are **table-stakes for the durable-execution
class** (Temporal · Inngest · Restate · at-least-once delivery systems), but
**not** for a finite single-run DAG (GitHub Actions · Argo single-run have no
idempotency keys either). They earn their keep alongside **durable execution +
resumption** (retries that survive a process restart), which is itself deferred
(engine/daemon · v0.3). A v0.1 `retry:` re-runs in-process · a side-effecting
task handles its own dedup via its tool args. Idempotency lands in v0.2
together with the durable-execution waypoint · NOT before. *(2025-2026 SOTA
gap-check · the only near-universal capability v1 lacks · and it is correctly
durable-class, not finite-DAG-class.)* *(The resumption half now has a
durable-lite design · [ADR-099](../adr/adr-099-durable-lite-run-resume.md) ·
resume shrinks the re-run window · at-least-once dedup stays THIS section's
deferral, at the full waypoint.)*

---

## The Connectome · DEFERRED capability · NOT a deferred verb

### Cognitive recall in workflows

```yaml
# The CAPABILITY is deferred (engine waypoint) · the SHAPE is already decided
recall:
    invoke:
      tool: "nika:connectome/recall"
      args:
        query: "Previous conversations about · ${{ inputs.topic }}"
        top_k: 5
```

**No 6th verb, ever.** When the Connectome (the engine's cognitive subsystem)
ships, recall and ingest are exposed as **builtin tools under
`invoke:`** (`nika:connectome/recall` · `nika:connectome/ingest`), NOT as a
new verb. The 4 verbs are absolute: a new verb would
require a `nika: v2` language contract (a frozen-forever envelope), so that is
effectively never. So
the *shape* of cognitive access is already final today; only the *capability*
waits on the engine.

For v1 today · workflows that need recall use `invoke: mcp:memory-server/recall`
with an external MCP memory server, then swap the tool id to
`nika:connectome/recall` when the native Connectome admits: zero workflow-shape
change (same `invoke:` verb).

---

## Advanced agent features · DEFERRED

### Multi-agent coordination (supervisor / worker)

```yaml
# NOT supported in v0.1
agents:
  supervisor:
    role: "Plan and delegate"
  workers:
    type: pool
    count: 3
    role: "Execute tasks"
```

**Why deferred** · multi-agent topology is hard to standardize. The v0.1 `agent:` verb is single-loop. Patterns like supervisor/worker can be expressed in v0.1 via multiple `agent:` tasks with explicit hand-off via `with:` scope.

### Persistent agent state across workflow runs

```yaml
# NOT supported in v0.1
ongoing:
    agent:
      persistent_state_id: "user-123"
```

**Why deferred** · ties into memory subsystem.

---

## Tooling extensibility · STDLIB v0.x

### 22 media builtins

`pdf_extract` · `svg_render` · `phash` · `compare` · `optimize` · `provenance` · `qr_validate` · `thumbhash` · `dominant_color` · `image_diff` · `import` · `thumbnail` · `provenance_verify` · `text_extract` · `ocr` · `transcribe` · `mediainfo` · `dimensions` · `colorize` · `palette_extract` · `qr_generate` · `barcode`

**Why deferred** · 22 builtins would nearly double the total builtin surface · they target a specific audience (media pipelines). Splitting them into stdlib v0.x keeps v0.1 scope lean without losing core capability.

Rows graduate one at a time, as counted stdlib builtins, when they pass the
admission razors: `chart` (§Media graduate #4 · 2026-07-09) and `tts`
(shipped as `tts_generate` · §Audio · 2026-07-05) already left this list —
see [canon.yaml](../canon.yaml) + [stdlib/builtins-v0.1.md](../stdlib/builtins-v0.1.md).
The rows above remain unshipped and don't count toward Stdlib v0.1 conformance.

### Pluggable provider adapters

```yaml
# NOT supported in v0.1
providers:
  custom_xyz:
    type: openai-compatible
    base_url: "https://custom.example.com/v1"
```

**Why deferred** · provider config belongs in engine config · not in workflow YAML. v0.2 may introduce a `provider_config:` envelope field for openai-compatible custom endpoints.

### MCP server dependency declaration (`requires_mcp:`)

```yaml
# NOT supported in v0.1
requires_mcp: [postgres, browser]    # engine pre-flight-validates these servers exist
tasks: ...
```

**Why deferred** · a workflow references MCP tools by `mcp:<server>/<tool>` · the
server set lives in **engine config** (like provider config · not workflow YAML).
A `requires_mcp:` manifest would let the engine *pre-flight* (« no `postgres`
server configured » *before* the run) and document intent, but it is a
convenience, **not a v0.1 incompleteness** · a missing server already fails with
a clear `NIKA-MCP` error at first call. Single-file v0.1 stays free of
external-resolution declarations (the same rationale that defers `import:`).
May land in v0.2 alongside the import grammar (the composition call form is already IN · [14](./14-composition.md)).

---

## Observability · DEFERRED

### OpenTelemetry export

Not in language spec. Engines MAY emit OpenTelemetry traces · this is engine concern · not workflow concern.

### Cost tracking annotations

```yaml
# NOT supported in v0.1
budget:
  max_cost_usd: 1.00
  warn_at_pct: 80
```

**Why deferred** · budget enforcement is engine concern · the `nika:inspect view: cost` introspection builtin (post-ADR-088 unification) gives workflows access to running cost. Hard caps may land in v0.2.

---

## Multi-workflow orchestration · NEVER (separate language)

Pipeline-of-workflows orchestration · cross-workflow dependencies · workflow registries · service mesh integration · ALL out of scope **forever**. Nika is a single-workflow language. Multi-workflow orchestration is a different problem (cf Temporal · Airflow · etc.).

If you need that · the reference engine's HTTP server exposes a per-workflow run API · build your orchestrator on top of it.

### The three floors (canonical framing)

« Never in the language » does NOT mean « impossible »: it means **not in
the YAML**. The ladder ·

| Floor | What | Where it lives | Status |
|---|---|---|---|
| **1 · launch a sibling run** | one workflow starts another, awaits its typed `outputs:` | `exec: nika run sub.yaml --output json` + `capture: stdout` (the documented workaround · §sub-workflow invocation above) | ✅ valid (the process floor) |
| **2 · composition** | a parent calls a child INSIDE a run · dispatch-and-await · one observable tree | the `invoke: workflow:` tagged union ([14-composition](./14-composition.md) · `NIKA-COMP-001..004` · static acyclic call graph · `NIKA-SEC-003` depth backstop) | ✅ shipped (spec 14 · reference engine v0.105+) |
| **3 · orchestration** | long-lived meshes · cross-run dependencies · registries · scheduling · cross-run retry | an orchestrator built ON TOP of the per-workflow run API · never a language construct | 🚫 forever out |

Why the line holds · the conformance contract ([07](./07-conformance.md))
stays implementable: a conformant engine executes ONE workflow; if
conformance demanded a durable state store, a scheduler and a registry, no
alternative engine could ever exist and « one YAML · any conformant engine »
dies. A language describes one run the way Make describes one build and a
shell script describes one execution: your CI and your cron live above
them, not inside them. **Composition IN the language (shipped ·
[14](./14-composition.md)) · orchestration ON TOP of the language (forever)
· never the inverse.**

---

## Horizon postures · the « did you think of X? » table (2026-06-10)

Every recurring 2026-class workflow-language question, answered in one row:
the posture is written, silence is forbidden. **IN** = normative in v0.1 ·
**PARTIAL** = a v0.1 piece exists, the rest is deferred · **OUT** = deferred
or a non-goal, with the line that says whose job it is.

| # | Horizon | Posture | The one-paragraph answer |
|---|---|---|---|
| H1 | Durable execution (checkpoint · resume · replay) | **OUT · durable-lite tier lifted by ADR-099** | A v0.1 run is a single OS process with in-memory state · crash = re-run from the top · `retry:` is in-process only. The **durable-lite tier** (crash-resume + re-run-from-a-node from the run's own trace · visible `task.cache_hit` · zero author-facing determinism constraints) is lifted by [ADR-099](../adr/adr-099-durable-lite-run-resume.md) · CLI + trace only. Idempotency keys + the full durable-execution waypoint (retries that survive a restart · at-least-once dedup) stay deferred (§Persistence · v0.2+) · until then durability beyond `--resume` is the **host's responsibility** (run under a supervisor · make side-effecting tools idempotent via their own args). |
| H2 | Streaming between tasks | **OUT** | Tasks are synchronous · a dependent reads the **final assembled value** (§Streaming). Engines MAY stream provider tokens internally/to the user. A between-task stream changes what an *edge* delivers: a future additive edge semantic, never a verb ([02 closure](./02-verbs.md#the-closure-argument--why-no-case-forces-a-5th-verb)). Pub/sub listeners · NEVER in the finite-DAG v1. |
| H3 | Multimodal artifacts (typed image/audio/video) | **PARTIAL** | v0.1 values are strings · JSON values · and **opaque bytes pass-through** (tool-determined · [02 §invoke output](./02-verbs.md#what--tasksidoutput--holds--per-verb) · `infer.vision` takes file/url refs). No typed media payloads · no content-addressing in the language. Both arrive with the deferred media builtins (stdlib v0.x · §Tooling extensibility). Addressing (e.g. blake3) is engine detail, not language surface. |
| H4 | Agent-of-agents (sub-agents · budget propagation · trust inheritance) | **PARTIAL** | The `agent:` verb is a **single loop** with per-task budgets (`max_turns` · `max_tokens_total` · normative · [02 §agent](./02-verbs.md)) and a default-deny tool whitelist. Budgets do NOT propagate across tasks (each declares its own). Multi-agent topology · §Advanced agent features (expressible today as multiple `agent:` tasks + explicit `with:` hand-off). Run-recursion is bounded by the composition laws (`NIKA-COMP-003` static acyclicity at check · `NIKA-SEC-003` depth backstop · [14](./14-composition.md)). |
| H5 | Human-in-the-loop (approval gates) | **IN** | `nika:prompt` (blocking confirm with `default:` for non-interactive mode) plus `nika:notify` (fire-and-forget). Pause-state is **live** (the run keeps a process while blocked) · **durable pause shipped with [ADR-099](../adr/adr-099-durable-lite-run-resume.md)'s `--resume`** (a non-interactive default-less blocked `nika:prompt` journals `workflow.paused` + exits cleanly · resume re-arms it) · time-bound it with the task-level `timeout:`. ONE construct · a tool under `invoke:` · forever. |
| H6 | Evals in-language (asserts · LLM-judge · golden runs) | **PARTIAL** | v0.1 ships the pieces · `schema:` (per-task structured-output gate · auto-retry) · `nika:assert` (fail-fast CEL guard) · `nika:validate` (JSON Schema over data). An LLM-judge is an `infer:` task with a verdict `schema:` — it produces cited FACTS; when the verdict must be a governed DECISION (thresholds · abstention · receipts), those facts feed `nika:decide` over a Decision Bundle ([11-decision](./11-decision.md) · the model never decides). Golden-run testing reuses the conformance fixture shape (input + expected output on `mock/`). Declarative in-file eval blocks · deferred. |
| H7 | Cost governance (budgets in €/tokens) | **PARTIAL** | Reading IS in-language · `nika:inspect view: cost` (running cost). Enforcement is NOT · hard caps are engine config (§Observability · `budget:` blocks deferred v0.2). The `agent:` budgets (`max_tokens_total`) are the one normative in-language cap today. |
| H8 | Model routing / fallback chains | **PARTIAL** | The language surface is ONE field · explicit `model:` per task (+ `inputs` parameterization: one workflow, any backend). Provider-side routing exists TODAY via `openrouter/…` (gateway fallback). Declarative capability-based routing in YAML ("any vision model under $X") · deferred · it must not create a second way to pick a model. |
| H9 | Memory / the Connectome | **IN (as posture)** | Recall is a TOOL, forever · `invoke: mcp:memory-server/recall` today · `nika:connectome/recall` + `/ingest` when the Connectome ships (§The Connectome) · the tool-reference grammar already reserves the `nika:connectome/*` group: the forward-compat seam exists NOW. Never a verb · never an ambient implicit memory. |
| H10 | Provenance (claim + evidence + confidence) | **PARTIAL** | The **evidence-first shape** (`{ claim, evidence: { step, path, quote }, confidence }`) is the RECOMMENDED (informative) output convention for audit-class workflows · fully expressible today via `schema:`. A normative, ProofChain-compatible machine run-report schema is deferred with the run-report work (v0.2). |
| H11 | Observability (OTel) | **OUT (mapping recommended)** | Engine concern (§Observability) · zero telemetry by default · local-first. The canonical mapping when an engine exports · **run = trace · task = span · retry attempts = span events**, recommended so two engines' traces line up · never required. |
| H12 | Security in-language vs runtime | **WRITTEN** | Language-visible security is now FOUR surfaces · the `exec:` blocklist (MUST · + the argv form as the structural injection fix) · the `agent:` tools whitelist (default-deny MUST) · secrets masking + no-inline-literals (`secrets:` envelope) · and the workflow-level **`permits:`** capability boundary ([01 §permits](./01-envelope.md#permits--optional--the-declared-capability-boundary) · default-deny once present · enforced statically + at runtime · NIKA-SEC-004). Everything else, trust levels · spotlighting · canaries · taint tracking (the reference engine's Shield · NIKA-SEC family), is **runtime-side** and intentionally NOT in YAML v0.1. |
| H13 | Packaging / distribution (registries · semver · signing) | **OUT** | The language has no versioning surface in the file (v1 forever · no slot · [01 §nika](./01-envelope.md)) · machine formats version independently. Workflow registries · package manifests · signing are ecosystem/engine concerns (the reference engine plans `nika-pck`) · not spec surface. |
| H14 | Testing workflows (dry-run · mocks · goldens) | **IN (pieces)** | The `mock/` provider is one of the <!-- canon:providers -->17<!-- /canon --> (deterministic · no LLM call): `model: mock/echo` per task or via one `inputs.model` swap turns any workflow into a test. Golden runs reuse the conformance fixture pattern. `--dry-run` (parse + plan · execute nothing) is an engine CLI concern · MAY. A workflow without a test story is a script: the test story is · swap to `mock/`, assert with `nika:assert`/`schema:`. |
| H15 | Concurrency / rate governance | **PARTIAL** | In-language · `for_each.max_parallel` + `for_each.fail_fast` + wave-parallel scheduling (normative · [03 §execution model](./03-dag.md#dag-execution-model)). Per-provider rate limits · engine config (the engine MUST honor declared caps · how it throttles providers is its business). Cross-run/global scheduling · NEVER (§Multi-workflow orchestration). |
| H16 | Interop (import/export GHA · LangGraph · Temporal) | **PROUD NON-GOAL** | Nika is a source language, not a compatibility layer: no importers/exporters, ever (a translated workflow is a worse workflow). **⚠️ AMENDED 2026-08-11 — the old wording said the interop boundaries are MCP and `exec:` « anything with a CLI ». That sentence manufactured the problem it pretended to solve: everything has a CLI, so it routed the world to the escape hatch.** The reach-for order is now a LADDER, and `exec:` is its last rung — see [§the interop ladder](#the-interop-ladder-normative) below. |
| H17 | Step caching / memoization (`cache_key:`) | **OUT (durable-adjacent)** | A v0.1 run executes every non-skipped task: no memoize field. The dominant cacheable class (deterministic `exec:`/`invoke:`) collides with side effects, and `infer:` is non-deterministic (a wrong default poisons correctness). Lands as an additive task field WITH the durable-execution waypoint (a run store exists then · H1). Until then · cache at the tool layer (`nika:read`/`nika:write` a content-keyed path · HTTP caches honor ETags). |
| H18 | Matrix strategy (cartesian product · include/exclude) | **PARTIAL (idiom · no field)** | `for_each` is one-dimensional by design: a matrix is **precomputed data, not control flow** · emit the product with `nika:jq` (`[ .os[] as $o | .ver[] as $v | {os:$o, ver:$v} ]`) · `for_each` over it (`item.os` · `item.ver`) · include/exclude = jq filters on the product. A `matrix:` sugar field would restate this in control-flow clothing · deferred unless empirical demand. |
| H19 | Value-conditioned retry / polling (« retry until the VALUE says done ») | **OUT (v0.2 `retry_when:` candidate)** | `retry:` fires on ERRORS · never on values. The v0.1 canonical polling pattern · make not-ready an error INSIDE the task: `nika:fetch` with a jq `mode` whose program `error("not ready")`s on the pending shape + `retry: { max_attempts: N, backoff_strategy: fixed, backoff_ms: 30000, on_codes: [...] }` · OR an `agent:` with fetch+wait+done (budget-bounded). `retry_when: ${{ … }}` is the additive v0.2 shape: it must not ship before the error-vs-value semantics are provably clean. |
| H20 | Environment targeting (dev / staging / prod) | **WRITTEN (idiom · no field)** | No `environments:` block. The pattern is **launch-time `inputs` + per-env secret paths** · pass `--var env=prod` (engine CLI), branch values with the CEL conditional (`model: ${{ inputs.env == 'prod' ? … : … }}`), and select the secret store path the same way (`secrets.api_key.key` is a per-env vault path the deploy supplies, OR two declared secrets gated by `when:`). Connection-set switching as a first-class construct (GHA environments · Prefect deployments) is a host/deploy concern: the language stays single-file and the env is just another input. An `environments:` sugar block is deferred unless empirical demand. |
| H21 | Else / default branch (exhaustive choice) | **WRITTEN (idiom · no construct)** | No `else:` / `Choice.Default` construct. Mutually-exclusive branches are two tasks with negated `when:` joined by the defined-null diamond pattern ([03 §branch joins](./03-dag.md)); an N-way switch is N tasks each `when: ${{ inputs.mode == '<case>' }}`. Exhaustiveness is author-maintained (a missing case = all-skip = a `null` at the join, which downstream sees explicitly: it does not silently corrupt). A dedicated `switch:`/`else:` would add a second control-flow primitive beside `when:`/`after:` · deferred · the CEL conditional `?:` already covers conditional VALUES (the common case) without any branch at all. |
| H22 | Embeddings (`embed(text) → vector`) | **WRITTEN (deliberate absence · `mcp:` / Connectome territory)** | No `embed:` verb and no `nika:embed` builtin. Raw embedding generation is not a language primitive: it is a tool, reached via `invoke: mcp:<embedder>/embed` (any embedding server) or `exec:` (a local model). Semantic RECALL (the RAG read path) is the reference engine's **Connectome** (`nika:connectome/*` · a deferred capability · [§Connectome](#the-connectome--deferred-capability--not-a-deferred-verb)), not a language verb. The language stays about ORCHESTRATION; embedding is a capability the orchestrated tools provide. Stated here because « an AI workflow language with no embeddings » is a fair « did you think of X »: yes · it is `mcp:`/tool territory by design, not a silent gap. |
| H23 | Cursor pagination / unbounded iteration (« fetch page → follow next-cursor until none ») | **OUT (the bounded forms are written · `while:` stays deferred)** | `for_each` iterates a collection KNOWN up front ([03 §fan-out](./03-dag.md)) and a task cannot `for_each` its own output: cursor-following is inherently unbounded iteration, which is `while:` (deferred above · it breaks the acyclic guarantee AND the static cost story: a workflow whose request count depends on a server's answers is not auditable-before-run). The v0.1 canonical forms · **(a) depth-known** · precompute the page list (`[range(1; N+1)]` via `nika:jq`) + `for_each` with `for_each.max_parallel` (page-numbered APIs); **(b) depth-unknown** · an `agent:` task whitelisted to `nika:fetch` + `nika:done` with `max_turns` as the page budget: the loop is budget-bounded and the cap is IN the file (auditable); **(c) heavy crawls** · the host loop (`exec: nika run fetch-page.yaml` per cursor · the sibling-run floor above — an unbounded cursor chain has no static call graph, so the native `invoke: workflow:` form cannot express it). A dedicated bounded construct (`while_bounded:` / `unfold:` with a MANDATORY `max_iterations:`) is the additive v0.2 candidate IF empirical demand shows (b) too heavy: it must carry a static iteration cap or it never ships. |

---

## Why this list exists

The defer list is canonical · grep-able · forever. Future contributors who think "should we add X?" can check this doc · X is either ·

- (a) explicitly deferred · with rationale · won't enter v0.1
- (b) absent from this doc · meaning it's never been considered · open an issue

This is the spec's structural protection against feature creep · Rams principle 10 in literary form.

---

🦋 *End of the v0.1 spec. The 5 pillars are locked. The defer list is canon.*

---

## The interop ladder (normative)

`exec:` is not an interop boundary. It is the **last rung of a ladder**,
and a workflow that reaches it has usually skipped a rung above.

> **Why this section exists.** The rule it replaces read « the interop
> boundaries are MCP and `exec:` (anything with a CLI) ». Everything has
> a CLI. That sentence made the escape hatch the default answer for the
> entire outside world, and the corpus shows the result: **13% of work
> tasks are `exec:`, and 54% of those are `bash` calling a shell-script
> library that lives outside any checkable file.** A language cannot
> claim a declared blast radius and then route the world through a rung
> that grants « run this program, and whatever it runs ».

Reach in this order. Each rung buys something the rung below cannot ·

| | rung | what it buys | reach for it when |
|---|---|---|---|
| 1 | **a builtin** (`invoke: tool: nika:*`) | typed args · typed output · per-operation permits · deterministic · no process | the capability is in the stdlib |
| 2 | **a sub-workflow** (`invoke: workflow:`) | **the reuse unit** · typed inputs/outputs · authority intersected · checkable · testable | you are about to write the same thing twice |
| 3 | **MCP** (`invoke: tool: mcp:<server>/<tool>`) | a typed tool contract someone already maintains | a server exposes it |
| 4 | **`agent:`** | a bounded loop that picks among tools | the sequence is not known in advance |
| 5 | **`exec:`** | a process | **nothing above fits, and you can say why** |

**Rung 2 is the one authors skip, and it is the one that matters.** It
shipped in engine 0.105 ([14 · Composition](./14-composition.md)) and
the corpus still contains workflows that `exec: bash -c "nika run ..."`
— shelling out to the very engine that was already offering the typed
call. Independent evidence that this is the load-bearing rung: in 68K
GitHub repositories, **half of all steps are shell, and half of THOSE
repeat a command that exists elsewhere in the corpus** (Decan, Mens et
al., ICSME 2022 — 139,501 distinct commands across 287,868 steps). The
missing thing was never a builtin. It was a place to put the second copy.

**What `exec:` is still for, and it is a short list** · a program with no
tool contract anywhere (`git`, `docker`, `cargo`), a one-off local
script, an OS operation with no builtin (`tar`). Those are legitimate and
they are why the rung exists. What is NOT legitimate is reaching rung 5
because rung 1 or 2 was never looked at — and the `native-first/*` lint
set exists to say so at check time.

**The measurable form of this rule.** A workflow whose `exec:` count
exceeds its `invoke:` count is not automatically wrong, but it is
answerable: *which rung did each `exec:` skip, and why?* If the answer is
« none, nothing above fits », the file is honest. If the answer is
« I did not know rung 2 existed », that is a documentation defect, not an
author defect.

