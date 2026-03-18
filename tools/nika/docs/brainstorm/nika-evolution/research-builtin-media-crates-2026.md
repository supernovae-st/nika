# Research: Rust Crates for Builtin Media Capabilities (March 2026 Update)

> **Research Date**: 2026-03-18
> **Purpose**: Updated audit of Rust ML/media crates for Nika builtin tools
> **Method**: Perplexity searches + crates.io metadata + GitHub activity analysis
> **Supplements**: Doc 16 (multimodal-builtin-research) and Doc 17 (smart-router-multimodal-deep)

---

## Executive Summary

All eight crate families investigated are actively maintained as of March 2026.
The Rust ML ecosystem has matured significantly since the original Doc 16 research
(2026-03-15), with several version bumps and new entrants worth noting.

**Key changes since Doc 16**:

1. **mistral.rs** now at v0.6.0+ with Qwen 2.5 VL, Qwen 3 VL, LLaVA, Gemma 3 vision support
2. **whisper-rs** bumped to v0.16.0 (from v0.15.x) -- repo migrated to Codeberg
3. **candle** at v0.9.2 -- still no explicit NLLB-200 support in examples
4. **fastembed** at v5.12.1 -- strong alternative to mistral.rs for embeddings-only use cases
5. **ort** still at 2.0.0-rc.12 (pre-release) binding ONNX Runtime 1.24
6. **vicinity** crate discovered -- multi-algorithm ANN (HNSW + DiskANN + IVF-PQ + ScaNN)

**Recommendation changes**: None to the Doc 16/17 architecture. All choices validated.
Consider `fastembed` as Tier 2 alternative for `nika:embed` if mistral.rs is too heavy.

---

## 1. mistral.rs -- Vision + LLM Inference

**Role in Nika**: `nika:vision`, `nika:embed`, core `infer:` verb backend

| Field | Value |
|-------|-------|
| Crate | `mistralrs` |
| Latest version | v0.6.0+ (crates.io) |
| Last release | Jan 27, 2026 (PyPI); Rust docs updated March 2026 |
| GitHub stars | ~6,700+ |
| Maintenance | Highly active -- frequent releases through 2026 |
| License | MIT |

### Vision Models Supported

- Qwen2-VL, Qwen 2.5 VL, Qwen 3 VL
- LLaVA
- Phi-3-Vision / Phi-3.5-Vision
- Idefics 3, SmolVLM
- MiniCpm-O 2.6
- Gemma 3 (vision topology)
- Auto-detection from HuggingFace GGUF checkpoints

### Key Features

- **True multimodality**: Vision, audio (experimental), speech generation (experimental), embeddings
- **VisionModelBuilder API**: Dedicated builder for vision pipelines
- **Multimodal prefix caching**: Reuse image encodings across prompts
- **Quantization**: GGUF, AWQ, blockwise FP8; `mistralrs tune` auto-optimizes
- **Hardware backends**: CUDA, Metal, CPU (AVX2/AVX-512)
- **PagedAttention**: KV cache scheduling for throughput

### Performance

- 6x prompt performance boost over prior versions
- Faster or comparable to MLX/llama.cpp on matching hardware
- `mistralrs tune` auto-optimizes quantization and device mapping
- Interactive mode with real-time metrics

### Memory Footprint

- Model-dependent. Quantized GGUF models significantly reduce requirements
- Example: Qwen2-VL 7B Q4 fits in ~6 GB RAM
- PagedAttention reduces per-request overhead

### Relevance to Nika

Already integrated via `nika provider` system. Vision support validates the `nika:vision`
Tier 1 builtin design. No changes needed to the Doc 17 architecture.

**Source**: https://github.com/EricLBuehler/mistral.rs

---

## 2. whisper-rs -- Speech-to-Text

**Role in Nika**: `nika:transcribe` builtin tool

| Field | Value |
|-------|-------|
| Crate | `whisper-rs` |
| Latest version | **v0.16.0** |
| Sys crate | `whisper-rs-sys` v0.15.0 |
| Last published | March 13, 2026 (crates.io) |
| Total versions | 28 |
| Maintenance | Active, but **repo migrated to Codeberg** (GitHub archived) |
| License | MIT |

### Capabilities

- Rust bindings to whisper.cpp
- Supports all Whisper model sizes: tiny, base, small, medium, large, large-v3
- CPU inference with SIMD (AVX2, FMA); GPU offload via whisper.cpp
- Multi-threaded state management
- No native streaming -- batch/near-real-time processing

### Performance

- ~0.33x realtime on large model CPU (Xeon/Titan)
- ~90% realtime possible on 16-core VPS with quantization
- Apple Silicon (M1/M2) well-supported via whisper.cpp Metal backend

### Memory Footprint

- **~800 MB to 2 GB** depending on model size and config
- large-v3: ~1.7 GB RAM
- small: ~500 MB
- tiny: ~75 MB

### Breaking Changes

- Repo migrated from `tazz4843/whisper-rs` (GitHub) to Codeberg
- Future issues/PRs should go to Codeberg
- v0.16.0 may have API changes from v0.15.x (check changelog)

### Relevance to Nika

Validates `nika:transcribe` as Tier 2 builtin (default OFF, feature flag `transcribe`).
The `small` model (~500 MB) is a reasonable default for on-device transcription.
No MCP servers exist for STT, confirming the builtin approach.

**Source**: https://crates.io/crates/whisper-rs, https://codeberg.org/tazz4843/whisper-rs

---

## 3. candle -- ML Framework

**Role in Nika**: `nika:translate` (NLLB-200), potential image generation (diffusion)

| Field | Value |
|-------|-------|
| Crate | `candle-core` |
| Latest version | **v0.9.2** |
| Last published | January 24, 2026 |
| GitHub stars | 19,000+ |
| Maintenance | Highly active (Hugging Face official) |
| License | MIT/Apache-2.0 |

### Supported Models

**LLMs**: Llama 1/2/3, Mistral, Mixtral, Phi-1/2/3, Gemma 1/2/3, Qwen 1.5/2/2.5/3, Yi, Falcon,
StableLM, RWKV v5/v6, Mamba, GLM4

**Multimodal**: Stable Diffusion (1.5/2.1/SDXL/Turbo), Whisper

**Quantization**: Native GGUF, PagedAttention-like KV-cache in integrations

### NLLB-200 Support

**Not explicitly documented** in candle examples or README. Doc 16 states
"candle supports NLLB-200 natively" but this may require a custom implementation
using candle-transformers with the NLLB-200 ONNX/safetensors weights from HuggingFace.
The seq2seq architecture (marian/m2m100 family) is not in the default examples list.

**Risk**: Medium. NLLB-200 integration would likely need custom code on top of
candle-transformers, not a plug-and-play model loader.

### Key Features

- PyTorch-like API in Rust
- Hardware: CUDA, Metal, Vulkan, CPU (MKL/Accelerate)
- WASM builds for browser inference
- HuggingFace Hub integration with caching
- LoRA training support (via metal-candle)
- Small static binaries

### Memory Footprint

- Quantized GGUF models reduce significantly
- WASM-compatible implies small baseline
- No specific numbers; model-dependent

### Ecosystem Crates

- **candelabra**: Desktop app wrapper for quantized GGUF models
- **metal-candle**: Apple Silicon LoRA training/inference
- **aha**: Cross-platform AI inference engine built on candle

### Relevance to Nika

The `nika:translate` decision depends on NLLB-200 actually working in candle.
Needs a proof-of-concept before committing. Alternative: use `ort` to run
NLLB-200 ONNX model directly.

**Source**: https://github.com/huggingface/candle, https://crates.io/crates/candle-core

---

## 4. rten -- Pure Rust ONNX Inference

**Role in Nika**: `nika:speak` (Piper TTS inference), potential OCR/vision models

| Field | Value |
|-------|-------|
| Crate | `rten` |
| Latest version (parser) | `rten-onnx` **v0.24.0** (Dec 23, 2025) |
| Maintenance | Active -- 2025-2026 updates, GPU planned for 2026 |
| License | MIT/Apache-2.0 |

### Key Features

- **Pure Rust** ONNX inference -- no C++ dependencies
- CPU only (AVX2, AVX-512, Arm Neon, WASM SIMD)
- Multi-threaded by default (physical cores)
- Quantization: int8/uint8 activations/weights (VNNI on x86, UDOT on Arm), int4 blockwise
- Pre/post-processing libraries for vision/audio/text
- CTC decoding for sequence tasks

### Piper TTS Compatibility

rten can run Piper ONNX models. A demo exists showing:
- ~120ms generation on Intel i5 for 1.9s audio
- Real-time on Raspberry Pi 4/5
- Slower on Pi Zero
- Text-to-phoneme needs separate handling (espeak-ng or custom)

### Performance vs ort

| Aspect | rten | ort |
|--------|------|-----|
| Dependencies | Pure Rust | C++ ONNX Runtime |
| GPU support | CPU only (GPU 2026) | CUDA, Metal, CoreML, DirectML |
| Binary size | Smaller | Larger (ships ORT libs) |
| Operator coverage | Good, growing | Complete |
| Best for | Lightweight, WASM, edge | Production, GPU-heavy |

### Memory Footprint

- Small runtime overhead (pure Rust, no external deps)
- Quantized models reduce significantly
- WASM-compatible

### Relevance to Nika

Validates `nika:speak` as Tier 2 builtin using rten + Piper. The pure Rust
approach avoids C++ build complexity. GPU support coming in 2026 would improve
throughput for longer audio generation.

**Source**: https://github.com/robertknight/rten, https://robertknight.me.uk/posts/rten-2025/

---

## 5. Piper TTS Rust Ecosystem

**Role in Nika**: `nika:speak` model layer

| Crate | Version | Backend | Notes |
|-------|---------|---------|-------|
| `piper-rs` | latest 2024 | Pure Rust (ort) | All Piper models, multi-language, 28 GitHub stars |
| `piper-tts-rs` | v0.1.4 (Jan 2026) | C++ bindings | Direct bindings to Piper C++ |
| `piper-tts-rust` | v0.1.x (Aug 2025) | ort 2.0.0-rc.10 | Simple ONNX interface, no espeak-ng |

### Model Format

- ONNX (VITS-based neural TTS architecture)
- Models from HuggingFace `rhasspy/piper-voices`
- Lightweight: designed to run on Raspberry Pi

### Memory Footprint

- piper-tts-rs-sys: ~0-2.2 MB binary overhead
- Model sizes vary by language/quality (typically 15-60 MB per voice)

### Recommended Approach for Nika

Use **piper-rs** (pure Rust + ort) or **rten** directly with Piper ONNX models.
Avoids espeak-ng GPL dependency. The `piper-tts-rust` crate using ort is the
simplest integration path.

**Source**: https://github.com/thewh1teagle/piper-rs, https://crates.io/crates/piper-tts-rust

---

## 6. ocrs -- Pure Rust OCR

**Role in Nika**: `nika:ocr` builtin tool

| Field | Value |
|-------|-------|
| Crate | `ocrs` |
| Latest version | **v0.11.0** (~January 2026) |
| Total versions | 16 |
| Maintenance | Active (by Robert Knight, same author as rten) |
| License | MIT/Apache-2.0 |

### Key Features

- Pure Rust OCR using neural network models
- Models trained in PyTorch, exported to ONNX, run via **rten**
- Text detection + recognition pipeline
- Works on documents, photos, screenshots
- Zero/minimal preprocessing needed (unlike Tesseract)
- WASM support for browser use
- Currently **Latin alphabet only** (English); more languages planned

### Accuracy

- No published quantitative benchmarks
- Qualitatively: works well across diverse image types
- Less preprocessing needed than Tesseract
- Early preview -- expected errors vs commercial engines

### Memory Footprint

- Models from `ocrs-models` repo in ONNX format
- WASM-compatible implies reasonable size
- No specific numbers published

### Alternative: ocr-rs

| Aspect | ocrs | ocr-rs |
|--------|------|--------|
| Backend | rten (pure Rust) | PaddleOCR + MNN |
| Languages | Latin only | 100+ languages |
| GPU | CPU only | Metal, OpenCL, Vulkan |
| Version | 0.11.0 | 2.0 |
| Pure Rust | Yes | No (MNN dependency) |

### Relevance to Nika

Doc 16 lists `nika:ocr` as Tier 2 builtin with `ocr-rs (Tesseract)`. Updated
recommendation: use **ocrs** (pure Rust, same ecosystem as rten) for Latin text,
or **ocr-rs** for multi-language support. The pure Rust path (ocrs) aligns better
with Nika's zero-external-deps philosophy.

**Source**: https://github.com/robertknight/ocrs, https://crates.io/crates/ocrs

---

## 7. Vector Search -- HNSW / ANN

**Role in Nika**: `nika:search`, `nika:index` meta tools for native RAG

### Crate Comparison

| Crate | Version | Pure Rust | Key Feature | Maintained |
|-------|---------|-----------|-------------|------------|
| `instant-distance` | v0.6.1 | Yes | Minimal HNSW, rayon parallel | Moderate (11 versions) |
| `hnswlib-rs` | **v0.10.0** (Jan 2026) | Yes | Persistence, L2/cosine, config | Highly active |
| `hnsw` (rust-cv) | 22 versions | Yes | SIMD distances, Bits128-4096 | Active (Jan 2025) |
| `vicinity` | New (Feb 2026) | Yes | Multi-algo: HNSW, DiskANN, IVF-PQ, ScaNN | New, active |
| `usearch` | Active | No (C++ wrap) | USearch engine, custom metrics | Active |

### Recommendation Update

**Doc 16 chose `instant-distance`**. Updated assessment:

- **hnswlib-rs v0.10.0** is now the strongest pure-Rust HNSW option (most recent,
  persistence support, configurable M/ef parameters)
- **vicinity** is interesting for future -- multi-algorithm support could let users
  choose the best index type
- **instant-distance** remains viable but less actively developed

Suggest switching recommendation to **hnswlib-rs** for `nika:search`/`nika:index`.

**Source**: https://lib.rs/crates/hnswlib-rs, https://lib.rs/crates/vicinity

---

## 8. fastembed -- Local Embedding Generation

**Role in Nika**: Alternative/complement to mistral.rs for `nika:embed`

| Field | Value |
|-------|-------|
| Crate | `fastembed` |
| Latest version | **v5.12.1** |
| Total versions | 89 |
| Last published | March 10, 2026 |
| Maintainer | Qdrant |
| Maintenance | Highly active |
| License | MIT |

### Supported Models

- AllMiniLM-L6-V2 (384 dims, default)
- AllMiniLM-L12-V2 (384 dims)
- BGE-Small-EN-V1.5 (384 dims)
- BGE-Base-EN-V1.5 (768 dims)
- BGE-Large-EN-V1.5 (1024 dims)
- Nomic-Embed-Text-V1 (768 dims)
- Quantized variants (Q suffix)
- Chinese models (BGE-Small-ZH-V1.5, 512 dims)
- Reranking models
- Image embedding models
- Sparse embedding models

### Key Features

- Runs ONNX models locally (no API key needed)
- Uses ONNX Runtime under the hood
- Batched processing (default 256)
- Parallel processing
- Quantized model support
- `TextEmbedding::list_supported_models()` for discovery
- Used by SurrealDB, trueno-rag, rig-fastembed

### Memory Footprint

- Small models (AllMiniLM-L6-V2): ~90 MB
- Quantized variants reduce further
- No PyTorch dependency

### Relevance to Nika

**New finding not in Doc 16**: fastembed v5.12.1 is a much lighter alternative to
mistral.rs for embedding-only use cases. For `nika:embed`, if mistral.rs is already
loaded for `infer:`, use its embedding support. But for standalone `nika:embed`
without a full LLM loaded, fastembed is significantly lighter (~90 MB vs ~6 GB).

Consider a **tiered approach**:
- If mistral.rs is loaded: use its embeddings (EmbeddingGemma, Qwen 3 Embedding)
- If not: fall back to fastembed for lightweight local embeddings

**Source**: https://crates.io/crates/fastembed, https://docs.rs/fastembed

---

## 9. ort -- ONNX Runtime Bindings

**Role in Nika**: Backend for fastembed, piper-tts-rust, potential universal inference

| Field | Value |
|-------|-------|
| Crate | `ort` |
| Latest stable | 1.x (binding ONNX Runtime 1.23) |
| Latest pre-release | **2.0.0-rc.12** (binding ONNX Runtime 1.24) |
| Sys crate | `ort-sys` 2.0.0-rc.12 |
| Last published | March 4, 2026 |
| Maintenance | Highly active (pykeio) |
| GitHub stars | 1,600+ |
| License | MIT/Apache-2.0 |

### v2 Changes (Breaking)

- **New builder API**: `Session::builder().commit_from_file()` replaces `new_session_builder()`
- **Environment builder**: `Environment::builder()` pattern
- **Input macro**: `inputs!["data" => tensor]?`
- **Optimization levels**: `GraphOptimizationLevel::Level3`
- Docs recommend `ort = "=2.0.0-rc.12"` for new projects

### Execution Providers

- CPU (default, SIMD-optimized)
- CUDA (NVIDIA GPUs)
- Metal (Apple Silicon)
- CoreML (Apple devices)
- DirectML (Windows)
- TensorRT, OpenVINO, NNAPI, QNN

### Performance

- Hardware-accelerated via ONNX Runtime
- GraphOptimizationLevel::Level3 for max performance
- Multi-threaded (configurable intra_threads)
- Near-native C++ performance through bindings

### Relevance to Nika

ort is a transitive dependency through fastembed and piper-tts-rust. Nika does
not need to depend on ort directly unless building custom ONNX inference pipelines.
The 2.0.0-rc.12 is recommended even though pre-release -- it is described as
"production-ready, just not API-stable".

**Source**: https://github.com/pykeio/ort, https://ort.pyke.io

---

## 10. image -- Image Processing

**Role in Nika**: Image validation, thumbnails, format conversion for media pipeline

| Field | Value |
|-------|-------|
| Crate | `image` |
| Latest version | **v0.25.6** |
| Total versions | 117 |
| Last published | March 10, 2026 |
| Maintenance | Highly active (image-rs org) |
| License | MIT/Apache-2.0 |
| MSRV | 1.67.1+ |

### Supported Formats

**Read + Write**: JPEG, PNG, WebP, TIFF, ICO, BMP, GIF, PNM, TGA, DDS, QOI, AVIF, EXR

### Key Features for Nika

- **Format detection**: `guess_format()`, `Reader::with_guessed_format()`
- **Validation**: `image_dimensions()` for fast size check without full decode
- **Resizing**: `img.resize(w, h, FilterType::Lanczos3)` for thumbnails
- **Format conversion**: Load any format, save as another
- **DynamicImage**: Generic container for all color types
- **imageops module**: Flip, rotate, crop, filter, overlay
- **ICC profiles**: JPEG/PNG support
- Pure Rust implementations for all decoders

### Performance

- Release mode essential (debug mode ~10x slower)
- PNG decoder highlighted as particularly fast (Dec 2025 talk)
- ~2x slower than OpenCV for resizing operations
- Suitable for validation/thumbnails, not for heavy batch processing

### Memory

- Full image decode loads entire image into memory
- `image_dimensions()` reads only header -- negligible memory
- Streaming decode not supported for most formats

### Companion Crates

- **imageproc**: Advanced processing (edge detection, geometric transforms)
- **fast_image_resize**: SIMD-accelerated resizing (faster than image crate)
- **webp**: Dedicated WebP encoder/decoder

### Relevance to Nika

The `image` crate is the standard choice for the media pipeline (Doc 23). Use it for:
1. Validating image data from MCP server responses
2. Extracting dimensions/format metadata
3. Generating thumbnails for TUI preview
4. Format conversion (e.g., WebP to PNG for compatibility)

Already implicitly used by ocrs and other vision crates.

**Source**: https://crates.io/crates/image, https://docs.rs/image

---

## Summary Matrix

| Builtin Tool | Primary Crate | Version | Status | Feature Flag | Memory |
|-------------|--------------|---------|--------|-------------|--------|
| `nika:vision` | mistral.rs | v0.6.0+ | Stable | `vision` (ON) | ~6 GB (Q4 7B) |
| `nika:embed` | mistral.rs / fastembed | v0.6.0+ / v5.12.1 | Stable | `embed` (ON) | ~90 MB (fastembed) |
| `nika:transcribe` | whisper-rs | v0.16.0 | Stable | `transcribe` (OFF) | ~500 MB (small) |
| `nika:speak` | rten + piper-rs | v0.24.0 / latest | Stable | `speak` (OFF) | ~60 MB model |
| `nika:ocr` | ocrs | v0.11.0 | Preview | `ocr` (OFF) | TBD |
| `nika:translate` | candle + NLLB-200 | v0.9.2 | Needs POC | `translate` (OFF) | ~2 GB |
| `nika:search` | hnswlib-rs | v0.10.0 | Stable | via `embed` | ~10-50B/vec |
| `nika:index` | hnswlib-rs | v0.10.0 | Stable | via `embed` | ~10-50B/vec |
| Media pipeline | image | v0.25.6 | Stable | always | per-image |

---

## Action Items

1. **Validate NLLB-200 on candle**: Build POC for `nika:translate` -- the model
   may need custom code on top of candle-transformers. Alternative: run NLLB-200
   ONNX model via ort directly.

2. **Consider hnswlib-rs over instant-distance**: More recent, better maintained,
   persistence support. Evaluate API compatibility.

3. **Evaluate fastembed as lightweight embed fallback**: When mistral.rs is not
   loaded, fastembed provides ~90 MB embedding capability vs ~6 GB for a full LLM.

4. **Monitor whisper-rs Codeberg migration**: Ensure build scripts and CI point
   to the new Codeberg repo for future updates.

5. **Track ocrs language expansion**: Currently Latin-only. Multi-language support
   would eliminate the need for ocr-rs (which has MNN dependency).

6. **Track rten GPU support**: Expected 2026. Would significantly improve
   `nika:speak` throughput for long-form audio generation.

7. **Monitor vicinity crate**: Multi-algorithm ANN search could offer flexibility
   for `nika:search` in the future (DiskANN for large datasets, IVF-PQ for memory
   efficiency).

---

## Sources

1. https://github.com/EricLBuehler/mistral.rs -- mistral.rs repository
2. https://crates.io/crates/whisper-rs -- whisper-rs v0.16.0
3. https://github.com/huggingface/candle -- Candle ML framework
4. https://github.com/robertknight/rten -- rten ONNX inference
5. https://robertknight.me.uk/posts/rten-2025/ -- rten 2025 retrospective
6. https://github.com/robertknight/ocrs -- ocrs OCR engine
7. https://lib.rs/crates/hnswlib-rs -- hnswlib-rs v0.10.0
8. https://lib.rs/crates/vicinity -- vicinity multi-algo ANN
9. https://crates.io/crates/fastembed -- fastembed v5.12.1
10. https://github.com/pykeio/ort -- ort ONNX Runtime bindings
11. https://crates.io/crates/image -- image crate v0.25.6
12. https://github.com/thewh1teagle/piper-rs -- piper-rs TTS
13. https://crates.io/crates/piper-tts-rust -- piper-tts-rust
14. https://lib.rs/crates/ocr-rs -- ocr-rs (PaddleOCR-based)
15. https://crates.io/crates/instant-distance -- instant-distance v0.6.1

## Methodology

- Tools used: Perplexity AI search (sonar model), 10 queries
- Pages analyzed: ~80 search results across crates.io, GitHub, docs.rs, lib.rs
- Time period covered: 2025-01 to 2026-03-18

## Confidence Level

**High** for version numbers and maintenance status (sourced from crates.io).
**Medium** for NLLB-200 candle support (not explicitly confirmed in examples).
**Medium** for memory footprints (model-dependent, few published benchmarks).
