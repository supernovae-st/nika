# Crate spec — `nika-rerank`

| | |
|---|---|
| Status | **Phase M proposal** (Gate 1 · shipped 2026-06-11 · admission at its Phase-M step of the Connectome climb (ADR-004 + ADR-042 lineage)) |
| Layer | L1 — recall satellite (M13 · cross-encoder rerank · NEW · ratified 2026-06-11) |
| LOC budget | ≤3,000 src |
| Crate version | tracks workspace |
| License | `AGPL-3.0-or-later` |
| Edition | 2024 |
| Publish | `false` initially · **publishable crates.io standalone** post-admission (per ADR-004 · external ecosystem value) |
| NIKA codes | `NIKA-CNX-*` family · exact range allocated at ceremony via `nika_error::codes` registry (never invented here) |

---

## 1. Purpose

The precision tail — a LOCAL cross-encoder reranks the top-N fused
candidates (+20-30% precision over RRF alone, the 2024-25 standard).
Pure-candle (xlm-roberta arch · bge-reranker-v2-m3 weights · Qwen3-0.6B
alternative) · quantized CPU inference · zero cloud, zero FFI.

## 2. Public API (sketch · Gate 1 level · final at ceremony)

```rust
pub struct Reranker { /* candle model handle */ }
impl Reranker {
    pub fn load(model_dir: &Path) -> Result<Self, NikaError>;
    pub fn rerank(&self, query: &str, candidates: Vec<Candidate>, top_k: usize)
        -> Result<Vec<(Candidate, f32)>, NikaError>;
}
```

## 3. Dependencies (LOCKED · Connectome stack ratification 2026-06-11 · crates.io-verified)

`candle-core@0.10` + `candle-transformers` (xlm-roberta) · weights sovereign local-path load (the nika-ocr `with_models` precedent · no auto-download).

## 4. SOTA references (ratified 2026-06-11 · arXiv-API-verified)

- BGE-M3 family · arXiv:2402.03216 (bge-reranker-v2-m3 weights)
- Qwen3 Embedding/Reranker · arXiv:2506.05176 (Apache-2.0 · the 2025 local SOTA)

## 5. Test strategy

Golden score-parity fixtures vs reference implementation outputs · latency budget test (p95 ≤80ms/pair CPU quantized) · Rule-2 model-inference mutation exemption (the nika-ocr precedent).

## 6. Note · why a 10th satellite

The original 9-satellite pipeline stopped at RRF; every production retrieval stack 2024+
carries a reranker tail. Formalized as mechanism M13 (operator-ratified 2026-06-11).

🦋 *Connectome satellite · « Memory is the function · the Connectome is the structure. »*
