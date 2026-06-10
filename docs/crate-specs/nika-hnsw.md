# Crate spec — `nika-hnsw`

| | |
|---|---|
| Status | **Phase M proposal** (Gate 1 · shipped 2026-06-11 · admission at its Phase-M step of the Connectome climb (ADR-004 + ADR-042 lineage)) |
| Layer | L1 — recall satellite (M2 · vector ANN) |
| LOC budget | ≤2,500 src |
| Crate version | tracks workspace |
| License | `AGPL-3.0-or-later` |
| Edition | 2024 |
| Publish | `false` initially · **publishable crates.io standalone** post-admission (per ADR-004 · external ecosystem value) |
| NIKA codes | `NIKA-CNX-*` family · exact range allocated at ceremony via `nika_error::codes` registry (never invented here) |

---

## 1. Purpose

Approximate-nearest-neighbor vector recall over the connectome's
embeddings — Pinecone-class, local, sovereign. Embeddings live IN the
RDF graph as `xsd:base64Binary` literals (ADR-004 storage scenario); the
HNSW index is an **ephemeral cache rebuilt at boot** — the graph is the
truth, the index is derived. Each embedding literal carries its model
provenance (embedding-provenance invariant) — no silent cross-model cosine.

## 2. Public API (sketch · Gate 1 level · final at ceremony)

```rust
pub struct HnswIndex { /* hnsw_rs wrapper · M dims · ef params */ }
impl HnswIndex {
    pub fn build(records: impl Iterator<Item = (RecordId, Vec<f32>)>) -> Self;
    pub fn search(&self, query: &[f32], k: usize) -> Vec<(RecordId, f32)>;
    pub fn insert(&mut self, id: RecordId, vec: Vec<f32>);
}
```

## 3. Dependencies (LOCKED · Connectome stack ratification 2026-06-11 · crates.io-verified)

`hnsw_rs@0.3.4` (MIT/Apache · pure-Rust · anndists/rayon). Matryoshka truncation + int8 quantization as feature-gated R2.

## 4. SOTA references (ratified 2026-06-11 · arXiv-API-verified)

- HNSW · Malkov & Yashunin · arXiv:1603.09320 (canonical)
- Matryoshka Representation Learning · arXiv:2205.13147 (truncatable dims · R2)

## 5. Test strategy

Recall@k vs brute-force oracle on synthetic + real corpora (proptest dims) · boot-rebuild determinism · insert-after-build parity. Mutation ≥90%.

🦋 *Connectome satellite · « Memory is the function · the Connectome is the structure. »*
