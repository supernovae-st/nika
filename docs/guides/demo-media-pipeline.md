# Demo: 24 Media Tools in One Workflow

> 10-minute demo video
> Target audience: developers working with images, media processing, content pipelines
> Format: screen recording with voice-over, showing terminal + file outputs
> Resolution: 1920x1080, terminal font JetBrains Mono 16pt

---

## Pre-Recording Setup

- Sample image file ready (a high-resolution photo, ideally a landscape or product shot)
- Nika installed with media-core feature enabled (default)
- A directory with mixed media files for the pipeline demo
- Image viewer application ready for showing outputs
- CAS directory clean (or show existing state)

---

## CHAPTER 1: The Media Architecture (0:00 - 1:30)

### Scene 1.1 -- Title and Overview (0:00 - 0:30)

[SLIDE] Title card: "24 Media Tools in One Workflow"
[SUBTITLE] "Content-Addressable Storage, SIMD Processing, and Zero Intermediate Files"

**Voice-over:**
Most media processing in AI workflows is an afterthought. Download an image, shell out to ImageMagick, hope the path is right. Nika takes a different approach: twenty-four builtin media tools, a content-addressable storage system, and in-memory pipeline chaining. Today I will show you how to build a complete media processing pipeline -- from import to provenance signing -- in a single workflow.

### Scene 1.2 -- The Three Tiers (0:30 - 1:30)

[SLIDE] Three-tier diagram:

```
TIER 1 -- Always-on (5 tools, zero feature flags)
  import  |  dimensions  |  thumbhash  |  dominant_color  |  pipeline

TIER 2 -- media-core default (6 tools)
  thumbnail  |  convert  |  strip  |  metadata  |  optimize  |  svg_render

TIER 3 -- Opt-in (13 tools)
  phash  |  compare  |  pdf_extract  |  chart  |  provenance  |  verify
  qr_validate  |  quality  |  html_to_md  |  css_select
  extract_metadata  |  extract_links  |  readability
```

**Voice-over:**
Twenty-four tools organized in three tiers. Tier 1 is always available -- five tools that work without any feature flags. Import files, read dimensions, generate thumbhash placeholders, extract dominant colors, and chain operations with the pipeline tool.

Tier 2 ships with the default `media-core` feature -- six tools for everyday image processing. SIMD-accelerated thumbnails, format conversion, metadata stripping, lossless PNG optimization, and SVG rasterization.

Tier 3 is opt-in for specialized use cases. Perceptual hashing for deduplication. C2PA provenance for content authenticity. QR code validation. Image quality assessment. PDF text extraction. And web content extraction tools.

All of them are accessible through the `invoke:` verb with the `nika:` prefix. No external binaries. No shell commands. Pure Rust.

---

## CHAPTER 2: Content-Addressable Storage (1:30 - 3:00)

### Scene 2.1 -- Importing a File (1:30 - 2:15)

[SCREEN] Terminal. Create a new workflow file.

[TYPE] `vim import-demo.nika.yaml` (or show in VS Code)

```yaml
schema: nika/workflow@0.12

tasks:
  - id: import_photo
    invoke:
      tool: nika:import
      input:
        path: ./photos/landscape.jpg
```

[ZOOM] On `nika:import` -- explain the tool naming convention.

**Voice-over:**
Everything starts with import. The `nika:import` tool takes a file path and brings it into the content-addressable storage system. Let me run this.

[TYPE] `nika run import-demo.nika.yaml`

[SCREEN] Output shows:
```json
{
  "hash": "3a7f2b4c8d...",
  "mime": "image/jpeg",
  "size": 2847392,
  "width": 4032,
  "height": 3024
}
```

[ZOOM] On the hash value.

**Voice-over:**
The output is a MediaRef -- a hash, MIME type, size, and dimensions. That hash is a blake3 digest of the file contents. The file is now stored in `.nika/cas/` compressed with zstd. Import the same file again? Same hash, no duplicate storage. Every subsequent tool references this hash, not a file path. Paths do not leak between tasks. Hashes are the universal reference.

### Scene 2.2 -- How CAS Works (2:15 - 3:00)

[SCREEN] Show the CAS directory structure:

[TYPE] `tree .nika/cas/ | head -20`

[SCREEN] Output:
```
.nika/cas/
  3a/
    7f/
      3a7f2b4c8d...
```

**Voice-over:**
The CAS uses a two-level directory structure based on the first four characters of the hash. This prevents any single directory from having thousands of files. The file itself is zstd-compressed -- smaller on disk, decompressed on demand.

[SLIDE] Animation showing:
```
Import:   file.jpg  -->  blake3  -->  "3a7f2b..."  -->  zstd  -->  .nika/cas/3a/7f/3a7f2b...
Retrieve: "3a7f2b..."  -->  locate  -->  zstd decompress  -->  image bytes in memory
```

**Voice-over:**
Security is built in at the import layer. Path traversal validation prevents `../../etc/passwd` attacks. Pre-read size checks enforce a fifty-megabyte default limit before any bytes are read into memory. The `validate_import_path()` function runs before the file is even opened.

---

## CHAPTER 3: Building the Pipeline (3:00 - 7:00)

### Scene 3.1 -- Thumbnail Generation (3:00 - 3:45)

[SCREEN] Edit the workflow to add a thumbnail task:

[TYPE]
```yaml
  - id: make_thumb
    depends_on: [import_photo]
    with: { photo: $import_photo }
    invoke:
      tool: nika:thumbnail
      input:
        hash: "{{with.photo.hash}}"
        width: 400
        height: 300
        fit: cover
```

[ZOOM] On the `{{with.photo.hash}}` template -- show how the hash flows from import to thumbnail.

**Voice-over:**
Now we generate a thumbnail. The `nika:thumbnail` tool uses SIMD-accelerated Lanczos3 resampling -- the same algorithm used in professional image editors, but running on your CPU's vector instructions. We pass the hash from the import step through a binding, specify target dimensions, and set the fit mode to `cover` for a cropped fill.

[TYPE] `nika run import-demo.nika.yaml`

[SCREEN] Show both tasks completing. The thumbnail task output includes a new hash.

**Voice-over:**
New hash. The thumbnail is a new CAS entry. The original is untouched. Every operation is non-destructive.

### Scene 3.2 -- Format Conversion and Optimization (3:45 - 4:30)

[SCREEN] Add more tasks:

[TYPE]
```yaml
  - id: to_webp
    depends_on: [make_thumb]
    with: { thumb: $make_thumb }
    invoke:
      tool: nika:convert
      input:
        hash: "{{with.thumb.hash}}"
        format: webp
        quality: 85

  - id: optimize_png
    depends_on: [import_photo]
    with: { photo: $import_photo }
    invoke:
      tool: nika:optimize
      input:
        hash: "{{with.photo.hash}}"
```

[ZOOM] On the parallel structure -- `to_webp` depends on `make_thumb`, but `optimize_png` depends on `import_photo` directly. They can run in parallel.

**Voice-over:**
Two more operations. Convert the thumbnail to WebP at 85% quality. And separately, run lossless PNG optimization on the original via oxipng. Notice the DAG structure: `to_webp` depends on the thumbnail, but `optimize_png` depends on the original import. They have no dependency between them, so Nika runs them in parallel. Your media pipeline gets automatic parallelism from the DAG.

### Scene 3.3 -- Metadata and Analysis (4:30 - 5:30)

[SCREEN] Add analysis tasks:

[TYPE]
```yaml
  - id: read_meta
    depends_on: [import_photo]
    with: { photo: $import_photo }
    invoke:
      tool: nika:metadata
      input:
        hash: "{{with.photo.hash}}"

  - id: colors
    depends_on: [import_photo]
    with: { photo: $import_photo }
    invoke:
      tool: nika:dominant_color
      input:
        hash: "{{with.photo.hash}}"
        count: 5

  - id: placeholder
    depends_on: [import_photo]
    with: { photo: $import_photo }
    invoke:
      tool: nika:thumbhash
      input:
        hash: "{{with.photo.hash}}"
```

**Voice-over:**
Three analysis tasks, all depending only on the original import, all running in parallel. `nika:metadata` extracts EXIF data -- camera model, GPS coordinates, ISO, shutter speed. Universal across JPEG, PNG, TIFF, and more. `nika:dominant_color` extracts a five-color palette. `nika:thumbhash` generates a twenty-five-byte placeholder hash that can render a blurred preview in the browser -- no network request needed for the initial page load.

[TYPE] `nika run import-demo.nika.yaml`

[SCREEN] Show all tasks completing. Highlight the parallel execution -- metadata, colors, and placeholder all start at the same time.

[ZOOM] On the metadata output showing EXIF fields. Then on the color palette. Then on the thumbhash string.

**Voice-over:**
All three started simultaneously. Metadata shows the camera details. The color palette gives us five hex values. And the thumbhash is a compact string that renders to a blurred placeholder image. Six tasks ran. Three in parallel. Total time: under a second for local processing.

### Scene 3.4 -- The Pipeline Tool (5:30 - 6:15)

[SCREEN] Show the `nika:pipeline` tool:

[TYPE]
```yaml
  - id: one_shot
    depends_on: [import_photo]
    with: { photo: $import_photo }
    invoke:
      tool: nika:pipeline
      input:
        hash: "{{with.photo.hash}}"
        steps:
          - operation: thumbnail
            width: 800
            height: 600
          - operation: convert
            format: webp
            quality: 90
          - operation: strip
```

[ZOOM] On the `steps:` array -- three operations chained.

**Voice-over:**
For simpler cases, the `nika:pipeline` tool chains operations in memory. Thumbnail, convert to WebP, strip metadata -- three operations, zero intermediate files. The image data passes through each step in memory, only hitting disk once at the end when the final result gets stored in CAS. This is significantly faster than three separate tasks when you do not need branching.

### Scene 3.5 -- Advanced: Provenance and Quality (6:15 - 7:00)

[SCREEN] Add provenance and quality tasks:

[TYPE]
```yaml
  - id: sign
    depends_on: [to_webp]
    with: { image: $to_webp }
    invoke:
      tool: nika:provenance
      input:
        hash: "{{with.image.hash}}"
        claim_generator: "Nika Media Pipeline"
        assertions:
          - label: c2pa.actions
            data:
              actions:
                - action: c2pa.created
                  softwareAgent: "Nika v0.42.0"

  - id: check_quality
    depends_on: [to_webp, import_photo]
    with: { original: $import_photo, processed: $to_webp }
    invoke:
      tool: nika:quality
      input:
        reference_hash: "{{with.original.hash}}"
        distorted_hash: "{{with.processed.hash}}"
```

**Voice-over:**
Two advanced tools. `nika:provenance` signs the image with C2PA content credentials -- this is the standard for content authenticity that the EU AI Act references. We are recording that this image was created by our pipeline, with a specific software agent and action type. The signed manifest embeds directly in the file.

`nika:quality` runs image quality assessment using DSSIM -- structural dissimilarity. It compares the processed image against the original and returns a score. Perfect for automated quality gates in production pipelines.

---

## CHAPTER 4: The Full Pipeline DAG (7:00 - 8:30)

### Scene 4.1 -- Visualizing the Pipeline (7:00 - 7:45)

[SCREEN] Run `nika check import-demo.nika.yaml` to show the DAG structure.

[ANIMATION] Draw the complete DAG:

```
                 [import_photo]
                /    |    |    \
               /     |    |     \
   [make_thumb] [read_meta] [colors] [placeholder] [optimize_png]
       |
   [to_webp]
    /      \
[sign] [check_quality]
```

**Voice-over:**
Look at the full DAG. Nine tasks. One import feeds five parallel branches. Thumbnail feeds WebP conversion. WebP feeds provenance signing and quality assessment. Metadata extraction, color analysis, and thumbhash generation all run independently. Maximum parallelism, zero manual configuration.

### Scene 4.2 -- Running in the TUI (7:45 - 8:30)

[SCREEN] Launch `nika ui`, load the workflow, run it in Monitor view.

[SCREEN] Show the live DAG with nodes lighting up:
1. import_photo starts and completes (green)
2. Five tasks launch simultaneously (all turn blue)
3. As each completes, dependent tasks launch
4. Final tasks complete

[ZOOM] On the timing display -- show parallel execution saving time.

**Voice-over:**
Watch it in the TUI. Import completes. Five tasks launch simultaneously. The thumbnail path feeds into conversion, which feeds into signing and quality check. The metadata, colors, and thumbhash paths complete independently. Nine tasks, executed in four layers, total time dominated by the longest chain -- not the sum of all tasks. That is the power of DAG-based execution for media processing.

---

## CHAPTER 5: Integration with AI (8:30 - 10:00)

### Scene 5.1 -- Vision + Media Pipeline (8:30 - 9:15)

[SCREEN] Show a combined workflow:

[TYPE]
```yaml
schema: nika/workflow@0.12

tasks:
  - id: import
    invoke:
      tool: nika:import
      input:
        path: ./product-photo.jpg

  - id: describe
    depends_on: [import]
    with: { photo: $import }
    infer:
      model: claude/claude-sonnet-4-20250514
      content:
        - type: image
          source: "{{with.photo.hash}}"
          detail: high
        - type: text
          text: "Describe this product photo for an e-commerce listing."

  - id: thumbnail
    depends_on: [import]
    with: { photo: $import }
    invoke:
      tool: nika:thumbnail
      input:
        hash: "{{with.photo.hash}}"
        width: 800
        height: 800
        fit: contain

  - id: listing
    depends_on: [describe, thumbnail]
    with: { description: $describe, thumb: $thumbnail }
    infer:
      model: claude/claude-haiku-3-5-20241022
      prompt: |
        Product description: {{with.description}}
        Thumbnail generated at: {{with.thumb.hash}}

        Write a JSON product listing with title, description, and tags.
      output:
        format: json
        schema:
          type: object
          properties:
            title: { type: string }
            description: { type: string }
            tags: { type: array, items: { type: string } }
          required: [title, description, tags]
```

[ZOOM] On the `content:` block in the describe task -- show how CAS hashes flow into vision.

**Voice-over:**
This is where media processing meets AI. Import a product photo. In parallel: send it to Claude for vision analysis and generate a thumbnail. Then combine both results into a structured product listing with JSON output validation. The vision content block takes a CAS hash directly -- Nika resolves it to base64 automatically. Paths never leak to the LLM API. The hash is the contract.

### Scene 5.2 -- Production Patterns (9:15 - 9:45)

[SLIDE] Show three production patterns:

```
Pattern 1: Batch Processing
  fetch (download) -> import -> pipeline -> sign

Pattern 2: Quality Gate
  import -> process -> quality -> (pass/fail branching via agent:)

Pattern 3: Content Catalog
  import -> [metadata, colors, thumbhash, describe] -> catalog entry
```

**Voice-over:**
Three production patterns. Batch processing: fetch images from a URL, import into CAS, run the pipeline, sign with provenance. Quality gate: process an image, run quality assessment, use an agent to decide if the result meets standards. Content catalog: extract everything about an image in parallel -- metadata, colors, placeholder, AI description -- and combine into a catalog entry.

### Scene 5.3 -- Closing (9:45 - 10:00)

[SLIDE] Summary card:

```
24 media tools  |  3 tiers  |  Content-addressable storage
blake3 hashing  |  zstd compression  |  Zero intermediate files
SIMD processing |  C2PA provenance  |  Parallel DAG execution
```

**Voice-over:**
Twenty-four tools. Three tiers. Content-addressable storage. SIMD-accelerated processing. C2PA provenance for content authenticity. And the DAG gives you automatic parallelism without writing a single line of orchestration code. This is media processing for the AI era.

[TITLE CARD] "Nika -- 24 Media Tools. One Workflow."

---

## Post-Production Notes

- Prepare sample images with rich EXIF data for the metadata demo
- Show actual file sizes before and after optimization
- Include a brief visual comparison of original vs thumbnail vs WebP
- Side-by-side: file browser showing CAS directory structure
- Consider a time-lapse of the full pipeline running in the TUI
- Terminal color scheme should make JSON output readable
- Caption all hash values (they are long and hard to read on screen)
