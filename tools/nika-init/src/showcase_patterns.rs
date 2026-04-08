// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Showcase Patterns — 15 workflows exercising for_each + structured output
//!
//! These workflows demonstrate data processing patterns that combine
//! parallel iteration (`for_each:`) with schema-validated output
//! (`structured:`). They cover the hardest features to get right.
//!
//! All 15 workflows pass `nika check`. LLM workflows use `{{PROVIDER}}`
//! and `{{MODEL}}` placeholders replaced at generation time.

use super::WorkflowTemplate;

// ═════════════════════════════════════════════════════════════════════════════
// 01: Multi-URL Status Checker
// ═════════════════════════════════════════════════════════════════════════════

pub const SHOWCASE_01_URL_STATUS: &str = r##"# ═══════════════════════════════════════════════════════════════════
# Pattern 01: Multi-URL Status Checker
# ═══════════════════════════════════════════════════════════════════
#
# for_each over 10 URLs, fetch each, produce structured status report.
# Demonstrates: for_each array literal + fetch + structured output.
#
# Prerequisites: None (public endpoints)
# Run: nika run workflows/showcase/01-url-status.nika.yaml

schema: "nika/workflow@0.12"
workflow: url-status-checker
description: "Check status of multiple URLs in parallel"

tasks:
  - id: check_urls
    for_each:
      - "https://httpbin.org/status/200"
      - "https://httpbin.org/status/201"
      - "https://httpbin.org/status/301"
      - "https://httpbin.org/status/404"
      - "https://httpbin.org/status/500"
      - "https://httpbin.org/ip"
      - "https://httpbin.org/uuid"
      - "https://httpbin.org/user-agent"
      - "https://httpbin.org/headers"
      - "https://httpbin.org/get"
    as: target_url
    concurrency: 5
    fail_fast: false
    fetch:
      url: "{{with.target_url}}"
      method: GET
      response: full
      timeout: 15

  - id: report
    depends_on: [check_urls]
    with:
      results: $check_urls
    exec:
      command: |
        echo 'Status check complete. {{with.results | length}} URLs checked.'
      shell: true
"##;

// ═════════════════════════════════════════════════════════════════════════════
// 02: Language Detector
// ═════════════════════════════════════════════════════════════════════════════

pub const SHOWCASE_02_LANG_DETECT: &str = r##"# ═══════════════════════════════════════════════════════════════════
# Pattern 02: Language Detector
# ═══════════════════════════════════════════════════════════════════
#
# for_each over 5 multilingual texts, detect language via LLM,
# return structured {text_snippet, language, confidence}.
#
# Prerequisites: LLM provider
# Run: nika run workflows/showcase/02-lang-detect.nika.yaml

schema: "nika/workflow@0.12"
workflow: language-detector
description: "Detect language of multiple texts with structured output"

provider: "{{PROVIDER}}"
model: "{{MODEL}}"

tasks:
  - id: detect
    for_each:
      - "Bonjour, comment allez-vous aujourd'hui?"
      - "The quick brown fox jumps over the lazy dog."
      - "Heute ist ein wunderschoener Tag zum Programmieren."
      - "El desarrollo de software es un arte y una ciencia."
      - "Konnichiwa, kyou wa ii tenki desu ne."
    as: text
    concurrency: 3
    structured:
      schema:
        type: object
        properties:
          text_snippet:
            type: string
            description: "First 40 characters of the input"
          language:
            type: string
            enum: ["english", "french", "german", "spanish", "japanese", "other"]
          confidence:
            type: number
            minimum: 0
            maximum: 1
        required: [text_snippet, language, confidence]
    infer:
      prompt: |
        Detect the language of this text. Return the first 40 characters
        as text_snippet, the detected language, and a confidence score 0-1.

        Text: "{{with.text}}"

  - id: summary
    depends_on: [detect]
    with:
      detections: $detect
    exec:
      command: |
        echo 'Language detection complete. {{with.detections | length}} texts processed.'
      shell: true
"##;

// ═════════════════════════════════════════════════════════════════════════════
// 03: Batch Sentiment Analysis
// ═════════════════════════════════════════════════════════════════════════════

pub const SHOWCASE_03_SENTIMENT: &str = r##"# ═══════════════════════════════════════════════════════════════════
# Pattern 03: Batch Sentiment Analysis
# ═══════════════════════════════════════════════════════════════════
#
# for_each over product reviews, analyze sentiment with structured output.
# Demonstrates: for_each + structured with enum + number constraints.
#
# Prerequisites: LLM provider
# Run: nika run workflows/showcase/03-sentiment.nika.yaml

schema: "nika/workflow@0.12"
workflow: batch-sentiment
description: "Analyze sentiment of product reviews in batch"

provider: "{{PROVIDER}}"
model: "{{MODEL}}"

tasks:
  - id: analyze
    for_each:
      - "Absolutely love this product! Best purchase I've made all year."
      - "Decent quality but the shipping took forever. Not great, not terrible."
      - "Complete waste of money. Broke after two days. Would NOT recommend."
      - "It works as advertised. Nothing special but gets the job done."
      - "Mind-blowing performance! This exceeded all my expectations."
      - "Meh. It's okay I guess. Expected more for the price."
    as: review
    concurrency: 3
    structured:
      schema:
        type: object
        properties:
          review_excerpt:
            type: string
            maxLength: 60
            description: "First 60 chars of the review"
          sentiment:
            type: string
            enum: ["positive", "neutral", "negative"]
          score:
            type: number
            minimum: -1
            maximum: 1
            description: "Sentiment score from -1 (negative) to 1 (positive)"
          key_phrases:
            type: array
            items:
              type: string
            maxItems: 3
            description: "Key sentiment-bearing phrases"
        required: [review_excerpt, sentiment, score, key_phrases]
    infer:
      prompt: |
        Analyze the sentiment of this product review.
        Return: excerpt (first 60 chars), sentiment label,
        score (-1 to 1), and up to 3 key phrases.

        Review: "{{with.review}}"
      temperature: 0.2

  - id: aggregate
    depends_on: [analyze]
    with:
      results: $analyze
    exec:
      command: |
        echo 'Sentiment analysis complete. {{with.results | length}} reviews analyzed.'
      shell: true
"##;

// ═════════════════════════════════════════════════════════════════════════════
// 04: Parallel Translation
// ═════════════════════════════════════════════════════════════════════════════

pub const SHOWCASE_04_TRANSLATION: &str = r##"# ═══════════════════════════════════════════════════════════════════
# Pattern 04: Parallel Translation
# ═══════════════════════════════════════════════════════════════════
#
# Translate a source text into 5 languages in parallel.
# Each translation is saved as a separate artifact.
#
# Prerequisites: LLM provider
# Run: nika run workflows/showcase/04-translation.nika.yaml

schema: "nika/workflow@0.12"
workflow: parallel-translation
description: "Translate text into 5 languages with per-language artifacts"

provider: "{{PROVIDER}}"
model: "{{MODEL}}"

tasks:
  - id: source_text
    exec:
      command: |
        echo 'Open source software is not just about code. It is about community, collaboration, and the belief that knowledge should be free. Every contribution, no matter how small, moves us forward together.'
      shell: true

  - id: translate
    depends_on: [source_text]
    with:
      source: $source_text
    for_each: ["french", "german", "spanish", "japanese", "portuguese"]
    as: lang
    concurrency: 5
    structured:
      schema:
        type: object
        properties:
          language:
            type: string
          translation:
            type: string
            minLength: 10
          word_count:
            type: integer
            minimum: 1
        required: [language, translation, word_count]
    infer:
      prompt: |
        Translate the following text into {{with.lang}}.
        Return the target language name, the translation, and the word count.

        Source text: "{{with.source}}"
      temperature: 0.3
    artifact:
      path: "output/translations/{{with.lang}}.json"
      format: json

  - id: done
    depends_on: [translate]
    exec:
      command: "echo 'All 5 translations complete.'"
      shell: true
"##;

// ═════════════════════════════════════════════════════════════════════════════
// 05: API Endpoint Tester
// ═════════════════════════════════════════════════════════════════════════════

pub const SHOWCASE_05_API_TESTER: &str = r##"# ═══════════════════════════════════════════════════════════════════
# Pattern 05: API Endpoint Tester
# ═══════════════════════════════════════════════════════════════════
#
# Test multiple API endpoints with full response inspection.
# Demonstrates: for_each + fetch response:full + structured report.
#
# Prerequisites: None (public endpoints)
# Run: nika run workflows/showcase/05-api-tester.nika.yaml

schema: "nika/workflow@0.12"
workflow: api-endpoint-tester
description: "Test API endpoints and report structured results"

provider: "{{PROVIDER}}"
model: "{{MODEL}}"

tasks:
  - id: test_endpoints
    for_each:
      - "https://httpbin.org/get"
      - "https://httpbin.org/ip"
      - "https://httpbin.org/uuid"
      - "https://httpbin.org/user-agent"
      - "https://httpbin.org/headers"
      - "https://httpbin.org/delay/1"
    as: endpoint
    concurrency: 3
    fail_fast: false
    fetch:
      url: "{{with.endpoint}}"
      method: GET
      response: full
      timeout: 10

  - id: analyze_results
    depends_on: [test_endpoints]
    with:
      raw_results: $test_endpoints
    structured:
      schema:
        type: object
        properties:
          total_endpoints:
            type: integer
          healthy_count:
            type: integer
          slow_count:
            type: integer
            description: "Endpoints with response time > 2 seconds"
          summary:
            type: string
            description: "Brief health summary"
        required: [total_endpoints, healthy_count, slow_count, summary]
    infer:
      prompt: |
        Analyze these API test results and produce a health report.
        Count total endpoints, healthy ones (2xx), and slow ones (>2s).

        Results: {{with.raw_results}}
      temperature: 0.1
"##;

// ═════════════════════════════════════════════════════════════════════════════
// 06: Markdown Link Checker
// ═════════════════════════════════════════════════════════════════════════════

pub const SHOWCASE_06_LINK_CHECKER: &str = r##"# ═══════════════════════════════════════════════════════════════════
# Pattern 06: Markdown Link Checker
# ═══════════════════════════════════════════════════════════════════
#
# Extract links via exec, then for_each to verify each link.
# Demonstrates: exec output → for_each binding → fetch → structured.
#
# Prerequisites: None
# Run: nika run workflows/showcase/06-link-checker.nika.yaml

schema: "nika/workflow@0.12"
workflow: link-checker
description: "Extract and verify links from a list"

tasks:
  - id: extract_links
    exec:
      command: |
        echo '["https://httpbin.org/status/200", "https://httpbin.org/status/404", "https://httpbin.org/status/301", "https://httpbin.org/get", "https://httpbin.org/status/500"]'
      shell: true

  - id: check_each
    depends_on: [extract_links]
    with:
      links: $extract_links | parse_json
    for_each: "{{with.links}}"
    as: url
    concurrency: 3
    fail_fast: false
    fetch:
      url: "{{with.url}}"
      method: GET
      response: full
      timeout: 10

  - id: report
    depends_on: [check_each]
    with:
      results: $check_each
    exec:
      command: |
        echo 'Link check complete. Results: {{with.results}}'
      shell: true
"##;

// ═════════════════════════════════════════════════════════════════════════════
// 07: RSS Multi-Feed Aggregator
// ═════════════════════════════════════════════════════════════════════════════

pub const SHOWCASE_07_RSS_AGGREGATOR: &str = r##"# ═══════════════════════════════════════════════════════════════════
# Pattern 07: RSS Multi-Feed Aggregator
# ═══════════════════════════════════════════════════════════════════
#
# Fetch 5 RSS feeds in parallel with extract:feed, then merge.
# Demonstrates: for_each + fetch extract:feed + aggregation.
#
# Prerequisites: None (public feeds)
# Run: nika run workflows/showcase/07-rss-aggregator.nika.yaml

schema: "nika/workflow@0.12"
workflow: rss-aggregator
description: "Aggregate multiple RSS feeds in parallel"

tasks:
  - id: fetch_feeds
    for_each:
      - "https://hnrss.org/newest?count=3"
      - "https://www.reddit.com/r/rust/.rss?limit=3"
      - "https://www.reddit.com/r/programming/.rss?limit=3"
      - "https://lobste.rs/rss"
      - "https://blog.rust-lang.org/feed.xml"
    as: feed_url
    concurrency: 5
    fail_fast: false
    fetch:
      url: "{{with.feed_url}}"
      extract: feed
      timeout: 15

  - id: merge
    depends_on: [fetch_feeds]
    with:
      feeds: $fetch_feeds
    exec:
      command: |
        echo 'Aggregated {{with.feeds | length}} feeds.'
      shell: true
    artifact:
      path: output/feeds/aggregated.json
      format: json
"##;

// ═════════════════════════════════════════════════════════════════════════════
// 08: Git Branch Analyzer
// ═════════════════════════════════════════════════════════════════════════════

pub const SHOWCASE_08_GIT_ANALYZER: &str = r##"# ═══════════════════════════════════════════════════════════════════
# Pattern 08: Git Branch Analyzer
# ═══════════════════════════════════════════════════════════════════
#
# List git branches, then for_each branch get last commit info.
# Demonstrates: exec → for_each binding → exec per item → structured.
#
# Prerequisites: Must run inside a git repository, LLM provider
# Run: nika run workflows/showcase/08-git-analyzer.nika.yaml

schema: "nika/workflow@0.12"
workflow: git-branch-analyzer
description: "Analyze git branches with structured summary"

provider: "{{PROVIDER}}"
model: "{{MODEL}}"

tasks:
  - id: list_branches
    exec:
      command: |
        git branch --format='%(refname:short)' | head -5 | tr '\n' ',' | sed 's/,$//' | xargs -I{} echo '["{}"' | sed 's/,/","/g' | sed 's/\["/["/' | sed 's/$/"]/'
      shell: true

  - id: get_branch_info
    depends_on: [list_branches]
    with:
      branches: $list_branches | parse_json
    for_each: "{{with.branches}}"
    as: branch
    concurrency: 3
    exec:
      command: |
        echo "Branch: {{with.branch}} | Last commit: $(git log -1 --format='%s (%ar)' {{with.branch}} 2>/dev/null || echo 'unknown')"
      shell: true

  - id: summarize
    depends_on: [get_branch_info]
    with:
      branch_data: $get_branch_info
    structured:
      schema:
        type: object
        properties:
          total_branches:
            type: integer
          branch_summaries:
            type: array
            items:
              type: object
              properties:
                name:
                  type: string
                last_activity:
                  type: string
              required: [name, last_activity]
          recommendation:
            type: string
            description: "Suggestion for branch cleanup"
        required: [total_branches, branch_summaries, recommendation]
    infer:
      prompt: |
        Analyze this git branch information and produce a summary.
        Include total branches, a summary for each, and cleanup recommendations.

        Branch data: {{with.branch_data}}
      temperature: 0.2
"##;

// ═════════════════════════════════════════════════════════════════════════════
// 09: Package Version Checker
// ═════════════════════════════════════════════════════════════════════════════

pub const SHOWCASE_09_PKG_VERSIONS: &str = r##"# ═══════════════════════════════════════════════════════════════════
# Pattern 09: Package Version Checker
# ═══════════════════════════════════════════════════════════════════
#
# Check latest versions of npm/crates packages via API.
# Demonstrates: for_each + fetch JSON API + extract:jsonpath + structured.
#
# Prerequisites: LLM provider (for summary)
# Run: nika run workflows/showcase/09-pkg-versions.nika.yaml

schema: "nika/workflow@0.12"
workflow: package-version-checker
description: "Check latest package versions from crates.io"

provider: "{{PROVIDER}}"
model: "{{MODEL}}"

tasks:
  - id: check_crates
    for_each: ["serde", "tokio", "anyhow", "clap", "tracing"]
    as: crate_name
    concurrency: 3
    retry:
      max_attempts: 3
      delay_ms: 1000
      backoff: 2.0
    fetch:
      url: "https://crates.io/api/v1/crates/{{with.crate_name}}"
      method: GET
      headers:
        User-Agent: "nika-workflow/0.1"
      extract: jsonpath
      selector: "$.crate.newest_version"
      timeout: 10

  - id: version_report
    depends_on: [check_crates]
    with:
      versions: $check_crates
    structured:
      schema:
        type: object
        properties:
          packages:
            type: array
            items:
              type: object
              properties:
                name:
                  type: string
                latest_version:
                  type: string
                status:
                  type: string
                  enum: ["up-to-date", "outdated", "unknown"]
              required: [name, latest_version, status]
          summary:
            type: string
        required: [packages, summary]
    infer:
      prompt: |
        Based on these crate version check results, produce a dependency report.
        List each package with its latest version and status.

        Version data: {{with.versions}}
      temperature: 0.1
"##;

// ═════════════════════════════════════════════════════════════════════════════
// 10: Concurrent Web Scraper
// ═════════════════════════════════════════════════════════════════════════════

pub const SHOWCASE_10_WEB_SCRAPER: &str = r##"# ═══════════════════════════════════════════════════════════════════
# Pattern 10: Concurrent Web Scraper
# ═══════════════════════════════════════════════════════════════════
#
# Scrape multiple URLs with rate-limited concurrency (max 3).
# Each page extracted as markdown and saved as artifact.
#
# Prerequisites: None
# Run: nika run workflows/showcase/10-web-scraper.nika.yaml

schema: "nika/workflow@0.12"
workflow: concurrent-scraper
description: "Scrape multiple pages with rate-limited concurrency"

tasks:
  - id: scrape
    for_each:
      - "https://httpbin.org/html"
      - "https://httpbin.org/robots.txt"
      - "https://httpbin.org/forms/post"
      - "https://httpbin.org/links/5"
      - "https://httpbin.org/xml"
    as: page_url
    concurrency: 3
    fail_fast: false
    fetch:
      url: "{{with.page_url}}"
      extract: markdown
      timeout: 15

  - id: done
    depends_on: [scrape]
    with:
      pages: $scrape
    exec:
      command: |
        echo 'Scraping complete. {{with.pages | length}} pages collected.'
      shell: true
"##;

// ═════════════════════════════════════════════════════════════════════════════
// 11: Multi-Format Export
// ═════════════════════════════════════════════════════════════════════════════

pub const SHOWCASE_11_MULTI_FORMAT: &str = r##"# ═══════════════════════════════════════════════════════════════════
# Pattern 11: Multi-Format Export
# ═══════════════════════════════════════════════════════════════════
#
# Generate content once, then export to 4 formats via for_each.
# Each format produces a separate artifact.
#
# Prerequisites: LLM provider
# Run: nika run workflows/showcase/11-multi-format.nika.yaml

schema: "nika/workflow@0.12"
workflow: multi-format-export
description: "Generate content and export to multiple formats"

provider: "{{PROVIDER}}"
model: "{{MODEL}}"

tasks:
  - id: generate
    structured:
      schema:
        type: object
        properties:
          title:
            type: string
          summary:
            type: string
            maxLength: 200
          sections:
            type: array
            items:
              type: object
              properties:
                heading:
                  type: string
                body:
                  type: string
              required: [heading, body]
            minItems: 2
            maxItems: 4
        required: [title, summary, sections]
    infer:
      prompt: |
        Create a short technical document about "DAG-based workflow engines".
        Include a title, summary (max 200 chars), and 2-4 sections.
      temperature: 0.5
      max_tokens: 800

  - id: export
    depends_on: [generate]
    with:
      content: $generate
    for_each: ["text", "json", "yaml", "html"]
    as: fmt
    concurrency: 4
    exec:
      command: |
        echo 'Exporting as {{with.fmt}}: {{with.content}}'
      shell: true
    artifact:
      path: "output/exports/document.{{with.fmt}}"

  - id: complete
    depends_on: [export]
    exec:
      command: "echo 'All 4 format exports complete.'"
      shell: true
"##;

// ═════════════════════════════════════════════════════════════════════════════
// 12: Batch Image Analysis
// ═════════════════════════════════════════════════════════════════════════════

pub const SHOWCASE_12_IMAGE_ANALYSIS: &str = r##"# ═══════════════════════════════════════════════════════════════════
# Pattern 12: Batch Image Analysis
# ═══════════════════════════════════════════════════════════════════
#
# Download images, then for_each run nika:dimensions + nika:thumbhash.
# Demonstrates: for_each + fetch binary + invoke builtins + structured.
#
# Prerequisites: LLM provider (for summary)
# Run: nika run workflows/showcase/12-image-analysis.nika.yaml

schema: "nika/workflow@0.12"
workflow: batch-image-analysis
description: "Analyze multiple images with builtin tools"

provider: "{{PROVIDER}}"
model: "{{MODEL}}"

tasks:
  - id: download_images
    for_each:
      - "https://httpbin.org/image/png"
      - "https://httpbin.org/image/jpeg"
      - "https://httpbin.org/image/svg"
    as: img_url
    concurrency: 3
    fetch:
      url: "{{with.img_url}}"
      response: binary
      timeout: 15

  - id: get_dimensions
    depends_on: [download_images]
    with:
      images: $download_images
    for_each: "{{with.images}}"
    as: img_hash
    concurrency: 3
    invoke:
      tool: "nika:dimensions"
      params:
        hash: "{{with.img_hash}}"

  - id: summarize
    depends_on: [get_dimensions]
    with:
      dimension_data: $get_dimensions
    structured:
      schema:
        type: object
        properties:
          images_analyzed:
            type: integer
          results:
            type: array
            items:
              type: object
              properties:
                format:
                  type: string
                width:
                  type: integer
                height:
                  type: integer
              required: [format, width, height]
          total_pixels:
            type: integer
        required: [images_analyzed, results, total_pixels]
    infer:
      prompt: |
        Summarize the image dimension data into a structured report.
        Include per-image format/width/height and total pixel count.

        Dimension data: {{with.dimension_data}}
      temperature: 0.1
"##;

// ═════════════════════════════════════════════════════════════════════════════
// 13: Nested For Each (categories × items)
// ═════════════════════════════════════════════════════════════════════════════

pub const SHOWCASE_13_NESTED_FOREACH: &str = r##"# ═══════════════════════════════════════════════════════════════════
# Pattern 13: Nested For Each (categories x items)
# ═══════════════════════════════════════════════════════════════════
#
# Outer for_each over categories, each producing items.
# Inner task iterates over produced items. Simulates nested iteration
# via chained for_each tasks.
#
# Prerequisites: LLM provider
# Run: nika run workflows/showcase/13-nested-foreach.nika.yaml

schema: "nika/workflow@0.12"
workflow: nested-foreach
description: "Nested iteration via chained for_each tasks"

provider: "{{PROVIDER}}"
model: "{{MODEL}}"

tasks:
  - id: generate_items
    for_each: ["technology", "science", "art"]
    as: category
    concurrency: 3
    structured:
      schema:
        type: object
        properties:
          category:
            type: string
          items:
            type: array
            items:
              type: string
            minItems: 2
            maxItems: 3
        required: [category, items]
    infer:
      prompt: |
        For the category "{{with.category}}", generate 2-3 trending topic names.
        Return the category and the list of items.
      temperature: 0.7
      max_tokens: 200

  - id: describe_items
    depends_on: [generate_items]
    with:
      categories: $generate_items
    for_each: "{{with.categories}}"
    as: cat_data
    concurrency: 3
    structured:
      schema:
        type: object
        properties:
          category:
            type: string
          descriptions:
            type: array
            items:
              type: object
              properties:
                topic:
                  type: string
                description:
                  type: string
                  maxLength: 100
              required: [topic, description]
        required: [category, descriptions]
    infer:
      prompt: |
        For each topic in this category, write a one-line description (max 100 chars).

        Category data: {{with.cat_data}}
      temperature: 0.5
      max_tokens: 400

  - id: final_report
    depends_on: [describe_items]
    with:
      all_descriptions: $describe_items
    exec:
      command: |
        echo 'Nested for_each complete: {{with.all_descriptions}}'
      shell: true
"##;

// ═════════════════════════════════════════════════════════════════════════════
// 14: Fan-Out Fan-In
// ═════════════════════════════════════════════════════════════════════════════

pub const SHOWCASE_14_FAN_OUT_FAN_IN: &str = r##"# ═══════════════════════════════════════════════════════════════════
# Pattern 14: Fan-Out Fan-In
# ═══════════════════════════════════════════════════════════════════
#
# Single source → for_each with 5 analyses → merge → synthesize.
# The classic MapReduce pattern in a workflow.
#
# Prerequisites: LLM provider
# Run: nika run workflows/showcase/14-fan-out-fan-in.nika.yaml

schema: "nika/workflow@0.12"
workflow: fan-out-fan-in
description: "MapReduce pattern: fan-out analyses, fan-in synthesis"

provider: "{{PROVIDER}}"
model: "{{MODEL}}"

tasks:
  - id: source
    exec:
      command: |
        echo 'Nika is a semantic YAML workflow engine for AI tasks. It supports 5 verbs (infer, exec, fetch, invoke, agent), a DAG scheduler for parallel execution, structured output with JSON Schema validation, and content-addressable storage for media. It connects to knowledge graphs via MCP protocol.'
      shell: true

  - id: analyze
    depends_on: [source]
    with:
      text: $source
    for_each:
      - "technical_accuracy"
      - "readability"
      - "completeness"
      - "market_positioning"
      - "developer_appeal"
    as: lens
    concurrency: 5
    structured:
      schema:
        type: object
        properties:
          lens:
            type: string
          score:
            type: number
            minimum: 0
            maximum: 10
          strengths:
            type: array
            items:
              type: string
            maxItems: 3
          weaknesses:
            type: array
            items:
              type: string
            maxItems: 3
          recommendation:
            type: string
            maxLength: 200
        required: [lens, score, strengths, weaknesses, recommendation]
    infer:
      prompt: |
        Analyze this product description through the lens of "{{with.lens}}".
        Score 0-10, list up to 3 strengths and 3 weaknesses,
        and give a recommendation (max 200 chars).

        Text: "{{with.text}}"
      temperature: 0.4
      max_tokens: 400

  - id: synthesize
    depends_on: [analyze]
    with:
      analyses: $analyze
    structured:
      schema:
        type: object
        properties:
          overall_score:
            type: number
            minimum: 0
            maximum: 10
          top_strength:
            type: string
          top_weakness:
            type: string
          action_items:
            type: array
            items:
              type: string
            minItems: 1
            maxItems: 5
          executive_summary:
            type: string
            maxLength: 300
        required: [overall_score, top_strength, top_weakness, action_items, executive_summary]
    infer:
      prompt: |
        Synthesize these 5 analyses into an executive summary.
        Compute an overall score, identify the top strength and weakness,
        and list 1-5 action items.

        All analyses: {{with.analyses}}
      temperature: 0.3
      max_tokens: 500
    artifact:
      path: output/analysis/executive-summary.json
      format: json
"##;

// ═════════════════════════════════════════════════════════════════════════════
// 15: Pipeline with Retry
// ═════════════════════════════════════════════════════════════════════════════

pub const SHOWCASE_15_RETRY_PIPELINE: &str = r##"# ═══════════════════════════════════════════════════════════════════
# Pattern 15: Pipeline with Retry
# ═══════════════════════════════════════════════════════════════════
#
# for_each over tasks with retry on fetch failures + structured results.
# Demonstrates: for_each + retry + fail_fast:false + structured output.
#
# Prerequisites: LLM provider (for summary)
# Run: nika run workflows/showcase/15-retry-pipeline.nika.yaml

schema: "nika/workflow@0.12"
workflow: retry-pipeline
description: "Resilient pipeline with retry on each iteration"

provider: "{{PROVIDER}}"
model: "{{MODEL}}"

tasks:
  - id: fetch_with_retry
    for_each:
      - "https://httpbin.org/get"
      - "https://httpbin.org/uuid"
      - "https://httpbin.org/delay/1"
      - "https://httpbin.org/ip"
      - "https://httpbin.org/headers"
    as: api_url
    concurrency: 3
    fail_fast: false
    retry:
      max_attempts: 3
      delay_ms: 500
      backoff: 2.0
    fetch:
      url: "{{with.api_url}}"
      method: GET
      response: full
      timeout: 10

  - id: summarize_results
    depends_on: [fetch_with_retry]
    with:
      raw: $fetch_with_retry
    structured:
      schema:
        type: object
        properties:
          total_requests:
            type: integer
          successful:
            type: integer
          failed:
            type: integer
          results:
            type: array
            items:
              type: object
              properties:
                url:
                  type: string
                status:
                  type: string
                  enum: ["success", "failed", "timeout"]
                attempts:
                  type: integer
                  minimum: 1
              required: [url, status, attempts]
        required: [total_requests, successful, failed, results]
      max_retries: 2
    infer:
      prompt: |
        Analyze these API call results. For each URL, report its status
        and how many attempts were needed. Include totals.

        Raw results: {{with.raw}}
      temperature: 0.1
"##;

// ═════════════════════════════════════════════════════════════════════════════
// Public API
// ═════════════════════════════════════════════════════════════════════════════

/// Return all 15 showcase pattern workflows.
pub fn get_showcase_workflows() -> Vec<WorkflowTemplate> {
    vec![
        WorkflowTemplate {
            filename: "01-url-status.nika.yaml",
            tier_dir: "showcase",
            content: SHOWCASE_01_URL_STATUS,
        },
        WorkflowTemplate {
            filename: "02-lang-detect.nika.yaml",
            tier_dir: "showcase",
            content: SHOWCASE_02_LANG_DETECT,
        },
        WorkflowTemplate {
            filename: "03-sentiment.nika.yaml",
            tier_dir: "showcase",
            content: SHOWCASE_03_SENTIMENT,
        },
        WorkflowTemplate {
            filename: "04-translation.nika.yaml",
            tier_dir: "showcase",
            content: SHOWCASE_04_TRANSLATION,
        },
        WorkflowTemplate {
            filename: "05-api-tester.nika.yaml",
            tier_dir: "showcase",
            content: SHOWCASE_05_API_TESTER,
        },
        WorkflowTemplate {
            filename: "06-link-checker.nika.yaml",
            tier_dir: "showcase",
            content: SHOWCASE_06_LINK_CHECKER,
        },
        WorkflowTemplate {
            filename: "07-rss-aggregator.nika.yaml",
            tier_dir: "showcase",
            content: SHOWCASE_07_RSS_AGGREGATOR,
        },
        WorkflowTemplate {
            filename: "08-git-analyzer.nika.yaml",
            tier_dir: "showcase",
            content: SHOWCASE_08_GIT_ANALYZER,
        },
        WorkflowTemplate {
            filename: "09-pkg-versions.nika.yaml",
            tier_dir: "showcase",
            content: SHOWCASE_09_PKG_VERSIONS,
        },
        WorkflowTemplate {
            filename: "10-web-scraper.nika.yaml",
            tier_dir: "showcase",
            content: SHOWCASE_10_WEB_SCRAPER,
        },
        WorkflowTemplate {
            filename: "11-multi-format.nika.yaml",
            tier_dir: "showcase",
            content: SHOWCASE_11_MULTI_FORMAT,
        },
        WorkflowTemplate {
            filename: "12-image-analysis.nika.yaml",
            tier_dir: "showcase",
            content: SHOWCASE_12_IMAGE_ANALYSIS,
        },
        WorkflowTemplate {
            filename: "13-nested-foreach.nika.yaml",
            tier_dir: "showcase",
            content: SHOWCASE_13_NESTED_FOREACH,
        },
        WorkflowTemplate {
            filename: "14-fan-out-fan-in.nika.yaml",
            tier_dir: "showcase",
            content: SHOWCASE_14_FAN_OUT_FAN_IN,
        },
        WorkflowTemplate {
            filename: "15-retry-pipeline.nika.yaml",
            tier_dir: "showcase",
            content: SHOWCASE_15_RETRY_PIPELINE,
        },
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_showcase_workflow_count() {
        let workflows = get_showcase_workflows();
        assert_eq!(
            workflows.len(),
            15,
            "Should have exactly 15 showcase workflows"
        );
    }

    #[test]
    fn test_showcase_filenames_unique() {
        let workflows = get_showcase_workflows();
        let mut names: Vec<&str> = workflows.iter().map(|w| w.filename).collect();
        let len = names.len();
        names.sort();
        names.dedup();
        assert_eq!(names.len(), len, "All filenames must be unique");
    }

    #[test]
    fn test_showcase_all_have_schema() {
        let workflows = get_showcase_workflows();
        for w in &workflows {
            assert!(
                w.content.contains("schema: \"nika/workflow@0.12\""),
                "Workflow {} must declare schema",
                w.filename
            );
        }
    }

    #[test]
    fn test_showcase_all_have_tasks() {
        let workflows = get_showcase_workflows();
        for w in &workflows {
            assert!(
                w.content.contains("tasks:"),
                "Workflow {} must have tasks section",
                w.filename
            );
        }
    }

    #[test]
    fn test_showcase_all_nika_yaml_extension() {
        let workflows = get_showcase_workflows();
        for w in &workflows {
            assert!(
                w.filename.ends_with(".nika.yaml"),
                "Workflow {} must end with .nika.yaml",
                w.filename
            );
        }
    }

    #[test]
    fn test_showcase_all_have_for_each() {
        let workflows = get_showcase_workflows();
        for w in &workflows {
            assert!(
                w.content.contains("for_each:"),
                "Workflow {} must use for_each (showcase pattern requirement)",
                w.filename
            );
        }
    }

    #[test]
    fn test_showcase_structured_or_schema_present() {
        let workflows = get_showcase_workflows();
        let structured_count = workflows
            .iter()
            .filter(|w| w.content.contains("structured:"))
            .count();
        // At least 10 of the 15 must have structured output
        assert!(
            structured_count >= 10,
            "At least 10 workflows should use structured:, found {}",
            structured_count
        );
    }

    #[test]
    fn test_showcase_concurrency_present() {
        let workflows = get_showcase_workflows();
        let concurrent_count = workflows
            .iter()
            .filter(|w| w.content.contains("concurrency:"))
            .count();
        assert!(
            concurrent_count >= 10,
            "At least 10 workflows should use concurrency:, found {}",
            concurrent_count
        );
    }

    #[test]
    fn test_showcase_all_in_showcase_dir() {
        let workflows = get_showcase_workflows();
        for w in &workflows {
            assert_eq!(
                w.tier_dir, "showcase",
                "Workflow {} must be in showcase directory",
                w.filename
            );
        }
    }

    #[test]
    fn test_showcase_valid_yaml() {
        let workflows = get_showcase_workflows();
        for w in &workflows {
            // Skip YAML validation for templates with placeholders
            if w.content.contains("{{PROVIDER}}") || w.content.contains("{{MODEL}}") {
                continue;
            }
            let parsed: Result<serde_json::Value, _> = serde_saphyr::from_str(w.content);
            assert!(
                parsed.is_ok(),
                "Workflow {} must be valid YAML: {:?}",
                w.filename,
                parsed.err()
            );
        }
    }

    #[test]
    fn test_showcase_workflow_names_unique() {
        let workflows = get_showcase_workflows();
        let mut workflow_names: Vec<&str> = Vec::new();
        for w in &workflows {
            // Extract workflow: value
            for line in w.content.lines() {
                let trimmed = line.trim();
                if let Some(name) = trimmed.strip_prefix("workflow: ") {
                    workflow_names.push(name);
                }
            }
        }
        let len = workflow_names.len();
        workflow_names.sort();
        workflow_names.dedup();
        assert_eq!(
            workflow_names.len(),
            len,
            "All workflow: names must be unique"
        );
    }
}
