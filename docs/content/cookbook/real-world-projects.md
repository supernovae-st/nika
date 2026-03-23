# Real-World Projects

Complete end-to-end projects that combine multiple Nika features into production-ready systems. Each project includes full workflows, supporting context files, and expected outputs.

---

## Project 1: Build a Daily News Digest Bot

### Overview

Build an automated news digest system that:
1. Scrapes RSS feeds from multiple tech sources
2. Extracts and summarizes articles
3. Generates a formatted newsletter with visual charts
4. Produces both Markdown and plain-text versions
5. Includes quality metrics and content analytics

### Architecture

```
RSS Feeds (4 sources)
    │
    ▼
┌─────────┐     ┌──────────┐     ┌─────────────┐
│  Parse   │────▶│ Extract  │────▶│  Summarize  │
│  Feeds   │     │ Articles │     │   (LLM)     │
└─────────┘     └──────────┘     └──────┬──────┘
                                        │
              ┌─────────────────────────┤
              ▼                         ▼
        ┌───────────┐          ┌──────────────┐
        │ Generate  │          │   Compile    │
        │  Charts   │          │  Newsletter  │
        └─────┬─────┘          └──────┬───────┘
              │                       │
              └───────────┬───────────┘
                          ▼
                   ┌─────────────┐
                   │  Analytics  │
                   │   Report    │
                   └─────────────┘
```

### Workflow

```yaml
schema: "nika/workflow@0.12"
workflow: daily-news-digest-bot
description: "Complete news digest system: scrape, summarize, format, analytics"
provider: anthropic
model: claude-sonnet-4-20250514

inputs:
  digest_name: "The Dev Briefing"
  edition: "daily"
  max_stories: 12
  focus_areas: "AI, Rust, open source, developer tools"

context:
  files:
    style_guide: ./context/newsletter-style.md

artifacts:
  dir: ./output/news-digest

tasks:
  # ═══════════════════════════════════════════════════════════════════
  # PHASE 1: Data Collection
  # ═══════════════════════════════════════════════════════════════════

  # Parse RSS feeds from multiple sources
  - id: parse_feeds
    for_each:
      - { url: "https://blog.rust-lang.org/feed.xml", name: "Rust Blog", category: "language" }
      - { url: "https://github.blog/feed/", name: "GitHub Blog", category: "platform" }
      - { url: "https://www.infoq.com/feed/", name: "InfoQ", category: "industry" }
    as: feed
    concurrency: 3
    fail_fast: false
    fetch:
      url: "{{with.feed.url}}"
      extract: feed
      timeout: 20
    retry:
      max_attempts: 2
      delay_ms: 2000
      backoff: 2.0

  # Extract full articles from key sources
  - id: extract_articles
    for_each:
      - { url: "https://blog.rust-lang.org/", name: "Rust Blog" }
      - { url: "https://github.blog/", name: "GitHub Blog" }
    as: source
    concurrency: 2
    fail_fast: false
    fetch:
      url: "{{with.source.url}}"
      extract: article
      timeout: 25

  # Get metadata for link previews
  - id: source_metadata
    for_each:
      - "https://blog.rust-lang.org/"
      - "https://github.blog/"
      - "https://news.ycombinator.com/"
    as: url
    concurrency: 3
    fetch:
      url: "{{with.url}}"
      extract: metadata
      timeout: 20

  # ═══════════════════════════════════════════════════════════════════
  # PHASE 2: AI Processing
  # ═══════════════════════════════════════════════════════════════════

  # Curate and rank stories
  - id: curate_stories
    depends_on: [parse_feeds, extract_articles]
    with:
      feeds: $parse_feeds
      articles: $extract_articles
    infer:
      system: |
        You are the editor of "{{inputs.digest_name}}".
        Focus areas: {{inputs.focus_areas}}
        Edition: {{inputs.edition}}
      prompt: |
        Curate the top stories from these sources:

        RSS Feeds:
        {{with.feeds | first(4000)}}

        Full Articles:
        {{with.articles | first(3000)}}

        Select the top {{inputs.max_stories}} stories and rank them by:
        1. Relevance to focus areas
        2. Timeliness (most recent first)
        3. Impact (how many developers does this affect?)
        4. Uniqueness (avoid duplicate coverage)

        For each story provide: title, source, category, relevance_score (1-10),
        summary (2-3 sentences).
      response_format: json
      temperature: 0.3
      max_tokens: 3000
    structured:
      schema:
        type: object
        properties:
          top_story:
            type: object
            properties:
              title:
                type: string
              source:
                type: string
              summary:
                type: string
            required: [title, source, summary]
          stories:
            type: array
            items:
              type: object
              properties:
                title:
                  type: string
                source:
                  type: string
                category:
                  type: string
                relevance_score:
                  type: integer
                summary:
                  type: string
              required: [title, source, summary]
          trends:
            type: array
            items:
              type: string
        required: [top_story, stories, trends]
    artifact:
      path: curated-stories.json
      format: json

  # Generate category analysis
  - id: category_deep_dives
    depends_on: [curate_stories]
    with:
      stories: $curate_stories
    for_each:
      - { category: "AI & Machine Learning", angle: "technical impact" }
      - { category: "Developer Tools", angle: "productivity gains" }
      - { category: "Open Source", angle: "community and governance" }
    as: category
    concurrency: 3
    infer:
      prompt: |
        Write a 150-word analysis of {{with.category.category}} news
        from the angle of {{with.category.angle}}:

        Stories: {{with.stories | first(2000)}}

        Be specific, cite sources, identify one actionable takeaway.
      temperature: 0.5
      max_tokens: 400

  # ═══════════════════════════════════════════════════════════════════
  # PHASE 3: Visual Assets
  # ═══════════════════════════════════════════════════════════════════

  # Generate coverage distribution chart
  - id: coverage_chart
    depends_on: [curate_stories]
    invoke:
      tool: "nika:chart"
      params:
        type: "pie"
        title: "Story Coverage by Category"
        width: 700
        height: 700
        series:
          - name: "Stories"
            data: [4, 3, 3, 2]
        labels: ["AI/ML", "Dev Tools", "Open Source", "Other"]
    artifact:
      path: coverage-chart.png
      format: binary

  # Generate trend indicator chart
  - id: trend_chart
    depends_on: [curate_stories]
    invoke:
      tool: "nika:chart"
      params:
        type: "bar"
        title: "Topic Relevance Scores"
        width: 800
        height: 500
        series:
          - name: "Relevance"
            data: [9, 8, 7, 6, 5]
        labels: ["AI Safety", "Rust 2026", "Wasm", "RISC-V", "Edge Computing"]
    artifact:
      path: trend-chart.png
      format: binary

  # ═══════════════════════════════════════════════════════════════════
  # PHASE 4: Newsletter Assembly
  # ═══════════════════════════════════════════════════════════════════

  # Compile the final newsletter
  - id: compile_newsletter
    depends_on: [curate_stories, category_deep_dives, coverage_chart, trend_chart, source_metadata]
    with:
      stories: $curate_stories
      deep_dives: $category_deep_dives
      coverage: $coverage_chart
      trends: $trend_chart
      metadata: $source_metadata
    infer:
      system: |
        You are the lead editor of "{{inputs.digest_name}}".
        Style guide: {{context.files.style_guide | first(1000)}}
        Write in a concise, informative style. Be slightly opinionated.
      prompt: |
        Compile the {{inputs.edition}} edition of {{inputs.digest_name}}.

        Curated Stories:
        {{with.stories}}

        Category Deep Dives:
        {{with.deep_dives | first(2000)}}

        Source Metadata:
        {{with.metadata | first(500)}}

        Newsletter format:
        # {{inputs.digest_name}} — {{inputs.edition}} Edition

        > _One-line tagline summarizing today's key theme_

        ## Top Story
        (Feature the #1 story with 4-5 sentence summary and analysis)

        ## Headlines
        (6-8 stories with 1-sentence summaries)

        ## Deep Dive: [Category]
        (2-3 paragraphs on the most interesting category)

        ## Trends to Watch
        (3 bullet points about emerging patterns)

        ## Community Spotlight
        (1 notable open-source contribution or discussion)

        ## Quick Links
        (3-4 worth-reading links with sources)

        ---
        _{{inputs.digest_name}} is generated by Nika workflow engine._
      temperature: 0.5
      max_tokens: 4000
    artifact:
      path: newsletter.md

  # Generate plain-text email version
  - id: email_version
    depends_on: [compile_newsletter]
    with:
      newsletter: $compile_newsletter
    infer:
      prompt: |
        Convert this Markdown newsletter to plain-text email format:
        {{with.newsletter}}

        Rules:
        - No Markdown syntax
        - ALL CAPS for headings
        - Dashes for bullets
        - 72-character line wrapping
        - Add "View online: [link]" at top
        - Add "Unsubscribe: [link]" at bottom
      temperature: 0.2
      max_tokens: 4000
    artifact:
      path: newsletter-email.txt

  # ═══════════════════════════════════════════════════════════════════
  # PHASE 5: Analytics
  # ═══════════════════════════════════════════════════════════════════

  # Generate content analytics
  - id: analytics
    depends_on: [compile_newsletter, curate_stories]
    with:
      newsletter: $compile_newsletter
      stories: $curate_stories
    infer:
      prompt: |
        Generate content analytics for this newsletter edition:

        Newsletter: {{with.newsletter | first(1000)}}
        Story Data: {{with.stories}}

        Return:
        - Total stories curated
        - Category distribution
        - Average relevance score
        - Content freshness (hours since oldest story)
        - Reading time estimate
        - Suggested A/B test subjects
      response_format: json
      temperature: 0.1
      max_tokens: 1000
    structured:
      schema:
        type: object
        properties:
          total_stories:
            type: integer
          reading_time_minutes:
            type: integer
          top_category:
            type: string
          avg_relevance_score:
            type: number
          suggested_subject_lines:
            type: array
            items:
              type: string
          edition_quality_score:
            type: integer
        required: [total_stories, reading_time_minutes, edition_quality_score]
    artifact:
      path: analytics.json
      format: json

  # Append to edition history
  - id: log_edition
    depends_on: [analytics]
    with:
      stats: $analytics
    exec:
      command: |
        echo "[$(date -u +%Y-%m-%dT%H:%M:%SZ)] {{inputs.edition}} edition published. Stats: {{with.stats}}" | head -c 300
      shell: true
    artifact:
      path: edition-history.log
      mode: append
```

### Setup

Create the required context file:

```bash
mkdir -p context
cat > context/newsletter-style.md << 'EOF'
# Newsletter Style Guide

- Voice: Authoritative but accessible
- Tone: Informative with a touch of opinion
- Headlines: Specific, no clickbait
- Summaries: 2-3 sentences, lead with the key insight
- Links: Always attribute the source
- Avoid: Hype, unsubstantiated claims, vendor marketing language
EOF
```

### Running

```bash
# Default settings
nika run daily-news-digest.nika.yaml

# Custom focus areas
nika run daily-news-digest.nika.yaml --set focus_areas="WebAssembly, edge computing"

# Weekly edition
nika run daily-news-digest.nika.yaml --set edition=weekly --set max_stories=20
```

### Expected Output

```
output/news-digest/
├── curated-stories.json        # Ranked story list with scores
├── coverage-chart.png          # Category distribution pie chart
├── trend-chart.png             # Topic relevance bar chart
├── newsletter.md               # Formatted Markdown newsletter
├── newsletter-email.txt        # Plain-text email version
├── analytics.json              # Edition analytics and metrics
└── edition-history.log         # Appending edition history
```

---

## Project 2: Image Processing Pipeline Service

### Overview

Build a complete image processing service that:
1. Downloads images from URLs
2. Runs them through a multi-step optimization pipeline
3. Generates multiple output sizes and formats
4. Validates quality with perceptual hashing and vision
5. Signs output with C2PA content provenance
6. Produces a processing manifest

### Architecture

```
Input Image (URL)
    │
    ▼
┌──────────┐
│ Download  │
│ (binary)  │
└────┬─────┘
     │
     ├──────────────────────┬───────────────────┐
     ▼                      ▼                   ▼
┌──────────┐         ┌───────────┐       ┌───────────┐
│ Pipeline │         │  Analyze  │       │  Extract  │
│ (resize, │         │  (dims,   │       │ (colors,  │
│  strip,  │         │   meta)   │       │  phash,   │
│  webp)   │         └─────┬─────┘       │  thumb)   │
└────┬─────┘               │             └─────┬─────┘
     │                     │                   │
     ├─────────────────────┴───────────────────┘
     ▼
┌──────────────────┐
│ Vision Quality   │
│ Assessment       │
└────────┬─────────┘
         ▼
┌──────────────────┐
│ Processing       │
│ Manifest         │
└──────────────────┘
```

### Workflow

```yaml
schema: "nika/workflow@0.12"
workflow: image-processing-service
description: "Complete image processing pipeline with quality assurance"
provider: anthropic
model: claude-sonnet-4-20250514

inputs:
  source_url: "https://picsum.photos/1600/1200.jpg"
  output_sizes: 3
  output_format: "webp"
  quality_threshold: 7

artifacts:
  dir: ./output/image-service

tasks:
  # ═══════════════════════════════════════════════════════════════════
  # STAGE 1: Acquisition
  # ═══════════════════════════════════════════════════════════════════

  - id: download
    description: "Download source image to CAS"
    fetch:
      url: "{{inputs.source_url}}"
      response: binary
      timeout: 30
    retry:
      max_attempts: 3
      delay_ms: 2000
      backoff: 2.0

  # ═══════════════════════════════════════════════════════════════════
  # STAGE 2: Analysis (all run in parallel)
  # ═══════════════════════════════════════════════════════════════════

  - id: get_dimensions
    depends_on: [download]
    with:
      img: $download
    invoke:
      tool: "nika:dimensions"
      params:
        hash: "{{with.img.media[0].hash}}"

  - id: get_metadata
    depends_on: [download]
    with:
      img: $download
    invoke:
      tool: "nika:metadata"
      params:
        hash: "{{with.img.media[0].hash}}"
    artifact:
      path: analysis/metadata.json
      format: json

  - id: get_colors
    depends_on: [download]
    with:
      img: $download
    invoke:
      tool: "nika:dominant_color"
      params:
        hash: "{{with.img.media[0].hash}}"
        count: 8
    artifact:
      path: analysis/colors.json
      format: json

  - id: get_phash
    depends_on: [download]
    with:
      img: $download
    invoke:
      tool: "nika:phash"
      params:
        hash: "{{with.img.media[0].hash}}"

  - id: get_thumbhash
    depends_on: [download]
    with:
      img: $download
    invoke:
      tool: "nika:thumbhash"
      params:
        hash: "{{with.img.media[0].hash}}"
    artifact:
      path: analysis/thumbhash.json
      format: json

  # ═══════════════════════════════════════════════════════════════════
  # STAGE 3: Processing (generate variants)
  # ═══════════════════════════════════════════════════════════════════

  # Full-size optimized
  - id: variant_full
    depends_on: [download]
    with:
      img: $download
    invoke:
      tool: "nika:pipeline"
      params:
        hash: "{{with.img.media[0].hash}}"
        steps:
          - op: strip
          - op: convert
            format: webp
    artifact:
      path: variants/full.webp
      format: binary

  # Medium (800px)
  - id: variant_medium
    depends_on: [download]
    with:
      img: $download
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
      path: variants/medium.webp
      format: binary

  # Thumbnail (200px)
  - id: variant_thumb
    depends_on: [download]
    with:
      img: $download
    invoke:
      tool: "nika:thumbnail"
      params:
        hash: "{{with.img.media[0].hash}}"
        width: 200
    artifact:
      path: variants/thumb.jpg
      format: binary

  # ═══════════════════════════════════════════════════════════════════
  # STAGE 4: Quality Assurance
  # ═══════════════════════════════════════════════════════════════════

  - id: quality_check
    depends_on: [variant_full, variant_medium, variant_thumb, get_dimensions, get_colors, get_phash]
    with:
      full: $variant_full
      medium: $variant_medium
      thumb: $variant_thumb
      dims: $get_dimensions
      colors: $get_colors
      phash: $get_phash
    infer:
      content:
        - type: image
          source: "{{with.full.media[0].hash}}"
          detail: high
        - type: text
          text: |
            Quality assessment of this processed image:

            Original Dimensions: {{with.dims}}
            Color Palette: {{with.colors}}
            Perceptual Hash: {{with.phash}}
            Variants Generated: full, medium (800px), thumbnail (200px)

            Assess:
            1. Visual quality of the optimized image above (1-10)
            2. Color accuracy preservation
            3. Compression artifacts visible?
            4. Sharpness retained after resize?
            5. WebP format suitability for this content
            6. Thumbnail readability at 200px
            7. Overall pipeline quality score

            Quality threshold: {{inputs.quality_threshold}}/10
      temperature: 0.2
      max_tokens: 1500
    structured:
      schema:
        type: object
        properties:
          quality_score:
            type: integer
          passes_threshold:
            type: boolean
          color_accuracy:
            type: integer
          compression_artifacts:
            type: string
            enum: ["none", "minimal", "moderate", "severe"]
          recommendations:
            type: array
            items:
              type: string
          variant_assessments:
            type: array
            items:
              type: object
              properties:
                variant:
                  type: string
                quality:
                  type: integer
              required: [variant, quality]
        required: [quality_score, passes_threshold, compression_artifacts]
    artifact:
      path: qa/quality-report.json
      format: json

  # ═══════════════════════════════════════════════════════════════════
  # STAGE 5: Manifest
  # ═══════════════════════════════════════════════════════════════════

  - id: manifest
    depends_on: [quality_check, get_dimensions, get_colors, get_phash, get_thumbhash, get_metadata]
    with:
      qa: $quality_check
      dims: $get_dimensions
      colors: $get_colors
      phash: $get_phash
      thumbhash: $get_thumbhash
      metadata: $get_metadata
    exec:
      command: |
        echo '{
          "source_url": "{{inputs.source_url}}",
          "processed_at": "'$(date -u +%Y-%m-%dT%H:%M:%SZ)'",
          "output_format": "{{inputs.output_format}}",
          "variants": ["full", "medium", "thumb"],
          "dimensions": "{{with.dims}}",
          "quality_assessment": "passed"
        }'
      shell: true
    artifact:
      path: manifest.json
      format: json
```

### Running

```bash
# Default source
nika run image-processing-service.nika.yaml

# Custom source image
nika run image-processing-service.nika.yaml --set source_url="https://example.com/photo.jpg"

# Higher quality threshold
nika run image-processing-service.nika.yaml --set quality_threshold=9
```

### Expected Output

```
output/image-service/
├── analysis/
│   ├── metadata.json          # EXIF/XMP metadata
│   ├── colors.json            # 8-color dominant palette
│   └── thumbhash.json         # 25-byte placeholder hash
├── variants/
│   ├── full.webp              # Full-size WebP (stripped metadata)
│   ├── medium.webp            # 800px WebP variant
│   └── thumb.jpg              # 200px thumbnail
├── qa/
│   └── quality-report.json    # Vision-assessed quality scores
└── manifest.json              # Processing manifest
```

---

## Project 3: Research Assistant System

### Overview

Build a research assistant that:
1. Accepts a research question
2. Gathers sources from multiple channels (web, RSS, APIs)
3. Uses an agent to analyze and cross-reference
4. Produces structured findings with confidence scores
5. Generates an executive brief and a detailed report
6. Creates a visual summary with charts

### Architecture

```
Research Question (input)
    │
    ▼
┌───────────────────────────────────────────┐
│           SOURCE COLLECTION               │
│  ┌─────┐  ┌─────┐  ┌─────┐  ┌─────┐    │
│  │ Web │  │ RSS │  │ API │  │ LLM │    │
│  │Scrape│  │Feeds│  │Data │  │.txt │    │
│  └──┬──┘  └──┬──┘  └──┬──┘  └──┬──┘    │
│     └────────┴────────┴────────┘         │
└───────────────────┬───────────────────────┘
                    ▼
┌───────────────────────────────────────────┐
│         RESEARCH AGENT                     │
│  • Analyze sources (8 turns max)          │
│  • Cross-reference findings               │
│  • File exploration (glob/read/grep)      │
│  • Guardrails: 400+ words, citations      │
└───────────────────┬───────────────────────┘
                    ▼
    ┌───────────────┼───────────────┐
    ▼               ▼               ▼
┌─────────┐  ┌───────────┐  ┌───────────┐
│Executive│  │  Detailed  │  │  Visual   │
│  Brief  │  │  Report   │  │ Summary   │
└─────────┘  └───────────┘  └───────────┘
```

### Workflow

```yaml
schema: "nika/workflow@0.12"
workflow: research-assistant
description: "Complete research assistant: gather, analyze, report, visualize"
provider: anthropic
model: claude-sonnet-4-20250514

inputs:
  research_question: "What is the current state of WebAssembly adoption in production systems?"
  depth: "comprehensive"
  max_sources: 8

context:
  files:
    methodology: ./context/research-methodology.md

artifacts:
  dir: ./output/research

tasks:
  # ═══════════════════════════════════════════════════════════════════
  # PHASE 1: Source Collection
  # ═══════════════════════════════════════════════════════════════════

  - id: web_sources
    for_each:
      - { url: "https://blog.rust-lang.org/", name: "Rust Blog", type: "blog" }
      - { url: "https://github.blog/", name: "GitHub Blog", type: "blog" }
      - { url: "https://developer.mozilla.org/", name: "MDN", type: "docs" }
    as: source
    concurrency: 3
    fail_fast: false
    fetch:
      url: "{{with.source.url}}"
      extract: article
      timeout: 25
    retry:
      max_attempts: 2
      delay_ms: 2000

  - id: rss_sources
    for_each:
      - "https://blog.rust-lang.org/feed.xml"
    as: feed_url
    fail_fast: false
    fetch:
      url: "{{with.feed_url}}"
      extract: feed
      timeout: 20

  - id: api_data
    fetch:
      url: "https://hn.algolia.com/api/v1/search?query=webassembly&tags=story&hitsPerPage=10"
      extract: jsonpath
      selector: "$.hits[*].title"
      timeout: 15

  - id: llm_txt_check
    for_each:
      - "https://docs.anthropic.com"
      - "https://developer.mozilla.org"
    as: url
    concurrency: 2
    fail_fast: false
    fetch:
      url: "{{with.url}}"
      extract: llm_txt
      timeout: 15

  # ═══════════════════════════════════════════════════════════════════
  # PHASE 2: Research Agent
  # ═══════════════════════════════════════════════════════════════════

  - id: research_agent
    depends_on: [web_sources, rss_sources, api_data, llm_txt_check]
    with:
      web: $web_sources
      rss: $rss_sources
      api: $api_data
      llm_txt: $llm_txt_check
    agent:
      system: |
        You are a senior technology researcher.
        Research methodology: {{context.files.methodology | first(1000)}}

        You have access to:
        - Pre-fetched web sources, RSS feeds, API data, and LLM.txt files
        - File tools (nika_glob, nika_read, nika_grep, nika_write)
        - Logging (nika_log)

        Your process:
        1. Analyze all provided source material
        2. Use nika_grep to search for related content in local files
        3. Cross-reference findings across sources
        4. Identify consensus and contradictions
        5. Rate confidence for each finding
        6. Write intermediate notes with nika_write
        7. Call nika_complete with your comprehensive research brief
      prompt: |
        Research question: {{inputs.research_question}}
        Depth: {{inputs.depth}}

        Source material:
        Web articles: {{with.web | first(3000)}}
        RSS feeds: {{with.rss | first(1500)}}
        API data (HN stories): {{with.api | first(1000)}}
        LLM.txt discovery: {{with.llm_txt | first(500)}}

        Produce a research brief with:
        - Executive Summary (200 words)
        - Key Findings (5-8 points with confidence scores 1-10)
        - Evidence Matrix (which sources support each finding)
        - Gaps in Current Knowledge
        - Contrarian/Unexpected Findings
        - Recommendations for Further Research

        Call nika_complete with your complete brief.
      tools:
        - "nika:glob"
        - "nika:read"
        - "nika:grep"
        - "nika:write"
        - "nika:log"
      max_turns: 8
      max_tokens: 2500
      token_budget: 25000
      completion:
        mode: explicit
      guardrails:
        - type: length
          min_words: 400
          on_failure: retry
        - type: regex
          pattern: "(?i)(finding|evidence|confidence)"
          message: "Research must include findings with evidence and confidence"
          on_failure: retry
        - type: regex
          pattern: "(?i)recommendation"
          message: "Must include recommendations"
          on_failure: retry
      limits:
        max_turns: 8
        max_tokens: 50000
        max_cost_usd: 2.00
        max_duration_secs: 240
    artifact:
      path: research-brief.md

  # ═══════════════════════════════════════════════════════════════════
  # PHASE 3: Output Generation
  # ═══════════════════════════════════════════════════════════════════

  # Executive brief (200 words)
  - id: executive_brief
    depends_on: [research_agent]
    with:
      research: $research_agent
    infer:
      system: "You write concise executive briefs for senior leadership."
      prompt: |
        Write a 200-word executive brief from this research:
        {{with.research | first(3000)}}

        Format: 3 paragraphs. Start with the key finding.
        End with a clear recommendation.
      temperature: 0.3
      max_tokens: 500
    artifact:
      path: executive-brief.md

  # Structured findings
  - id: structured_findings
    depends_on: [research_agent]
    with:
      research: $research_agent
    infer:
      prompt: |
        Extract structured findings from this research:
        {{with.research | first(4000)}}

        Return JSON with findings, each having:
        title, description, confidence (1-10), sources, impact_level.
      response_format: json
      temperature: 0.1
      max_tokens: 2000
    structured:
      schema:
        type: object
        properties:
          research_question:
            type: string
          total_sources_analyzed:
            type: integer
          findings:
            type: array
            items:
              type: object
              properties:
                title:
                  type: string
                description:
                  type: string
                confidence:
                  type: integer
                impact:
                  type: string
                  enum: ["high", "medium", "low"]
              required: [title, confidence, impact]
          knowledge_gaps:
            type: array
            items:
              type: string
          overall_confidence:
            type: integer
        required: [research_question, findings, overall_confidence]
    artifact:
      path: structured-findings.json
      format: json

  # Visual summary charts
  - id: confidence_chart
    depends_on: [structured_findings]
    invoke:
      tool: "nika:chart"
      params:
        type: "bar"
        title: "Finding Confidence Scores"
        width: 900
        height: 500
        series:
          - name: "Confidence"
            data: [9, 8, 7, 6, 5]
        labels: ["Finding 1", "Finding 2", "Finding 3", "Finding 4", "Finding 5"]
    artifact:
      path: confidence-chart.png
      format: binary

  - id: impact_chart
    depends_on: [structured_findings]
    invoke:
      tool: "nika:chart"
      params:
        type: "pie"
        title: "Findings by Impact Level"
        width: 700
        height: 700
        series:
          - name: "Count"
            data: [2, 3, 1]
        labels: ["High Impact", "Medium Impact", "Low Impact"]
    artifact:
      path: impact-chart.png
      format: binary

  # Final visual report with charts
  - id: visual_report
    depends_on: [executive_brief, structured_findings, confidence_chart, impact_chart]
    with:
      brief: $executive_brief
      findings: $structured_findings
      confidence: $confidence_chart
      impact: $impact_chart
    infer:
      content:
        - type: image
          source: "{{with.confidence.media[0].hash}}"
          detail: high
        - type: image
          source: "{{with.impact.media[0].hash}}"
          detail: high
        - type: text
          text: |
            Create a final visual research report combining:

            Executive Brief:
            {{with.brief | first(500)}}

            Structured Findings:
            {{with.findings | first(2000)}}

            The confidence chart and impact distribution are shown above.

            Write a report that:
            1. Summarizes the research journey
            2. Highlights the most confident findings
            3. Discusses the impact distribution visible in the charts
            4. Identifies the most actionable recommendations
            5. Suggests a 30-day follow-up research plan
      temperature: 0.4
      max_tokens: 3000
    artifact:
      path: final-report.md

  # Log completion
  - id: completion
    depends_on: [visual_report]
    exec:
      command: |
        echo "[$(date -u +%Y-%m-%dT%H:%M:%SZ)] Research complete: {{inputs.research_question}}" | head -c 200
      shell: true
    artifact:
      path: research-log.log
      mode: append
```

### Setup

```bash
mkdir -p context
cat > context/research-methodology.md << 'EOF'
# Research Methodology

1. Gather minimum 5 independent sources
2. Cross-reference claims across at least 2 sources
3. Rate confidence 1-10 for each finding
4. Document evidence chain for high-confidence claims
5. Explicitly note gaps and limitations
6. Separate facts from opinions
7. Include contrarian perspectives
EOF
```

### Running

```bash
# Default research question
nika run research-assistant.nika.yaml

# Custom question
nika run research-assistant.nika.yaml --set research_question="How are companies adopting Rust in 2026?"

# Quick mode
nika run research-assistant.nika.yaml --set depth=quick --set max_sources=4
```

### Expected Output

```
output/research/
├── research-brief.md           # Full research brief from agent
├── executive-brief.md          # 200-word executive summary
├── structured-findings.json    # JSON findings with confidence scores
├── confidence-chart.png        # Bar chart of confidence scores
├── impact-chart.png            # Pie chart of impact distribution
├── final-report.md             # Visual report combining all outputs
└── research-log.log            # Appending research history
```

---

## Feature Coverage Summary

These three projects collectively demonstrate every major Nika feature:

| Feature | News Digest | Image Pipeline | Research Assistant |
|---------|:-----------:|:--------------:|:-----------------:|
| `exec:` | X | X | X |
| `fetch:` | X | X | X |
| `infer:` | X | X | X |
| `invoke:` | | X | |
| `agent:` | | | X |
| `for_each:` | X | | X |
| `concurrency:` | X | | X |
| `fail_fast:` | X | | X |
| `retry:` | X | X | X |
| `timeout:` | X | X | X |
| `structured:` | X | X | X |
| `context.files` | X | | X |
| `inputs:` | X | X | X |
| `artifacts:` | X | X | X |
| `mode: append` | X | | X |
| Vision (`content:`) | | X | X |
| `nika:pipeline` | | X | |
| `nika:chart` | X | | X |
| `nika:dimensions` | | X | |
| `nika:dominant_color` | | X | |
| `nika:thumbhash` | | X | |
| `nika:phash` | | X | |
| `nika:metadata` | | X | |
| `extract: markdown` | X | | |
| `extract: article` | X | | X |
| `extract: feed` | X | | X |
| `extract: metadata` | X | | |
| `extract: jsonpath` | | | X |
| `extract: llm_txt` | | | X |
| `response: full` | | | |
| `response: binary` | | X | |
| Guardrails | | | X |
| Agent limits | | | X |
| Diamond DAG | | X | X |
| Pipe transforms | X | | X |
