# 18 -- MCP Multimodal Ecosystem Research (March 2026)

> **Research Date**: 2026-03-15
> **Scope**: Current state of MCP servers for multimodal capabilities and Rust crates for local inference
> **Method**: GitHub Search API, crates.io API, npm registry API, GitHub Contents API (README extraction)
> **Dependencies**: Doc 13 (Multi-Modal Worker Architectures), Doc 16 (Multi-Modal Builtin Research), Doc 17 (Smart Router)
> **Status**: RESEARCH COMPLETE

---

## Executive Summary

The MCP multimodal ecosystem in March 2026 is **active but fragmented**. Image generation has the
most mature MCP server coverage (ComfyUI, Replicate, fal.ai, Stability AI all have working servers).
Audio/TTS is dominated by the official ElevenLabs MCP server (1,262 stars, 24 tools). Translation
has an official DeepL server but no Google Translate MCP. Vision/OCR is served by general-purpose
image processing servers (ImageSorcery, OpenCV MCP) rather than dedicated OCR servers.

On the Rust crate side, the ecosystem is production-ready for core ML operations: `candle-core`
(3.1M downloads), `hf-hub` (5M downloads), `whisper-rs` (208K downloads), and `ocrs` (68K downloads)
provide solid foundations for local multimodal processing without MCP overhead.

---

## Table of Contents

1. [Image Generation MCP Servers](#1-image-generation-mcp-servers)
2. [Translation MCP Servers](#2-translation-mcp-servers)
3. [Audio / TTS / STT MCP Servers](#3-audio--tts--stt-mcp-servers)
4. [Vision / OCR MCP Servers](#4-vision--ocr-mcp-servers)
5. [Rust Crates for Multimodal](#5-rust-crates-for-multimodal)
6. [Synthesis for Nika](#6-synthesis-for-nika)
7. [Sources](#7-sources)

---

## 1. Image Generation MCP Servers

### Comparison Matrix

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

### Detailed Analysis

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

## 2. Translation MCP Servers

### Comparison Matrix

| Server | GitHub | Stars | Lang | Tools | Package | Last Updated |
|--------|--------|------:|------|------:|---------|:------------:|
| **DeepL MCP** (Official) | `DeepLcom/deepl-mcp-server` | 95 | JavaScript | 8 | npm `deepl-mcp-server@1.1.0` | 2026-03-13 |
| **Google Translate MCP** | -- | -- | -- | -- | -- | Does not exist |
| **KlicStudio MCP** | `krillinai/KlicStudioMCP` | 20 | Python | ~5 | PyPI | 2026-03-14 |
| **AllVoiceLab MCP** | `allvoicelab/AllVoiceLab-MCP` | 56 | Python | ~8 | PyPI | 2026-01-22 |

### Detailed Analysis

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

## 3. Audio / TTS / STT MCP Servers

### Comparison Matrix

| Server | GitHub | Stars | Lang | Tools | Package | Last Updated |
|--------|--------|------:|------|------:|---------|:------------:|
| **ElevenLabs MCP** (Official) | `elevenlabs/elevenlabs-mcp` | 1,262 | Python | 24 | PyPI (uvx) | 2026-03-14 |
| **Whisper MCP** | `arcaputo3/mcp-server-whisper` | 48 | Python | ~12 | PyPI `mcp-server-whisper` | 2026-03-12 |
| **Kokoro TTS MCP** | `mberg/kokoro-tts-mcp` | 75 | Python | ~3 | PyPI (uvx) | 2026-03-12 |
| **MLX Whisper MCP** | `kachiO/mlx-whisper-mcp` | 22 | Python | ~4 | PyPI (uvx) | 2026-02-25 |
| **Fast Whisper MCP** | `BigUncle/Fast-Whisper-MCP-Server` | 14 | Python | ~4 | PyPI | 2026-02-05 |
| **Local STT MCP** | `SmartLittleApps/local-stt-mcp` | 11 | TypeScript | ~3 | npm | 2026-03-06 |
| **VoiceMode** (2-way voice) | `mbailey/voicemode` | 894 | Python | -- | PyPI | 2026-03-14 |

### Detailed Analysis

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

## 4. Vision / OCR MCP Servers

### Comparison Matrix

| Server | GitHub | Stars | Lang | Tools | Package | Last Updated |
|--------|--------|------:|------|------:|---------|:------------:|
| **ImageSorcery MCP** | `sunriseapps/imagesorcery-mcp` | 293 | Python | 17 | PyPI (uvx) | 2026-03-13 |
| **Computer Control MCP** | `AB498/computer-control-mcp` | 120 | Python | ~8 | PyPI | 2026-03-05 |
| **VisionCraft MCP** | `augmentedstartups/VisionCraft-MCP-Server` | 116 | JavaScript | ~10 | npm | 2026-03-13 |
| **OpenCV MCP** | `GongRzhe/opencv-mcp-server` | 97 | Python | ~20 | PyPI | 2026-03-13 |
| **ScreenMonitor MCP** | `inkbytefo/ScreenMonitorMCP` | 71 | Python | ~5 | PyPI | 2026-03-03 |
| **Tesseract MCP** | -- | -- | -- | -- | -- | Does not exist |

### Detailed Analysis

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

## 5. Rust Crates for Multimodal

### Core ML Crates

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

### Image Processing Crates

| Crate | Version | Total DL | Recent DL | Last Updated | Key Feature |
|-------|---------|----------:|----------:|:------------:|-------------|
| **`image`** | 0.25.10 | 105,883,495 | 18,137,277 | 2026-03-10 | Core imaging (encode/decode/process) |
| **`imageproc`** | 0.26.1 | 7,480,118 | 1,709,000 | 2026-02-28 | Image processing ops |
| **`resvg`** | 0.47.0 | 10,758,922 | 3,064,587 | 2026-02-09 | SVG rendering |
| **`tiny-skia`** | 0.12.0 | 22,437,269 | 5,594,682 | 2026-02-02 | 2D graphics (Skia subset, pure Rust) |
| **`ab_glyph`** | 0.2.32 | 24,466,318 | 4,956,007 | 2025-09-28 | Font glyph loading + rasterization |

### Detailed Crate Analysis

#### `candle-core` + `candle-transformers` + `candle-nn` (HuggingFace)

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

#### `hf-hub` v0.5.0

Repository: https://github.com/huggingface/hf-hub

The standard Rust crate for downloading models from HuggingFace. 5M+ downloads makes it the
most downloaded crate in this list. Keywords: `machine-learning`, `huggingface`, `hf`, `hub`.

Features: Async downloads, cache management, repo API, model card parsing.

**Nika relevance**: Already a dependency of the native inference pipeline. Essential for
model management via `nika model pull`.

#### `whisper-rs` v0.16.0

Repository: https://codeberg.org/tazz4843/whisper-rs

Rust bindings for whisper.cpp (ggerganov's C++ implementation). v0.16.0 was published
**3 days ago** (2026-03-12). 93K recent downloads shows active and growing usage.

Features: Multiple model sizes (tiny to large), language detection, word-level timestamps,
voice activity detection (VAD), full API coverage of whisper.cpp.

**Nika relevance**: Strongest candidate for `nika:transcribe` builtin tool. Local STT
without needing an MCP server or OpenAI API key.

#### `ocrs` v0.12.1

Repository: https://github.com/robertknight/ocrs

Pure Rust OCR engine by the same author as `rten`. v0.12.1 (Feb 2026). Uses ONNX models
via `rten` for text detection and recognition. Zero C dependencies.

Features: Text detection + recognition pipeline, word/line bounding boxes, pure Rust.

**Nika relevance**: Prime candidate for `nika:ocr` builtin tool. No Python or C deps.
Perfect for the QR Code AI use case (reading text from QR code destinations).

#### `rten` v0.24.0 vs `ort` v2.0.0-rc.12

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

#### `instant-distance` v0.6.1

Repository: https://github.com/InstantDomain/instant-distance

HNSW (Hierarchical Navigable Small World) vector search. v0.6.1 but **last updated June 2023**.
Still functional but appears unmaintained. 28K recent downloads suggests continued passive usage.

**Nika relevance**: Could power local embedding search (satellite routing, context retrieval).
However, unmaintained status is concerning. Alternatives worth evaluating:
- `usearch` crate (more active)
- `hnsw` crate
- Custom implementation with `candle-core` tensors

---

## 6. Synthesis for Nika

### MCP Server Recommendations for `MCP_ALIASES`

Based on this research, the following servers are recommended for inclusion in Nika's MCP alias
registry (currently 48 aliases in `src/core/mcp_aliases.rs`):

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

### Native Capabilities (No MCP Needed)

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

### Gap Analysis

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

### Recommended Architecture

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

## 7. Sources

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

---

## Methodology

- **GitHub Search API**: Repository search across image gen, translation, audio, vision MCP servers (40+ repos analyzed)
- **crates.io API**: Version, download counts, update dates for 14 Rust crates
- **npm Registry API**: Package versions for 8 npm packages
- **GitHub Contents API**: README extraction and tool counting from source code
- **Data freshness**: All data retrieved 2026-03-15

## Confidence Level

**High** -- MCP server landscape (comprehensive GitHub search + star counts + tool extraction from READMEs and source).
**High** -- Rust crate data (crates.io API provides exact numbers, all verified).
**Medium** -- Tool counts on some MCP servers (extracted from README; some marked with ~ for estimates).
**Low** -- Midjourney MCP coverage (no official server exists, community options immature).
