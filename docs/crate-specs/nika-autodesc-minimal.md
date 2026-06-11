# Crate spec — `nika-autodesc-minimal`

| | |
|---|---|
| Status | **Phase M proposal** (Gate 1 · shipped 2026-06-11 · admission at its Phase-M step of the Connectome climb (ADR-004 + ADR-042 lineage)) |
| Layer | L1 — ingest satellite (M9 · ★ THE MOAT) |
| LOC budget | ≤4,000 src |
| Crate version | tracks workspace |
| License | `AGPL-3.0-or-later` |
| Edition | 2024 |
| Publish | `false` initially · **publishable crates.io standalone** post-admission (per ADR-004 · external ecosystem value) |
| NIKA codes | `NIKA-CNX-*` family · exact range allocated at ceremony via `nika_error::codes` registry (never invented here) |

---

## 1. Purpose

The zero-LLM write path — deterministic ingest of records into the
connectome with PROV-O provenance attached to every triple (source ·
agent · activity · time · ed25519-signable). Write operations follow
the Mem0-validated pipeline (ADD / MERGE / DELETE with entity-cluster
matching per KGGen). **Ingest redaction guard (ADR-081-class)** ·
secret/PII patterns are refused at the write path — the engine's
Guard 1-2 lesson applied to memory. Embedding literals carry model
provenance (embedding-provenance invariant) — migration-safe by schema.

## 2. Public API (sketch · Gate 1 level · final at ceremony)

```rust
pub struct Ingestor { /* redaction rules · prov context */ }
impl Ingestor {
    pub fn add(&self, store: &mut ConnectomeStore, record: Record) -> Result<ProvenancedId, NikaError>;
    pub fn merge(&self, store: &mut ConnectomeStore, a: RecordId, b: RecordId) -> Result<RecordId, NikaError>;
    pub fn delete(&self, store: &mut ConnectomeStore, id: RecordId) -> Result<Tombstone, NikaError>;
}
```

## 3. Dependencies (LOCKED · Connectome stack ratification 2026-06-11 · crates.io-verified)

PROV-O vocabulary over oxigraph · `ed25519-dalek@2.2` (pin 2.x · 3.0-rc breaking imminent).

## 4. SOTA references (ratified 2026-06-11 · arXiv-API-verified)

- PROV-O · W3C 2013 (canonical)
- Mem0 · arXiv:2504.19413 (ADD/MERGE/DELETE ops pipeline)
- KGGen · arXiv:2502.09956 (entity-resolution clustering)

## 5. Test strategy

Provenance completeness invariant (NO triple without prov) · redaction guard negative tests (plant secrets · MUST refuse) · merge idempotence · signature round-trip. Mutation ≥90% · security-sensitive → proptest (Gate 6 MANDATORY here).

🦋 *Connectome satellite · « Memory is the function · the Connectome is the structure. »*
