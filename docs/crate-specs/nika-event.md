# Crate spec — `nika-event`

| | |
|---|---|
| Status | **ADMITTED 2026-05-24** (`d009b1dd8`) · was **L0 admission target** (event chronicle surface · foundational) |
| Layer | L0 — pure · zero I/O · zero async · `Send + Sync` |
| Design | Caller-supplied id + timestamp (L0 never reads a clock) · `#[non_exhaustive]` taxonomy · object-safe `Emitter` trait |
| LOC budget | ≤800 src (actual ~462 · kind ~103 + event ~105 + emitter ~118 + error ~71 + lib ~65) |
| File cap | ≤1,500 LOC each (max 118 · well under) |
| Function cap | ≤100 lines each (all small) |
| Crate version | tracks workspace |
| License | `AGPL-3.0-or-later` |
| Edition | 2024 |
| Publish | `false` — foundation crate (ADR-017) |
| NIKA codes | NIKA-801..802 (`Category::Observability`) |

---

## 1. Purpose

`nika-event` is the **engine runtime chronicle surface**. It provides the
canonical event envelope ([`Event`]), the closed-but-extensible taxonomy
([`EventKind`]), and the object-safe sink trait ([`Emitter`]) with two L0
implementations (`NoOpEmitter`, `InMemoryEmitter`).

Every downstream verb path (`infer · exec · invoke · agent`) and the
workflow/task lifecycle emit events through this surface. Keeping it at L0
(pure values · caller supplies id + timestamp) makes the entire chronicle
deterministic and trivially testable — the clock is an **L1** effect
(`nika-clock`), never read here.

**Domain boundary** · this is the *engine* chronicle (runtime events). The
*studio* keeps a separate chronicle in its own private tree, disjoint per
`journal-storage-tiers.md` — they share the NDJSON serialization spirit but
have disjoint taxonomies. Do not conflate.

---

## 2. Public API

```rust
// kind.rs
#[non_exhaustive] pub enum EventKind {
    WorkflowStarted, WorkflowCompleted, WorkflowFailed,
    TaskScheduled, TaskStarted, TaskCompleted, TaskFailed, TaskSkipped,
    VerbInvoked, ToolInvoked, CheckpointWritten,
    // 2026-06-11 cohort (§4bis · additive)
    TaskRetrying, TaskCancelled, WorkflowCancelled,
    CostIncurred, InferChunk, PermitChecked,
}
#[non_exhaustive] pub enum EventClass {
    Workflow, Task, Dispatch, Durability, Cost, Stream, Security,
}
impl EventKind {
    pub const fn as_str(&self) -> &'static str;   // snake_case wire slug
    pub const fn is_terminal(&self) -> bool;       // completed | failed | cancelled
    pub const fn is_failure(&self) -> bool;        // failed only — cancelled is NOT
    pub const fn class(&self) -> EventClass;       // the 7-class coarse partition
}

// event.rs — consuming builder, all fields pub for downstream projection
#[non_exhaustive] pub struct Event {
    pub id: EventId, pub timestamp: Timestamp, pub kind: EventKind,
    pub run: Option<RunId>, pub correlation: Option<CorrelationId>,
    pub fields: Vec<KeyValue>,
}
impl Event {
    pub fn new(id, timestamp, kind) -> Self;
    pub fn with_run(self, RunId) -> Self;
    pub fn with_correlation(self, CorrelationId) -> Self;
    pub fn with_field(self, KeyValue) -> Self;
    pub fn with_fields(self, Vec<KeyValue>) -> Self;
}

// emitter.rs — object-safe (Vec<Box<dyn Emitter>> fan-out)
pub trait Emitter: Send + Sync {
    fn emit(&self, event: Event) -> Result<(), EventError>;
}
pub struct NoOpEmitter;                 // drops every event, never fails
pub struct InMemoryEmitter;             // unbounded() | bounded(cap) · len/is_empty/drain

// error.rs
#[non_exhaustive] pub enum EventError {
    SerializationFailed { detail: String },   // NIKA-801 (future I/O emitters)
    BufferFull { capacity: usize },            // NIKA-802 (bounded InMemory)
}
```

---

## 3. Forward-compat invariants (Gate 12)

- `EventKind` is `#[non_exhaustive]` — variants added on MINOR (downstream
  `match` carries `_`). Per `no-legacy-no-back-compat.md` Class 1.
- `Event` is `#[non_exhaustive]` with a `new()` constructor (Invariant #19).
- `EventError` is `#[non_exhaustive]` (Invariant #25 · error enums from day one).
- `EventKind::as_str()` slugs are **wire-stable** (snapshot-locked · `tests/`).
- `Emitter::emit` returns `Result` so I/O-backed emitters (higher layers)
  reuse the exact contract — additive, no break.

---

## 4. The 12 gates

| Gate | Status | Evidence |
|---|---|---|
| 1 SPEC | ✅ | this file |
| 2 TDD | ✅ | `tests/event_contract.rs` · 22 integration + 5 doctests |
| 3 IMPL | ✅ | ~462 LOC src · compiles · zero `.unwrap()`/`.expect()` in src |
| 4 CLIPPY 0 | ✅ | `cargo clippy --workspace --all-targets -- -D warnings` GREEN |
| 5 MUTATION ≥90% | ✅ | `cargo mutants -p nika-event` · 20/21 caught = **95.2%** (1 equivalent mutant: `unbounded()` ≡ `Default::default()`, documented in `emitter.rs`) |
| 6 PROPERTY | ✅ | 4 proptest properties (as_str non-empty · terminal-implies-workflow · bounded never exceeds cap · unbounded accepts N) |
| 7 BENCHMARKS | N/A | pure value types · no hot path (justified) |
| 8 DOCS | ✅ | `RUSTDOCFLAGS=-D warnings cargo doc --no-deps` 0 warnings · every pub item documented |
| 9 CANARY E2E | N/A | L0 types · no `.nika.yaml` runtime surface (justified) |
| 10 PARITY | N/A | no standalone brouillon `nika-event` crate · event types embedded in the brouillon engine monolith (`tools/nika-engine/src/event/`) · taxonomy is **post-brouillon canonical** (4-verb model D-2026-05-22-N18) · CRAFT-fresh per ADR-001 (justified exemption) |
| 11 REVIEW SWARM | ✅ | 3-agent parallel (spn-nika:code-reviewer + spn-rust:rust-pro + feature-dev:code-reviewer) |
| 12 ATOMIC | ✅ | 1 commit · Nika 🦋 trailer |

---

## 4bis. Vocabulary cohort 2026-06-11 — contract closure (additive)

Six kinds + the `EventClass` coarse classifier landed as a MINOR-additive
cohort, closing the vocabulary over the `nika-cli` display contract (the
contract NAMED `cost_incurred`/`infer_chunk` verbatim in §3.3 and required
`retrying`/`cancelled` states in §3.1 that no event could express — UI
states must be ⊆ event-expressible states):

| Kind | Slug | Why |
|---|---|---|
| `TaskRetrying` | `task_retrying` | §3.1 `↻` — attempt failed, retry scheduled (`attempt`/`max_attempts`/`delay_ms` fields · the runtime stamps the chosen backoff) |
| `TaskCancelled` | `task_cancelled` | §3.1 `◼` — a decision, not a defect (NOT `is_failure`) · the runtime emits it for an upstream-failure cascade (spec 03 default gate); a task `timeout:` is a FAILURE (`NIKA-TIMEOUT-001`), never a cancellation |
| `WorkflowCancelled` | `workflow_cancelled` | terminal-not-failure (joins `is_terminal`) |
| `CostIncurred` | `cost_incurred` | §3.3 verbatim — the live `~$` meter refold driver (`tokens`/`usd` deltas) |
| `InferChunk` | `infer_chunk` | §3.3 verbatim — streaming output delta (`delta` field) |
| `PermitChecked` | `permit_checked` | the `permits:` boundary observable at runtime (`gate`/`subject`/`decision`) — ADR-092's audit moat |

Cost shape law: `cost_incurred` carries SPEND (deltas a meter folds);
`task_completed` carries OUTCOME — consumers never double-count.

`EventKind::class()` → `EventClass` (Workflow · Task · Dispatch ·
Durability · Cost · Stream · Security · Agent · `#[non_exhaustive]`):
renderers and routers branch on the stable classes instead of every
variant. Tests pin: the full slug wire table (serde ↔ `as_str` FCI-003) ·
the classification partition · the contract-named slugs verbatim ·
cancellation ≠ failure. Mutation on `kind.rs`: 7 caught / 1 unviable
(100% viable-kill).

## 4ter. Agent-loop cohort 2026-06-12 — the loop's observable mind (additive)

Five kinds + the `EventClass::Agent` class (ADR-096): every internal
DECISION the `agent` verb takes is event-expressible. The L2 loop reports
through its `AgentObserver` seam; the L3 runtime maps payloads onto these
kinds 1:1 (INV-024 — the adapter is the ONE emission site). Per AgentOps
(arXiv:2411.05285), agent traces must expose decisions, not just I/O.

| Kind | Slug | Why |
|---|---|---|
| `AgentToolsSelected` | `agent_tools_selected` | the per-turn routing decision (`offered`/`universe`/per-source counts — the MCP-Zero-style active-discovery surface) |
| `AgentNudge` | `agent_nudge` | a bounded Reflexion corrective was injected (`reason` · `repeated_actions`/`error_streak`) |
| `AgentStalled` | `agent_stalled` | the no-progress stop's evidence (`period`/`repeats` — the TRAIL repetitive-action class) rides IN the trace; diagnostic, NOT `is_failure` (the task event carries the verdict) |
| `AgentComposeChecked` | `agent_compose_checked` | an `agent:compose` draft got its static verdict (`valid`/`violations` — generation is not permission) |
| `AgentBudgetCheckpoint` | `agent_budget_checkpoint` | per-turn spend snapshot — the curve is observable mid-run, not just at the end |

## 5. Consumers (downstream)

- **L2 verb crates** (`nika-verb-*`) — emit `VerbInvoked` / `ToolInvoked` at
  exactly 1 site per verb path (Invariant #24).
- **L3 runtime** — emits the workflow + task lifecycle.
- **Future `nika-connectome`** — ingests events for the chronicle/recall split
  (the engine chronicle projects into the Connectome's RDF substrate; raw
  payloads stay hashed per sovereignty Rule 1).
