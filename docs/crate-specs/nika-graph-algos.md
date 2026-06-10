# Crate spec — `nika-graph-algos`

| | |
|---|---|
| Status | **Phase M proposal** (Gate 1 · shipped 2026-06-11 · admission at its Phase-M step of the Connectome climb (ADR-004 + ADR-042 lineage)) |
| Layer | L1 — reasoning satellite (M6) |
| LOC budget | ≤4,000 src |
| Crate version | tracks workspace |
| License | `AGPL-3.0-or-later` |
| Edition | 2024 |
| Publish | `false` initially · **publishable crates.io standalone** post-admission (per ADR-004 · external ecosystem value) |
| NIKA codes | `NIKA-CNX-*` family · exact range allocated at ceremony via `nika_error::codes` registry (never invented here) |

---

## 1. Purpose

Graph cognition over the connectome — community detection (Leiden),
centrality, shortest paths, and **Personalized PageRank** (the HippoRAG
bridge: seed PPR from query-matched entities to surface associatively-
linked memories — the single strongest memory↔graph result of 2024-25,
portable pure-Rust).

## 2. Public API (sketch · Gate 1 level · final at ceremony)

```rust
pub fn leiden(g: &ConnectomeGraph, resolution: f64) -> Communities;
pub fn personalized_pagerank(g: &ConnectomeGraph, seeds: &[NodeId], damping: f64)
    -> Vec<(NodeId, f64)>;
pub fn centrality(g: &ConnectomeGraph, kind: Centrality) -> Vec<(NodeId, f64)>;
```

## 3. Dependencies (LOCKED · Connectome stack ratification 2026-06-11 · crates.io-verified)

`petgraph@0.8.3` (MIT/Apache) + `graphrs@0.11` (MIT · native Leiden/Louvain) · PPR = maison (~200 LOC power-iteration).

## 4. SOTA references (ratified 2026-06-11 · arXiv-API-verified)

- Leiden · Traag et al. · arXiv:1810.08473
- HippoRAG · arXiv:2405.14831 · HippoRAG 2 · arXiv:2502.14802 (ICML 2025 · the PPR memory pattern)

## 5. Test strategy

Golden fixtures vs NetworkX outputs (small graphs) · Leiden well-connectedness property · PPR convergence tolerance tests. Mutation ≥90%.

🦋 *Connectome satellite · « Memory is the function · the Connectome is the structure. »*
