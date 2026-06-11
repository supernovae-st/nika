# Crate spec — `nika-autodesc-full`

| | |
|---|---|
| Status | **Phase M proposal** (Gate 1 · shipped 2026-06-11 · admission at its Phase-M step of the Connectome climb (ADR-004 + ADR-042 lineage)) |
| Layer | L1 — ingest satellite (M5 · ★ schema evolution) |
| LOC budget | ≤6,000 src |
| Crate version | tracks workspace |
| License | `AGPL-3.0-or-later` |
| Edition | 2024 |
| Publish | `false` initially · **publishable crates.io standalone** post-admission (per ADR-004 · external ecosystem value) |
| NIKA codes | `NIKA-CNX-*` family · exact range allocated at ceremony via `nika_error::codes` registry (never invented here) |

---

## 1. Purpose

Schema induction + entity resolution + hierarchical summarization —
the connectome grows its OWN ontology from what it ingests
(AutoSchemaKG pattern · no predefined schema) · clusters duplicate
entities (KGGen) · maintains RAPTOR-style hierarchical summaries and
A-MEM Zettelkasten-style linked notes. LLM-enrich is OPT-IN
(`feature = "llm-enrich"` per ADR-040) — the deterministic floor works
without any model.

## 2. Public API (sketch · Gate 1 level · final at ceremony)

```rust
pub struct SchemaEvolver { /* induction state */ }
impl SchemaEvolver {
    pub fn observe(&mut self, batch: &[Record]) -> SchemaDelta;
    pub fn resolve_entities(&self, store: &ConnectomeStore) -> Vec<MergeCandidate>;
    pub fn summarize(&self, community: &Community) -> Result<SummaryNode, NikaError>;
}
```

## 3. Dependencies (LOCKED · Connectome stack ratification 2026-06-11 · crates.io-verified)

builds on autodesc-minimal + graph-algos (Leiden communities feed summaries).

## 4. SOTA references (ratified 2026-06-11 · arXiv-API-verified)

- AutoSchemaKG · arXiv:2505.23628 (schema induction)
- RAPTOR · arXiv:2401.18059 (hierarchical abstractive retrieval trees)
- A-MEM · arXiv:2502.12110 (agentic Zettelkasten notes)

## 5. Test strategy

Schema-delta determinism on golden corpora · entity-resolution precision/recall fixtures · summary-tree shape invariants. Mutation ≥85% (Rule-2 exemption where llm-enrich gated).

🦋 *Connectome satellite · « Memory is the function · the Connectome is the structure. »*
