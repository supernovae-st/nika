# 08 -- Media Pipeline

## Overview

Nika ships 24 builtin media tools organized in 3 tiers, accessible via `invoke: nika:*`. All tools operate on a Content-Addressable Store (CAS) using blake3 hashing for deduplication and integrity.

---

## Architecture

```
invoke: nika:thumbnail
        |
        v
BuiltinToolRouter
        |
        +-- is_builtin("nika_*") = true
        |
        v
MediaToolAdapter
        |
        +-- Parse JSON args
        +-- Timeout wrap (30s default)
        +-- MediaOp::execute()
        +-- CAS write (binary result)
        +-- Serialize -> JSON String
```

### MediaOp Trait

Every media tool implements the `MediaOp` trait. The `MediaToolAdapter` bridges `MediaOp` to `BuiltinTool`, handling argument parsing, timeout enforcement, CAS storage, and serialization.

### MediaToolContext

```rust
pub struct MediaToolContext {
    pub cas: CasStore,
}
```

Shared context providing access to the CAS store. Created once per TaskExecutor and shared across all media tool invocations.

---

## Content-Addressable Store (CAS)

### Location

`.nika/media/store/` relative to the workflow directory. Can also be at `~/.nika/media/store/` for global storage.

### Hashing

Files are stored by their blake3 hash. The hash is computed over the raw file bytes. This provides:

- **Deduplication**: Same content produces same hash, stored once
- **Integrity**: Hash verifies content has not been modified
- **Addressability**: Any CAS reference is a valid hash pointing to stored content

### Operations

```rust
impl CasStore {
    pub fn write(&self, data: &[u8]) -> Result<String, MediaError>;     // Returns hash
    pub fn read(&self, hash: &str) -> Result<Vec<u8>, MediaError>;      // Returns bytes
    pub fn exists(&self, hash: &str) -> bool;
    pub fn workspace_default(working_dir: &Path) -> Self;
}
```

### Reflink Support

On filesystems that support it (APFS, Btrfs, XFS), `reflink-copy` is used for zero-copy file operations.

### Lockfile

During workflow execution, a `.nika-run.lock` file prevents `nika media clean` from garbage-collecting blobs in use. An RAII `LockfileGuard` ensures the lockfile is removed even on errors or panics.

---

## Tier 1 -- Always-On (5 Tools)

These tools have zero feature gates and are always available.

### nika:import

Import any file into CAS.

```yaml
invoke:
  tool: nika:import
  params:
    path: ./images/photo.jpg
```

**Security:**
- Path validation against traversal attacks (`../`, symlinks)
- Pre-read size check (50 MB default limit)
- MIME type detection via magic bytes

**Returns:** `{ "hash": "blake3_hash", "size": 12345, "mime": "image/jpeg" }`

### nika:dimensions

Read image dimensions from headers without decoding the full image.

```yaml
invoke:
  tool: nika:dimensions
  params:
    hash: "abc123..."
```

**Performance:** ~0.1ms (reads only header bytes via `imagesize` crate).

**Returns:** `{ "width": 1920, "height": 1080 }`

### nika:thumbhash

Generate a 25-byte image placeholder hash for lazy loading.

```yaml
invoke:
  tool: nika:thumbhash
  params:
    hash: "abc123..."
```

**Returns:** `{ "thumbhash": "base64_encoded_25_bytes" }`

### nika:dominant_color

Extract the dominant color palette from an image.

```yaml
invoke:
  tool: nika:dominant_color
  params:
    hash: "abc123..."
    count: 5
```

Uses the `color-thief` crate for palette extraction.

**Returns:** `{ "colors": [{"r": 255, "g": 128, "b": 0}, ...] }`

### nika:pipeline

Chain multiple operations in-memory without intermediate CAS writes.

```yaml
invoke:
  tool: nika:pipeline
  params:
    source: "abc123..."
    operations:
      - op: thumbnail
        width: 800
        height: 600
      - op: optimize
      - op: convert
        format: webp
```

**Performance:** Zero intermediate files. Operations execute in sequence on in-memory buffers. Only the final result is written to CAS.

---

## Tier 2 -- media-core Default (6 Tools)

These tools are enabled by the `media-core` feature (which is default). Each has its own feature flag for granular control.

### nika:thumbnail

SIMD-accelerated image resize using Lanczos3 filter.

**Feature:** `media-thumbnail` (dep: `fast_image_resize`)

```yaml
invoke:
  tool: nika:thumbnail
  params:
    hash: "abc123..."
    width: 400
    height: 300
    filter: lanczos3    # nearest, bilinear, catmullrom, mitchell, lanczos3
```

**Returns:** `{ "hash": "new_hash", "width": 400, "height": 300, "size": 23456 }`

### nika:convert

Format conversion between PNG, JPEG, WebP, and GIF.

**Feature:** `media-thumbnail` (dep: `image`)

```yaml
invoke:
  tool: nika:convert
  params:
    hash: "abc123..."
    format: webp
    quality: 85          # JPEG/WebP quality (1-100)
```

### nika:strip

Remove all metadata from an image (decode + re-encode). Useful for privacy before sharing.

**Feature:** `media-thumbnail` (dep: `image`)

### nika:metadata

Extract universal metadata: EXIF from photos, ID3/Vorbis from audio, container metadata from video.

**Feature:** `media-metadata` (deps: `nom-exif`, `lofty`)

```yaml
invoke:
  tool: nika:metadata
  params:
    hash: "abc123..."
```

**Returns:** JSON object with all discovered metadata fields.

### nika:optimize

Lossless PNG optimization via oxipng. Reduces file size without quality loss.

**Feature:** `media-optimize` (dep: `oxipng`)

```yaml
invoke:
  tool: nika:optimize
  params:
    hash: "abc123..."
    level: 3              # Optimization level (1-6)
```

### nika:svg_render

Rasterize SVG to PNG at specified dimensions.

**Feature:** `media-svg` (deps: `resvg`, `usvg`, `tiny-skia`, `fontdb`)

```yaml
invoke:
  tool: nika:svg_render
  params:
    hash: "abc123..."     # Must be an SVG file
    width: 800
    height: 600
```

**Security:** SVG content is always sanitized via `sanitize_svg()` BEFORE `usvg` parsing to prevent XXE, script injection, and external entity resolution.

---

## Tier 3 -- Opt-In (13 Tools)

These tools have individual feature flags. Most are enabled by default in v0.49.0 except `media-provenance`.

### nika:phash

Perceptual image hashing for content-based matching.

**Feature:** `media-phash` (dep: `image_hasher`)

```yaml
invoke:
  tool: nika:phash
  params:
    hash: "abc123..."
```

**Returns:** `{ "phash": "hex_string_64bit" }`

### nika:compare

Compare two images using perceptual hash distance.

**Feature:** `media-phash`

```yaml
invoke:
  tool: nika:compare
  params:
    hash_a: "abc123..."
    hash_b: "def456..."
```

**Returns:** `{ "distance": 3, "similar": true, "threshold": 10 }`

### nika:pdf_extract

Extract text content from PDF files.

**Feature:** `media-pdf` (dep: `pdf-extract`)

### nika:chart

Generate bar, line, or pie charts from JSON data.

**Feature:** `media-chart` (dep: `charts-rs` with image-encoder)

```yaml
invoke:
  tool: nika:chart
  params:
    type: bar
    data:
      labels: ["Q1", "Q2", "Q3", "Q4"]
      values: [100, 150, 120, 200]
    title: "Quarterly Revenue"
    width: 800
    height: 400
```

### nika:provenance

Sign images with C2PA content credentials for provenance tracking.

**Feature:** `media-provenance` (dep: `c2pa` -- heavy, opt-in only)

```yaml
invoke:
  tool: nika:provenance
  params:
    hash: "abc123..."
    claim_generator: "Nika v0.49.0"
    assertions:
      - label: "c2pa.actions"
        data: { "actions": [{"action": "c2pa.created"}] }
```

### nika:verify

Verify C2PA manifests and check EU AI Act compliance.

**Feature:** `media-provenance`

### nika:qr_validate

Decode QR codes and compute a 0-100 scan score.

**Feature:** `media-qr` (dep: `qrcode-ai-scanner-core`)

```yaml
invoke:
  tool: nika:qr_validate
  params:
    hash: "abc123..."
```

**Returns:** `{ "decoded": "https://example.com", "score": 95, "format": "qr" }`

### nika:quality

Image quality assessment using DSSIM (Structural Dissimilarity) and SSIM metrics.

**Feature:** `media-iqa` (deps: `dssim-core`, `rgb`)

```yaml
invoke:
  tool: nika:quality
  params:
    hash_original: "abc123..."
    hash_modified: "def456..."
```

**Returns:** `{ "dssim": 0.0012, "ssim": 0.998, "quality": "excellent" }`

### nika:html_to_md

Convert HTML content to clean Markdown.

**Feature:** `fetch-markdown` (dep: `htmd`)

### nika:css_select

CSS selector extraction from HTML.

**Feature:** `fetch-html` (dep: `scraper`)

### nika:extract_metadata

Extract OG, Twitter Cards, JSON-LD, and SEO metadata from HTML.

**Feature:** `fetch-html`

### nika:extract_links

Rich link classification: internal/external, navigation/content/footer.

**Feature:** `fetch-html` (deps: `scraper`, `psl`)

### nika:readability

Article content extraction using the Readability algorithm.

**Feature:** `fetch-article` (dep: `dom_smoothie`)

---

## Security Rules

1. **Never use `image::load_from_memory()` directly.** Always use `decode_image_safe()` from `media/safety.rs` which enforces `image::Limits` (max dimensions, max bytes) to prevent decompression bombs.

2. **SVG: always `sanitize_svg()` BEFORE parsing.** SVG content can contain scripts, external entity references, and XXE attacks. The sanitizer strips dangerous elements before `usvg` parses the SVG tree.

3. **Import: validate paths against traversal attacks.** Use `validate_import_path()` to block `../` patterns, symlinks outside the workspace, and absolute paths outside allowed directories.

4. **Pre-read size check.** Always check file size before reading into memory. Default limit: 50 MB.

5. **Timeout: 30s default on all operations.** Every media operation is wrapped in a `tokio::time::timeout()` of 30 seconds to prevent hung operations.

---

## Pipeline Chaining

The `nika:pipeline` tool chains operations without intermediate CAS writes:

```yaml
invoke:
  tool: nika:pipeline
  params:
    source: "{{with.photo.media[0].hash}}"
    operations:
      - op: thumbnail
        width: 1200
      - op: strip          # Remove EXIF data
      - op: optimize       # Lossless compression
      - op: convert
        format: webp
        quality: 90
```

This produces a single CAS entry from the final result. Intermediate buffers are in-memory only.

Available pipeline operations: `thumbnail`, `convert`, `strip`, `optimize`, `svg_render` (any Tier 1-2 operation that transforms image data).

---

## CAS Storage Details

### File Layout

```
.nika/media/store/
  af/
    1349b9f5523e7a...   # First 2 chars as directory, rest as filename
  b5/
    2f9c3d7e1a8b4f...
```

Files are stored without extensions. The hash is the sole identifier. The two-character prefix directory prevents any single directory from accumulating too many entries.

### Framing Protocol

The CAS uses a 4-byte framing header for stored blobs (when `media-compression` feature is enabled):

```
[N][K][FLAG][VERSION]  +  DATA
 |  |   |      |
 |  |   |      +-- 0x00 (current version)
 |  |   +--------- 0x00 (raw) or 0x01 (zstd compressed)
 |  +-------------- Magic bytes: b"NK"
 +----------------- Magic bytes: b"NK"
```

- **Compressed**: `NK\x01\x00` + zstd-compressed data (level 3)
- **Uncompressed**: `NK\x00\x00` + raw data
- **Legacy**: Anything not starting with `NK` is returned as-is (pre-framing compatibility)

### Size Limits

- `MAX_STORE_SIZE`: 100 MB per blob (defense-in-depth)
- `MAX_DECOMPRESS_SIZE`: 200 MB decompressed (prevents zstd decompression bombs)
- `VERIFY_THRESHOLD`: 1 MB (read-back verification only for large files)

### StoreResult

```rust
pub struct StoreResult {
    pub hash: String,       // "blake3:af1349..."
    pub path: PathBuf,      // Absolute path in CAS
    pub size: u64,          // Bytes
    pub deduplicated: bool, // Already existed
    pub verified: bool,     // Read-back check passed
    pub latency_ms: u64,    // Pipeline duration
}
```

### MediaRef

```rust
pub struct MediaRef {
    pub hash: String,          // blake3 hash
    pub mime_type: String,     // MIME type (image/jpeg, etc.)
    pub size: u64,             // Original size in bytes
    pub media_type: MediaType, // Image, Audio, Video, Document, Other
}
```

### MediaBudget

Configurable limits on media storage to prevent runaway workflows:

```rust
pub struct MediaBudget {
    pub max_files: Option<u64>,
    pub max_total_bytes: Option<u64>,
}
```

When the budget is exceeded, new imports produce NIKA-254.

---

## Integration with fetch: binary

When `fetch:` uses `response: binary`, the response body is stored in CAS:

```yaml
- id: download
  fetch:
    url: "https://example.com/image.jpg"
    response: binary

- id: process
  invoke:
    tool: nika:thumbnail
    params:
      hash: "{{with.image}}"
      width: 400
  with:
    image: $download
```

The fetch verb returns the CAS hash, which subsequent media tools use to access the stored content.

---

## Integration with Vision (infer: content:)

CAS hashes are automatically resolved to base64 when used in vision content blocks:

```yaml
- id: import
  invoke:
    tool: nika:import
    params:
      path: ./photos/scene.jpg

- id: describe
  infer:
    content:
      - type: image
        source: "{{with.photo.hash}}"
        detail: high
      - type: text
        text: "Describe this image"
  with:
    photo: $import
```

The executor reads the CAS blob, detects the image format via magic bytes, encodes to base64, and sends to the LLM API. Paths never leak to LLM APIs.

---

## Media CLI

```bash
nika media list              # List CAS entries
nika media stats             # Storage statistics
nika media clean             # Garbage collect unused blobs
nika media clean --force     # Override lockfile protection
```

The `clean` command removes CAS entries that are not referenced by any recent workflow trace. The lockfile prevents GC during active workflow execution.
