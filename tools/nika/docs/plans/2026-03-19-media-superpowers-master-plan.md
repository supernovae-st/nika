# PR3: Media Superpowers — Master Plan v2.0

> **Version:** v2.0 (consolidated from 36 research agents + 7 verification agents)
> **Date:** 2026-03-19
> **Branch:** `feat/media-superpowers`
> **Baseline:** After PR2 merged (v0.32.0)
> **Target:** v0.33.0 (PR3a) + v0.34.0 (PR3b)
> **Research:** 36 agents, 300+ crates, 21 research files, 292 planned tests
> **Verification:** security audit (24 findings), perf analysis, async review, code review

**Parent:** [Master Plan](./2026-03-18-media-pipeline-master-plan.md) | **Prev:** [PR2](./2026-03-18-media-pipeline-pr2-artifacts-v6.md)

---

## Table of Contents

1. [Vision](#vision)
2. [Architecture](#architecture)
3. [Naming Convention](#naming)
4. [Security Framework](#security)
5. [Async & Performance](#async-perf)
6. [Dependency Strategy](#deps)
7. [PR3a: Essentials (v0.33.0)](#pr3a)
8. [PR3b: Superpowers (v0.34.0)](#pr3b)
9. [Test Strategy](#tests)
10. [Benchmark Plan](#benchmarks)
11. [Watch List](#watch)
12. [Research Index](#research)

---

## 1. Vision <a id="vision"></a>

**"Nika is the only workflow engine where AI agents can see, read, hear, create, optimize, sign, and verify every media type — all in pure Rust, all in a single YAML file."**

### What changes

| Before (v0.32) | After (v0.34) |
|-----------------|---------------|
| Agents generate media, Nika stores it | Agents generate, transform, analyze, optimize, and sign media |
| `invoke:` calls external MCP tools | `invoke: nika:thumbnail` runs natively, no MCP needed |
| CAS stores raw bytes | CAS stores bytes + auto-enriched metadata (dimensions, thumbhash) |
| No media intelligence | OCR, QR validation, perceptual hashing, dominant colors |
| No provenance | C2PA content credentials on every artifact |

### Competitive position

```
Capability                     Nika     n8n    Dagger  Temporal  Prefect  Argo
──────────────────────────────────────────────────────────────────────────────
SIMD thumbnails                YES      No*    No      No        No       No
Universal metadata extract     YES      No     No      No        No       No
OCR (pure Rust + WASM)         YES      No     No      No        No       No
Chart generation (JSON→PNG)    YES      No     No      No        No       No
QR validation (89% artistic)   YES      No     No      No        No       No
C2PA AI provenance             YES      No     No      No        No       No
SVG rendering                  YES      No     No      No        No       No
PDF text extraction            YES      No     No      No        No       No
Image optimization             YES      No*    No      No        No       No
Perceptual dedup               YES      No     No      No        No       No
ThumbHash placeholders         YES      No     No      No        No       No
Pipeline chaining (in-memory)  YES      No     No      No        No       No
Feature-gated (lean core)      YES      N/A    N/A     N/A       N/A      N/A

* n8n wraps GraphicsMagick (C dep), not native
```

---

## 2. Architecture <a id="architecture"></a>

### Validated Design (rust-architect agent)

```
Workflow YAML
    │
    invoke: nika:thumbnail
    │
    ▼
BuiltinToolRouter.dispatch()
    │  extract_name("nika:thumbnail") → "thumbnail"
    │  tools.get("thumbnail") → Arc<dyn BuiltinTool>
    ▼
MediaToolAdapter (implements BuiltinTool)
    │  1. Parse JSON args
    │  2. Delegate to MediaOp::execute(args, ctx)
    ▼
ThumbnailOp (implements MediaOp)
    │  ctx.read_media(hash)           ← CAS read (async)
    │  compute.compute(decode+resize)  ← ComputePool (rayon, 4 threads)
    │  return MediaOpResult::Binary { data, mime, metadata }
    ▼
MediaToolAdapter (continued)
    │  3. ctx.store_media(data)        ← CAS write + budget check
    │  4. Serialize MediaRef + metadata → JSON String
    ▼
Result<String, NikaError>              → back through router
```

### Key Types

```rust
/// Internal trait for media operations.
/// Each tool implements this. The MediaToolAdapter bridges to BuiltinTool.
pub(crate) trait MediaOp: Send + Sync {
    fn name(&self) -> &'static str;
    fn description(&self) -> &'static str;
    fn parameters_schema(&self) -> serde_json::Value;
    fn execute<'a>(
        &'a self,
        args: serde_json::Value,
        ctx: &'a MediaToolContext,
    ) -> Pin<Box<dyn Future<Output = Result<MediaOpResult, NikaError>> + Send + 'a>>;
}

/// Result of a media operation.
pub(crate) enum MediaOpResult {
    Metadata(Value),                              // dimensions, phash, etc.
    Binary { data: Vec<u8>, mime_type: String,     // thumbnail, optimize, etc.
             extension: String, metadata: Value },
    MultiBinary(Vec<BinaryOutput>),               // pipeline, compare
}

/// Pipeline-capable media operations.
pub(crate) trait PipelineCapable: MediaOp {
    fn transform(&self, data: &[u8], mime: &str, params: &Value)
        -> Result<PipelineBuffer, NikaError>;
}
```

### Shared Context (per workflow run)

```rust
pub struct MediaToolContext {
    pub cas: CasStore,
    pub budget: Arc<MediaBudget>,                  // 500MB CAS cumulative
    pub compute: Arc<ComputePool>,                 // 4 rayon threads, isolated from tokio
    pub decode_semaphore: Arc<Semaphore>,           // max 4 concurrent decodes
    pub working_memory: Arc<WorkingMemoryBudget>,   // 512MB transient buffers
    pub cancel: CancellationToken,                  // workflow cancellation
    // Lazy-loaded expensive resources
    ocr_engine: OnceLock<Arc<OcrEngine>>,           // ~20MB, loaded on first nika:ocr
    fontdb: OnceLock<Arc<fontdb::Database>>,         // loaded on first nika:svg_render
}
```

### Directory Structure

```
src/runtime/builtin/media/
├── mod.rs              MediaOp trait + MediaToolAdapter + create_media_tool_adapters()
├── context.rs          MediaToolContext + ComputePool + WorkingMemoryBudget
├── safety.rs           decode_image_safe() + sanitize_svg() + extract_pdf_safe()
├── pipeline.rs         PipelineOp + PipelineCapable trait
├── error.rs            NIKA-290..299 media tool errors
├── thumbnail.rs        nika:thumbnail (fast_image_resize + image)
├── metadata.rs         nika:metadata (nom-exif + lofty + mp4)
├── optimize.rs         nika:optimize (oxipng)
├── svg.rs              nika:svg_render (resvg + usvg + tiny-skia)
├── pdf.rs              nika:pdf_extract (pdf-extract + lopdf)
├── ocr.rs              nika:ocr (ocrs + rten)
├── chart.rs            nika:chart (charts-rs)
├── qr.rs               nika:qr_validate (qrcode-ai-scanner-core)
├── thumbhash_tool.rs   nika:thumbhash (thumbhash)
├── color.rs            nika:dominant_color (color-thief)
├── dimensions.rs       nika:dimensions (imagesize)
├── phash.rs            nika:phash (image_hasher)
├── provenance.rs       nika:provenance (c2pa)
├── convert.rs          nika:convert (image)
├── strip.rs            nika:strip (image re-encode)
├── compare.rs          nika:compare (image_hasher + pixel diff)
└── tests/
    ├── fixtures.rs     In-memory test fixtures (PNG, JPEG, SVG, PDF, MP3, etc.)
    ├── security.rs     Fuzz + decompression bomb + SVG XSS + PDF JS tests
    ├── integration.rs  Cross-tool chains
    └── e2e.rs          Full YAML workflow tests
```

---

## 3. Naming Convention <a id="naming"></a>

### Decision: Flat verbs (validated by naming research agent)

```
nika:thumbnail      nika:metadata       nika:optimize
nika:ocr            nika:chart          nika:qr_validate
nika:svg_render     nika:pdf_extract    nika:thumbhash
nika:dominant_color nika:dimensions     nika:phash
nika:provenance     nika:convert        nika:strip
nika:compare        nika:pipeline
```

**Justification:**
- Consistent with existing 12 tools (`nika:sleep`, `nika:read`, `nika:edit`)
- Zero router change (extract_name strips `nika:`, looks up key)
- ffmpeg, ImageMagick 7, n8n all use flat verbs
- Rig adapter: `nika_thumbnail` (underscore, LLM API compatible)

**Workflow usage:**
```yaml
tasks:
  thumb:
    invoke:
      tool: nika:thumbnail
      input:
        hash: "{{with.img.media[0].hash}}"
        width: 256
```

---

## 4. Security Framework <a id="security"></a>

### MUST-HAVE before any media tool ships (from security audit)

#### S1: `decode_image_safe()` — Shared safe decoder

**Every tool that decodes images MUST use this function. Never call `image::load_from_memory()` directly.**

```rust
// src/runtime/builtin/media/safety.rs

use image::io::Reader as ImageReader;
use image::io::Limits;
use std::io::Cursor;

const MAX_DECODED_BYTES: u64 = 256 * 1024 * 1024;  // 256 MB
const MAX_IMAGE_DIM: u32 = 10_000;                   // 10000x10000 max

pub fn decode_image_safe(data: &[u8]) -> Result<image::DynamicImage, NikaError> {
    let reader = ImageReader::new(Cursor::new(data))
        .with_guessed_format()
        .map_err(|e| tool_error("decode", format!("Format guess failed: {e}")))?;

    let mut limits = Limits::default();
    limits.max_alloc = Some(MAX_DECODED_BYTES);
    limits.max_image_width = Some(MAX_IMAGE_DIM);
    limits.max_image_height = Some(MAX_IMAGE_DIM);

    reader.with_limits(limits)
        .decode()
        .map_err(|e| tool_error("decode", format!("[NIKA-290] Decode failed: {e}")))
}
```

#### S2: `sanitize_svg()` — Strip dangerous elements before storage

```rust
pub fn sanitize_svg(input: &str) -> Result<&str, NikaError> {
    let lower = input.to_ascii_lowercase();
    for pattern in ["<script", "<foreignobject", "javascript:"] {
        if lower.contains(pattern) {
            return Err(tool_error("svg", format!("[NIKA-291] SVG contains forbidden element: {pattern}")));
        }
    }
    // Event handlers: onload=, onclick=, onerror=
    if regex::Regex::new(r#"\bon\w+\s*="#).unwrap().is_match(&lower) {
        return Err(tool_error("svg", "[NIKA-291] SVG contains event handler"));
    }
    Ok(input)
}
```

#### S3: `extract_pdf_safe()` — Stack-limited PDF extraction

```rust
pub fn extract_pdf_safe(data: &[u8]) -> Result<String, NikaError> {
    let data = data.to_vec();
    let handle = std::thread::Builder::new()
        .stack_size(4 * 1024 * 1024)  // 4 MB stack limit
        .name("pdf-extract".into())
        .spawn(move || pdf_extract::extract_text_from_mem(&data))
        .map_err(|e| tool_error("pdf", format!("Thread spawn: {e}")))?;

    match handle.join() {
        Ok(Ok(text)) => Ok(text),
        Ok(Err(e)) => Err(tool_error("pdf", format!("Extraction: {e}"))),
        Err(_) => Err(tool_error("pdf", "[NIKA-290] PDF panicked (recursive references?)")),
    }
}
```

#### S4: Per-operation timeout (every tool)

```rust
const DEFAULT_TIMEOUT: Duration = Duration::from_secs(30);
const MAX_TIMEOUT: Duration = Duration::from_secs(120);

// Wrapping pattern in MediaToolAdapter::call():
tokio::time::timeout(DEFAULT_TIMEOUT, self.op.execute(parsed, &self.ctx))
    .await
    .map_err(|_| tool_error(self.op.name(), "[NIKA-293] Operation timed out"))?
```

#### S5: Additional fixes found by audit

| Fix | Code | Risk |
|-----|------|------|
| `MediaBudget::rollback` underflow | `saturating_sub` via `fetch_update` | Bug (DONE) |
| `NIKA_MEDIA_STORE` env injection | Block in exec: security vars | Medium |
| SVG case-insensitive detection | `to_ascii_lowercase()` in `has_svg_element` | Low |
| Post-write symlink check | `canonicalize()` after artifact write | Medium |

### Error Codes: NIKA-290..299

| Code | Variant | Usage |
|------|---------|-------|
| NIKA-290 | `MediaToolError` | Generic tool execution failure |
| NIKA-291 | `MediaToolUnsupportedFormat` | Wrong format for tool (OCR on audio) |
| NIKA-292 | `MediaToolDependencyMissing` | Feature disabled, hint to enable |
| NIKA-293 | `MediaToolTimeout` | Operation exceeded timeout |
| NIKA-294 | `MediaToolInvalidArgs` | Tool-specific param validation |
| NIKA-295 | `MediaPipelineStepFailed` | Pipeline step N failed |
| NIKA-296 | `MediaPipelineEmpty` | No steps in pipeline |
| NIKA-297 | `MediaSecurityViolation` | SVG XSS, decompression bomb, etc. |
| NIKA-298-299 | Reserved | Future media tool errors |

---

## 5. Async & Performance <a id="async-perf"></a>

### ComputePool (validated by rust-async-expert)

Dedicated rayon ThreadPool (4 threads) isolated from tokio. Prevents tokio-rayon conflicts when oxipng/fast_image_resize use rayon internally.

```rust
pub struct ComputePool {
    pool: rayon::ThreadPool,
}

impl ComputePool {
    pub fn new() -> Self {
        Self {
            pool: rayon::ThreadPoolBuilder::new()
                .num_threads(num_cpus::get().min(4))
                .thread_name(|idx| format!("nika-media-{idx}"))
                .build()
                .expect("failed to create media compute pool"),
        }
    }

    pub async fn compute<F, T>(&self, f: F) -> Result<T, NikaError>
    where F: FnOnce() -> T + Send + 'static, T: Send + 'static {
        let (tx, rx) = tokio::sync::oneshot::channel();
        self.pool.spawn(move || { let _ = tx.send(f()); });
        rx.await.map_err(|_| tool_error("compute", "Task panicked"))
    }
}
```

### Pipeline pattern: 1 CAS read → N in-memory → 1 CAS write

```rust
// All sync steps in ONE compute() call with cancellation checkpoints
let result = ctx.compute.compute(move || -> Result<Vec<u8>, NikaError> {
    let mut data = input_data;
    for (i, step) in steps.iter().enumerate() {
        if cancel.is_cancelled() {
            return Err(tool_error("pipeline", format!("Cancelled at step {i}")));
        }
        data = step.transform(&data)?;
    }
    Ok(data)
}).await?;
```

### Performance baselines (from rust-perf agent)

| Operation | Typical | Timeout |
|-----------|---------|---------|
| `nika:thumbnail` (2000x1500 → 512x384) | 130ms | 5s |
| `nika:optimize` (500KB PNG, level 2) | 300ms | 30s |
| `nika:ocr` (warm, small image) | 250ms | 10s |
| `nika:svg_render` (1024x768) | 15ms | 5s |
| `nika:dimensions` (header read) | <1ms | 1s |
| `nika:thumbhash` | ~5ms | 1s |
| `nika:dominant_color` | ~50ms | 5s |
| Full pipeline (3 steps) | ~255ms | 30s |

**Key insight:** Decode is always the bottleneck (150-300ms for JPEG). Resize (10ms) and encode (10ms) are negligible.

---

## 6. Dependency Strategy <a id="deps"></a>

### Tier 1 — Always On (zero/tiny deps)

```toml
imagesize = "0.13"           # 0 deps, header-only dimensions
thumbhash = "0.1"            # 0 deps, 25-byte placeholder
```

### Tier 2 — Default Features (pure Rust, moderate deps)

```toml
[features]
default = ["media-core"]
media-core = ["media-thumbnail", "media-metadata", "media-optimize", "media-svg"]

[dependencies]
fast_image_resize = { version = "6.0", features = ["image"], optional = true }
image = { version = "0.25", optional = true, default-features = false,
          features = ["png", "jpeg", "webp", "gif"] }
nom-exif = { version = "2.7", optional = true }
lofty = { version = "0.23", optional = true }
mp4 = { version = "0.14", optional = true }
oxipng = { version = "10.1", optional = true, default-features = false,
           features = ["parallel"] }
resvg = { version = "0.47", optional = true }
usvg = { version = "0.47", optional = true }
tiny-skia = { version = "0.12", optional = true }
color-thief = { version = "0.2", optional = true }
```

### Tier 3 — Opt-In (larger deps)

```toml
[dependencies]
ocrs = { version = "0.12", optional = true }
rten = { version = "0.24", optional = true }
charts-rs = { version = "0.3", optional = true }
qrcode-ai-scanner-core = { version = "0.2", optional = true }
c2pa = { version = "0.78", optional = true, features = ["rust_native_crypto"] }
image_hasher = { version = "3.1", optional = true }
symphonia = { version = "0.5", optional = true }
pdf-extract = { version = "0.10", optional = true }
lopdf = { version = "0.39", optional = true }
zstd = { version = "0.13", optional = true, default-features = false }
```

---

## 7. PR3a: Essentials (v0.33.0) <a id="pr3a"></a>

### 14 commits, 5 phases

#### Phase A: Infrastructure (C1-C3)

| # | Commit | Files | LOC |
|---|--------|-------|-----|
| C1 | `feat(media): media tool dispatcher + MediaOp trait` | `builtin/media/{mod,context,safety,error}.rs` | ~300 |
| C2 | `feat(media): ComputePool + WorkingMemoryBudget` | `builtin/media/context.rs` | ~150 |
| C3 | `feat(media): wire media tools into BuiltinToolRouter` | `builtin/router.rs`, `executor/verbs.rs` | ~50 |

#### Phase B: Tier 1 Always-On (C4-C6)

| # | Commit | Tool | Crate | LOC |
|---|--------|------|-------|-----|
| C4 | `feat(media): nika:dimensions` | Image dimensions from headers | `imagesize` (0 deps) | ~40 |
| C5 | `feat(media): nika:thumbhash` | 25-byte image placeholder | `thumbhash` (0 deps) | ~60 |
| C6 | `feat(media): nika:dominant_color` | Color palette extraction | `color-thief` | ~50 |

#### Phase C: Tier 2 Default Features (C7-C12)

| # | Commit | Tool | Crate(s) | LOC |
|---|--------|------|----------|-----|
| C7 | `feat(media): nika:thumbnail — SIMD resize` | Thumbnails | `fast_image_resize` + `image` | ~120 |
| C8 | `feat(media): nika:metadata — universal extract` | EXIF/audio/video | `nom-exif` + `lofty` + `mp4` | ~200 |
| C9 | `feat(media): nika:optimize — lossless PNG` | PNG optimization | `oxipng` | ~80 |
| C10 | `feat(media): nika:svg_render — SVG to PNG` | SVG rasterization | `resvg` + `usvg` + `tiny-skia` | ~100 |
| C11 | `feat(media): nika:convert — format conversion` | PNG↔JPEG↔WebP | `image` | ~80 |
| C12 | `feat(media): nika:strip — remove metadata` | Privacy strip | `image` (re-encode) | ~60 |

#### Phase D: Auto-Enrichment (C13)

| # | Commit | What | LOC |
|---|--------|------|-----|
| C13 | `feat(media): auto-enrich MediaRef with dimensions + thumbhash` | On CAS store, auto-extract for images | ~80 |

MediaRef gains a `metadata` field:
```rust
pub struct MediaRef {
    // existing fields...
    #[serde(default, skip_serializing_if = "serde_json::Map::is_empty")]
    pub metadata: serde_json::Map<String, Value>,
}
```

Template access: `{{with.img.media[0].metadata.width}}`, `{{with.img.media[0].metadata.thumbhash}}`

#### Phase E: Tests + Polish (C14)

| # | Commit | What | Tests |
|---|--------|------|-------|
| C14 | `test(media): tool tests + integration + security` | All Phase A-D tools | ~150 |

---

## 8. PR3b: Superpowers (v0.34.0) <a id="pr3b"></a>

### 12 commits

| # | Commit | Tool | Crate | Feature |
|---|--------|------|-------|---------|
| C1 | `feat(media): nika:ocr — pure Rust OCR` | Text from images | `ocrs` + `rten` | `media-ocr` |
| C2 | `feat(media): nika:chart — JSON to charts` | JSON → PNG/SVG | `charts-rs` | `media-chart` |
| C3 | `feat(media): nika:qr_validate — QR scoring` | 4-tier decode, 0-100 score | `qrcode-ai-scanner-core` | `media-qr` |
| C4 | `feat(media): nika:phash — perceptual similarity` | Near-duplicate detection | `image_hasher` | `media-phash` |
| C5 | `feat(media): nika:compare — visual diff` | 2 images → similarity | `image_hasher` + pixel diff | `media-phash` |
| C6 | `feat(media): nika:provenance — C2PA signing` | Content credentials | `c2pa` | `media-provenance` |
| C7 | `feat(media): nika:pdf_extract — PDF to text` | Text extraction | `pdf-extract` + `lopdf` | `media-pdf` |
| C8 | `feat(media): nika:pipeline — chain operations` | In-memory chaining | (orchestrator) | always on |
| C9 | `feat(media): CAS compression for non-media blobs` | zstd compression | `zstd` | `media-compression` |
| C10 | `feat(cli): nika media tools — list capabilities` | CLI discovery | — | — |
| C11 | `test(media): PR3b tests + E2E workflows + fuzz` | All PR3b tools | — | ~142 tests |
| C12 | `chore: bump version to v0.34.0` | — | — | — |

### Pipeline Tool Design (C8)

```yaml
tasks:
  process:
    invoke:
      tool: nika:pipeline
      input:
        hash: "{{with.img.media[0].hash}}"
        steps:
          - op: thumbnail
            width: 512
          - op: optimize
            level: 4
            strip: true
          - op: convert
            format: webp
```

1 CAS read → 3 in-memory transforms → 1 CAS write. Budget charged once.

---

## 9. Test Strategy <a id="tests"></a>

### 292 test functions (validated by rust-pro agent)

| Category | Count |
|----------|-------|
| Per-tool happy path + error + edge | 255 |
| Cross-tool integration | 9 |
| CAS integrity | 9 |
| Telemetry/events | 5 |
| Feature flags | 4 |
| Fuzz (6 targets x 100 iterations) | 6 |
| E2E workflows | 4 |
| **Total** | **292** |

### Security-critical tests (MUST pass before merge)

| Test | Risk | What |
|------|------|------|
| `svg_render_script_tag_stripped` | CRITICAL | SVG `<script>` rejected |
| `svg_render_xxe_entity_blocked` | CRITICAL | SVG XXE entity rejected |
| `svg_render_xlink_href_external` | CRITICAL | SVG SSRF blocked |
| `svg_render_foreignObject_blocked` | CRITICAL | SVG HTML injection blocked |
| `pdf_extract_javascript_ignored` | CRITICAL | PDF JS not executed |
| `thumbnail_decompression_bomb` | CRITICAL | 65535x65535 PNG rejected |
| `pipeline_recursive_pipeline` | HIGH | Infinite loop prevented |
| `provenance_no_private_key_leak` | HIGH | Keys never in output |
| All `fuzz_*` tests | HIGH | No panics on random input |

### In-memory test fixtures (no binary files in git)

```rust
fixture_png_1x1()              // 67 bytes, valid PNG
fixture_jpeg_1x1()             // Valid JPEG
fixture_svg_100x100()          // Valid SVG with rect
fixture_svg_xss()              // SVG with <script> (security)
fixture_svg_xxe()              // SVG with entity expansion (security)
fixture_pdf_minimal()          // Valid PDF
fixture_pdf_with_js()          // PDF with JavaScript (security)
fixture_mp3_id3()              // MP3 with ID3 tags
fixture_mp4_ftyp()             // MP4 container
fixture_empty()                // Zero bytes
fixture_png_corrupt_truncated()// Truncated PNG
```

---

## 10. Benchmark Plan <a id="benchmarks"></a>

File: `benches/media_pipeline.rs` (criterion)

| Benchmark | Baseline | Alert |
|-----------|----------|-------|
| blake3 hash (1MB) | <1ms | >3ms |
| CAS store (1MB) | <5ms | >15ms |
| JPEG decode (2000x1500) | <100ms | >300ms |
| Resize Lanczos3 (2000→512) | <10ms | >30ms |
| JPEG encode (512x384) | <10ms | >30ms |
| oxipng level 2 (500KB) | <300ms | >900ms |
| SVG render (1024x768) | <15ms | >45ms |
| Full thumbnail pipeline | <130ms | >400ms |

---

## 11. Watch List <a id="watch"></a>

| Crate | What | Why wait |
|-------|------|----------|
| **OxiMedia** (106 crates) | "FFmpeg of Rust" | 1 author, 5 weeks old, red flag maturity |
| **lele** | ONNX AOT compiler, 1.93x ORT | Only 4 models supported |
| **jxl-rs** 0.4 | Official JPEG XL decoder (in Chromium!) | JXL adoption still low |
| **Extism** (5.5K stars) | WASM plugin system | Major architecture change for v0.35+ |
| **kornia-rs** (608 stars) | Pure Rust CV, 3-5x faster | Needs evaluation vs image-rs |
| **burn-vision** | GPU image preprocessing | Pre-release, too early |
| **jpegli** 0.1 | Google JPEG, 35% better than MozJPEG | Too new |
| **quantette** | Color quantization, no GPL | Watch vs imagequant |
| **ultrahdr-core** 0.2 | Ultra HDR gain map | 159 downloads |
| **trustmark** | Invisible neural watermark | Requires ONNX Runtime (~20MB) |

---

## 12. Research Index <a id="research"></a>

All files in `docs/research/2026-03-18-*`:

| File | Crates | Domain |
|------|--------|--------|
| `media-pipeline-crate-survey.md` | 50+ | Comprehensive survey |
| `rust-image-crates-survey.md` | 30+ | image-rs ecosystem |
| `rust-audio-crates-ecosystem.md` | 20+ | symphonia, lofty |
| `rust-video-crates-landscape.md` | 15+ | mp4, ffmpeg-sidecar |
| `rust-document-processing-crates.md` | 25+ | pdf_oxide, krilla |
| `media-metadata-crates.md` | 20+ | nom-exif, moxcms |
| `rust-media-optimization-crates.md` | 40+ | oxipng, mozjpeg |
| `rust-wasm-media-crates.md` | 10+ | Extism, WASM SIMD |
| `bleeding-edge-rust-media-crates.md` | 20+ | OxiMedia, lele |
| `cas-rust-crates-research.md` | 15+ | blake3, cacache |
| `special-media-crates-survey.md` | 130+ | ocrs, xcap, plotters |
| `steganography-watermark-c2pa-crates.md` | 13+ | c2pa, trustmark |
| `compression-crates-for-cas.md` | 10+ | zstd, lz4 |
| `rust-color-rendering-crates.md` | 15+ | tiny-skia, palette |
| `rust-data-visualization-crates.md` | 10+ | charts-rs, plotters |
| `animated-media-rust-crates.md` | 15+ | gif, webp_animation |
| `rust-ml-inference-crates.md` | 20+ | ort, candle, lele |
| `tiny-media-crates.md` | 15+ | thumbhash, qoi |
| `bleeding-edge-rust-media-projects.md` | 6 | OxiMedia, Extism, kornia |
| `crate-api-reference.md` | 7 | c2pa, symphonia, ocrs APIs |
| `media-crate-api-reference.md` | 7 | fast_image_resize, resvg APIs |
| `rust-media-crate-apis.md` | 7 | color-thief, calamine APIs |

---

## Appendix: Key Scenarios

### Scenario 1: AI Designer Autonome

```yaml
tasks:
  generate:
    invoke: { tool: dalle:generate, input: { prompt: "butterfly logo" } }
  colors:
    depends_on: [generate]
    invoke: { tool: nika:dominant_color,
              input: { hash: "{{with.generate.media[0].hash}}" } }
  thumb:
    depends_on: [generate]
    invoke: { tool: nika:pipeline, input: {
      hash: "{{with.generate.media[0].hash}}",
      steps: [
        { op: thumbnail, width: 256 },
        { op: optimize, level: 4 },
      ]
    }}
    artifact: { path: "thumbs/logo.png", format: binary }
  sign:
    depends_on: [generate]
    invoke: { tool: nika:provenance,
              input: { hash: "{{with.generate.media[0].hash}}" } }
```

### Scenario 2: QA Automatisee QR Code AI

```yaml
tasks:
  generate:
    invoke: { tool: qr_gen, input: { url: "https://qrcode-ai.com", style: "artistic" } }
  validate:
    depends_on: [generate]
    invoke: { tool: nika:qr_validate,
              input: { hash: "{{with.generate.media[0].hash}}" } }
  decide:
    depends_on: [validate]
    agent:
      model: claude
      prompt: "Score: {{with.validate.score}}. Si < 70, dit 'regenerate'. Sinon 'approve'."
```

### Scenario 3: Document Analysis

```yaml
tasks:
  extract:
    invoke: { tool: nika:pdf_extract,
              input: { hash: "{{with.doc.media[0].hash}}" } }
  ocr_screenshots:
    invoke: { tool: nika:ocr,
              input: { hash: "{{with.screenshot.media[0].hash}}" } }
  analyze:
    depends_on: [extract, ocr_screenshots]
    infer:
      model: claude
      prompt: "PDF: {{with.extract.text}}\nOCR: {{with.ocr.text}}\nSynthetise."
  chart:
    depends_on: [analyze]
    invoke: { tool: nika:chart, input: { config: "{{with.analyze.output}}" } }
    artifact: { path: "reports/chart.png", format: binary }
```
