# Crate spec — `nika-temporal`

| | |
|---|---|
| Status | **Phase M proposal** (Gate 1 · shipped 2026-06-11 · admission at its Phase-M step of the Connectome climb (ADR-004 + ADR-042 lineage)) |
| Layer | L1 — time satellite (M8) |
| LOC budget | ≤2,000 src |
| Crate version | tracks workspace |
| License | `AGPL-3.0-or-later` |
| Edition | 2024 |
| Publish | `false` initially · **publishable crates.io standalone** post-admission (per ADR-004 · external ecosystem value) |
| NIKA codes | `NIKA-CNX-*` family · exact range allocated at ceremony via `nika_error::codes` registry (never invented here) |

---

## 1. Purpose

Bitemporal layer — every assertion carries valid-time (when true in
the world) and transaction-time (when recorded). « What did I believe
on May 10th? » and « what was true on May 10th? » are different
queries, and both work. Zep (2025) validates exactly this design for
agent memory in production.

## 2. Public API (sketch · Gate 1 level · final at ceremony)

```rust
pub struct TemporalQuery { /* as-of · between · valid vs tx */ }
pub fn as_of(store: &ConnectomeStore, tx_time: Timestamp) -> SnapshotView;
pub fn valid_during(store: &ConnectomeStore, interval: Interval) -> TripleSet;
```

## 3. Dependencies (LOCKED · Connectome stack ratification 2026-06-11 · crates.io-verified)

RDF-star annotations over `oxigraph@0.5.8` (the locked store) · no extra deps.

## 4. SOTA references (ratified 2026-06-11 · arXiv-API-verified)

- Bitemporal model · Snodgrass 1995 (canonical DB time)
- Zep · arXiv:2501.13956 (temporal KG for agent memory · prod validation)

## 5. Test strategy

Bitemporal truth-table fixtures (4 quadrants valid×tx) · retroactive-correction scenarios · as-of determinism. Mutation ≥90%.

🦋 *Connectome satellite · « Memory is the function · the Connectome is the structure. »*
