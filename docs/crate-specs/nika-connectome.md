# Crate spec — `nika-connectome`

| | |
|---|---|
| Status | **Phase M proposal** (Gate 1 · shipped 2026-06-11 · admission at its Phase-M step of the Connectome climb (ADR-004 + ADR-042 lineage)) |
| Layer | L2 — THE ORCHESTRATOR |
| LOC budget | ≤10,000 src |
| Crate version | tracks workspace |
| License | `AGPL-3.0-or-later` |
| Edition | 2024 |
| Publish | `false` initially · **publishable crates.io standalone** post-admission (per ADR-004 · external ecosystem value) |
| NIKA codes | `NIKA-CNX-*` family · exact range allocated at ceremony via `nika_error::codes` registry (never invented here) |

---

## 1. Purpose

The cognitive organ's conductor — owns the Oxigraph store (in-memory
+ N-Quads snapshot per the ratified-stack honesty note · `feature = \"rocksdb\"`
opt-in later), composes the recall pipeline (BM25 ∥ HNSW → RRF →
rerank M13 → **context assembly M15** · token-budgeted packing with
dedup + diversity), routes ingest through autodesc, exposes the
GraphRAG query path (M11 · LazyGraphRAG-style local-NLP index ·
deferred summarization), and schedules FSRS review sweeps. Ships the
**memory bench harness** (LongMemEval + LoCoMo · sister of the spec
repo's eval/) at R2.

## 2. Public API (sketch · Gate 1 level · final at ceremony)

```rust
pub struct Connectome { /* store + satellites */ }
impl Connectome {
    pub fn open(path: &Path) -> Result<Self, NikaError>;          // snapshot load
    pub fn recall(&self, query: RecallQuery) -> Result<AssembledContext, NikaError>;
    pub fn ingest(&mut self, record: Record) -> Result<ProvenancedId, NikaError>;
    pub fn sparql(&self, q: &str) -> Result<QueryResults, NikaError>;
    pub fn snapshot(&self) -> Result<PathBuf, NikaError>;          // N-Quads dump
}
```

## 3. Dependencies (LOCKED · Connectome stack ratification 2026-06-11 · crates.io-verified)

`oxigraph@0.5.8` + all Phase-M satellites (downward deps only · L2→L1).

## 4. SOTA references (ratified 2026-06-11 · arXiv-API-verified)

- GraphRAG · arXiv:2404.16130 · LazyGraphRAG (MSR blog 2024-11 · 0.1% index cost)
- MemOS · arXiv:2507.03724 (lifecycle scheduling informs the sweep design)
- Benchmarks · LongMemEval arXiv:2410.10813 · LoCoMo arXiv:2402.17753

## 5. Test strategy

End-to-end recall quality gates on bench corpora · snapshot round-trip (open→ingest→snapshot→open = identical SPARQL results) · context-assembly budget invariants (never exceeds token budget · dedup holds). Mutation ≥85%.

## 6. Error prefix

`NIKA-CNX-NNN` (Phase-M reservation) · allocated in the `nika_error::codes` registry at ceremony.

🦋 *Connectome satellite · « Memory is the function · the Connectome is the structure. »*
