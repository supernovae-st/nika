# Media Processing Recipes

Production-ready workflows for image optimization, thumbnail generation, QR code validation, chart creation, PDF extraction, and content provenance using Nika's 24 built-in media tools.

---

## Media Tool Reference

### Tier 1 -- Always Available (5 tools)

| Tool | Description |
|------|-------------|
| `nika:import` | Import any file into CAS |
| `nika:dimensions` | Image dimensions from headers (~0.1ms) |
| `nika:thumbhash` | 25-byte image placeholder |
| `nika:dominant_color` | Color palette extraction |
| `nika:pipeline` | Chain operations in-memory (zero intermediate files) |

### Tier 2 -- Default (6 tools, `media-core` feature)

| Tool | Description |
|------|-------------|
| `nika:thumbnail` | SIMD-accelerated resize (Lanczos3) |
| `nika:convert` | Format conversion (PNG/JPEG/WebP) |
| `nika:strip` | Remove metadata (decode+re-encode) |
| `nika:metadata` | Universal EXIF/audio/video metadata |
| `nika:optimize` | Lossless PNG optimization (oxipng) |
| `nika:svg_render` | SVG to PNG rasterization (resvg) |

### Tier 3 -- Opt-in (13 tools)

| Tool | Description |
|------|-------------|
| `nika:phash` | Perceptual image hashing |
| `nika:compare` | Visual comparison via perceptual hash |
| `nika:pdf_extract` | PDF text extraction |
| `nika:chart` | Bar/line/pie charts from JSON data |
| `nika:provenance` | C2PA content credentials (sign) |
| `nika:verify` | C2PA manifest verification |
| `nika:qr_validate` | QR decode + 0-100 scan score |
| `nika:quality` | Image quality assessment (DSSIM/SSIM) |
| `nika:html_to_md` | HTML to clean Markdown |
| `nika:css_select` | CSS selector extraction |
| `nika:extract_metadata` | OG, Twitter Cards, JSON-LD |
| `nika:extract_links` | Rich link classification |
| `nika:readability` | Article content extraction |

---

## Recipe 1: Image Optimization Pipeline

**Problem:** You need to download images, generate multiple sizes, strip metadata for privacy, convert to WebP for web delivery, and produce a visual quality report.

**Solution:**

```yaml
schema: "nika/workflow@0.12"
workflow: image-optimization-pipeline
description: "Multi-step image optimization with quality analysis"
provider: anthropic
model: claude-sonnet-4-20250514

artifacts:
  dir: ./output/image-pipeline

tasks:
  # Download the source image
  - id: download_original
    fetch:
      url: "https://picsum.photos/1200/800.jpg"
      response: binary
      timeout: 20

  # Get original dimensions
  - id: original_dims
    depends_on: [download_original]
    with:
      img: $download_original
    invoke:
      tool: "nika:dimensions"
      params:
        hash: "{{with.img.media[0].hash}}"

  # Extract color palette
  - id: color_palette
    depends_on: [download_original]
    with:
      img: $download_original
    invoke:
      tool: "nika:dominant_color"
      params:
        hash: "{{with.img.media[0].hash}}"
        count: 6
    artifact:
      path: colors.json
      format: json

  # Generate thumbnail
  - id: make_thumbnail
    depends_on: [download_original]
    with:
      img: $download_original
    invoke:
      tool: "nika:thumbnail"
      params:
        hash: "{{with.img.media[0].hash}}"
        width: 400
    artifact:
      path: thumbnail.jpg
      format: binary

  # Run the full optimization pipeline (in-memory, zero intermediate files)
  - id: optimize_webp
    depends_on: [download_original]
    with:
      img: $download_original
    invoke:
      tool: "nika:pipeline"
      params:
        hash: "{{with.img.media[0].hash}}"
        steps:
          - op: thumbnail
            width: 800
          - op: strip
          - op: convert
            format: webp
    artifact:
      path: optimized.webp
      format: binary

  # Optimize the thumbnail (lossless PNG optimization)
  - id: optimize_thumb
    depends_on: [make_thumbnail]
    with:
      thumb: $make_thumbnail
    invoke:
      tool: "nika:optimize"
      params:
        hash: "{{with.thumb.media[0].hash}}"
    artifact:
      path: optimized-thumb.png
      format: binary

  # Generate thumbhash placeholder
  - id: placeholder
    depends_on: [download_original]
    with:
      img: $download_original
    invoke:
      tool: "nika:thumbhash"
      params:
        hash: "{{with.img.media[0].hash}}"
    artifact:
      path: thumbhash.json
      format: json

  # Vision-powered quality analysis
  - id: quality_report
    depends_on: [original_dims, color_palette, optimize_webp, optimize_thumb, placeholder]
    with:
      dims: $original_dims
      colors: $color_palette
      optimized: $optimize_webp
      thumb: $optimize_thumb
      hash: $placeholder
    infer:
      content:
        - type: image
          source: "{{with.optimized.media[0].hash}}"
          detail: high
        - type: text
          text: |
            Analyze this optimized image pipeline output:

            ## Original Dimensions
            {{with.dims}}

            ## Color Palette
            {{with.colors}}

            ## Optimization Result
            {{with.optimized}}

            ## Thumbnail
            {{with.thumb}}

            ## ThumbHash Placeholder
            {{with.hash}}

            Write a technical report covering:
            1. Compression ratio achieved
            2. Visual quality assessment of the image above
            3. Color palette analysis and dominant tones
            4. WebP format advantages
            5. ThumbHash placeholder quality
            6. Recommendations for production web delivery
      temperature: 0.3
      max_tokens: 1500
    artifact:
      path: quality-report.md
```

**Explanation:**

This workflow demonstrates the core media pipeline:

1. `fetch: response: binary` downloads the image and stores it in the CAS (content-addressable store), returning a hash reference.
2. `nika:dimensions` reads image dimensions from headers without decoding the full image (~0.1ms).
3. `nika:dominant_color` extracts a 6-color palette.
4. `nika:thumbnail` generates a 400px wide thumbnail using SIMD-accelerated Lanczos3.
5. `nika:pipeline` chains three operations in-memory (no intermediate files): resize to 800px, strip EXIF metadata, convert to WebP.
6. `nika:optimize` performs lossless PNG optimization via oxipng.
7. `nika:thumbhash` generates a 25-byte placeholder for progressive loading.
8. The final `infer:` task uses vision (`content:` with `type: image`) to let the LLM visually assess the optimized result.

The key pattern is `{{with.img.media[0].hash}}` -- when a task produces binary output, the media hash is available at `.media[0].hash` in the binding.

**Expected Output:** Multiple image variants (thumbnail, WebP, optimized PNG), color palette JSON, thumbhash placeholder, and a visual quality report.

---

## Recipe 2: QR Code Validation Suite

**Problem:** You need to validate QR codes for scan reliability, decode their content, and assess print quality.

**Solution:**

```yaml
schema: "nika/workflow@0.12"
workflow: qr-code-validator
description: "Validate QR codes for scan score, content, and print quality"
provider: anthropic
model: claude-sonnet-4-20250514

artifacts:
  dir: ./output/qr-validation

tasks:
  # Generate a test QR code
  - id: fetch_qr
    fetch:
      url: "https://api.qrserver.com/v1/create-qr-code/?size=400x400&data=https://github.com/supernovae-st/nika"
      response: binary
      timeout: 15

  # Validate the QR code (decode + scan score)
  - id: validate_qr
    depends_on: [fetch_qr]
    with:
      qr: $fetch_qr
    invoke:
      tool: "nika:qr_validate"
      params:
        hash: "{{with.qr.media[0].hash}}"
    artifact:
      path: validation-results.json
      format: json

  # Get QR code dimensions
  - id: qr_dims
    depends_on: [fetch_qr]
    with:
      qr: $fetch_qr
    invoke:
      tool: "nika:dimensions"
      params:
        hash: "{{with.qr.media[0].hash}}"

  # Generate thumbhash for the QR code
  - id: qr_thumbhash
    depends_on: [fetch_qr]
    with:
      qr: $fetch_qr
    invoke:
      tool: "nika:thumbhash"
      params:
        hash: "{{with.qr.media[0].hash}}"

  # Get color analysis
  - id: qr_colors
    depends_on: [fetch_qr]
    with:
      qr: $fetch_qr
    invoke:
      tool: "nika:dominant_color"
      params:
        hash: "{{with.qr.media[0].hash}}"
        count: 4

  # Comprehensive quality report
  - id: quality_report
    depends_on: [validate_qr, qr_dims, qr_thumbhash, qr_colors]
    with:
      validation: $validate_qr
      dims: $qr_dims
      thumbhash: $qr_thumbhash
      colors: $qr_colors
    infer:
      prompt: |
        Analyze these QR code validation results:

        ## Validation
        {{with.validation}}

        ## Dimensions
        {{with.dims}}

        ## ThumbHash
        {{with.thumbhash}}

        ## Color Analysis
        {{with.colors}}

        Produce a quality report:
        1. Scan score interpretation (0-100 scale)
        2. Decoded content verification
        3. Image size assessment for scanning reliability
        4. Color contrast analysis (critical for QR readability)
        5. Print recommendations (minimum size, DPI, quiet zone)
        6. Mobile scanning compatibility
        7. Improvements for higher scan reliability
      temperature: 0.3
      max_tokens: 1500
    structured:
      schema:
        type: object
        properties:
          scan_score:
            type: integer
          decoded_content:
            type: string
          print_ready:
            type: boolean
          recommendations:
            type: array
            items:
              type: string
          minimum_print_size_mm:
            type: integer
        required: [scan_score, decoded_content, print_ready, recommendations]
    artifact:
      path: quality-report.json
      format: json
```

**Explanation:**

The `nika:qr_validate` tool uses `qrcode-ai-scanner-core` to decode the QR code and produce a 0-100 scan score. The score reflects real-world scanning reliability -- taking into account module contrast, quiet zone, error correction level, and image resolution. Combining this with dimension and color analysis gives a complete print-readiness assessment.

**Expected Output:** Validation results JSON and a structured quality report with print recommendations.

---

## Recipe 3: Chart Generation from Data

**Problem:** You need to generate data visualizations as images from raw data, then analyze them with vision.

**Solution:**

```yaml
schema: "nika/workflow@0.12"
workflow: chart-generator
description: "Generate bar and line charts from data, analyze with vision"
provider: anthropic
model: claude-sonnet-4-20250514

artifacts:
  dir: ./output/charts

tasks:
  # Gather data
  - id: gather_metrics
    exec: |
      echo '{"cpu": [65, 72, 58, 81, 45, 90, 73], "memory": [45, 52, 48, 55, 42, 61, 50], "disk": [67, 67, 68, 69, 70, 71, 72], "days": ["Mon", "Tue", "Wed", "Thu", "Fri", "Sat", "Sun"]}'

  # Generate resource usage bar chart
  - id: resource_chart
    invoke:
      tool: "nika:chart"
      params:
        type: "bar"
        title: "System Resource Usage (%)"
        width: 900
        height: 600
        series:
          - name: "CPU"
            data: [65, 72, 58, 81, 45, 90, 73]
          - name: "Memory"
            data: [45, 52, 48, 55, 42, 61, 50]
          - name: "Disk"
            data: [67, 67, 68, 69, 70, 71, 72]
        labels: ["Mon", "Tue", "Wed", "Thu", "Fri", "Sat", "Sun"]
    artifact:
      path: resource-usage.png
      format: binary

  # Generate trend line chart
  - id: trend_chart
    invoke:
      tool: "nika:chart"
      params:
        type: "line"
        title: "Response Time Trend (ms)"
        width: 900
        height: 500
        series:
          - name: "p50"
            data: [120, 115, 130, 125, 118, 112, 108]
          - name: "p95"
            data: [450, 420, 510, 480, 440, 400, 380]
          - name: "p99"
            data: [890, 820, 950, 880, 830, 780, 720]
        labels: ["Mon", "Tue", "Wed", "Thu", "Fri", "Sat", "Sun"]
    artifact:
      path: response-times.png
      format: binary

  # Generate pie chart for traffic distribution
  - id: distribution_chart
    invoke:
      tool: "nika:chart"
      params:
        type: "pie"
        title: "Traffic Distribution by Region"
        width: 700
        height: 700
        series:
          - name: "Traffic"
            data: [35, 28, 22, 10, 5]
        labels: ["North America", "Europe", "Asia Pacific", "Latin America", "Africa"]
    artifact:
      path: traffic-distribution.png
      format: binary

  # Analyze all charts with vision
  - id: visual_analysis
    depends_on: [resource_chart, trend_chart, distribution_chart, gather_metrics]
    with:
      resources: $resource_chart
      trends: $trend_chart
      distribution: $distribution_chart
      raw_data: $gather_metrics
    infer:
      content:
        - type: image
          source: "{{with.resources.media[0].hash}}"
          detail: high
        - type: image
          source: "{{with.trends.media[0].hash}}"
          detail: high
        - type: image
          source: "{{with.distribution.media[0].hash}}"
          detail: high
        - type: text
          text: |
            Analyze these three monitoring charts:

            Raw metrics: {{with.raw_data}}

            1. Resource Usage: Identify peak usage days and capacity risks
            2. Response Times: Assess latency trends and p99 concerns
            3. Traffic Distribution: Geographic optimization opportunities

            Provide an operational summary with recommendations.
      temperature: 0.3
      max_tokens: 2000
    artifact:
      path: visual-analysis.md
```

**Explanation:**

The `nika:chart` tool supports three chart types: `bar`, `line`, and `pie`. Each chart is rendered as a PNG image and stored in the CAS. The final task sends all three charts to a vision-capable LLM using `content:` blocks with `type: image`, enabling visual analysis of the generated charts. The `detail: high` parameter requests maximum visual fidelity.

**Expected Output:** Three chart PNGs (bar, line, pie) plus a vision-powered analysis report.

---

## Recipe 4: PDF Document Processing

**Problem:** You need to extract text from PDF documents, summarize them, and produce structured metadata.

**Solution:**

```yaml
schema: "nika/workflow@0.12"
workflow: pdf-processor
description: "Extract, summarize, and structure PDF document content"
provider: anthropic
model: claude-sonnet-4-20250514

artifacts:
  dir: ./output/pdf-processing

tasks:
  # Download the PDF
  - id: fetch_pdf
    fetch:
      url: "https://www.w3.org/WAI/WCAG21/Techniques/pdf/img/table-word.pdf"
      response: binary
      timeout: 30

  # Extract text from PDF
  - id: extract_text
    depends_on: [fetch_pdf]
    with:
      pdf: $fetch_pdf
    invoke:
      tool: "nika:pdf_extract"
      params:
        hash: "{{with.pdf.media[0].hash}}"
    artifact:
      path: extracted-text.txt

  # Get PDF dimensions (as an image)
  - id: pdf_info
    depends_on: [fetch_pdf]
    with:
      pdf: $fetch_pdf
    invoke:
      tool: "nika:dimensions"
      params:
        hash: "{{with.pdf.media[0].hash}}"

  # Summarize the document
  - id: summarize
    depends_on: [extract_text]
    with:
      text: $extract_text
    infer:
      prompt: |
        Summarize this extracted PDF content:

        {{with.text | first(4000)}}

        Provide:
        1. Document title and type
        2. Executive summary (3-5 sentences)
        3. Key points as bullet list
        4. Notable data or statistics
        5. Target audience assessment
      temperature: 0.3
      max_tokens: 1500
    structured:
      schema:
        type: object
        properties:
          title:
            type: string
          document_type:
            type: string
          summary:
            type: string
          key_points:
            type: array
            items:
              type: string
          statistics:
            type: array
            items:
              type: string
          target_audience:
            type: string
        required: [title, document_type, summary, key_points]
    artifact:
      path: document-summary.json
      format: json

  # Generate a reading guide
  - id: reading_guide
    depends_on: [summarize]
    with:
      summary: $summarize
      raw_text: $extract_text
    infer:
      prompt: |
        Create a reading guide for this document:

        Summary: {{with.summary}}
        Full text preview: {{with.raw_text | first(2000)}}

        Include:
        - Key concepts to understand first
        - Suggested reading order
        - Important sections to focus on
        - Sections to skip if time is limited
        - Related resources to explore
      temperature: 0.4
      max_tokens: 1000
    artifact:
      path: reading-guide.md
```

**Explanation:**

The `nika:pdf_extract` tool extracts raw text from PDF documents. The `first(4000)` transform ensures that even large PDFs do not overflow the LLM's context window. The `structured:` output on the summarize task guarantees consistent metadata extraction across different document types.

**Expected Output:** Extracted text, structured summary JSON, and a reading guide.

---

## Recipe 5: Content Provenance and Verification

**Problem:** You need to sign generated content with C2PA credentials for provenance tracking and verify the integrity of received media.

**Solution:**

```yaml
schema: "nika/workflow@0.12"
workflow: content-provenance
description: "Sign content with C2PA credentials and verify media integrity"
provider: anthropic
model: claude-sonnet-4-20250514

artifacts:
  dir: ./output/provenance

tasks:
  # Download an image to process
  - id: fetch_image
    fetch:
      url: "https://picsum.photos/800/600.jpg"
      response: binary
      timeout: 20

  # Import a local file into CAS
  - id: import_local
    invoke:
      tool: "nika:import"
      params:
        path: "./assets/logo.png"

  # Sign the fetched image with C2PA provenance
  - id: sign_image
    depends_on: [fetch_image]
    with:
      img: $fetch_image
    invoke:
      tool: "nika:provenance"
      params:
        hash: "{{with.img.media[0].hash}}"
    artifact:
      path: signed-image.jpg
      format: binary

  # Verify the signed image
  - id: verify_signed
    depends_on: [sign_image]
    with:
      signed: $sign_image
    invoke:
      tool: "nika:verify"
      params:
        hash: "{{with.signed.media[0].hash}}"
    artifact:
      path: verification-report.json
      format: json

  # Extract metadata from original
  - id: image_metadata
    depends_on: [fetch_image]
    with:
      img: $fetch_image
    invoke:
      tool: "nika:metadata"
      params:
        hash: "{{with.img.media[0].hash}}"
    artifact:
      path: image-metadata.json
      format: json

  # Compare perceptual hashes
  - id: phash_original
    depends_on: [fetch_image]
    with:
      img: $fetch_image
    invoke:
      tool: "nika:phash"
      params:
        hash: "{{with.img.media[0].hash}}"

  # Provenance report
  - id: provenance_report
    depends_on: [verify_signed, image_metadata, phash_original]
    with:
      verification: $verify_signed
      metadata: $image_metadata
      phash: $phash_original
    infer:
      prompt: |
        Generate a content provenance and integrity report:

        C2PA Verification: {{with.verification}}
        Image Metadata (EXIF): {{with.metadata}}
        Perceptual Hash: {{with.phash}}

        Include:
        1. C2PA manifest chain analysis
        2. EU AI Act compliance assessment
        3. Metadata integrity check
        4. Provenance recommendations
        5. Content authenticity score
      temperature: 0.2
      max_tokens: 1500
    artifact:
      path: provenance-report.md
```

**Explanation:**

- `nika:provenance` signs an image with C2PA (Coalition for Content Provenance and Authenticity) credentials, creating an auditable chain of custody.
- `nika:verify` checks the C2PA manifest and provides EU AI Act compliance assessment.
- `nika:phash` generates a perceptual hash that can detect visual similarity even after compression or resizing.
- `nika:metadata` extracts EXIF, IPTC, and XMP metadata.

These tools are essential for media organizations, news outlets, and any application requiring content authenticity verification.

**Expected Output:** Signed image, verification JSON, metadata, and a comprehensive provenance report.

---

## Recipe 6: Batch Image Processing with for_each

**Problem:** You need to process multiple images through the same optimization pipeline.

**Solution:**

```yaml
schema: "nika/workflow@0.12"
workflow: batch-image-processor
description: "Process multiple images through a standardized pipeline"

artifacts:
  dir: ./output/batch-images

tasks:
  # Download multiple images
  - id: download_images
    for_each:
      items:
        - { url: "https://picsum.photos/1200/800.jpg", name: "landscape" }
        - { url: "https://picsum.photos/800/1200.jpg", name: "portrait" }
        - { url: "https://picsum.photos/800/800.jpg", name: "square" }
      as: img
      concurrency: 3
    fetch:
      url: "{{with.img.url}}"
      response: binary
      timeout: 20

  # Process each image through the pipeline
  - id: optimize_all
    depends_on: [download_images]
    with:
      images: $download_images
    for_each:
      items:
        - { index: 0, name: "landscape", width: 600 }
        - { index: 1, name: "portrait", width: 400 }
        - { index: 2, name: "square", width: 500 }
      as: config
      concurrency: 3
    invoke:
      tool: "nika:pipeline"
      params:
        hash: "{{with.images.media[0].hash}}"
        steps:
          - op: thumbnail
            width: "{{with.config.width}}"
          - op: strip
          - op: convert
            format: webp
    artifact:
      path: "optimized/{{with.config.name}}.webp"
      format: binary

  # Generate batch report
  - id: batch_report
    depends_on: [download_images, optimize_all]
    with:
      originals: $download_images
      optimized: $optimize_all
    exec: |
      echo '{
        "processed": 3,
        "formats": ["landscape", "portrait", "square"],
        "output_format": "webp",
        "timestamp": "'$(date -u +%Y-%m-%dT%H:%M:%SZ)'"
      }'
    shell: true
    artifact:
      path: batch-report.json
      format: json
```

**Explanation:**

The `for_each:` pattern enables batch processing. Each image is downloaded in parallel, then each is optimized through the `nika:pipeline` tool with configuration specific to its aspect ratio. The pipeline chains `thumbnail`, `strip`, and `convert` operations in-memory, producing zero intermediate files on disk.

**Expected Output:** Three optimized WebP images and a batch processing report.

---

## Key Patterns for Media Processing

### CAS Hash Flow

All media in Nika flows through the content-addressable store:

```
fetch: response: binary  →  CAS hash  →  nika:pipeline  →  new CAS hash  →  artifact
          ↓                                    ↓
   {{with.img.media[0].hash}}        {{with.result.media[0].hash}}
```

### Pipeline Chaining

Chain multiple operations in a single `nika:pipeline` call:

```yaml
invoke:
  tool: "nika:pipeline"
  params:
    hash: "{{with.img.media[0].hash}}"
    steps:
      - op: thumbnail
        width: 800
      - op: strip          # Remove EXIF
      - op: convert
        format: webp
      - op: optimize       # Lossless optimization
```

### Vision Analysis Pattern

Send media to the LLM for visual analysis:

```yaml
infer:
  content:
    - type: image
      source: "{{with.img.media[0].hash}}"
      detail: high
    - type: text
      text: "Describe this image."
```

### Binary Artifacts

Save media output to files:

```yaml
artifact:
  path: output/image.webp
  format: binary
```
