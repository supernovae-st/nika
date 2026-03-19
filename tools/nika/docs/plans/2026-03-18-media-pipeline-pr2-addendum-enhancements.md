# PR2 Addendum: Media Pipeline Enhancements

> **Date:** 2026-03-18
> **Branch:** `feat/media-artifacts`
> **Context:** Additional commits to execute AFTER the core PR2 8-commit plan (v6.0)
> **Research:** 22 parallel agents, 300+ crates surveyed, live-verified via Perplexity + crates.io API
> **Research files:** `docs/research/2026-03-18-*.md` (10+ files)

---

## Executive Summary

22 parallel research agents surveyed 300+ Rust crates across 20 media domains. This addendum identifies **4 commits to add to PR2** (~100 LOC total, zero risk) and documents **the complete Rust media ecosystem** for the v0.31+ roadmap.

The biggest discoveries:
1. **`c2pa`** (8.6M downloads) -- official C2PA SDK by Adobe/CAI for AI content provenance
2. **`fast_image_resize`** -- 14x faster than image-rs for thumbnails via SIMD
3. **`symphonia`** -- pure Rust audio decoder replacing the entire old ecosystem
4. **`ocrs`** -- OCR in pure Rust, compiles to WASM
5. **`charts-rs`** -- JSON-first chart generation, maps directly to Nika `with:` bindings
6. **`qrcode-ai-scanner-core`** -- SuperNovae's own QR validation crate
7. **`thumbhash`** -- 25 bytes encode an image placeholder (better than BlurHash, zero deps)
8. **`lele`** -- AOT ONNX-to-Rust compiler, 1.93x faster than ORT on CPU (Feb 2026)
9. **`qoi`** -- 36M downloads, 10-20x faster than PNG encode/decode (pipeline intermediate)

---

## PR2 Additional Commits (after C8)

### C9: `perf(io): use reflink CoW for binary artifact copies`

**Crate:** `reflink-copy = "0.1"` (7.6M downloads)
**LOC:** ~15
**Risk:** Near-zero (auto-fallback to `std::fs::copy`)

Replace `tokio::fs::copy()` with `reflink_or_copy()` in `write_binary()`.
On APFS (macOS) and btrfs (Linux): **instant, zero disk cost**.
On other filesystems: transparent fallback, zero regression.

```rust
// In src/io/writer.rs, write_binary() BinarySource::CasPath branch:
// BEFORE:
//   tokio::fs::copy(&cas_path, &full_path).await?;
// AFTER:
let cas = cas_path.clone();
let art = full_path.clone();
let result = tokio::task::spawn_blocking(move || {
    // Delete target first if exists (reflink requires create_new)
    let _ = std::fs::remove_file(&art);
    reflink_copy::reflink_or_copy(&cas, &art)
}).await.map_err(|e| NikaError::ArtifactWriteError {
    path: full_path.display().to_string(),
    reason: format!("copy task failed: {e}"),
})??;

match result {
    None => tracing::debug!("reflink (instant CoW clone)"),
    Some(bytes) => tracing::debug!(bytes, "regular copy fallback"),
}
```

**Cargo.toml:**
```toml
reflink-copy = "0.1"
```

**Test:** Write binary from CAS, verify content matches, check file size.

**Gotcha:** `reflink_or_copy()` requires destination NOT to exist (uses `create_new`).
Must `remove_file` first if overwriting.

---

### C10: `feat(media): add image dimensions to MediaRef metadata`

**Crate:** `imagesize = "0.13"` (12M downloads, literally ZERO dependencies)
**LOC:** ~20
**Risk:** Near-zero (header-only read, no full decode)

After CAS store, read image dimensions from the first few bytes. Add `width` and `height` to `MediaRef` metadata. This enables `{{with.img.width}}` and `{{with.img.height}}` in templates.

```rust
// In src/media/processor.rs, after CAS store:
let dimensions = if media_ref.media_type.is_image() {
    imagesize::blob_size(&data).ok().map(|s| (s.width, s.height))
} else {
    None
};
// Add to MediaRef:
if let Some((w, h)) = dimensions {
    media_ref.metadata.insert("width".into(), serde_json::json!(w));
    media_ref.metadata.insert("height".into(), serde_json::json!(h));
}
```

**Cargo.toml:**
```toml
imagesize = "0.13"
```

**Formats supported:** PNG, JPEG, GIF, WebP, BMP, TIFF, PSD, ICO, HEIF, AVIF, JXL, QOI (20+ formats from header only).

**Test:** Store a known PNG, verify `media_ref.metadata["width"]` and `height`.

---

### C11: `feat(media): add perceptual hash to MediaRef for near-duplicate detection`

**Crate:** `image_hasher = "3.1"` (459K downloads)
**LOC:** ~25
**Risk:** Low (additive metadata, no behavior change)

Compute a pHash (DCT perceptual hash) for each stored image. Store as base64 in MediaRef metadata. Enables near-duplicate detection across workflow runs.

```rust
// In src/media/processor.rs, after CAS store (feature-gated):
#[cfg(feature = "media-hash")]
{
    if media_ref.media_type.is_image() {
        if let Ok(img) = image::load_from_memory(&data) {
            let hasher = image_hasher::HasherConfig::new()
                .hash_alg(image_hasher::HashAlg::DoubleGradient)
                .hash_size(8, 8)
                .to_hasher();
            let hash = hasher.hash_image(&img);
            media_ref.metadata.insert("phash".into(), serde_json::json!(hash.to_base64()));
        }
    }
}
```

**Cargo.toml:**
```toml
[features]
media-hash = ["dep:image_hasher"]

[dependencies]
image_hasher = { version = "3.1", optional = true }
```

**Test:** Hash two identical images → distance 0. Hash resized version → distance < 10.

---

### C12: `docs(research): comprehensive Rust media ecosystem survey`

**LOC:** 0 (docs-only commit)
**Risk:** Zero

Commit all `docs/research/2026-03-18-*.md` files produced by the 22 research agents. These serve as the definitive reference for future media feature development.

---

## Cargo.toml Summary (PR2 additions)

```toml
# Always-on (Tier 1)
reflink-copy = "0.1"        # CoW file cloning
imagesize = "0.13"           # Header-only image dimensions (zero deps)

# Feature-gated (Tier 2)
image_hasher = { version = "3.1", optional = true }  # Perceptual hashing

[features]
media-hash = ["dep:image_hasher"]
```

---

## Roadmap: Feature-Gated Media Capabilities (v0.31+)

Everything below is documented and researched. Each could become a PR.

### Tier 1 -- High Impact, Pure Rust, Low Risk

| Feature | Crate(s) | Downloads | LOC est. | Impact |
|---------|----------|-----------|----------|--------|
| **Thumbnail generation** | `image` 0.25 + `fast_image_resize` 6.0 | 107M + 13M | ~80 | SIMD, 14x faster than image-rs alone |
| **Audio metadata** | `symphonia` 0.5 + `lofty` 0.23 | 4.9M + 489K | ~60 | Duration, bitrate, tags for audio artifacts |
| **Video metadata** | `mp4` 0.14 | 9.8M | ~40 | Duration, resolution, codec for video |
| **SVG rendering** | `resvg` 0.47 + `usvg` 0.47 | 11M + 12M | ~50 | SVG → PNG, pure Rust |
| **PDF text extraction** | `pdf_oxide` 0.3 | 433 stars | ~30 | 0.8ms, 100% pass rate, has MCP server |
| **Chart generation** | `charts-rs` 0.3 | 116K | ~50 | JSON → SVG/PNG, 10 chart types |
| **CAS compression** | `zstd` 0.13 | 57M | ~40 | 1.5 GB/s decompress, write-once-read-many |

### Tier 2 -- Game Changers, Some Deps

| Feature | Crate(s) | Downloads | Notes |
|---------|----------|-----------|-------|
| **C2PA provenance** | `c2pa` 0.78 | 8.6M | Adobe SDK, cryptographic AI content proof, MIT |
| **QR validation** | `qrcode-ai-scanner-core` 0.2 | 97 | SuperNovae's own, 89% success on artistic QR |
| **OCR** | `ocrs` 0.12 | 69K | Pure Rust, WASM, ~280ms/page |
| **Image optimization** | `oxipng` 10.1 | 1.1M | Lossless PNG, multithreaded |
| **EXIF read/write** | `nom-exif` 2.7 + `little_exif` 0.6 | 104K + 107K | Images + video + audio metadata |

### Tier 3 -- Specialized, Feature-Flagged

| Feature | Crate(s) | Notes |
|---------|----------|-------|
| **Invisible watermark** | `trustmark` | Neural watermark surviving compression/crop (ICCV 2025) |
| **Video thumbnails** | `ffmpeg-sidecar` 2.4 | No FFI, wraps FFmpeg binary, 889K downloads |
| **RAW camera** | `rawler` 0.7 | CR2/NEF/ARW decode, pure Rust |
| **HEIF/HEIC** | `libheif-rs` 2.7 | iPhone photos, requires C dep |
| **ML inference** | `ort` 2.0 or `candle` | ONNX models for super-resolution, classification |
| **Animated GIF/WebP** | `gif` 76.5M + `webp_animation` 89K | Frame-based universal pattern |
| **Color science** | `palette` 0.7 + `tiny-skia` 0.12 | OKLCH/LAB + 2D compositing |
| **Text overlay** | `cosmic-text` 3.8M + `ab_glyph` 24.7M | Multi-line, BiDi, font fallback |
| **Data visualization** | `plotters` 138.9M | THE chart library, SVG/PNG |
| **Spreadsheet** | `calamine` 6.2M + `rust_xlsxwriter` 1.6M | Read + write xlsx |
| **Perceptual quality** | `dssim-core` 3.4 | MS-SSIM quality metric (AGPL!) |
| **3D models** | `gltf` 6M | glTF 2.0 standard |
| **Barcode** | `rxing` 281K | ZXing port, all 1D + 2D codes |
| **Screenshot** | `xcap` 537K | Cross-platform capture |
| **Lottie** | `rlottie` | Lottie animation → frames |
| **Dominant color** | `color-thief` 460K | Extract palette from image |
| **BlurHash** | `blurhash` 0.2 | Compact image placeholder |
| **JPEG XL** | `jxl-rs` 0.4 | Official libjxl team, pure Rust |

### Tier 3.5 -- Tiny Gems (Zero/Minimal Deps, Huge Value)

| Feature | Crate | Downloads | Notes |
|---------|-------|-----------|-------|
| **Image placeholder** | `thumbhash` 0.1 | 173K | 25 bytes → blurry preview. Better than BlurHash. Zero deps. |
| **Fast intermediate** | `qoi` 0.4 | 36.1M | QOI format: 10-20x faster than PNG, lossless. Pipeline intermediate. |
| **Dominant color** | `dominant_color` | 74K | Extract dominant color. Zero deps. |
| **Color palette** | `color-thief` | 460K | N dominant colors via median cut. |
| **Minimal PNG** | `minipng` | 302K | no_std, no_alloc PNG decoder. 9x smaller in WASM. |
| **Fast BlurHash** | `fast-blurhash` | 6.8K | Zero-alloc BlurHash. Zero deps. |

### Tier 3.6 -- ML Inference

| Feature | Crate | Downloads | Notes |
|---------|-------|-----------|-------|
| **ONNX inference** | `ort` 2.0 | 7.3M | CUDA/Metal/TensorRT. Run any ONNX model. |
| **HF models** | `candle` ~0.8 | 1.7M recent | CLIP, BLIP, SD, ResNet. Metal on Mac. 19.7K stars. |
| **Pure Rust ML** | `rten` 0.24 | 170K | Pure Rust ONNX runtime. Powers `ocrs`. WASM. |
| **AOT compiler** | `lele` (Feb 2026) | new | ONNX → Rust, 1.93x faster than ORT on CPU. 1.7MB WASM. |
| **Backend-agnostic** | `burn` 0.19 | 200K | Multi-GPU via WGPU/Tch. 14.6K stars. |
| **Edge inference** | `tract` | mature | Sonos engine. Audio/streaming on constrained hardware. |

### Tier 4 -- Watch List (Too New or Niche)

| Crate | Notes |
|-------|-------|
| `OxiMedia` (40+ crates) | "FFmpeg of Rust", born Feb 2026, broadcast-grade ambition |
| `ultrahdr-core` 0.2 | Ultra HDR gain map, by Imageflow team |
| `burn-vision` | Image preprocessing on GPU tensors (March 2026) |
| `jpegli` 0.1 | Google's next-gen JPEG, 35% better than MozJPEG |
| `quantette` | New color quantization, faster than imagequant, no GPL |
| `lele` | AOT ONNX→Rust, watch for model support expansion |

---

## The Vision: Nika as Universal Media Pipeline

```
                    ┌─────────────────────────────┐
                    │      Nika Workflow YAML      │
                    │   infer: / exec: / invoke:   │
                    └──────────────┬──────────────┘
                                   │
                    ┌──────────────▼──────────────┐
                    │       Media Pipeline         │
                    │                              │
                    │  ┌─── Detection ──────────┐  │
                    │  │ infer (magic bytes)     │  │
                    │  │ imagesize (dimensions)  │  │
                    │  │ image_hasher (phash)    │  │
                    │  └────────────────────────┘  │
                    │                              │
                    │  ┌─── Storage ────────────┐  │
                    │  │ blake3 (CAS hash)      │  │
                    │  │ reflink-copy (CoW)     │  │
                    │  │ zstd (compression)     │  │
                    │  │ bao-tree (streaming)   │  │
                    │  └────────────────────────┘  │
                    │                              │
                    │  ┌─── Processing ─────────┐  │
                    │  │ fast_image_resize      │  │
                    │  │ resvg (SVG→PNG)        │  │
                    │  │ oxipng (optimize)      │  │
                    │  │ charts-rs (charts)     │  │
                    │  └────────────────────────┘  │
                    │                              │
                    │  ┌─── Metadata ───────────┐  │
                    │  │ nom-exif (EXIF)        │  │
                    │  │ lofty (audio tags)     │  │
                    │  │ mp4 (video meta)       │  │
                    │  │ c2pa (provenance)      │  │
                    │  └────────────────────────┘  │
                    │                              │
                    │  ┌─── Intelligence ───────┐  │
                    │  │ ocrs (OCR)             │  │
                    │  │ qrcode-ai-scanner-core │  │
                    │  │ ort/candle (ML)        │  │
                    │  └────────────────────────┘  │
                    │                              │
                    └──────────────┬──────────────┘
                                   │
                    ┌──────────────▼──────────────┐
                    │     CAS Store (.nika/)       │
                    │  blake3 hash → flat file     │
                    │  + .obao (verified stream)   │
                    │  + .provenance.json (C2PA)   │
                    │  + metadata (dimensions,     │
                    │    phash, EXIF, tags)         │
                    └─────────────────────────────┘
```

---

## Competitive Landscape After Full Implementation

```
Feature                        Nika    n8n     Dagger  Temporal  Argo
──────────────────────────────────────────────────────────────────────
CAS artifact storage           YES     No      No      No        No
blake3 integrity               YES     No      No      No        No
Reflink zero-copy              YES     No      No      No        No
C2PA AI provenance             YES     No      No      No        No
SIMD thumbnail generation      YES     Partial No      No        No
Audio metadata extraction      YES     No      No      No        No
Video metadata extraction      YES     No      No      No        No
OCR (pure Rust)                YES     No      No      No        No
QR code validation             YES     No      No      No        No
Chart generation (JSON→PNG)    YES     No      No      No        No
PDF text extraction            YES     No      No      No        No
SVG rendering                  YES     No      No      No        No
Image optimization             YES     Partial No      No        No
Perceptual dedup               YES     No      No      No        No
WASM media plugins             YES     No      No      No        No
Verified streaming (bao)       YES     No      No      No        No
```

No competitor is even attempting this. Nika would be **generationally ahead**.

---

## Research Files Index

All saved in `docs/research/`:

| File | Domain | Crates surveyed |
|------|--------|-----------------|
| `2026-03-18-media-pipeline-crate-survey.md` | Comprehensive survey | 50+ |
| `2026-03-18-rust-image-crates-survey.md` | Image processing | 30+ |
| `2026-03-18-rust-audio-crates-ecosystem.md` | Audio | 20+ |
| `2026-03-18-rust-video-crates-landscape.md` | Video | 15+ |
| `2026-03-18-rust-document-processing-crates.md` | PDF/DOCX/SVG | 25+ |
| `2026-03-18-media-metadata-crates.md` | EXIF/XMP/ID3/ICC | 20+ |
| `2026-03-18-rust-media-optimization-crates.md` | Compression/optimization | 40+ |
| `2026-03-18-rust-wasm-media-crates.md` | WASM processing | 10+ |
| `2026-03-18-bleeding-edge-rust-media-crates.md` | 2026 cutting edge | 20+ |
| `2026-03-18-cas-rust-crates-research.md` | CAS/integrity | 15+ |
| `2026-03-18-special-media-crates-survey.md` | 3D/QR/fonts/charts | 130+ |
| `2026-03-18-steganography-watermark-c2pa-crates.md` | Provenance/watermark | 13+ |
| `2026-03-18-compression-crates-for-cas.md` | zstd/lz4/brotli | 10+ |
| `2026-03-18-rust-color-rendering-crates.md` | Color/rendering | 15+ |
| `2026-03-18-rust-data-visualization-crates.md` | Charts/plots | 10+ |
| `2026-03-18-animated-media-rust-crates.md` | GIF/WebP/APNG/Lottie | 15+ |
| `2026-03-18-rust-ml-inference-crates.md` | ONNX/candle/burn/lele | 20+ |
| `2026-03-18-tiny-media-crates.md` | thumbhash/qoi/blurhash | 15+ |
| `2026-03-18-rust-audio-crates-ecosystem.md` | symphonia/lofty/hound | 20+ |
| `2026-03-18-rust-video-crates-landscape.md` | mp4/ffmpeg-sidecar | 15+ |

---

## Sources

- 22 parallel research agents (Perplexity AI + crates.io API + GitHub)
- 300+ crates analyzed
- All download counts verified via crates.io API on 2026-03-18
- Cross-referenced with Context7 documentation for key APIs
- Patent/license analysis for HEIF, gifski, dssim
