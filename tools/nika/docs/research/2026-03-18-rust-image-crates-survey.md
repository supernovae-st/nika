# Rust Image Processing Crates Survey (March 2026)

> Comprehensive survey of the Rust image processing ecosystem.
> Data sourced from crates.io on 2026-03-18.

---

## Executive Summary

The Rust image ecosystem has matured dramatically. Key developments in 2025-2026:

1. **`image` crate hit v0.25.10** -- now uses `moxcms` (pure Rust ICC) instead of lcms2, `pic-scale-safe` for resizing, `zune-jpeg` as default JPEG decoder, and `dav1d` for native AVIF decoding.
2. **`moxcms`** (10M recent downloads!) replaced `lcms2` for ICC color management -- fully pure Rust.
3. **`jxl`** (the official libjxl team's Rust decoder) appeared on crates.io -- high-perf SIMD JXL decoding.
4. **`zune-jpeg`** dominates JPEG decoding (15.7M recent downloads) with SIMD acceleration.
5. **`quantette`** is the new king of color quantization (faster than `imagequant`).

---

## Tier 1: Core Image Libraries (The Foundation)

### `image` -- The Standard Library
| Field | Value |
|-------|-------|
| Version | 0.25.10 |
| Total Downloads | 107,044,824 |
| Recent Downloads | 18,768,156 |
| Pure Rust | Yes (with optional C deps for AVIF native) |
| Repository | https://github.com/image-rs/image |
| Updated | 2026-03-10 |

**What it does**: The de facto standard. Decodes/encodes PNG, JPEG, GIF, WebP, BMP, TIFF, TGA, QOI, EXR, Farbfeld, HDR, ICO, PNM, AVIF. Basic image processing (resize, crop, rotate, color convert). **Now uses `moxcms` for ICC profiles and `pic-scale-safe` for resizing.**

**Format support via features**:
- `avif` -- rav1e encoder (pure Rust)
- `avif-native` -- dav1d decoder + mp4parse (C dep)
- `jpeg` -- zune-jpeg decoder + jpeg-encoder (SIMD)
- `png` -- png crate (pure Rust)
- `webp` -- image-webp (pure Rust)
- `nasm` -- enables NASM assembly in rav1e
- `rayon` -- parallel processing

**Performance**: Good for general use. Not the fastest for any single format, but excellent breadth. Resize performance improved with `pic-scale-safe`.

**Verdict**: Start here. Add specialized crates when you need peak performance for a specific format.

---

### `zune-image` -- The Challenger
| Field | Value |
|-------|-------|
| Version | 0.5.0 |
| Total Downloads | 46,613 |
| Recent Downloads | 10,479 |
| Pure Rust | Yes |
| Repository | https://github.com/etemesi254/zune-image |
| Updated | 2026-01-24 |

**What it does**: Alternative to `image` with focus on raw speed. Modular design: each format is a separate crate. Supports JPEG, PNG, BMP, HDR, PSD, PPM, Farbfeld, QOI, JXL (encode). Image operations: blur, color conversion, exposure, resize.

**Performance**: JPEG decoding is its crown jewel (zune-jpeg is 2-5x faster than jpeg-decoder). SIMD-accelerated throughout.

**Sub-crates**:
- `zune-core` (0.5.1) -- 42.7M downloads, 15.3M recent -- core types
- `zune-jpeg` (0.5.13) -- 44.5M downloads, 15.7M recent -- fastest pure Rust JPEG decoder
- `zune-png` (0.5.2) -- 77K downloads -- fast PNG decoder
- `zune-bmp` (0.5.2) -- 39K downloads -- BMP decoder
- `zune-hdr` (0.5.2) -- 35K downloads -- Radiance HDR
- `zune-psd` (0.5.1) -- 36K downloads -- PSD decoder
- `zune-qoi` (0.5.2) -- 32K downloads -- QOI codec
- `zune-jpegxl` (0.5.2) -- 44K downloads -- JXL encoder
- `zune-inflate` (0.2.54) -- 37.5M downloads -- fast deflate decompressor

**Verdict**: The individual sub-crates (especially `zune-jpeg` and `zune-core`) are battle-tested and widely used. The umbrella `zune-image` is less adopted. `zune-jpeg` is already the default in `image 0.25`.

---

### `imageproc` -- Image Processing Algorithms
| Field | Value |
|-------|-------|
| Version | 0.26.1 |
| Total Downloads | 7,572,535 |
| Recent Downloads | 1,756,680 |
| Pure Rust | Yes |
| Repository | https://github.com/image-rs/imageproc |
| Updated | 2026-02-28 |

**What it does**: Computer vision and image processing operations built on top of `image`. Includes: edge detection (Canny, Sobel), morphological operations, contour detection, Hough transforms, template matching, drawing primitives, connected components, seam carving, geometric transforms.

**Verdict**: The go-to for algorithmic image processing in Rust. Pairs with `image` crate.

---

## Tier 2: Format-Specific Codecs (The Specialists)

### JPEG

#### `zune-jpeg` -- Fastest Pure Rust JPEG Decoder
| Field | Value |
|-------|-------|
| Version | 0.5.13 |
| Downloads | 44,570,070 |
| Recent | 15,777,062 |
| Pure Rust | Yes (SIMD intrinsics, no C) |

**Performance**: 2-5x faster than `jpeg-decoder`. Uses SIMD (SSE2/AVX2/NEON). Now the default decoder in `image 0.25`.

#### `jpeg-decoder` -- Legacy Pure Rust JPEG Decoder
| Field | Value |
|-------|-------|
| Version | 0.3.2 |
| Downloads | 67,581,985 |
| Recent | 7,395,913 |
| Pure Rust | Yes |

**Note**: Still widely used but being superseded by `zune-jpeg` in new projects.

#### `jpeg-encoder` -- Pure Rust JPEG Encoder
| Field | Value |
|-------|-------|
| Version | 0.7.0 |
| Downloads | 5,055,677 |
| Recent | 1,409,849 |
| Pure Rust | Yes (with optional SIMD feature) |

**Performance**: Good with `simd` feature enabled. Used by `image` crate.

#### `mozjpeg` -- Best Compression Quality JPEG Encoder
| Field | Value |
|-------|-------|
| Version | 0.10.13 |
| Downloads | 2,154,884 |
| Recent | 578,374 |
| Pure Rust | NO -- C library (requires nasm + C compiler) |
| Repository | https://github.com/ImageOptim/mozjpeg-rust |

**What it does**: Wraps Mozilla's MozJPEG. Produces 10-30% smaller JPEG files than standard encoders at equivalent quality. Supports progressive JPEG, trellis quantization, scan optimization.

**Performance**: Encoding is slower than `jpeg-encoder` but file sizes are significantly smaller. The gold standard for JPEG compression quality.

#### `turbojpeg` -- Fastest JPEG Codec (System Dep)
| Field | Value |
|-------|-------|
| Version | 1.4.0 |
| Downloads | 1,153,650 |
| Recent | 618,164 |
| Pure Rust | NO -- Wraps libturbojpeg (C library) |

**What it does**: Bindings for libjpeg-turbo. Fastest JPEG encoding and decoding available. Supports lossless JPEG transforms (rotate, crop without re-encoding).

**Performance**: Fastest option. 2-6x faster than IJG libjpeg. Lossless transforms are unique to this crate.

---

### PNG

#### `png` -- Standard PNG Codec
| Field | Value |
|-------|-------|
| Version | 0.18.1 |
| Downloads | 112,281,032 |
| Recent | 22,321,677 |
| Pure Rust | Yes |
| Repository | https://github.com/image-rs/image-png |

**What it does**: Full PNG decoder/encoder. Supports all color types, interlacing, animated PNG (APNG). Uses `fdeflate` for fast specialized deflate and `simd-adler32`.

**Performance**: Good. `fdeflate` gives it a significant speed boost for common cases.

#### `oxipng` -- PNG Optimizer
| Field | Value |
|-------|-------|
| Version | 10.1.0 |
| Downloads | 1,125,327 |
| Recent | 226,333 |
| Pure Rust | Yes |
| Repository | https://github.com/oxipng/oxipng |

**What it does**: Lossless PNG compression optimizer. Rewrite of OptiPNG in Rust. Tries multiple filter/compression strategies. Multi-threaded. Can strip metadata.

**Performance**: Reduces PNG file size by 5-40% losslessly. Parallelized.

#### `lodepng` -- Alternative PNG Codec
| Field | Value |
|-------|-------|
| Version | 3.12.2 |
| Downloads | 2,319,146 |
| Recent | 274,293 |
| Pure Rust | Yes (port of C LodePNG) |

**What it does**: Pure Rust port of LodePNG. Simpler API than `png`. Good for embedding.

---

### WebP

#### `image-webp` -- Pure Rust WebP
| Field | Value |
|-------|-------|
| Version | 0.2.4 |
| Downloads | 32,922,133 |
| Recent | 8,039,962 |
| Pure Rust | Yes |
| Repository | https://github.com/image-rs/image-webp |

**What it does**: WebP encoding and decoding in pure Rust. Part of the image-rs ecosystem. Default WebP backend for `image` crate.

**Performance**: Good for decoding. Encoding quality/speed may not match libwebp.

#### `webp` -- libwebp Bindings
| Field | Value |
|-------|-------|
| Version | 0.3.1 |
| Downloads | 2,880,847 |
| Recent | 704,103 |
| Pure Rust | NO -- Wraps Google's libwebp (C) |

**What it does**: Full libwebp bindings. Best compression ratio and quality for WebP format.

#### `webp-animation` -- WebP Animation
| Field | Value |
|-------|-------|
| Version | 0.9.0 |
| Downloads | 88,669 |
| Recent | 8,201 |
| Pure Rust | NO -- Wraps libwebp |

**What it does**: Encode/decode animated WebP files.

---

### AVIF (AV1 Image Format)

#### `ravif` -- Pure Rust AVIF Encoder
| Field | Value |
|-------|-------|
| Version | 0.13.0 |
| Downloads | 21,200,671 |
| Recent | 5,950,411 |
| Pure Rust | Yes (uses rav1e) |
| Repository | https://github.com/kornelski/cavif-rs |

**What it does**: Encodes images to AVIF using rav1e (pure Rust AV1 encoder). Powers the `cavif` CLI tool. Used by `image` crate's `avif` feature.

**Performance**: Good quality, slower than native dav1d/aom. Threading via rayon. NASM assembly optional for speed boost.

#### `rav1e` -- AV1 Encoder (Underpinning)
| Field | Value |
|-------|-------|
| Version | 0.8.1 |
| Downloads | 21,049,163 |
| Recent | 5,839,451 |
| Pure Rust | Yes (optional NASM for assembly) |

**What it does**: The fastest and safest AV1 encoder. Written by Xiph.org. Pure Rust with optional assembly acceleration.

#### `dav1d` -- AV1 Decoder Bindings
| Field | Value |
|-------|-------|
| Version | 0.11.1 |
| Downloads | 748,643 |
| Recent | 118,978 |
| Pure Rust | NO -- Wraps dav1d C library |

**What it does**: Bindings for VideoLAN's dav1d, the fastest AV1 decoder. Used by `image` crate's `avif-native` feature for AVIF decoding.

#### `avif-serialize` -- AVIF Container Writer
| Field | Value |
|-------|-------|
| Version | 0.8.8 |
| Downloads | 21,493,042 |
| Recent | 6,015,289 |
| Pure Rust | Yes |

**What it does**: Minimal writer for AVIF header structure (MPEG/HEIF/MIAF/ISO-BMFF). Used by `ravif`.

#### `avif-parse` -- AVIF Container Reader
| Field | Value |
|-------|-------|
| Version | 2.0.0 |
| Downloads | 108,917 |
| Recent | 20,948 |
| Pure Rust | Yes |

#### `avif-decode` -- AVIF to Pixels
| Field | Value |
|-------|-------|
| Version | 1.0.2 |
| Downloads | 33,763 |
| Recent | 12,413 |
| Pure Rust | NO (uses dav1d) |

---

### JPEG XL (JXL)

#### `jxl` -- Official libjxl Team's Rust Decoder (NEW!)
| Field | Value |
|-------|-------|
| Version | 0.4.0 |
| Downloads | 17,528 |
| Recent | 16,804 |
| Pure Rust | Yes (SIMD intrinsics) |
| Repository | https://github.com/libjxl/jxl-rs |

**What it does**: High-performance JPEG XL decoder written by the libjxl team themselves. Uses SIMD (jxl_simd sub-crate). This is the cutting edge -- the official team building a Rust-native decoder.

**Performance**: Designed to match or approach libjxl C performance. Very new (almost all downloads are recent).

**Verdict**: Watch this closely. This is the future of JXL in Rust.

#### `jxl-oxide` -- Established Pure Rust JXL Decoder
| Field | Value |
|-------|-------|
| Version | 0.12.5 |
| Downloads | 1,123,853 |
| Recent | 616,747 |
| Pure Rust | Yes |
| Repository | https://github.com/tirr-c/jxl-oxide |

**What it does**: The most mature pure Rust JXL decoder. Supports progressive decoding, all JXL features. Well-tested.

**Performance**: Good but not as fast as the C libjxl reference implementation.

**Verdict**: Production-ready today. Will likely be superseded by `jxl` crate long-term.

#### `jpegxl-rs` -- libjxl Bindings (Encode + Decode)
| Field | Value |
|-------|-------|
| Version | 0.13.1+libjxl-0.11.2 |
| Downloads | 123,127 |
| Recent | 9,232 |
| Pure Rust | NO -- Wraps libjxl C++ library |

**What it does**: Safe bindings for the official JPEG XL reference implementation. Both encoding AND decoding (only option for JXL encoding with full feature parity).

**Verdict**: Only option if you need JXL encoding with full spec compliance.

#### `zune-jpegxl` -- Zune JXL Encoder
| Field | Value |
|-------|-------|
| Version | 0.5.2 |
| Downloads | 44,918 |
| Recent | 11,537 |
| Pure Rust | Yes |

**What it does**: JXL encoder from the zune-image family.

---

### HEIF/HEIC

#### `libheif-rs` -- HEIF/HEIC via libheif
| Field | Value |
|-------|-------|
| Version | 2.7.0 |
| Downloads | 305,682 |
| Recent | 73,523 |
| Pure Rust | NO -- Wraps libheif (C++) |
| Repository | https://github.com/Cykooz/libheif-rs |

**What it does**: Safe wrapper around libheif for reading/writing HEIF/HEIC files. This is effectively the ONLY production option for HEIF in Rust.

**Note**: There is a `heif` crate (0.1.0, 434 downloads) attempting a pure Rust implementation, but it is not production-ready.

**Verdict**: System dep required. No viable pure Rust alternative exists.

---

### Other Formats

#### `gif` -- GIF Codec
| Version | 0.14.1 | Downloads | 76,528,692 | Recent | 13,249,370 | Pure Rust | Yes |

#### `tiff` -- TIFF Codec
| Version | 0.11.3 | Downloads | 65,079,068 | Recent | 12,843,541 | Pure Rust | Yes |

#### `exr` -- OpenEXR
| Version | 1.74.0 | Downloads | 41,061,970 | Recent | 8,431,405 | Pure Rust | Yes (zero unsafe) |

#### `qoi` -- QOI Format
| Version | 0.4.1 | Downloads | 36,142,152 | Recent | 8,008,682 | Pure Rust | Yes |

Ultra-fast encode/decode. Lossless. Files are ~10-30% larger than PNG but encode 20-50x faster.

#### `psd` -- Photoshop PSD
| Version | 0.3.5 | Downloads | 99,613 | Recent | 8,609 | Pure Rust | Yes |

#### `ico` -- ICO Format
| Version | 0.5.0 | Downloads | 11,719,950 | Recent | 3,378,122 | Pure Rust | Yes |

---

## Tier 3: Image Optimization & Compression

### `imagequant` -- PNG Color Quantization (pngquant)
| Field | Value |
|-------|-------|
| Version | 4.4.1 |
| Downloads | 943,228 |
| Recent | 148,407 |
| Pure Rust | Yes |
| Repository | https://github.com/ImageOptim/libimagequant |

**What it does**: Converts 24/32-bit images to 8-bit palette with alpha. Powers pngquant. Best lossy PNG compression available.

### `quantette` -- Next-Gen Color Quantization (NEW!)
| Field | Value |
|-------|-------|
| Version | 0.5.1 |
| Downloads | 124,458 |
| Recent | 63,695 |
| Pure Rust | Yes |
| Repository | https://github.com/IanManske/quantette |

**What it does**: Fast, high-quality image quantization and palette generation. Designed as a modern, faster alternative to imagequant.

**Verdict**: Rising fast. Watch this as a potential imagequant replacement.

### `rimage` -- Multi-Format Optimizer
| Field | Value |
|-------|-------|
| Version | 0.12.3 |
| Downloads | 32,000 |
| Recent | 688 |
| Pure Rust | Mixed (wraps mozjpeg, etc.) |

**What it does**: Unified image optimization: MozJPEG, OxiPNG, WebP, AVIF encoders. CLI and library.

---

## Tier 4: Image Resizing

### `fast_image_resize` -- SIMD Resizing
| Field | Value |
|-------|-------|
| Version | 6.0.0 |
| Downloads | 12,969,769 |
| Recent | 1,673,762 |
| Pure Rust | Yes (SIMD intrinsics) |
| Repository | https://github.com/cykooz/fast_image_resize |

**What it does**: High-performance image resizing using SIMD (SSE4.1, AVX2, NEON). Supports multiple algorithms: Nearest, Box, Bilinear, Catmull-Rom, Mitchell, Gaussian, Lanczos3. Handles alpha premultiplication correctly.

**Performance**: 2-10x faster than `image::resize`. The best pure Rust option for resizing.

### `pic-scale` / `pic-scale-safe` -- New Contender
| Field | Value |
|-------|-------|
| Version | 0.6.15 / 0.1.10 |
| Downloads | 72,181 / 15,155 |
| Recent | 3,036 / 8,583 |
| Pure Rust | Yes |
| Repository | https://github.com/awxkee/pic-scale-safe |

**What it does**: High-performance image scaling. `pic-scale-safe` is the safe API version, now used by `image 0.25` as its resize backend. By the same author as `moxcms` and `yuv`.

**Verdict**: `pic-scale-safe` is now the default resizer in `image` crate. Important new player.

### `resize` -- Simple Pure Rust Resizing
| Field | Value |
|-------|-------|
| Version | 0.8.9 |
| Downloads | 1,746,677 |
| Recent | 158,242 |
| Pure Rust | Yes |

**What it does**: Simple image resampling. Lanczos3, Mitchell, etc. From the Piston ecosystem.

---

## Tier 5: Color Science & ICC Profiles

### `moxcms` -- Pure Rust ICC Color Management (NEW! IMPORTANT!)
| Field | Value |
|-------|-------|
| Version | 0.8.1 |
| Downloads | 17,847,720 |
| Recent | 10,036,520 |
| Pure Rust | Yes |
| Repository | https://github.com/awxkee/moxcms |

**What it does**: Simple Color Management in Rust. ICC profile parsing and color space transforms. Replaced `lcms2` bindings in the `image` crate. Exploding in adoption (10M recent downloads!).

**Verdict**: THE choice for ICC color management in pure Rust. Already the default in `image 0.25`.

### `lcms2` -- Little CMS Bindings (Legacy)
| Field | Value |
|-------|-------|
| Version | 6.1.1 |
| Downloads | 4,511,990 |
| Recent | 464,258 |
| Pure Rust | NO -- Wraps Little CMS 2 (C library) |

**What it does**: Full ICC color profile handling. Industry-standard color management. Still the most feature-complete option.

**Verdict**: Use if you need advanced ICC features that `moxcms` doesn't cover yet. Being superseded.

### `qcms` -- Firefox's Color Management
| Field | Value |
|-------|-------|
| Version | 0.3.0 |
| Downloads | 858,662 |
| Recent | 396,892 |
| Pure Rust | Yes |
| Repository | https://github.com/FirefoxGraphics/qcms |

**What it does**: Lightweight color management from Firefox/Mozilla. Fast sRGB path.

### `palette` -- Color Science Library
| Field | Value |
|-------|-------|
| Version | 0.7.6 |
| Downloads | 6,748,704 |
| Recent | 1,257,684 |
| Pure Rust | Yes |
| Repository | https://github.com/Ogeon/palette |

**What it does**: Comprehensive color space library. Convert between sRGB, linear RGB, HSL, HSV, HWB, Lab, Lch, Oklab, Oklch, XYZ, and more. Color blending, mixing, chromatic adaptation. Type-safe color space conversions.

**Verdict**: Best for programmatic color manipulation, blending, perceptual uniformity.

### `yuv` -- YUV Color Space Conversion
| Field | Value |
|-------|-------|
| Version | 0.8.11 |
| Downloads | 1,581,244 |
| Recent | 381,729 |
| Pure Rust | Yes (SIMD) |
| Repository | https://github.com/awxkee/yuvutils-rs |

**What it does**: High-performance YUV format handling and conversion. SIMD-accelerated. By the same author as `moxcms` and `pic-scale`.

### `color` -- Linebender Color Library
| Field | Value |
|-------|-------|
| Version | 0.3.2 |
| Downloads | 1,081,270 |
| Recent | 214,689 |
| Pure Rust | Yes |
| Repository | https://github.com/linebender/color |

**What it does**: Color representation and manipulation from the Linebender project (tiny-skia, resvg).

---

## Tier 6: Pixel/Buffer Primitives

### `rgb` -- Pixel Type Interop
| Field | Value |
|-------|-------|
| Version | 0.8.53 |
| Downloads | 67,558,125 |
| Recent | 14,949,477 |
| Pure Rust | Yes |

**What it does**: `RGB`, `RGBA`, `Gray` pixel structs with zero-copy interop between crates. De facto standard pixel type.

### `imgref` -- 2D Image Buffer
| Field | Value |
|-------|-------|
| Version | 1.12.0 |
| Downloads | 25,725,065 |
| Recent | 5,988,689 |
| Pure Rust | Yes |

**What it does**: Safe 2D image buffer with width, height, stride. Used by ravif, mozjpeg, imagequant, dssim.

### `color_quant` -- Basic Color Quantization
| Field | Value |
|-------|-------|
| Version | 1.1.0 |
| Downloads | 75,467,769 |
| Recent | 11,189,215 |
| Pure Rust | Yes |

**What it does**: Reduce n colors to 256. Simple NeuQuant algorithm. Used by `gif` crate.

---

## Tier 7: 2D Rendering & SVG

### `tiny-skia` -- Software 2D Renderer
| Version | 0.12.0 | Downloads | 22,722,017 | Recent | 5,718,741 | Pure Rust | Yes |

Skia-compatible 2D rendering. Path filling/stroking, anti-aliasing, clipping, transforms.

### `resvg` -- SVG Renderer
| Version | 0.47.0 | Downloads | 10,912,853 | Recent | 3,136,381 | Pure Rust | Yes |

High-quality SVG rendering. Uses `usvg` for parsing, `tiny-skia` for rendering.

### `usvg` -- SVG Parser/Simplifier
| Version | 0.47.0 | Downloads | 12,075,956 | Recent | 3,754,409 | Pure Rust | Yes |

Simplifies SVG DOM into a minimal, easy-to-render tree.

---

## Tier 8: Metadata

### `kamadak-exif` -- EXIF Reader
| Version | 0.6.1 | Downloads | 8,406,730 | Recent | 1,534,789 | Pure Rust | Yes |

The standard EXIF parser. Read-only.

### `little_exif` -- EXIF Read/Write
| Version | 0.6.23 | Downloads | 107,383 | Recent | 40,421 | Pure Rust | Yes |

The only pure Rust crate with both read AND write EXIF support. Covers PNG, JPEG, HEIF/HEIC/AVIF, JXL, TIFF, WebP.

### `rexif` -- Alternative EXIF Parser
| Version | 0.7.5 | Downloads | 8,875,040 | Recent | 413,570 | Pure Rust | Yes |

### `img-parts` -- Low-Level Container Manipulation
| Version | 0.4.0 | Downloads | 9,087,329 | Recent | 342,354 | Pure Rust | Yes |

Read/write JPEG, PNG, RIFF containers at the chunk level. Useful for metadata injection without re-encoding.

---

## Tier 9: Perceptual Hashing & Quality Metrics

### `image_hasher` -- Perceptual Hashing
| Version | 3.1.1 | Downloads | 459,296 | Recent | 60,484 | Pure Rust | Yes |

Perceptual hashing (aHash, dHash, pHash) and Hamming distance for image similarity.

### `blurhash` -- BlurHash Placeholders
| Version | 0.2.3 | Downloads | 402,529 | Recent | 75,147 | Pure Rust | Yes |

Encode images into short strings that represent blurred placeholders (for progressive loading).

### `thumbhash` -- ThumbHash Placeholders
| Version | 0.1.0 | Downloads | 173,628 | Recent | 19,132 | Pure Rust | Yes |

More advanced than BlurHash. Preserves aspect ratio and colors better.

### `dssim` -- Structural Similarity
| Version | 3.4.0 | Downloads | 208,528 | Recent | 10,106 | Pure Rust | Yes |

Multi-scale SSIM for image quality comparison.

### `ssimulacra2` -- SSIMULACRA2 Metric
| Version | 0.5.1 | Downloads | 19,721 | Recent | 1,455 | Pure Rust | Yes |

State-of-the-art perceptual quality metric from the JPEG XL project.

### `butteraugli` -- Google's Quality Metric
| Version | 0.9.0 | Downloads | 3,717 | Recent | 2,038 | Pure Rust | Yes |

Pure Rust port of Google's butteraugli from libjxl.

---

## Tier 10: Compression Infrastructure

### `fdeflate` -- Fast Deflate
| Version | 0.3.7 | Downloads | 79,367,419 | Recent | 16,754,887 | Pure Rust | Yes |

Specialized fast deflate for PNG. Part of image-rs.

### `zune-inflate` -- Fast Inflate
| Version | 0.2.54 | Downloads | 37,546,021 | Recent | 8,043,162 | Pure Rust | Yes |

Heavily optimized deflate decompressor.

### `simd-adler32` -- SIMD Checksum
| Version | 0.3.8 | Downloads | 153,965,668 | Recent | 49,881,015 | Pure Rust | Yes |

SIMD-accelerated Adler-32 for PNG/zlib.

---

## Tier 11: RAW Camera Formats

### `rawloader` -- Camera RAW Decoder
| Version | 0.37.1 | Downloads | 290,656 | Recent | 6,989 | Pure Rust | Yes |

Extracts data from camera RAW formats (CR2, NEF, ARW, etc.).

### `rawler` -- Modern RAW Library
| Version | 0.7.2 | Downloads | 43,097 | Recent | 22,279 | Pure Rust | Mostly |

Extract images and metadata from camera RAW formats. Powers dnglab.

### `libraw-rs` -- LibRaw Bindings
| Version | 0.0.4 | Downloads | 39,858 | Recent | 5,367 | Pure Rust | NO (C) |

---

## Tier 12: Video/Multimedia (Adjacent)

### `ffmpeg-next` -- FFmpeg Bindings
| Version | 8.1.0 | Downloads | 2,120,287 | Recent | 1,192,042 | System Dep | FFmpeg |

Safe FFmpeg wrapper. For when you need to extract frames or transcode.

### `gstreamer` -- GStreamer Bindings
| Version | 0.25.1 | Downloads | 7,114,932 | Recent | 981,893 | System Dep | GStreamer |

### `symphonia` -- Pure Rust Audio
| Version | 0.5.5 | Downloads | 4,912,472 | Recent | 1,413,173 | Pure Rust | Yes |

### `mp4parse` -- MP4 Parser
| Version | 0.17.0 | Downloads | 862,791 | Recent | 206,254 | Pure Rust | Yes |

Mozilla's MP4 parser. Used by `image` crate for AVIF container parsing.

---

## Tier 13: System-Dep Powerhouses

### `libvips` -- libvips Bindings
| Version | 1.7.3 | Downloads | 787,779 | Recent | 87,386 | System Dep | libvips |

Fast, low-memory image processing. The nuclear option for throughput.

### `magick_rust` -- ImageMagick Bindings
| Version | 2.0.0 | Downloads | 93,017 | Recent | 4,142 | System Dep | ImageMagick |

---

## Decision Matrix: Pure Rust vs. System Dependencies

| Use Case | Pure Rust | System Dep (Faster/Better) |
|----------|-----------|---------------------------|
| JPEG decode | `zune-jpeg` | `turbojpeg` |
| JPEG encode | `jpeg-encoder` | `mozjpeg` / `turbojpeg` |
| PNG decode/encode | `png` | -- |
| PNG optimize | `oxipng` | -- |
| WebP codec | `image-webp` | `webp` (libwebp) |
| AVIF encode | `ravif` | -- |
| AVIF decode | (via image avif-native) | `dav1d` |
| JXL decode | `jxl-oxide` or `jxl` | `jpegxl-rs` |
| JXL encode | `zune-jpegxl` | `jpegxl-rs` |
| HEIF/HEIC | -- (none viable) | `libheif-rs` |
| ICC profiles | `moxcms` | `lcms2` |
| Color quantization | `quantette` / `imagequant` | -- |
| Image resize | `fast_image_resize` | -- |
| YUV conversion | `yuv` | -- |
| SVG render | `resvg` + `tiny-skia` | -- |
| Bulk processing | -- | `libvips` |

---

## Key Ecosystem Players (Authors to Follow)

| Author | Crates | Focus |
|--------|--------|-------|
| **kornelski** | ravif, avif-serialize, mozjpeg, dssim, rgb, imgref, lodepng, lcms2 | ImageOptim ecosystem, AVIF, compression |
| **awxkee** | moxcms, pic-scale, pic-scale-safe, yuv | Color management, scaling, YUV (now in image-rs) |
| **etemesi254** | zune-* (jpeg, png, core, image, inflate, etc.) | SIMD-accelerated pure Rust codecs |
| **image-rs** | image, png, gif, tiff, fdeflate, jpeg-decoder, image-webp, imageproc | Standard library ecosystem |
| **linebender** | tiny-skia, resvg, usvg, color | 2D rendering |
| **libjxl team** | jxl, jxl_simd, jxl_transforms | Official JXL Rust decoder |

---

## Recommendations for Nika Media Pipeline

For the Nika media pipeline (CAS-based binary artifacts), the recommended stack:

### Minimal (Pure Rust, Zero System Deps)
```toml
image = { version = "0.25", default-features = false, features = ["jpeg", "png", "webp", "gif", "qoi"] }
fast_image_resize = "6.0"
```

### Standard (Pure Rust + AVIF)
```toml
image = { version = "0.25", features = ["default-formats", "rayon"] }
fast_image_resize = "6.0"
blurhash = "0.2"          # placeholder generation
img-parts = "0.4"         # metadata manipulation without re-encoding
```

### Full (Best Quality, System Deps OK)
```toml
image = { version = "0.25", features = ["default-formats", "avif-native", "rayon", "nasm"] }
mozjpeg = "0.10"           # best JPEG compression
oxipng = "10.1"            # PNG optimization
turbojpeg = "1.4"          # fastest JPEG decode/encode
libheif-rs = "2.7"         # HEIF/HEIC support
jpegxl-rs = "0.13"         # full JXL encode+decode
fast_image_resize = "6.0"  # SIMD resizing
imagequant = "4.4"         # lossy PNG compression
```

---

## Sources & Methodology

- **Data source**: crates.io REST API, queried 2026-03-18
- **Crates analyzed**: 80+
- **Metrics**: total downloads, recent downloads (last 90 days), version, update date
- **Verification**: Cross-referenced Cargo.toml of `image` crate for current dependencies
- **Confidence**: High -- data comes directly from crates.io
