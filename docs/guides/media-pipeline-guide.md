# Media Pipeline Guide

Nika includes a comprehensive media processing pipeline with 24 builtin tools. These tools handle image import, transformation, analysis, metadata extraction, chart generation, PDF processing, QR code validation, content provenance (C2PA), and more. All media operations go through a Content-Addressable Store (CAS) for efficient, deduplicated storage.

## How the Media Pipeline Works

The media pipeline follows a consistent pattern:

1. **Import** -- Bring a file into the CAS with `nika:import`
2. **Process** -- Apply operations using tools like `nika:thumbnail`, `nika:convert`
3. **Use** -- Reference processed media by hash in downstream tasks

All files are stored by their content hash (SHA-256). This means identical files are never duplicated, and every version is immutable and traceable.

```yaml
schema: nika/workflow@0.12
workflow: media-basics

tasks:
  # Step 1: Import
  - id: import
    invoke:
      tool: "nika:import"
      params:
        path: "./images/photo.jpg"

  # Step 2: Process
  - id: thumb
    depends_on: [import]
    with:
      img: $import
    invoke:
      tool: "nika:thumbnail"
      params:
        hash: "{{with.img.media[0].hash}}"
        width: 200

  # Step 3: Use
  - id: describe
    depends_on: [import]
    with:
      img: $import
    infer:
      content:
        - type: image
          source: "{{with.img.media[0].hash}}"
        - type: text
          text: "Describe this image."
```

## The Content-Addressable Store (CAS)

Media files are stored in `.nika/media/store/`. Each file is named by its SHA-256 hash, making lookups fast and deduplication automatic.

Manage the store with the CLI:

```bash
# List stored media
nika media list

# Show store statistics
nika media stats

# Clean unused files
nika media clean
```

## Tier 1 Tools -- Always Available

These 5 tools are always compiled into Nika with no optional feature flags.

### nika:import

Import any file into the CAS.

```yaml
  - id: import_image
    invoke:
      tool: "nika:import"
      params:
        path: "./images/hero.jpg"
```

**Output:**
```json
{
  "media": [{
    "hash": "sha256:abc123...",
    "size": 245000,
    "mime": "image/jpeg",
    "width": 1920,
    "height": 1080
  }]
}
```

**Security:** File paths are validated against directory traversal attacks. Maximum import size: 50 MB.

### nika:dimensions

Get image dimensions from headers (extremely fast, ~0.1ms).

```yaml
  - id: dims
    depends_on: [import_image]
    with:
      img: $import_image
    invoke:
      tool: "nika:dimensions"
      params:
        hash: "{{with.img.media[0].hash}}"
```

**Output:** `{ "width": 1920, "height": 1080 }`

### nika:thumbhash

Generate a 25-byte placeholder hash that can be decoded into a blurred preview image. Useful for progressive image loading.

```yaml
  - id: placeholder
    depends_on: [import_image]
    with:
      img: $import_image
    invoke:
      tool: "nika:thumbhash"
      params:
        hash: "{{with.img.media[0].hash}}"
```

**Output:** `{ "thumbhash": "1QcSHQRnh493V4dIh4eXh1h4kJUI" }`

### nika:dominant_color

Extract the dominant color palette from an image.

```yaml
  - id: palette
    depends_on: [import_image]
    with:
      img: $import_image
    invoke:
      tool: "nika:dominant_color"
      params:
        hash: "{{with.img.media[0].hash}}"
```

**Output:** `{ "colors": ["#2a4858", "#8ca87c", "#d4c5a9"] }`

### nika:pipeline

Chain multiple operations in memory without writing intermediate files to disk.

```yaml
  - id: process_image
    depends_on: [import_image]
    with:
      img: $import_image
    invoke:
      tool: "nika:pipeline"
      params:
        hash: "{{with.img.media[0].hash}}"
        operations:
          - type: thumbnail
            width: 800
          - type: strip
          - type: convert
            format: webp
          - type: optimize
```

Each operation feeds its output to the next. The final result is stored in CAS.

## Tier 2 Tools -- Media Core

These 6 tools are available by default through the `media-core` feature flag.

### nika:thumbnail

SIMD-accelerated image resize using Lanczos3 interpolation.

```yaml
  - id: resize
    invoke:
      tool: "nika:thumbnail"
      params:
        hash: "{{with.img.media[0].hash}}"
        width: 400            # Target width (height auto-calculated)
        # height: 300         # Or specify height (width auto-calculated)
        # Both: exact resize (may distort)
```

### nika:convert

Convert between image formats.

```yaml
  - id: to_webp
    invoke:
      tool: "nika:convert"
      params:
        hash: "{{with.img.media[0].hash}}"
        format: "webp"        # png | jpeg | webp
```

### nika:strip

Remove all metadata (EXIF, IPTC, XMP) from an image by decoding and re-encoding.

```yaml
  - id: clean
    invoke:
      tool: "nika:strip"
      params:
        hash: "{{with.img.media[0].hash}}"
```

### nika:metadata

Extract comprehensive metadata from images, audio, and video files.

```yaml
  - id: meta
    invoke:
      tool: "nika:metadata"
      params:
        hash: "{{with.img.media[0].hash}}"
```

**Output example:**
```json
{
  "format": "JPEG",
  "width": 4032,
  "height": 3024,
  "exif": {
    "camera": "iPhone 15 Pro",
    "date": "2026-03-20T14:30:00",
    "gps": { "lat": 48.8566, "lon": 2.3522 },
    "iso": 100,
    "aperture": "f/1.78"
  }
}
```

### nika:optimize

Lossless PNG optimization that reduces file size without quality loss.

```yaml
  - id: optimize
    invoke:
      tool: "nika:optimize"
      params:
        hash: "{{with.img.media[0].hash}}"
```

### nika:svg_render

Rasterize SVG files to PNG.

```yaml
  - id: render_svg
    invoke:
      tool: "nika:svg_render"
      params:
        hash: "{{with.svg.media[0].hash}}"
        width: 800
```

## Tier 3 Tools -- Opt-in

These tools require specific feature flags. Check availability with `nika features`.

### nika:phash -- Perceptual Hashing

Generate a perceptual hash that identifies visually similar images.

```yaml
  - id: hash_image
    invoke:
      tool: "nika:phash"
      params:
        hash: "{{with.img.media[0].hash}}"
```

### nika:compare -- Visual Comparison

Compare two images by their perceptual hashes and return a similarity score.

```yaml
  - id: compare
    depends_on: [import_a, import_b]
    with:
      a: $import_a
      b: $import_b
    invoke:
      tool: "nika:compare"
      params:
        hash_a: "{{with.a.media[0].hash}}"
        hash_b: "{{with.b.media[0].hash}}"
```

**Output:** `{ "distance": 3, "similar": true }`

### nika:pdf_extract -- PDF Text Extraction

Extract text content from PDF files.

```yaml
  - id: read_pdf
    invoke:
      tool: "nika:pdf_extract"
      params:
        hash: "{{with.pdf.media[0].hash}}"
```

### nika:chart -- Chart Generation

Create charts from JSON data.

```yaml
  - id: bar_chart
    invoke:
      tool: "nika:chart"
      params:
        type: "bar"           # bar | line | pie
        data:
          labels: ["Q1", "Q2", "Q3", "Q4"]
          values: [120, 340, 250, 410]
        title: "Quarterly Revenue"
        width: 800
        height: 400
```

Chart types:

| Type | Description | Data Format |
|------|-------------|-------------|
| `bar` | Vertical bar chart | `{ labels, values }` |
| `line` | Line chart | `{ labels, values }` or `{ labels, series: [{name, values}] }` |
| `pie` | Pie chart | `{ labels, values }` |

### nika:provenance -- C2PA Content Credentials

Sign images with C2PA (Coalition for Content Provenance and Authenticity) credentials.

```yaml
  - id: sign
    invoke:
      tool: "nika:provenance"
      params:
        hash: "{{with.img.media[0].hash}}"
        claim:
          creator: "Nika Workflow Engine"
          description: "AI-generated content"
```

### nika:verify -- C2PA Verification

Verify C2PA manifests and check EU AI Act compliance.

```yaml
  - id: verify
    invoke:
      tool: "nika:verify"
      params:
        hash: "{{with.signed_img.media[0].hash}}"
```

### nika:qr_validate -- QR Code Validation

Decode QR codes and assess scan quality with a 0-100 score.

```yaml
  - id: check_qr
    invoke:
      tool: "nika:qr_validate"
      params:
        hash: "{{with.qr.media[0].hash}}"
```

**Output:** `{ "decoded": "https://example.com", "score": 85, "version": 3 }`

### nika:quality -- Image Quality Assessment

Assess image quality using DSSIM (Structural Dissimilarity) and SSIM metrics.

```yaml
  - id: assess
    invoke:
      tool: "nika:quality"
      params:
        hash_a: "{{with.original.media[0].hash}}"
        hash_b: "{{with.compressed.media[0].hash}}"
```

### Web Content Tools (Tier 3)

These tools provide the same functionality as `fetch:` extract modes but as standalone invoke targets:

| Tool | Equivalent | Purpose |
|------|-----------|---------|
| `nika:html_to_md` | `extract: markdown` | HTML to Markdown |
| `nika:css_select` | `extract: selector` | CSS selector extraction |
| `nika:extract_metadata` | `extract: metadata` | Page metadata |
| `nika:extract_links` | `extract: links` | Link classification |
| `nika:readability` | `extract: article` | Article extraction |

## Complete Workflow Examples

### Image Processing Pipeline

```yaml
schema: nika/workflow@0.12
workflow: image-pipeline

tasks:
  - id: import
    invoke:
      tool: "nika:import"
      params:
        path: "./photos/hero.jpg"

  - id: original_meta
    depends_on: [import]
    with:
      img: $import
    invoke:
      tool: "nika:metadata"
      params:
        hash: "{{with.img.media[0].hash}}"

  - id: thumb_small
    depends_on: [import]
    with:
      img: $import
    invoke:
      tool: "nika:pipeline"
      params:
        hash: "{{with.img.media[0].hash}}"
        operations:
          - type: thumbnail
            width: 200
          - type: strip
          - type: convert
            format: webp

  - id: thumb_medium
    depends_on: [import]
    with:
      img: $import
    invoke:
      tool: "nika:pipeline"
      params:
        hash: "{{with.img.media[0].hash}}"
        operations:
          - type: thumbnail
            width: 800
          - type: strip
          - type: convert
            format: webp

  - id: placeholder
    depends_on: [import]
    with:
      img: $import
    invoke:
      tool: "nika:thumbhash"
      params:
        hash: "{{with.img.media[0].hash}}"

  - id: colors
    depends_on: [import]
    with:
      img: $import
    invoke:
      tool: "nika:dominant_color"
      params:
        hash: "{{with.img.media[0].hash}}"

  - id: manifest
    depends_on: [original_meta, thumb_small, thumb_medium, placeholder, colors]
    with:
      meta: $original_meta | to_json
      small: $thumb_small | to_json
      medium: $thumb_medium | to_json
      hash: $placeholder | to_json
      palette: $colors | to_json
    exec:
      command: |
        echo '{"metadata": {{with.meta}}, "thumbnails": {"small": {{with.small}}, "medium": {{with.medium}}}, "placeholder": {{with.hash}}, "palette": {{with.palette}}}'
      shell: true
    artifact:
      path: image-manifest.json
      format: json
```

### Vision Analysis Pipeline

Combine media import with LLM vision capabilities:

```yaml
schema: nika/workflow@0.12
workflow: vision-analysis
provider: anthropic

tasks:
  - id: import_photo
    invoke:
      tool: "nika:import"
      params:
        path: "./product.jpg"

  - id: dimensions
    depends_on: [import_photo]
    with:
      img: $import_photo
    invoke:
      tool: "nika:dimensions"
      params:
        hash: "{{with.img.media[0].hash}}"

  - id: describe
    depends_on: [import_photo]
    with:
      img: $import_photo
    infer:
      content:
        - type: image
          source: "{{with.img.media[0].hash}}"
          detail: high
        - type: text
          text: |
            This is a product image. Generate:
            1. A 50-word product description
            2. 5 relevant tags
            3. Suggested category
      temperature: 0.3
    structured:
      schema:
        type: object
        properties:
          description: { type: string }
          tags: { type: array, items: { type: string } }
          category: { type: string }
        required: [description, tags, category]
```

### Data Visualization

Generate charts from computed data:

```yaml
schema: nika/workflow@0.12
workflow: data-viz
provider: anthropic

tasks:
  - id: fetch_data
    fetch:
      url: "https://api.example.com/stats"
      extract: jsonpath
      selector: "$.monthly"

  - id: chart
    depends_on: [fetch_data]
    with:
      data: $fetch_data
    invoke:
      tool: "nika:chart"
      params:
        type: line
        data:
          labels: ["Jan", "Feb", "Mar", "Apr", "May", "Jun"]
          series:
            - name: "Revenue"
              values: [100, 120, 130, 125, 140, 160]
            - name: "Costs"
              values: [80, 85, 90, 88, 92, 95]
        title: "Revenue vs Costs"
        width: 1000
        height: 500
```

### QR Code Quality Check

```yaml
schema: nika/workflow@0.12
workflow: qr-check

tasks:
  - id: import_qr
    invoke:
      tool: "nika:import"
      params:
        path: "./qr-code.png"

  - id: validate
    depends_on: [import_qr]
    with:
      qr: $import_qr
    invoke:
      tool: "nika:qr_validate"
      params:
        hash: "{{with.qr.media[0].hash}}"

  - id: report
    depends_on: [validate]
    with:
      result: $validate
    exec:
      command: |
        echo "QR Code Analysis:"
        echo "  Decoded: {{with.result.decoded}}"
        echo "  Scan Score: {{with.result.score}}/100"
      shell: true
```

## Feature Flags and nika features

To see which media tools are available in your Nika build:

```bash
nika features
```

This shows all compiled feature flags grouped by tier:

```
Nika v0.49.0 -- Compiled Features

Core
  ✓ tui                Terminal UI (ratatui)
  ✓ native-inference   Local GGUF models (mistral.rs)
  ✓ lsp                Language Server Protocol

Media (Tier 2 -- media-core)
  ✓ media-thumbnail    SIMD image resize
  ✓ media-metadata     EXIF/audio metadata
  ✓ media-optimize     Lossless PNG optimization
  ✓ media-svg          SVG to PNG rasterization

Media (Tier 3 -- opt-in)
  ✓ media-phash        Perceptual image hashing
  ✗ media-pdf          PDF text extraction
  ✓ media-chart        Chart generation (bar/line/pie)
  ✗ media-provenance   C2PA content credentials
  ✓ media-qr           QR code validation
  ✗ media-iqa          Image quality assessment (DSSIM)
```

If a feature is not compiled in (`✗`), calling its tools will return a clear error explaining which feature flag is needed.

## Binary Response and Media Pipeline Integration

The media pipeline integrates seamlessly with the `fetch:` verb's `response: binary` mode. This enables workflows that download files from the web and process them through the pipeline:

```yaml
schema: nika/workflow@0.12
workflow: download-and-process

tasks:
  # Download an image from the web
  - id: download
    fetch:
      url: "https://example.com/photo.jpg"
      response: binary

  # Get dimensions
  - id: dimensions
    depends_on: [download]
    with:
      img: $download
    invoke:
      tool: "nika:dimensions"
      params:
        hash: "{{with.img.media[0].hash}}"

  # Create responsive thumbnails
  - id: thumb_small
    depends_on: [download]
    with:
      img: $download
    invoke:
      tool: "nika:pipeline"
      params:
        hash: "{{with.img.media[0].hash}}"
        operations:
          - type: thumbnail
            width: 150
          - type: convert
            format: webp

  - id: thumb_large
    depends_on: [download]
    with:
      img: $download
    invoke:
      tool: "nika:pipeline"
      params:
        hash: "{{with.img.media[0].hash}}"
        operations:
          - type: thumbnail
            width: 600
          - type: convert
            format: webp

  # Generate placeholder
  - id: placeholder
    depends_on: [download]
    with:
      img: $download
    invoke:
      tool: "nika:thumbhash"
      params:
        hash: "{{with.img.media[0].hash}}"

  # Use vision to describe
  - id: describe
    depends_on: [download]
    with:
      img: $download
    provider: anthropic
    infer:
      content:
        - type: image
          source: "{{with.img.media[0].hash}}"
          detail: high
        - type: text
          text: "Write an alt text for this image in 20 words or less."
```

## CAS Storage Details

The Content-Addressable Store uses SHA-256 hashes as filenames:

```
.nika/media/store/
├── sha256:a1b2c3d4e5f6...    # Original photo (2.1 MB)
├── sha256:f6e5d4c3b2a1...    # WebP thumbnail (45 KB)
├── sha256:1a2b3c4d5e6f...    # Another image
└── ...
```

Key properties:
- **Deduplication** -- Identical files are stored once regardless of how many times they are imported
- **Immutability** -- Files are never modified after storage. Operations create new entries
- **Integrity** -- The filename IS the hash, so corruption is immediately detectable
- **Garbage collection** -- Run `nika media clean` to remove unreferenced files

### Managing the Store

```bash
# List all stored files with sizes and MIME types
nika media list

# Show aggregate statistics
nika media stats

# Clean unreferenced files
nika media clean

# Clean with preview (dry run)
nika media clean --dry-run
```

## Error Handling for Media Tools

Media operations can fail for several reasons. Here is how to handle them:

### File Too Large

The default import size limit is 50 MB. If you need to process larger files, this is a security boundary -- consider splitting the file first.

### Unsupported Format

Not all image formats are supported for all operations. Common supported formats: JPEG, PNG, WebP, SVG. If you get a format error, convert first:

```yaml
  - id: convert_first
    invoke:
      tool: "nika:convert"
      params:
        hash: "{{with.img.media[0].hash}}"
        format: "png"

  - id: then_process
    depends_on: [convert_first]
    with:
      png: $convert_first
    invoke:
      tool: "nika:thumbnail"
      params:
        hash: "{{with.png.media[0].hash}}"
        width: 400
```

### Timeout on Large Operations

Media operations default to a 30-second timeout. For very large images or complex pipelines, add explicit timeouts:

```yaml
  - id: large_operation
    invoke:
      tool: "nika:pipeline"
      params:
        hash: "{{with.img.media[0].hash}}"
        operations:
          - type: thumbnail
            width: 8000
      timeout: 60
```

## Best Practices

1. **Import first** -- Always use `nika:import` before other tools. Direct file paths are not accepted by processing tools.
2. **Use pipelines for chains** -- `nika:pipeline` is faster than chaining individual tools because it avoids writing intermediate files
3. **Strip metadata for publishing** -- Remove EXIF data with `nika:strip` before publishing images to protect location privacy
4. **Generate placeholders** -- Use `nika:thumbhash` for progressive loading in web applications
5. **Check dimensions first** -- Use `nika:dimensions` (0.1ms) before expensive operations to skip unnecessary processing
6. **Use WebP for web** -- Convert to WebP with `nika:convert` for 25-35% smaller file sizes
7. **Optimize PNGs** -- `nika:optimize` achieves lossless size reduction (5-30% typically)
