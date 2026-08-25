# Crate spec — `nika-dataflow`

| | |
|---|---|
| Status | **ADMITTED 2026-08-25** for v0.115 — one signed-off replacement commit carries the descent, compatibility repair, immutable run-start dataflow, and all 12 gate proofs. This is a descent, not a new design: the code shipped and was tested inside `nika-runtime` before it moved. |
| Layer | L0 — the mechanical sort for pure evaluation: zero I/O, zero async, zero clock |
| Design | The run's **dataflow**: what a task record IS, and how a value referencing those records resolves. |
| LOC budget | ≤4,000 production LOC (actual 1,316 by `check-crate-size.sh`), ≤6,000 hard cap |
| File cap | ≤1,500 LOC (largest: `expr.rs`, 1,176 lines) |
| Function cap | ≤100 lines |
| License | `AGPL-3.0-or-later` · Edition 2024 · `publish = false` |
| Extraction source | `crates/nika-runtime/src/{expr.rs,jq.rs,record.rs}` + the four evaluation variants of `RuntimeError` |
| NIKA codes | **none allocated** — `DataflowError` keeps the engine-internal `NIKA_1702`/`NIKA_1703` it always carried (the range names the CLASS, not the crate) and speaks the spec-plane `NIKA-VAR-001` · `-002` · `-004` · `-005` · `-006` on the wire |

---

## 1. Purpose

Two questions live here, and they are one question:

- **What a task record IS** (`record`) — `TaskStatus` · `TerminalCause` ·
  `TaskErrorRecord` · `TaskRecord`, the spec-13 transition law (`legal`),
  the failure-cause triage (`failure_cause`), the Outcome IR
  (`outcome_json`) and the canonical value→string rendering
  (`render_value`).
- **How a value referencing those records resolves** (`expr` · `jq`) —
  `Scope` and its `${{ }}` island rendering (`render` · `render_json`),
  `cel-subset/0.1` gate evaluation (`eval_when` · `resolve_expr`), and
  `output:` named jq bindings (`eval_binding`).

They descend together because they are not separable: `expr` projects a
`TaskRecord` into the CEL object a `${{ tasks.x.output }}` island reads, and
renders values back out through `record::render_value`. A seam between them
would cut a single concept in half.

## 2. What it does NOT own

The **executor** stays in `nika-runtime`: waves, dispatch, settlement, the
event stream, retry/timeout, `on_error:`, the unwind lane, and every effect
seam. This crate is asked a question and answers with a value; it never
schedules, never emits, never waits.

The split point is exactly that seam: everything above it answers *what does
this value resolve to*, everything below answers *when does it run and what
happened*.

`TIMEOUT_CODE` sits here, beside `failure_cause` — the triage that READS it
— and `nika-runtime::task` re-exports it at its historical path so the
producer (the timeout race) and the classifier keep naming ONE constant.

## 3. Why the descent happened

`nika-runtime` reached its 15,000 prod-LOC wall — measured 14,994/15,000
(issue #1203), with a written PR unable to land because the *merge* crossed
at 15,020 even though branch and `main` were each green alone. The L3
orchestrator is the crate every feature lands in, so it is the crate that
hits the wall first.

The evaluation half was the cleanest candidate. Before the move it had
exactly **three** intra-crate edges — `crate::errors::RuntimeError`,
`crate::task::TIMEOUT_CODE`, and `expr → record` (internal to the trio) —
against **ten-plus** for the next candidate cluster (approval / pause /
resume / recover, which is woven through `task`, `settle`, `integrity`,
`agent_events`, `proof`, `witness` and `stamp`). It also needs nothing from
the runtime: no `EventSink`, no clock, no compose ladder, no session state.

This follows the same descent shape as `nika-proof`, `nika-dap`,
`nika-cap`, and `nika-secret`. The pattern: find the module the
wall-crate merely *hosts* rather than *owns*, give it its own crate,
re-export for compat.

**Result:** under the corrected production-LOC gate carried by this admission,
`nika-runtime` moves from **14,787 → 14,053** LOC (−734). The historical
14,994 measurement came from the counter defect repaired in the same wave;
the current numbers are derived from the self-tested counter over this tree.

## 4. Public surface

```rust
pub enum DataflowError { UnresolvedTemplate · WhenUnsupported · CelEval · OutputBinding }
impl DataflowError { spec_code() · wire_message() · from_cel() }
impl NikaErrorCode for DataflowError    // NIKA_1702 · NIKA_1703

#[non_exhaustive] pub struct Scope<'a> { /* private authority + context fields */ }
impl Scope<'_> { workflow_with_value_authorities() · with_task_context() · authority/context accessors · resolve_expr() }
pub fn expr::{render, render_json, eval_when}
pub fn jq::eval_binding

#[non_exhaustive] pub enum TaskStatus · TerminalCause
#[non_exhaustive] pub struct TaskRecord · TaskErrorRecord
pub const fn legal
pub const TIMEOUT_CODE
pub fn record::{failure_cause, outcome_json, render_value}
```

`Scope::workflow` and `Scope::workflow_with_secrets` remain empty-namespace
TEST constructors behind the `testing` feature. In production the struct is
`#[non_exhaustive]`, every field is private, and
`workflow_with_value_authorities` is the only root constructor;
`with_task_context` can only layer task/iteration context onto that
authority-bearing value. A downstream crate therefore cannot forge or mutate
an empty-authority scope with a literal. The public compile-fail example and
API snapshot pin that FCI-002 boundary.

## 5. The seam did not move

`nika-runtime` keeps source-compatible `TaskRecord`, `TaskStatus`,
`TerminalCause`, `TaskErrorRecord` and `legal` wrappers at their historical
`nika_runtime::…` paths. The wrappers preserve exhaustive matches and error
field literals, while `RunOutcome` converts from the canonical dataflow record
once. Internally `crate::{expr,jq,record}` remain aliases, and the runtime converts the dataflow-owned error
back into the four historical `RuntimeError::{UnresolvedTemplate,
WhenUnsupported,CelEval,OutputBinding}` variants. Their constructors, pattern
matching, fields, `Display`, `Diagnostic`, `spec_code()` and `nika_code()`
remain intact. `RuntimeError::from_cel` still exists and delegates.

The wire form a consumer sees (`tasks.X.error.code`, `on_codes:` filtering,
the run report) is byte-identical to before the descent. The conversion and
public-construction tests in `crates/nika-runtime/src/errors.rs` prove both
directions of the compatibility seam: dataflow errors map to the old variants,
and downstream code can still construct and match those variants directly.

## 6. Gates

| Gate | Verdict |
|---|---|
| 1 SPEC | ✅ this document |
| 2 TDD | ✅ inherited — the tests were written RED before the code, in `nika-runtime`, and moved with it (`git mv`, history preserved) |
| 3 IMPL | ✅ production LOC under the crate cap; expanded unit suite passes |
| 4 CLIPPY | ✅ 0 warnings, `--workspace --all-targets` |
| 5 MUTATION | ✅ 98% killed — 82/83 viable mutants caught by `check-mutation-floor.sh nika-dataflow` (2026-08-25); `docs/mutation/nika-dataflow-2026-08-25.txt` records the exact source+harness closure digest, reproducibly checked by `scripts/ci/check-dataflow-mutation-evidence.sh` |
| 6 PROPERTY | ✅ 512 generated cases — two proptests at 256 cases each cover total template rendering and deterministic/reparseable canonical values |
| 7 BENCHMARKS | N/A — CEL evaluation over a `BTreeMap` scope; the hot path is the verb, not the render |
| 8 DOCS | ✅ every public item documented, `# Errors` on every fallible door |
| 9 CANARY | N/A — this is a pure member split; existing runtime workflow tests exercise `${{ }}`, `when:`, and `output:` end to end |
| 10 PARITY | ✅ deterministic pre/post differential probe covers record projection with non-null timestamps, transitions, outcome/canonical rendering, value namespaces/loop locals, CEL, jq, the real over-16-MiB path, and all four runtime error wrappers |
| 11 REVIEW | ✅ three independent parallel audits completed; all P1 blockers repaired in the replacement |
| 12 ATOMIC | ✅ one signed-off replacement commit carries the admission, repair, 12-line ledger, totals, WIP removal, and regenerated status projection |

### Layer placement and ADR-027 fanout

The mechanical sort in `crate-layer-registry.md` is decisive: a pure, sync,
zero-I/O evaluator is L0. Five sibling dependencies exceed ADR-027's default
fanout of three, so the manifest uses that policy's explicit
`L0-DEP-FANOUT-EXEMPT` record instead of relabelling pure logic as an L1
effect. Each edge is part of the one dataflow concept: record values
(`nika-types` + `nika-cap`, which canonically owns `Permits`), island/CEL
evaluation (`nika-tmpl` + `nika-cel`), and canonical error identity
(`nika-error`).

The jq seam installs natives through one typed `nika-cap` policy. It refuses
`env`, diagnostic output (`debug`/`stderr`), and process control
(`halt`/`halt_error`); it replaces the accepted clock/timezone natives with
pure definitions over the run-start value supplied by the effect-owning
caller (timezone projection is deterministic UTC because the language grants
no host-timezone authority). Any escaped halt becomes `NIKA-VAR-004` without
calling the upstream process-exiting unwrap helper. Public-behavior tests
enumerate this effect surface and drive the real rendered-size ceiling.

## 7. Related

- `docs/crate-specs/nika-secret.md` — the closest descent precedent (same crate, same wall)
- `docs/crate-specs/nika-runtime.md` §2 — the executor that consumes this
- spec `04-variables.md` §task output reference — the record this crate defines
- spec `05-errors.md` §142 — why an engine-internal code must never reach `tasks.X.error`
- issue #1203 — the wall that forced the descent
