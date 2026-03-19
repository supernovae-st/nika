# Research Report: Rust Crates for WASM Media Processing

**Date:** 2026-03-18
**Context:** Nika workflow engine -- evaluating WASM plugin feasibility for media pipeline

---

## Summary

The Rust+WASM ecosystem for media processing is mature for **image operations** (resize, filter, format conversion) but remains immature for **audio codecs** and essentially nonexistent for **video encoding/decoding**. For a workflow engine plugin system, **Extism** (built on Wasmtime) is the most practical framework, and image crates like `fast_image_resize`, `pic-scale`, and `photon-rs` are production-ready WASM targets with SIMD support.

---

## Key Findings

### 1. Image Processing Crates -- WASM Readiness

| Crate | WASM Target | SIMD Support | Status | Notes |
|-------|-------------|-------------|--------|-------|
| **photon-rs** | `wasm32-unknown-unknown` | No SIMD (scalar) | Production-ready | 90+ filters, 4-10x faster than JS. Built for WASM from day 1. |
| **fast_image_resize** | `wasm32` | **SIMD128** | Production-ready | Lanczos/convolution resize. All pixel formats supported on WASM. |
| **pic-scale** | `wasm32` | **SIMD128** (via feature flag) | Production-ready | Multi-colorspace resize (sRGB, L\*u\*v\*, XYZ). `wasm-simd128` feature. |
| **image** (image-rs) | `wasm32-unknown-unknown` | No | Partial | Core decode/encode works. No filesystem I/O in WASM. Needs `Cursor<Vec<u8>>` for I/O. `no_std` blockers remain for some paths. |
| **opencv-wasm** | `wasm32-unknown-unknown` | Via OpenCV internals | Experimental | OpenCV bindings for browser. Large binary size. |

**Verdict:** For a Nika WASM plugin doing image transforms, `fast_image_resize` + `image` (for decode/encode) is the strongest combo. `photon-rs` is simpler if you only need filters. `pic-scale` is best for color-space-aware resize with SIMD.

- Source: https://github.com/silvia-odwyer/photon
- Source: https://github.com/Cykooz/fast_image_resize
- Source: https://lib.rs/crates/pic-scale
- Source: https://crates.io/crates/image-base64-wasm

### 2. Audio Processing Crates -- WASM Readiness

| Crate | WASM Target | Status | Blocker |
|-------|-------------|--------|---------|
| **dasp** | `wasm32-unknown-unknown` | Works | Pure Rust, no_std-friendly. DSP primitives (sample conversion, ring buffers, signal). |
| **rubato** | `wasm32-unknown-unknown` | Likely works | Pure Rust resampler. FFT-based. No C deps. |
| **hound** | `wasm32-unknown-unknown` | Likely works | WAV read/write. Pure Rust. Needs `Cursor` instead of `File`. |
| **symphonia** | Unknown | Unconfirmed | Pure Rust codec library (MP3, AAC, FLAC, Vorbis, WAV). Should compile but no official WASM target docs. |
| **cpal** | N/A | Does NOT work | System audio I/O (ALSA, CoreAudio, WASAPI). Inherently non-WASM. |
| **rodio** | N/A | Does NOT work | Built on cpal. Same system dependency issue. |

**Verdict:** For codec work in WASM, `dasp` (DSP primitives) + `rubato` (resampling) + `hound` (WAV I/O) is the safe bet. `symphonia` is the holy grail (multi-codec pure Rust) but needs verification. Audio I/O crates (cpal, rodio) are fundamentally incompatible with WASM.

- Source: https://users.rust-lang.org/t/pure-rust-can-compile-to-wasm-autio-codecs/76322
- Source: https://rustwasm.github.io/book/reference/which-crates-work-with-wasm.html

### 3. Video Processing -- Effectively Nonexistent in WASM

No Rust crate provides video encoding/decoding compiled to WASM:

- **rav1e** (AV1 encoder): No WASM target support
- **ffmpeg bindings**: C FFI, does not compile to WASM from Rust
- **WebCodecs API**: Browser-only, accessed via JS interop, not a Rust crate

For per-frame video processing, the pattern is:
1. Demux/decode in native code (or via browser WebCodecs)
2. Pass raw pixel buffers into WASM plugin
3. Plugin processes frame as image data
4. Return buffer to host for re-encoding

**Verdict:** Video codec work stays native. WASM plugins process raw frames only.

### 4. Performance: WASM vs Native

| Metric | WASM (no SIMD) | WASM + SIMD128 | Native (SSE4.1/AVX2) |
|--------|----------------|----------------|----------------------|
| vs JavaScript | 4-10x faster | 8-10x faster | N/A |
| vs Native Rust | ~50-70% of native | ~80-90% of native | Baseline |
| Image resize (4928x3279 -> 852x567) | ~15-25ms | ~10-15ms | ~7-10ms (AVX2) |
| Array operations | 1.4ms | 0.231ms (6x gain from SIMD) | ~0.15ms |
| JPEG decode | ~36 cycles/pixel | Better with SIMD | Much better with native SIMD |

**Key insight:** WASM+SIMD128 reaches 80-90% of native performance for compute-heavy image work. The 10-20% overhead comes from data marshalling, no threading, and WASM sandbox overhead. This is acceptable for a plugin system.

- Source: https://byteiota.com/rust-webassembly-performance-8-10x-faster-2025-benchmarks/
- Source: https://silvia-odwyer.github.io/photon/
- Source: https://github.com/silvia-odwyer/photon/wiki/Benchmarks

### 5. WASM Runtime Selection for Plugin Host

| Runtime | Type | Speed | Startup | Binary Size | Best For |
|---------|------|-------|---------|-------------|----------|
| **Wasmtime** | JIT/AOT (Cranelift) | Near-native | Slower (JIT compile) | ~5-10MB | Production plugins, Extism default |
| **Wasmer** | JIT/AOT (multiple backends) | Near-native | Moderate | ~5-10MB | Multi-backend flexibility |
| **wasmi** | Interpreter | 10-100x slower | Instant | ~200KB | Tiny footprint, no-JIT policies |
| **Extism** | Framework (uses Wasmtime) | Near-native | Moderate | ~5-10MB | Turnkey plugin system |

**Verdict for Nika:** Extism is the clear winner for a plugin system. It provides:
- Host SDK for Rust (load .wasm, call functions, pass data)
- Guest PDK for Rust (write plugins that compile to .wasm)
- Sandboxing, memory limits, fuel-based execution limits
- Cross-language plugins (users could write Nika plugins in Rust, Go, JS, Python)

If binary size matters (Nika is a CLI tool), `wasmi` is worth considering despite the speed penalty -- it adds ~200KB vs ~5-10MB for Wasmtime.

- Source: https://extism.org
- Source: https://github.com/extism/extism
- Source: https://github.com/typst/typst/issues/7215

### 6. WASM SIMD Status (2025-2026)

WASM SIMD128 is now universally supported:
- Chrome 91+, Firefox 89+, Safari 16.4+, Edge 91+
- Wasmtime and Wasmer both support SIMD
- Enable in Rust: `RUSTFLAGS="-C target-feature=+simd128"`
- Crates opt-in: `pic-scale` uses `wasm-simd128` feature flag; `fast_image_resize` enables automatically

**WasmGC** (garbage collection) is less relevant for media processing but signals WASM's growing capability.

- Source: https://dev.to/dataformathub/rust-webassembly-2025-why-wasmgc-and-simd-change-everything-3ldh

---

## Practical Architecture for Nika WASM Media Plugins

```
Nika Workflow Engine (native Rust)
├── Media Pipeline (PR1: extraction, CAS storage)
├── Artifact Processor (PR2: binary artifacts)
│
├── WASM Plugin Host (Extism / Wasmtime)
│   ├── Load .wasm plugin from CAS or registry
│   ├── Pass raw pixel buffer (bytes) to plugin
│   ├── Plugin processes (resize, filter, watermark)
│   ├── Receive processed buffer back
│   └── Write result to CAS
│
└── Plugin Guest (compiled to .wasm)
    ├── Uses: fast_image_resize, photon-rs, pic-scale
    ├── Uses: dasp, rubato (audio DSP)
    ├── SIMD128 enabled for performance
    └── No filesystem access (sandboxed)
```

### What a Nika plugin YAML might look like:

```yaml
tasks:
  - id: resize_hero
    infer: null
    exec:
      plugin: "registry://image-resize@1.0.0"  # .wasm from package registry
      input: "{{with.hero_image}}"
      params:
        width: 1200
        height: 630
        filter: lanczos3
```

---

## Recommended Crate Stack for Nika

### Image Processing (compile to WASM plugin)
1. **fast_image_resize** -- resize with SIMD128
2. **image** -- decode/encode (PNG, JPEG, WebP, AVIF)
3. **photon-rs** -- filters (blur, sharpen, contrast, etc.)

### Audio Processing (compile to WASM plugin)
1. **dasp** -- sample format conversion, signal processing
2. **rubato** -- resampling
3. **hound** -- WAV I/O

### Plugin Host (embed in Nika binary)
1. **Extism Host SDK** -- if full plugin framework desired (~5MB)
2. **wasmi** -- if minimal footprint needed (~200KB, 10-100x slower)
3. **wasmtime** -- if raw control needed without Extism wrapper

### Stay Native (do NOT put in WASM)
1. Video decode/encode (use ffmpeg or gstreamer natively)
2. Audio device I/O (cpal, rodio)
3. GPU-accelerated processing

---

## Sources

1. [photon-rs GitHub](https://github.com/silvia-odwyer/photon) -- WASM image processing, benchmarks
2. [fast_image_resize GitHub](https://github.com/Cykooz/fast_image_resize) -- SIMD128 WASM resize
3. [pic-scale on lib.rs](https://lib.rs/crates/pic-scale) -- WASM SIMD color-space-aware scaling
4. [Rust WASM 2025 Benchmarks](https://byteiota.com/rust-webassembly-performance-8-10x-faster-2025-benchmarks/) -- 8-10x vs JS
5. [Extism Framework](https://extism.org) -- Turnkey WASM plugin system
6. [Extism GitHub](https://github.com/extism/extism) -- Rust host SDK
7. [Typst wasmi discussion](https://github.com/typst/typst/issues/7215) -- wasmi vs wasmtime tradeoff
8. [Rust WASM Book](https://rustwasm.github.io/book/reference/which-crates-work-with-wasm.html) -- Crate compatibility guide
9. [WASM SIMD + WasmGC 2025](https://dev.to/dataformathub/rust-webassembly-2025-why-wasmgc-and-simd-change-everything-3ldh) -- SIMD maturity
10. [Photon Benchmarks](https://github.com/silvia-odwyer/photon/wiki/Benchmarks) -- vs libvips, ImageMagick
11. [Shuttle WASM tutorial](https://www.shuttle.dev/blog/2024/03/06/writing-wasm-rust) -- image crate in WASM example
12. [WASM runtime benchmarks 2023](https://00f.net/2023/01/04/webassembly-benchmark-2023/) -- Runtime comparison

## Methodology

- **Tools used:** Perplexity Sonar (9 queries)
- **Pages analyzed:** ~70 search results evaluated
- **Time period covered:** 2023-2026 (focus on 2024-2026)

## Confidence Level

**High** for image crates (photon-rs, fast_image_resize, pic-scale) -- well-documented WASM support.
**Medium** for audio crates -- dasp/rubato should work based on pure-Rust architecture but lack explicit WASM documentation.
**Medium** for Extism/Wasmtime -- well-documented framework, but no media-specific production case studies found.
**Low** for video -- confirmed gap; no viable Rust WASM video crate exists.

## Further Research Suggestions

- Test `symphonia` compilation to `wasm32-unknown-unknown` (could unlock MP3/AAC/FLAC decode in WASM)
- Benchmark Extism plugin call overhead for Nika's use case (how many ms per host-guest roundtrip?)
- Evaluate `wasmi` vs `wasmtime` binary size impact on `nika` CLI binary
- Investigate WASI-NN for running ML inference models inside WASM plugins
- Check if `image` crate's AVIF/WebP encoders work in WASM (they depend on C libs in some configurations)
- Explore Component Model (WASI P2/P3) for structured plugin interfaces instead of raw byte passing
