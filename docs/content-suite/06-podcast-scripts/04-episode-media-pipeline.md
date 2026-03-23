# Episode 4: 24 Media Tools, One Pipeline -- From Import to Provenance

## Metadata

| Field | Value |
|-------|-------|
| **Series** | Building Nika -- A Rust AI Engine from Scratch |
| **Episode** | 04 |
| **Duration** | ~25 minutes |
| **Topics** | CAS, media tools, C2PA provenance, QR validation, pipeline chaining, feature tiers |
| **Guest Suggestions** | An image processing engineer, a C2PA/content provenance expert, a media pipeline architect |
| **Audience** | Developers building media-heavy AI workflows, content authenticity advocates |
| **Prerequisites** | Episodes 1-2 (understanding of invoke: verb) |

---

## Cold Open (30 seconds)

[MUSIC: Visual, cinematic -- like processing frames]

**Host:** You import a photograph. Nika hashes it with blake3, stores it in a sharded content-addressable store, generates a thumbnail using SIMD-accelerated Lanczos3 resampling, extracts the dominant color palette, creates a 25-byte placeholder hash, renders a chart from its metadata, signs it with C2PA content credentials for EU AI Act compliance, and validates the QR code embedded in it -- all without writing a single temporary file to disk.

[PAUSE]

That is the Nika media pipeline. 24 tools. Three feature tiers. One philosophy: process everything in memory, store everything by hash, prove everything with provenance.

[MUSIC FADES]

---

## Intro (1 minute)

**Host:** Episode 4 of "Building Nika." We have covered what Nika is, how the five verbs work, and the Rust architecture underneath. Today we are going into a feature that most workflow engines do not even attempt: a built-in media processing pipeline.

Why would a YAML workflow engine need image processing? Because modern AI workflows are not text-only. You scrape a webpage and get images. You generate a report and need a chart. You process user uploads and need thumbnails. You publish content and need to prove it was not tampered with.

Nika does not punt this to "install ImageMagick and call it from exec:". It has 24 built-in Rust tools that handle media processing natively, with the same security guarantees as the rest of the engine.

Let us start with how files are stored.

---

## Segment 1: Content-Addressable Storage (8 minutes)

**Host:** At the foundation of the media pipeline is the CAS -- Content-Addressable Storage. This is a simple but powerful idea: instead of naming files by path, you name them by content.

When you import a file into Nika's CAS, here is what happens:

1. The file is read and hashed with blake3 -- a cryptographic hash function designed for speed. On modern hardware, blake3 can hash data at nearly the speed of memcpy.

2. The hash becomes the file's identifier, prefixed with `blake3:`. For example: `blake3:a7f2e8c1d4...`

3. The file is stored in a sharded directory layout: `{root}/{hash[0..2]}/{hash[2..]}`. The first two characters of the hash create a directory, spreading files across 256 subdirectories. This prevents any single directory from having too many entries, which would degrade filesystem performance.

4. The write is atomic via `O_EXCL` -- the operating system guarantees that if two processes try to write the same file simultaneously, only one succeeds and the other gets an error. Since the content determines the path, the result is the same either way. This is automatic deduplication.

5. For files 1 MB or larger, Nika performs a read-back verification -- it reads the stored file and recomputes the hash to confirm the write was successful. This catches silent disk corruption.

[CODE EXAMPLE]
```yaml
# Import a file into CAS
- id: import_photo
  invoke:
    tool: nika:import
    args:
      path: "./photos/sunset.jpg"

# The result contains the hash:
# { "hash": "blake3:a7f2e8c1d4...", "size": 2458624, "mime": "image/jpeg" }
```

[EMPHASIS] Why content-addressable storage instead of just using file paths? Three reasons.

**Deduplication.** If you import the same image twice, it is stored once. The hash is the same, so the path is the same, so the write is a no-op. In a workflow that processes hundreds of images, this saves significant disk space.

**Immutability.** Once a file is in the CAS, it never changes. The hash guarantees this -- if the content changed, the hash would change, and it would be a different entry. This means you can safely reference CAS files across tasks, across runs, even across time. The reference will always point to exactly the content you expect.

**Security.** File paths can be manipulated -- path traversal attacks use `../` to escape intended directories. CAS hashes are just hex strings. There is no path structure to exploit.

The CAS is managed by a `MediaBudget` -- an atomic, lock-free counter that tracks total storage per workflow run, with a default limit of 500 MB. This prevents a runaway workflow from filling your disk.

[PAUSE]

Nika also supports optional zstd compression (via the `media-compression` feature flag) with a 200 MB decompression bomb limit. This prevents a small compressed file from expanding into gigabytes and exhausting memory.

---

## Segment 2: The Three-Tier Tool System (8 minutes)

**Host:** Nika's 24 media tools are organized into three tiers based on dependency weight. This is a deliberate design choice: not every user needs PDF extraction or C2PA provenance, so those capabilities are behind feature flags that keep the binary small when you do not need them.

### Tier 1 -- Always On (5 tools)

These tools have zero or tiny dependencies and are always available:

**`nika:import`** -- Import any file into the CAS. Validates paths against traversal attacks, checks file size before reading (50 MB default limit), computes blake3 hash, and stores atomically.

**`nika:dimensions`** -- Get image width and height in approximately 0.1 milliseconds. It reads only the file headers, not the entire image. This is incredibly fast because most image formats store dimensions in the first few hundred bytes.

**`nika:thumbhash`** -- Generate a 25-byte image placeholder using the ThumbHash algorithm. This is a tiny representation of an image that can be decoded into a blurred preview, perfect for progressive loading in web applications.

**`nika:dominant_color`** -- Extract the dominant color palette from an image. Returns hex colors sorted by prominence.

**`nika:pipeline`** -- Chain multiple operations in memory with zero intermediate files. This is the composition tool:

[CODE EXAMPLE]
```yaml
- id: process
  invoke:
    tool: nika:pipeline
    args:
      hash: "{{with.photo.hash}}"
      steps:
        - op: thumbnail
          width: 800
        - op: convert
          format: webp
        - op: optimize
```

All three operations happen on the in-memory image buffer. No temporary files are written between steps. The final result is stored in the CAS once.

### Tier 2 -- Media Core (6 tools)

These tools are enabled by the `media-core` feature flag, which is on by default:

**`nika:thumbnail`** -- SIMD-accelerated image resize using Lanczos3 resampling. Lanczos3 produces visually excellent results with minimal aliasing artifacts. The SIMD acceleration means this runs at hardware speed on modern CPUs.

**`nika:convert`** -- Format conversion between PNG, JPEG, and WebP. Each format has trade-offs: PNG for lossless transparency, JPEG for photographic compression, WebP for modern web delivery.

**`nika:strip`** -- Remove all metadata from an image by decoding it completely and re-encoding. This is the nuclear option for metadata stripping -- not just removing EXIF tags, but eliminating any data that is not pixel data. Critical for privacy-sensitive workflows.

**`nika:metadata`** -- Universal metadata extraction for images, audio, and video. Returns EXIF data, GPS coordinates, camera settings, audio sample rates, video codecs -- everything the file format supports.

**`nika:optimize`** -- Lossless PNG optimization via oxipng. Reduces file size without changing any pixel data by finding optimal compression parameters.

**`nika:svg_render`** -- SVG to PNG rasterization via resvg. This is important for workflows that generate SVG content (charts, diagrams) and need raster output.

### Tier 3 -- Opt-In (13 tools)

These tools have heavier dependencies and are enabled individually:

[EMPHASIS] Let me highlight the most interesting ones.

**`nika:provenance` and `nika:verify`** -- C2PA content credentials. C2PA (Coalition for Content Provenance and Authenticity) is a standard for digitally signing media with metadata about its origin and editing history. This is becoming legally required under the EU AI Act for AI-generated content. Nika can sign images with C2PA manifests and verify existing manifests.

[CODE EXAMPLE]
```yaml
# Sign an image with C2PA provenance
- id: sign
  invoke:
    tool: nika:provenance
    args:
      hash: "{{with.generated_image.hash}}"
      claim:
        generator: "Nika Workflow Engine"
        actions:
          - type: "c2pa.created"
            description: "AI-generated image"

# Verify C2PA manifest
- id: verify
  invoke:
    tool: nika:verify
    args:
      hash: "{{with.image.hash}}"
```

**`nika:qr_validate`** -- QR code decode and scan quality scoring. Returns a 0-100 scan score using the qrcode-ai-scanner-core library. This is directly relevant to QR Code AI, the product that Nika is built for.

**`nika:chart`** -- Generate bar, line, and pie charts from JSON data. Returns a PNG stored in the CAS. Useful for data visualization in automated reports.

**`nika:phash` and `nika:compare`** -- Perceptual image hashing and comparison. Unlike cryptographic hashes (which change completely if a single pixel changes), perceptual hashes are similar for visually similar images. This enables detecting duplicate or near-duplicate images.

**`nika:quality`** -- Image quality assessment using DSSIM (Structural Dissimilarity) and SSIM metrics. Compare a processed image to the original and get a quantitative quality score.

**`nika:pdf_extract`** -- PDF text extraction for feeding document content to LLMs.

**`nika:html_to_md`, `nika:css_select`, `nika:extract_metadata`, `nika:extract_links`, `nika:readability`** -- These five web content tools bridge the fetch and media pipelines. They process HTML into structured data: clean Markdown, CSS-selected fragments, OpenGraph/Twitter Card metadata, classified link lists, and Readability-extracted article content. They are the same engines that power the nine `extract:` modes of the `fetch:` verb, but exposed as standalone tools for agents to call.

### A Real-World Pipeline Example

Let me show you what a complete media workflow looks like. Imagine you are building a product catalog:

[CODE EXAMPLE]
```yaml
schema: nika/workflow@0.12

tasks:
  # Import the product photo
  - id: import
    invoke:
      tool: nika:import
      args:
        path: "./photos/product-001.jpg"

  # Run a pipeline: thumbnail + WebP conversion + optimization
  - id: optimize
    depends_on: [import]
    with:
      photo: "$import"
    invoke:
      tool: nika:pipeline
      args:
        hash: "{{with.photo.hash}}"
        steps:
          - op: thumbnail
            width: 1200
            height: 800
          - op: convert
            format: webp
          - op: optimize

  # Generate a placeholder for progressive loading
  - id: placeholder
    depends_on: [import]
    with:
      photo: "$import"
    invoke:
      tool: nika:thumbhash
      args:
        hash: "{{with.photo.hash}}"

  # Extract color palette for UI theming
  - id: colors
    depends_on: [import]
    with:
      photo: "$import"
    invoke:
      tool: nika:dominant_color
      args:
        hash: "{{with.photo.hash}}"

  # Use vision to describe the product
  - id: describe
    depends_on: [import]
    with:
      photo: "$import"
    infer:
      content:
        - type: image
          source: "{{with.photo.hash}}"
        - type: text
          text: "Write a compelling product description for this item"
```

[EMPHASIS] Notice: `optimize`, `placeholder`, `colors`, and `describe` all depend only on `import`. They run in parallel automatically. The DAG figures it out. And the media pipeline operations (optimize, placeholder, colors) run alongside the LLM vision call -- no reason to wait for one when the other is independent.

---

## Segment 3: Security in the Media Pipeline (5 minutes)

**Host:** The media pipeline has its own security considerations, and Nika addresses each one explicitly.

[EMPHASIS] Rule number one: never use `image::load_from_memory()` directly.

The Rust `image` crate's default decoder has no resource limits. A specially crafted image file could decompress into gigabytes of memory, crashing the process. Nika uses `decode_image_safe()` -- a wrapper that sets explicit Limits on image dimensions and memory allocation before decoding.

**SVG Sanitization.** SVGs are XML documents that can contain JavaScript, external entity references, and other potentially dangerous content. Nika always calls `sanitize_svg()` BEFORE parsing any SVG with the usvg library. The sanitizer strips scripts, external references, and embedded objects.

**Path Traversal Protection.** The `nika:import` tool validates all file paths against traversal attacks. A path like `../../etc/passwd` is rejected. The `validate_import_path()` function ensures the resolved path stays within the intended directory.

**Size Limits.** All imports are pre-checked against a configurable file size limit (default 50 MB) BEFORE reading the file into memory. This prevents a workflow from attempting to process a 10 GB file.

**Timeouts.** All media operations have a 30-second default timeout. A hanging image decode does not block the entire workflow.

**Decompression Bomb Protection.** When using zstd compression, decompression is capped at 200 MB. A small compressed file that expands into gigabytes is detected and rejected.

**MediaBudget Enforcement.** The per-run budget of 500 MB is tracked atomically using lock-free operations. When any tool attempts to store a result in the CAS, the budget is checked first. If the budget would be exceeded, the tool call fails with a clear error (NIKA-283: MediaSizeError) rather than silently filling the disk. This is important for agent loops where a poorly configured agent might attempt to process thousands of images -- the budget acts as a safety valve.

[CODE EXAMPLE]
```rust
// WRONG: No size limits, vulnerable to decompression bombs
let img = image::load_from_memory(&bytes)?;

// RIGHT: Safe decoding with explicit limits
let img = decode_image_safe(&bytes, &media_limits)?;
```

---

## Wrap-up & Preview (2 minutes)

**Host:** The media pipeline is one of those features that sounds simple -- "just process some images" -- but gets complex fast when you think about security, performance, and correctness.

Nika's approach: content-addressable storage with blake3 for deduplication and immutability. SIMD-accelerated processing with zero temporary files. Three feature tiers to keep binaries lean. C2PA provenance for legal compliance. And a security model that treats every input as potentially hostile.

24 tools, organized into Tier 1 (always on, zero deps), Tier 2 (media-core, default on), and Tier 3 (opt-in for heavy capabilities). All accessible through the `invoke:` verb with the `nika:` prefix.

[PAUSE]

Next episode is about security -- not just in the media pipeline, but across the entire engine. Command injection protection, Unicode normalization, environment variable validation, and why security in AI workflows is more important than most people think. Episode 5: "Security by Design."

[MUSIC: Outro theme]

---

## Show Notes

### Media Tools Quick Reference

| Tool | Tier | Description |
|------|------|-------------|
| `nika:import` | 1 | Import files into CAS |
| `nika:dimensions` | 1 | Image dimensions (~0.1ms) |
| `nika:thumbhash` | 1 | 25-byte image placeholder |
| `nika:dominant_color` | 1 | Color palette extraction |
| `nika:pipeline` | 1 | Chain operations in-memory |
| `nika:thumbnail` | 2 | SIMD Lanczos3 resize |
| `nika:convert` | 2 | PNG/JPEG/WebP conversion |
| `nika:strip` | 2 | Metadata removal |
| `nika:metadata` | 2 | Universal EXIF/metadata |
| `nika:optimize` | 2 | Lossless PNG optimization |
| `nika:svg_render` | 2 | SVG to PNG rasterization |
| `nika:phash` | 3 | Perceptual hashing |
| `nika:compare` | 3 | Visual comparison |
| `nika:pdf_extract` | 3 | PDF text extraction |
| `nika:chart` | 3 | Chart generation |
| `nika:provenance` | 3 | C2PA signing |
| `nika:verify` | 3 | C2PA verification |
| `nika:qr_validate` | 3 | QR scan scoring |
| `nika:quality` | 3 | Image quality (DSSIM/SSIM) |

### Key Concepts
- **CAS** -- Content-Addressable Storage (blake3 hash, sharded layout)
- **blake3** -- Cryptographic hash at memcpy speed
- **Lanczos3** -- High-quality image resampling algorithm
- **C2PA** -- Coalition for Content Provenance and Authenticity
- **ThumbHash** -- 25-byte image placeholder algorithm
- **DSSIM** -- Structural Dissimilarity Image Metric
- **oxipng** -- Lossless PNG optimizer
- **resvg** -- SVG rendering library

### Security Rules
1. Never `image::load_from_memory()` -- use `decode_image_safe()` with Limits
2. Always `sanitize_svg()` before SVG parsing
3. Always `validate_import_path()` for import paths
4. Pre-check file size before reading into memory
5. 30s timeout on all operations
6. 200 MB decompression bomb limit for zstd
