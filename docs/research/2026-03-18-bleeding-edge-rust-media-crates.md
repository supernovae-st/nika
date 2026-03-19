# Bleeding-Edge Rust Media Crates -- March 2026

> Research date: 2026-03-18
> Methodology: crates.io API queries (8 targeted searches), GitHub API star counts via `gh` CLI
> Scope: Crates released or significantly updated in late 2025 through March 2026

---

## Executive Summary

The Rust media ecosystem is entering a maturity inflection point. The established players
(`image-rs`, `zune-*`, `jxl-oxide`) continue to improve, while two genuinely new ecosystems
emerged in early 2026: the **imazen/zen\*** family (from the Imageflow team) and **OxiMedia**
(a broadcast-grade Rust media toolkit). On the ML side, **Burn 0.21** with **CubeCL** is the
first credible pure-Rust GPU compute story for image processing. JPEG XL now has two
competitive pure-Rust decoders. The biggest gap remains: no production-ready pure-Rust WebP2
or AV2 codec exists yet.

---

## Tier 1 -- Established Powerhouses (Battle-Tested, High Stars)

### image-rs/image
- **Version:** 0.25.10 (updated 2026-03-10)
- **Downloads:** 18.7M recent
- **Stars:** 5,694
- **Status:** Still the default. Now delegates to zune-jpeg for JPEG decoding.
  Supports WebP (pure Rust via `image-webp`), AVIF (via `ravif`), TIFF, PNG, BMP, etc.
- **Relevance:** Baseline -- everything else is measured against this.

### zune-jpeg / zune-image family
- **Version:** zune-jpeg 0.5.13 / zune-image 0.5.0 (updated 2026-03-11 / 2026-01-24)
- **Downloads:** zune-jpeg 15.7M recent | zune-image 10.5K recent
- **Stars:** 461 (etemesi254/zune-image)
- **Status:** The **fastest safe JPEG decoder** in Rust. Now used by image-rs internally.
  Family includes: zune-png (0.5.2), zune-bmp, zune-hdr, zune-psd, zune-qoi, zune-farbfeld,
  zune-jpegxl (encoder only, 0.5.2), zune-imageprocs (filters/transforms).
- **Competitive advantage:** 2-4x faster than image-rs for JPEG decode. SIMD-accelerated.
  The `zune-jpegxl` crate is a **pure-Rust JXL encoder** -- one of the only ones.

### Candle (Hugging Face)
- **Version:** 0.9.2 (updated 2026-01-24)
- **Downloads:** candle-core 1.7M | candle-transformers 439K
- **Stars:** 19,714
- **Status:** Most popular Rust ML framework by stars. Minimalist, GPU-accelerated.
  `candle-transformers` has pre-built vision models (CLIP, BLIP, SAM, etc.).
- **Relevance:** Best path for running pre-trained vision models in Rust today.

### Burn Framework
- **Version:** 0.21.0-pre.2 (updated 2026-03-02)
- **Downloads:** burn 200K recent
- **Stars:** 14,650
- **Sub-crates:** burn-wgpu (178K), burn-cuda (156K), burn-rocm (140K),
  burn-cubecl (153K), **burn-vision (2.8K -- NEW)**
- **Status:** The most **architecturally ambitious** Rust ML framework.
  Multi-backend: WGPU, CUDA, ROCm, CubeCL. The new `burn-vision` crate (0.21-pre)
  adds image transforms, augmentation, and preprocessing directly on tensors.
- **Key insight:** `burn-vision` is brand new and specifically targets the
  "image preprocessing for ML inference" use case.

### CubeCL (Tracel AI)
- **Version:** 0.10.0-pre.2 (updated 2026-03-02)
- **Downloads:** ~185K recent
- **Stars:** 2,056
- **Status:** **GPU compute language extension for Rust.** Compiles to WGSL, CUDA, ROCm
  shaders. This is the compute backend that powers Burn. Can also be used standalone
  for custom GPU image processing kernels.
- **Why it matters:** Write image processing kernels once in Rust, run on any GPU.

---

## Tier 2 -- Rising Stars (Strong Momentum, Clear Niche)

### jxl-oxide (JPEG XL decoder)
- **Version:** 0.12.5 (updated 2025-09-30)
- **Downloads:** 616K recent
- **Stars:** 448 (tirr-c/jxl-oxide)
- **Status:** Pure Rust, spec-compliant JPEG XL decoder. Mature and well-maintained.
  Used in production by several image viewers.

### jxl-rs (libjxl official Rust port)
- **Version:** 0.4.0 (updated 2026-03-16) **<-- VERY RECENT**
- **Downloads:** 16.8K recent
- **Stars:** 508 (libjxl/jxl-rs)
- **Status:** **Official** high-performance Rust JXL decoder from the libjxl team.
  Includes SIMD support (`jxl_simd`). Actively developed. This is the "reference
  implementation in Rust" counterpart to libjxl in C++.
- **Competitive edge:** Expected to match/beat jxl-oxide on performance due to
  SIMD-first design and backing by the libjxl team.

### ort (ONNX Runtime)
- **Version:** 2.0.0-rc.12 (updated 2026-03-05)
- **Downloads:** 2.6M recent
- **Stars:** 2,086
- **Status:** Safe Rust wrapper for ONNX Runtime 1.24. The primary way to run
  ONNX models (super-resolution, segmentation, detection) in Rust.
  Supports CPU, CUDA, CoreML, DirectML, TensorRT backends.
- **Relevance:** This + pre-trained ONNX models = AI image processing in Rust today.

### fast_image_resize
- **Version:** 6.0.0 (updated 2026-01-13)
- **Downloads:** 1.67M recent
- **Stars:** 429
- **Status:** SIMD-accelerated image resizing. SSE4.1, AVX2, NEON.
  Supports u8, u16, f32 pixels. Multiple algorithms (Lanczos, Mitchell, etc.).
  6.0.0 is a major release with breaking API changes.
- **Why it matters:** 5-10x faster than image-rs resize for common operations.

### ravif (AVIF encoder)
- **Version:** 0.13.0 (updated 2026-01-19)
- **Downloads:** 5.9M recent
- **Stars:** 663 (kornelski/cavif-rs)
- **Status:** rav1e-based pure Rust AVIF encoder. Powers the `cavif` tool.
  Production quality. Used by image-rs for AVIF encoding.

### Symphonia (audio)
- **Version:** 0.5.5 (updated 2025-10-11)
- **Downloads:** 1.4M recent
- **Stars:** 3,102
- **Status:** Pure Rust audio decoding. MP3, AAC, FLAC, Vorbis, WAV, ALAC, etc.
  The go-to for audio format detection and decoding.

### Photon
- **Version:** 0.3.3 (updated 2025-05-10)
- **Downloads:** 7.9K recent
- **Stars:** 3,389
- **Status:** High-performance image processing, works in browser (WASM) and native.
  Filters, effects, color manipulation. Good API but less active than others.

---

## Tier 3 -- New & Bleeding Edge (2025-2026 Debuts)

### OxiMedia Ecosystem **[NEW -- February 2026]**
- **Version:** 0.1.2 across all crates (updated 2026-03-17)
- **Stars:** 100 (cool-japan/oximedia) -- created 2026-02-12
- **Crate count:** 40+ sub-crates covering the entire media pipeline:
  - `oximedia-core` (types/traits), `oximedia-codec` (video codecs)
  - `oximedia-audio` (audio codecs), `oximedia-io` (I/O layer)
  - `oximedia-cv` (computer vision), `oximedia-neural` (NN inference)
  - `oximedia-scaling` (pro video scaling), `oximedia-graph` (filter pipeline)
  - `oximedia-metering` (ITU-R BS.1770-4, EBU R128), `oximedia-timecode` (LTC/VITC)
  - `oximedia-subtitle`, `oximedia-metadata`, `oximedia-transcode`
  - `oximedia-ndi`, `oximedia-aaf`, `oximedia-edl`, `oximedia-imf`
  - `oximedia-stabilize`, `oximedia-restore`, `oximedia-forensics`
  - `oximedia-colormgmt` (ICC, ACES, HDR), `oximedia-quality` (metrics)
- **Assessment:** Incredibly ambitious scope. Broadcast-grade aspirations (AAF, NDI, IMF,
  SMPTE compliance). Very new (1 month old), low downloads (~50-250 each).
  Worth watching but NOT production-ready. If the team delivers, this could be
  the "FFmpeg of Rust."

### imazen/zen\* Ecosystem **[NEW -- January 2026]**
- **Crates:** zenwebp (0.3.2), zenresize (0.1.0), zensim (0.2.0),
  zenpixels (0.1.0), zenlayout (0.2.0), ultrahdr-core (0.2.0), ultrahdr-rs (0.1.1),
  codec-corpus (1.0.3)
- **Stars:** Low (0-4 per repo) -- created Jan-Mar 2026
- **From:** imazen -- the team behind **Imageflow** (4,376 stars)
- **Key innovations:**
  - `zenwebp` -- Pure Rust WebP encoder/decoder (not libwebp bindings!)
  - `zenresize` -- 31 resampling filters, streaming API, SIMD-accelerated
  - `zensim` -- Psychovisual image similarity metric (like SSIM but faster)
  - `zenpixels` -- Transfer-function-aware pixel conversion and gamut mapping
  - `ultrahdr-rs` -- Pure Rust Ultra HDR (JPEG with gain map) encoder/decoder
  - `codec-corpus` -- Test image dataset management (for benchmarking)
- **Assessment:** From a proven team (Imageflow). Modular, well-architected.
  The `ultrahdr-rs` crate is particularly notable -- Ultra HDR is the new
  standard for HDR photos (adopted by Android/Chrome). Very few implementations exist.

### burn-vision **[NEW -- March 2026]**
- **Version:** 0.21.0-pre.2
- **Downloads:** 2.8K recent
- **Status:** Image transforms and preprocessing directly on Burn tensors.
  Crop, resize, normalize, augment -- all GPU-accelerated via CubeCL/WGPU.
- **Why it matters:** First-class "image in, tensor out" pipeline in pure Rust.

### purecv **[NEW -- March 2026]**
- **Version:** 0.1.3
- **Downloads:** 47
- **Repo:** webarkit/purecv
- **Status:** Pure Rust computer vision library. Safety-focused, portable.
  Very early stage.

### dct-io **[NEW -- March 2026]**
- **Version:** 0.1.1
- **Downloads:** 21
- **Status:** Read and write quantized DCT coefficients in JPEG files directly.
  Niche but useful for lossless JPEG manipulation without full decode/reencode.

### webp-rust **[NEW -- March 2026]**
- **Version:** 0.1.0
- **Downloads:** 12
- **Status:** Another pure Rust WebP implementation (separate from zenwebp).
  Very early.

---

## Tier 4 -- Established Supporting Crates

| Crate | Version | Recent DL | Stars | What |
|-------|---------|-----------|-------|------|
| `turbojpeg` | 1.4.0 | 618K | 23 | TurboJPEG bindings (fastest JPEG via libturbojpeg) |
| `oxipng` | 10.1.0 | 226K | -- | Lossless PNG optimizer |
| `mozjpeg` | 0.10.13 | 578K | -- | Mozilla JPEG encoder (best quality/size) |
| `image-webp` | 0.2.4 | 8.0M | -- | Pure Rust WebP codec (used by image-rs) |
| `avif-serialize` | 0.8.8 | 6.0M | -- | AVIF header writer |
| `avif-decode` | 1.0.2 | 12K | -- | AVIF decoder |
| `imgref` | 1.12.0 | 5.9M | -- | 2D pixel buffer abstraction |
| `rgb` | 0.8.92 | 14.9M | -- | Pixel struct types for interop |
| `lcms2` | 6.1.1 | 464K | -- | ICC color profile handling |
| `rav1e` | 0.8.1 | 5.8M | -- | AV1 encoder (powers ravif) |
| `pic-scale` | 0.6.15 | 3K | 35 | High-perf image scaling (newer alt to fast_image_resize) |
| `pic-scale-safe` | 0.1.10 | 8.5K | -- | Safe wrapper for pic-scale |
| `ffmpeg-next` | 8.1.0 | 1.2M | -- | Safe FFmpeg 4 wrapper |
| `ffmpeg-sidecar` | 2.4.0 | 292K | 519 | FFmpeg CLI wrapper with Iterator API |
| `rodio` | 0.22.2 | 1.2M | -- | Audio playback and recording |
| `rubato` | 1.0.1 | 1.4M | -- | Audio resampling |
| `whisper-rs` | 0.16.0 | 98K | -- | Rust bindings for whisper.cpp |
| `oar-ocr` | 0.6.2 | 248K | 76 | OCR + document layout analysis |
| `vello` | 0.7.0 | 67K | 3,847 | GPU compute-centric 2D renderer |

---

## Format Support Matrix -- What's Possible in Pure Rust Today

| Format | Decode | Encode | Best Crate | Pure Rust? |
|--------|--------|--------|------------|------------|
| JPEG | YES | YES | zune-jpeg / mozjpeg | zune=yes, mozjpeg=no |
| PNG | YES | YES | image-rs / oxipng | yes |
| WebP | YES | YES | image-webp / zenwebp | yes |
| AVIF | YES | YES | ravif + avif-decode | yes (via rav1e) |
| JPEG XL | YES | YES | jxl-oxide / jxl-rs (decode) + zune-jpegxl (encode) | yes |
| Ultra HDR | YES | YES | ultrahdr-rs | yes |
| HEIC | partial | NO | -- | no pure Rust |
| WebP2 | NO | NO | libwebp2 (placeholder) | no |
| AV2 | NO | NO | -- | does not exist yet |
| TIFF | YES | YES | image-rs | yes |
| BMP | YES | YES | zune-bmp / image-rs | yes |
| PSD | YES | NO | zune-psd | yes (decode only) |
| HDR/Radiance | YES | YES | zune-hdr | yes |
| QOI | YES | YES | zune-qoi | yes |
| GIF | YES | YES | image-rs | yes |
| DPX/EXR | YES | YES | oximedia-image (new) | TBD |

---

## GPU Compute for Image Processing

| Approach | Crate | Maturity | Backends |
|----------|-------|----------|----------|
| General GPU compute | `wgpu` 28.0 | Production | Vulkan, Metal, DX12, WebGPU |
| ML-optimized | `cubecl` 0.10 | RC | WGSL, CUDA, ROCm |
| ML inference | `ort` 2.0-rc | RC | CPU, CUDA, CoreML, TensorRT |
| ML training+inference | `burn` 0.21-pre | Pre-release | wgpu, cuda, rocm, ndarray |
| ML inference (light) | `candle` 0.9.2 | Stable | CPU, CUDA, Metal |
| GPU 2D rendering | `vello` 0.7 | Stable | wgpu |
| GPU FFT | `gpu-fft` 1.1.1 | Early | wgpu |

---

## What Does NOT Exist Yet (Gaps / Opportunities)

1. **WebP2** -- Google's successor to WebP. No Rust implementation at all.
   `libwebp2` on crates.io is a placeholder (0 downloads).
2. **AV2** -- The successor to AV1. Not even a C reference encoder yet.
   Years away from Rust.
3. **Pure Rust HEIC/HEIF encoder** -- Only partial decode exists.
4. **GPU-accelerated image codec** -- No Rust crate does GPU JPEG/PNG/WebP
   encode/decode. This is where NVIDIA's nvJPEG lives.
5. **Real-time AI super-resolution in pure Rust** -- `ort` + ONNX models can
   do it, but no turnkey crate exists. `oximedia-neural` is attempting this
   but is very early.
6. **Unified media pipeline** -- OxiMedia is trying but is 1 month old.
   No mature equivalent to GStreamer/FFmpeg in pure Rust.

---

## Recommendations for Nika Media Pipeline

### For CAS binary storage and format detection:
- `infer` (13M dl) -- magic byte detection, zero-dep
- `tree_magic_mini` (2.1M dl) -- MIME type detection

### For image decode/encode:
- **Conservative:** `image` 0.25 (delegates to zune-jpeg internally)
- **Performance:** `zune-image` 0.5 + individual zune-* codecs
- **Next-gen formats:** `jxl-rs` 0.4 (JXL decode) + `ravif` 0.13 (AVIF encode)

### For image transforms:
- `fast_image_resize` 6.0 (SIMD resize)
- `zune-imageprocs` 0.5 (filters, color ops)

### For ML-powered processing:
- `ort` 2.0 + pre-trained ONNX models (pragmatic, production-ready)
- `burn-vision` 0.21 + `cubecl` (ambitious, GPU-native, but pre-release)
- `candle-transformers` 0.9 (Hugging Face models, stable)

### For audio:
- `symphonia` 0.5 (decode) + `rubato` 1.0 (resample) + `rodio` 0.22 (playback)

### For video:
- `ffmpeg-sidecar` 2.4 (safe FFmpeg wrapper, no C linking needed)
- `ffmpeg-next` 8.1 (full FFmpeg bindings, requires libavcodec)

### Watch list (check back in 3-6 months):
- **OxiMedia** -- if they ship quality code, it changes everything
- **imazen/zen\*** -- from a proven team, modular, could replace scattered deps
- **burn-vision** -- GPU image preprocessing for ML inference
- **jxl-rs** 0.4+ -- the official reference Rust JXL decoder

---

## Sources

All data from:
1. crates.io API (`/api/v1/crates?q=...`)
2. GitHub API via `gh` CLI (authenticated)
3. Research date: 2026-03-18

## Confidence Level

**High** for download counts, version numbers, and GitHub stars (live API data).
**Medium** for performance claims (based on crate descriptions and known benchmarks,
not independently verified).
**Low** for OxiMedia and zen* ecosystem assessments (too new to evaluate code quality).
