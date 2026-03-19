# Research Report: Rust Media Optimization Crates

**Date:** 2026-03-18
**Scope:** Compression, minification, format conversion, quality optimization
**Crates analyzed:** 40+
**Confidence:** High (crates.io API + GitHub READMEs + dependency analysis)

---

## Summary

The Rust ecosystem has a mature, production-ready toolkit for media optimization. The best crates achieve compression ratios competitive with or superior to their C equivalents. The landscape splits cleanly into **pure-Rust** (safe, easy to build, portable) and **FFI bindings** (faster, battle-tested C/C++ underneath). For Nika's media pipeline, the sweet spot is a layered approach: pure-Rust for common formats, optional FFI for maximum compression.

---

## 1. PNG Optimization

### oxipng (THE pick)

| Field | Value |
|-------|-------|
| Crate | `oxipng` |
| Version | 10.1.0 |
| License | MIT |
| Downloads | 1,125,327 |
| Pure Rust? | **Yes** (pure Rust + optional Zopfli) |
| MSRV | 1.85.1 |
| Repo | https://github.com/oxipng/oxipng |

**What it does:** Multithreaded lossless PNG/APNG compression optimizer. Rewrite of OptiPNG in Rust.

**Optimization levels:**
- `-o 0` to `-o 6` (or `-o max`)
- Default `-o 2`: fast, good compression
- Higher levels: diminishing returns but measurably better

**Key features:**
- Lossless-only (pixel-perfect output)
- Alpha channel optimization (alters transparent pixel colors for better compression)
- Metadata stripping (`--strip safe` or `--strip all`)
- APNG (animated PNG) support (limited but functional)
- Parallel processing via `rayon` (feature-gated)
- Optional Zopfli backend for even better compression (feature `zopfli`)
- Library API: `Options` struct + `optimize()` function
- Used by: ImageOptim, Squoosh, FileOptimizer, Trunk

**Compression ratios (typical):**
- 5-25% reduction on already-optimized PNGs
- 20-50% reduction on unoptimized PNGs (Photoshop exports, screenshots)
- Zopfli mode: additional 3-8% over default, but 10-50x slower

**Library usage:**
```toml
oxipng = { version = "10.0", features = ["parallel", "zopfli"], default-features = false }
```

**Dependencies (slim):** `bitvec`, `indexmap`, `libdeflater`, `rgb`, `rustc-hash` + optional `rayon`, `zopfli`

### zopfli (compression backend)

| Field | Value |
|-------|-------|
| Crate | `zopfli` |
| Version | 0.8.3 |
| License | Apache-2.0 |
| Downloads | 55,192,391 |
| Pure Rust? | **Yes** (port of Google's Zopfli) |

Pure Rust port of Google's Zopfli algorithm. Produces the smallest possible DEFLATE streams. Used as optional oxipng backend. Extremely slow but maximum compression.

### png (decode/encode)

| Field | Value |
|-------|-------|
| Crate | `png` |
| Version | 0.18.1 |
| License | MIT OR Apache-2.0 |
| Downloads | 112,281,032 |
| Pure Rust? | **Yes** |

The standard PNG decode/encode crate. Not an optimizer per se, but required for reading/writing PNGs. Used by the `image` crate internally.

---

## 2. JPEG Optimization

### mozjpeg (THE pick for lossy JPEG)

| Field | Value |
|-------|-------|
| Crate | `mozjpeg` (high-level) + `mozjpeg-sys` (FFI) |
| Version | 0.10.13 / 2.2.3 |
| License | IJG (permissive, JPEG-specific) |
| Downloads | 2,154,884 / 2,439,212 |
| Pure Rust? | **No** (C library via FFI, statically linked) |
| Build deps | NASM + C compiler |
| Repo | https://github.com/ImageOptim/mozjpeg-rust |

**What it does:** High-level Rust wrapper around Mozilla's MozJPEG, which is an optimized fork of libjpeg-turbo focused on compression efficiency.

**Compression ratios:**
- 10-15% smaller than libjpeg-turbo at equivalent quality
- Trellis quantization for optimal Huffman coding
- Progressive JPEG output by default

**Key features:**
- SIMD-accelerated (NASM feature flag)
- Parallel build support
- Encode and decode
- Color space conversion (YCbCr, RGB, CMYK)

**Caveats:**
- Error handling uses `catch_unwind` (libjpeg's setjmp/longjmp design)
- `panic=abort` crates: any JPEG error kills the process
- Requires NASM for SIMD (optional but recommended)

**Library usage:**
```toml
mozjpeg = { version = "0.10", features = ["nasm_simd"] }
```

### turbojpeg (alternative)

| Field | Value |
|-------|-------|
| Crate | `turbojpeg` |
| Version | 1.4.0 |
| License | Unlicense OR MIT |
| Downloads | 1,153,650 |
| Pure Rust? | **No** (wraps libjpeg-turbo) |

Wraps libjpeg-turbo directly (not MozJPEG). Faster encoding/decoding but slightly worse compression than MozJPEG. Good for throughput-critical paths where you need speed over size.

### jpegli (next-gen, experimental)

| Field | Value |
|-------|-------|
| Crate | `jpegli` |
| Version | 0.1.0 |
| License | BSD-3-Clause |
| Downloads | 2,438 |
| Pure Rust? | **No** (wraps Google's jpegli from libjxl) |
| Repo | https://github.com/Szpadel/jpegli-rs |

Google's next-gen JPEG encoder from the JPEG XL project. Claims 35% better compression than MozJPEG at equivalent perceptual quality. Very new, very few downloads -- **not production-ready yet**.

---

## 3. WebP Encoding

### image-webp (pure Rust pick)

| Field | Value |
|-------|-------|
| Crate | `image-webp` |
| Version | 0.2.4 |
| License | MIT OR Apache-2.0 |
| Downloads | 32,922,133 |
| Pure Rust? | **Yes -- zero unsafe code** |

**What it does:** Independent pure-Rust WebP implementation. Backend for the `image` crate.

**Current status:**
- **Decoder:** Full feature support (lossless, lossy, alpha, animation). 70-100% of libwebp speed.
- **Encoder:** Lossless only. Fast but not as good compression as libwebp. Often still smaller than PNG.
- **No lossy encoding** -- this is the major limitation.

**Zero unsafe guarantee:**
```
cargo geiger: 0/0 unsafe in entire dependency tree
```

### webp (FFI binding)

| Field | Value |
|-------|-------|
| Crate | `webp` |
| Version | 0.3.1 |
| License | MIT OR Apache-2.0 |
| Downloads | 2,880,847 |
| Pure Rust? | **No** (binds libwebp via `libwebp-sys`) |

Full-featured WebP encoding including lossy compression with quality control. Wraps Google's reference libwebp implementation.

**Use when:** You need lossy WebP encoding with quality parameter control.

### libwebp-sys (low-level)

| Field | Value |
|-------|-------|
| Crate | `libwebp-sys` |
| Version | 0.14.2 |
| License | (see libwebp) |
| Downloads | 3,148,354 |

Raw FFI bindings. Use `webp` crate instead for ergonomic API.

---

## 4. AVIF Encoding

### ravif (THE pick)

| Field | Value |
|-------|-------|
| Crate | `ravif` |
| Version | 0.13.0 |
| License | BSD-3-Clause |
| Downloads | 21,200,671 |
| Pure Rust? | **Almost** (pure Rust via rav1e, C LCMS2 for color profiles) |
| Repo | https://github.com/kornelski/cavif-rs |

**What it does:** AVIF image encoder built on `rav1e`. Powers the `cavif` command-line tool.

**Compression ratios:**
- AVIF typically achieves 30-50% smaller files than JPEG at equivalent perceptual quality
- Quality scale 1-100 (default 80)
- Speed 1-10 (default 4): speeds 1-2 are extremely slow but 3-5% smaller

**Key features:**
- Multi-threaded encoding (feature `threading`)
- SIMD acceleration (feature `asm`)
- 8-bit and 10-bit depth
- Alpha channel support
- YCbCr and RGB color space options
- `--dirty-alpha` to preserve transparent pixel colors

**Dependencies:** `rav1e`, `avif-serialize`, `imgref`, `rgb`

### rav1e (AV1 encoder underneath)

| Field | Value |
|-------|-------|
| Crate | `rav1e` |
| Version | 0.8.1 |
| License | BSD-2-Clause |
| Downloads | 21,049,163 |
| Pure Rust? | **Mostly** (Rust core + optional NASM assembly for x86_64/aarch64) |
| Build deps | NASM (optional, for SIMD) |
| Repo | https://github.com/xiph/rav1e |

"The fastest and safest AV1 encoder." Xiph.org project. Supports:
- 11 speed settings (0-10)
- 8/10/12-bit depth
- 4:2:0, 4:2:2, 4:4:4 chroma
- Still picture mode (exactly what AVIF needs)

**Performance tip:** Build with `RUSTFLAGS="-C target-cpu=native"` for 11-13% speedup.

### libavif / libavif-sys (alternative)

| Field | Value |
|-------|-------|
| Crate | `libavif` |
| Version | 0.14.0 |
| License | (see libavif) |
| Downloads | 67,741 |
| Pure Rust? | **No** (wraps C libavif which wraps libaom/dav1d) |

Uses libaom (reference AV1 encoder) instead of rav1e. Potentially better compression at slow speeds, but much harder build chain. Low adoption compared to ravif.

### avif-serialize

| Field | Value |
|-------|-------|
| Crate | `avif-serialize` |
| Version | 0.8.8 |
| License | BSD-3-Clause |
| Downloads | 21,493,042 |
| Pure Rust? | **Yes** |

Minimal AVIF/HEIF/MIAF/ISO-BMFF container writer. Used by ravif internally.

---

## 5. JPEG XL

### jpegxl-rs

| Field | Value |
|-------|-------|
| Crate | `jpegxl-rs` |
| Version | 0.13.1+libjxl-0.11.2 |
| License | **GPL-3.0-or-later** |
| Downloads | 123,127 |
| Pure Rust? | **No** (wraps libjxl reference implementation) |
| Repo | https://github.com/inflation/jpegxl-rs |

**WARNING: GPL-3.0 license -- incompatible with Nika's AGPL without careful consideration.**

JPEG XL offers:
- Lossless JPEG recompression (20-30% smaller, bit-exact reconstruction)
- Progressive decoding
- HDR support
- Potentially the best compression ratio of any format

**Status:** Browser support is limited (Chrome removed it, Firefox has it behind a flag). Not recommended for web delivery today. Potentially excellent for archival/storage.

---

## 6. GIF Optimization

### gifski (highest quality GIF encoder)

| Field | Value |
|-------|-------|
| Crate | `gifski` |
| Version | 1.34.0 |
| License | **AGPL-3.0-or-later** |
| Downloads | 526,638 |
| Pure Rust? | **Mostly** (Rust + optional C gifsicle for lossy) |
| Repo | https://github.com/ImageOptim/gifski |

Based on pngquant's color quantization. Creates animated GIFs with thousands of colors per frame via cross-frame palettes and temporal dithering. By far the highest-quality GIF encoder available.

**License note:** AGPL-3.0 is compatible with Nika's AGPL.

### gifsicle (lossy GIF compression)

| Field | Value |
|-------|-------|
| Crate | `gifsicle` |
| Version | 1.95.0 |
| License | **non-standard (GPL-adjacent)** |
| Downloads | 106,942 |
| Pure Rust? | **No** (C FFI bindings) |

Rust bindings to the gifsicle C library. Lossy GIF compression. Used as optional backend in gifski.

### gif (standard encoder/decoder)

| Field | Value |
|-------|-------|
| Crate | `gif` |
| Version | 0.14.1 |
| License | MIT OR Apache-2.0 |
| Downloads | 76,528,692 |
| Pure Rust? | **Yes** |

Standard GIF decode/encode. Not optimized for compression, but necessary for basic read/write.

### gif-dispose

| Field | Value |
|-------|-------|
| Crate | `gif-dispose` |
| Version | 6.0.0 |
| License | (see crate) |
| Downloads | 3,935,029 |

Handles GIF disposal methods (frame compositing). Needed for correct animated GIF rendering.

### color_quant

| Field | Value |
|-------|-------|
| Crate | `color_quant` |
| Version | 1.1.0 |
| License | MIT OR Apache-2.0 |
| Downloads | 75,467,769 |
| Pure Rust? | **Yes** |

Color quantization (reduce to 256 colors). Used internally by the `image` crate for GIF encoding.

---

## 7. Audio Compression

### opus (THE pick for lossy audio)

| Field | Value |
|-------|-------|
| Crate | `opus` |
| Version | 0.3.1 |
| License | MIT/Apache-2.0 |
| Downloads | 901,619 |
| Pure Rust? | **No** (safe bindings to libopus) |

Opus is the gold standard for lossy audio compression. Covers speech and music at all bitrates. IETF standard (RFC 6716).

**Compression ratios:**
- 64 kbps stereo: transparent quality for most content
- 32 kbps mono: excellent for speech
- Dramatically better than MP3 at any bitrate

### symphonia (pure Rust audio decode)

| Field | Value |
|-------|-------|
| Crate | `symphonia` |
| Version | 0.5.5 |
| License | MPL-2.0 |
| Downloads | 4,912,472 |
| Pure Rust? | **Yes** |

Pure Rust media demuxer and audio decoder. Supports:
- Formats: WAV, FLAC, OGG, MP4/M4A, MKV/WebM, AIFF, CAF
- Codecs: MP3, AAC, FLAC, Vorbis, ALAC, PCM, ADPCM
- Feature-gated: enable only what you need

Excellent for reading/transcoding audio. Does **not** encode (it's decode-only).

### ogg (container)

| Field | Value |
|-------|-------|
| Crate | `ogg` |
| Version | 0.9.2 |
| License | (BSD-style) |
| Downloads | 6,427,379 |
| Pure Rust? | **Yes** |

Ogg container encode/decode. Needed to wrap Opus/Vorbis streams.

### hound (WAV I/O)

| Field | Value |
|-------|-------|
| Crate | `hound` |
| Version | 3.5.1 |
| License | Apache-2.0 |
| Downloads | 10,128,838 |
| Pure Rust? | **Yes** |

Simple WAV encode/decode. Very stable, very mature.

### rubato (resampling)

| Field | Value |
|-------|-------|
| Crate | `rubato` |
| Version | 1.0.1 |
| License | MIT |
| Downloads | 4,407,414 |
| Pure Rust? | **Yes** |

Asynchronous audio resampling. Needed when changing sample rates (e.g., 48kHz to 44.1kHz).

---

## 8. Image Format Conversion & Processing

### image (THE Swiss Army knife)

| Field | Value |
|-------|-------|
| Crate | `image` |
| Version | 0.25.10 |
| License | MIT OR Apache-2.0 |
| Downloads | 107,044,824 |
| Pure Rust? | **Yes** (all built-in codecs are pure Rust) |

The standard Rust imaging library. Supports reading/writing: PNG, JPEG, GIF, WebP, BMP, TIFF, ICO, PNM, DDS, QOI, AVIF, and more.

**Not an optimizer**, but the backbone for format detection, decoding, color conversion, and basic transforms (resize, crop, rotate, flip).

### fast_image_resize

| Field | Value |
|-------|-------|
| Crate | `fast_image_resize` |
| Version | 6.0.0 |
| License | MIT OR Apache-2.0 |
| Downloads | 12,969,769 |
| Pure Rust? | **Yes** (with SIMD intrinsics) |
| Repo | https://github.com/cykooz/fast_image_resize |

SIMD-accelerated image resizing. Dramatically faster than `image::imageops::resize`. Supports:
- Nearest, bilinear, bicubic, Lanczos3, Mitchell-Netravali
- AVX2, SSE4.1, NEON acceleration
- Alpha channel handling

**Benchmarks (from repo):** 2-10x faster than `image` crate resize depending on algorithm and image size.

### resize

| Field | Value |
|-------|-------|
| Crate | `resize` |
| Version | 0.8.9 |
| License | (see crate) |
| Downloads | 1,746,677 |
| Pure Rust? | **Yes** |

Simple image resampling library. Less feature-rich than `fast_image_resize` but simpler API.

---

## 9. Quality Metrics (for optimization feedback)

### dssim

| Field | Value |
|-------|-------|
| Crate | `dssim` |
| Version | 3.4.0 |
| Downloads | 208,528 |

Multi-scale structural dissimilarity metric. Measures perceptual quality loss. Used by ImageOptim to find optimal compression levels.

### ssimulacra2

| Field | Value |
|-------|-------|
| Crate | `ssimulacra2` |
| Version | 0.5.1 |
| Downloads | 19,721 |

Rust implementation of SSIMULACRA2, the perceptual quality metric from the JPEG XL project. More accurate than SSIM for modern codecs.

### butteraugli

| Field | Value |
|-------|-------|
| Crate | `butteraugli` |
| Version | 0.9.0 |
| Downloads | 3,717 |
| Pure Rust? | **Yes** |

Pure Rust implementation of Google's butteraugli perceptual quality metric.

---

## 10. SVG Optimization

### usvg + resvg

| Field | Value |
|-------|-------|
| Crate | `usvg` / `resvg` |
| Version | 0.47.0 / 0.47.0 |
| License | MPL-2.0 |
| Downloads | 12,075,956 / 10,912,853 |
| Pure Rust? | **Yes** |

- `usvg`: SVG simplification (resolves `use`, styles, transforms). Reduces SVG complexity.
- `resvg`: SVG rendering to raster. Can be used for SVG-to-PNG conversion.

### svg-hush

| Field | Value |
|-------|-------|
| Crate | `svg-hush` |
| Version | 0.9.6 |
| Downloads | 126,321 |

Strips scripting and abusable features from SVG files. Security-focused SVG sanitization.

### svgcleaner

| Field | Value |
|-------|-------|
| Crate | `svgcleaner` |
| Version | 0.9.5 |
| Downloads | 39,435 |

General SVG cleanup/minification. Removes unnecessary data.

---

## 11. General Compression (for binary/data artifacts)

| Crate | Version | Downloads | Type | Notes |
|-------|---------|-----------|------|-------|
| `brotli` | 8.0.2 | 158M | Pure Rust | Brotli compression. Best for web (HTTP). |
| `zstd` | 0.13.3 | 246M | FFI (libzstd) | Zstandard. Best general-purpose ratio/speed. |
| `lz4_flex` | 0.13.0 | 80M | Pure Rust | LZ4. Fastest decompression. No unsafe by default. |
| `miniz_oxide` | 0.9.1 | 576M | Pure Rust | DEFLATE. Used by flate2. |

---

## 12. Video (for completeness)

| Crate | Version | Downloads | Notes |
|-------|---------|-----------|-------|
| `rav1e` | 0.8.1 | 21M | AV1 encoder (also used for AVIF stills) |
| `ffmpeg-next` | 8.1.0 | 2.1M | Safe FFmpeg bindings (all formats) |
| `matroska` | 0.30.0 | 144K | MKV metadata parsing |

---

## Recommended Nika Optimization Toolkit

### Tier 1: Core (pure Rust, permissive license, essential)

| Format | Crate | License | Pure Rust | Priority |
|--------|-------|---------|-----------|----------|
| PNG optimize | `oxipng` | MIT | Yes | P0 |
| PNG read/write | `png` | MIT/Apache | Yes | P0 |
| Image Swiss Army | `image` | MIT/Apache | Yes | P0 |
| WebP (lossless) | `image-webp` | MIT/Apache | Yes | P0 |
| Image resize | `fast_image_resize` | MIT/Apache | Yes | P1 |
| GIF read/write | `gif` | MIT/Apache | Yes | P1 |
| Audio decode | `symphonia` | MPL-2.0 | Yes | P1 |
| WAV I/O | `hound` | Apache-2.0 | Yes | P2 |
| SVG simplify | `usvg` | MPL-2.0 | Yes | P2 |

### Tier 2: Enhanced (FFI or copyleft, behind feature flags)

| Format | Crate | License | Pure Rust | Feature Flag |
|--------|-------|---------|-----------|-------------|
| JPEG optimize | `mozjpeg` | IJG | No (C FFI) | `optimize-jpeg` |
| AVIF encode | `ravif` | BSD-3 | Almost | `avif` |
| WebP lossy | `webp` | MIT/Apache | No (libwebp FFI) | `webp-lossy` |
| Audio encode | `opus` | MIT/Apache | No (libopus FFI) | `audio-encode` |
| GIF high-quality | `gifski` | **AGPL** | Mostly | `gif-hq` |
| General compress | `zstd` | BSD | No (libzstd FFI) | `zstd` |

### Tier 3: Experimental / Future

| Format | Crate | License | Notes |
|--------|-------|---------|-------|
| JPEG XL | `jpegxl-rs` | **GPL-3.0** | License issue + limited browser support |
| Jpegli | `jpegli` | BSD-3 | Too new (2,438 downloads), v0.1.0 |
| Quality metric | `ssimulacra2` | (check) | For adaptive quality optimization |

---

## License Compatibility Matrix (with Nika AGPL-3.0)

| License | Compatible? | Notes |
|---------|-------------|-------|
| MIT | Yes | Permissive |
| Apache-2.0 | Yes | Permissive |
| BSD-2-Clause | Yes | Permissive |
| BSD-3-Clause | Yes | Permissive |
| IJG | Yes | JPEG-specific permissive |
| MPL-2.0 | Yes | File-level copyleft, compatible |
| AGPL-3.0 | Yes | Same license |
| GPL-3.0 | **Caution** | Compatible with AGPL but viral |
| Unlicense | Yes | Public domain equivalent |

---

## Build Complexity Matrix

| Crate | Build Deps | Cross-compile | WASM | Docker |
|-------|-----------|---------------|------|--------|
| `oxipng` | None | Easy | Yes | Easy |
| `image` | None | Easy | Yes | Easy |
| `image-webp` | None | Easy | Yes | Easy |
| `png` | None | Easy | Yes | Easy |
| `zopfli` | None | Easy | Yes | Easy |
| `fast_image_resize` | None | Easy | Partial | Easy |
| `symphonia` | None | Easy | Partial | Easy |
| `mozjpeg` | NASM + CC | Medium | No | Medium |
| `ravif`/`rav1e` | NASM (opt) | Medium | No | Medium |
| `webp` (FFI) | CMake | Hard | No | Hard |
| `opus` | CC | Medium | No | Medium |
| `jpegxl-rs` | CMake + CC | Hard | No | Hard |
| `ffmpeg-next` | FFmpeg dev | Very Hard | No | Very Hard |

---

## Compression Ratio Summary (approximate, typical web images)

| Technique | Typical Savings | Speed | Lossless? |
|-----------|----------------|-------|-----------|
| oxipng -o2 | 15-30% | Fast | Yes |
| oxipng -o6 + zopfli | 25-40% | Slow | Yes |
| mozjpeg (re-encode q85) | 20-40% vs naive JPEG | Fast | No |
| PNG -> WebP lossless | 25-35% | Fast | Yes |
| JPEG -> AVIF (q80) | 30-50% vs JPEG | Medium | No |
| PNG -> AVIF (q80) | 40-60% vs PNG | Medium | No |
| GIF -> gifski | Better quality, similar size | Slow | N/A |
| SVG -> usvg | 10-30% | Fast | Yes |
| WAV -> Opus 64kbps | 90%+ | Fast | No |

---

## Methodology

- **Data source:** crates.io REST API (versions, downloads, dependencies, licenses)
- **READMEs:** GitHub API / raw.githubusercontent.com
- **Dependency analysis:** crates.io dependency endpoints for latest versions
- **Pure Rust determination:** Dependency tree analysis (presence of `-sys` crates, build deps)
- **Compression ratios:** From project documentation, READMEs, and established benchmarks
- **All data retrieved:** 2026-03-18

---

## Sources

1. [oxipng](https://github.com/oxipng/oxipng) -- Lossless PNG optimizer (1.1M downloads)
2. [mozjpeg-rust](https://github.com/ImageOptim/mozjpeg-rust) -- MozJPEG wrapper (2.1M downloads)
3. [mozjpeg-sys](https://github.com/kornelski/mozjpeg-sys.git) -- MozJPEG FFI (2.4M downloads)
4. [image-webp](https://github.com/image-rs/image-webp) -- Pure Rust WebP (32.9M downloads)
5. [webp](https://github.com/jaredforth/webp.git) -- libwebp bindings (2.8M downloads)
6. [ravif / cavif](https://github.com/kornelski/cavif-rs) -- AVIF encoder (21.2M downloads)
7. [rav1e](https://github.com/xiph/rav1e/) -- AV1 encoder (21M downloads)
8. [gifski](https://github.com/ImageOptim/gifski) -- High-quality GIF encoder (526K downloads)
9. [image](https://crates.io/crates/image) -- Core imaging library (107M downloads)
10. [symphonia](https://crates.io/crates/symphonia) -- Pure Rust audio decoder (4.9M downloads)
11. [fast_image_resize](https://github.com/cykooz/fast_image_resize) -- SIMD resize (12.9M downloads)
12. [zopfli](https://crates.io/crates/zopfli) -- Zopfli compression (55.1M downloads)
13. [jpegxl-rs](https://github.com/inflation/jpegxl-rs) -- JPEG XL bindings (123K downloads)
14. [opus](https://crates.io/crates/opus) -- Opus audio bindings (901K downloads)
15. [usvg](https://crates.io/crates/usvg) -- SVG simplification (12M downloads)
16. [resvg](https://crates.io/crates/resvg) -- SVG rendering (10.9M downloads)
