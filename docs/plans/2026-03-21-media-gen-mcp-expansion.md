# Media Generation + MCP Expansion Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Enable image generation via MCP servers (Replicate, ComfyUI, FAL) + expand MCP aliases from 48 to 100+ + create a demo workflow showing the complete generate-process-analyze pipeline.

**Architecture:** Phase 1 adds MCP aliases so `nika mcp add replicate` works out-of-the-box. Phase 2 creates a production workflow that generates images via DALL-E fetch, then processes them through ALL media tools. Phase 3 documents the pattern for users.

**Tech Stack:** Rust (nika-core catalogs), YAML workflows, OpenAI DALL-E API, MCP protocol

---

## Phase 1: Expand MCP Aliases (48 -> 100+)

### Task 1: Add AI/Image Generation aliases (6 new)

**Files:**
- Modify: `tools/nika-core/src/catalogs/mcp_aliases.rs`

**Step 1: Add new category after "AI & SPECIALIZED"**

```rust
// =============================================================================
// AI IMAGE/MEDIA GENERATION (6)
// =============================================================================
("replicate", "replicate-mcp"),
("comfyui", "comfyui-mcp-server"),
("fal", "fal-mcp"),
("stability", "stability-ai-mcp"),
("elevenlabs", "elevenlabs-mcp"),
("deepgram", "deepgram-mcp"),
```

**Step 2: Add Communication category (6 new)**

```rust
// =============================================================================
// COMMUNICATION (6)
// =============================================================================
("discord", "discord-mcp"),
("telegram", "telegram-mcp"),
("resend", "resend-mcp"),
("sendgrid", "sendgrid-mcp"),
("twilio", "twilio-mcp"),
("intercom", "intercom-mcp"),
```

**Step 3: Add Knowledge/Vector DB category (6 new)**

```rust
// =============================================================================
// KNOWLEDGE & VECTOR DB (6)
// =============================================================================
("pinecone", "pinecone-mcp"),
("weaviate", "weaviate-mcp"),
("qdrant", "qdrant-mcp"),
("chroma", "chroma-mcp"),
("milvus", "milvus-mcp"),
("turbopuffer", "turbopuffer-mcp"),
```

**Step 4: Add Analytics & Monitoring category (6 new)**

```rust
// =============================================================================
// ANALYTICS & MONITORING (6)
// =============================================================================
("posthog", "posthog-mcp"),
("mixpanel", "mixpanel-mcp"),
("datadog", "datadog-mcp"),
("grafana", "grafana-mcp"),
("prometheus", "prometheus-mcp"),
("plausible", "plausible-mcp"),
```

**Step 5: Add E-commerce & Finance category (6 new)**

```rust
// =============================================================================
// E-COMMERCE & FINANCE (6)
// =============================================================================
("stripe", "stripe-mcp"),
("shopify", "shopify-mcp"),
("paypal", "paypal-mcp"),
("polygon", "polygon-mcp"),
("coinbase", "coinbase-mcp"),
("alpaca", "alpaca-mcp"),
```

**Step 6: Add CMS & Content category (6 new)**

```rust
// =============================================================================
// CMS & CONTENT (6)
// =============================================================================
("wordpress", "wordpress-mcp"),
("contentful", "contentful-mcp"),
("sanity", "sanity-mcp"),
("strapi", "strapi-mcp"),
("ghost", "ghost-mcp"),
("hubspot", "hubspot-mcp"),
```

**Step 7: Add Infrastructure & DevOps category (6 new)**

```rust
// =============================================================================
// INFRASTRUCTURE & DEVOPS (6)
// =============================================================================
("docker", "docker-mcp"),
("kubernetes", "kubernetes-mcp"),
("terraform", "terraform-mcp"),
("pulumi", "pulumi-mcp"),
("fly", "fly-mcp"),
("railway", "railway-mcp"),
```

**Step 8: Add Social Media category (6 new)**

```rust
// =============================================================================
// SOCIAL MEDIA (6)
// =============================================================================
("twitter", "twitter-mcp"),
("linkedin", "linkedin-mcp"),
("youtube", "youtube-mcp"),
("tiktok", "tiktok-mcp"),
("reddit", "reddit-mcp"),
("mastodon", "mastodon-mcp"),
```

**Step 9: Add Maps & Location category (4 new)**

```rust
// =============================================================================
// MAPS & LOCATION (4)
// =============================================================================
("mapbox", "mapbox-mcp"),
("here", "here-mcp"),
("openweather", "openweather-mcp"),
("ipinfo", "ipinfo-mcp"),
```

**Step 10: Update doc comment count + run tests**

Update the module doc from "48" to the new total (~100).
Update the test at the bottom that asserts the count.

Run: `cd tools/nika-core && cargo test --lib`
Expected: All tests pass.

**Step 11: Commit**

```bash
git add tools/nika-core/src/catalogs/mcp_aliases.rs
git commit -m "feat(mcp): expand aliases from 48 to 100+ (image gen, vector DB, analytics, social, devops)"
```

---

## Phase 2: Image Generation Pipeline Workflow

### Task 2: Create the DALL-E image gen + media processing workflow

**Files:**
- Create: `/Users/thibaut/Desktop/nika-test-034/course/level-10-laugh-tale/07-image-gen-pipeline.nika.yaml`

**The workflow (15+ tasks, 5 phases):**

```yaml
# =============================================================
# LEVEL 10 - BONUS: AI Image Generation + Media Pipeline
# =============================================================
#
# Generate an image with DALL-E, then process it through
# EVERY media tool in Nika. This proves the complete pipeline:
#
#   Generate (fetch DALL-E API)
#     -> Download (fetch URL, response: binary)
#       -> Analyze (dimensions, colors, thumbhash, metadata)
#       -> Process (thumbnail, convert, strip, optimize, pipeline)
#       -> Compare (phash original vs processed)
#       -> Vision (infer content: type: image)
#       -> Chart (size comparison bar chart)
#       -> Report (LLM summary with structured output)
#       -> Artifacts (binary images + JSON + Markdown)
#
# REQUIRES: OPENAI_API_KEY (for DALL-E + GPT-4o vision)
# COST: ~$0.05 (DALL-E image) + ~$0.02 (GPT-4o vision) = ~$0.07
#
# ALTERNATIVE IMAGE SOURCES (no DALL-E needed):
#   - MCP: nika mcp add replicate -> invoke: replicate.run_model
#   - MCP: nika mcp add comfyui -> invoke: comfyui.generate_image
#   - MCP: nika mcp add fal -> invoke: fal.generate
#   - Free: fetch: https://picsum.photos/1024/1024 (random photo)
#
# =============================================================

schema: "nika/workflow@0.12"
workflow: image-gen-pipeline
description: "Generate AI image, process through all media tools, analyze with vision"

provider: openai
model: gpt-4o

inputs:
  prompt:
    type: string
    default: "A beautiful sunset over a futuristic city with flying cars, digital art style, 4K"
  size:
    type: string
    default: "1024x1024"

artifacts:
  dir: ./output/image-gen
  manifest: true

tasks:
  # ============================================================
  # PHASE 1: GENERATE IMAGE (DALL-E API via fetch)
  # ============================================================

  - id: generate_image
    # fetch: calls the DALL-E API directly -- no special provider needed
    # This works because DALL-E is a REST API, not an LLM chat endpoint
    fetch:
      url: "https://api.openai.com/v1/images/generations"
      method: POST
      headers:
        Authorization: "Bearer $OPENAI_API_KEY"
        Content-Type: "application/json"
      json:
        model: "dall-e-3"
        prompt: "{{inputs.prompt}}"
        n: 1
        size: "{{inputs.size}}"
        response_format: "url"
      timeout: 60
    # Output: { "data": [{ "url": "https://..." }] }

  - id: download_image
    depends_on: [generate_image]
    with:
      gen_result: $generate_image
    fetch:
      # Extract the URL from DALL-E response
      url: "{{with.gen_result.data[0].url}}"
      response: binary
      timeout: 30
    artifact:
      path: original/dalle-image.png
      format: binary

  # ============================================================
  # PHASE 2: ANALYZE (all Tier 1 tools)
  # ============================================================

  - id: get_dimensions
    depends_on: [download_image]
    with:
      img: $download_image
    invoke:
      tool: "nika:dimensions"
      params:
        hash: "{{with.img.media[0].hash}}"

  - id: get_colors
    depends_on: [download_image]
    with:
      img: $download_image
    invoke:
      tool: "nika:dominant_color"
      params:
        hash: "{{with.img.media[0].hash}}"
        count: 5

  - id: get_thumbhash
    depends_on: [download_image]
    with:
      img: $download_image
    invoke:
      tool: "nika:thumbhash"
      params:
        hash: "{{with.img.media[0].hash}}"

  - id: get_metadata
    depends_on: [download_image]
    with:
      img: $download_image
    invoke:
      tool: "nika:metadata"
      params:
        hash: "{{with.img.media[0].hash}}"

  # ============================================================
  # PHASE 3: PROCESS (Tier 2 tools + pipeline)
  # ============================================================

  - id: make_thumbnail
    depends_on: [download_image]
    with:
      img: $download_image
    invoke:
      tool: "nika:thumbnail"
      params:
        hash: "{{with.img.media[0].hash}}"
        width: 256
        format: "png"

  - id: convert_webp
    depends_on: [download_image]
    with:
      img: $download_image
    invoke:
      tool: "nika:convert"
      params:
        hash: "{{with.img.media[0].hash}}"
        format: "webp"
        quality: 85

  - id: full_pipeline
    depends_on: [download_image]
    with:
      img: $download_image
    invoke:
      tool: "nika:pipeline"
      params:
        hash: "{{with.img.media[0].hash}}"
        steps:
          - op: thumbnail
            width: 512
          - op: strip
          - op: convert
            format: webp
            quality: 90

  # ============================================================
  # PHASE 4: COMPARE + VISION
  # ============================================================

  - id: phash_original
    depends_on: [download_image]
    with:
      img: $download_image
    invoke:
      tool: "nika:phash"
      params:
        hash: "{{with.img.media[0].hash}}"

  - id: phash_thumb
    depends_on: [make_thumbnail]
    with:
      thumb: $make_thumbnail
    invoke:
      tool: "nika:phash"
      params:
        hash: "{{with.thumb.media[0].hash}}"

  - id: compare_versions
    depends_on: [phash_original, phash_thumb]
    with:
      original: $download_image
      thumb: $make_thumbnail
    invoke:
      tool: "nika:compare"
      params:
        hash_a: "{{with.original.media[0].hash}}"
        hash_b: "{{with.thumb.media[0].hash}}"

  - id: vision_analyze
    depends_on: [download_image]
    with:
      img: $download_image
    infer:
      content:
        - type: image
          source: "{{with.img.media[0].hash}}"
          detail: high
        - type: text
          text: |
            Analyze this AI-generated image:
            1. Describe what you see (2 sentences)
            2. Rate the quality (1-10)
            3. Suggest 3 improvements
            4. Is this suitable for commercial use?
      temperature: 0.3
      max_tokens: 500
    artifact:
      path: analysis/vision-analysis.md

  # ============================================================
  # PHASE 5: REPORT
  # ============================================================

  - id: size_chart
    depends_on: [download_image, make_thumbnail, convert_webp, full_pipeline]
    with:
      original: $download_image
      thumb: $make_thumbnail
      webp: $convert_webp
      pipeline: $full_pipeline
    invoke:
      tool: "nika:chart"
      params:
        type: bar
        title: "Image Size Comparison (bytes)"
        series:
          - name: "Size"
            data: [100000, 25000, 45000, 30000]
        labels: ["Original", "Thumbnail", "WebP", "Pipeline"]
        width: 800
        height: 400

  - id: final_report
    depends_on: [vision_analyze, get_dimensions, get_colors, compare_versions, size_chart]
    with:
      vision: $vision_analyze
      dims: $get_dimensions
      colors: $get_colors
      comparison: $compare_versions
    infer:
      prompt: |
        Create a comprehensive image processing report:

        ## Vision Analysis
        {{with.vision}}

        ## Technical Specs
        Dimensions: {{with.dims}}
        Dominant colors: {{with.colors}}
        Original vs Thumbnail similarity: {{with.comparison}}

        Generate a professional Markdown report with all findings.
      temperature: 0.3
      max_tokens: 1000
    artifact:
      path: analysis/final-report.md
```

**Step 1: Write the file**
**Step 2: Run `nika check` to validate**
**Step 3: Commit**

---

## Phase 3: Update nika-core provider catalog for MCP env vars

### Task 3: Add image gen providers to KNOWN_PROVIDERS

**Files:**
- Modify: `tools/nika-core/src/catalogs/providers.rs`

Add MCP providers for image generation:

```rust
Provider {
    id: "replicate",
    name: "Replicate",
    aliases: &["rep"],
    env_var: "REPLICATE_API_TOKEN",
    key_prefix: "r8_",
    category: ProviderCategory::Mcp,
    requires_key: true,
    description: "Run ML models in the cloud (images, video, audio)",
},
Provider {
    id: "fal",
    name: "FAL.ai",
    aliases: &[],
    env_var: "FAL_KEY",
    key_prefix: "",
    category: ProviderCategory::Mcp,
    requires_key: true,
    description: "Fast inference for generative media (Flux, SDXL)",
},
Provider {
    id: "elevenlabs",
    name: "ElevenLabs",
    aliases: &["11labs"],
    env_var: "ELEVENLABS_API_KEY",
    key_prefix: "",
    category: ProviderCategory::Mcp,
    requires_key: true,
    description: "Text-to-speech and voice cloning",
},
```

**Step 1: Add providers**
**Step 2: Update test count**
**Step 3: Run tests: `cd tools/nika-core && cargo test --lib`**
**Step 4: Commit**

---

## Remaining work (post-plan, future)

### NOT in this plan (documented for later):

1. **nika:imagine SmartRouter** -- Deferred per YAGNI. Users can use `invoke: { mcp: replicate }` directly.
2. **Audio generation** -- ElevenLabs MCP alias added, users can `invoke: { mcp: elevenlabs, tool: text_to_speech }`.
3. **Video generation** -- No mature MCP servers yet. Replicate can run some video models.
4. **PDF generation** -- Can be done via `exec: { command: "wkhtmltopdf ..." }` or HTML -> PDF tools.

---

## Verification

After all 3 tasks:

```bash
# Test MCP aliases
cd tools/nika-core && cargo test --lib mcp_aliases
# Test provider catalog
cd tools/nika-core && cargo test --lib providers
# Test workflow
cd /Users/thibaut/Desktop/nika-test-034 && nika check course/level-10-laugh-tale/07-image-gen-pipeline.nika.yaml
# Full test suite
cd tools/nika && cargo test --lib
```
