# Crate spec — `nika-types`

| | |
|---|---|
| Status | Phase 1 — admitted to workspace; **rename → `nika-core` planned in Phase D** (Foundation v0.81 lock) |
| Layer | L0 (PURE, zero I/O, zero async) |
| Sub-tier | L0-tier-0 — leaf crate, zero `nika-*` dependencies (per ADR-024 L0 sub-tiers) |
| Design | **Monolithic value-types crate** — all primitive domain values, IDs, hashes, budgets, traces, costs |
| LOC budget | ≤5,000 src (currently 2,862 — comfortable headroom) |
| File cap | ≤1,500 LOC each |
| Function cap | ≤100 lines each |
| Source on `main` (reference) | `tools/nika-core/src/{id,trust,hash,...}.rs` (~3,500 LOC scattered) |
| Crate version | tracks workspace |
| License | `AGPL-3.0-or-later` |
| Edition | 2024 |
| Publish | `false` — foundation crate, never on crates.io (see ADR-017 + `feedback_publish_false_foundation_strategy.md`) |

---

## 1. Purpose

`nika-types` is **the leaf crate** of the Diamond architecture: every other crate
depends on it, and it depends on nothing in the `nika-*` namespace. It defines
the **value types** that flow through the entire engine — IDs, hashes,
trust levels, costs, budgets, retry policies, baggage, cancellation contexts,
and the schema-frame envelope.

The crate is intentionally **value-only** — no behavior beyond constructors,
accessors, serde derive, and a few small invariants checked at construction time.
Anything that touches I/O, async, or domain logic lives in higher layers.

Anchors the L0 sub-tier discipline (ADR-024): zero downstream `nika-*` deps,
maximally reusable, can be embedded in any future binary or extension crate.

**Renaming note**: this crate will be renamed `nika-core` during Phase D
of the Foundation v0.81 arc. The current name `nika-types` understates its
role — it is THE Nika core value layer. The rename is purely cosmetic; the
public API surface stays identical and is protected by `cargo public-api` +
`#[non_exhaustive]` ratchet.

---

## 2. Public API surface (16 modules)

Each module lives in its own file. All public types are `#[non_exhaustive]`
with `pub fn new(...)` constructors per Invariant #19.

```rust
// ── IDs (id.rs) ────────────────────────────────────────────────
pub struct RunId(/* private */);
pub struct EventId(/* private */);
pub struct CorrelationId(/* private */);
pub struct TraceId(/* private */);
pub struct SpanId(/* private */);
pub struct TaskId(pub String);          // P0: scheduled to private in Phase B.8
pub struct ToolCallId(pub String);      // P0: scheduled to private in Phase B.8

// ── Hashing (hash.rs) ──────────────────────────────────────────
pub struct Blake3Hash([u8; 32]);
pub struct ContentDigest { /* hash + algorithm */ }
pub struct BlobRef { /* digest + size */ }

// ── Trust (trust.rs) ───────────────────────────────────────────
#[non_exhaustive] pub enum TrustLevel { Untrusted, External, Internal, System }
impl TrustLevel { pub fn join(self, other: Self) -> Self; pub fn meet(self, other: Self) -> Self; }

// ── Cost (cost.rs) ─────────────────────────────────────────────
pub struct Cost { /* tokens_in/out, dollars, walltime_ms, trust */ }

// ── Token usage (token_usage.rs) ───────────────────────────────
pub struct TokenUsage { /* 18 fields: input/output/cached/reasoning × prompt/completion/tool */ }

// ── Budget (budget.rs) ─────────────────────────────────────────
pub enum BudgetDirective { Inherit, Override(Budget), None }
pub struct Budget { /* dollar/token/walltime caps */ }

// ── Cancellation (cancel.rs) ───────────────────────────────────
pub struct CancelCtx { /* Arc<AtomicBool> */ }
impl CancelCtx { pub fn new() -> Self; pub fn cancel(&self); pub fn is_cancelled(&self) -> bool; }

// ── Baggage (baggage.rs) ───────────────────────────────────────
pub const MAX_ENTRIES: usize = 64;
pub const MAX_SIZE_BYTES: usize = 8192;
pub struct Baggage { /* W3C Baggage header */ }
pub struct BaggageEntry { /* key + value + metadata */ }

// ── Compression (compression.rs) ───────────────────────────────
pub struct CompressionPolicy { /* algorithm + level + threshold */ }

// ── Retry (retry.rs) ───────────────────────────────────────────
pub struct RetryPolicy { /* max_attempts + backoff + jitter */ }

// ── Resource (resource.rs) ─────────────────────────────────────
pub struct ResourceCaps { /* max_memory_mb + max_walltime_ms */ }

// ── Role (role.rs) ─────────────────────────────────────────────
#[non_exhaustive] pub enum Role { System, User, Assistant, Tool }

// ── Schema (schema.rs) ─────────────────────────────────────────
pub struct Span { /* line + column + offset */ }       // P0: #[non_exhaustive] in Phase B.8
pub struct Spanned<T> { value: T, span: Span }         // P0: PartialEq fix in Phase B.8
pub struct LineCol { line: u32, col: u32 }             // P0: #[non_exhaustive] in Phase B.8
pub struct Templatable { /* parsed template + raw */ } // P0: #[non_exhaustive] in Phase B.8

// ── Checkpoint (checkpoint.rs) ─────────────────────────────────
pub struct AgentCheckpoint { /* messages + state */ }
pub struct CheckpointMessage { /* role + content + tool_calls */ }
pub struct ToolCallRecord { /* id + name + args + result */ }

// ── Memory (memory.rs) ─────────────────────────────────────────
pub struct MemoryFrameRef { /* anchor for the 2.0 Connectome */ }
```

---

## 3. File layout

```
crates/nika-types/
  Cargo.toml
  src/
    lib.rs           (~80 LOC — pub mod + re-exports)
    baggage.rs       W3C Baggage header types
    budget.rs        Budget caps + BudgetDirective enum
    cancel.rs        CancelCtx (Arc<AtomicBool>)
    checkpoint.rs    Agent checkpoint + tool-call records
    compression.rs   Compression policy
    cost.rs          Cost per request (tokens, dollars, walltime, trust)
    hash.rs          Blake3Hash, ContentDigest, BlobRef
    id.rs            All ID newtypes (RunId, EventId, …)
    memory.rs        MemoryFrameRef (the Connectome seam, 2.0 reservation)
    resource.rs      Resource caps
    retry.rs         Retry policy
    role.rs          Role enum (System/User/Assistant/Tool)
    schema.rs        Span, Spanned<T>, LineCol, Templatable
    token_usage.rs   18-field TokenUsage
    trust.rs         TrustLevel lattice (join/meet)
```

Total: **2,862 LOC** src across 16 files, all under the 1,500-LOC file cap.

---

## 4. Dependencies

```toml
[dependencies]
serde      = { workspace = true, optional = true }
serde_json = { workspace = true, optional = true }
uuid       = { workspace = true, optional = true }

[features]
default = ["serde"]
serde   = ["dep:serde", "dep:serde_json", "dep:uuid"]
```

Zero `nika-*` dependencies — true L0-tier-0 leaf.

---

## 5. Test plan

| Test location | Scope | Count |
|---|---|---|
| Inline `#[cfg(test)] mod tests` per file | constructor invariants, serde roundtrip, lattice properties | ~190 |
| `trust.rs` proptest | join/meet commutativity, idempotence, ordering | ~5 |
| `id.rs` proptest | ID format invariants, parse/serialize roundtrip | ~3 |
| `hash.rs` proptest | Blake3 deterministic, BlobRef equality | ~2 |

All inline. Run with `cargo test -p nika-types --lib`.

Currently **~200 tests** for nika-types alone (sums into the workspace 846).

---

## 6. Gate exemptions

- **Gate 7 (Benchmarks)**: Exempt. Pure value types with no hot path.
- **Gate 9 (Canary E2E)**: Exempt. No runtime to canary.

All other gates met.

---

## 7. Foundation v0.81 evolution

Phase D refactor (Foundation v0.81 lock) will:

1. **Rename** `nika-types` → `nika-core` (Cargo.toml, all `path = "../nika-types"` deps, all `use nika_types::` imports, all docs)
2. **Apply P0 fixes (Phase B.8)**:
   - `#[non_exhaustive]` on `Span`, `Spanned<T>`, `LineCol`, `Templatable`
   - Make inner field of `TaskId` and `ToolCallId` private (force constructor)
   - Fix `Spanned<T>` `PartialEq` to ignore span (or document why it doesn't)
3. **Extract** schema-related types (`Span`, `Spanned`, `LineCol`, `Templatable`) to a new sibling crate `nika-schema-ast` per ADR-022
4. **Reserve** memory-frame fields for the 2.0 Connectome (already done partially)

The rename + extraction is a single atomic Phase D commit — no interim state.

---

## 8. Actual metrics (post-admission)

| Metric | Value |
|---|---|
| Total LOC (src) | 2,862 |
| Files | 16 |
| Tests | ~200 (inline) |
| Mutation score | **measured 2026-06-10** · 213/219 viable caught · 6 equivalent-mutant exemptions (see below) |
| Clippy warnings | 0 |
| Doc warnings | 0 |
| Unwraps in src | 0 (test-only) |
| Forward-compat ratchet | `#[non_exhaustive]` on most enums; partial on structs (P0 fix in B.8) |

<!-- GATE5-EXEMPT: 6 -->
<!-- ^ SSOT for scripts/ci/check-mutation-floor.sh BUDGET mode (reproducible).
     MEASURED 2026-06-10 · `cargo mutants -p nika-types -- --lib` → 24 real
     survivors killed (timestamp/cost/id/memory/trust targeted tests). The 6
     remaining are EQUIVALENT mutants, not test gaps:
       - timestamp.rs `> → >=` in the 4 saturating clamps (from_unix_ms,
         WallDuration::from_micros/millis/secs): identical at the boundary —
         when `v == max`, both `>` and `>=` yield `max`.
       - timestamp.rs civil_from_days `z<0` negative-branch (2 mutants on
         `z - 146_096`): unreachable for the Timestamp type's ±292-year range
         (z = days + 719_468 stays positive for all realistic instants). -->


---

## 9. Audit trail

| Date | Author | Change |
|---|---|---|
| 2026-04-13 | Phase 1 W1 | Initial admission to workspace as `nika-types`. **Gate 1 SPEC was skipped — corrected retroactively in Phase B.0 unblock-push (this doc).** |
| 2026-04-15 | S1-C | Added 23 L0 types (cost, id, trust, retry, budget, schema, hash, baggage, resource — see HANDOFF S1-C) |
| 2026-04-16 | Phase B.0 | `publish = false` lock added (commit `a4ed8c309`) |
| 2026-04-16 | Phase B.0 unblock | Spec written retroactively to satisfy Gate 1 + hygiene vector 6 |
| Phase D (planned) | Foundation v0.81 | Rename → `nika-core`, extract schema-ast types, P0 ratchets |

🦋
