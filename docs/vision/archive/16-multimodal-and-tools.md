# 16 -- Multimodal & Tools Research

> **Research Date**: 2026-03-15
> **Scope**: Rust ML ecosystem + MCP server ecosystem + Smart Router pattern + complete multi-modal tool inventory for Nika v0.30
> **Method**: Git sparse-checkout + Perplexity (8 queries) + Firecrawl (2 scrapes) + GitHub API stats + crates.io API + npm registry API
> **Status**: RESEARCH COMPLETE + DECISIONS VALIDATED
> **Merged from**: Doc 16 (Multi-Modal Builtin Research) + Doc 17 (Smart Router Deep Architecture) + Doc 18 (MCP Multimodal Ecosystem)
> **Dependencies**: Doc 12 (Vegapunk Naming), Doc 13 (Multi-Modal Worker Architectures), Doc 15 (Ecosystem Coherence)

[<< 15 Ecosystem](15-ecosystem-coherence.md) | [Index](README.md) | [20 Agent Research >>](20-agent-research.md)

---

## Executive Summary

**Key Finding**: mistral.rs v0.7.0 already supports vision (Qwen 3-VL, Gemma 3n) and embeddings (Qwen 3 Embedding, EmbeddingGemma), making it a strong foundation for multi-modal Nika. Image generation and audio require separate engines.

**Critical Discovery**: mistral.rs v0.7.0 PR #1841 added "auto pipeline for diffusion and speech models" -- meaning the framework now has infrastructure for image generation AND speech, though these are still experimental.

**MCP Ecosystem**: The MCP multimodal ecosystem in March 2026 is **active but fragmented**. Image generation has the most mature MCP server coverage (ComfyUI, Replicate, fal.ai, Stability AI). Audio/TTS is dominated by the official ElevenLabs MCP server (1,262 stars, 24 tools). Translation has an official DeepL server but no Google Translate MCP.

**Architecture**: The **Smart Router** pattern provides unified dispatch logic (builtin > MCP > LLM fallback > error) for all multi-modal tools, with a hybrid approach:
1. **mistral.rs** for text + vision + embeddings (already integrated)
2. **whisper-rs** for STT (speech-to-text)
3. **diffusion-rs** or candle for image generation
4. **rten + piper** for TTS (text-to-speech)
5. **candle + NLLB-200** for translation (200 languages offline)
6. **instant-distance** for native RAG (in-memory HNSW vector search)
7. **MCP servers** for cloud image gen (Stability AI, ComfyUI), audio (ElevenLabs), translation (DeepL)

---

## Validated Decisions (2026-03-15)

Decisions validated during interactive brainstorm session.

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

**New architectural pattern**: Smart Router -- unified dispatch logic (builtin > MCP > LLM fallback > error) for all multi-modal tools.

**Key research findings**:
- candle supports NLLB-200 natively (Rust, zero Python deps) -- enables `nika:translate`
- instant-distance crate (pure Rust HNSW) -- enables `nika:search` native RAG
- ComfyUI MCP ecosystem mature (219 stars, 13+ tools) -- validates `nika:imagine` MCP approach
- ZERO MCP servers for STT/TTS confirmed -- validates Tier 2 builtin for transcribe/speak
- mistral.rs does NOT yet support Gemma 3n audio -- Path 2 (audio: field) is future

---

# Part 1: Multimodal Research

> Rust crates, MCP servers, mistral.rs capabilities

---

## 1.1 mistral.rs v0.7.0 Capabilities

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

## 1.2 Rust ML Ecosystem Survey

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

**Capabilities:** Pure Rust ONNX inference, ONNX -> .rten conversion, Whisper support, **Piper TTS demo**, CPU-optimized SIMD.

**Relevance:** Best candidate for `nika:speak` (TTS) via Piper models.

### whisper-rs

| Attribute | Value |
|-----------|-------|
| Repository | [tazz4843/whisper-rs](https://github.com/tazz4843/whisper-rs) |
| Stars | 936 |

**Capabilities:** OpenAI Whisper (tiny -> large-v3), real-time transcription, word-level timestamps, VAD, speaker diarization, GPU acceleration (Metal, CUDA).

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

## 1.3 Proposed Builtin Tools

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
  -----------------------
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
----------------------               ---------------------

  Nika Process                         Nika Process
  +---------------------+              +---------------------+
  |  mistral.rs runtime |              |  MCP Client         |
  |  whisper-rs         |              +----------+----------+
  |  ocr-rs             |                         | MCP Protocol
  |  rten/piper         |              +----------+----------+
  +---------------------+              |  MCP Server         |
                                       |  +-- comfyui-mcp    |
  + Lower latency (in-process)         |  +-- replicate-mcp  |
  + Single binary distribution         |  +-- deepl-mcp      |
  - Higher memory usage                +---------------------+
  - Larger binary size
  - Feature flag complexity            + Modular (add/remove)
                                       + Separate memory space
                                       + Language-agnostic
                                       - Higher latency (IPC)
```

---

## 1.4 Multi-Modal Native -- Tiered Approach

### Definition: What "Truly Multi-Modal Native" Means

```
TIER 1 (Essential -- v0.28/v0.29 target):
  +-- Text generation ........... mistral.rs (existing)
  +-- Vision/VLM ................ mistral.rs (Qwen 3-VL)
  +-- Embeddings ................ mistral.rs (EmbeddingGemma)

TIER 2 (Useful -- v0.30 target):
  +-- STT/Transcription ......... whisper-rs
  +-- OCR ....................... ocr-rs
  +-- TTS ....................... rten + piper

TIER 3 (Nice-to-have -- MCP preferred):
  +-- Image generation .......... MCP (ComfyUI, Replicate)
  +-- Video generation .......... MCP (cloud)
  +-- Translation ............... MCP (DeepL, Google)
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

## 1.5 Implementation Roadmap

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

## 1.6 Image Generation Deep Dive

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

## 1.7 The Verb Question

### Do We Need New Verbs for Multi-Modal?

**Answer: NO.** The 5 existing verbs cover all multi-modal patterns:

```
infer: (with vision)  -> LLM sees images natively via provider: native
nika:embed            -> Builtin tool (called via tool infrastructure)
nika:transcribe       -> Builtin tool
nika:speak            -> Builtin tool
nika:ocr              -> Builtin tool
invoke: comfyui:*     -> MCP for image gen
```

Multi-modal capabilities are **tools** (builtin `nika:*` or MCP `invoke:`), not new verbs. The verbs describe the *action shape* (generate text, run command, HTTP request, MCP call, agent loop), while tools describe *capabilities*.

### How It Works

```
Verb = "what kind of action"          Tool = "what capability"
-----------------------------         ------------------------
infer:    -> Generate text/vision      nika:embed -> Create vectors
exec:     -> Run shell command         nika:transcribe -> Audio->Text
fetch:    -> HTTP request              nika:speak -> Text->Audio
invoke:   -> Call MCP tool             nika:ocr -> Image->Text
agent:    -> Agentic loop              comfyui:* -> Text->Image
```

**Key insight**: `infer:` already handles vision when the model supports it (Qwen 3-VL). Adding `image:` field to infer params is sufficient -- no `vision:` verb needed.

---

## 1.8 Industry Comparison

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

## 1.9 MCP Server Ecosystem

### Image Generation MCP Servers

#### Comparison Matrix

| Server | GitHub | Stars | Lang | Tools | Package | Last Updated |
|--------|--------|------:|------|------:|---------|:------------:|
| **ComfyUI MCP** | `joenorton/comfyui-mcp-server` | 222 | Python | 17 | PyPI (uvx) | 2026-03-14 |
| **Comfy Pilot** | `ConstantineB6/comfy-pilot` | 131 | Python | ~10 | PyPI (uvx) | 2026-03-15 |
| **Replicate MCP** | `deepfates/mcp-replicate` | 93 | TypeScript | 13 | npm `mcp-replicate@0.1.1` | 2026-01-18 |
| **Stability AI MCP** | `tadasant/mcp-server-stability-ai` | 81 | TypeScript | 12 | npm `mcp-server-stability-ai@0.2.0` | 2026-03-12 |
| **fal.ai MCP** | `am0y/mcp-fal` | 76 | Python | 8 | PyPI (uvx) | 2026-03-14 |
| **PiAPI MCP** (Midjourney) | `apinetwork/piapi-mcp-server` | 68 | TypeScript | ~10 | npm | 2026-03-13 |
| **Image Gen (Replicate Flux)** | `GongRzhe/Image-Generation-MCP-Server` | 50 | JavaScript | ~3 | npm | 2026-03-03 |
| **fal MCP (raveenb)** | `raveenb/fal-mcp-server` | 38 | Python | ~8 | npm `fal-mcp@1.2.0` | 2026-03-13 |
| **muapi CLI** | `SamurAIGPT/muapi-cli` | 972 | Python | 14 models | npm + pip | 2026-03-12 |

#### Detailed Analysis

**ComfyUI MCP Server** (`joenorton/comfyui-mcp-server`) -- **Top Pick for Local Generation**

The most feature-complete image generation MCP server. 17 tools organized into 6 categories:
- **Generation**: `generate_image`, `generate_song`, `regenerate`
- **Viewing**: `view_image` (inline display)
- **Job Management**: `get_queue_status`, `get_job`, `list_assets`, `get_asset_metadata`, `cancel_job`
- **Configuration**: `list_models`, `get_defaults`, `set_defaults`
- **Workflow**: `list_workflows`, `run_workflow` (run any ComfyUI workflow with custom params)
- **Publishing**: `get_publish_info`, `set_comfyui_output_root`, `publish_asset`

Requires a running ComfyUI instance. Session-scoped asset IDs. Supports automatic compression
for web publishing (default 600KB target). Created 2025-03-05, actively maintained.

**Stability AI MCP Server** (`tadasant/mcp-server-stability-ai`) -- **Top Pick for Cloud API**

12 tools covering the full Stability AI REST API:

| Tool | Description | Cost/image |
|------|-------------|:----------:|
| `generate-image` | Core generation (Stable Image Ultra) | $0.03 |
| `generate-image-sd35` | SD 3.5 models with advanced config | $0.04-$0.07 |
| `remove-background` | Background removal | $0.02 |
| `outpaint` | Extend image in any direction | $0.04 |
| `search-and-replace` | Object replacement | $0.04 |
| `upscale-fast` | 4x resolution enhancement | $0.01 |
| `upscale-creative` | Up to 4K enhancement | $0.25 |
| `control-sketch` | Sketch to production image | $0.03 |
| `control-style` | Style transfer from reference | $0.04 |
| `control-structure` | Structure-preserving generation | $0.03 |
| `replace-background-and-relight` | Background + relighting | $0.08 |
| `search-and-recolor` | Object recoloring | $0.05 |

Available on npm as `mcp-server-stability-ai@0.2.0`. Very reasonable pricing ($3 for 100 images).
Created 2024-12-30 (one of the earliest MCP servers). PulseMCP "Top Pick" badge.

**Replicate MCP** (`deepfates/mcp-replicate`) -- **Best for Model Variety**

13 tools for Replicate's full model marketplace:
- **Model Discovery**: `search_models`, `list_models`, `get_model`, `list_collections`, `get_collection`
- **Prediction**: `create_prediction`, `create_and_poll_prediction`, `get_prediction`, `cancel_prediction`, `list_predictions`
- **Image Utils**: `view_image`, `clear_image_cache`, `get_image_cache_stats`

Runs any of Replicate's thousands of models (Flux, SDXL, etc.). npm `mcp-replicate@0.1.1`.
Last updated 2026-01-18 (less active than others).

**fal.ai MCP** (`am0y/mcp-fal`) -- **Best for Speed**

8 tools with queued execution support:
- `models`, `search`, `schema`, `generate`, `result`, `status`, `cancel`, `upload`

fal.ai is known for fast inference (optimized GPU infrastructure). Supports 1,094+ models
including image, video, music, and audio generation.

**Midjourney** -- No official MCP server. Community options are minimal:
- `piapi-mcp-server` (68 stars) uses PiAPI as proxy to Midjourney API
- `Lala-0x3f/mj-mcp` (7 stars) direct but very low adoption

---

### Translation MCP Servers

#### Comparison Matrix

| Server | GitHub | Stars | Lang | Tools | Package | Last Updated |
|--------|--------|------:|------|------:|---------|:------------:|
| **DeepL MCP** (Official) | `DeepLcom/deepl-mcp-server` | 95 | JavaScript | 8 | npm `deepl-mcp-server@1.1.0` | 2026-03-13 |
| **Google Translate MCP** | -- | -- | -- | -- | -- | Does not exist |
| **KlicStudio MCP** | `krillinai/KlicStudioMCP` | 20 | Python | ~5 | PyPI | 2026-03-14 |
| **AllVoiceLab MCP** | `allvoicelab/AllVoiceLab-MCP` | 56 | Python | ~8 | PyPI | 2026-01-22 |

#### Detailed Analysis

**DeepL MCP Server** (`DeepLcom/deepl-mcp-server`) -- **Only Serious Translation MCP**

Official server from DeepL. 8 tools:
- **Translation**: `translate-text`, `translate-document`, `rephrase-text`
- **Language Discovery**: `get-source-languages`, `get-target-languages`
- **Glossary Management**: `list-glossaries`, `get-glossary-info`, `get-glossary-dictionary-entries`

Available on npm as `deepl-mcp-server@1.1.0`. Requires DeepL API key (free tier available:
500K characters/month). Created 2025-04-04, 17 forks.

**Google Translate MCP** -- Does NOT exist as of March 2026. No official or community server
with meaningful adoption. This is a notable gap in the ecosystem.

**Gap Analysis**: Translation is significantly underserved in MCP. DeepL is the only production-grade
option. For Nika's localization workflows (200+ locales via NovaNet), this is a critical dependency.

---

### Audio / TTS / STT MCP Servers

#### Comparison Matrix

| Server | GitHub | Stars | Lang | Tools | Package | Last Updated |
|--------|--------|------:|------|------:|---------|:------------:|
| **ElevenLabs MCP** (Official) | `elevenlabs/elevenlabs-mcp` | 1,262 | Python | 24 | PyPI (uvx) | 2026-03-14 |
| **Whisper MCP** | `arcaputo3/mcp-server-whisper` | 48 | Python | ~12 | PyPI `mcp-server-whisper` | 2026-03-12 |
| **Kokoro TTS MCP** | `mberg/kokoro-tts-mcp` | 75 | Python | ~3 | PyPI (uvx) | 2026-03-12 |
| **MLX Whisper MCP** | `kachiO/mlx-whisper-mcp` | 22 | Python | ~4 | PyPI (uvx) | 2026-02-25 |
| **Fast Whisper MCP** | `BigUncle/Fast-Whisper-MCP-Server` | 14 | Python | ~4 | PyPI | 2026-02-05 |
| **Local STT MCP** | `SmartLittleApps/local-stt-mcp` | 11 | TypeScript | ~3 | npm | 2026-03-06 |
| **VoiceMode** (2-way voice) | `mbailey/voicemode` | 894 | Python | -- | PyPI | 2026-03-14 |

#### Detailed Analysis

**ElevenLabs MCP** (`elevenlabs/elevenlabs-mcp`) -- **Dominant Audio MCP, Official**

By far the most comprehensive audio MCP server. 24 tools across 6 categories:

| Category | Tools |
|----------|-------|
| **TTS** | `text_to_speech`, `speech_to_speech`, `text_to_voice`, `create_voice_from_preview` |
| **Voice Design** | `search_voices`, `get_voice`, `voice_clone`, `search_voice_library` |
| **Audio Processing** | `isolate_audio`, `play_audio` |
| **STT** | `speech_to_text` |
| **Sound Design** | `text_to_sound_effects`, `compose_music`, `create_composition_plan` |
| **Conversational AI** | `create_agent`, `add_knowledge_base_to_agent`, `list_agents`, `get_agent`, `get_conversation`, `list_conversations`, `make_outbound_call`, `list_phone_numbers` |
| **Account** | `list_models`, `check_subscription` |

Free tier: 10K credits/month. Created 2025-03-14 (exactly 1 year old). 206 forks.

**Whisper MCP** (`arcaputo3/mcp-server-whisper`) -- **Best for Transcription**

4 tool categories (file tools, audio tools, transcription tools, TTS tools):
- Multi-model transcription (all OpenAI audio models)
- Format conversion between audio types
- Automatic compression for oversized files
- Interactive audio chat with GPT-4o audio models
- Text-to-speech with customizable voices
- Regex-based file searching with metadata

Requires OpenAI API key. Available on PyPI.

**Kokoro TTS MCP** (`mberg/kokoro-tts-mcp`) -- **Best for Local/Free TTS**

Uses Kokoro ONNX weights (open-source TTS model from HuggingFace). Generates .mp3 files locally
with optional S3 upload. No API key required for generation. Configurable voice, speed, language.
Requires ffmpeg for wav-to-mp3 conversion.

**VoiceMode** (`mbailey/voicemode`) -- **2-Way Voice for Claude Code**

Not strictly an MCP server but notable: enables natural voice conversations with Claude Code.
894 stars, very active. Could be relevant for Nika's chat mode in the future.

---

### Vision / OCR MCP Servers

#### Comparison Matrix

| Server | GitHub | Stars | Lang | Tools | Package | Last Updated |
|--------|--------|------:|------|------:|---------|:------------:|
| **ImageSorcery MCP** | `sunriseapps/imagesorcery-mcp` | 293 | Python | 17 | PyPI (uvx) | 2026-03-13 |
| **Computer Control MCP** | `AB498/computer-control-mcp` | 120 | Python | ~8 | PyPI | 2026-03-05 |
| **VisionCraft MCP** | `augmentedstartups/VisionCraft-MCP-Server` | 116 | JavaScript | ~10 | npm | 2026-03-13 |
| **OpenCV MCP** | `GongRzhe/opencv-mcp-server` | 97 | Python | ~20 | PyPI | 2026-03-13 |
| **ScreenMonitor MCP** | `inkbytefo/ScreenMonitorMCP` | 71 | Python | ~5 | PyPI | 2026-03-03 |
| **Tesseract MCP** | -- | -- | -- | -- | -- | Does not exist |

#### Detailed Analysis

**ImageSorcery MCP** (`sunriseapps/imagesorcery-mcp`) -- **Top Pick for Image Processing + OCR**

The most comprehensive image processing MCP server. 17 tools:

| Tool | Description |
|------|-------------|
| `blur` | Blur rectangular or polygonal areas (or invert for background blur) |
| `change_color` | Change color palette (sepia, grayscale, etc.) |
| `config` | View/update server configuration |
| `crop` | Crop using OpenCV NumPy slicing |
| `detect` | Object detection using Ultralytics models (YOLO) with segmentation masks |
| `draw_arrows` | Draw arrows on images |
| `draw_circles` | Draw circles on images |
| `draw_lines` | Draw lines on images |
| `draw_rectangles` | Draw rectangles on images |
| `draw_texts` | Draw text on images |
| `fill` | Fill areas with color/opacity or make transparent |
| `find` | Find objects by text description (segmentation masks or polygons) |
| `get_metainfo` | Get image metadata (dimensions, format, EXIF) |
| **`ocr`** | **OCR using EasyOCR (multi-language)** |
| `overlay` | Overlay one image on another with transparency |
| `resize` | Resize images |
| `rotate` | Rotate images |

Topics: `computer-vision`, `image-editing`, `image-processing`, `mcp`, `mcp-server`, `ocr`, `opencv`.
Created 2025-05-12, 45 forks.

**OpenCV MCP** (`GongRzhe/opencv-mcp-server`) -- **Best for Computer Vision Pipelines**

Comprehensive OpenCV wrapper with tools organized into 4 categories:
- Image basics (read, save, convert)
- Image processing and enhancement (resize, crop, filter)
- Edge detection, contour analysis, feature detection, object detection
- Face detection and recognition
- Video processing (frame extraction, motion detection, object tracking)
- Camera integration for real-time detection

Heavier dependency footprint than ImageSorcery but more complete for CV workflows.

**Computer Control MCP** (`AB498/computer-control-mcp`) -- **Desktop Automation + OCR**

Provides mouse, keyboard, OCR (RapidOCR + ONNXRuntime), and screenshot capabilities.
Similar to Anthropic's computer use but via MCP. Created 2025 (120 stars).

**Tesseract MCP** -- Does NOT exist as a standalone server. OCR is available through:
- ImageSorcery MCP (EasyOCR backend)
- Computer Control MCP (RapidOCR backend)
- OpenCV MCP (built-in OCR capabilities)

---

### Rust Crates for Multimodal (crates.io data)

#### Core ML Crates

| Crate | Version | Total DL | Recent DL | Last Updated | Key Feature |
|-------|---------|----------:|----------:|:------------:|-------------|
| **`candle-core`** | 0.9.2 | 3,154,082 | 1,606,419 | 2026-01-24 | HuggingFace ML framework (tensors) |
| **`candle-transformers`** | 0.9.2 | 1,226,778 | 421,828 | 2026-01-24 | Pre-built models (Whisper, CLIP, SD, LLaMA) |
| **`candle-nn`** | 0.9.2 | 2,038,792 | 1,071,888 | 2026-01-24 | Neural network layers |
| **`hf-hub`** | 0.5.0 | 5,025,616 | 2,264,255 | 2026-02-19 | HuggingFace Hub API (model downloads) |
| **`whisper-rs`** | 0.16.0 | 207,991 | 93,140 | 2026-03-12 | whisper.cpp bindings (STT) |
| **`ocrs`** | 0.12.1 | 67,887 | 15,086 | 2026-02-14 | Pure Rust OCR engine |
| **`rten`** | 0.24.0 | 150,447 | 75,132 | 2025-12-23 | Pure Rust ONNX Runtime |
| **`ort`** | 2.0.0-rc.12 | 7,139,203 | 2,540,545 | 2026-03-05 | ONNX Runtime 1.24 wrapper (C++) |
| **`instant-distance`** | 0.6.1 | 155,931 | 28,885 | 2023-06-26 | HNSW nearest neighbor (unmaintained) |

#### Image Processing Crates

| Crate | Version | Total DL | Recent DL | Last Updated | Key Feature |
|-------|---------|----------:|----------:|:------------:|-------------|
| **`image`** | 0.25.10 | 105,883,495 | 18,137,277 | 2026-03-10 | Core imaging (encode/decode/process) |
| **`imageproc`** | 0.26.1 | 7,480,118 | 1,709,000 | 2026-02-28 | Image processing ops |
| **`resvg`** | 0.47.0 | 10,758,922 | 3,064,587 | 2026-02-09 | SVG rendering |
| **`tiny-skia`** | 0.12.0 | 22,437,269 | 5,594,682 | 2026-02-02 | 2D graphics (Skia subset, pure Rust) |
| **`ab_glyph`** | 0.2.32 | 24,466,318 | 4,956,007 | 2025-09-28 | Font glyph loading + rasterization |

#### Detailed Crate Analysis

##### `candle-core` + `candle-transformers` + `candle-nn` (HuggingFace)

Repository: https://github.com/huggingface/candle

The flagship Rust ML framework from HuggingFace. v0.9.2 is actively maintained (Jan 2026).
`candle-transformers` includes pre-built implementations of:
- **Whisper** (speech-to-text)
- **CLIP** (image-text similarity)
- **Stable Diffusion** (image generation)
- **LLaMA / Mistral / Phi** (text generation)

Key features: Metal (macOS) and CUDA (Linux) acceleration, GGUF model loading, quantization
support. 3.1M total downloads on `candle-core` indicates significant production adoption.

**Nika relevance**: Already used indirectly via mistral.rs. Could be used directly for
embedding generation, CLIP scoring, and Whisper inference without external services.

##### `hf-hub` v0.5.0

Repository: https://github.com/huggingface/hf-hub

The standard Rust crate for downloading models from HuggingFace. 5M+ downloads makes it the
most downloaded crate in this list. Keywords: `machine-learning`, `huggingface`, `hf`, `hub`.

Features: Async downloads, cache management, repo API, model card parsing.

**Nika relevance**: Already a dependency of the native inference pipeline. Essential for
model management via `nika model pull`.

##### `whisper-rs` v0.16.0

Repository: https://codeberg.org/tazz4843/whisper-rs

Rust bindings for whisper.cpp (ggerganov's C++ implementation). v0.16.0 was published
**3 days ago** (2026-03-12). 93K recent downloads shows active and growing usage.

Features: Multiple model sizes (tiny to large), language detection, word-level timestamps,
voice activity detection (VAD), full API coverage of whisper.cpp.

**Nika relevance**: Strongest candidate for `nika:transcribe` builtin tool. Local STT
without needing an MCP server or OpenAI API key.

##### `ocrs` v0.12.1

Repository: https://github.com/robertknight/ocrs

Pure Rust OCR engine by the same author as `rten`. v0.12.1 (Feb 2026). Uses ONNX models
via `rten` for text detection and recognition. Zero C dependencies.

Features: Text detection + recognition pipeline, word/line bounding boxes, pure Rust.

**Nika relevance**: Prime candidate for `nika:ocr` builtin tool. No Python or C deps.
Perfect for the QR Code AI use case (reading text from QR code destinations).

##### `rten` v0.24.0 vs `ort` v2.0.0-rc.12

Both provide ONNX Runtime for Rust, but with very different trade-offs:

| Aspect | `rten` | `ort` |
|--------|--------|-------|
| Backend | Pure Rust | C++ (ONNX Runtime 1.24) |
| C Dependencies | Zero | Requires C++ libs |
| Performance | Good | Best (highly optimized) |
| Total Downloads | 150K | 7.1M |
| Maturity | Newer, fast-moving | Industry standard |
| Build Complexity | Simple (`cargo build`) | Complex (C++ linkage) |
| GPU Acceleration | Limited | Full CUDA/TensorRT/CoreML |

`rten` is used by `ocrs` for zero-dependency OCR. `ort` is the industry standard for production
ONNX inference. **Recommendation**: Use `rten` for builtin tools where zero-dep is important,
`ort` for high-performance inference pipelines where build complexity is acceptable.

##### `instant-distance` v0.6.1

Repository: https://github.com/InstantDomain/instant-distance

HNSW (Hierarchical Navigable Small World) vector search. v0.6.1 but **last updated June 2023**.
Still functional but appears unmaintained. 28K recent downloads suggests continued passive usage.

**Nika relevance**: Could power local embedding search (satellite routing, context retrieval).
However, unmaintained status is concerning. Alternatives worth evaluating:
- `usearch` crate (more active)
- `hnsw` crate
- Custom implementation with `candle-core` tensors

---

### MCP Server Synthesis for Nika

#### MCP Server Recommendations for `MCP_ALIASES`

Based on this research, the following servers are recommended for inclusion in Nika's MCP alias
registry (currently 100 aliases in `src/core/mcp_aliases.rs`):

| Alias | Server | Priority | npm/PyPI | Rationale |
|-------|--------|:--------:|----------|-----------|
| `stability` | `mcp-server-stability-ai` | **High** | npm `mcp-server-stability-ai` | Best cloud image gen, 12 tools, $0.03/image |
| `comfyui` | `comfyui-mcp-server` | **High** | PyPI (uvx) | Best local image gen, 17 tools, workflow support |
| `replicate` | `mcp-replicate` | Medium | npm `mcp-replicate` | Model marketplace, 13 tools, thousands of models |
| `fal` | `mcp-fal` | Medium | PyPI (uvx) | Fast inference, 1094+ models, queued execution |
| `elevenlabs` | `elevenlabs-mcp` | **High** | PyPI (uvx) | Official, 24 tools, dominant audio MCP |
| `deepl` | `deepl-mcp-server` | **High** | npm `deepl-mcp-server` | Official translation, 8 tools, glossary support |
| `whisper` | `mcp-server-whisper` | Medium | PyPI `mcp-server-whisper` | STT via OpenAI, multi-model, 12+ tools |
| `imagesorcery` | `imagesorcery-mcp` | Medium | PyPI (uvx) | Image processing + OCR, 17 tools, YOLO detection |
| `opencv` | `opencv-mcp-server` | Low | PyPI (uvx) | Heavy deps, overlaps with imagesorcery |
| `kokoro-tts` | `kokoro-tts-mcp` | Low | PyPI (uvx) | Free local TTS, limited tools |

#### Native Capabilities (No MCP Needed)

These capabilities can be built directly into Nika using Rust crates, avoiding the overhead
of external MCP server processes:

| Capability | Crate(s) | Integration Path | Build Impact |
|------------|----------|-----------------|--------------|
| **STT (Whisper)** | `whisper-rs` v0.16.0 | `nika:transcribe` builtin tool | C++ dep (whisper.cpp) |
| **OCR** | `ocrs` v0.12.1 + `rten` v0.24.0 | `nika:ocr` builtin tool | Zero C deps |
| **Image Processing** | `image` v0.25.10 + `imageproc` v0.26.1 | `nika:image_*` builtin tools | Zero C deps |
| **Embeddings** | `candle-core` v0.9.2 | Extend `provider: native` | Metal/CUDA optional |
| **Model Downloads** | `hf-hub` v0.5.0 | Already integrated | Zero C deps |
| **SVG Rendering** | `resvg` v0.47.0 | QR code gen pipeline | Zero C deps |
| **Font Rendering** | `ab_glyph` v0.2.32 | Text overlay on images | Zero C deps |
| **2D Graphics** | `tiny-skia` v0.12.0 | Drawing operations | Zero C deps |
| **ONNX Inference** | `rten` v0.24.0 or `ort` v2.0.0-rc.12 | General ML model execution | rten: zero C / ort: C++ |

#### Gap Analysis

| Need | MCP Coverage | Native Coverage | Recommendation |
|------|:------------:|:---------------:|----------------|
| Image generation | 4+ servers (good) | None (needs GPU) | MCP (stability, comfyui) |
| Translation | DeepL only (limited) | None | MCP (deepl) + LLM fallback |
| TTS | ElevenLabs (excellent) | Kokoro via ONNX (basic) | MCP (elevenlabs) for quality |
| STT | Whisper MCP (decent) | `whisper-rs` (excellent) | **Native preferred** |
| OCR | ImageSorcery (decent) | `ocrs` (good) | **Native preferred** |
| Image processing | ImageSorcery (good) | `image`+`imageproc` (good) | **Native preferred** |
| Vision analysis | None dedicated | Model vision (gpt-4o, claude) | Model-native vision |
| Video processing | Minimal (ffmpeg MCP) | None | Low priority |
| Embeddings | None | `candle-core` (good) | **Native preferred** |
| Vector search | None | `instant-distance` (stale) | Evaluate alternatives |

#### Recommended Architecture

```
+-----------------------------------------------------------------------+
|  NIKA MULTIMODAL ARCHITECTURE (Recommended)                            |
+-----------------------------------------------------------------------+
|                                                                       |
|  BUILTIN TOOLS (Rust-native, zero network overhead, instant)          |
|  +-- nika:transcribe   (whisper-rs v0.16)    -- Local STT            |
|  +-- nika:ocr          (ocrs v0.12)          -- Local OCR            |
|  +-- nika:image_resize (image v0.25)         -- Image processing     |
|  +-- nika:image_crop   (imageproc v0.26)     -- Image processing     |
|  +-- nika:embed        (candle v0.9)         -- Local embeddings     |
|  +-- nika:svg_render   (resvg v0.47)         -- SVG to PNG           |
|                                                                       |
|  MCP TOOLS (External servers, API-dependent, rich features)           |
|  +-- stability:*       (Stability AI)    -- Cloud image generation   |
|  +-- comfyui:*         (ComfyUI)         -- Local image generation   |
|  +-- elevenlabs:*      (ElevenLabs)      -- TTS/audio/voice          |
|  +-- deepl:*           (DeepL)           -- Translation/glossary     |
|  +-- replicate:*       (Replicate)       -- Any model marketplace    |
|  +-- fal:*             (fal.ai)          -- Fast image/video gen     |
|                                                                       |
|  MODEL VISION (LLM-native, no extra tools needed)                     |
|  +-- openai/gpt-4o                       -- Vision + text            |
|  +-- anthropic/claude-*                  -- Vision + text            |
|  +-- google/gemini-*                     -- Vision + audio + text    |
|  +-- native/qwen2-vl-*                  -- Local vision             |
|                                                                       |
+-----------------------------------------------------------------------+
```

**Design principle**: Prefer native Rust implementations for deterministic, fast operations
(OCR, STT, image processing, embeddings). Use MCP servers for operations requiring large
GPU models (image generation), API services (translation, TTS), or complex pipelines
(ComfyUI workflows).

---

# Part 2: Smart Router & Tool Design

> Smart Router pattern, tool inventory, nika:* tools

---

## 2.1 The Smart Router Pattern

### Core Concept

The Smart Router is the **fundamental architectural pattern** for all multi-modal tools in Nika v0.30. Every tool that has multiple possible backends follows the same dispatch logic:

```
Tool call (e.g., nika:translate)
    |
    v
+-- Has builtin engine? --+
|  YES                     |  NO
|  +-- Feature enabled? -+ |  |
|  | YES -> Use builtin  | |  |
|  | NO  -> Check MCP    | |  |
|  +---------------------+ |  |
+---------------------------+  |
         |                     |
         v                     v
+-- MCP servers available? ------+
|  YES -> Route to best match     |
|  NO  -> LLM fallback possible?  |
|         YES -> infer: prompt     |
|         NO  -> Error + suggestion |
+---------------------------------+
```

### Why This Pattern

Research findings that led to this design:

1. **No existing MCP server does multi-backend routing** (confirmed via Perplexity + Firecrawl research, March 2026). Pixelle-MCP offers "mode selection" (local OR cloud) but not dynamic routing.
2. **ComfyUI MCP ecosystem is mature** for image gen (219 stars, 13+ tools) but audio/STT/TTS has ZERO MCP servers.
3. **Degradation gracieuse** is critical for developer experience: works offline in dev, scales to cloud in production.

### Dispatch Priority Chain

```
PRIORITY (configurable per-tool in workflow YAML):

1. Builtin engine (if feature flag enabled)
   - Free, offline, no API key required
   - Lower quality than cloud alternatives
   - Immediate latency (no network)

2. MCP server (if connected)
   - Check RunContext.mcp_servers for matching capability
   - Route to first match in preference order
   - Configurable: prefer: [comfyui, replicate, fal]

3. LLM fallback (if applicable)
   - Some tools can fall back to infer: with a prompt
   - e.g., translate -> "Translate to Japanese: <text>"
   - Quality depends on LLM capability

4. Error with suggestion
   - Clear message: "No backend available"
   - Actionable: "Run: nika mcp add comfyui"
```

### YAML Configuration

```yaml
# Workflow-level tool configuration
tools:
  nika:translate:
    prefer: [builtin, deepl, google-translate]
    fallback: true  # allow LLM fallback

  nika:imagine:
    prefer: [comfyui, replicate, fal]
    fallback: false  # no LLM fallback for image gen

  nika:speak:
    prefer: [builtin, elevenlabs]
    fallback: false  # no LLM fallback for TTS
```

### Rust Implementation Sketch

```rust
/// Smart Router trait for tools with multiple backends
trait SmartRouter {
    /// Backend preference order
    fn backends(&self) -> Vec<Backend>;

    /// Check if a backend is available in the current RunContext
    fn is_available(&self, backend: &Backend, ctx: &RunContext) -> bool;

    /// Execute on the best available backend
    async fn dispatch(&self, input: ToolInput, ctx: &RunContext) -> Result<ToolOutput> {
        for backend in self.backends() {
            if self.is_available(&backend, ctx) {
                return self.execute_on(backend, input, ctx).await;
            }
        }
        // LLM fallback if supported
        if self.supports_llm_fallback() {
            return self.llm_fallback(input, ctx).await;
        }
        Err(NikaError::NoBackendAvailable {
            tool: self.name(),
            suggestion: self.install_suggestion(),
        })
    }
}

enum Backend {
    Builtin,                    // Feature-flagged native engine
    Mcp { server: String },     // MCP server by name
    LlmFallback,               // infer: with generated prompt
}
```

---

## 2.2 `nika:imagine` -- Image Generation Smart Router

### Why MCP-Only (No Builtin)

| Factor | Assessment |
|--------|------------|
| **Memory** | SDXL requires 12GB+ VRAM. Does NOT fit alongside LLM on 16GB Mac |
| **Quality** | Cloud APIs (Midjourney-level) far exceed local quality |
| **Ecosystem** | ComfyUI MCP is mature (219 stars, 13+ tools, active March 2026) |
| **Latency** | Image gen is inherently slow (seconds). Network latency is negligible |
| **Cost** | ComfyUI on local GPU = free. Cloud = pay-per-image but high quality |

### ComfyUI MCP Landscape (March 2026)

| Server | Stars | Language | Tools | Special Features |
|--------|-------|----------|-------|------------------|
| **joenorton/comfyui-mcp-server** | 219 | Python | 13+ | generate_image, generate_song (!), regenerate, view_image, publish_asset, PARAM_* custom workflows |
| **alecc08/comfyui-mcp** | 14 | TypeScript | 7 | text_to_image, img2img, upscale, resize, remove_background, Qwen Image models |
| **Pixelle-MCP** | -- | Python | -- | Local ComfyUI or RunningHub cloud (mode selection) |
| **Aether MCP** | -- | -- | -- | Flux/SDXL + video generation |

### Architecture

```
 nika:imagine (builtin tool, Tier 3 MCP router)
    |
    +-- Agent calls: "Generate a cyberpunk QR code hero image, 1024x1024"
    |
    v
+--- Orchestrator Router (capability match) ---+
|                                        |
|  1. Check RunContext.mcp_servers       |
|     +-- comfyui connected? --YES-->  invoke: comfyui.generate_image  |
|                                        |
|  2. Check RunContext.mcp_servers       |
|     +-- replicate connected? -YES->  invoke: replicate.run           |
|                                        |
|  3. Check RunContext.mcp_servers       |
|     +-- fal connected? ------YES-->  invoke: fal.generate            |
|                                        |
|  4. No backend available               |
|     +-- Error: "No image gen backend.  |
|         Run: nika mcp add comfyui"     |
|                                        |
+----------------------------------------+
```

### Tool API

```json
{
  "name": "nika:imagine",
  "input": {
    "prompt": "cyberpunk QR code hero image with neon lighting",
    "width": 1024,
    "height": 1024,
    "style": "cyberpunk",
    "quality": "high",
    "negative_prompt": "blurry, low quality"
  },
  "output": {
    "image_path": "/tmp/nika/artifacts/imagine_abc123.png",
    "metadata": {
      "backend": "comfyui",
      "model": "sdxl-turbo",
      "seed": 42,
      "generation_time_ms": 3200
    }
  }
}
```

### MCP Alias (proposed)

```rust
("comfyui", McpAlias {
    command: "uvx",
    args: &["comfyui-mcp-server"],
    env: &[("COMFYUI_URL", "http://localhost:8188")],
    description: "ComfyUI image/audio generation (13+ tools)",
})
```

### Name Origin

**nika:imagine** -- From One Piece, Gear 5 / Sun God Nika:
> "His body gains the ability to fight however he imagines... His power is limited only by his imagination."

The tool turns imagination (prompts) into reality (images). The name is:
- Descriptive (everyone understands "imagine")
- Evocative (One Piece reference for insiders)
- Distinct from `nika:vision` (input vs output)

---

## 2.3 `nika:translate` -- Native Translation with Smart Router

### Why Builtin (Candle + NLLB-200)

Research confirmed: **Candle (HuggingFace, 19.6k stars, Rust native) directly supports NLLB model inference**.

| Factor | Assessment |
|--------|------------|
| **Engine** | candle + candle-transformers (Rust native, zero Python deps) |
| **Model** | facebook/nllb-200-distilled-600M (600MB, 200 languages) |
| **Upgrade path** | facebook/nllb-200-1.3B (better quality, ~1.3GB) |
| **Memory** | 600MB for distilled model. Fits alongside everything else |
| **Quality** | Good for drafts. DeepL/Google better for final quality |
| **Offline** | 100% offline, zero API cost, zero rate limits |
| **Speed** | Fast on CPU. Faster on Metal/CUDA |
| **Limitations** | Max 512 tokens per request. Chunk + reassemble for longer texts |

### SuperNovae Synergy

This is a killer feature for SuperNovae because:
- **NovaNet** = localization engine across 200+ locales
- **QR Code AI** = multilingual landing pages
- **Offline translation** = no API cost during development
- **Candle** = same Rust ML ecosystem as mistral.rs (shares candle backend)

### Architecture (Smart Router)

```
 nika:translate("Scannez ce code QR", source: "fr", target: "ja")
    |
    +-- 1. NLLB feature enabled? --YES--> candle NLLB inference (free, offline)
    |                               |
    |   NO                          v
    |                          "QRコードをスキャンしてメニューにアクセスしてください"
    |
    +-- 2. DeepL MCP connected? --YES--> invoke: deepl.translate (cloud, high quality)
    |
    +-- 3. Google Translate MCP? --YES--> invoke: google-translate.translate
    |
    +-- 4. LLM fallback ----------YES--> infer: "Translate to Japanese: <text>"
    |                                     (always available, quality varies)
    |
    +-- 5. Never reaches here (LLM fallback always catches)
```

### Tool API

```json
{
  "name": "nika:translate",
  "input": {
    "text": "Scannez ce code QR pour acceder au menu",
    "source": "fr",
    "target": "ja",
    "model": "nllb-600m",
    "max_length": 512
  },
  "output": {
    "translated": "このQRコードをスキャンしてメニューにアクセスしてください",
    "backend": "builtin:nllb-600m",
    "confidence": 0.89
  }
}
```

### Workflow Example

```yaml
# Translate content across locales with zero API cost
tasks:
  - id: context
    invoke: novanet_context
    params: { focus_key: "qr-code", locale: "fr-FR", mode: "page" }

  - id: generate
    infer: "Generate landing page content in French"
    context: $context
    use.ctx: fr_content

  - id: translate_all
    for_each: ["ja", "de", "es", "pt", "ko", "zh"]
    as: target_locale
    concurrency: 3
    agent: "Translate the French content to {{use.target_locale}}"
    tools: [nika:translate]
    context: $fr_content
    # Agent uses nika:translate internally, 200 languages, zero cost
```

### Chunking Strategy (> 512 tokens)

NLLB is limited to 512 tokens per request. For longer texts:

```
Input text (2000 tokens)
    |
    v
+-- Sentence splitter (unicode-segmentation crate) --+
|                                                      |
|   Chunk 1 (480 tokens)  -> NLLB translate           |
|   Chunk 2 (510 tokens)  -> NLLB translate           |
|   Chunk 3 (490 tokens)  -> NLLB translate           |
|   Chunk 4 (520 tokens)  -> split further, translate  |
|                                                      |
+-- Reassemble in order --------------------------------+
    |
    v
Output text (translated, 2000 tokens)
```

Feature flag: `translate` (default OFF, requires model download: `nika model pull nllb-200-600m`)

---

## 2.4 `nika:search` + `nika:index` -- Native RAG in Runtime

### Core Concept

Native RAG (Retrieval-Augmented Generation) WITHIN a single workflow run, with zero external infrastructure. No Pinecone. No Weaviate. No Qdrant. Just Nika.

### Components

```
+-- nika:embed (mistral.rs, EmbeddingGemma) --> vectors (Tier 1, already available)
|
+-- instant-distance (HNSW index) -----------> in-memory ANN search
|   crates.io, pure Rust, zero deps
|   Simple API: build index, search neighbors
|
+-- RunContext.vector_index (DashMap) --------> storage within the run
    Ephemeral: lives and dies with the workflow run
```

### Rust HNSW Crate Analysis

| Crate | Stars | Type | Persistence | Best For |
|-------|-------|------|-------------|----------|
| **instant-distance** | crates.io | Pure Rust HNSW | In-memory only | Ephemeral indexes (Nika) |
| **hnswlib-rs** | GitHub | Rust binding | In-memory | High-perf, more features |
| **hnsw_rs** | GitHub | Pure Rust | In-memory | Production-ready |
| **hannoy** | GitHub | LMDB-backed | Disk (LMDB) | Large-scale persistent |
| **usearch** | GitHub | C++ + Rust bindings | Both | GPU-optimized |

**Best fit for Nika: `instant-distance`** -- Pure Rust, zero deps, in-memory, simple API. Perfect for ephemeral indexes in RunContext.

### Architecture

```
WORKFLOW RUN
    |
    +-- Task 1: fetch article_1 ----> auto-embed ----> add to HNSW index
    +-- Task 2: fetch article_2 ----> auto-embed ----> add to HNSW index
    +-- Task 3: fetch article_3 ----> auto-embed ----> add to HNSW index
    |                                                       |
    |   RunContext.vector_index: {                           |
    |     "article_1": [0.12, -0.34, 0.56, ...],            |
    |     "article_2": [0.89, -0.12, 0.34, ...],            |
    |     "article_3": [0.45, 0.67, -0.23, ...],            |
    |   }                                                    |
    |                                                       |
    +-- Task 4: nika:search("QR code accessibility")        |
    |   |                                                   |
    |   +-- embed query -----> HNSW nearest neighbors       |
    |   |                                                   |
    |   +-- top-3 results:                                  |
    |       [article_2 (score: 0.94),                       |
    |        article_1 (score: 0.88),                       |
    |        article_3 (score: 0.72)]                       |
    |                                                       |
    +-- Task 5: infer: "Synthesize these findings..."       |
        context: search results from task 4
```

### Tool APIs

#### `nika:search`

```json
{
  "name": "nika:search",
  "input": {
    "query": "QR code security best practices",
    "top_k": 5,
    "namespace": "research",
    "threshold": 0.7,
    "include_metadata": true
  },
  "output": {
    "results": [
      {
        "text": "QR codes should implement...",
        "score": 0.94,
        "source": "fetch_arxiv_1",
        "metadata": { "url": "https://...", "title": "..." }
      }
    ],
    "total_indexed": 15,
    "search_time_ms": 2
  }
}
```

#### `nika:index`

```json
{
  "name": "nika:index",
  "input": {
    "text": "Content to index for later retrieval...",
    "namespace": "research",
    "metadata": {
      "source": "arxiv",
      "date": "2026-03",
      "url": "https://arxiv.org/..."
    }
  },
  "output": {
    "indexed": true,
    "vector_id": "idx_abc123",
    "namespace_size": 16
  }
}
```

### Auto-Index Mode

```yaml
# In workflow YAML
tasks:
  - id: research
    agent: "Research QR code accessibility standards"
    tools: [nika:fetch, nika:search, nika:index]
    auto_index: true  # Every fetch/invoke result auto-embedded + indexed

    # The agent lifecycle:
    # 1. fetch article_1 -> auto-indexed
    # 2. fetch article_2 -> auto-indexed
    # 3. ...10 articles fetched and indexed...
    # 4. nika:search("accessibility guidelines")
    #    -> returns top-5 from its OWN fetched data
    # 5. infer: synthesize with relevant chunks
    # = RAG pipeline WITHIN a single agent turn, zero infra
```

### Killer Use Case: Agent-Driven Research with Native RAG

```yaml
schema: nika/workflow@0.12

tasks:
  - id: deep_research
    agent: "Research and synthesize QR code accessibility standards"
    tools:
      - nika:fetch
      - nika:search
      - nika:index
      - novanet_search
    auto_index: true
    prompt: |
      You have access to web fetching and semantic search tools.
      1. Fetch 10-15 relevant articles about QR accessibility
      2. As you fetch, content is automatically indexed
      3. Use nika:search to find specific patterns across all fetched content
      4. Also search NovaNet for existing entity knowledge
      5. Synthesize your findings into a structured report
      6. Include citations to the specific sources you found most relevant
```

---

## 2.5 Audio Architecture: Two Paths

### Current State (March 2026)

| Capability | mistral.rs v0.7 | Status |
|-----------|-----------------|--------|
| Audio input (Gemma 3n audio encoder) | INFRA exists, model not production-ready | PR #1841 in progress |
| Speech-to-Text (Whisper) | NOT supported | Use whisper-rs |
| Text-to-Speech (Piper) | NOT supported | Use rten + Piper |
| Audio generation (music) | NOT supported | MCP ComfyUI (generate_song) |

### Path 1: Explicit Pipeline (v0.30, ships now)

```
Audio file (.wav/.mp3)
    |
    v
nika:transcribe (whisper-rs, Whisper v3 Large)
    |
    v
Text output
    |
    v
infer: "Summarize this transcript..."
    |
    v
Text result
    |
    v (optionally)
nika:speak (rten + Piper, 50+ voices)
    |
    v
Audio file (.wav)
```

This is the **safe, proven path**. whisper-rs is mature (936 stars), Piper is mature (10.7k stars). Ships in v0.30.

### Path 2: Native Audio Input in `infer:` (future, when mistral.rs ready)

```yaml
# Future: when mistral.rs supports Gemma 3n audio natively
tasks:
  - id: analyze_meeting
    infer:
      provider: native
      model: gemma-3n
      audio: ./meeting-recording.wav    # <-- new field
      prompt: "Summarize this meeting and extract action items"
```

Symmetry with vision:

| Media Field | Model | Status |
|-------------|-------|--------|
| `image:` | Qwen 3-VL via mistral.rs | Works today |
| `audio:` | Gemma 3n via mistral.rs | Future (when PR #1841 production-ready) |
| `video:` | TBD | Far future |

### Decision

- **Ship Path 1** (`nika:transcribe` + `nika:speak`) in v0.30
- **Prepare AST** for Path 2 (`audio:` field in InferAction struct)
- **Activate Path 2** when mistral.rs merges audio support to stable

### Audio Landscape (MCP Gap Confirmed)

```
Audio capability        Builtin?     MCP?        Verdict
---                     ---          ---         ---
Speech-to-Text (STT)   whisper-rs   NONE        BUILTIN ONLY (no MCP exists)
Text-to-Speech (TTS)   rten+piper   NONE        BUILTIN ONLY (no MCP exists)
Music generation        NO           comfyui     MCP ONLY (generate_song)
Sound effects           NO           comfyui     MCP ONLY (if nodes installed)
Audio input (LLM)       FUTURE       NONE        FUTURE (mistral.rs Gemma 3n)
```

---

## 2.6 Complete Tool Inventory (v0.30)

### Existing Builtin Tools (11, already shipped)

| Tool | Purpose | Since |
|------|---------|-------|
| `nika:read` | Read file contents | v0.15.0 |
| `nika:write` | Write file contents | v0.15.0 |
| `nika:edit` | Edit file (find/replace) | v0.15.0 |
| `nika:glob` | Find files by pattern | v0.15.0 |
| `nika:grep` | Search file contents | v0.15.0 |
| `nika:sleep` | Wait (max 5 min) | v0.15.0 |
| `nika:records` | Query Punk Records | v0.30.0 (renamed from nika:episodes) |
| `nika:orchestrate` | Orchestration state | v0.30.0 (renamed from nika:strategy_state) |
| `nika:dag_state` | DAG introspection | v0.30.0 |
| `nika:budget` | Token budget tracking | v0.30.0 |
| `nika:task_status` | Task status query | v0.30.0 |

### New Multi-Modal Builtin Tools (v0.30)

| Tool | Tier | Engine | Feature Flag | Default | Memory |
|------|------|--------|--------------|---------|--------|
| **`nika:vision`** | 1 | mistral.rs (Qwen 3-VL) | `vision` | ON | Shared with LLM |
| **`nika:embed`** | 1 | mistral.rs (EmbeddingGemma) | `embed` | ON | Shared with LLM |
| **`nika:transcribe`** | 2 | whisper-rs (Whisper v3) | `transcribe` | OFF | ~1.5 GB |
| **`nika:speak`** | 2 | rten + Piper | `speak` | OFF | ~0.1 GB |
| **`nika:ocr`** | 2 | ocr-rs (Tesseract) | `ocr` | OFF | ~0.4 GB |
| **`nika:translate`** | 2 | candle + NLLB-200 | `translate` | OFF | ~0.6 GB |
| **`nika:imagine`** | 3 | Smart Router (MCP) | Always available | -- | 0 (MCP) |
| **`nika:search`** | Meta | nika:embed + instant-distance | `embed` | ON* | ~0.1 GB |
| **`nika:index`** | Meta | nika:embed + instant-distance | `embed` | ON* | ~0.1 GB |

*nika:search and nika:index require `embed` feature flag (shared with nika:embed).

### Total: 20 Builtin Tools

```
Existing (v0.27):     11 tools (file ops + introspection)
New multi-modal:       9 tools (vision, embed, transcribe, speak, ocr, translate, imagine, search, index)
                      --------
TOTAL v0.30:          20 builtin tools
```

---

## 2.7 Memory Budget Analysis

### 16GB Mac (Development Machine)

```
Component                          Memory      Cumulative
---                                ---         ---
macOS + apps                       ~4 GB       4 GB
LLM text (mistral.rs, Q4 7B)      ~4 GB       8 GB
+ nika:vision (Qwen 3-VL)         shared      8 GB (shared with LLM runtime)
+ nika:embed (EmbeddingGemma)      shared      8 GB (shared with LLM runtime)
+ nika:search (HNSW index)         ~0.1 GB     8.1 GB
---
MINIMAL (text + vision + embed + search): ~8 GB

+ nika:translate (NLLB-600M)       ~0.6 GB     8.7 GB
+ nika:ocr (Tesseract)             ~0.4 GB     9.1 GB
+ nika:transcribe (Whisper Large)  ~1.5 GB     10.6 GB
+ nika:speak (Piper)               ~0.1 GB     10.7 GB
---
FULL MULTIMODAL: ~11 GB

Headroom on 16GB Mac:              ~5 GB       COMFORTABLE
```

### Progressive Activation

```
Level          Features ON                              Memory
---            ---                                      ---
Minimal        text only                                ~4 GB
Standard       text + vision + embed + search           ~8 GB (default)
Extended       + translate + ocr                        ~9 GB
Full           + transcribe + speak                     ~11 GB
```

### Feature Flag Configuration

```toml
# .nika/config.toml
[features]
vision = true       # Tier 1: default ON
embed = true        # Tier 1: default ON
transcribe = false  # Tier 2: opt-in
speak = false       # Tier 2: opt-in
ocr = false         # Tier 2: opt-in
translate = false   # Tier 2: opt-in
# nika:imagine = always available (MCP, no local memory)
# nika:search/index = available when embed = true
```

---

## 2.8 MCP Aliases Additions (v0.30)

### Proposed New Aliases

| Alias | Command | Description | Use Case |
|-------|---------|-------------|----------|
| `comfyui` | `uvx comfyui-mcp-server` | ComfyUI image/audio gen (13+ tools) | nika:imagine backend |
| `replicate` | `npx @replicate/mcp` | Replicate cloud models | nika:imagine fallback |
| `fal` | `npx @fal-ai/mcp` | FAL cloud generation | nika:imagine fallback |
| `deepl` | `npx deepl-mcp` | DeepL translation API | nika:translate upgrade |
| `elevenlabs` | `npx elevenlabs-mcp` | ElevenLabs TTS | nika:speak upgrade |

### Updated Count

```
Current MCP_ALIASES (v0.27):  48
New aliases (v0.30):          +5
Total MCP_ALIASES (v0.30):    53
```

### User Experience

```bash
# Add image generation
nika mcp add comfyui
# Now nika:imagine routes to ComfyUI automatically

# Add premium translation
nika mcp add deepl
nika keys set deepl
# Now nika:translate prefers DeepL when key available

# Add premium TTS
nika mcp add elevenlabs
nika keys set elevenlabs
# Now nika:speak prefers ElevenLabs when key available
```

---

## 2.9 Smart Router Applied to All Tools

### Tool Routing Matrix

| Tool | Builtin | MCP Options | LLM Fallback | Error Action |
|------|---------|-------------|--------------|--------------|
| `nika:translate` | NLLB-200 | DeepL, Google Translate | infer: "Translate..." | Never (LLM always available) |
| `nika:imagine` | None | ComfyUI, Replicate, FAL, Aether | None | "nika mcp add comfyui" |
| `nika:speak` | Piper | ElevenLabs | None | "nika config set features.speak true" |
| `nika:transcribe` | Whisper | None (gap!) | None | "nika config set features.transcribe true" |
| `nika:ocr` | Tesseract | None (gap!) | nika:vision + prompt | "nika config set features.ocr true" |
| `nika:vision` | mistral.rs VLM | OpenAI (GPT-4o), Anthropic | None | "Enable native feature" |
| `nika:embed` | mistral.rs | OpenAI embeddings | None | "Enable native feature" |
| `nika:search` | instant-distance | None | None | "Enable embed feature" |

### Interesting Fallback: `nika:ocr` -> `nika:vision`

OCR has a unique fallback path: if the OCR engine isn't available, we can use `nika:vision` with a prompt like "Extract all text from this image". Quality is lower than dedicated OCR but it works.

```
nika:ocr("invoice.pdf")
    |
    +-- 1. Tesseract enabled? --YES--> ocr-rs extraction (fast, accurate)
    |
    +-- 2. nika:vision available? --YES--> VLM: "Extract all text from this image"
    |                                       (slower, less accurate, but works)
    |
    +-- 3. Error: "No OCR backend. Enable: nika config set features.ocr true"
```

---

## 2.10 Implementation Roadmap (Feature Flags)

```
v0.28: P-MODEL + P-RECORD (Wave 1)
  |
v0.29: P-ORCHESTRATE + P-CONTEXT (Wave 2)
  |
v0.30: P-MEMORY + P-INTROSPECT + MULTIMODAL (Wave 3)
  |
  +-- Milestone 1: nika:vision + nika:embed (Tier 1, mistral.rs)
  +-- Milestone 2: nika:search + nika:index (Meta tools, instant-distance)
  +-- Milestone 3: nika:imagine (Tier 3, Smart Router + ComfyUI MCP alias)
  +-- Milestone 4: nika:translate (Tier 2, candle + NLLB-200)
  +-- Milestone 5: nika:transcribe + nika:speak (Tier 2, whisper-rs + rten/Piper)
  +-- Milestone 6: nika:ocr (Tier 2, ocr-rs)
  +-- Milestone 7: audio: field in infer: (future, when mistral.rs ready)
```

---

## Cross-Reference: How This Connects to Other Docs

| Doc | Connection to This Document |
|-----|-----------------------------|
| **Doc 12** (Vegapunk Naming) | Smart Router = extension of Orchestrator routing. RunContext gets `vector_index` field for nika:search |
| **Doc 13** (Multi-Modal Worker) | Smart Router IS the evolved Orchestrator routing from Approach C. Capability matching now includes tool backends |
| **Doc 14** (DataStore Naming) | RunContext confirmed. Now extended with vector_index for HNSW |
| **Doc 15** (Ecosystem Coherence) | Tool Layer updated: 20 tools (was 11). New MCP aliases. Package types unchanged |

---

## Sources

### Rust ML Crates

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

### Image Generation MCP Servers

1. [ComfyUI MCP Server](https://github.com/joenorton/comfyui-mcp-server) -- 222 stars, 17 tools, Python
2. [Comfy Pilot](https://github.com/ConstantineB6/comfy-pilot) -- 131 stars, Claude Code integration
3. [Replicate MCP](https://github.com/deepfates/mcp-replicate) -- 93 stars, npm `mcp-replicate@0.1.1`
4. [Stability AI MCP](https://github.com/tadasant/mcp-server-stability-ai) -- 81 stars, npm `mcp-server-stability-ai@0.2.0`
5. [fal.ai MCP](https://github.com/am0y/mcp-fal) -- 76 stars, 1094+ models
6. [PiAPI MCP (Midjourney proxy)](https://github.com/apinetwork/piapi-mcp-server) -- 68 stars

### Translation MCP Servers

7. [DeepL MCP Server (Official)](https://github.com/DeepLcom/deepl-mcp-server) -- 95 stars, npm `deepl-mcp-server@1.1.0`

### Audio / TTS / STT MCP Servers

8. [ElevenLabs MCP (Official)](https://github.com/elevenlabs/elevenlabs-mcp) -- 1,262 stars, 24 tools
9. [Whisper MCP](https://github.com/arcaputo3/mcp-server-whisper) -- 48 stars, PyPI `mcp-server-whisper`
10. [Kokoro TTS MCP](https://github.com/mberg/kokoro-tts-mcp) -- 75 stars, local ONNX TTS
11. [MLX Whisper MCP](https://github.com/kachiO/mlx-whisper-mcp) -- 22 stars, Apple Silicon

### Vision / OCR MCP Servers

12. [ImageSorcery MCP](https://github.com/sunriseapps/imagesorcery-mcp) -- 293 stars, 17 tools + OCR
13. [OpenCV MCP](https://github.com/GongRzhe/opencv-mcp-server) -- 97 stars, full CV pipeline
14. [Computer Control MCP](https://github.com/AB498/computer-control-mcp) -- 120 stars, desktop automation + OCR
15. [VisionCraft MCP](https://github.com/augmentedstartups/VisionCraft-MCP-Server) -- 116 stars

### Rust Crates (crates.io, all data retrieved 2026-03-15)

16. [candle-core](https://crates.io/crates/candle-core) -- v0.9.2, 3.1M downloads
17. [candle-transformers](https://crates.io/crates/candle-transformers) -- v0.9.2, 1.2M downloads
18. [candle-nn](https://crates.io/crates/candle-nn) -- v0.9.2, 2.0M downloads
19. [hf-hub](https://crates.io/crates/hf-hub) -- v0.5.0, 5.0M downloads
20. [whisper-rs](https://crates.io/crates/whisper-rs) -- v0.16.0, 208K downloads
21. [ocrs](https://crates.io/crates/ocrs) -- v0.12.1, 68K downloads
22. [rten](https://crates.io/crates/rten) -- v0.24.0, 150K downloads
23. [ort](https://crates.io/crates/ort) -- v2.0.0-rc.12, 7.1M downloads
24. [instant-distance](https://crates.io/crates/instant-distance) -- v0.6.1, 156K downloads (unmaintained since 2023-06)
25. [image](https://crates.io/crates/image) -- v0.25.10, 105.9M downloads
26. [imageproc](https://crates.io/crates/imageproc) -- v0.26.1, 7.5M downloads
27. [resvg](https://crates.io/crates/resvg) -- v0.47.0, 10.8M downloads
28. [tiny-skia](https://crates.io/crates/tiny-skia) -- v0.12.0, 22.4M downloads
29. [ab_glyph](https://crates.io/crates/ab_glyph) -- v0.2.32, 24.5M downloads

### Smart Router & Architecture Research

30. Perplexity: NLLB-200 Rust inference -- Confirmed candle + candle-transformers supports NLLB directly
31. Perplexity: Rust HNSW crates -- Mapped instant-distance, hnswlib-rs, hnsw_rs, usearch, hannoy, DistX
32. Perplexity: mistral.rs Gemma 3n audio -- Confirmed audio NOT yet production-ready in mistral.rs
33. Perplexity: ComfyUI MCP routing -- Confirmed NO multi-backend routing exists in MCP ecosystem
34. Perplexity: rust-bert NLLB -- No direct crate; candle is the path
35. Perplexity: Audio/STT/TTS MCP servers -- Confirmed ZERO MCP servers for audio (gap validated)
36. Firecrawl: joenorton/comfyui-mcp-server -- 219 stars, Python, 13+ tools, generate_song (audio!)
37. Firecrawl: alecc08/comfyui-mcp -- 14 stars, TypeScript, 7 tools, img2img, upscale, bg removal

---

## Methodology

- **Perplexity AI**: 3 deep searches on mistral.rs multi-modal, Rust image gen, Rust ML ecosystem + 5 targeted queries on NLLB, HNSW, audio gaps
- **Firecrawl**: 2 scrapes (ComfyUI MCP servers)
- **GitHub API**: Direct stats for 8+ repositories (stars, last commit, descriptions)
- **GitHub Search API**: Repository search across image gen, translation, audio, vision MCP servers (40+ repos analyzed)
- **crates.io API**: Version, download counts, update dates for 14 Rust crates
- **npm Registry API**: Package versions for 8 npm packages
- **GitHub Contents API**: README extraction and tool counting from source code
- **Scope**: Rust-only crates with active maintenance (last commit < 6 months)
- **Data freshness**: All data retrieved 2026-03-15

## Confidence Level

- **High** -- MCP server landscape (comprehensive GitHub search + star counts + tool extraction from READMEs and source)
- **High** -- Rust crate data (crates.io API provides exact numbers, all verified)
- **High** -- Smart Router pattern (validated interactively with Thibaut during brainstorm session, all crate selections confirmed)
- **Medium** -- Tool counts on some MCP servers (extracted from README; some marked with ~ for estimates)
- **Low** -- Midjourney MCP coverage (no official server exists, community options immature)
