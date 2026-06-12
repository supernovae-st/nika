# `nika run` — the composer plan (the L4 wiring of the shipped L3)

> Plan artifact · 2026-06-13 · post runtime-v2 ship (`be582b247`).
> The contract is LOCKED in `docs/crate-specs/nika-cli.md` §v0.81
> (`nika run <file>` · exit 0 ok · 1 workflow failed · live render §3)
> — this plan is the IMPLEMENTATION sequence. nika-cli is WIP (no
> 12-gate yet), so a WIP→WIP dep on nika-builtin is legal (the
> admitted-never-deps-WIP law binds ADMITTED crates only · the wave-3
> e2e already composes the dispatcher in tests).

## 0 · Why now

Every piece exists and is admitted (or already wired):

| Piece | State |
|---|---|
| `nika-runtime` (L3 executor · concurrency · full task pipeline) | ✅ admitted · v2 |
| `nika-builtin` `BuiltinDispatcher` (ToolExecute + ToolDefinitionProvider) | WIP · wired in cli tests |
| Production seams: `nika-fs` · `nika-clock` · `nika-http` · `nika-exec-runner` | ✅ admitted |
| `nika-providers` registry (real catalog · env keys) | ✅ admitted |
| Display fold + frames (`RunView` · `frame()` · §3.1 glyphs incl. `↻`/`◼`) | ✅ shipped |
| Exit-code contract + `--json` purity law | ✅ pinned by bin_smoke |

What does NOT exist: the prod `Stamper` (uuid-v7 + real clock — by
design: "prod stamps are the composer's concern, L4") · the live sink
· the composition fn.

## 1 · Batches (each lands green · TDD)

### B1 · `SystemStamper` + `JsonSink` (the machine lane first)
- `verbs/run/stamp.rs`: `SystemStamper` — `uuid::Uuid::now_v7()` +
  `SystemTime` → `Timestamp` (~30 lines + 3 tests: monotonic ids ·
  ts sanity · uniqueness over 10k).
- `JsonSink`: `EventSink` that writes one NDJSON line per event to
  stdout (the `--json` lane · never coloured · the contract's "NDJSON
  events verbatim"). Buffered writer · flush per event (an agent
  tailing the stream needs liveness).

### B2 · the production composition (`verbs/run/compose.rs`)
One fn: `production_runtime(model_default: &str) -> Runtime<…>` —
- `ExecVerb::new(ExecRunner::new())` (the real subprocess runner ·
  kill_on_drop)
- `BuiltinDispatcher::new(real fs · real http · real clock ·
  NullEmitter | event-bridge later · NonInteractive · NoWorkflow)`
  shared `Arc` → `InvokeVerb` + the agent's tool-defs
- `InferVerb::new(ProviderRegistry::new(real http · env keys) ·
  envelope model)`
- `AgentVerb::new(same registry-provider · same dispatcher ×2 ·
  envelope model)`
- `SystemClock` (nika-clock) · `RuntimeConfig::default()`
Type alias `ProdRuntime` tames the generic spelling. Unit test:
compose + run a trivial exec workflow against `/usr/bin/true`?
NO — composition is smoke-tested via mock/echo model + `exec: echo`
(hermetic · no network) in tests/run_verb.rs.

### B3 · `verbs/run.rs` — the verb
```text
parse (Strict) → check → dirty? render findings · exit 2
                       → clean → run(runtime · stamper · sink) →
outcome.ok → exit 0 | exit 1 (workflow failed · the failure card)
```
- `--json`: JsonSink lane · zero frames · exit codes identical.
- TTY lane v0: fold events into `RunView` AFTER run completes is NOT
  acceptable (dead screen) → the MINIMAL live lane: a `FoldSink` that
  applies each event + repaints `frame()` on every event (clear +
  redraw · the demo replay loop already proves the painter). Spinner
  ticks arrive with a later polish pass (event-driven repaint only at
  v0 · documented).
- envelope defaults: `model:` from the workflow · `--model` override.
- `permits:` enforcement stays STATIC (the check gate) at v0 —
  documented honestly in `--help` (the runtime PermitChecked emission
  is the ADR-092 follow-up).
- exit 3 lanes: unreadable file · provider key missing for the
  resolved model (the doctor hint in the message).

### B4 · `examples run <slug>` flips from refusal to the real path
Same composition · the embedded pack's YAML. The refusal message
deletes (`pack_surface.rs`).

### B5 · bin coverage
`tests/bin_smoke.rs` grows: `run` on a 2-task exec workflow (echo ·
hermetic) → exit 0 + stdout carries the storyboard glyphs · a failing
command → exit 1 + the failure card · `--json` → NDJSON parseable ·
dirty file → exit 2 before ANY execution (audit-before-run at the
binary plane).

## 2 · Non-goals (the spec's own fence)
`--resume` (needs CheckpointWritten · v0.82) · streaming InferChunk
frames (provider streaming seam first) · cost meter (CostIncurred
emission is consumer-signal gated) · daemon/serve · sandbox.

## 3 · Risks
- ProviderRegistry::new real-http construction needs the env-key
  shape — read `nika-providers` docs first (NO invention; if a key is
  absent at RESOLVE time the error is already typed · surface it).
- The repaint loop on non-TTY must degrade to line-append (CI logs) —
  `IsTerminal` gate · ascii theme default.
- Concurrent session B is active in nika-cli/nika-builtin — land
  batches behind their commits (mtime-stable windows · the train law).
