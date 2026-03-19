# PR3: Media Superpowers -- Full Media Processing Toolkit

> **Version:** v1.0
> **Date:** 2026-03-18
> **Branch:** `feat/media-superpowers`
> **Baseline:** After PR2 merged (PR1 media extraction + PR2 binary artifacts)
> **Depends on:** PR2 (`feat/media-artifacts`) merged
> **Scope:** Feature-gated media processing capabilities — thumbnails, metadata, optimization, provenance, OCR, charts, QR validation, audio/video metadata, and more
> **Research:** 22 parallel agents, 300+ crates surveyed, 18 research reports in `docs/research/`
> **Target version:** v0.33.0

**Parent:** [Master Plan](./2026-03-18-media-pipeline-master-plan.md) | **Prev:** [PR2](./2026-03-18-media-pipeline-pr2-artifacts-v6.md)

---

## Design Philosophy

PR1 built the media extraction pipeline. PR2 added binary artifacts and CAS CLI. PR3 turns Nika into a **universal media processing engine** — the first workflow engine where AI agents can natively manipulate, analyze, and transform every media type.

### Principles

1. **Feature-gated everything** — core Nika stays lean. Each capability is behind a Cargo feature flag.
2. **Pure Rust first** — prefer zero-dep crates. FFI only when pure Rust doesn't exist.
3. **Builtin tools** — each capability becomes a `builtin:` tool usable in workflows.
4. **Pipeline-aware** — each tool reads from CAS and writes to CAS. Content-addressed in, content-addressed out.
5. **Agent-friendly** — tools are designed for AI agents to compose in workflows.

### Architecture

```
Workflow YAML
  │
  ├─ infer: / invoke: / exec:
  │   └─ Media extracted → CAS store (PR1)
  │   └─ Binary artifacts written (PR2)
  │
  └─ builtin:                           ← PR3 NEW
      ├─ builtin:thumbnail              ← Resize images via SIMD
      ├─ builtin:metadata               ← Extract EXIF/audio/video metadata
      ├─ builtin:optimize               ← Compress PNG/JPEG/WebP
      ├─ builtin:ocr                    ← Extract text from images
      ├─ builtin:chart                  ← JSON → chart image
      ├─ builtin:qr_validate            ← QR code scoring
      ├─ builtin:svg_render             ← SVG → PNG rasterization
      ├─ builtin:pdf_extract            ← PDF → text extraction
      ├─ builtin:thumbhash              ← Generate compact image placeholder
      ├─ builtin:dominant_color         ← Extract color palette
      ├─ builtin:provenance             ← C2PA manifest signing
      └─ builtin:phash                  ← Perceptual similarity check
```

Each builtin tool:
- Accepts a CAS hash (or raw path) as input
- Processes the media
- Stores result in CAS
- Returns a `MediaRef` or structured JSON

---

## Dependency Strategy

### Tier 1 — Always On (pure Rust, zero/tiny deps)

These add minimal compile time and binary size. Always enabled.

```toml
imagesize = "0.13"                # Image dimensions from headers (0 deps)
thumbhash = "0.1"                 # 25-byte image placeholders (0 deps)
dominant_color = "0.3"            # Dominant color extraction (0 deps)
```

### Tier 2 — Default Features (pure Rust, moderate deps)

Enabled by default but can be disabled with `--no-default-features`.

```toml
[features]
default = ["media-thumbnail", "media-metadata", "media-optimize", "media-svg"]

media-thumbnail = ["dep:fast_image_resize", "dep:image"]
media-metadata = ["dep:nom-exif", "dep:lofty", "dep:mp4"]
media-optimize = ["dep:oxipng"]
media-svg = ["dep:resvg", "dep:usvg", "dep:tiny-skia"]

[dependencies]
fast_image_resize = { version = "6.0", features = ["image"], optional = true }
image = { version = "0.25", optional = true, default-features = false, features = ["png", "jpeg", "webp", "gif"] }
nom-exif = { version = "2.7", optional = true }
lofty = { version = "0.23", optional = true }
mp4 = { version = "0.14", optional = true }
oxipng = { version = "10.1", optional = true, default-features = false, features = ["parallel"] }
resvg = { version = "0.47", optional = true }
usvg = { version = "0.47", optional = true }
tiny-skia = { version = "0.12", optional = true }
```

### Tier 3 — Opt-In Features (larger deps or FFI)

Must be explicitly enabled.

```toml
[features]
media-ocr = ["dep:ocrs", "dep:rten"]
media-chart = ["dep:charts-rs"]
media-qr = ["dep:qrcode-ai-scanner-core"]
media-provenance = ["dep:c2pa"]
media-phash = ["dep:image_hasher"]
media-audio-decode = ["dep:symphonia"]
media-pdf = ["dep:pdf-extract", "dep:lopdf"]
media-compression = ["dep:zstd"]
media-color = ["dep:palette", "dep:tiny-skia"]
media-text-overlay = ["dep:cosmic-text", "dep:ab_glyph"]
media-spreadsheet = ["dep:calamine", "dep:rust_xlsxwriter"]

[dependencies]
ocrs = { version = "0.12", optional = true }
rten = { version = "0.24", optional = true }
charts-rs = { version = "0.3", optional = true }
qrcode-ai-scanner-core = { version = "0.2", optional = true }
c2pa = { version = "0.78", optional = true, features = ["rust_native_crypto"] }
image_hasher = { version = "3.1", optional = true }
symphonia = { version = "0.5", optional = true, features = ["aac", "flac", "mp3", "pcm", "vorbis", "isomp4", "ogg", "wav"] }
pdf-extract = { version = "0.10", optional = true }
lopdf = { version = "0.39", optional = true }
zstd = { version = "0.13", optional = true, default-features = false }
palette = { version = "0.7", optional = true }
cosmic-text = { version = "0.12", optional = true }
ab_glyph = { version = "0.2", optional = true }
calamine = { version = "0.34", optional = true }
rust_xlsxwriter = { version = "0.94", optional = true }
```

### Tier 4 — Heavy/FFI (explicitly opt-in, requires system deps)

```toml
[features]
media-video = ["dep:ffmpeg-sidecar"]
media-jpeg-pro = ["dep:mozjpeg"]
media-heif = ["dep:libheif-rs"]
media-raw = ["dep:rawler"]
media-watermark = ["dep:trustmark"]  # Requires ONNX Runtime
media-ml = ["dep:ort"]               # Requires ONNX Runtime

[dependencies]
ffmpeg-sidecar = { version = "2.4", optional = true }
mozjpeg = { version = "0.10", optional = true }
libheif-rs = { version = "2.7", optional = true }
rawler = { version = "0.7", optional = true }
```

---

## Feature Flag Matrix

```
Feature              Crate(s)              Pure Rust  Binary +   Compile +
─────────────────────────────────────────────────────────────────────────
media-thumbnail      fast_image_resize     Yes        ~800KB     +3s
                     + image
media-metadata       nom-exif + lofty      Yes        ~400KB     +2s
                     + mp4
media-optimize       oxipng                Yes        ~300KB     +2s
media-svg            resvg + usvg          Yes        ~2MB       +5s
                     + tiny-skia
media-ocr            ocrs + rten           Yes        ~5MB       +8s
media-chart          charts-rs             Yes        ~1MB       +3s
media-qr             qrcode-ai-scanner     Yes        ~2MB       +4s
media-provenance     c2pa                  Yes*       ~3MB       +5s
media-phash          image_hasher          Yes        ~200KB     +1s
media-audio-decode   symphonia             Yes        ~1MB       +3s
media-pdf            pdf-extract + lopdf   Yes        ~500KB     +2s
media-compression    zstd                  No (FFI)   ~400KB     +2s
media-video          ffmpeg-sidecar        External   ~100KB     +1s
media-ml             ort                   No (FFI)   ~20MB      +10s

* c2pa with rust_native_crypto feature = no OpenSSL
```

---

## Phase A: Core Infrastructure (Commits 1-3)

### Commit 1: `feat(media): builtin tool dispatcher infrastructure`

**Files:**
- `src/runtime/builtin/mod.rs` — add media tool registry
- `src/runtime/builtin/media/mod.rs` — NEW: media tools module
- `src/runtime/builtin/media/dispatch.rs` — NEW: route tool names to handlers

**What it does:**
Creates the infrastructure for `builtin:thumbnail`, `builtin:metadata`, etc. Each media builtin tool:
- Receives a JSON input (from workflow `with:` bindings)
- Reads from CAS via hash
- Processes
- Writes result to CAS (or returns JSON)
- Returns structured output

```rust
// src/runtime/builtin/media/dispatch.rs
pub async fn dispatch_media_tool(
    tool_name: &str,
    input: &Value,
    cas_store: &CasStore,
    task_id: &str,
) -> Result<BuiltinResult, NikaError> {
    match tool_name {
        #[cfg(feature = "media-thumbnail")]
        "thumbnail" => thumbnail::generate(input, cas_store, task_id).await,

        #[cfg(feature = "media-metadata")]
        "metadata" => metadata::extract(input, cas_store, task_id).await,

        #[cfg(feature = "media-optimize")]
        "optimize" => optimize::run(input, cas_store, task_id).await,

        #[cfg(feature = "media-ocr")]
        "ocr" => ocr::recognize(input, cas_store, task_id).await,

        #[cfg(feature = "media-chart")]
        "chart" => chart::render(input, cas_store, task_id).await,

        #[cfg(feature = "media-qr")]
        "qr_validate" => qr::validate(input, cas_store, task_id).await,

        #[cfg(feature = "media-svg")]
        "svg_render" => svg::render(input, cas_store, task_id).await,

        #[cfg(feature = "media-pdf")]
        "pdf_extract" => pdf::extract(input, cas_store, task_id).await,

        "thumbhash" => thumbhash_tool::generate(input, cas_store, task_id).await,
        "dominant_color" => color::dominant(input, cas_store, task_id).await,
        "dimensions" => dimensions::get(input, cas_store, task_id).await,

        #[cfg(feature = "media-phash")]
        "phash" => phash::compute(input, cas_store, task_id).await,

        #[cfg(feature = "media-provenance")]
        "provenance" => provenance::sign(input, cas_store, task_id).await,

        _ => Err(NikaError::BuiltinToolNotFound {
            tool: format!("media:{tool_name}"),
        }),
    }
}
```

**Workflow usage:**
```yaml
tasks:
  generate:
    invoke:
      tool: image_gen
      input: { prompt: "butterfly logo" }

  thumb:
    depends_on: [generate]
    with: { img: $generate }
    builtin:
      tool: thumbnail
      input:
        hash: "{{with.img.media[0].hash}}"
        width: 256
        height: 256
```

**Tests:** 3 — dispatch routing, unknown tool error, feature-gated compilation

---

### Commit 2: `feat(media): CAS-aware tool base types`

**Files:**
- `src/runtime/builtin/media/types.rs` — NEW: shared types for media tools

```rust
/// Input for media tools that read from CAS
#[derive(Debug, Deserialize)]
pub struct CasInput {
    /// blake3 hash of source media in CAS
    pub hash: String,
    /// Alternative: raw file path (for non-CAS inputs)
    pub path: Option<PathBuf>,
}

impl CasInput {
    /// Read data from CAS store or file path
    pub async fn read_data(&self, cas: &CasStore) -> Result<Vec<u8>, NikaError> {
        if let Some(path) = &self.path {
            tokio::fs::read(path).await.map_err(|e| NikaError::MediaError(
                MediaError::MediaStoreIo { path: path.display().to_string(), source: e }
            ))
        } else {
            cas.read(&self.hash).await.map_err(NikaError::MediaError)
        }
    }
}

/// Output for media tools that write to CAS
#[derive(Debug, Serialize)]
pub struct MediaToolOutput {
    /// New CAS hash of processed media
    pub hash: Option<String>,
    /// Structured metadata (JSON)
    pub data: Value,
    /// Media refs for downstream artifact writes
    pub media: Vec<MediaRef>,
}
```

**Tests:** 2 — CasInput read from hash, CasInput read from path

---

### Commit 3: `feat(media): wire media builtins into executor`

**Files:**
- `src/runtime/executor/verbs.rs` — add media builtin dispatch
- `src/runtime/builtin/mod.rs` — register media tools

Wire `builtin: { tool: thumbnail, ... }` through the executor to the media dispatcher.

**Integration point:** `verbs.rs` already has builtin tool dispatch (sleep, log, emit, assert, etc.). Add:

```rust
// In run_builtin(), after existing tool dispatch:
if tool_name.starts_with("media:") || MEDIA_BUILTINS.contains(&tool_name.as_str()) {
    let cas = CasStore::workspace_default(&workspace_root);
    return dispatch_media_tool(
        tool_name.strip_prefix("media:").unwrap_or(tool_name),
        &input,
        &cas,
        &task_id,
    ).await;
}

const MEDIA_BUILTINS: &[&str] = &[
    "thumbnail", "metadata", "optimize", "ocr", "chart",
    "qr_validate", "svg_render", "pdf_extract", "thumbhash",
    "dominant_color", "dimensions", "phash", "provenance",
];
```

**Tests:** 2 — builtin dispatch integration, unknown tool error

---

## Phase B: Tier 1 Always-On Tools (Commits 4-6)

### Commit 4: `feat(media): builtin:dimensions — image dimensions from headers`

**Crate:** `imagesize = "0.13"` (12M downloads, ZERO deps)
**Feature:** Always on (Tier 1)
**File:** `src/runtime/builtin/media/dimensions.rs`

```rust
pub async fn get(input: &Value, cas: &CasStore, _task_id: &str) -> Result<BuiltinResult, NikaError> {
    let cas_input: CasInput = serde_json::from_value(input.clone())?;
    let data = cas_input.read_data(cas).await?;

    let size = imagesize::blob_size(&data)
        .map_err(|_| NikaError::MediaError(MediaError::UnsupportedMediaType {
            mime_type: "unknown".into(),
            reason: "Could not determine image dimensions".into(),
        }))?;

    Ok(BuiltinResult::json(json!({
        "width": size.width,
        "height": size.height,
    })))
}
```

**Workflow:**
```yaml
tasks:
  get_size:
    builtin:
      tool: dimensions
      input:
        hash: "{{with.img.media[0].hash}}"
    # Output: { "width": 1920, "height": 1080 }
```

**Supported formats:** PNG, JPEG, GIF, WebP, BMP, TIFF, PSD, ICO, HEIF, AVIF, JXL, QOI, KTX2, DDS, TGA, HDR, OpenEXR (20+ formats, header-only, no full decode).

**Tests:** 3 — PNG dimensions, JPEG dimensions, unsupported format error

---

### Commit 5: `feat(media): builtin:thumbhash — compact image placeholders`

**Crate:** `thumbhash = "0.1"` (173K downloads, ZERO deps)
**Feature:** Always on (Tier 1)
**File:** `src/runtime/builtin/media/thumbhash_tool.rs`

ThumbHash encodes ANY image into ~25 bytes that decode to a beautiful blurry placeholder. Created by Evan Wallace (esbuild author). Better than BlurHash in every way: supports alpha, auto-encodes aspect ratio, no configuration needed.

```rust
pub async fn generate(input: &Value, cas: &CasStore, _task_id: &str) -> Result<BuiltinResult, NikaError> {
    let cas_input: CasInput = serde_json::from_value(input.clone())?;
    let data = cas_input.read_data(cas).await?;

    // Decode image to RGBA
    let img = image::load_from_memory(&data)
        .map_err(|e| NikaError::MediaError(MediaError::UnsupportedMediaType {
            mime_type: "unknown".into(),
            reason: format!("Image decode failed: {e}"),
        }))?;

    let rgba = img.to_rgba8();
    let (w, h) = rgba.dimensions();

    // Encode to thumbhash (~25 bytes)
    let hash = thumbhash::rgba_to_thumb_hash(w as usize, h as usize, rgba.as_raw());
    let b64 = base64::engine::general_purpose::STANDARD.encode(&hash);

    // Also decode back to get placeholder dimensions
    let (pw, ph, _) = thumbhash::thumb_hash_to_approximate_aspect_ratio(&hash);

    Ok(BuiltinResult::json(json!({
        "thumbhash": b64,
        "thumbhash_bytes": hash.len(),
        "source_width": w,
        "source_height": h,
        "placeholder_aspect_ratio": format!("{pw}:{ph}"),
    })))
}
```

**Workflow:**
```yaml
tasks:
  placeholder:
    builtin:
      tool: thumbhash
      input:
        hash: "{{with.img.media[0].hash}}"
    # Output: { "thumbhash": "VggKDYB...", "thumbhash_bytes": 25, ... }
```

**Use case:** Store the 25-byte thumbhash in artifact metadata. Any client (web, mobile) can instantly render a placeholder while the full image loads. This is how Instagram, Mastodon, and Bluesky handle image loading.

**Tests:** 3 — encode PNG, encode with alpha, roundtrip encode→decode dimensions

---

### Commit 6: `feat(media): builtin:dominant_color — extract color palette`

**Crate:** `dominant_color = "0.3"` (74K downloads, ZERO deps) + `color-thief = "0.2"` (460K downloads)
**Feature:** Always on (Tier 1)
**File:** `src/runtime/builtin/media/color.rs`

```rust
pub async fn dominant(input: &Value, cas: &CasStore, _task_id: &str) -> Result<BuiltinResult, NikaError> {
    let cas_input: CasInput = serde_json::from_value(input.clone())?;
    let count: usize = input.get("count").and_then(|v| v.as_u64()).unwrap_or(5) as usize;
    let data = cas_input.read_data(cas).await?;

    let img = image::load_from_memory(&data)?;
    let rgba = img.to_rgba8();
    let pixels: Vec<[u8; 4]> = rgba.pixels().map(|p| p.0).collect();

    // Get dominant color (single, fast)
    let dominant = dominant_color::get_dominant_color(&pixels);

    // Get palette (N colors via median cut)
    let palette = color_thief::get_palette(
        rgba.as_raw(),
        color_thief::ColorFormat::Rgba,
        1, // quality (1 = best, 10 = fastest)
        count as u8,
    ).unwrap_or_default();

    Ok(BuiltinResult::json(json!({
        "dominant": {
            "r": dominant.map(|c| c[0]),
            "g": dominant.map(|c| c[1]),
            "b": dominant.map(|c| c[2]),
            "hex": dominant.map(|c| format!("#{:02x}{:02x}{:02x}", c[0], c[1], c[2])),
        },
        "palette": palette.iter().map(|c| json!({
            "r": c.r, "g": c.g, "b": c.b,
            "hex": format!("#{:02x}{:02x}{:02x}", c.r, c.g, c.b),
        })).collect::<Vec<_>>(),
    })))
}
```

**Workflow:**
```yaml
tasks:
  colors:
    builtin:
      tool: dominant_color
      input:
        hash: "{{with.img.media[0].hash}}"
        count: 5
    # Output: { "dominant": { "hex": "#ff6b35" }, "palette": [...] }
```

**Use case:** AI agent generates an image → extract dominant colors → use in website theme, social media post, or complementary design elements. Agents can now "see" colors.

**Tests:** 3 — single dominant, palette of 5, grayscale image

---

## Phase C: Tier 2 Default Features (Commits 7-12)

### Commit 7: `feat(media): builtin:thumbnail — SIMD-accelerated image resize`

**Crates:** `fast_image_resize = { version = "6.0", features = ["image"] }` + `image = "0.25"`
**Feature:** `media-thumbnail`
**File:** `src/runtime/builtin/media/thumbnail.rs`

```rust
#[cfg(feature = "media-thumbnail")]
pub async fn generate(input: &Value, cas: &CasStore, task_id: &str) -> Result<BuiltinResult, NikaError> {
    let cas_input: CasInput = serde_json::from_value(input.clone())?;
    let width: u32 = input["width"].as_u64().unwrap_or(256) as u32;
    let height: u32 = input["height"].as_u64().unwrap_or(256) as u32;
    let format: &str = input.get("format").and_then(|v| v.as_str()).unwrap_or("png");

    let data = cas_input.read_data(cas).await?;
    let src = image::load_from_memory(&data)?;

    // SIMD-accelerated resize (AVX2/SSE4.1/Neon auto-detected)
    use fast_image_resize::{images::Image, IntoImageView, Resizer};
    let pixel_type = src.pixel_type().ok_or_else(|| NikaError::MediaError(
        MediaError::UnsupportedMediaType {
            mime_type: "unknown".into(),
            reason: "Unsupported pixel format for resize".into(),
        }
    ))?;

    let mut dst = Image::new(width, height, pixel_type);
    let mut resizer = Resizer::new();
    resizer.resize(&src, &mut dst, None)?;

    // Encode to target format
    let mut output_buf = Vec::new();
    let output_img = image::DynamicImage::from(/* convert dst back */);
    match format {
        "png" => output_img.write_to(&mut Cursor::new(&mut output_buf), image::ImageFormat::Png)?,
        "jpeg" | "jpg" => output_img.write_to(&mut Cursor::new(&mut output_buf), image::ImageFormat::Jpeg)?,
        "webp" => output_img.write_to(&mut Cursor::new(&mut output_buf), image::ImageFormat::WebP)?,
        _ => output_img.write_to(&mut Cursor::new(&mut output_buf), image::ImageFormat::Png)?,
    }

    // Store in CAS
    let store_result = cas.store(&output_buf).await?;

    let media_ref = MediaRef {
        hash: store_result.hash.clone(),
        mime_type: format!("image/{format}"),
        size_bytes: store_result.size,
        path: store_result.path.clone(),
        extension: format.to_string(),
        created_by: task_id.to_string(),
    };

    Ok(BuiltinResult::media(json!({
        "hash": store_result.hash,
        "width": width,
        "height": height,
        "format": format,
        "size": store_result.size,
        "original_width": src.width(),
        "original_height": src.height(),
    }), vec![media_ref]))
}
```

**Performance:** 13ms for 4928x3279 → 852x567 with AVX2 (vs 190ms with image-rs alone).

**Gotchas from Context7:**
- RGBA images: MUST premultiply alpha with `MulDiv` before resize, then divide after. Otherwise colors bleed at transparent edges.
- sRGB: The resizer does NOT convert to linear colorspace. For correct sRGB results, use `create_srgb_mapper()` before/after.

**Workflow:**
```yaml
tasks:
  thumb:
    depends_on: [generate]
    with: { img: $generate }
    builtin:
      tool: thumbnail
      input:
        hash: "{{with.img.media[0].hash}}"
        width: 256
        height: 256
        format: webp
    artifact:
      path: "thumbs/{{task_id}}.webp"
      format: binary
```

**Tests:** 4 — resize PNG, resize JPEG, RGBA alpha handling, WebP output

---

### Commit 8: `feat(media): builtin:metadata — universal media metadata extraction`

**Crates:** `nom-exif = "2.7"` + `lofty = "0.23"` + `mp4 = "0.14"`
**Feature:** `media-metadata`
**File:** `src/runtime/builtin/media/metadata.rs`

The unified metadata extractor. Routes based on MIME type detected by `infer`:

```rust
#[cfg(feature = "media-metadata")]
pub async fn extract(input: &Value, cas: &CasStore, _task_id: &str) -> Result<BuiltinResult, NikaError> {
    let cas_input: CasInput = serde_json::from_value(input.clone())?;
    let data = cas_input.read_data(cas).await?;

    // Detect type
    let kind = infer::get(&data);
    let mime = kind.map(|k| k.mime_type()).unwrap_or("application/octet-stream");

    let metadata = if mime.starts_with("image/") {
        extract_image_metadata(&data)?
    } else if mime.starts_with("audio/") {
        extract_audio_metadata(&data)?
    } else if mime.starts_with("video/") {
        extract_video_metadata(&data)?
    } else {
        json!({ "mime_type": mime, "size": data.len() })
    };

    Ok(BuiltinResult::json(metadata))
}

fn extract_image_metadata(data: &[u8]) -> Result<Value, NikaError> {
    use nom_exif::*;
    let mut parser = MediaParser::new();
    let ms = MediaSource::seekable_buf(data)?;

    let mut result = json!({});
    if let Ok(iter) = parser.parse::<ExifIter>(ms) {
        let exif: Exif = iter.into();
        // Camera info
        if let Some(v) = exif.get(ExifTag::Make) { result["camera_make"] = json!(v.as_str()); }
        if let Some(v) = exif.get(ExifTag::Model) { result["camera_model"] = json!(v.as_str()); }
        // Image dimensions
        if let Some(v) = exif.get(ExifTag::ImageWidth) { result["width"] = json!(v.as_u32()); }
        if let Some(v) = exif.get(ExifTag::ImageHeight) { result["height"] = json!(v.as_u32()); }
        // GPS
        if let Some(v) = exif.get(ExifTag::GPSLatitude) { result["gps_lat"] = json!(v.as_float()); }
        if let Some(v) = exif.get(ExifTag::GPSLongitude) { result["gps_lon"] = json!(v.as_float()); }
        // Dates
        if let Some(v) = exif.get(ExifTag::DateTimeOriginal) { result["date_taken"] = json!(v.as_str()); }
        // Orientation
        if let Some(v) = exif.get(ExifTag::Orientation) { result["orientation"] = json!(v.as_u32()); }
    }

    // Also get dimensions from imagesize (more reliable than EXIF)
    if let Ok(size) = imagesize::blob_size(data) {
        result["pixel_width"] = json!(size.width);
        result["pixel_height"] = json!(size.height);
    }

    Ok(result)
}

fn extract_audio_metadata(data: &[u8]) -> Result<Value, NikaError> {
    use lofty::prelude::*;
    let tagged = lofty::read_from(&mut std::io::Cursor::new(data))?;

    let mut result = json!({});
    if let Some(tag) = tagged.primary_tag() {
        if let Some(v) = tag.title() { result["title"] = json!(v); }
        if let Some(v) = tag.artist() { result["artist"] = json!(v); }
        if let Some(v) = tag.album() { result["album"] = json!(v); }
        if let Some(v) = tag.year() { result["year"] = json!(v); }
        if let Some(v) = tag.genre() { result["genre"] = json!(v); }
        if let Some(v) = tag.track() { result["track"] = json!(v); }
    }
    let props = tagged.properties();
    result["duration_secs"] = json!(props.duration().as_secs_f64());
    if let Some(br) = props.overall_bitrate() { result["bitrate_kbps"] = json!(br); }
    if let Some(sr) = props.sample_rate() { result["sample_rate"] = json!(sr); }
    if let Some(ch) = props.channels() { result["channels"] = json!(ch); }

    Ok(result)
}

fn extract_video_metadata(data: &[u8]) -> Result<Value, NikaError> {
    use mp4::Mp4Reader;
    let reader = Mp4Reader::read_header(&mut std::io::Cursor::new(data), data.len() as u64)?;

    let mut result = json!({
        "duration_secs": reader.duration().as_secs_f64(),
        "timescale": reader.timescale(),
    });

    let mut tracks = vec![];
    for track in reader.tracks().values() {
        tracks.push(json!({
            "track_type": format!("{:?}", track.track_type()),
            "codec": track.media_type().map(|m| format!("{m:?}")),
            "width": track.width(),
            "height": track.height(),
            "bitrate_bps": track.bitrate(),
            "sample_rate": track.sample_rate().map(|sr| sr.value()),
        }));
    }
    result["tracks"] = json!(tracks);

    Ok(result)
}
```

**Workflow:**
```yaml
tasks:
  analyze:
    builtin:
      tool: metadata
      input:
        hash: "{{with.photo.media[0].hash}}"
    # Image output: { "camera_make": "Apple", "camera_model": "iPhone 15 Pro",
    #                  "gps_lat": 48.8566, "gps_lon": 2.3522, "orientation": 1,
    #                  "pixel_width": 4032, "pixel_height": 3024, "date_taken": "..." }
    # Audio output: { "title": "Song", "artist": "Artist", "duration_secs": 213.4,
    #                  "bitrate_kbps": 320, "sample_rate": 44100, "channels": 2 }
    # Video output: { "duration_secs": 30.0, "tracks": [...] }
```

**Why nom-exif over kamadak-exif:** nom-exif handles images AND video/audio (MP4, MOV, MKV, WebM). Unified API. Async-ready. Fuzz-tested. The modern choice.

**Tests:** 6 — JPEG EXIF, PNG no EXIF, MP3 ID3, MP4 metadata, unknown format, GPS extraction

---

### Commit 9: `feat(media): builtin:optimize — lossless PNG optimization`

**Crate:** `oxipng = { version = "10.1", features = ["parallel"] }`
**Feature:** `media-optimize`
**File:** `src/runtime/builtin/media/optimize.rs`

```rust
#[cfg(feature = "media-optimize")]
pub async fn run(input: &Value, cas: &CasStore, task_id: &str) -> Result<BuiltinResult, NikaError> {
    let cas_input: CasInput = serde_json::from_value(input.clone())?;
    let level: u8 = input.get("level").and_then(|v| v.as_u64()).unwrap_or(2) as u8;
    let strip_metadata: bool = input.get("strip").and_then(|v| v.as_bool()).unwrap_or(true);
    let data = cas_input.read_data(cas).await?;

    let original_size = data.len();

    // Detect format
    let kind = infer::get(&data);
    let mime = kind.map(|k| k.mime_type()).unwrap_or("");

    let optimized = match mime {
        "image/png" => {
            let mut opts = oxipng::Options::from_preset(level);
            if strip_metadata {
                opts.strip = oxipng::StripChunks::Safe;
            }
            oxipng::optimize_from_memory(&data, &opts)
                .map_err(|e| NikaError::MediaError(MediaError::UnsupportedMediaType {
                    mime_type: mime.into(),
                    reason: format!("PNG optimization failed: {e}"),
                }))?
        }
        _ => {
            // Pass-through for non-PNG (future: JPEG, WebP, AVIF)
            return Ok(BuiltinResult::json(json!({
                "skipped": true,
                "reason": format!("Optimization not supported for {mime}"),
                "original_size": original_size,
            })));
        }
    };

    let savings = original_size as i64 - optimized.len() as i64;
    let savings_pct = if original_size > 0 {
        (savings as f64 / original_size as f64 * 100.0).round()
    } else { 0.0 };

    // Store optimized version in CAS
    let store_result = cas.store(&optimized).await?;

    let media_ref = MediaRef {
        hash: store_result.hash.clone(),
        mime_type: "image/png".into(),
        size_bytes: store_result.size,
        path: store_result.path.clone(),
        extension: "png".into(),
        created_by: task_id.into(),
    };

    Ok(BuiltinResult::media(json!({
        "hash": store_result.hash,
        "original_size": original_size,
        "optimized_size": optimized.len(),
        "savings_bytes": savings,
        "savings_percent": savings_pct,
        "stripped_metadata": strip_metadata,
    }), vec![media_ref]))
}
```

**Workflow:**
```yaml
tasks:
  optimize:
    depends_on: [generate]
    with: { img: $generate }
    builtin:
      tool: optimize
      input:
        hash: "{{with.img.media[0].hash}}"
        level: 4          # 0-6 (higher = slower, better)
        strip: true       # Remove non-essential metadata
    # Output: { "savings_percent": 32, "savings_bytes": 45000, ... }
```

**Tests:** 4 — optimize PNG, strip metadata, non-PNG passthrough, level 0 vs 4

---

### Commit 10: `feat(media): builtin:svg_render — SVG to PNG rasterization`

**Crates:** `resvg = "0.47"` + `usvg = "0.47"` + `tiny-skia = "0.12"`
**Feature:** `media-svg`
**File:** `src/runtime/builtin/media/svg.rs`

```rust
#[cfg(feature = "media-svg")]
pub async fn render(input: &Value, cas: &CasStore, task_id: &str) -> Result<BuiltinResult, NikaError> {
    let cas_input: CasInput = serde_json::from_value(input.clone())?;
    let target_width: Option<u32> = input.get("width").and_then(|v| v.as_u64()).map(|v| v as u32);
    let target_height: Option<u32> = input.get("height").and_then(|v| v.as_u64()).map(|v| v as u32);
    let data = cas_input.read_data(cas).await?;

    let svg_str = String::from_utf8(data)
        .map_err(|_| NikaError::MediaError(MediaError::UnsupportedMediaType {
            mime_type: "image/svg+xml".into(),
            reason: "SVG is not valid UTF-8".into(),
        }))?;

    // Parse SVG
    let opt = usvg::Options::default();
    let tree = usvg::Tree::from_str(&svg_str, &opt)?;
    let svg_size = tree.size();

    // Determine output dimensions
    let (w, h) = match (target_width, target_height) {
        (Some(w), Some(h)) => (w, h),
        (Some(w), None) => {
            let scale = w as f32 / svg_size.width();
            (w, (svg_size.height() * scale) as u32)
        }
        (None, Some(h)) => {
            let scale = h as f32 / svg_size.height();
            ((svg_size.width() * scale) as u32, h)
        }
        (None, None) => (svg_size.width() as u32, svg_size.height() as u32),
    };

    // Render to pixmap
    let mut pixmap = tiny_skia::Pixmap::new(w, h)
        .ok_or_else(|| NikaError::MediaError(MediaError::UnsupportedMediaType {
            mime_type: "image/svg+xml".into(),
            reason: format!("Cannot create {w}x{h} pixmap"),
        }))?;

    let sx = w as f32 / svg_size.width();
    let sy = h as f32 / svg_size.height();
    let transform = tiny_skia::Transform::from_scale(sx, sy);

    resvg::render(&tree, transform, &mut pixmap.as_mut());

    // Encode as PNG
    let png_data = pixmap.encode_png()?;

    // Store in CAS
    let store_result = cas.store(&png_data).await?;

    let media_ref = MediaRef {
        hash: store_result.hash.clone(),
        mime_type: "image/png".into(),
        size_bytes: store_result.size,
        path: store_result.path.clone(),
        extension: "png".into(),
        created_by: task_id.into(),
    };

    Ok(BuiltinResult::media(json!({
        "hash": store_result.hash,
        "width": w,
        "height": h,
        "svg_width": svg_size.width(),
        "svg_height": svg_size.height(),
        "size": store_result.size,
    }), vec![media_ref]))
}
```

**Workflow:**
```yaml
tasks:
  render:
    builtin:
      tool: svg_render
      input:
        hash: "{{with.diagram.media[0].hash}}"
        width: 1024
    artifact:
      path: "renders/diagram.png"
      format: binary
```

**Use case:** Mermaid diagrams → SVG → PNG. AI agents generate SVG charts → rendered to images for reports. Pure Rust, pixel-reproducible across platforms.

**Tests:** 3 — basic SVG, scaled SVG, invalid SVG error

---

### Commit 11: `feat(media): builtin:pdf_extract — PDF text extraction`

**Crates:** `pdf-extract = "0.10"` + `lopdf = "0.39"`
**Feature:** `media-pdf`
**File:** `src/runtime/builtin/media/pdf.rs`

```rust
#[cfg(feature = "media-pdf")]
pub async fn extract(input: &Value, cas: &CasStore, _task_id: &str) -> Result<BuiltinResult, NikaError> {
    let cas_input: CasInput = serde_json::from_value(input.clone())?;
    let data = cas_input.read_data(cas).await?;

    let text = pdf_extract::extract_text_from_mem(&data)
        .map_err(|e| NikaError::MediaError(MediaError::UnsupportedMediaType {
            mime_type: "application/pdf".into(),
            reason: format!("PDF extraction failed: {e}"),
        }))?;

    // Page count via lopdf
    let doc = lopdf::Document::load_mem(&data)
        .map_err(|e| NikaError::MediaError(MediaError::UnsupportedMediaType {
            mime_type: "application/pdf".into(),
            reason: format!("PDF parse failed: {e}"),
        }))?;
    let page_count = doc.get_pages().len();

    Ok(BuiltinResult::json(json!({
        "text": text,
        "page_count": page_count,
        "char_count": text.len(),
        "word_count": text.split_whitespace().count(),
    })))
}
```

**Workflow:**
```yaml
tasks:
  extract:
    builtin:
      tool: pdf_extract
      input:
        hash: "{{with.doc.media[0].hash}}"
    # Output: { "text": "...", "page_count": 5, "word_count": 1234 }

  summarize:
    depends_on: [extract]
    with: { pdf: $extract }
    infer:
      model: claude
      prompt: "Summarize this document: {{with.pdf.text}}"
```

**Use case:** AI agents extract text from PDFs → feed to LLM for summarization, Q&A, analysis.

**Tests:** 3 — extract text, page count, non-PDF error

---

### Commit 12: `feat(media): auto-enrich MediaRef with dimensions + thumbhash on CAS store`

**Files:** `src/media/processor.rs` — enrich after store
**Feature:** Always on (Tier 1 crates)

After every CAS store, automatically extract dimensions and thumbhash for images. This enriches `MediaRef` with metadata accessible via `{{with.task.media[0].metadata.width}}`.

```rust
// In MediaProcessor::process_base64(), after successful store:

// Auto-enrich for images
let mut metadata = serde_json::Map::new();

if detected.mime_type.starts_with("image/") {
    // Dimensions (header-only, ~0.1ms)
    if let Ok(size) = imagesize::blob_size(&decoded) {
        metadata.insert("width".into(), json!(size.width));
        metadata.insert("height".into(), json!(size.height));
    }

    // ThumbHash (~5ms for typical image)
    if decoded.len() < 10 * 1024 * 1024 { // Skip for >10MB
        if let Ok(img) = image::load_from_memory(&decoded) {
            let rgba = img.resize(100, 100, image::imageops::FilterType::Triangle).to_rgba8();
            let (w, h) = rgba.dimensions();
            let hash = thumbhash::rgba_to_thumb_hash(w as usize, h as usize, rgba.as_raw());
            metadata.insert("thumbhash".into(), json!(base64::engine::general_purpose::STANDARD.encode(&hash)));
        }
    }
}

let media_ref = MediaRef {
    hash: store_result.hash.clone(),
    mime_type: detected.mime_type.clone(),
    size_bytes: store_result.size,
    path: store_result.path.clone(),
    extension: detected.extension.clone(),
    created_by: task_id.to_string(),
    metadata, // NEW field
};
```

**MediaRef update** (`types.rs`):
```rust
pub struct MediaRef {
    // ... existing fields ...
    /// Auto-enriched metadata (dimensions, thumbhash, etc.)
    #[serde(default, skip_serializing_if = "serde_json::Map::is_empty")]
    pub metadata: serde_json::Map<String, Value>,
}
```

**Template access:**
```yaml
with: { img: $generate }
# {{with.img.media[0].metadata.width}}      → 1920
# {{with.img.media[0].metadata.height}}     → 1080
# {{with.img.media[0].metadata.thumbhash}}  → "VggKDYB..."
```

**Tests:** 4 — PNG enriched, JPEG enriched, audio NOT enriched, >10MB skips thumbhash

---

## Phase D: Tier 3 Opt-In Features (Commits 13-20)

### Commit 13: `feat(media): builtin:ocr — pure Rust text recognition`

**Crate:** `ocrs = "0.12"` + `rten = "0.24"`
**Feature:** `media-ocr`
**File:** `src/runtime/builtin/media/ocr.rs`

Pure Rust OCR via `ocrs` (uses `rten` runtime, no ONNX C++ deps). ~280ms/page on CPU. Compiles to WASM. Latin alphabet focused.

**Workflow:**
```yaml
tasks:
  read:
    builtin:
      tool: ocr
      input:
        hash: "{{with.screenshot.media[0].hash}}"
    # Output: { "text": "Hello World\nLine 2", "confidence": 0.94, "regions": [...] }
```

---

### Commit 14: `feat(media): builtin:chart — JSON to chart image`

**Crate:** `charts-rs = "0.3"`
**Feature:** `media-chart`
**File:** `src/runtime/builtin/media/chart.rs`

JSON-first API maps directly to Nika `with:` bindings. An `infer:` step generates chart config JSON, the builtin renders it to SVG/PNG.

**Workflow:**
```yaml
tasks:
  data:
    infer:
      model: claude
      prompt: "Generate a charts-rs JSON config for a bar chart showing Q1-Q4 revenue"

  render:
    depends_on: [data]
    with: { config: $data }
    builtin:
      tool: chart
      input:
        config: "{{with.config.output}}"
        format: png
        width: 800
        height: 400
    artifact:
      path: "charts/revenue.png"
      format: binary
```

---

### Commit 15: `feat(media): builtin:qr_validate — QR code validation + scoring`

**Crate:** `qrcode-ai-scanner-core = "0.2"` (SuperNovae's own!)
**Feature:** `media-qr`
**File:** `src/runtime/builtin/media/qr.rs`

4-tier progressive decoding. 89% success on artistic QR codes. Scannability score 0-100.

**Workflow:**
```yaml
tasks:
  check:
    builtin:
      tool: qr_validate
      input:
        hash: "{{with.qr.media[0].hash}}"
        min_score: 70
    # Output: { "valid": true, "content": "https://...", "score": 85,
    #           "error_correction": "H", "tier_used": 2 }
```

---

### Commit 16: `feat(media): builtin:phash — perceptual similarity detection`

**Crate:** `image_hasher = "3.1"`
**Feature:** `media-phash`
**File:** `src/runtime/builtin/media/phash.rs`

Compare two images by perceptual hash. Distance < 10 = near-duplicate.

**Workflow:**
```yaml
tasks:
  compare:
    builtin:
      tool: phash
      input:
        hash_a: "{{with.img1.media[0].hash}}"
        hash_b: "{{with.img2.media[0].hash}}"
    # Output: { "distance": 3, "similar": true, "algorithm": "DoubleGradient" }
```

---

### Commit 17: `feat(media): builtin:provenance — C2PA content credentials`

**Crate:** `c2pa = { version = "0.78", features = ["rust_native_crypto"] }`
**Feature:** `media-provenance`
**File:** `src/runtime/builtin/media/provenance.rs`

**THE game changer.** Sign artifacts with C2PA manifests (Adobe/CAI standard). Every AI-generated artifact carries cryptographic proof of origin: which workflow, which model, which Nika version, which timestamp.

```yaml
tasks:
  sign:
    depends_on: [generate]
    with: { img: $generate }
    builtin:
      tool: provenance
      input:
        hash: "{{with.img.media[0].hash}}"
        claim_generator: "Nika/0.33.0"
        assertions:
          - label: "c2pa.actions"
            data:
              actions:
                - action: "c2pa.created"
                  softwareAgent: "Nika Workflow Engine"
                  parameters:
                    workflow: "image-generation"
                    model: "dall-e-3"
    # Output: { "signed": true, "manifest_hash": "...", "claim_generator": "Nika/0.33.0" }
```

**EU AI Act compliance:** Regulations increasingly require provenance for AI-generated content. This makes Nika workflows legally compliant out of the box.

---

### Commit 18: `feat(media): builtin:audio metadata via symphonia + lofty`

**Crate:** `symphonia = "0.5"` (pure Rust audio decode)
**Feature:** `media-audio-decode`

Deep audio analysis: waveform peaks, spectral features, duration with sample-level precision.

---

### Commit 19: `feat(media): CAS compression with zstd for non-media blobs`

**Crate:** `zstd = "0.13"`
**Feature:** `media-compression`

Compress text/JSON artifacts in CAS with zstd. Skip already-compressed media (JPEG, PNG, MP4). Write-once-read-many: 1.5 GB/s decompression regardless of compression level.

---

### Commit 20: `feat(media): MediaRef.metadata auto-enrichment for audio + video`

Extend Commit 12's auto-enrichment to audio (duration, bitrate via symphonia) and video (duration, resolution via mp4 crate).

---

## Phase E: Integration + Tests (Commits 21-24)

### Commit 21: `test(media): builtin tool integration tests`

Comprehensive tests for all builtin media tools. Each tool gets:
- Happy path test
- Error handling test (invalid input, unsupported format)
- CAS roundtrip test (read → process → store → verify)

### Commit 22: `docs(media): builtin tool reference documentation`

Documentation for all media builtin tools in workflow YAML syntax.

### Commit 23: `feat(cli): nika media tools — list available media capabilities`

```bash
nika media tools           # List all available builtin media tools
nika media tools --json    # JSON output with feature status
```

### Commit 24: `chore: bump version to v0.33.0`

---

## Watch List — Future PRs (v0.34+)

These are documented and researched but deferred beyond PR3:

| Feature | Crate | Why deferred |
|---------|-------|-------------|
| **OxiMedia** (40+ crates) | `oximedia-*` | Born Feb 2026, too new. Watch for stability. Broadcast-grade ambition. |
| **lele AOT compiler** | `lele` | 1.93x faster than ORT but only 4 models supported. Watch for expansion. |
| **burn-vision** | `burn-vision` | GPU image preprocessing. March 2026, needs architecture design. |
| **Invisible watermark** | `trustmark` | Requires ONNX Runtime (~20MB). Opt-in feature for v0.34. |
| **WASM plugin system** | `extism` | Wraps Wasmtime, host+guest SDK. Major architecture change. |
| **Video frame extraction** | `ffmpeg-sidecar` | Requires FFmpeg binary on PATH. Feature-gated. |
| **HEIF/HEIC decode** | `libheif-rs` | C++ dependency, patent concerns. Feature-gated. |
| **RAW camera** | `rawler` | CR2/NEF/ARW decode. Unstable API. |
| **Spreadsheet** | `calamine` + `rust_xlsxwriter` | Read+write xlsx. Useful but niche. |
| **3D models** | `gltf` | glTF 2.0 parsing. Very niche for current use cases. |
| **Animated GIF/WebP** | `gif` + `webp_animation` | Frame-based pattern, needs architecture. |
| **Text overlay** | `cosmic-text` + `ab_glyph` | Watermarks, captions on images. |
| **JPEG XL** | `jxl-rs` 0.4 | Official libjxl team decoder. Watch for encoder support. |
| **jpegli** | `jpegli` 0.1 | Google's next-gen JPEG, 35% better than MozJPEG. Too new. |
| **quantette** | `quantette` | Color quantization, faster than imagequant, no GPL. |
| **ultrahdr-core** | `ultrahdr-core` 0.2 | Ultra HDR gain map by Imageflow team. |
| **QOI intermediate** | `qoi` | 10-20x faster than PNG for pipeline intermediates. |
| **kornia-rs** | `kornia-rs` | Computer vision in Rust. Watch. |

---

## Competitive Position After PR3

```
Capability                     Nika     n8n    Dagger  Temporal  Prefect  Argo
──────────────────────────────────────────────────────────────────────────────
CAS artifact storage           YES      No     No      No        No       No
SIMD thumbnail generation      YES      No*    No      No        No       No
Universal metadata extract     YES      No     No      No        No       No
OCR (pure Rust)                YES      No     No      No        No       No
Chart generation               YES      No     No      No        No       No
QR code validation             YES      No     No      No        No       No
SVG rendering                  YES      No     No      No        No       No
PDF text extraction            YES      No     No      No        No       No
Image optimization             YES      No*    No      No        No       No
C2PA AI provenance             YES      No     No      No        No       No
Perceptual dedup               YES      No     No      No        No       No
ThumbHash placeholders         YES      No     No      No        No       No
Dominant color extraction      YES      No     No      No        No       No
Audio metadata                 YES      No     No      No        No       No
Video metadata                 YES      No     No      No        No       No
Reflink zero-copy              YES      No     No      No        No       No
Verified streaming (bao)       YES      No     No      No        No       No
Feature-gated (lean core)      YES      N/A    N/A     N/A       N/A      N/A

* n8n wraps GraphicsMagick (C dependency), not native
```

**Nika with PR3 = the only workflow engine where AI agents can natively process every media type.** No competitor is even attempting this scope.

---

## Research Files

All research in `docs/research/2026-03-18-*.md`:

| File | Crates | Key Findings |
|------|--------|-------------|
| `media-pipeline-crate-survey.md` | 50+ | Comprehensive initial survey |
| `rust-image-crates-survey.md` | 30+ | image-rs ecosystem, jxl-rs, moxcms |
| `rust-audio-crates-ecosystem.md` | 20+ | symphonia dominance, lofty for tags |
| `rust-video-crates-landscape.md` | 15+ | No pure Rust H.264, mp4 for metadata |
| `rust-document-processing-crates.md` | 25+ | pdf_oxide MCP server, krilla for gen |
| `media-metadata-crates.md` | 20+ | nom-exif unified, moxcms for ICC |
| `rust-media-optimization-crates.md` | 40+ | oxipng, jpegli watch, license safety |
| `rust-wasm-media-crates.md` | 10+ | Extism for plugins, 80-90% native perf |
| `bleeding-edge-rust-media-crates.md` | 20+ | OxiMedia, lele AOT, burn-vision |
| `cas-rust-crates-research.md` | 15+ | DIY CAS confirmed best, ed25519-dalek |
| `special-media-crates-survey.md` | 130+ | ocrs pure Rust, xcap screenshots |
| `steganography-watermark-c2pa-crates.md` | 13+ | C2PA 8.6M, trustmark ICCV 2025 |
| `compression-crates-for-cas.md` | 10+ | zstd wins, skip pre-compressed |
| `rust-color-rendering-crates.md` | 15+ | tiny-skia, palette, cosmic-text |
| `rust-data-visualization-crates.md` | 10+ | charts-rs JSON-first, plotters |
| `animated-media-rust-crates.md` | 15+ | Frames universal pattern |
| `rust-ml-inference-crates.md` | 20+ | lele AOT 1.93x, ort + candle |
| `tiny-media-crates.md` | 15+ | thumbhash, qoi, minipng |

---

## Sources

- 22+ research agents (Perplexity AI + crates.io API + GitHub)
- 4 Context7 API doc agents
- 1 codebase exploration agent (full integration map)
- 300+ crates analyzed across 20 domains
- All download counts verified via crates.io API on 2026-03-18
