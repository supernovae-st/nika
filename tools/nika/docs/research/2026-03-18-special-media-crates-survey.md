# Research Report: Rust Crates for Special Media Types in AI Agent Workflows

**Date:** 2026-03-18
**Scope:** Comprehensive survey of every media type a Rust workflow engine could handle
**Method:** crates.io API queries across 130+ crates, cross-referenced by download counts

---

## Summary

Rust's ecosystem can handle **every media type an AI agent would generate or consume**, from QR codes to 3D models to animated GIFs. The ecosystem splits into two tiers: **battle-tested giants** (>10M downloads, used in production everywhere) and **specialist crates** (100K-5M downloads, focused on one format). This survey catalogs 130+ crates across 16 media categories.

---

## 1. QR Code & Barcode Generation/Decoding

The most directly relevant category for QR Code AI workflows.

| Crate | Version | Downloads | Recent (90d) | Role |
|-------|---------|-----------|--------------|------|
| **qrcode** | 0.14.1 | 11.4M | 2.2M | QR code **encoder** -- the standard. Outputs to `image` crate or SVG |
| **rqrr** | 0.10.1 | 3.0M | 573K | QR code **decoder** from images. Pure Rust |
| **fast_qr** | 0.13.1 | 232K | 38K | Optimized QR encoder, faster than `qrcode` for bulk |
| **rxing** | 0.8.5 | 281K | 43K | ZXing port -- **multi-format** reader/writer (QR, DataMatrix, Aztec, PDF417, all 1D) |
| **quircs** | 0.10.3 | 222K | 19K | QR decoder, pure Rust, alternative to rqrr |
| **barcoders** | 2.0.0 | 283K | 41K | 1D barcode **encoder** (EAN-13, UPC-A, Code39, Code128, Codabar, ITF, etc.) |

**Key insight:** `qrcode` + `rqrr` is the canonical encode/decode pair. `rxing` is the Swiss Army knife that handles everything including 2D codes. For Nika's QR Code AI use case, `qrcode` is already the right choice.

---

## 2. 3D Model Processing (glTF, OBJ, STL)

| Crate | Version | Downloads | Recent (90d) | Role |
|-------|---------|-----------|--------------|------|
| **gltf** | 1.4.1 | 6.0M | 1.1M | glTF 2.0 loader/parser -- the standard for 3D on the web |
| **stl_io** | 0.11.0 | 2.5M | 1.0M | STL read/write -- the standard for 3D printing |
| **obj-rs** | 0.7.4 | 703K | 23K | Wavefront OBJ parser (mesh + materials) |
| **mesh-loader** | 0.1.13 | 108K | 15K | Multi-format 3D parser (OBJ, glTF, STL) |
| **three-d** | 0.18.2 | 266K | 43K | Full 2D/3D renderer (cross-platform, WebGL) |
| **nom-stl** | 0.2.2 | 36K | 511 | Alternative STL parser using nom |

**Key insight:** glTF is the "JPEG of 3D" -- AI image generators increasingly output 3D via glTF. The `gltf` crate is mature and well-maintained. STL matters for 3D printing workflows. An AI agent generating 3D assets would produce glTF; one preparing print files would use STL.

---

## 3. Font Rendering & Text Rasterization

| Crate | Version | Downloads | Recent (90d) | Role |
|-------|---------|-----------|--------------|------|
| **ttf-parser** | 0.25.1 | 56.5M | 12.7M | Font file parser (TrueType, OpenType, AAT). Zero-alloc |
| **owned_ttf_parser** | 0.25.1 | 31.8M | 6.1M | ttf-parser with owned data support |
| **ab_glyph** | 0.2.32 | 24.7M | 5.1M | Font loading, scaling, positioning, rasterization |
| **ab_glyph_rasterizer** | 0.1.10 | 25.4M | 4.9M | Low-level coverage rasterizer for bezier curves |
| **rusttype** | 0.9.3 | 13.9M | 1.3M | TrueType rasterizer (older, still widely used) |
| **font-kit** | 0.14.3 | 9.9M | 2.8M | Cross-platform font loading (system fonts, files) |
| **swash** | 0.2.6 | 5.5M | 1.3M | Font introspection, complex text shaping, glyph rendering |
| **fontdue** | 0.9.3 | 3.9M | 1.4M | no_std font rasterizer, very fast |
| **cosmic-text** | 0.18.2 | 3.8M | 951K | Multi-line text layout (used by COSMIC desktop) |

**Key insight:** For rendering text onto images (watermarks, captions, QR labels), the chain is: `ttf-parser` -> `ab_glyph` -> rasterize onto `image` buffer. `cosmic-text` handles complex multi-line layout. `fontdue` is the fastest single-glyph rasterizer.

---

## 4. Image Codecs (Encode/Decode)

| Crate | Version | Downloads | Recent (90d) | Format |
|-------|---------|-----------|--------------|--------|
| **image** | 0.25.10 | 107M | 18.8M | Meta-crate: PNG, JPEG, GIF, WebP, TIFF, BMP, ICO, AVIF |
| **png** | 0.18.1 | 112M | 22.3M | PNG encode/decode |
| **jpeg-decoder** | 0.3.2 | 67.6M | 7.4M | JPEG decode |
| **tiff** | 0.11.3 | 65.1M | 12.8M | TIFF encode/decode |
| **gif** | 0.14.1 | 76.5M | 13.2M | GIF encode/decode |
| **image-webp** | 0.2.4 | 32.9M | 8.0M | WebP encode/decode (pure Rust) |
| **webp** | 0.3.1 | 2.9M | 704K | WebP via libwebp bindings |
| **ravif** | 0.13.0 | 21.2M | 6.0M | AVIF encoding (via rav1e) |
| **avif-serialize** | 0.8.8 | 21.5M | 6.0M | AVIF container writer |
| **jxl-oxide** | 0.12.5 | 1.1M | 617K | JPEG XL decode (pure Rust) |
| **oxipng** | 10.1.0 | 1.1M | 226K | PNG optimization (lossless compression) |
| **fast_image_resize** | 6.0.0 | 13.0M | 1.7M | SIMD-accelerated image resizing |

**Key insight:** The `image` crate is the universal entry point (107M downloads). For an AI workflow engine, it already handles 90% of format needs. AVIF and JPEG XL are the modern next-gen formats. `fast_image_resize` is critical for thumbnail generation.

---

## 5. Image Processing & Composition

| Crate | Version | Downloads | Recent (90d) | Role |
|-------|---------|-----------|--------------|------|
| **imageproc** | 0.26.1 | 7.6M | 1.8M | Image processing: blur, sharpen, contrast, edge detection, drawing, text overlay |
| **tiny-skia** | 0.12.0 | 22.7M | 5.7M | 2D rendering (Skia subset): paths, gradients, clipping, compositing |
| **skia-safe** | 0.93.1 | 2.3M | 372K | Full Skia bindings: everything (text, paths, effects, GPU) |
| **color-thief** | 0.2.2 | 460K | 49K | Extract dominant colors from images |
| **image-hasher** | 3.1.1 | 459K | 60K | Perceptual image hashing (similarity detection) |
| **kamadak-exif** | 0.6.1 | 8.4M | 1.5M | EXIF metadata read/write |
| **img-parts** | 0.4.0 | 9.1M | 342K | Low-level JPEG/PNG/RIFF container manipulation |

**Key insight:** For watermarking and overlays, `imageproc` does text drawing and compositing directly on `image` buffers. `tiny-skia` is the go-to for vector-quality rendering without GPU. Together they handle every image composition task an AI agent might need.

---

## 6. Animated Images (GIF, APNG, Animated WebP)

| Crate | Version | Downloads | Recent (90d) | Role |
|-------|---------|-----------|--------------|------|
| **gif** | 0.14.1 | 76.5M | 13.2M | GIF encode/decode with frame-level control |
| **gif-dispose** | 6.0.0 | 3.9M | 112K | Proper GIF frame compositing (disposal methods) |
| **apng** | 0.3.4 | 28K | 1.8K | APNG (animated PNG) encoder |
| **webp-animation** | 0.9.0 | 89K | 8.2K | Animated WebP encode/decode |
| **lottieconv** | 0.3.1 | 15K | 461 | Lottie -> WebP/GIF converter |
| **rlottie** | 0.5.4 | 25K | 1.1K | Lottie animation player/renderer |
| **lottie** | 0.1.0 | 2.1K | 113 | Lottie JSON format parser |

**Key insight:** GIF creation is mature (76M downloads). Animated WebP is the modern replacement. Lottie is important for AI-generated vector animations but the Rust ecosystem is still nascent there. For an AI agent generating animated content, `gif` + `webp-animation` cover the main use cases.

---

## 7. Charts, Plots & Data Visualization

| Crate | Version | Downloads | Recent (90d) | Role |
|-------|---------|-----------|--------------|------|
| **plotters** | 0.3.7 | 138.9M | 22.2M | The standard. Line/bar/scatter/histogram/heatmap -> PNG, SVG, WASM |
| **plotters-svg** | 0.3.7 | 134.7M | 22.0M | SVG backend for plotters |
| **plotters-bitmap** | 0.3.7 | 7.4M | 1.4M | Bitmap (PNG) backend for plotters |
| **charming** | 0.6.0 | 878K | 212K | ECharts-inspired visualization (rich chart types) |
| **poloto** | 19.1.2 | 593K | 56K | Simple 2D plotting -> SVG, CSS-styleable |
| **textplots** | 0.8.7 | 853K | 97K | Terminal/ASCII plotting |

**Key insight:** `plotters` at 139M downloads is the undisputed king. An AI agent could generate any chart type and output it as SVG or PNG. `charming` offers ECharts-style rich visualizations. For a workflow engine, `plotters` alone covers dashboards, reports, and data visualization.

---

## 8. SVG Processing & Vector Graphics

| Crate | Version | Downloads | Recent (90d) | Role |
|-------|---------|-----------|--------------|------|
| **usvg** | 0.47.0 | 12.1M | 3.8M | SVG simplifier/normalizer (complex SVG -> simple tree) |
| **resvg** | 0.47.0 | 10.9M | 3.1M | SVG -> raster renderer (uses tiny-skia) |
| **svg** | 0.18.0 | 5.0M | 649K | SVG composer and parser |
| **usvg-text-layout** | 0.38.0 | 2.2M | 264K | SVG text layout engine |
| **svg2pdf** | 0.13.0 | 779K | 247K | SVG -> PDF converter |
| **lyon** | 1.0.19 | 3.5M | 429K | Path tessellation (vector paths -> GPU triangles) |
| **kurbo** | 0.13.0 | 17.1M | 5.5M | 2D curve math (beziers, arcs, strokes) |
| **vello** | 0.7.0 | 210K | 67K | GPU-accelerated 2D renderer (next-gen) |
| **piet** | 0.8.0 | 998K | 273K | 2D graphics abstraction layer |

**Key insight:** `resvg` + `usvg` is the canonical SVG pipeline: parse, simplify, render to pixels. For an AI agent that generates SVG (diagrams, icons, charts), this pipeline converts to any raster format. `svg2pdf` closes the loop for document workflows.

---

## 9. PDF Generation & Manipulation

| Crate | Version | Downloads | Recent (90d) | Role |
|-------|---------|-----------|--------------|------|
| **lopdf** | 0.39.0 | 5.0M | 1.7M | PDF read/write/manipulation (the workhorse) |
| **printpdf** | 0.9.1 | 896K | 211K | PDF creation with text, images, vector graphics |
| **krilla** | 0.6.0 | 224K | 169K | High-level PDF creation (new, growing fast) |
| **typst** | 0.14.2 | 778K | 379K | Full typesetting system -> PDF (markup-based) |
| **typst-pdf** | 0.14.2 | 699K | 356K | Typst's PDF export backend |
| **svg2pdf** | 0.13.0 | 779K | 247K | SVG -> PDF |
| **pdf-extract** | 0.10.0 | 885K | 323K | Extract text/images from PDFs |
| **wkhtmltopdf** | 0.4.0 | 88K | 4.5K | HTML -> PDF via wkhtmltopdf |
| **fullbleed** | 0.6.11 | 114 | 114 | HTML/CSS -> PDF (new, pure Rust, designed for AI agents) |

**Key insight:** `lopdf` is the Swiss Army knife. `typst` is the most exciting option -- an AI agent could generate Typst markup and get beautiful PDFs. `krilla` is growing rapidly as a modern alternative. `fullbleed` is brand new but explicitly targets AI agent document workflows.

---

## 10. Audio Processing

| Crate | Version | Downloads | Recent (90d) | Role |
|-------|---------|-----------|--------------|------|
| **cpal** | 0.17.3 | 11.3M | 2.1M | Low-level cross-platform audio I/O |
| **hound** | 3.5.1 | 10.1M | 2.2M | WAV read/write |
| **rodio** | 0.22.2 | 6.9M | 1.2M | High-level audio playback |
| **ogg** | 0.9.2 | 6.4M | 1.0M | OGG container encode/decode |
| **symphonia** | 0.5.5 | 4.9M | 1.4M | Multi-format audio decoder (MP3, AAC, FLAC, WAV, OGG, etc.) |
| **opus** | 0.3.1 | 902K | 149K | Opus codec bindings |
| **whisper-rs** | 0.16.0 | 215K | 99K | Speech-to-text via whisper.cpp |

**Key insight:** AI agents generate audio (TTS) and consume it (STT). `hound` handles WAV (the interchange format). `symphonia` decodes everything. `whisper-rs` is the bridge to speech recognition. For a workflow engine, WAV/MP3/OGG/FLAC are the must-support formats.

---

## 11. Video Processing

| Crate | Version | Downloads | Recent (90d) | Role |
|-------|---------|-----------|--------------|------|
| **mp4** | 0.14.0 | 9.8M | 838K | MP4 container read/write |
| **gstreamer** | 0.25.1 | 7.1M | 982K | GStreamer bindings (full multimedia framework) |
| **rav1e** | 0.8.1 | 21.0M | 5.8M | AV1 video encoder (pure Rust) |
| **ffmpeg-next** | 8.1.0 | 2.1M | 1.2M | FFmpeg safe bindings (all codecs, all formats) |
| **video-rs** | 0.11.0 | 169K | 73K | High-level video read/write (over FFmpeg) |
| **matroska** | 0.30.0 | 145K | 14K | MKV container parser |

**Key insight:** Video is the heaviest media type. `ffmpeg-next` is the pragmatic choice (wraps FFmpeg). `video-rs` provides a friendlier API. For AI-generated video (emerging from models like Sora/Runway), MP4 with H.264/AV1 is the target. A workflow engine should shell out to FFmpeg for complex video tasks rather than linking it.

---

## 12. Screenshot & Screen Capture

| Crate | Version | Downloads | Recent (90d) | Role |
|-------|---------|-----------|--------------|------|
| **xcap** | 0.9.2 | 537K | 224K | Cross-platform screen capture (Linux/Mac/Win). Successor to `screenshots` |
| **display-info** | 0.5.8 | 349K | 81K | Monitor/display information (resolution, position) |
| **scrap** | 0.5.0 | 101K | 13K | Screen capture (older) |
| **captrs** | 0.3.1 | 22K | 1.2K | Cross-platform capture (smaller) |

**Key insight:** `xcap` is the clear winner and actively maintained. For AI agents that need visual context (UI testing, web scraping verification, visual QA), screenshot capture is essential. Pairs with `headless_chrome` for web page screenshots.

---

## 13. Headless Browser & HTML Rendering

| Crate | Version | Downloads | Recent (90d) | Role |
|-------|---------|-----------|--------------|------|
| **chromiumoxide** | 0.9.1 | 1.5M | 888K | Chrome DevTools Protocol (async, modern) |
| **headless_chrome** | 1.0.21 | 1.4M | 607K | Chrome automation (screenshots, PDF, DOM) |
| **fantoccini** | 0.22.1 | 2.9M | 213K | WebDriver automation |

**Key insight:** For AI agents generating HTML reports or needing web page screenshots, `chromiumoxide` or `headless_chrome` render HTML -> PNG/PDF. This is the path for rich document generation: AI generates HTML/CSS, headless browser renders to image/PDF.

---

## 14. OCR & Image Analysis

| Crate | Version | Downloads | Recent (90d) | Role |
|-------|---------|-----------|--------------|------|
| **rusty-tesseract** | 1.1.10 | 83K | 17K | Tesseract OCR wrapper |
| **ocrs** | 0.12.1 | 69K | 16K | Pure Rust OCR engine |
| **tesseract-rs** | 0.1.20 | 29K | 5K | Tesseract bindings |
| **image-hasher** | 3.1.1 | 459K | 60K | Perceptual image hashing (similarity/dedup) |

**Key insight:** OCR lets AI agents read text from images (receipts, screenshots, documents). `ocrs` is exciting as a pure-Rust OCR engine -- no Tesseract dependency. `image-hasher` enables deduplication and similarity search on generated images.

---

## 15. Document Formats (Office, Markdown, CSV)

| Crate | Version | Downloads | Recent (90d) | Role |
|-------|---------|-----------|--------------|------|
| **csv** | 1.4.0 | 160.6M | 23.4M | CSV read/write (the gold standard) |
| **pulldown-cmark** | 0.13.1 | 75.9M | 17.8M | Markdown parser (CommonMark) |
| **calamine** | 0.34.0 | 6.2M | 1.6M | Excel/ODS spreadsheet reader |
| **rust_xlsxwriter** | 0.94.0 | 1.6M | 443K | Excel .xlsx writer |
| **docx-rs** | 0.4.19 | 1.4M | 700K | Word .docx writer |
| **comrak** | 0.51.0 | 3.7M | 825K | GFM Markdown parser/renderer |

**Key insight:** AI agents constantly generate structured documents. CSV and Markdown are the most common AI outputs. Excel generation (`rust_xlsxwriter`) and Word generation (`docx-rs`) are critical for business workflows. All are mature and well-maintained.

---

## 16. Geospatial Data

| Crate | Version | Downloads | Recent (90d) | Role |
|-------|---------|-----------|--------------|------|
| **rstar** | 0.12.2 | 19.5M | 5.8M | R*-tree spatial index |
| **geo** | 0.32.0 | 14.4M | 2.5M | Geospatial primitives and algorithms |
| **geojson** | 1.0.0 | 7.5M | 1.3M | GeoJSON read/write |
| **shapefile** | 0.7.0 | 538K | 58K | Shapefile read/write |
| **proj** | 0.31.0 | 548K | 47K | Coordinate projections (PROJ bindings) |
| **gpx** | 0.10.0 | 228K | 31K | GPX track files |

**Key insight:** AI agents working with location data, maps, or spatial analysis need GeoJSON as the interchange format. The `geo` ecosystem is comprehensive and production-ready.

---

## 17. Procedural Generation & Computational Geometry

| Crate | Version | Downloads | Recent (90d) | Role |
|-------|---------|-----------|--------------|------|
| **noise** | 0.9.0 | 1.9M | 248K | Perlin/Simplex/Worley noise generation |
| **delaunator** | 1.0.2 | 306K | 30K | Delaunay triangulation |
| **voronoi** | 0.1.4 | 29K | 620 | Voronoi diagrams |

**Key insight:** Relevant for AI agents generating textures, terrain, or procedural art. `noise` is the foundation for texture synthesis.

---

## 18. Encoding, Hashing & MIME (Infrastructure)

| Crate | Version | Downloads | Recent (90d) | Role |
|-------|---------|-----------|--------------|------|
| **base64** | 0.22.1 | 999M | 164M | Base64 encode/decode (for data URIs) |
| **sha2** | 0.11.0 | 516M | 95M | SHA-256 hashing (for CAS storage) |
| **blake3** | 1.8.3 | 108M | 21M | BLAKE3 hashing (faster than SHA-256) |
| **uuid** | 1.22.0 | 480M | 79M | UUID generation |
| **mime** | 0.3.17 | 383M | 59M | MIME type handling |
| **mime_guess** | 2.0.5 | 173M | 27M | MIME type detection by extension |

**Key insight:** These are the plumbing crates every media pipeline needs. `blake3` is ideal for content-addressable storage (CAS) -- faster than SHA-256, cryptographically strong. `mime_guess` maps file extensions to MIME types automatically.

---

## 19. Serialization Formats (Data as Media)

| Crate | Version | Downloads | Recent (90d) | Role |
|-------|---------|-----------|--------------|------|
| **serde_json** | 1.0.149 | 783M | 129M | JSON |
| **toml** | 1.0.7 | 534M | 89M | TOML |
| **quick-xml** | 0.39.2 | 233M | 49M | XML |
| **yaml-rust2** | 0.11.0 | 30M | 8.2M | YAML |
| **prost** | 0.14.3 | 343M | 69M | Protocol Buffers |
| **flatbuffers** | 25.12.19 | 63M | 12.6M | FlatBuffers |

**Key insight:** These are "data media" -- AI agents produce structured data that needs serialization. Already in Nika's dependency tree via serde_json and yaml-rust2.

---

## 20. Compression & Archives

| Crate | Version | Downloads | Recent (90d) | Role |
|-------|---------|-----------|--------------|------|
| **flate2** | 1.1.9 | 414M | 69M | Deflate/gzip/zlib |
| **zstd** | 0.13.3 | 246M | 42M | Zstandard compression |
| **zip** | 8.2.0 | 150M | 33M | ZIP archive read/write |
| **tar** | 0.4.44 | 140M | 23M | TAR archive read/write |
| **brotli** | 8.0.2 | 158M | 30M | Brotli compression |
| **lz4** | 1.28.1 | 52M | 9M | LZ4 compression |
| **sevenz-rust** | 0.6.1 | 691K | 104K | 7z archive |

**Key insight:** AI agents producing multi-file outputs (project scaffolds, dataset exports, model bundles) need archive support. `zip` is the universal container. `zstd` is ideal for CAS blob compression.

---

## 21. ML Inference (AI-Specific Media Processing)

| Crate | Version | Downloads | Recent (90d) | Role |
|-------|---------|-----------|--------------|------|
| **ort** | 2.0.0-rc.12 | 7.3M | 2.7M | ONNX Runtime bindings (run any ML model) |
| **candle-core** | 0.9.2 | 3.3M | 1.7M | Minimalist ML framework (Hugging Face) |
| **candle-nn** | 0.9.2 | 2.1M | 1.1M | Neural network layers for candle |
| **burn** | 0.21.0 | 675K | 201K | Comprehensive deep learning framework |
| **ndarray** | 0.17.2 | 74.9M | 19.5M | N-dimensional arrays (NumPy equivalent) |
| **polars** | 0.53.0 | 9.2M | 2.0M | DataFrame library (Pandas equivalent) |

**Key insight:** For an AI workflow engine, `ort` (ONNX Runtime) lets you run any exported ML model locally. `candle` from Hugging Face is the emerging pure-Rust ML stack. These enable on-device inference for image classification, embeddings, etc.

---

## Complete Media Type Coverage Matrix

This is every media type a Rust workflow engine can handle today:

```
IMAGES (raster)
  PNG ............... png (112M)              encode + decode
  JPEG .............. jpeg-decoder (68M)      decode; image crate for encode
  GIF ............... gif (77M)               encode + decode + animation
  WebP .............. image-webp (33M)        encode + decode
  AVIF .............. ravif (21M)             encode; image crate for decode
  TIFF .............. tiff (65M)              encode + decode
  BMP ............... image (107M)            encode + decode
  ICO ............... image (107M)            encode + decode
  JPEG XL ........... jxl-oxide (1.1M)        decode
  DDS ............... ddsfile (540K)          game textures
  OpenEXR ........... openexr (23K)           HDR imaging

IMAGES (vector)
  SVG ............... svg (5M) / resvg (11M)  compose + render to raster
  PDF (vector) ...... lopdf (5M)              read + write + manipulate

ANIMATION
  Animated GIF ...... gif (77M)               frame-by-frame encode
  Animated WebP ..... webp-animation (89K)    encode + decode
  APNG .............. apng (28K)              encode
  Lottie/JSON ....... rlottie (25K)           play + render

3D MODELS
  glTF/GLB .......... gltf (6M)               load + parse
  STL ............... stl_io (2.5M)           read + write
  OBJ ............... obj-rs (703K)           parse
  Multi-format ...... mesh-loader (108K)      OBJ + glTF + STL

AUDIO
  WAV ............... hound (10M)             encode + decode
  MP3 ............... symphonia (4.9M)        decode
  OGG/Vorbis ........ ogg (6.4M)             encode + decode
  FLAC .............. symphonia (4.9M)        decode
  Opus .............. opus (902K)             encode + decode
  AAC ............... symphonia (4.9M)        decode

VIDEO
  MP4/H.264 ......... mp4 (9.8M)             container read/write
  AV1 ............... rav1e (21M)             encode
  MKV ............... matroska (145K)         parse
  Any format ........ ffmpeg-next (2.1M)      transcode everything

DOCUMENTS
  PDF ............... lopdf (5M)              full manipulation
  DOCX .............. docx-rs (1.4M)          write
  XLSX .............. rust_xlsxwriter (1.6M)  write
  CSV ............... csv (161M)              read + write
  Markdown .......... pulldown-cmark (76M)    parse + render

CODES
  QR Code ........... qrcode (11.4M)          encode
  QR Decode ......... rqrr (3M)              decode
  Barcode (1D) ...... barcoders (283K)        encode (EAN, UPC, Code128...)
  Multi-code ........ rxing (281K)            encode + decode (all types)

FONTS
  TTF/OTF parse ..... ttf-parser (57M)        parse
  Glyph render ...... ab_glyph (25M)          rasterize
  Text layout ....... cosmic-text (3.8M)      multi-line layout
  System fonts ...... font-kit (10M)          cross-platform loading

GEOSPATIAL
  GeoJSON ........... geojson (7.5M)          read + write
  Shapefile ......... shapefile (538K)        read + write
  GPX ............... gpx (228K)              read + write
  KML ............... kml (238K)              read + write

DATA INTERCHANGE
  JSON .............. serde_json (783M)       read + write
  YAML .............. yaml-rust2 (30M)        read + write
  TOML .............. toml (534M)             read + write
  XML ............... quick-xml (233M)        read + write
  Protobuf .......... prost (343M)            encode + decode
  FlatBuffers ....... flatbuffers (63M)       encode + decode

ARCHIVES
  ZIP ............... zip (150M)              read + write
  TAR ............... tar (140M)              read + write
  GZIP .............. flate2 (414M)           compress + decompress
  ZSTD .............. zstd (246M)             compress + decompress
  7Z ................ sevenz-rust (691K)      read + write
  Brotli ............ brotli (158M)           compress + decompress

CHARTS / VISUALIZATION
  Any chart ......... plotters (139M)         line/bar/scatter/histogram -> PNG/SVG
  ECharts-style ..... charming (878K)         rich interactive charts
  Terminal plots .... textplots (853K)        ASCII art charts
  Simple SVG ........ poloto (593K)           lightweight 2D plots

SCREENSHOTS
  Screen capture .... xcap (537K)             cross-platform
  Browser render .... chromiumoxide (1.5M)    HTML -> screenshot/PDF

OCR
  Text extraction ... ocrs (69K)              pure Rust OCR
  Tesseract ......... rusty-tesseract (83K)   Tesseract wrapper

PROCEDURAL
  Noise/textures .... noise (1.9M)            Perlin/Simplex/Worley
  Triangulation ..... delaunator (306K)       Delaunay
  Voronoi ........... voronoi (29K)           Voronoi diagrams
```

---

## Recommendations for Nika Media Pipeline

### Tier 1 -- Already Essential (should be in dependency tree)
- `image` (universal codec hub)
- `qrcode` + `rqrr` (QR Code AI core business)
- `blake3` or `sha2` (CAS hashing)
- `mime_guess` (auto MIME detection)
- `base64` (data URI encoding)

### Tier 2 -- High Value for AI Workflows
- `plotters` (chart generation from data)
- `resvg` + `usvg` (SVG rendering)
- `lopdf` or `krilla` (PDF output)
- `hound` (WAV for TTS output)
- `imageproc` (watermarks, overlays, composition)
- `ab_glyph` + `ttf-parser` (text-on-image rendering)

### Tier 3 -- Specialized but Valuable
- `gltf` (3D model output, emerging from AI)
- `gif` + `webp-animation` (animated output)
- `xcap` (screenshot for visual QA)
- `rxing` (universal barcode read/write)
- `rust_xlsxwriter` + `docx-rs` (office document generation)
- `csv` (data export)

### Tier 4 -- Future / Niche
- `ffmpeg-next` or `video-rs` (AI video generation)
- `ort` / `candle-core` (on-device ML inference)
- `ocrs` (OCR without external deps)
- `typst` (beautiful document typesetting)
- `noise` (procedural texture generation)
- `geo` + `geojson` (spatial data workflows)

---

## Sources

All data from crates.io API (`https://crates.io/api/v1/crates/<name>`), queried 2026-03-18.
Download counts are lifetime totals; "recent" is last 90 days.

## Confidence Level

**High** -- All data comes directly from crates.io. Download counts are authoritative. Capability descriptions are verified against crate documentation. The only uncertainty is around very new crates (fullbleed, krilla) whose APIs may still be unstable.
