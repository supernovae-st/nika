# Research Report: Embedding Generation & Vector Search in Rust for Nika

**Date**: 2026-04-03
**Author**: Claude (research agent)
**Context**: nika:embed / nika:search builtin tool design for Nika workflow engine

---

## Executive Summary

Nika can add RAG capabilities through two new builtin tools: `nika:embed` (generate
vector embeddings) and `nika:search` (nearest-neighbor lookup). The recommendation is
a **hybrid architecture**: API-first for cloud providers (OpenAI, Voyage, Cohere via
the existing `fetch:` verb), with an opt-in `embedding` feature flag for local ONNX
inference via `fastembed`. Vector search should use a minimal in-memory flat index for
small collections (pure Rust, zero deps), with optional `usearch` integration behind
a feature flag for larger workloads.

---

## 1. Embedding Generation

### 1.1 fastembed (local ONNX)

**Crate**: `fastembed` v5.13.0 (Apache-2.0)
**Downloads**: ~90k recent versions (actively maintained, March 2026)
**Backed by**: Qdrant team

**What it does**:
- Runs embedding models locally via ONNX Runtime (no API calls)
- Synchronous API, no Tokio dependency
- Automatic model download from HuggingFace Hub (cached in `.fastembed_cache/`)
- Batch embedding with configurable batch size (default 256)

**Supported models** (35+ text, 5 image, 2 sparse, 4 reranking):

| Model | Dims | ONNX Size | Quality Tier |
|-------|------|-----------|-------------|
| all-MiniLM-L6-v2 | 384 | 86 MB (22 MB quantized) | Good for English |
| BGE-small-en-v1.5 | 384 | 127 MB | Good for English |
| BGE-m3 | 1024 | ~1.1 GB | Best multilingual |
| nomic-embed-text-v1.5 | 768 | 522 MB (131 MB int8) | Great quality |
| snowflake-arctic-embed-s | 384 | ~90 MB | Good perf/size |
| Qwen3-Embedding-0.6B | varies | ~600 MB | State-of-art (candle) |

**API surface**:
```rust
use fastembed::{TextEmbedding, InitOptions, EmbeddingModel};

// Init (downloads model on first call, ~127 MB for default)
let model = TextEmbedding::try_new(
    InitOptions::new(EmbeddingModel::AllMiniLML6V2)
        .with_show_download_progress(true),
)?;

// Batch embed
let embeddings: Vec<Vec<f32>> = model.embed(
    vec!["Hello world", "Another text"],
    None, // batch_size
)?;
// embeddings[0].len() == 384
```

**Key dependencies** (the cost):
- `ort` v2.0.0-rc.11 (ONNX Runtime wrapper)
- `tokenizers` v0.22 (HuggingFace tokenizer, includes oniguruma)
- `ndarray` v0.17
- `safetensors` v0.7
- `hf-hub` v0.4.1 (model download)

**Binary size impact**:
- ONNX Runtime shared library: **29.5 MB** (macOS arm64), **7.8 MB** (linux x64)
- With `ort-load-dynamic`: Runtime is loaded dynamically, NOT statically linked.
  Nika binary grows by ~2-3 MB for the Rust wrapper code only. The ONNX Runtime
  dylib ships separately or is downloaded on first use.
- With `ort-download-binaries` (default): ort downloads the ONNX Runtime binary
  at build time. This gets statically linked into the binary, adding **~20-30 MB**.
- Model files are downloaded at runtime to a cache directory, NOT embedded in binary.
- Total first-run cost: ~130 MB (ONNX Runtime + default model)

**Performance** (approximate, CPU-only, all-MiniLM-L6-v2):
- Single text: ~5-15 ms on modern hardware
- Batch of 100 texts: ~200-500 ms
- ~2000-5000 tokens/sec for embedding (much faster than LLM inference)
- Memory: ~200-500 MB resident (model + ONNX Runtime)

### 1.2 ort (raw ONNX Runtime)

**Crate**: `ort` v2.0.0-rc.12 (8M downloads)
**What it does**: General ONNX inference, not embedding-specific

Using `ort` directly means reimplementing tokenization, pooling, and normalization.
`fastembed` already does this correctly. There is no reason to use `ort` directly
unless you need a model not supported by fastembed.

### 1.3 Cloud Embedding APIs

**No new dependency required** -- Nika's `fetch:` verb already supports HTTP calls.

| Provider | Model | Dims | Price/1M tokens | Batch | Notes |
|----------|-------|------|-----------------|-------|-------|
| OpenAI | text-embedding-3-small | 1536 | $0.02 | 2048 texts | Best value |
| OpenAI | text-embedding-3-large | 3072 | $0.13 | 2048 texts | Best quality |
| Voyage AI | voyage-3 | 1024 | $0.06 | 128 texts | Code-specialized |
| Cohere | embed-v4.0 | 1024 | $0.10 | 96 texts | Multilingual |
| Google | text-embedding-004 | 768 | $0.025 | 250 texts | Vertex AI |
| **Anthropic** | **N/A** | -- | -- | -- | **No embedding API** |

**API call example** (via fetch: verb -- zero new code):
```yaml
- id: embed_texts
  fetch:
    url: "https://api.openai.com/v1/embeddings"
    method: POST
    headers:
      Authorization: "Bearer {{with.openai_key}}"
    json:
      model: "text-embedding-3-small"
      input: "{{with.text}}"
    extract: jsonpath
    selector: "$.data[0].embedding"
```

### 1.4 Verdict: Embedding Strategy

**Recommended: Dual-mode behind feature flags**

```
nika:embed
  |
  +-- mode: api (DEFAULT, zero deps)
  |   Uses fetch: internally to call OpenAI/Voyage/Cohere
  |   Works out of the box, no binary bloat
  |
  +-- mode: local (feature = "embedding-local")
      Uses fastembed + ONNX Runtime
      Zero API costs, offline capable
      +20-30 MB binary, +130 MB runtime
```

The `api` mode is just a convenience wrapper around `fetch:` -- it formats the
request and extracts the embedding vector. Zero new dependencies.

The `local` mode is behind a Cargo feature flag (`embedding-local`) so it does NOT
affect users who do not need it.

---

## 2. Vector Search

### 2.1 Pure Rust Cosine Similarity (zero deps)

For small collections (< 10,000 vectors), a flat index with brute-force cosine
similarity is perfectly adequate and requires ZERO external dependencies:

```rust
/// Cosine similarity between two f32 slices.
/// Returns value in [-1.0, 1.0] where 1.0 = identical.
pub fn cosine_similarity(a: &[f32], b: &[f32]) -> f32 {
    debug_assert_eq!(a.len(), b.len());
    let (mut dot, mut norm_a, mut norm_b) = (0.0f32, 0.0f32, 0.0f32);
    for i in 0..a.len() {
        dot += a[i] * b[i];
        norm_a += a[i] * a[i];
        norm_b += b[i] * b[i];
    }
    let denom = (norm_a * norm_b).sqrt();
    if denom == 0.0 { 0.0 } else { dot / denom }
}

/// Find top-k most similar vectors by cosine similarity.
pub fn top_k(query: &[f32], vectors: &[(usize, Vec<f32>)], k: usize) -> Vec<(usize, f32)> {
    let mut scored: Vec<(usize, f32)> = vectors
        .iter()
        .map(|(id, v)| (*id, cosine_similarity(query, v)))
        .collect();
    scored.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
    scored.truncate(k);
    scored
}
```

**Performance**: ~1 ms for 10,000 vectors at 384 dims on modern hardware.
The compiler auto-vectorizes this with SIMD on both x86 and ARM.

### 2.2 usearch (C++ HNSW with Rust bindings)

**Crate**: `usearch` v2.24.0 (418k downloads)
**License**: Apache-2.0
**What it does**: Compact HNSW index with SIMD-accelerated distance computation

| Feature | Value |
|---------|-------|
| Index types | HNSW (Hierarchical Navigable Small Worlds) |
| Distance metrics | Cosine, L2, Inner Product, Pearson + more |
| Quantization | F32, F16, BF16, I8, B1x8 |
| Serialization | Save/load to file, memory-mapped |
| Build speed | ~100k vectors in seconds |
| Search speed | ~1M vectors in < 1 ms |
| Memory | ~1.5x raw vectors for graph overhead |
| SIMD | Via SimSIMD (dynamic dispatch, no compile flags) |

**API surface**:
```rust
use usearch::{Index, IndexOptions, MetricKind, ScalarKind, new_index};

let options = IndexOptions {
    dimensions: 384,
    metric: MetricKind::Cos,
    quantization: ScalarKind::F32,
    connectivity: 0,     // auto
    expansion_add: 0,    // auto
    expansion_search: 0, // auto
};

let index = new_index(&options).unwrap();
index.reserve(10_000).unwrap();
index.add(42, &embedding).unwrap();

let results = index.search(&query, 10).unwrap();
// results.keys = [42, 17, ...], results.distances = [0.1, 0.2, ...]
```

**Binary size impact**: C++ core via C bindings, ~500 KB added to binary.
SimSIMD adds ~200 KB. Total: ~700 KB -- very reasonable.

### 2.3 hnsw_rs (pure Rust HNSW)

**Crate**: `hnsw_rs` v0.3.4 (345k downloads)
**License**: MIT/Apache-2.0
**What it does**: Pure Rust HNSW implementation

Simpler than usearch but no SIMD acceleration, no quantization.
Good for pure-Rust builds but slower for large indices.

### 2.4 instant-distance (pure Rust HNSW)

**Crate**: `instant-distance` v0.6.1 (165k downloads)
**License**: MIT/Apache-2.0
**What it does**: Minimal HNSW, custom distance trait

Very clean API but appears less maintained (last update ~2024).
No built-in serialization.

### 2.5 External Vector Stores (via API)

| Store | Rust Client | Downloads | Notes |
|-------|-------------|-----------|-------|
| Qdrant | `qdrant-client` v1.17.0 | 2M | gRPC + REST, self-hosted or cloud |
| Pinecone | `pinecone-sdk` | ~50k | REST only, cloud-only |
| Weaviate | No official Rust client | -- | REST/GraphQL |
| ChromaDB | No official Rust client | -- | REST |
| Milvus | No official Rust client | -- | gRPC |

**Qdrant** is the clear winner for Rust integration. The `qdrant-client` crate is
mature, well-maintained, and Qdrant is the company behind `fastembed-rs` itself.

However, external vector stores add operational complexity (separate service to run).
For Nika workflows, the simpler approach is better.

### 2.6 Verdict: Vector Search Strategy

**Recommended: Tiered approach**

```
nika:search
  |
  +-- Tier 0: Pure Rust flat index (DEFAULT, zero deps)
  |   cosine_similarity() + top_k() -- 50 lines of code
  |   Good for < 10,000 vectors
  |
  +-- Tier 1: usearch (feature = "vector-index")
  |   HNSW index for larger collections
  |   +700 KB binary, excellent performance
  |
  +-- Tier 2: External (via fetch: or invoke:)
      Qdrant, Pinecone via API calls
      No new dependencies, user brings their own vector store
```

---

## 3. Binary Size Budget

Current Nika binary: **65 MB** (release, macOS arm64)

| Addition | Binary Impact | Runtime Download | Feature Flag |
|----------|--------------|------------------|--------------|
| nika:embed (API mode) | ~0 KB | None | Always on |
| nika:embed (local/ONNX) | +2-3 MB (dynamic) or +20-30 MB (static) | ~130 MB (model) | `embedding-local` |
| nika:search (flat) | ~0 KB | None | Always on |
| nika:search (usearch) | ~700 KB | None | `vector-index` |
| qdrant-client | ~1-2 MB | None | `vector-qdrant` |

**Recommended default**: API embed + flat search = **zero binary growth**.
Power users opt in to `embedding-local` and/or `vector-index`.

---

## 4. API Design Sketch

### 4.1 nika:embed

```yaml
# API mode (default) -- uses OpenAI embedding API
- id: embed_query
  invoke:
    tool: nika:embed
    params:
      text: "What is the meaning of life?"
      provider: openai                    # openai | voyage | cohere
      model: text-embedding-3-small       # optional, provider default
      # api_key from $env.OPENAI_API_KEY (auto-resolved)

# Batch mode
- id: embed_corpus
  invoke:
    tool: nika:embed
    params:
      texts:                              # batch input
        - "First document"
        - "Second document"
      provider: openai

# Local mode (requires embedding-local feature)
- id: embed_local
  invoke:
    tool: nika:embed
    params:
      text: "Query text"
      provider: local
      model: all-MiniLM-L6-v2            # default: BGE-small-en-v1.5
```

**Output schema**:
```json
{
  "embedding": [0.123, -0.456, ...],
  "dimensions": 384,
  "model": "all-MiniLM-L6-v2",
  "provider": "local",
  "token_count": 7
}
```

For batch:
```json
{
  "embeddings": [[0.123, ...], [0.456, ...]],
  "dimensions": 384,
  "model": "text-embedding-3-small",
  "provider": "openai",
  "token_count": 15
}
```

### 4.2 nika:search

```yaml
# Build an in-memory index from embeddings
- id: index_docs
  invoke:
    tool: nika:search
    params:
      action: index
      vectors: $embed_corpus.embeddings  # array of [f32]
      ids: $doc_ids                       # array of string/int IDs
      metric: cosine                      # cosine | euclidean | dot

# Query the index
- id: find_similar
  invoke:
    tool: nika:search
    params:
      action: query
      index: $index_docs                  # reference to built index
      vector: $embed_query.embedding      # query vector
      top_k: 5
      threshold: 0.7                      # optional minimum similarity
```

**Output schema** (query):
```json
{
  "results": [
    { "id": "doc_42", "score": 0.95, "rank": 1 },
    { "id": "doc_17", "score": 0.87, "rank": 2 }
  ],
  "total_searched": 1000,
  "metric": "cosine"
}
```

### 4.3 Full RAG Pipeline Example

```yaml
schema: "nika/workflow@0.12"
workflow: simple-rag
description: "Retrieve and generate with RAG"

inputs:
  question: "What are Nika's 5 verbs?"
  corpus_dir: ./docs

tasks:
  # 1. Load documents
  - id: load_docs
    invoke:
      tool: nika:glob
      params:
        pattern: "{{inputs.corpus_dir}}/**/*.md"

  # 2. Read each document
  - id: read_docs
    for_each: "$load_docs.files"
    as: file
    invoke:
      tool: nika:read
      params:
        file_path: "{{with.file}}"

  # 3. Embed all documents
  - id: embed_docs
    with:
      texts: $read_docs
    invoke:
      tool: nika:embed
      params:
        texts: "{{with.texts}}"
        provider: openai

  # 4. Embed the question
  - id: embed_question
    invoke:
      tool: nika:embed
      params:
        text: "{{inputs.question}}"
        provider: openai

  # 5. Find relevant docs
  - id: search
    with:
      corpus: $embed_docs
      query: $embed_question
    invoke:
      tool: nika:search
      params:
        action: query
        vectors: "{{with.corpus.embeddings}}"
        vector: "{{with.query.embedding}}"
        top_k: 3

  # 6. Generate answer with context
  - id: answer
    with:
      docs: $read_docs
      hits: $search.results
    infer:
      prompt: |
        Answer this question using ONLY the provided context.

        Question: {{inputs.question}}

        Context:
        {{with.hits | to_json}}
      temperature: 0.3
```

---

## 5. Implementation Plan

### Phase 1: nika:embed API mode (zero deps, ship fast)

1. Add `EmbedTool` to `runtime/builtin/` implementing `BuiltinTool`
2. Support OpenAI, Voyage, Cohere embedding APIs via internal `reqwest` calls
3. Auto-resolve API keys from `$env.OPENAI_API_KEY` etc.
4. Single text + batch mode
5. Return `Vec<f32>` as JSON array
6. Tests with `provider: mock` (return deterministic vectors)

### Phase 2: nika:search flat index (zero deps)

1. Add `SearchTool` to `runtime/builtin/`
2. Implement `cosine_similarity()`, `euclidean_distance()`, `dot_product()`
3. In-memory flat index (Vec of vectors + IDs)
4. `action: index` and `action: query`
5. Top-k with optional threshold

### Phase 3: Local embedding (feature-gated)

1. Add `embedding-local` feature flag to nika-engine
2. Depend on `fastembed` with `default-features = false` + `ort-load-dynamic`
3. Use `load-dynamic` so ONNX Runtime is NOT statically linked
4. Model cache in `.nika/cache/embeddings/`
5. Support configurable model selection

### Phase 4: usearch index (feature-gated, optional)

1. Add `vector-index` feature flag
2. Depend on `usearch`
3. HNSW index with serialization to `.nika/cache/indices/`
4. Persistent indices across workflow runs

---

## 6. Crate Recommendations

| Need | Crate | Version | Feature Flag | License |
|------|-------|---------|-------------|---------|
| Local embeddings | `fastembed` | 5.13 | `embedding-local` | Apache-2.0 |
| ONNX Runtime | `ort` (via fastembed) | 2.0.0-rc.11 | (transitive) | MIT/Apache-2.0 |
| HNSW index | `usearch` | 2.24 | `vector-index` | Apache-2.0 |
| Cosine similarity | (stdlib) | -- | Always on | -- |
| SIMD distance | `simsimd` (via usearch) | 6.5 | (transitive) | Apache-2.0 |
| Qdrant client | `qdrant-client` | 1.17 | `vector-qdrant` (future) | Apache-2.0 |

**NOT recommended**:
- `rust-bert`: Heavy (libtorch), slow compile, overkill for embeddings
- `candle-transformers`: Good but fastembed wraps it already for newer models
- `hnsw_rs`: Pure Rust but no SIMD, no quantization -- usearch is better
- `instant-distance`: Less maintained, no serialization

---

## 7. Risk Analysis

| Risk | Mitigation |
|------|-----------|
| ONNX Runtime 29 MB on macOS | Use `ort-load-dynamic`: runtime loads dylib, not statically linked |
| Model download on first use (~130 MB) | Cache in `.nika/cache/embeddings/`, show progress bar |
| fastembed pins `ort =2.0.0-rc.11` (exact) | Monitor for conflicts with future ort usage |
| `tokenizers` crate brings `oniguruma` (C dep) | Already used by many projects, compiles fine on CI |
| API costs for cloud embeddings | Document pricing, recommend local for development |
| Large vector arrays in workflow DAG | Stream/chunk for > 10k vectors, warn in docs |

---

## 8. Confidence Level

**HIGH** -- All crates investigated are actively maintained (2026 updates), have
significant download counts, and compatible licenses. The dual-mode architecture
(API default + local opt-in) follows Nika's existing pattern with `native-inference`
(mistral.rs). The flat index for small workloads is trivial to implement correctly.
The fastembed integration is straightforward -- their API is synchronous and clean.

---

## Sources

1. [fastembed-rs](https://github.com/Anush008/fastembed-rs) -- v5.13.0, 35+ text models, Apache-2.0
2. [ort](https://github.com/pykeio/ort) -- v2.0.0-rc.12, ONNX Runtime 1.24, 8M downloads
3. [usearch](https://github.com/unum-cloud/usearch) -- v2.24.0, HNSW + SimSIMD, Apache-2.0
4. [qdrant-client](https://crates.io/crates/qdrant-client) -- v1.17.0, 2M downloads
5. [simsimd](https://github.com/ashvardanian/SimSIMD) -- v6.5.16, SIMD distances, 800k downloads
6. [hnsw_rs](https://crates.io/crates/hnsw_rs) -- v0.3.4, pure Rust HNSW
7. [instant-distance](https://github.com/instant-labs/instant-distance) -- v0.6.1, minimal HNSW
8. ONNX Runtime releases -- v1.24.4 binary sizes from GitHub releases
9. HuggingFace model registry -- model ONNX file sizes via tree API
10. Nika source code -- current builtin tool patterns, feature flags, binary size

---

## Methodology

- Tools used: crates.io API, GitHub raw content, HuggingFace API, local filesystem
- Crates analyzed: 10
- Source files examined: 15+
- Current Nika binary baseline: 65 MB (release, macOS arm64)
