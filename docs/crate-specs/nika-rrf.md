# Crate spec — `nika-rrf`

| | |
|---|---|
| Status | **Phase M proposal** (Gate 1 · shipped 2026-06-11 · admission at its Phase-M step of the Connectome climb (ADR-004 + ADR-042 lineage)) |
| Layer | L1 — recall satellite (M3 · rank fusion) |
| LOC budget | ≤400 src |
| Crate version | tracks workspace |
| License | `AGPL-3.0-or-later` |
| Edition | 2024 |
| Publish | `false` initially · **publishable crates.io standalone** post-admission (per ADR-004 · external ecosystem value) |
| NIKA codes | `NIKA-CNX-*` family · exact range allocated at ceremony via `nika_error::codes` registry (never invented here) |

---

## 1. Purpose

Reciprocal Rank Fusion — merges the lexical (BM25) and vector (HNSW)
result lists into one hybrid ranking. ~150 LOC of load-bearing
simplicity; the canonical k=60 constant; deterministic, total, pure.

## 2. Public API (sketch · Gate 1 level · final at ceremony)

```rust
pub fn fuse(lists: &[Vec<RecordId>], k: f32) -> Vec<(RecordId, f32)>;
```

## 3. Dependencies (LOCKED · Connectome stack ratification 2026-06-11 · crates.io-verified)

none beyond std (pure function crate).

## 4. SOTA references (ratified 2026-06-11 · arXiv-API-verified)

- RRF · Cormack, Clarke & Buettcher · SIGIR 2009 (canonical · pre-arXiv)

## 5. Test strategy

Property tests (permutation invariance per-list · monotonicity · k sensitivity) · golden fixtures vs the paper's worked example. Mutation 100% target (tiny surface).

🦋 *Connectome satellite · « Memory is the function · the Connectome is the structure. »*
