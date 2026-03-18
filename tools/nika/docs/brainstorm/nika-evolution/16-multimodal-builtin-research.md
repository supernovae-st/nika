# 16 -- Multi-Modal Builtin Tools Research

> **Research Date**: 2026-03-15
> **Scope**: mistral.rs v0.7.0 capabilities + Rust ML ecosystem for expanding Nika's native multi-modal abilities
> **Method**: Git sparse-checkout + Perplexity searches + GitHub API stats across 8 Rust ML crates
> **Status**: RESEARCH COMPLETE + DECISIONS VALIDATED (Doc 17)

---

## Executive Summary

**Key Finding**: mistral.rs v0.7.0 already supports vision (Qwen 3-VL, Gemma 3n) and embeddings (Qwen 3 Embedding, EmbeddingGemma), making it a strong foundation for multi-modal Nika. Image generation and audio require separate engines.

**Critical Discovery**: mistral.rs v0.7.0 PR #1841 added "auto pipeline for diffusion and speech models" -- meaning the framework now has infrastructure for image generation AND speech, though these are still experimental.

**Recommendation**: A hybrid architecture:
1. **mistral.rs** for text + vision + embeddings (already integrated)
2. **whisper-rs** for STT (speech-to-text)
3. **diffusion-rs** or candle for image generation
4. **rten + piper** for TTS (text-to-speech)
5. **candle + NLLB-200** for translation (200 languages offline)
6. **instant-distance** for native RAG (in-memory HNSW vector search)

---

## Validated Decisions (2026-03-15)

Decisions validated during interactive brainstorm session. See Doc 17 for full architecture.

| Tool | Decision | Engine | Feature Flag |
|------|----------|--------|-------------|
| `nika:vision` | Tier 1 builtin | mistral.rs (Qwen 3-VL) | `vision` (default ON) |
| `nika:embed` | Tier 1 builtin | mistral.rs (EmbeddingGemma) | `embed` (default ON) |
| `nika:transcribe` | Tier 2 builtin | whisper-rs | `transcribe` (default OFF) |
| `nika:speak` | Tier 2 builtin | rten + Piper | `speak` (default OFF) |
| `nika:ocr` | Tier 2 builtin | ocr-rs (Tesseract) | `ocr` (default OFF) |
| `nika:translate` | Tier 2 builtin (NEW) | candle + NLLB-200 | `translate` (default OFF) |
| `nika:imagine` | Tier 3 MCP router | ComfyUI/Replicate/FAL | Always available |
| `nika:search` | Meta tool (NEW) | nika:embed + instant-distance | Requires `embed` |
| `nika:index` | Meta tool (NEW) | nika:embed + instant-distance | Requires `embed` |

**New architectural pattern**: Smart Router -- unified dispatch logic (builtin > MCP > LLM fallback > error) for all multi-modal tools. See Doc 17 Section 1.

**New research findings (added)**:
- candle supports NLLB-200 natively (Rust, zero Python deps) -- enables `nika:translate`
- instant-distance crate (pure Rust HNSW) -- enables `nika:search` native RAG
- ComfyUI MCP ecosystem mature (219 stars, 13+ tools) -- validates `nika:imagine` MCP approach
- ZERO MCP servers for STT/TTS confirmed -- validates Tier 2 builtin for transcribe/speak
- mistral.rs does NOT yet support Gemma 3n audio -- Path 2 (audio: field) is future

---

## Part 1: mistral.rs v0.7.0 Capabilities

### GitHub Stats

| Metric | Value |
|--------|-------|
| Repository | [EricLBuehler/mistral.rs](https://github.com/EricLBuehler/mistral.rs) |
| Stars | 6,690 |
| Latest Version | v0.7.0 (2026-01-28) |
| Last Commit | 2026-03-14 |
| crates.io | `mistralrs = "0.7"` |

### Supported Modalities

```
+-------------------+------------------+------------------------------------------+
|     Modality      |     Supported?   |                 Models                   |
+-------------------+------------------+------------------------------------------+
| Text Generation   |       YES        | Llama 4, Qwen 3, Gemma 3n, Mistral,     |
|                   |                  | DeepSeek v3, GLM-4, Phi-4, SmolLM3      |
+-------------------+------------------+------------------------------------------+
| Vision (VLM)      |       YES        | Qwen 3-VL, Qwen 3-VL MoE, Gemma 3n,    |
|                   |                  | Qwen 2.5-VL                             |
+-------------------+------------------+------------------------------------------+
| Embeddings        |       YES        | Qwen 3 Embedding, EmbeddingGemma        |
|                   |       (v0.7)     |                                         |
+-------------------+------------------+------------------------------------------+
| Audio Input       |       YES        | Gemma 3n (audio encoder)                |
|                   |       (v0.7)     |                                         |
+-------------------+------------------+------------------------------------------+
| Diffusion (exp.)  |    INFRA ONLY    | Auto pipeline added (PR #1841)          |
|                   |       (v0.7)     | Not yet production-ready                |
+-------------------+------------------+------------------------------------------+
| Speech (exp.)     |    INFRA ONLY    | Auto pipeline added (PR #1841)          |
|                   |       (v0.7)     | Not yet production-ready                |
+-------------------+------------------+------------------------------------------+
| TTS               |       NO         | Not supported (use piper/rten)          |
+-------------------+------------------+------------------------------------------+
| STT/Whisper       |       NO         | Not supported (use whisper-rs)          |
+-------------------+------------------+------------------------------------------+
```

### v0.7.0 Highlights

- **Prefix Caching** -- Accelerates multi-turn conversations and RAG via KV cache reuse
- **Dynamic Model Loading** -- Load/unload models at runtime
- **Embedding Models** -- Qwen 3 Embedding, EmbeddingGemma for vector search
- **Vision Models** -- Qwen 3-VL (including MoE variant)
- **Audio Processing** -- Gemma 3n audio encoder with normalize/fade functions
- **Auto Pipeline** -- Infrastructure for diffusion + speech models (experimental)
- **CUDA 13.0/13.1** -- Latest NVIDIA support
- **candle 0.9.2** -- Migrated to official crates.io release
- **MCP Support** -- Built-in MCP client integration

### Nika Integration Status

Nika currently uses mistral.rs v0.7 for text generation only:

```rust
// tools/nika/Cargo.toml
mistralrs = { version = "0.7", optional = true }  // native-inference feature

// tools/nika/src/provider/native/runtime.rs
use mistralrs::{GgufModelBuilder, Model, RequestBuilder, TextMessages};
```

**Expansion potential:**
- Vision: Use `ImageMessages` or OpenAI-compatible image_url
- Embeddings: Use EmbeddingGemma or Qwen 3 Embedding models
- Audio: Use Gemma 3n audio encoder (limited)

---

## Part 2: Rust ML Ecosystem Survey

### Comparison Matrix

```
+----------------+--------+--------+--------+---------+-------+-------+--------+
|     Crate      | Stars  | Text   | Vision | Image   | Embed | STT   | TTS    |
|                |        | Gen    |        | Gen     |       |       |        |
+----------------+--------+--------+--------+---------+-------+-------+--------+
| mistral.rs     | 6,690  |   +++  |  ++    |    ~    |  ++   |   -   |   -    |
| candle         | 19,674 |   ++   |  ++    |   ++    |  ++   |   +   |   +    |
| burn           | 14,598 |   +    |  ++    |   +     |  +    |   +   |   +    |
| ort            | 2,073  |   ++   |  ++    |   +     |  ++   |  ++   |  ++    |
| rten           | 294    |   +    |  ++    |    -    |  ++   |  ++   |  ++    |
| whisper-rs     | 936    |   -    |   -    |    -    |   -   | +++   |   -    |
| diffusion-rs   | 40     |   -    |   -    |  ++     |   -   |   -   |   -    |
| diffusers-rs   | 587    |   -    |   -    |  ++     |   -   |   -   |   -    |
+----------------+--------+--------+--------+---------+-------+-------+--------+

Legend: +++ = best-in-class | ++ = good | + = possible | ~ = experimental | - = N/A
```

### candle (HuggingFace)

| Attribute | Value |
|-----------|-------|
| Repository | [huggingface/candle](https://github.com/huggingface/candle) |
| Stars | 19,674 |
| crates.io | `candle-core`, `candle-nn`, `candle-transformers` |
| Latest | v0.9.2 |

**Capabilities:** Text generation with quantization, vision models, **Stable Diffusion** inference, embedding models (all-minilm, bge), CPU/GPU (CUDA/Metal) support.

**Relevance:** Powers mistral.rs backend. Could be used directly for SD inference.

### burn

| Attribute | Value |
|-----------|-------|
| Repository | [tracel-ai/burn](https://github.com/tracel-ai/burn) |
| Stars | 14,598 |
| Last commit | 2026-03-14 |

**Capabilities:** PyTorch-like API in Rust, CNN/Transformer backends, GPU/edge deployment.

**Relevance:** Research-grade, not directly useful for Nika builtins yet.

### ort (ONNX Runtime)

| Attribute | Value |
|-----------|-------|
| Repository | [pykeio/ort](https://github.com/pykeio/ort) |
| Stars | 2,073 |
| Last commit | 2026-03-14 |

**Capabilities:** Run any ONNX model (LLMs, vision, embeddings), GPU/NPU/DirectML acceleration, Whisper ONNX exports, TTS ONNX models.

**Relevance:** Good for `nika:embed` if we want ONNX-based models (fastembed pattern).

### rten

| Attribute | Value |
|-----------|-------|
| Repository | [robertknight/rten](https://github.com/robertknight/rten) |
| Stars | 294 |

**Capabilities:** Pure Rust ONNX inference, ONNX → .rten conversion, Whisper support, **Piper TTS demo**, CPU-optimized SIMD.

**Relevance:** Best candidate for `nika:speak` (TTS) via Piper models.

### whisper-rs

| Attribute | Value |
|-----------|-------|
| Repository | [tazz4843/whisper-rs](https://github.com/tazz4843/whisper-rs) |
| Stars | 936 |

**Capabilities:** OpenAI Whisper (tiny → large-v3), real-time transcription, word-level timestamps, VAD, speaker diarization, GPU acceleration (Metal, CUDA).

**Relevance:** Best candidate for `nika:transcribe` (STT). Mature, well-maintained.

### diffusion-rs

| Attribute | Value |
|-----------|-------|
| Repository | [newfla/diffusion-rs](https://github.com/newfla/diffusion-rs) |
| Stars | 40 |
| Last commit | 2026-03-13 |

**Capabilities:** SD 1.5, 2.1, SDXL via stable-diffusion.cpp bindings, Vulkan acceleration.

**Relevance:** Only actively-maintained Rust image gen crate. But memory-intensive.

### diffusers-rs

| Attribute | Value |
|-----------|-------|
| Repository | [LaurentMazare/diffusers-rs](https://github.com/LaurentMazare/diffusers-rs) |
| Stars | 587 |
| Last commit | 2024-04-04 |

**Capabilities:** SD 1.5/2.1, text-to-image, image-to-image, inpainting, ControlNet.

**Relevance:** Stale since April 2024. Not recommended.

---

## Part 3: Proposed Builtin Tools

### Feasibility Assessment

```
+------------------+-------------+----------+----------+---------------------------+
|       Tool       | Feasibility | Memory   | CPU OK?  | Backend                   |
+------------------+-------------+----------+----------+---------------------------+
| nika:embed       |    HIGH     | 0.5-2GB  |   YES    | mistral.rs v0.7           |
| nika:vision      |    HIGH     | 4-8GB    |   YES    | mistral.rs v0.7           |
| nika:transcribe  |    HIGH     | 1-3GB    |   YES    | whisper-rs (mature)       |
| nika:speak       |   MEDIUM    | 0.2-1GB  |   YES    | rten + piper              |
| nika:ocr         |    HIGH     | 0.4GB    |   YES    | ocr-rs (pure Rust)        |
| nika:imagine     |    LOW      | 8-16GB+  |   NO     | diffusion-rs / candle     |
| nika:translate   |    LOW      | varies   |   YES    | No mature Rust crate      |
+------------------+-------------+----------+----------+---------------------------+
```

### Memory Budget (16GB MacBook)

```
Comfortable (sum < 8GB):            Uncomfortable (sum > 10GB):
  LLM (7B Q4) ........ 4-6GB         LLM (7B) ........... 6GB
  Embeddings ......... 0.5-1GB        Vision model ....... 4GB  << risk
  Whisper small ...... 1GB            Image gen (SDXL) ... 8GB+ << NOT FEASIBLE
  TTS (Piper) ....... 0.3GB
  OCR ............... 0.4GB
  ─────────────────────
  Total: ~7GB (OK)
```

**Conclusion**: Image generation is NOT practical for a CLI tool on 16GB. Use MCP or cloud APIs.

### Builtin vs MCP Decision Matrix

```
+------------------+----------------+----------------+----------------------------+
|    Capability    |    Builtin?    |      MCP?      |           Why?             |
+------------------+----------------+----------------+----------------------------+
| Text Generation  |      YES       |       -        | Core functionality         |
| Embeddings       |      YES       |       -        | Low latency for RAG        |
| Vision/VLM       |      YES       |       -        | mistral.rs already has it  |
| STT (Whisper)    |    MAYBE       |      YES       | Memory tradeoff            |
| TTS              |    MAYBE       |      YES       | Piper works but optional   |
| OCR              |      YES       |       -        | Lightweight, useful        |
| Image Generation |       NO       |      YES       | Too memory-intensive       |
| Translation      |       NO       |      YES       | No good Rust crate         |
+------------------+----------------+----------------+----------------------------+
```

### Architecture Pattern

```
BUILTIN TOOLS (nika:*)               MCP SERVERS (invoke:)
──────────────────────               ─────────────────────

  Nika Process                         Nika Process
  ┌─────────────────────┐              ┌─────────────────────┐
  │  mistral.rs runtime │              │  MCP Client         │
  │  whisper-rs         │              └──────────┬──────────┘
  │  ocr-rs             │                         │ MCP Protocol
  │  rten/piper         │              ┌──────────┴──────────┐
  └─────────────────────┘              │  MCP Server         │
                                       │  ├── comfyui-mcp    │
  + Lower latency (in-process)         │  ├── replicate-mcp  │
  + Single binary distribution         │  └── deepl-mcp      │
  - Higher memory usage                └─────────────────────┘
  - Larger binary size
  - Feature flag complexity            + Modular (add/remove)
                                       + Separate memory space
                                       + Language-agnostic
                                       - Higher latency (IPC)
```

---

## Part 4: Multi-Modal Native -- Tiered Approach

### Definition: What "Truly Multi-Modal Native" Means

```
TIER 1 (Essential -- v0.31/v0.32 target):
  ├── Text generation ........... mistral.rs (existing)
  ├── Vision/VLM ................ mistral.rs (Qwen 3-VL)
  └── Embeddings ................ mistral.rs (EmbeddingGemma)

TIER 2 (Useful -- v0.33 target):
  ├── STT/Transcription ......... whisper-rs
  ├── OCR ....................... ocr-rs
  └── TTS ....................... rten + piper

TIER 3 (Nice-to-have -- MCP preferred):
  ├── Image generation .......... MCP (ComfyUI, Replicate)
  ├── Video generation .......... MCP (cloud)
  └── Translation ............... MCP (DeepL, Google)
```

### Minimum Viable Multi-Modal Workflow

```yaml
schema: nika/workflow@0.10

tasks:
  # Vision analysis (builtin, Tier 1)
  - id: analyze_screenshot
    infer:
      provider: native
      model: qwen3-vl-7b.gguf
      prompt: "Describe what you see in this image"
      image: ./screenshot.png

  # Embeddings (builtin, Tier 1)
  - id: embed_description
    nika:embed:
      model: embedding-gemma-2b.gguf
      input: $analyze_screenshot
    use.vector: embedding

  # Transcribe audio (builtin, Tier 2)
  - id: transcribe_meeting
    nika:transcribe:
      model: whisper-medium
      audio: ./meeting.mp3
    use.text: transcript

  # Image generation (MCP, Tier 3)
  - id: generate_image
    invoke:
      tool: comfyui:text_to_image
      params:
        prompt: "Based on: {{analyze_screenshot}}"
        model: sdxl
    use.artifact: generated_image
```

---

## Part 5: Implementation Roadmap

### Feature Flags

```toml
# Cargo.toml feature flags

[features]
default = ["native-inference"]

# Core (already exists)
native-inference = ["dep:mistralrs"]

# Tier 1: Multi-modal via mistral.rs
native-vision = ["native-inference"]   # Uses mistral.rs VLM
native-embed = ["native-inference"]    # Uses mistral.rs embeddings

# Tier 2: Additional engines
native-stt = ["dep:whisper-rs"]
native-tts = ["dep:rten"]
native-ocr = ["dep:ocr-rs"]

# Bundle
multimodal = ["native-vision", "native-embed", "native-stt", "native-tts", "native-ocr"]
```

### Phase Timeline

| Phase | Tools | Backend | Feature Flag |
|-------|-------|---------|-------------|
| Phase 1 | `nika:vision`, `nika:embed` | mistral.rs v0.7 | `native-vision`, `native-embed` |
| Phase 2 | `nika:transcribe` | whisper-rs | `native-stt` |
| Phase 3 | `nika:ocr` | ocr-rs | `native-ocr` |
| Phase 4 | `nika:speak` | rten + piper | `native-tts` |
| MCP | `comfyui:*`, `replicate:*`, `deepl:*` | External servers | N/A |

---

## Part 6: Image Generation Deep Dive

### Why NOT Builtin

| Library | Models | Memory | Speed | Status |
|---------|--------|--------|-------|--------|
| diffusion-rs | SD 1.5, 2.1, SDXL | 6-16GB | Medium | Active (40 stars) |
| diffusers-rs | SD 1.5, 2.1 | 6-12GB | Medium | Stale (April 2024) |
| candle (examples) | SD | 6-12GB | Medium | Examples only |

**FLUX status**: No Rust crate supports FLUX as of March 2026. Requires 12GB+ VRAM.

**Memory reality on 16GB Mac**:
```
SDXL alone:      ~12GB peak
LLM + SDXL:      ~18GB+ (NOT FEASIBLE)
FLUX.1-dev:      ~16GB+ (IMPOSSIBLE on 16GB)
```

**Verdict**: Image generation stays as MCP. Use ComfyUI, Replicate, or FAL via `invoke:`.

---

## Part 7: The Verb Question

### Do We Need New Verbs for Multi-Modal?

**Answer: NO.** The 5 existing verbs cover all multi-modal patterns:

```
infer: (with vision)  → LLM sees images natively via provider: native
nika:embed            → Builtin tool (called via tool infrastructure)
nika:transcribe       → Builtin tool
nika:speak            → Builtin tool
nika:ocr              → Builtin tool
invoke: comfyui:*     → MCP for image gen
```

Multi-modal capabilities are **tools** (builtin `nika:*` or MCP `invoke:`), not new verbs. The verbs describe the *action shape* (generate text, run command, HTTP request, MCP call, agent loop), while tools describe *capabilities*.

### How It Works

```
Verb = "what kind of action"          Tool = "what capability"
─────────────────────────             ────────────────────────
infer:    → Generate text/vision      nika:embed → Create vectors
exec:     → Run shell command         nika:transcribe → Audio→Text
fetch:    → HTTP request              nika:speak → Text→Audio
invoke:   → Call MCP tool             nika:ocr → Image→Text
agent:    → Agentic loop              comfyui:* → Text→Image
```

**Key insight**: `infer:` already handles vision when the model supports it (Qwen 3-VL). Adding `image:` field to infer params is sufficient -- no `vision:` verb needed.

---

## Industry Comparison

| Framework | Approach | Multi-Modal Pattern |
|-----------|----------|---------------------|
| **LangChain** | Tool-based | External APIs, some local via Transformers |
| **CrewAI** | Agent teams | Tools invoke external services |
| **Dify** | Workflow | Builtin nodes + plugins for capabilities |
| **n8n** | Workflow | API nodes for all capabilities |
| **AutoGen** | Multi-agent | Tool calling to external services |
| **Nika** | **Proposed** | **Builtin tools (Tier 1-2) + MCP (Tier 3)** |

**Industry trend**: Most frameworks use plugin/tool architecture for multi-modal, treating capabilities as external services rather than new action types.

---

## Sources

| Source | Stars | What It Provided |
|--------|-------|------------------|
| [mistral.rs](https://github.com/EricLBuehler/mistral.rs) | 6,690 | Vision, embeddings, auto pipeline for diffusion/speech |
| [candle](https://github.com/huggingface/candle) | 19,674 | SD examples, embedding models, tensor ops |
| [burn](https://github.com/tracel-ai/burn) | 14,598 | Next-gen DL framework (research) |
| [ort](https://github.com/pykeio/ort) | 2,073 | ONNX Runtime, universal model format |
| [whisper-rs](https://github.com/tazz4843/whisper-rs) | 936 | Best Rust STT (whisper.cpp bindings) |
| [piper](https://github.com/rhasspy/piper) | 10,682 | Fast local neural TTS |
| [diffusion-rs](https://github.com/newfla/diffusion-rs) | 40 | SD bindings (active, stable-diffusion.cpp) |
| [diffusers-rs](https://github.com/LaurentMazare/diffusers-rs) | 587 | SD API in Rust (stale since 2024) |
| [rten](https://github.com/robertknight/rten) | 294 | Pure Rust ONNX with Piper TTS demo |

## Methodology

- **Perplexity AI**: 3 deep searches on mistral.rs multi-modal, Rust image gen, Rust ML ecosystem
- **GitHub API**: Direct stats for 8 repositories (stars, last commit, descriptions)
- **Scope**: Rust-only crates with active maintenance (last commit < 6 months)
- **Confidence**: HIGH -- based on direct source analysis + real-time GitHub stats
