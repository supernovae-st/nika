# Research Report: Rust Crates for ML Inference in Media Processing

**Date**: 2026-03-18
**Researcher**: Claude Opus 4.6 (1M context)
**Relevance**: Nika media pipeline -- native ML inference for super resolution, style transfer, object detection, classification, segmentation

---

## Summary

The Rust ML inference ecosystem has matured significantly by early 2026. There are **6 major inference runtimes** (`ort`, `candle`, `burn`, `tract`, `rten`, `lele`), each with distinct trade-offs between performance, portability, and dependency weight. For Nika's media pipeline, the recommended stack is **`ort` for GPU-heavy tasks** (super resolution, style transfer) and **`candle` for vision models with HuggingFace ecosystem integration**. Pure-Rust options (`rten`, `tract`, `lele`) are viable for edge/WASM but lack GPU acceleration.

No dedicated Rust crates exist for super resolution, style transfer, or background removal -- these tasks require loading pre-trained ONNX/safetensors models through a general-purpose runtime.

---

## 1. General-Purpose Inference Runtimes

### 1.1 ort -- ONNX Runtime Bindings (RECOMMENDED for GPU)

| Property | Value |
|----------|-------|
| **Crate** | [`ort`](https://crates.io/crates/ort) |
| **Version** | `2.0.0-rc.12` (production-ready, API-unstable) |
| **ONNX Runtime** | 1.24 |
| **Maintainer** | pyke.io |
| **Approach** | Safe Rust bindings to C++ ONNX Runtime |
| **Docs** | https://ort.pyke.io |

**Execution Providers**: CUDA, CoreML, Metal, DirectML, TensorRT, ROCm, OpenVINO, NNAPI, CPU (with hardware-specific optimizations).

**API Example**:
```rust
use ort::{Session, SessionBuilder};

let session = Session::builder()?
    .with_optimization_level(GraphOptimizationLevel::Level3)?
    .commit_from_file("model.onnx")?;

let outputs = session.run(ort::inputs![input_tensor]?)?;
```

**Strengths**:
- Widest hardware support (every major GPU vendor + mobile)
- Production-proven (used by Bloop, Microsoft internal projects)
- Highest raw throughput for GPU inference
- Supports training + inference

**Weaknesses**:
- C++ dependency (complicates cross-compilation, WASM)
- Large binary size (~50-100MB with runtime)
- API still alpha (breaking changes possible before 2.0 stable)

**Verdict**: Best choice for GPU-accelerated media processing. Use for super resolution, style transfer, any model where throughput matters.

---

### 1.2 candle -- Hugging Face ML Framework

| Property | Value |
|----------|-------|
| **Crate** | [`candle-core`](https://crates.io/crates/candle-core) + `candle-nn` + `candle-transformers` |
| **Version** | ~0.8.x (frequent releases) |
| **Maintainer** | Hugging Face |
| **GitHub Stars** | 19k+ |
| **Approach** | Pure Rust ML framework, PyTorch-like API |

**Backends**: CUDA, Metal (Apple Silicon), CPU (MKL on Linux, Accelerate on macOS), WASM.

**Supported Vision Models** (out of the box):
- Stable Diffusion 1.5 / 2.1 / SDXL / Turbo
- BLIP (image captioning)
- CLIP (image-text similarity)
- TrOCR (OCR)
- DINOv2, ViT, ResNet, VGG, EfficientNet
- ConvNeXT, MobileNetv4, RepVGG

**Supported LLMs**: Llama 1/2/3, Mistral, Mixtral, Phi-1/2/3, Gemma 1/2, Qwen, Yi, Falcon, StarCoder.

**Supported Audio**: Whisper (speech-to-text), Mamba.

**Strengths**:
- PyTorch-like ergonomics in Rust
- Direct Hugging Face Hub integration (download models by ID)
- safetensors native support
- WASM support for browser deployment
- f16/bf16 precision, GGUF quantization (4-bit/8-bit)
- Flash attention support

**Weaknesses**:
- No ONNX import (safetensors only, or re-implement model architecture)
- Does not outperform heavily tuned PyTorch in isolated benchmarks
- Narrower execution provider support than ort

**Verdict**: Best for HuggingFace model ecosystem. Ideal when you want Stable Diffusion, CLIP, or BLIP in Rust without ONNX conversion. Excellent for image classification/captioning.

---

### 1.3 burn -- Tracel Deep Learning Framework

| Property | Value |
|----------|-------|
| **Crate** | [`burn`](https://crates.io/crates/burn) |
| **Version** | 0.19.0 (stable), 0.20.0 (demos) |
| **Maintainer** | Tracel AI |
| **GitHub Stars** | 13.2k |
| **Approach** | Framework with swappable backends, ONNX import |

**Backends**: WGPU (Metal, Vulkan, WebGPU via CubeCL), Tch (LibTorch), Ndarray (CPU), Candle, LLVM (SIMD-optimized CPU, new in 0.19).

**Key Features**:
- **ONNX import**: Load PyTorch/TF models converted to ONNX, then run on any backend
- Dynamic graphs with static-graph performance (JIT, kernel fusion)
- INT4/INT8 quantization (per-tensor, per-block)
- Distributed multi-GPU training (DDP)
- Autodiff for training

**Strengths**:
- Backend-agnostic: same code runs on CUDA, Metal, WebGPU, CPU
- ONNX import pipeline (convert once, deploy anywhere)
- Active development, 224 contributors
- WebGPU/WASM ready via WGPU backend

**Weaknesses**:
- Younger ecosystem, fewer pre-built model examples than candle
- Some backends experimental (ROCm, LLVM)
- No quantitative benchmarks published vs. PyTorch

**Verdict**: Best for portable inference where the same model must run on GPU, CPU, and WASM. The ONNX import is a differentiator. Good for Nika's cross-platform story.

---

### 1.4 tract -- Sonos Inference Engine

| Property | Value |
|----------|-------|
| **Crate** | [`tract-onnx`](https://crates.io/crates/tract-onnx) |
| **Maintainer** | Sonos |
| **Approach** | Pure Rust, ONNX/NNEF/TFLite, edge-optimized |

**Key Features**:
- ONNX, NNEF, TensorFlow Lite format support
- Graph optimization and streaming for real-time audio
- Symbolic dynamic shapes
- Quantization support
- `torch-to-nnef` companion tool for PyTorch export

**Strengths**:
- Battle-tested in production on millions of Sonos devices
- Lightweight, no C++ dependencies
- Excellent for audio/streaming workloads
- Predictable latency on constrained hardware

**Weaknesses**:
- CPU-only (no GPU acceleration)
- Primarily optimized for audio/NLP, not vision
- Fewer vision operators than ort

**Verdict**: Best for audio processing tasks in Nika (Whisper, VAD). Not recommended for heavy vision workloads.

---

### 1.5 rten -- Pure Rust ONNX Runtime

| Property | Value |
|----------|-------|
| **Crate** | [`rten`](https://crates.io/crates/rten) |
| **Maintainer** | Robert Knight |
| **Approach** | Pure Rust ONNX runtime, CPU + WASM |

**Key Features**:
- End-to-end Rust dependencies (no C/C++)
- float32, int8/uint8 quantized, int4 blockwise
- CPU intrinsics: VNNI (x86), UDOT/i8mm (Arm)
- Control flow ops (If, Loop, Scan)
- Custom protobuf parser (minimal copies)
- WASM-ready, small binaries

**Strengths**:
- Zero C++ dependencies = easy cross-compilation
- Good quantization support
- Broad ONNX operator coverage (18 new ops in 2025)

**Weaknesses**:
- CPU-only (GPU planned but not shipped)
- No training
- Smaller community than ort/candle

**Verdict**: Great for WASM and edge deployment. Good fallback when ort's C++ deps are problematic.

---

### 1.6 lele -- AOT ONNX-to-Rust Compiler (NEW, 2026)

| Property | Value |
|----------|-------|
| **Crate** | [`lele`](https://crates.io/crates/lele) |
| **Released** | January 2026 |
| **Approach** | Compiles ONNX models into specialized Rust code with hand-crafted SIMD kernels |

**Benchmarks vs ONNX Runtime** (CPU):

| Model | ORT Latency | Lele Latency | Speedup |
|-------|-------------|--------------|---------|
| Silero VAD | 0.0031 RTF | 0.0016 RTF | **1.93x** |
| SenseVoice | 0.032 RTF | 0.0254 RTF | **1.26x** |
| Supertonic (TTS) | 0.130 RTF | 0.0824 RTF | **1.58x** |
| YOLOv26 | 925.59ms | 478.85ms | **1.93x** |

**Supported Models**: Silero VAD, SenseVoice (ASR), Supertonic (TTS), YOLOv26.

**Key Features**:
- Zero runtime dependencies (~1.7MB WASM binaries)
- SIMD: NEON (Apple Silicon), AVX/SSE (x86), WASM SIMD128
- Static buffer allocation, zero-copy weight loading
- Bare-metal compatible

**Strengths**:
- Fastest CPU inference (beats ONNX Runtime by 1.3-1.9x)
- Smallest possible binary (model baked into code)
- No allocations after warmup

**Weaknesses**:
- Very limited model support (4 models)
- No GPU support
- Early stage, not production-ready for general use
- Adding new models requires manual kernel work

**Verdict**: Fascinating approach. Watch for maturity. Could be excellent for specific hot-path models in Nika's pipeline (VAD, lightweight detection).

---

## 2. Task-Specific Analysis

### 2.1 Super Resolution / Image Upscaling

**Status**: No dedicated Rust crate exists.

**Approach**: Load Real-ESRGAN, ESRGAN, or SwinIR ONNX models through `ort` or `burn`.

```
Pipeline:
1. Export model: PyTorch -> ONNX (Real-ESRGAN, SwinIR)
2. Load in Rust: ort::Session::builder().commit_from_file("realesrgan_x4.onnx")
3. Preprocess: image crate -> ndarray -> ort tensor
4. Infer: session.run(inputs)
5. Postprocess: ort tensor -> ndarray -> image crate -> output
```

**Models available as ONNX**:
- Real-ESRGAN x4 (general upscaling)
- ESRGAN x4 (photorealistic)
- SwinIR (state-of-the-art quality)
- NTIRE 2026 competition models (bleeding edge)

**GPU recommended**: These models are compute-heavy. Use `ort` with CUDA/Metal.

---

### 2.2 Style Transfer

**Status**: No dedicated Rust crate exists.

**Approaches**:
1. **ONNX Model Zoo** fast_neural_style models via `ort`
2. **Candle** with VGG19 for Gatys-style iterative transfer
3. **Pre-trained AdaIN/arbitrary style transfer** via ONNX

```rust
// Fast style transfer via ort
let session = Session::builder()?.commit_from_file("mosaic_style.onnx")?;
let styled = session.run(ort::inputs![content_tensor]?)?;
```

**ONNX Hub Models**: `vision/style_transfer/fast_neural_style` (Mosaic, Candy, Rain Princess, Udnie, Pointilism).

---

### 2.3 Object Detection (YOLO)

**Status**: Multiple Rust crates exist.

| Crate | YOLO Versions | Backend | Benchmark |
|-------|---------------|---------|-----------|
| `yolo-rs` | YOLOv11 | ONNX | 57ms (M3 Max, YOLOv11x) |
| `yolo_detector` | Multiple | OpenCV + ONNX | -- |
| `object-detection-opencv-rust` | v3, v4, v5, v7, v8 | OpenCV DNN | -- |
| `lele` | YOLOv26 | AOT Rust | 478ms (vs 925ms ORT) |

**Latest YOLO**: YOLOv26 (Jan 2026) -- NMS-free, end-to-end inference, quantization-stable.

**Recommended for Nika**:
- Quick integration: `yolo-rs` (ONNX-based, YOLOv11)
- Maximum performance: `ort` with YOLOv26 ONNX export
- Edge/WASM: `lele` with YOLOv26 (1.93x faster than ORT on CPU)

---

### 2.4 Image Classification

**Status**: Well-supported via candle and ort.

**Candle built-in models**: ResNet, VGG, ViT, DINOv2, EfficientNet, ConvNeXT, MobileNetv4, RepVGG.

**Via ONNX (ort)**: Any ImageNet model (ResNet-50, EfficientNet-B7, ViT-L, etc.) from ONNX Model Zoo.

---

### 2.5 Image Segmentation / Background Removal

**Status**: Very limited native Rust support.

| Crate | Type | Backend |
|-------|------|---------|
| `segmentation-models-burn` | Semantic segmentation (UNet++) | Burn |
| `onnx-infer` | CV-focused pure Rust ONNX | Pure Rust (CPU) |

**No native Rust SAM (Segment Anything) or rembg equivalent.**

**Approach**: Export SAM/rembg/U2-Net to ONNX, run via `ort`.

```
Models for background removal (ONNX):
- U2-Net (lightweight, good quality)
- IS-Net (DIS, high quality)
- SAM (Segment Anything, interactive)
- MODNet (real-time matting)
```

---

### 2.6 Audio: Speech Recognition (Whisper)

**Status**: Strong ecosystem.

| Crate | Approach | Notes |
|-------|----------|-------|
| `whisper-rs` (v0.15.1) | Bindings to whisper.cpp | CUDA, Metal, Vulkan, ROCm, OpenBLAS |
| `whisper-apr` (v0.2.4) | Pure Rust, WASM-first | INT4/INT8 quantization, 99 languages, streaming |
| `whisper-cli-rs` | CLI via whisper.cpp | Significantly faster than Python on M1 |
| `whisp` | Desktop app | Local Whisper with Metal/CoreML (~3x speedup) |

**whisper-apr** is notable: pure Rust, WASM-ready, supports large-v3-turbo (809M params), GGUF loading.

---

## 3. Decision Matrix for Nika

| Task | Recommended Crate | Why |
|------|--------------------|-----|
| Super Resolution | `ort` | GPU required, ONNX models available |
| Style Transfer | `ort` | Fast neural style ONNX models |
| Object Detection | `ort` + YOLO ONNX | Best perf; `yolo-rs` for quick start |
| Image Classification | `candle` | Built-in models, HF ecosystem |
| Image Captioning | `candle` (BLIP) | Native support |
| Segmentation | `ort` + U2-Net ONNX | No native Rust alternative |
| Background Removal | `ort` + MODNet/U2-Net | No native Rust alternative |
| Speech-to-Text | `whisper-rs` | Production-ready, GPU support |
| Edge/WASM inference | `rten` or `burn` (WGPU) | Pure Rust, portable |
| Hot-path VAD | `lele` | 1.93x faster than ORT |

---

## 4. Recommended Architecture for Nika

```
                    +--------------------+
                    |  Nika Media Worker  |
                    +--------------------+
                             |
                    +--------+--------+
                    |                 |
              GPU Available?     CPU / WASM
                    |                 |
              +-----+-----+    +-----+-----+
              |           |    |           |
            ort 2.0    candle  rten      burn
          (ONNX RT)   (HF)   (pure)   (WGPU)
              |           |    |           |
         +----+----+      |    |      +----+----+
         |    |    |      |    |      |         |
       CUDA Metal DML   Metal CPU   CPU     WebGPU
```

### Dependency Strategy

**Option A: ort-only** (simplest)
```toml
[dependencies]
ort = { version = "2.0.0-rc.12", features = ["cuda", "coreml"] }
image = "0.25"
ndarray = "0.16"
```
- One runtime for all tasks
- Largest binary, C++ dependency
- Best GPU performance

**Option B: ort + candle** (recommended)
```toml
[dependencies]
ort = { version = "2.0.0-rc.12", features = ["cuda", "coreml"] }
candle-core = "0.8"
candle-nn = "0.8"
candle-transformers = "0.8"
image = "0.25"
```
- ort for ONNX models (super res, detection, segmentation)
- candle for HuggingFace models (classification, captioning, generation)
- Best ecosystem coverage

**Option C: burn-only** (most portable)
```toml
[dependencies]
burn = { version = "0.19", features = ["wgpu", "onnx"] }
image = "0.25"
```
- One framework, ONNX import
- Same code on GPU/CPU/WASM
- Weakest ecosystem of pre-built models

---

## 5. Performance Benchmark Summary

All benchmarks are from source reports; no independent validation.

| Operation | Crate | Hardware | Latency | Notes |
|-----------|-------|----------|---------|-------|
| YOLOv11x detection | yolo-rs (ort) | M3 Max | 57ms | Single image |
| YOLOv26 detection | lele | CPU (Apple Silicon) | 478ms | vs 925ms ORT |
| Silero VAD | lele | CPU | 0.0016 RTF | vs 0.0031 ORT |
| SenseVoice ASR | lele | CPU | 0.0254 RTF | vs 0.032 ORT |
| Whisper encoding | whisp | macOS Metal/CoreML | ~3x faster | vs CPU baseline |

---

## Sources

1. [ort crate docs](https://ort.pyke.io) -- ONNX Runtime Rust bindings documentation
2. [ort on crates.io](https://crates.io/crates/ort) -- Latest version 2.0.0-rc.12, wraps ONNX Runtime 1.24
3. [candle GitHub](https://github.com/huggingface/candle) -- 19k+ stars, HuggingFace ML framework
4. [Candle: Rust's Top Framework for Local LLM Inference in 2026](https://ryuru.com/candle-rusts-top-framework-for-local-llm-inference-in-2026/) -- Feb 2026 overview
5. [burn.dev](https://burn.dev) -- Tracel AI deep learning framework
6. [burn docs.rs](https://docs.rs/burn) -- v0.19.0, comprehensive DL framework
7. [tract FOSDEM 2026 talk](https://fosdem.org/2026/schedule/event/YJJQTD-tract-and-torch-to-nnef/) -- Sonos inference engine
8. [rten GitHub](https://github.com/robertknight/rten) -- Pure Rust ONNX runtime
9. [RTen in 2025](https://robertknight.me.uk/posts/rten-2025/) -- Year in review, 18 new operators
10. [lele on lib.rs](https://lib.rs/crates/lele) -- AOT ONNX-to-Rust compiler, benchmarks
11. [lele Rust Users Forum](https://users.rust-lang.org/t/lele-bare-metal-ml-inference-engine-in-pure-rust-compile-onnx-into-rust/138195) -- Feb 2026 announcement
12. [yolo-rs GitHub](https://github.com/pan93412/yolo-rs) -- YOLOv11 Rust implementation
13. [yolo-rs on lib.rs](https://lib.rs/crates/yolo-rs) -- Feb 2026 update
14. [segmentation-models-burn](https://github.com/femshima/segmentation-models-burn) -- UNet++ in Burn
15. [onnx-infer crate](https://crates.io/crates/onnx-infer) -- Pure Rust ONNX for CV, zero-heap
16. [whisper-rs docs.rs](https://docs.rs/crate/whisper-rs/latest) -- v0.15.1, whisper.cpp bindings
17. [whisper-apr on crates.io](https://crates.io/crates/whisper-apr) -- Pure Rust Whisper, WASM-first
18. [YOLOv26 overview](https://learnopencv.com/yolov26-real-time-deployment/) -- NMS-free, edge-first, Jan 2026

---

## Methodology

- **Tools used**: Perplexity AI (sonar model), 12 targeted searches
- **Sources analyzed**: 60+ (crates.io, GitHub, lib.rs, docs.rs, blog posts, conference talks)
- **Time period covered**: 2024-2026 (focus on 2025-2026 developments)
- **Search date**: 2026-03-18

## Confidence Level

**High** for runtime comparisons (ort, candle, burn, tract, rten) -- well-documented, active projects.
**Medium** for benchmarks -- mostly self-reported, no independent validation.
**Medium** for lele -- very new (Feb 2026), impressive but unproven at scale.
**Low** for super resolution / segmentation in Rust -- no dedicated crates, all approaches use generic ONNX runtimes.

## Further Research Suggestions

- Benchmark ort vs candle vs burn on identical vision models (ResNet-50, YOLOv8) on Apple Silicon
- Test Real-ESRGAN ONNX model through ort on M-series Mac (latency per image at various resolutions)
- Evaluate burn's ONNX import quality for complex vision models
- Track lele's roadmap for GPU support and broader model coverage
- Investigate onnx-infer (pure Rust, zero-heap CV inference) -- very new (March 2026)
- Profile memory usage of each runtime under sustained batch processing
