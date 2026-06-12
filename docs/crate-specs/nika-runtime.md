# nika-runtime — crate spec (L3 · orchestration)

> Gate 1 artifact · 2026-06-12 · the first L3 crate. The DAG executor
> that owns what `crates/nika-cli/tests/e2e_pipeline.rs` rehearsed: the
> harness PLAYED the missing layer over the real verb crates · this
> crate IS that layer · the harness assertions are its conformance
> floor (same YAML in · same event stream out).

## 1 · Role

Execute one **checked** workflow wave-by-wave through the four verb
crates, emitting the canonical event stream. The runtime is the ONE
emission site per verb path (INV-024 · the verbs stay event-free) and
the ONE place dataflow bindings live.

```text
RawWorkflow + CheckReport (clean)        nika-schema   (audit BEFORE run)
        │
        ▼
nika_runtime::Runtime::run()             THIS CRATE
        │  waves (CheckReport order) · upstream-failure cascade
        │  `when:` gate (v0 subset · loud on out-of-scope)
        │  `${{ }}` interpolation (v0 textual · the CEL seam)
        ├──▶ infer  → nika-verb-infer
        ├──▶ exec   → nika-verb-exec
        ├──▶ invoke → nika-verb-invoke
        ├──▶ agent  → nika-verb-agent
        ▼
Vec<Event> via EventSink                 nika-event    (display folds it)
```

## 2 · Public API (v0)

```rust
pub struct Runtime<S, T, P, D> { /* the four verb instances */ }

impl<S, T, P, D> Runtime<S, T, P, D>
where
    S: ShellDyn,                  // exec seam   (kernel)
    T: ToolExecuteDyn,            // invoke seam (kernel)
    P: ProviderInferDyn,          // infer/agent seam (providers)
    D: ToolDefinitionProviderDyn, // agent tool-defs seam
{
    pub fn new(shell: Arc<S>, tools: Arc<T>, provider: Arc<P>,
               tool_defs: Arc<D>, default_model: impl Into<String>) -> Self;

    /// Audit-before-run is a hard precondition · a dirty report is
    /// NIKA-1700 (never executes).
    pub async fn run(
        &self,
        wf: &RawWorkflow,
        report: &CheckReport,
        stamper: &mut dyn Stamper,
        sink: &mut dyn EventSink,
    ) -> Result<RunOutcome, RuntimeError>;
}

pub trait Stamper   { fn next(&mut self) -> (EventId, Timestamp); }
pub trait EventSink { fn emit(&mut self, event: Event); }

pub struct RunOutcome {
    pub ok: bool,                              // terminal Completed vs Failed
    pub bindings: BTreeMap<String, String>,    // tasks.<id>.output
    pub outputs: BTreeMap<String, String>,     // workflow outputs: resolved
}
```

`DeterministicStamper` (seq·10ms · the EventPen idiom) ships in the
crate for tests + replay. `VecSink` (collect) ships for tests. Prod
stampers (kernel clock + uuid-v7) are the composer's concern (L4).

**Why generic over 4 seams** · the agent tool-defs impl lives in
`nika-builtin` (WIP · NOT admitted). An admitted crate never depends on
a WIP crate · the runtime stays seam-generic exactly like `AgentVerb`
and the composer (nika-cli L4) injects. Zero Cargo edge to any L2
domain impl beyond the four verb crates.

## 3 · Semantics (v0 scope · declared honest)

- **Waves** · `CheckReport.waves` is the schedule (the checker owns
  topology · the runtime never re-sorts). Tasks within a wave run
  sequentially in v0 (concurrency = roadmap · the event contract is
  order-stable for replay).
- **Cascade** · a task whose `depends_on` contains a dead id (failed or
  skip-cascaded) emits `TaskSkipped(note: "upstream failed")` and joins
  the dead set. A `when:`-skip is NOT a cascade (downstream of the gate
  still runs if its other deps are alive — floor semantics).
- **`when:` gate (v0 subset)** · `${{ <ref> == '<lit>' }}` ·
  `${{ <ref> != '<lit>' }}` · bare `${{ <ref> }}` (truthy = non-empty ·
  not `"no"`/`"false"`/`"0"`). `<ref>` = `vars.<key>` or
  `tasks.<id>.output`. ANYTHING else = NIKA-1703 loud · never a silent
  closed-gate (the rehearsal's stand-in dies here).
- **Interpolation (v0)** · textual `${{ tasks.<id>.output }}` +
  `${{ vars.<key> }}` substitution · invoke `args:` resolved over every
  JSON string leaf · an unresolved `${{` left in a rendered string is
  NIKA-1702 loud (the silent-literal class). The CEL evaluator (03-dag)
  replaces the module behind the same `expr::render` seam.
- **Per-verb notes** (display contract) · `invoke · <tool>` ·
  `exec · <argv0>` · `infer · <model_resolved>` · `agent · <model>` ·
  tokens field on infer/agent completions.
- **Terminal** · `WorkflowCompleted` iff zero failed tasks · else
  `WorkflowFailed`. `outputs:` resolve AFTER the terminal event from
  the final bindings (absent binding → output omitted · ok unchanged).

## 4 · Errors (NIKA-1700 range · Category::Runtime)

| code | when |
|---|---|
| NIKA-1700 | dirty CheckReport handed to run (audit-before-run violated) |
| NIKA-1701 | wave index out of bounds (checker/runtime contract breach) |
| NIKA-1702 | unresolved `${{ }}` after render (silent-literal guard) |
| NIKA-1703 | `when:` expression outside the v0 subset |

Verb failures are NOT runtime errors — they are `TaskFailed` events
carrying the verb's own `nika_code()` (the run continues per cascade).
`is_transient()` = false for all four (contract breaches + static
expression classes · retry never helps).

## 5 · Tests (the floor, ported + owned)

1. **Conformance floor** · the e2e fixture (4 waves · 6 tasks · diamond
   DAG) byte-equal event stream vs the rehearsal expectations (states ·
   notes · tokens · skip reasons) — then `e2e_pipeline.rs` flips from
   playing the runtime to CALLING it (the rehearsal retires its
   dispatch loop · same assertions).
2. **Cascade** · fail mid-DAG → downstream skipped · terminal Failed.
3. **Gate v0** · `==` open/closed · `!=` · bare-ref truthy table ·
   out-of-scope expr = NIKA-1703.
4. **Interpolation** · proptest (random ids/vars · render never panics ·
   resolved strings contain no `${{` · unknown ref = NIKA-1702).
5. **Agent dispatch** · scripted provider + mock tool-defs (the s12
   pattern) through the YAML path.
6. **Snapshots** · insta on the event stream (replay-stable ·
   DeterministicStamper).
7. **Mutation** · `cargo mutants -p nika-runtime` ≥90% killed.

## 6 · Non-goals (v0 · roadmap-tracked)

Full CEL · intra-wave concurrency · retry/backoff (`on_error` policies)
· checkpoints/resume (`CheckpointWritten` stays unemitted) · streaming
token frames · `nika-shield` enforcement (L3 sibling) · sandbox.

## 7 · Dependencies

`nika-types` · `nika-error` · `nika-event` · `nika-schema` ·
`nika-kernel` (hub) · the four `nika-verb-*` crates. Dev ·
`nika-kernel-mock` · `nika-providers` (mock/echo + scripted) · insta ·
proptest · tokio.
