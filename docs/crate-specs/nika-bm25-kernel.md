# Crate spec — `nika-bm25-kernel`

| | |
|---|---|
| Status | **W3 admission target · Q6 split adapter** · ADR-038 + ADR-043 binding |
| Layer | L1 — thin adapter · couples `nika-bm25-core` to `nika-kernel` sealed traits |
| LOC budget | ≤120 src (target ~80) — `recall_impl.rs` (60) + `error_map.rs` (20) |
| File cap | ≤1,500 LOC each |
| Crate version | tracks workspace (`0.80.0`) |
| License | `AGPL-3.0-or-later` |
| Edition | 2024 |
| Publish | `false` — Diamond-internal adapter · the publishable standalone value lives in `nika-bm25-core` |

---

## 1. Purpose

`nika-bm25-kernel` is the **thin trait-adapter crate** for the Q6 split
architecture (Option D · rust-architect 2026-05-12 audit · per ADR-043). It
implements `nika_kernel::ai::memory::MemoryRecall` for the
`nika_bm25_core::BmIndex` published-standalone primitive.

The split exists because Cargo features don't gate `pub` items in published
crates (tantivy/serde semver-checks history confirms · `cargo public-api`
Gate 12 forward-compat surface inflated). Two crates = clean compiler-
enforced separation · external `crates.io` consumers of `nika-bm25-core`
never see `nika-kernel` types in rustdoc.

## 2. Public API

```rust
//! `nika-bm25-kernel` · MemoryRecall adapter for nika-bm25-core.

use nika_bm25_core::BmIndex;
use nika_kernel::ai::memory::{MemoryRecall, RecallQuery, MemoryHit, MemoryError};

impl MemoryRecall for BmIndex {
    async fn recall(
        &self,
        query: RecallQuery,
    ) -> Result<Vec<MemoryHit>, MemoryError> {
        // ~60 LOC · maps BmIndex::top_k results → Vec<MemoryHit>
        // + tokio::task::consume_budget().await every 128 iters per
        //   BLUEPRINT v1.3 §4 cooperative scheduling
    }
}
```

## 3. Internal modules

| Module | LOC | Purpose |
|---|---|---|
| `lib.rs` | ~10 | re-exports + crate-level docs |
| `recall_impl.rs` | ~60 | `MemoryRecall` trait impl · maps BmIndex score → MemoryHit + cooperative budget |
| `error_map.rs` | ~20 | maps `nika_bm25_core::BmError` → `nika_kernel::ai::memory::MemoryError` |
| **Total** | **~90** | within ≤120 budget · 30 LOC headroom |

## 4. Dependencies

```toml
nika-bm25-core = path · zero-dep core
nika-kernel    = path · sealed traits
nika-types     = path · MemoryHit · MemoryId
nika-error     = path · MemoryError mapping
thiserror      = workspace
tokio          = workspace · features = ["rt"] (consume_budget)
trait-variant  = workspace
```

## 5. Gate-by-gate readiness

Inherits from `nika-bm25-core.md` 12-gate map. Specifically ·
- Gate 1 SPEC · this doc ✅
- Gate 2 TDD · adapter tests via `MockMemoryRecall` harness (Gate 9 EXEMPT inherits)
- Gate 3 IMPL · ~90 LOC · ships W3 GREEN phase
- Gate 5 MUTATION ≥90% · ~30 mutants expected · trivial adapter
- Gate 9 CANARY · EXEMPT (thin adapter · no I/O of its own)
- Gate 10 PARITY · EXEMPT (CRAFT · zero brouillon counterpart)
- Gate 12 ATOMIC · **paired admission** with `nika-bm25-core` in single commit ·
  `feat(nika-bm25): admit pair (core + kernel) — all 12 gates passed`

## 6. References

- `docs/crate-specs/nika-bm25-core.md` (binding · same 12-gate ceremony)
- `docs/adr/adr-038-nika-bm25-admission.md`
- `docs/adr/adr-043-nika-bm25-q6-split.md` (NEW · queued · pending author)
- BLUEPRINT_2036.md v1.3 §2.5 + §4 + ADR-040 feature matrix
