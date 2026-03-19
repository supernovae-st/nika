# Media Pipeline Crate Survey

> Research date: 2026-03-18
> Source: crates.io API, GitHub READMEs, docs.rs

---

## 1. Image Metadata Extraction (EXIF / XMP)

### kamadak-exif (the incumbent)

| Field | Value |
|-------|-------|
| Crate | `kamadak-exif` |
| Version | 0.6.1 |
| Downloads | 8,406,730 |
| Reverse deps | 107 |
| Last update | 2024-11-06 |
| Repo | https://github.com/kamadak/exif-rs |
| Pure Rust | Yes |
| License | BSD-2-Clause |

- **Read-only** EXIF parser. No write support.
- Supports JPEG and TIFF containers.
- Stable, widely used (107 reverse dependencies).
- The `image` crate re-exports it under the `exif` feature.

### rexif

| Field | Value |
|-------|-------|
| Crate | `rexif` |
| Version | 0.7.5 |
| Downloads | 8,875,040 |
| Pure Rust | Yes |

- Native Rust, extracts EXIF from JPEG and TIFF.
- Surprisingly high download count (close to kamadak-exif).
- Read-only.

### nom-exif (recommended alternative)

| Field | Value |
|-------|-------|
| Crate | `nom-exif` |
| Version | 2.7.0 |
| Downloads | 103,758 |
| Repo | https://github.com/mindeng/nom-exif |
| Pure Rust | Yes |

- **Supports both images AND video/audio**: HEIC, JPEG, TIFF, RAF, CR3, MP4, MOV, WebM, MKV, MKA, 3GP.
- Unified `MediaParser` + `MediaSource` API for all file types.
- Zero-copy, lazy-parsed, fuzz-tested.
- Supports both sync and async APIs.
- Iterator style (`ExifIter`) and get style (`Exif`).

```rust
use nom_exif::*;
let mut parser = MediaParser::new();
let ms = MediaSource::file_path("photo.heic")?;
let mut iter: ExifIter = parser.parse(ms)?;
let exif: Exif = iter.into();
let make = exif.get(ExifTag::Make).unwrap().as_str().unwrap();
```

### little_exif (read AND write)

| Field | Value |
|-------|-------|
| Crate | `little_exif` |
| Version | 0.6.23 |
| Downloads | 107,383 |
| Repo | https://github.com/TechnikTobi/little_exif |
| Pure Rust | Yes |

- **Only pure Rust crate with true read AND write EXIF support**.
- Supports: PNG, JPEG, HEIF/HEIC/AVIF, JXL, TIFF, WebP.
- Useful for embedding metadata into generated artifacts.

```rust
use little_exif::metadata::Metadata;
use little_exif::exif_tag::ExifTag;

let mut metadata = Metadata::new_from_path(&image_path);
metadata.set_tag(ExifTag::ImageDescription("Hello World!".to_string()));
metadata.write_to_file(&image_path)?;
```

### img-parts (low-level container manipulation)

| Field | Value |
|-------|-------|
| Crate | `img-parts` |
| Version | 0.4.0 |
| Downloads | 9,087,329 |
| Repo | https://github.com/paolobarbolini/img-parts |
| Pure Rust | Yes |

- Low-level API for reading/writing JPEG, PNG, and RIFF (WebP) containers.
- High-level API for reading/writing raw ICC profiles and EXIF metadata.
- Good for stripping or transplanting metadata between images.

```rust
let mut jpeg = Jpeg::from_bytes(input.into())?;
let icc_profile = jpeg.icc_profile();
let exif_metadata = jpeg.exif();
jpeg.set_exif(Some(new_exif_metadata.into()));
jpeg.encoder().write_to(output)?;
```

### XMP Support

| Crate | Version | Downloads | Notes |
|-------|---------|-----------|-------|
| `xmp_toolkit` | 1.12.0 | 196,157 | Rust bindings for Adobe's XMP Toolkit (FFI) |
| `xmp-writer` | 0.3.2 | 867,238 | Write XMP metadata step-by-step (pure Rust) |

### Recommendation for Nika

**Primary**: `nom-exif` for reading (unified image + video, async-ready, fuzz-tested).
**Write support**: `little_exif` for embedding EXIF into generated artifacts.
**Low-level**: `img-parts` for container-level metadata manipulation.
**XMP**: `xmp-writer` for writing XMP (pure Rust, no FFI).

---

## 2. Content Type Detection (Magic Bytes / MIME Sniffing)

### infer (top recommendation)

| Field | Value |
|-------|-------|
| Crate | `infer` |
| Version | 0.19.0 |
| Downloads | 77,346,043 |
| Repo | https://github.com/bojand/infer |
| Pure Rust | Yes |
| no_std | Yes |

- Detects file type by magic number signatures (not file extension).
- Supports: images, video, audio, archives, documents, books.
- Custom matcher support for proprietary formats.
- Minimal dependencies, `no_std` and `no_alloc` compatible.

```rust
// From buffer
let buf = [0xFF, 0xD8, 0xFF, 0xAA];
let kind = infer::get(&buf).expect("file type is known");
assert_eq!(kind.mime_type(), "image/jpeg");
assert_eq!(kind.extension(), "jpg");

// From file path
let kind = infer::get_from_path("sample.jpg")?.expect("known");

// Check category
assert!(infer::is_image(&buf));

// Check specific type
assert!(infer::image::is_jpeg(&buf));
```

### file-format

| Field | Value |
|-------|-------|
| Crate | `file-format` |
| Version | 0.28.0 |
| Downloads | 1,010,270 |
| Pure Rust | Yes |

- Checks signature, uses specific readers for deeper identification.
- Falls back to BIN for unknown formats.
- Feature-gated readers for ZIP, XML, CFB, MP4, etc.
- Provides `name()`, `short_name()`, `media_type()`, `extension()`, `kind()`.

```rust
let fmt = FileFormat::from_file("sample.pdf")?;
assert_eq!(fmt.media_type(), "application/pdf");
assert_eq!(fmt.kind(), Kind::Document);
```

### tree_magic_mini

| Field | Value |
|-------|-------|
| Crate | `tree_magic_mini` |
| Version | 3.2.2 |
| Downloads | 6,771,762 |

- Traverses a filetype tree (FreeDesktop shared-mime-info database).
- More thorough than magic-number-only but heavier.

### Other MIME crates (extension-based, not magic-byte)

| Crate | Version | Downloads | Purpose |
|-------|---------|-----------|---------|
| `mime` | 0.3.17 | 382,966,232 | Strongly-typed MIME types |
| `mime_guess` | 2.0.5 | 173,301,415 | Extension-to-MIME mapping |
| `mediatype` | 0.21.0 | 4,180,422 | MIME parsing |
| `content_inspector` | 0.2.4 | 9,709,188 | Binary vs text detection |

### Recommendation for Nika

**Primary**: `infer` -- 77M downloads, no_std, magic-byte based, extensible with custom matchers.
**Fallback**: `mime_guess` for extension-based detection when magic bytes are inconclusive.
**Deep identification**: `file-format` if you need reader-based detection for container formats.

---

## 3. Image Optimization / Compression

### PNG Optimization

#### oxipng

| Field | Value |
|-------|-------|
| Crate | `oxipng` |
| Version | 10.1.0 |
| Downloads | 1,125,327 |
| Repo | https://github.com/oxipng/oxipng |
| License | MIT |

- Multithreaded lossless PNG/APNG compression optimizer.
- Successor to OptiPNG, rewritten in Rust.
- Used by ImageOptim, Squoosh.
- Optimization levels `-o 0` through `-o 6` (or `-o max`).
- Can strip metadata with `--strip safe`.
- Library API: create `Options` struct, call `optimize()`.

```toml
oxipng = { version = "10.0", features = ["parallel", "zopfli", "filetime"], default-features = false }
```

#### imagequant (lossy PNG via palette reduction)

| Field | Value |
|-------|-------|
| Crate | `imagequant` |
| Version | 4.4.1 |
| Downloads | 943,228 |

- Converts 24/32-bit images to 8-bit palette with alpha.
- Powers `pngquant` -- the standard lossy PNG compressor.
- Dual-licensed (GPL or commercial).

### JPEG Compression

#### mozjpeg

| Field | Value |
|-------|-------|
| Crate | `mozjpeg` |
| Version | 0.10.13 |
| Downloads | 2,154,884 |
| Repo | https://github.com/ImageOptim/mozjpeg-rust |

- Safe(r) Rust wrapper over MozJPEG/libjpeg-turbo.
- Requires `nasm` and C compiler (FFI).
- Error handling via `catch_unwind` (libjpeg design quirk).

```rust
std::panic::catch_unwind(|| -> std::io::Result<Vec<u8>> {
    let mut comp = mozjpeg::Compress::new(mozjpeg::ColorSpace::JCS_RGB);
    comp.set_size(width, height);
    let mut comp = comp.start_compress(Vec::new())?;
    comp.write_scanlines(&pixels[..])?;
    let writer = comp.finish()?;
    Ok(writer)
});
```

#### turbojpeg

| Field | Value |
|-------|-------|
| Crate | `turbojpeg` |
| Version | 1.4.0 |
| Downloads | 1,153,650 |

- Bindings to libjpeg-turbo.
- Fast encoding/decoding + lossless transforms.
- Simpler API than raw mozjpeg.

#### zune-jpeg (pure Rust decoder)

| Field | Value |
|-------|-------|
| Crate | `zune-jpeg` |
| Version | 0.5.13 |
| Downloads | 44,570,070 |

- Fast, correct, safe JPEG decoder. Pure Rust.
- Decode only (no encode).

### WebP

| Crate | Version | Downloads | Notes |
|-------|---------|-----------|-------|
| `image-webp` | 0.2.4 | 32,922,133 | Pure Rust WebP encode/decode (used by `image` crate) |
| `webp` | 0.3.1 | 2,880,847 | Wrapper over libwebp (FFI) |
| `libwebp-sys` | 0.14.2 | 3,148,354 | Raw FFI bindings to libwebp |

- `image-webp` is pure Rust and integrated into the `image` crate ecosystem.
- `webp`/`libwebp-sys` wrap Google's C library for better compression ratios.

### AVIF

| Crate | Version | Downloads | Notes |
|-------|---------|-----------|-------|
| `ravif` | 0.13.0 | 21,200,671 | Pure Rust AVIF encoder via rav1e |

- Powers the `cavif` CLI tool.
- Pure Rust, no FFI needed.

### JPEG XL

| Crate | Version | Downloads | Notes |
|-------|---------|-----------|-------|
| `jxl-oxide` | 0.12.5 | 1,123,853 | Pure Rust JPEG XL decoder |

- Decode only for now (no encode).

### Recommendation for Nika

**PNG lossless**: `oxipng` (standard, multithreaded, battle-tested).
**PNG lossy**: `imagequant` (powers pngquant, but note GPL license).
**JPEG**: `mozjpeg` for best compression, `turbojpeg` for simpler API.
**WebP**: `image-webp` for pure Rust, `webp` for better compression via libwebp FFI.
**AVIF**: `ravif` for future-proofing.
**Decode-only**: `zune-jpeg` is the fastest pure-Rust JPEG decoder.

---

## 4. fast_image_resize (Thumbnail Generation)

| Field | Value |
|-------|-------|
| Crate | `fast_image_resize` |
| Version | 6.0.0 |
| Downloads | 12,969,769 |
| Repo | https://github.com/Cykooz/fast_image_resize |
| License | MIT / Apache-2.0 |

### Key Features

- **SIMD-accelerated**: SSE4.1, AVX2, Neon, WASM SIMD128.
- Pixel formats: U8, U8x2, U8x3, U8x4, U16 variants, F32 variants.
- Resize algorithms: Nearest, Box, Bilinear, Bicubic, Lanczos3.
- Multi-threading via `rayon` feature.
- `no_std` support.
- `image` feature for direct `DynamicImage` integration.

### Performance (4928x3279 -> 852x567, RGB8, single-thread)

| Library | Lanczos3 |
|---------|----------|
| image crate | 189.93 ms |
| resize crate | 140.26 ms |
| libvips | 15.78 ms |
| fir (pure Rust) | 38.08 ms |
| fir (SSE4.1) | 15.30 ms |
| fir (AVX2) | 13.21 ms |

**5-14x faster than the `image` crate, competitive with libvips.**

### API Usage with image-rs

```toml
[dependencies]
fast_image_resize = { version = "6.0", features = ["image"] }
image = "0.25"
```

```rust
use fast_image_resize::{IntoImageView, Resizer, ResizeOptions};
use fast_image_resize::images::Image;
use image::ImageReader;

// Read source image
let src_image = ImageReader::open("photo.png")?.decode()?;

// Create destination container
let dst_width = 256;
let dst_height = 256;
let mut dst_image = Image::new(dst_width, dst_height, src_image.pixel_type().unwrap());

// Resize
let mut resizer = Resizer::new();
resizer.resize(&src_image, &mut dst_image, None).unwrap();

// Access raw pixels
let pixels: &[u8] = dst_image.buffer();
```

### Resize with Cropping

```rust
resizer.resize(
    &img,
    &mut dst_image,
    &ResizeOptions::new().crop(10.0, 10.0, 2000.0, 2000.0),
).unwrap();
```

### Alternatives

| Crate | Version | Downloads | Notes |
|-------|---------|-----------|-------|
| `resize` | 0.8.9 | 1,746,677 | Pure Rust, simpler API, slower |
| `image` (built-in) | 0.25.10 | 107,044,824 | `imageops::resize()`, no SIMD |
| `imageproc` | 0.26.1 | 7,572,535 | Image processing ops (not just resize) |

### Recommendation for Nika

`fast_image_resize` with the `image` feature is the clear winner for thumbnail generation. Enable `rayon` for multi-threaded batch operations.

---

## 5. Content-Addressable Storage (CAS) Patterns

### blake3 (you already have this)

| Field | Value |
|-------|-------|
| Crate | `blake3` |
| Version | 1.8.3 |
| Downloads | 107,897,364 |

- The standard for CAS hashing in 2025/2026.
- 256-bit output, tree-structured, parallelizable.
- Hardware-accelerated (SIMD).

### bao-tree (you already have this)

| Field | Value |
|-------|-------|
| Crate | `bao-tree` |
| Version | 0.16.0 |
| Downloads | 242,906 |

- BLAKE3 verified streaming with custom chunk groups.
- Range set queries for partial verification.
- Used by iroh-blobs.

### cacache (CAS disk cache)

| Field | Value |
|-------|-------|
| Crate | `cacache` |
| Version | 13.1.0 |
| Downloads | 2,738,814 |
| Repo | https://github.com/zkat/cacache-rs |

- Content-addressable, key-value, high-performance on-disk cache.
- Async-first (async-std or tokio).
- Subresource Integrity (SRI) web standard support.
- Multi-hash support (sha1, sha512, etc.).
- Automatic content deduplication.
- Atomic writes, fault tolerant, lockless concurrency.

```rust
// Write
cacache::write(&dir, "key", b"my-data").await?;
// Read
let data = cacache::read(&dir, "key").await?;
```

### ssri (Subresource Integrity)

| Field | Value |
|-------|-------|
| Crate | `ssri` |
| Version | 9.2.0 |
| Downloads | 2,989,324 |

- Utilities for handling Subresource Integrity hashes.
- Used by cacache internally.

### iroh-blobs (distributed CAS)

| Field | Value |
|-------|-------|
| Crate | `iroh-blobs` |
| Version | 0.99.0 |
| Downloads | 119,966 |
| Repo | https://github.com/n0-computer/iroh-blobs |

- Content-addressed blobs over QUIC (p2p).
- Built on bao-tree + BLAKE3 verified streaming.
- Overkill for local CAS but interesting for distributed workflows.

### IPFS-style CAS

| Crate | Version | Downloads | Notes |
|-------|---------|-----------|-------|
| `cid` | 0.11.1 | 14,220,958 | Content Identifier (IPFS-style) |
| `multihash` | 0.19.3 | 33,949,091 | Self-describing hash format |

### CAS Patterns for Nika (DIY approach)

Since Nika already uses blake3, a minimal CAS implementation is straightforward:

```
.nika/cas/
  ab/cd1234...   # first 2 chars as directory prefix
  ef/5678abcd... # content addressed by blake3 hash
```

Key design decisions:
1. **Hash function**: blake3 (already chosen -- fast, parallel, 256-bit)
2. **Storage layout**: 2-char prefix directories (like git objects)
3. **Deduplication**: automatic via content addressing
4. **Atomic writes**: write to temp, rename (already using reflink-copy)
5. **Metadata**: sidecar JSON or embedded in a manifest

### Recommendation for Nika

**Keep the DIY approach** with blake3 + reflink-copy. The CAS pattern is simple enough that a dedicated crate adds more complexity than it saves. Consider `cacache` only if you need key-value lookup alongside content addressing, or if you need multi-hash support.

---

## 6. Video Thumbnail Extraction

### ffmpeg-sidecar (recommended for Nika)

| Field | Value |
|-------|-------|
| Crate | `ffmpeg-sidecar` |
| Version | 2.4.0 |
| Downloads | 889,266 |
| Repo | https://github.com/nathanbabcock/ffmpeg-sidecar |

- Wraps a standalone FFmpeg binary (no FFI, no build complexity).
- Iterator interface over decoded frames.
- Auto-download of FFmpeg binary.
- Cross-platform (Windows, macOS, Linux).

```rust
use ffmpeg_sidecar::command::FfmpegCommand;

let iter = FfmpegCommand::new()
    .input("video.mp4")
    .rawvideo()
    .spawn()?
    .iter()?;

for frame in iter.filter_frames() {
    let _pixels: Vec<u8> = frame.data; // raw RGB pixels
    break; // first frame = thumbnail
}
```

### ffmpeg-next (full FFI bindings)

| Field | Value |
|-------|-------|
| Crate | `ffmpeg-next` |
| Version | 8.1.0 |
| Downloads | 2,120,287 |

- Full FFmpeg bindings (FFmpeg 4+ compatible).
- Requires system FFmpeg libraries.
- Complex build setup, especially on Windows.
- Most powerful option but heaviest dependency.

### video-rs (high-level FFmpeg wrapper)

| Field | Value |
|-------|-------|
| Crate | `video-rs` |
| Version | 0.11.0 |
| Downloads | 168,617 |
| Repo | https://github.com/oddity-ai/video-rs |

- High-level video toolkit based on ffmpeg.
- Depends on `ffmpeg-next` (same build complexity).
- Nice iterator API for frame decoding.

```rust
use video_rs::decode::Decoder;
let mut decoder = Decoder::new(source)?;
for frame in decoder.decode_iter() {
    if let Ok((_, frame)) = frame { /* ... */ }
}
```

### gstreamer

| Field | Value |
|-------|-------|
| Crate | `gstreamer` |
| Version | 0.25.1 |
| Downloads | 7,114,932 |

- Full GStreamer bindings.
- Very powerful, supports everything.
- Heavy dependency, requires GStreamer installation.

### Recommendation for Nika

**`ffmpeg-sidecar`** is the best fit for Nika:
- No FFI, no build complexity (just a binary on PATH).
- Auto-download support.
- Simple iterator API for frame extraction.
- Feature-gate it behind `video` feature so it's optional.

---

## 7. infer Crate (File Type Detection) -- Deep Dive

| Field | Value |
|-------|-------|
| Crate | `infer` |
| Version | 0.19.0 |
| Downloads | 77,346,043 |
| Repo | https://github.com/bojand/infer |
| no_std | Yes |
| no_alloc | Yes (without custom matchers) |

### Supported Types

**Images**: JPEG, PNG, GIF, WebP, CR2, TIFF, BMP, HEIF, AVIF, JXR, PSD, ICO, ORA, DJVU
**Video**: MP4, M4V, MKV, WebM, MOV, AVI, WMV, MPEG, FLV
**Audio**: MIDI, MP3, M4A, OGG, FLAC, WAV, AMR, AAC, AIFF, DSF, APE
**Archives**: ZIP, TAR, RAR, GZ, BZ2, 7Z, XZ, PDF, SQLite, ZST, LZ4
**Documents**: DOC, DOCX, XLS, XLSX, PPT, PPTX, ODT, ODS, ODP

### API Summary

```rust
// From bytes (only needs first few bytes)
let kind = infer::get(&buf).expect("known type");
kind.mime_type()  // "image/jpeg"
kind.extension()  // "jpg"

// From file path (reads only needed bytes)
let kind = infer::get_from_path("file.png")?.expect("known");

// Category checks
infer::is_image(&buf)
infer::is_video(&buf)
infer::is_audio(&buf)
infer::is_archive(&buf)
infer::is_document(&buf)

// Specific type checks
infer::image::is_jpeg(&buf)
infer::image::is_png(&buf)
infer::image::is_webp(&buf)

// Custom matchers
let mut info = infer::Infer::new();
info.add("custom/nika-artifact", "nika", |buf| {
    buf.len() >= 4 && &buf[..4] == b"NIKA"
});
```

### Key Properties for Nika

- Only reads a few bytes (efficient for large files).
- Works with `&[u8]` slices (no file I/O required).
- Custom matchers for Nika-specific artifact formats.
- `no_std` compatible if you need it for WASM targets.

---

## 8. resvg (SVG to PNG Rasterization)

| Field | Value |
|-------|-------|
| Crate | `resvg` |
| Version | 0.47.0 |
| Downloads | 10,912,853 |
| Repo | https://github.com/linebender/resvg |
| License | MIT / Apache-2.0 |
| MSRV | 1.87.0+ |
| Pure Rust | Yes (100%) |

### Architecture

```
resvg (renderer, ~2500 LOC)
  --> usvg (SVG preprocessor/simplifier)
  --> tiny-skia (Skia subset, ported to Rust)
  --> rustybuzz (harfbuzz subset, ported to Rust)
  --> ttf-parser (TrueType/OpenType font parser)
  --> fontdb (CSS-like font database)
  --> roxmltree (XML parser)
  --> simplecss (CSS 2 parser)
```

Total project: ~75,000 LOC. Final binary: <3MB.

### Key Properties

- **Correctness**: ~1600 SVG-to-PNG regression tests.
- **Safety**: 100% Rust, almost no `unsafe`, no non-Rust code in final binary.
- **Portable**: Compiles everywhere Rust does, including WASM.
- **Reproducible**: Identical pixel output across all platforms.
- **Zero bloat**: No external dependencies at runtime.
- **Two-phase**: Parsing (usvg) and rendering (resvg) are separate.

### Ecosystem Crates

| Crate | Version | Downloads | Purpose |
|-------|---------|-----------|-------|
| `resvg` | 0.47.0 | 10,912,853 | SVG renderer |
| `usvg` | 0.47.0 | 12,075,956 | SVG simplifier/preprocessor |
| `tiny-skia` | 0.12.0 | 22,722,017 | 2D rendering engine |
| `svgtypes` | 0.16.1 | 12,625,871 | SVG type parser |
| `xmlwriter` | 0.1.0 | 10,528,984 | Streaming XML writer |

### API Usage

```rust
use resvg::usvg;
use resvg::tiny_skia;

// Parse SVG
let opt = usvg::Options::default();
let tree = usvg::Tree::from_str(&svg_string, &opt)?;

// Get the SVG size
let size = tree.size();

// Create a pixmap (RGBA buffer)
let mut pixmap = tiny_skia::Pixmap::new(
    size.width() as u32,
    size.height() as u32,
)?;

// Render
resvg::render(&tree, tiny_skia::Transform::default(), &mut pixmap.as_mut());

// Save as PNG
pixmap.save_png("output.png")?;
```

### Rendering at Custom Size

```rust
let tree = usvg::Tree::from_str(&svg_string, &usvg::Options::default())?;
let target_width = 1024u32;
let target_height = 768u32;

let mut pixmap = tiny_skia::Pixmap::new(target_width, target_height)?;

// Scale to fit
let sx = target_width as f32 / tree.size().width();
let sy = target_height as f32 / tree.size().height();
let scale = sx.min(sy); // maintain aspect ratio

let transform = tiny_skia::Transform::from_scale(scale, scale);
resvg::render(&tree, transform, &mut pixmap.as_mut());
```

### Limitations

- No animations (by design -- static SVG only).
- No native text rendering (uses its own font rendering).
- UTF-8 only.

### Recommendation for Nika

`resvg` is the only serious option for SVG rasterization in Rust. It is production-quality, 100% Rust, cross-platform reproducible, and actively maintained (now under the linebender org). Use `usvg` for SVG validation/simplification before rendering.

---

## Summary: Recommended Crate Stack for Nika Media Pipeline

| Capability | Crate | Version | Downloads | Why |
|-----------|-------|---------|-----------|-----|
| **File type detection** | `infer` | 0.19.0 | 77M | Magic bytes, no_std, extensible |
| **Image decode/encode** | `image` | 0.25.10 | 107M | Ecosystem standard |
| **Thumbnail resize** | `fast_image_resize` | 6.0.0 | 13M | SIMD, 5-14x faster than image |
| **EXIF read** | `nom-exif` | 2.7.0 | 104K | Images + video, async, fuzz-tested |
| **EXIF write** | `little_exif` | 0.6.23 | 107K | Only pure Rust read+write |
| **Metadata containers** | `img-parts` | 0.4.0 | 9M | Low-level JPEG/PNG/WebP |
| **PNG optimization** | `oxipng` | 10.1.0 | 1.1M | Multithreaded, lossless |
| **JPEG compression** | `mozjpeg` | 0.10.13 | 2.2M | Best compression ratios |
| **WebP** | `image-webp` | 0.2.4 | 33M | Pure Rust, in image ecosystem |
| **SVG rasterization** | `resvg` | 0.47.0 | 11M | 100% Rust, cross-platform |
| **CAS hashing** | `blake3` | 1.8.3 | 108M | Already chosen, fastest |
| **Verified streaming** | `bao-tree` | 0.16.0 | 243K | Already chosen |
| **Video thumbnails** | `ffmpeg-sidecar` | 2.4.0 | 889K | No FFI, optional feature |
| **XMP write** | `xmp-writer` | 0.3.2 | 867K | Pure Rust |

### Dependency Weight Tiers

**Tier 1 (always included, pure Rust):**
- `infer`, `image`, `fast_image_resize`, `blake3`, `imagesize`

**Tier 2 (feature-gated, pure Rust):**
- `nom-exif`, `little_exif`, `img-parts`, `oxipng`, `resvg`/`usvg`/`tiny-skia`

**Tier 3 (feature-gated, FFI):**
- `mozjpeg` (requires nasm + C compiler)
- `webp`/`libwebp-sys` (requires cmake)

**Tier 4 (feature-gated, external binary):**
- `ffmpeg-sidecar` (requires ffmpeg on PATH)

---

## Sources

All data retrieved from:
- crates.io API (`https://crates.io/api/v1/crates/<name>`)
- GitHub README files (raw.githubusercontent.com)
- docs.rs documentation pages

## Methodology

- Crates.io API queried for: version, download counts, descriptions, repository URLs
- GitHub READMEs fetched for: API examples, feature lists, benchmarks
- 50+ crates surveyed across 8 categories
- Cross-referenced reverse dependency counts for ecosystem adoption

## Confidence Level

**High** -- All data from primary sources (crates.io, GitHub repos). Download counts and version numbers are authoritative. API examples verified against published documentation.
