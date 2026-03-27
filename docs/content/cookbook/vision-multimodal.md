# Vision and Multimodal Recipes

Production-ready workflows for image analysis, document OCR, visual QA, product photo processing, and multimodal content generation using Nika's vision support.

---

## Vision Architecture

The `infer:` verb supports multimodal `content:` blocks for sending images to vision-capable LLMs:

```yaml
infer:
  content:
    - type: image
      source: "{{with.img.media[0].hash}}"   # CAS hash -> base64 automatically
      detail: high                             # high | low | auto
    - type: text
      text: "Describe this image."
```

### Key Rules

- `prompt:` is optional when `content:` is present (if both: prompt prepended as first text part)
- CAS images are auto-resolved to base64 (file paths never leak to LLM APIs)
- Multiple images can be sent in a single `content:` block
- `detail: high` requests maximum visual fidelity

### Supported Providers

| Provider | Vision Support | API Key |
|----------|---|---|
| Claude (Anthropic) | Full support | ANTHROPIC_API_KEY |
| GPT-4o (OpenAI) | Full support | OPENAI_API_KEY |
| Mistral | Full support | MISTRAL_API_KEY |
| Groq | Full support | GROQ_API_KEY |
| Gemini (Google) | Full support | GEMINI_API_KEY |
| xAI (Grok) | Full support | XAI_API_KEY |
| DeepSeek | Not supported | DEEPSEEK_API_KEY |

---

## Recipe 1: Product Photo Analyzer

**Problem:** You need to analyze product photos for e-commerce -- assessing quality, extracting visual attributes, and generating alt text and descriptions.

**Solution:**

```yaml
schema: "nika/workflow@0.12"
workflow: product-photo-analyzer
description: "Analyze product photos for quality, attributes, and SEO descriptions"
provider: anthropic
model: claude-sonnet-4-20250514

artifacts:
  dir: ./output/product-photos

tasks:
  # Download the product image
  - id: download_photo
    fetch:
      url: "https://picsum.photos/1200/1200.jpg"
      response: binary
      timeout: 20

  # Get image dimensions and metadata
  - id: photo_dims
    depends_on: [download_photo]
    with:
      img: $download_photo
    invoke:
      tool: "nika:dimensions"
      params:
        hash: "{{with.img.media[0].hash}}"

  # Extract color palette
  - id: photo_colors
    depends_on: [download_photo]
    with:
      img: $download_photo
    invoke:
      tool: "nika:dominant_color"
      params:
        hash: "{{with.img.media[0].hash}}"
        count: 8

  # Generate thumbhash for lazy loading
  - id: photo_thumbhash
    depends_on: [download_photo]
    with:
      img: $download_photo
    invoke:
      tool: "nika:thumbhash"
      params:
        hash: "{{with.img.media[0].hash}}"

  # Vision: Analyze the product image
  - id: visual_analysis
    depends_on: [download_photo, photo_dims, photo_colors]
    with:
      img: $download_photo
      dims: $photo_dims
      colors: $photo_colors
    infer:
      content:
        - type: image
          source: "{{with.img.media[0].hash}}"
          detail: high
        - type: text
          text: |
            Analyze this product photo for e-commerce listing:

            Image dimensions: {{with.dims}}
            Color palette: {{with.colors}}

            Provide:
            1. Product identification (what is this product?)
            2. Visual quality score (1-10): sharpness, lighting, composition
            3. Background assessment (clean/busy, color recommendations)
            4. SEO alt text (under 125 characters, descriptive)
            5. Product description (50-100 words, compelling)
            6. Suggested categories and tags
            7. Photography improvement suggestions
      temperature: 0.3
      max_tokens: 1500
    structured:
      schema:
        type: object
        properties:
          product_name:
            type: string
          quality_score:
            type: integer
          alt_text:
            type: string
          description:
            type: string
          categories:
            type: array
            items:
              type: string
          tags:
            type: array
            items:
              type: string
          improvements:
            type: array
            items:
              type: string
        required: [product_name, quality_score, alt_text, description]
    artifact:
      path: product-analysis.json
      format: json

  # Generate optimized variants
  - id: web_variant
    depends_on: [download_photo]
    with:
      img: $download_photo
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
      path: product-web.webp
      format: binary

  - id: thumb_variant
    depends_on: [download_photo]
    with:
      img: $download_photo
    invoke:
      tool: "nika:thumbnail"
      params:
        hash: "{{with.img.media[0].hash}}"
        width: 200
    artifact:
      path: product-thumb.jpg
      format: binary
```

**Explanation:**

This workflow combines vision analysis with media processing:
1. The product photo is downloaded and processed through multiple analysis tools in parallel (dimensions, colors, thumbhash)
2. The vision `infer:` task sends the image with `detail: high` for maximum visual fidelity
3. `structured:` output ensures consistent JSON with product metadata
4. Two optimized variants are generated: a WebP for web delivery and a small thumbnail

The CAS hash flow (`{{with.img.media[0].hash}}`) is the key pattern: binary content is stored once and referenced everywhere by hash.

**Expected Output:** Product analysis JSON, WebP web image, and thumbnail.

---

## Recipe 2: Multi-Image Comparison

**Problem:** You need to compare multiple images side by side -- for example, comparing an original with its optimized version, or comparing product variants.

**Solution:**

```yaml
schema: "nika/workflow@0.12"
workflow: multi-image-comparison
description: "Compare multiple images using vision and perceptual hashing"
provider: anthropic
model: claude-sonnet-4-20250514

artifacts:
  dir: ./output/comparison

tasks:
  # Download two images to compare
  - id: image_a
    fetch:
      url: "https://picsum.photos/800/600.jpg"
      response: binary
      timeout: 20

  - id: image_b
    fetch:
      url: "https://picsum.photos/800/600.jpg"
      response: binary
      timeout: 20

  # Get dimensions for both
  - id: dims_a
    depends_on: [image_a]
    with:
      img: $image_a
    invoke:
      tool: "nika:dimensions"
      params:
        hash: "{{with.img.media[0].hash}}"

  - id: dims_b
    depends_on: [image_b]
    with:
      img: $image_b
    invoke:
      tool: "nika:dimensions"
      params:
        hash: "{{with.img.media[0].hash}}"

  # Perceptual hash for visual similarity
  - id: phash_a
    depends_on: [image_a]
    with:
      img: $image_a
    invoke:
      tool: "nika:phash"
      params:
        hash: "{{with.img.media[0].hash}}"

  - id: phash_b
    depends_on: [image_b]
    with:
      img: $image_b
    invoke:
      tool: "nika:phash"
      params:
        hash: "{{with.img.media[0].hash}}"

  # Vision: Compare both images
  - id: visual_comparison
    depends_on: [image_a, image_b, dims_a, dims_b, phash_a, phash_b]
    with:
      img1: $image_a
      img2: $image_b
      dims1: $dims_a
      dims2: $dims_b
      hash1: $phash_a
      hash2: $phash_b
    infer:
      content:
        - type: image
          source: "{{with.img1.media[0].hash}}"
          detail: high
        - type: image
          source: "{{with.img2.media[0].hash}}"
          detail: high
        - type: text
          text: |
            Compare these two images:

            Image A: {{with.dims1}} | PHash: {{with.hash1}}
            Image B: {{with.dims2}} | PHash: {{with.hash2}}

            Analyze:
            1. Visual similarity (composition, subject, style)
            2. Quality comparison (sharpness, color, noise)
            3. Dimension and format differences
            4. Perceptual hash similarity interpretation
            5. Which image is better for web use and why
            6. Recommendations for image selection
      temperature: 0.3
      max_tokens: 1500
    structured:
      schema:
        type: object
        properties:
          similarity_score:
            type: integer
          preferred_image:
            type: string
            enum: ["A", "B", "both_equal"]
          quality_comparison:
            type: object
            properties:
              image_a_score:
                type: integer
              image_b_score:
                type: integer
            required: [image_a_score, image_b_score]
          differences:
            type: array
            items:
              type: string
          recommendation:
            type: string
        required: [similarity_score, preferred_image, recommendation]
    artifact:
      path: comparison-report.json
      format: json
```

**Explanation:**

Multiple images are sent in a single `content:` block. The LLM sees both images and can compare them visually. The `nika:phash` tool provides a mathematical similarity metric that complements the LLM's visual assessment. Each `type: image` block references a different CAS hash.

**Expected Output:** A structured comparison report with similarity scores and recommendations.

---

## Recipe 3: Chart Analysis Pipeline

**Problem:** You need to generate data visualizations and then use vision to extract insights that the LLM could not derive from raw numbers alone.

**Solution:**

```yaml
schema: "nika/workflow@0.12"
workflow: chart-vision-analysis
description: "Generate charts, then analyze them with vision for visual insights"
provider: anthropic
model: claude-sonnet-4-20250514

artifacts:
  dir: ./output/chart-vision

tasks:
  # Generate multiple charts
  - id: revenue_chart
    invoke:
      tool: "nika:chart"
      params:
        type: "line"
        title: "Monthly Revenue ($K)"
        width: 900
        height: 500
        series:
          - name: "Revenue"
            data: [45, 52, 48, 61, 58, 67, 72, 68, 75, 82, 79, 91]
          - name: "Target"
            data: [50, 55, 60, 65, 70, 75, 80, 85, 90, 95, 100, 105]
        labels: ["Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec"]
    artifact:
      path: revenue.png
      format: binary

  - id: segment_chart
    invoke:
      tool: "nika:chart"
      params:
        type: "pie"
        title: "Revenue by Segment"
        width: 700
        height: 700
        series:
          - name: "Revenue"
            data: [45, 30, 15, 10]
        labels: ["Enterprise", "Pro", "Starter", "Free-to-Paid"]
    artifact:
      path: segments.png
      format: binary

  - id: growth_chart
    invoke:
      tool: "nika:chart"
      params:
        type: "bar"
        title: "Quarter-over-Quarter Growth (%)"
        width: 900
        height: 500
        series:
          - name: "Revenue Growth"
            data: [12, 8, 15, 22]
          - name: "User Growth"
            data: [18, 14, 20, 28]
        labels: ["Q1", "Q2", "Q3", "Q4"]
    artifact:
      path: growth.png
      format: binary

  # Analyze all three charts with vision
  - id: visual_insights
    depends_on: [revenue_chart, segment_chart, growth_chart]
    with:
      revenue: $revenue_chart
      segments: $segment_chart
      growth: $growth_chart
    infer:
      content:
        - type: image
          source: "{{with.revenue.media[0].hash}}"
          detail: high
        - type: image
          source: "{{with.segments.media[0].hash}}"
          detail: high
        - type: image
          source: "{{with.growth.media[0].hash}}"
          detail: high
        - type: text
          text: |
            Analyze these three business charts as a data analyst:

            1. Revenue Trend (line): Monthly revenue vs target
            2. Segment Distribution (pie): Revenue split by customer tier
            3. Growth Rates (bar): QoQ revenue and user growth

            Provide:
            - Visual patterns you notice (trends, outliers, seasonality)
            - Revenue vs target gap analysis
            - Segment health assessment
            - Growth acceleration/deceleration analysis
            - 3 strategic recommendations based on the visual data
            - Risk factors visible in the charts
      temperature: 0.3
      max_tokens: 2500
    artifact:
      path: chart-insights.md
```

**Explanation:**

Three charts are generated in parallel using `nika:chart` (no `depends_on:` between them). The vision analysis task sends all three images in a single prompt, enabling cross-chart pattern recognition. The LLM can identify visual patterns like trend divergence, segment imbalance, and growth acceleration that would be harder to spot from raw numbers.

**Expected Output:** Three chart PNGs and a visual insights report.

---

## Recipe 4: Document Visual Analysis

**Problem:** You need to analyze document images, extracting visual layout information, reading text, and assessing document quality.

**Solution:**

```yaml
schema: "nika/workflow@0.12"
workflow: document-visual-analysis
description: "Analyze document images with vision for OCR and layout assessment"
provider: anthropic
model: claude-sonnet-4-20250514

artifacts:
  dir: ./output/document-analysis

tasks:
  # Download a document image
  - id: fetch_document
    fetch:
      url: "https://httpbin.org/image/png"
      response: binary
      timeout: 20

  # Get image info
  - id: doc_info
    depends_on: [fetch_document]
    with:
      doc: $fetch_document
    invoke:
      tool: "nika:dimensions"
      params:
        hash: "{{with.doc.media[0].hash}}"

  # Extract colors for layout analysis
  - id: doc_colors
    depends_on: [fetch_document]
    with:
      doc: $fetch_document
    invoke:
      tool: "nika:dominant_color"
      params:
        hash: "{{with.doc.media[0].hash}}"
        count: 5

  # Vision: Full document analysis
  - id: analyze_document
    depends_on: [fetch_document, doc_info, doc_colors]
    with:
      doc: $fetch_document
      info: $doc_info
      colors: $doc_colors
    infer:
      content:
        - type: image
          source: "{{with.doc.media[0].hash}}"
          detail: high
        - type: text
          text: |
            Analyze this document image:

            Dimensions: {{with.info}}
            Color palette: {{with.colors}}

            Perform:
            1. OCR: Extract all visible text
            2. Layout Analysis: Header, body, footer zones
            3. Typography: Font sizes, hierarchy, readability
            4. Visual Elements: Images, tables, charts present
            5. Document Type Classification
            6. Quality Assessment: resolution, contrast, readability score
            7. Accessibility: contrast ratio, text size compliance
      temperature: 0.2
      max_tokens: 2000
    structured:
      schema:
        type: object
        properties:
          document_type:
            type: string
          extracted_text:
            type: string
          layout:
            type: object
            properties:
              has_header:
                type: boolean
              has_footer:
                type: boolean
              columns:
                type: integer
            required: [has_header, has_footer]
          quality_score:
            type: integer
          readability_score:
            type: integer
          visual_elements:
            type: array
            items:
              type: string
        required: [document_type, quality_score]
    artifact:
      path: document-analysis.json
      format: json
```

**Explanation:**

Vision analysis combined with Nika's image metadata tools enables comprehensive document processing. The `detail: high` setting is essential for document analysis since text readability depends on image resolution. The structured output provides machine-readable metadata that can feed into downstream document processing pipelines.

**Expected Output:** A structured JSON document analysis with OCR text, layout info, and quality scores.

---

## Recipe 5: Multimodal Content Generation

**Problem:** You need to generate content that responds to visual input -- for example, writing product descriptions from photos or generating social media posts about images.

**Solution:**

```yaml
schema: "nika/workflow@0.12"
workflow: multimodal-content-generator
description: "Generate content from visual input using vision + media tools"
provider: anthropic
model: claude-sonnet-4-20250514

artifacts:
  dir: ./output/multimodal

tasks:
  # Download the source image
  - id: source_image
    fetch:
      url: "https://picsum.photos/1200/800.jpg"
      response: binary
      timeout: 20

  # Analyze and extract features
  - id: image_features
    depends_on: [source_image]
    with:
      img: $source_image
    invoke:
      tool: "nika:dominant_color"
      params:
        hash: "{{with.img.media[0].hash}}"
        count: 6

  # Generate content for multiple platforms from the image
  - id: generate_content
    depends_on: [source_image, image_features]
    with:
      img: $source_image
      colors: $image_features
    for_each:
      - platform: "Instagram"
        style: "Engaging caption with emojis and hashtags. 2-3 sentences."
        max_length: 300
      - platform: "LinkedIn"
        style: "Professional insight about what the image represents. 3-4 sentences."
        max_length: 500
      - platform: "Blog"
        style: "Detailed description suitable as a blog header image description. Include alt text."
        max_length: 200
      - platform: "E-commerce"
        style: "Product-focused description emphasizing visual qualities. Include color names."
        max_length: 400
    as: platform
    concurrency: 4
    infer:
      content:
        - type: image
          source: "{{with.img.media[0].hash}}"
          detail: high
        - type: text
          text: |
            Generate {{with.platform.platform}} content for this image.
            Color palette: {{with.colors}}

            Style: {{with.platform.style}}
            Max length: {{with.platform.max_length}} characters.

            Incorporate the color information into your description.
      temperature: 0.7
      max_tokens: 500

  # Compile all content
  - id: content_package
    depends_on: [generate_content, image_features, source_image]
    with:
      content: $generate_content
      colors: $image_features
    infer:
      prompt: |
        Compile this multimodal content package:

        Platform-specific content:
        {{with.content}}

        Color palette:
        {{with.colors}}

        Create a JSON content package ready for publishing.
      response_format: json
      temperature: 0.2
      max_tokens: 2000
    structured:
      schema:
        type: object
        properties:
          platforms:
            type: array
            items:
              type: object
              properties:
                platform:
                  type: string
                content:
                  type: string
                character_count:
                  type: integer
              required: [platform, content]
          color_palette:
            type: array
            items:
              type: string
        required: [platforms]
    artifact:
      path: content-package.json
      format: json
```

**Explanation:**

The `for_each:` block generates platform-specific content from the same source image. Each iteration sends the image via `content: type: image` and customizes the prompt with platform-specific instructions. The `concurrency: 4` processes all four platforms simultaneously. The color palette from `nika:dominant_color` is passed as additional context so the LLM can incorporate specific color names into descriptions.

**Expected Output:** A JSON content package with platform-specific descriptions derived from the image.

---

## Key Patterns for Vision Workflows

### Single Image Analysis

```yaml
infer:
  content:
    - type: image
      source: "{{with.img.media[0].hash}}"
      detail: high
    - type: text
      text: "Describe this image."
```

### Multi-Image Comparison

```yaml
infer:
  content:
    - type: image
      source: "{{with.img1.media[0].hash}}"
    - type: image
      source: "{{with.img2.media[0].hash}}"
    - type: text
      text: "Compare these two images."
```

### Prompt + Content Combination

When `prompt:` and `content:` are both present, the prompt is prepended as the first text part:

```yaml
infer:
  prompt: "You are analyzing a product photo."
  content:
    - type: image
      source: "{{with.photo.media[0].hash}}"
      detail: high
    - type: text
      text: "Rate this photo 1-10 for e-commerce use."
```

### Chart + Vision Pattern

Generate a chart, then analyze it visually:

```yaml
- id: chart
  invoke:
    tool: "nika:chart"
    params: { type: "bar", ... }

- id: analyze
  depends_on: [chart]
  with:
    chart: $chart
  infer:
    content:
      - type: image
        source: "{{with.chart.media[0].hash}}"
      - type: text
        text: "What trends do you see?"
```
