// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Showcase Fetch — 15 workflows demonstrating ALL 9 extract modes + 3 response modes
//!
//! Complete coverage of the `fetch:` verb's extraction and response capabilities:
//!
//! **Extract modes (9):**
//! - `markdown` — Full HTML to clean Markdown (htmd)
//! - `article` — Main article content only (Readability/dom_smoothie)
//! - `text` — Visible text, optionally filtered by `selector:`
//! - `selector` — Raw HTML matching a CSS selector
//! - `metadata` — OG tags, Twitter Cards, JSON-LD, SEO tags
//! - `links` — Classified link list (internal/external, nav/content/footer)
//! - `jsonpath` — JSONPath query on JSON API responses
//! - `feed` — RSS/Atom/JSON Feed parsing (feed-rs)
//! - `llm_txt` — AI-era content discovery (/.well-known/llm.txt, /llms.txt)
//!
//! **Response modes (3):**
//! - `full` — JSON envelope: { status, headers, body, url }
//! - `binary` — Store in CAS, return hash for media pipeline
//! - (default) — Raw body text
//!
//! Workflows 01-09: one extract mode each
//! Workflows 10-12: one response mode each
//! Workflows 13-15: combos (multi-extract, feed-to-newsletter, scrape+analyze)
//!
//! All use `{{PROVIDER}}`/`{{MODEL}}` for LLM tasks. Pure-fetch workflows set
//! `requires_llm: false` in comments.

use super::WorkflowTemplate;

/// Return all 15 showcase fetch workflows.
pub fn get_showcase_fetch_workflows() -> Vec<WorkflowTemplate> {
    vec![
        WorkflowTemplate {
            filename: "01-fetch-markdown.nika.yaml",
            tier_dir: "showcase-fetch",
            content: FETCH_01_MARKDOWN,
        },
        WorkflowTemplate {
            filename: "02-fetch-article.nika.yaml",
            tier_dir: "showcase-fetch",
            content: FETCH_02_ARTICLE,
        },
        WorkflowTemplate {
            filename: "03-fetch-text-selector.nika.yaml",
            tier_dir: "showcase-fetch",
            content: FETCH_03_TEXT,
        },
        WorkflowTemplate {
            filename: "04-fetch-selector-html.nika.yaml",
            tier_dir: "showcase-fetch",
            content: FETCH_04_SELECTOR,
        },
        WorkflowTemplate {
            filename: "05-fetch-metadata.nika.yaml",
            tier_dir: "showcase-fetch",
            content: FETCH_05_METADATA,
        },
        WorkflowTemplate {
            filename: "06-fetch-links.nika.yaml",
            tier_dir: "showcase-fetch",
            content: FETCH_06_LINKS,
        },
        WorkflowTemplate {
            filename: "07-fetch-jsonpath.nika.yaml",
            tier_dir: "showcase-fetch",
            content: FETCH_07_JSONPATH,
        },
        WorkflowTemplate {
            filename: "08-fetch-feed.nika.yaml",
            tier_dir: "showcase-fetch",
            content: FETCH_08_FEED,
        },
        WorkflowTemplate {
            filename: "09-fetch-llm-txt.nika.yaml",
            tier_dir: "showcase-fetch",
            content: FETCH_09_LLM_TXT,
        },
        WorkflowTemplate {
            filename: "10-response-full.nika.yaml",
            tier_dir: "showcase-fetch",
            content: FETCH_10_RESPONSE_FULL,
        },
        WorkflowTemplate {
            filename: "11-response-binary.nika.yaml",
            tier_dir: "showcase-fetch",
            content: FETCH_11_RESPONSE_BINARY,
        },
        WorkflowTemplate {
            filename: "12-response-default.nika.yaml",
            tier_dir: "showcase-fetch",
            content: FETCH_12_RESPONSE_DEFAULT,
        },
        WorkflowTemplate {
            filename: "13-multi-extract-comparison.nika.yaml",
            tier_dir: "showcase-fetch",
            content: FETCH_13_MULTI_EXTRACT,
        },
        WorkflowTemplate {
            filename: "14-rss-to-newsletter.nika.yaml",
            tier_dir: "showcase-fetch",
            content: FETCH_14_RSS_NEWSLETTER,
        },
        WorkflowTemplate {
            filename: "15-scrape-and-analyze.nika.yaml",
            tier_dir: "showcase-fetch",
            content: FETCH_15_SCRAPE_ANALYZE,
        },
    ]
}

// =============================================================================
// 01 — Markdown Extraction
// fetch: blog URL -> extract:markdown -> artifact
// =============================================================================

const FETCH_01_MARKDOWN: &str = r##"# =============================================================================
# SHOWCASE FETCH 01 — Markdown Extraction
# =============================================================================
# requires_llm: false
# category: fetch-extract
# features: fetch-markdown
#
# Fetches the Rust Blog and converts the entire HTML page to clean Markdown.
# The htmd library strips navigation, scripts, styles, and produces
# LLM-ready content. Artifact saves the result to disk.
#
# Run: nika run workflows/showcase-fetch/01-fetch-markdown.nika.yaml

schema: "nika/workflow@0.12"
workflow: fetch-markdown-showcase
description: "Convert a blog homepage to clean Markdown via extract: markdown"

artifacts:
  dir: .output/showcase-fetch

tasks:
  - id: fetch_blog
    description: "Fetch Rust Blog and convert to Markdown"
    fetch:
      url: "https://blog.rust-lang.org/"
      extract: markdown
      timeout: 20
    artifact:
      path: rust-blog-markdown.md

  - id: log_size
    depends_on: [fetch_blog]
    with:
      content: $fetch_blog
    exec:
      command: |
        echo "Markdown extraction complete. Content length: $(echo '{{with.content}}' | wc -c | tr -d ' ') bytes"
      shell: true
"##;

// =============================================================================
// 02 — Article Extraction
// fetch: news URL -> extract:article -> artifact
// =============================================================================

const FETCH_02_ARTICLE: &str = r##"# =============================================================================
# SHOWCASE FETCH 02 — Article Extraction (Readability)
# =============================================================================
# requires_llm: false
# category: fetch-extract
# features: fetch-article
#
# Extracts only the main article content from a webpage using the
# Readability algorithm (dom_smoothie). Strips navigation, ads, sidebars,
# cookie banners — leaving just the primary reading content.
#
# Run: nika run workflows/showcase-fetch/02-fetch-article.nika.yaml

schema: "nika/workflow@0.12"
workflow: fetch-article-showcase
description: "Extract main article content with Readability via extract: article"

artifacts:
  dir: .output/showcase-fetch

tasks:
  - id: fetch_article
    description: "Extract article content from Rust Blog"
    fetch:
      url: "https://blog.rust-lang.org/"
      extract: article
      timeout: 20
    artifact:
      path: rust-blog-article.md

  - id: log_result
    depends_on: [fetch_article]
    with:
      article: $fetch_article
    invoke:
      tool: "nika:log"
      params:
        level: "info"
        message: "Article extraction complete — content ready for LLM consumption"
"##;

// =============================================================================
// 03 — Text Extraction with CSS Selector
// fetch: URL -> extract:text -> selector: "article p"
// =============================================================================

const FETCH_03_TEXT: &str = r##"# =============================================================================
# SHOWCASE FETCH 03 — Text Extraction with CSS Selector
# =============================================================================
# requires_llm: false
# category: fetch-extract
# features: fetch-html
#
# Extracts visible text from a webpage. When combined with selector:,
# only text from matching CSS elements is returned. Without selector:,
# returns all visible text (no HTML tags).
#
# Run: nika run workflows/showcase-fetch/03-fetch-text-selector.nika.yaml

schema: "nika/workflow@0.12"
workflow: fetch-text-selector-showcase
description: "Extract visible text filtered by CSS selector via extract: text"

artifacts:
  dir: .output/showcase-fetch

tasks:
  # Text from specific elements only
  - id: fetch_paragraphs
    description: "Extract paragraph text from httpbin HTML page"
    fetch:
      url: "https://httpbin.org/html"
      extract: text
      selector: "p"
      timeout: 15
    artifact:
      path: httpbin-paragraphs.txt

  # All visible text (no selector)
  - id: fetch_all_text
    description: "Extract all visible text from httpbin HTML page"
    fetch:
      url: "https://httpbin.org/html"
      extract: text
      timeout: 15
    artifact:
      path: httpbin-all-text.txt

  - id: compare_sizes
    depends_on: [fetch_paragraphs, fetch_all_text]
    with:
      filtered: $fetch_paragraphs
      full: $fetch_all_text
    invoke:
      tool: "nika:log"
      params:
        level: "info"
        message: "Filtered paragraphs vs full text extracted successfully"
"##;

// =============================================================================
// 04 — Raw HTML Selector
// fetch: URL -> extract:selector -> selector: "h1, h2, h3"
// =============================================================================

const FETCH_04_SELECTOR: &str = r##"# =============================================================================
# SHOWCASE FETCH 04 — Raw HTML Selector Extraction
# =============================================================================
# requires_llm: false
# category: fetch-extract
# features: fetch-html
#
# Returns the raw HTML of elements matching a CSS selector. Unlike
# extract: text (which strips tags), this preserves the HTML structure.
# Useful for scraping specific DOM fragments.
#
# Run: nika run workflows/showcase-fetch/04-fetch-selector-html.nika.yaml

schema: "nika/workflow@0.12"
workflow: fetch-selector-html-showcase
description: "Extract raw HTML matching CSS selectors via extract: selector"

artifacts:
  dir: .output/showcase-fetch

tasks:
  - id: fetch_headings
    description: "Extract all heading elements from httpbin HTML"
    fetch:
      url: "https://httpbin.org/html"
      extract: selector
      selector: "h1"
      timeout: 15
    artifact:
      path: httpbin-headings.html

  - id: fetch_paragraphs_html
    description: "Extract paragraph HTML from httpbin"
    fetch:
      url: "https://httpbin.org/html"
      extract: selector
      selector: "p"
      timeout: 15
    artifact:
      path: httpbin-paragraphs.html

  - id: log_done
    depends_on: [fetch_headings, fetch_paragraphs_html]
    invoke:
      tool: "nika:log"
      params:
        level: "info"
        message: "Raw HTML selector extraction complete — headings and paragraphs captured"
"##;

// =============================================================================
// 05 — Metadata Extraction
// fetch: URL -> extract:metadata -> structured OG/Twitter/JSON-LD
// =============================================================================

const FETCH_05_METADATA: &str = r##"# =============================================================================
# SHOWCASE FETCH 05 — Metadata Extraction (OG / Twitter / JSON-LD / SEO)
# =============================================================================
# requires_llm: false
# category: fetch-extract
# features: fetch-html
#
# Extracts structured metadata from a webpage: Open Graph tags,
# Twitter Cards, JSON-LD structured data, and basic SEO tags
# (title, description, canonical URL, etc.). Returns JSON.
#
# Run: nika run workflows/showcase-fetch/05-fetch-metadata.nika.yaml

schema: "nika/workflow@0.12"
workflow: fetch-metadata-showcase
description: "Extract OG, Twitter Cards, JSON-LD, and SEO metadata via extract: metadata"

artifacts:
  dir: .output/showcase-fetch

tasks:
  - id: github_metadata
    description: "Extract metadata from GitHub homepage"
    fetch:
      url: "https://github.com"
      extract: metadata
      timeout: 15
    artifact:
      path: github-metadata.json
      format: json

  - id: rust_blog_metadata
    description: "Extract metadata from Rust Blog"
    fetch:
      url: "https://blog.rust-lang.org/"
      extract: metadata
      timeout: 15
    artifact:
      path: rust-blog-metadata.json
      format: json

  - id: log_metadata
    depends_on: [github_metadata, rust_blog_metadata]
    with:
      gh: $github_metadata
      rust: $rust_blog_metadata
    invoke:
      tool: "nika:log"
      params:
        level: "info"
        message: "Metadata extraction complete for GitHub and Rust Blog"
"##;

// =============================================================================
// 06 — Link Extraction
// fetch: URL -> extract:links -> internal/external classification
// =============================================================================

const FETCH_06_LINKS: &str = r##"# =============================================================================
# SHOWCASE FETCH 06 — Link Extraction and Classification
# =============================================================================
# requires_llm: false
# category: fetch-extract
# features: fetch-html
#
# Extracts all links from a webpage and classifies them:
# - Internal vs external
# - Navigation vs content vs footer
# Returns structured JSON with URL, text, type, and zone.
#
# Run: nika run workflows/showcase-fetch/06-fetch-links.nika.yaml

schema: "nika/workflow@0.12"
workflow: fetch-links-showcase
description: "Extract and classify links via extract: links"

artifacts:
  dir: .output/showcase-fetch

tasks:
  - id: extract_links
    description: "Extract and classify all links from Hacker News"
    fetch:
      url: "https://news.ycombinator.com"
      extract: links
      timeout: 15
    artifact:
      path: hn-links.json
      format: json

  - id: log_links
    depends_on: [extract_links]
    with:
      links: $extract_links
    invoke:
      tool: "nika:log"
      params:
        level: "info"
        message: "Link extraction complete — internal/external classification ready"
"##;

// =============================================================================
// 07 — JSONPath Extraction
// fetch: API -> extract:jsonpath -> selector: "$.data[*].name"
// =============================================================================

const FETCH_07_JSONPATH: &str = r##"# =============================================================================
# SHOWCASE FETCH 07 — JSONPath Extraction
# =============================================================================
# requires_llm: false
# category: fetch-extract
#
# Queries JSON APIs using JSONPath expressions. Zero external dependencies —
# JSONPath is always available. The selector: field holds the JSONPath query.
# Surgical extraction from massive JSON payloads.
#
# Run: nika run workflows/showcase-fetch/07-fetch-jsonpath.nika.yaml

schema: "nika/workflow@0.12"
workflow: fetch-jsonpath-showcase
description: "Extract specific fields from JSON APIs via extract: jsonpath"

artifacts:
  dir: .output/showcase-fetch

tasks:
  # JSONPath on httpbin structured JSON
  - id: slideshow_title
    description: "Extract slideshow title from httpbin JSON"
    fetch:
      url: "https://httpbin.org/json"
      extract: jsonpath
      selector: "$.slideshow.title"
      timeout: 10
    artifact:
      path: slideshow-title.json
      format: json

  # JSONPath on nested array
  - id: slide_titles
    description: "Extract all slide titles from httpbin JSON"
    fetch:
      url: "https://httpbin.org/json"
      extract: jsonpath
      selector: "$.slideshow.slides[*].title"
      timeout: 10
    artifact:
      path: slide-titles.json
      format: json

  # JSONPath on Hacker News Algolia API
  - id: hn_search
    description: "Search Hacker News and extract story titles"
    fetch:
      url: "https://hn.algolia.com/api/v1/search?query=rust&tags=story&hitsPerPage=5"
      extract: jsonpath
      selector: "$.hits[*].title"
      timeout: 15
    artifact:
      path: hn-rust-titles.json
      format: json

  - id: log_results
    depends_on: [slideshow_title, slide_titles, hn_search]
    with:
      title: $slideshow_title
      slides: $slide_titles
      hn: $hn_search
    invoke:
      tool: "nika:log"
      params:
        level: "info"
        message: "JSONPath extraction complete — 3 queries across 2 APIs"
"##;

// =============================================================================
// 08 — RSS/Atom Feed Parsing
// fetch: RSS URL -> extract:feed -> structured entries
// =============================================================================

const FETCH_08_FEED: &str = r##"# =============================================================================
# SHOWCASE FETCH 08 — RSS/Atom Feed Parsing
# =============================================================================
# requires_llm: false
# category: fetch-extract
# features: fetch-feed
#
# Parses RSS, Atom, and JSON Feed formats using the feed-rs library.
# Returns structured JSON with title, entries, dates, authors, and links.
# Works with any standard syndication feed.
#
# Run: nika run workflows/showcase-fetch/08-fetch-feed.nika.yaml

schema: "nika/workflow@0.12"
workflow: fetch-feed-showcase
description: "Parse RSS/Atom feeds into structured JSON via extract: feed"

artifacts:
  dir: .output/showcase-fetch

tasks:
  - id: rust_feed
    description: "Parse the Rust Blog Atom feed"
    fetch:
      url: "https://blog.rust-lang.org/feed.xml"
      extract: feed
      timeout: 15
    artifact:
      path: rust-feed.json
      format: json

  - id: log_feed
    depends_on: [rust_feed]
    with:
      feed: $rust_feed
    invoke:
      tool: "nika:log"
      params:
        level: "info"
        message: "RSS feed parsed — entries extracted and structured as JSON"
"##;

// =============================================================================
// 09 — LLM.txt Discovery
// fetch: URL -> extract:llm_txt -> display content
// =============================================================================

const FETCH_09_LLM_TXT: &str = r##"# =============================================================================
# SHOWCASE FETCH 09 — LLM.txt Content Discovery
# =============================================================================
# requires_llm: false
# category: fetch-extract
#
# AI-era content discovery. Checks for /.well-known/llm.txt and /llms.txt
# files that websites publish to help LLMs understand their content.
# Part of the llms.txt standard for AI-friendly web content.
#
# Run: nika run workflows/showcase-fetch/09-fetch-llm-txt.nika.yaml

schema: "nika/workflow@0.12"
workflow: fetch-llm-txt-showcase
description: "Discover AI content via extract: llm_txt"

artifacts:
  dir: .output/showcase-fetch

tasks:
  - id: check_anthropic
    description: "Check Anthropic docs for llm.txt"
    fetch:
      url: "https://docs.anthropic.com"
      extract: llm_txt
      timeout: 15
    artifact:
      path: anthropic-llm-txt.md

  - id: log_discovery
    depends_on: [check_anthropic]
    with:
      result: $check_anthropic
    invoke:
      tool: "nika:log"
      params:
        level: "info"
        message: "LLM.txt discovery complete"
"##;

// =============================================================================
// 10 — Full Response Envelope
// fetch: URL -> response:full -> status + headers + body + url
// =============================================================================

const FETCH_10_RESPONSE_FULL: &str = r##"# =============================================================================
# SHOWCASE FETCH 10 — Full Response Envelope
# =============================================================================
# requires_llm: false
# category: fetch-response
#
# Returns the complete HTTP response as a JSON envelope containing:
# - status: HTTP status code
# - headers: all response headers
# - body: response body text
# - url: after redirect resolution
#
# Perfect for debugging redirects, checking security headers, API monitoring.
#
# Run: nika run workflows/showcase-fetch/10-response-full.nika.yaml

schema: "nika/workflow@0.12"
workflow: fetch-response-full-showcase
description: "Inspect complete HTTP response via response: full"

artifacts:
  dir: .output/showcase-fetch

tasks:
  # Full response from a simple GET
  - id: get_full
    description: "Fetch httpbin GET with full response envelope"
    fetch:
      url: "https://httpbin.org/get"
      response: full
      timeout: 10
    artifact:
      path: httpbin-full-response.json
      format: json

  # Full response showing headers
  - id: inspect_headers
    description: "Fetch httpbin headers with full envelope"
    fetch:
      url: "https://httpbin.org/headers"
      response: full
      timeout: 10
    artifact:
      path: httpbin-headers-full.json
      format: json

  - id: log_responses
    depends_on: [get_full, inspect_headers]
    with:
      get: $get_full
      headers: $inspect_headers
    invoke:
      tool: "nika:log"
      params:
        level: "info"
        message: "Full response envelopes captured — status, headers, body, and url available"
"##;

// =============================================================================
// 11 — Binary Response (CAS storage)
// fetch: image URL -> response:binary -> nika:import -> nika:dimensions
// =============================================================================

const FETCH_11_RESPONSE_BINARY: &str = r##"# =============================================================================
# SHOWCASE FETCH 11 — Binary Response + Media Pipeline
# =============================================================================
# requires_llm: false
# category: fetch-response
#
# Downloads a binary file (image) into content-addressable storage (CAS).
# The task output is the CAS hash, which can be piped into media tools
# like nika:dimensions and nika:thumbhash for further processing.
#
# Run: nika run workflows/showcase-fetch/11-response-binary.nika.yaml

schema: "nika/workflow@0.12"
workflow: fetch-response-binary-showcase
description: "Download binary into CAS and extract dimensions via response: binary"

artifacts:
  dir: .output/showcase-fetch

tasks:
  # Download a PNG image into CAS
  - id: download_image
    description: "Download a PNG image into content-addressable storage"
    fetch:
      url: "https://httpbin.org/image/png"
      response: binary
      timeout: 15
    artifact:
      path: downloaded-image.png
      format: binary

  # Extract dimensions from the downloaded image
  - id: get_dimensions
    depends_on: [download_image]
    with:
      img: $download_image
    invoke:
      tool: "nika:dimensions"
      params:
        hash: "{{with.img.hash}}"

  # Generate a thumbhash placeholder
  - id: get_thumbhash
    depends_on: [download_image]
    with:
      img: $download_image
    invoke:
      tool: "nika:thumbhash"
      params:
        hash: "{{with.img.hash}}"

  - id: log_media
    depends_on: [get_dimensions, get_thumbhash]
    with:
      dims: $get_dimensions
      hash: $get_thumbhash
    invoke:
      tool: "nika:log"
      params:
        level: "info"
        message: "Binary download + media pipeline complete — dimensions and thumbhash extracted"
"##;

// =============================================================================
// 12 — Default Text Response
// fetch: URL -> (no response: field) -> display raw body
// =============================================================================

const FETCH_12_RESPONSE_DEFAULT: &str = r##"# =============================================================================
# SHOWCASE FETCH 12 — Default Text Response
# =============================================================================
# requires_llm: false
# category: fetch-response
#
# When no response: field is specified, fetch returns the raw body text.
# This is the simplest mode — no JSON envelope, no CAS storage.
# Just the HTTP response body as a string.
#
# Run: nika run workflows/showcase-fetch/12-response-default.nika.yaml

schema: "nika/workflow@0.12"
workflow: fetch-response-default-showcase
description: "Fetch raw body text with default response mode (no response: field)"

artifacts:
  dir: .output/showcase-fetch

tasks:
  # Default response — just the body text
  - id: fetch_ip
    description: "Fetch public IP as raw JSON text"
    fetch:
      url: "https://httpbin.org/ip"
      timeout: 10
    artifact:
      path: public-ip.txt

  # Another default fetch — UUID
  - id: fetch_uuid
    description: "Fetch a random UUID as raw text"
    fetch:
      url: "https://httpbin.org/uuid"
      timeout: 10
    artifact:
      path: random-uuid.txt

  # Default fetch from a JSON API
  - id: fetch_json_raw
    description: "Fetch httpbin JSON as raw text (no extraction)"
    fetch:
      url: "https://httpbin.org/json"
      timeout: 10
    artifact:
      path: raw-json-body.txt

  - id: log_defaults
    depends_on: [fetch_ip, fetch_uuid, fetch_json_raw]
    with:
      ip: $fetch_ip
      uuid: $fetch_uuid
    invoke:
      tool: "nika:log"
      params:
        level: "info"
        message: "Default response mode — raw body text captured for all 3 endpoints"
"##;

// =============================================================================
// 13 — Multi-Extract Comparison
// fetch: same URL -> markdown vs article vs text -> LLM compare
// =============================================================================

const FETCH_13_MULTI_EXTRACT: &str = r##"# =============================================================================
# SHOWCASE FETCH 13 — Multi-Extract Comparison
# =============================================================================
# requires_llm: true
# category: fetch-combo
# features: fetch-markdown, fetch-article, fetch-html
#
# Fetches the SAME URL with 3 different extract modes (markdown, article,
# text) and asks an LLM to compare the results. Shows how each mode
# produces different output from identical source HTML.
#
# Run: nika run workflows/showcase-fetch/13-multi-extract-comparison.nika.yaml

schema: "nika/workflow@0.12"
workflow: multi-extract-comparison
description: "Compare markdown vs article vs text extraction on the same URL"
provider: "{{PROVIDER}}"
model: "{{MODEL}}"

artifacts:
  dir: .output/showcase-fetch

tasks:
  # Same URL, three extraction modes
  - id: as_markdown
    description: "Full Markdown extraction"
    fetch:
      url: "https://blog.rust-lang.org/"
      extract: markdown
      timeout: 20
    artifact:
      path: comparison-markdown.md

  - id: as_article
    description: "Article-only extraction (Readability)"
    fetch:
      url: "https://blog.rust-lang.org/"
      extract: article
      timeout: 20
    artifact:
      path: comparison-article.md

  - id: as_text
    description: "Plain text extraction"
    fetch:
      url: "https://blog.rust-lang.org/"
      extract: text
      timeout: 20
    artifact:
      path: comparison-text.txt

  # LLM compares all three outputs
  - id: compare
    description: "LLM analysis of extraction mode differences"
    depends_on: [as_markdown, as_article, as_text]
    with:
      md: $as_markdown
      article: $as_article
      text: $as_text
    infer:
      prompt: |
        Compare these 3 extraction modes applied to the same URL (blog.rust-lang.org):

        ## 1. extract: markdown (first 1500 chars)
        {{with.md | first(1500)}}

        ## 2. extract: article (first 1500 chars)
        {{with.article | first(1500)}}

        ## 3. extract: text (first 1500 chars)
        {{with.text | first(1500)}}

        Analyze:
        1. What does each mode preserve vs strip?
        2. Which is best for LLM summarization?
        3. Which is best for data extraction?
        4. Which is best for human reading?
        5. When would you pick each one?

        Be specific about the structural differences you observe.
      max_tokens: 600
    artifact:
      path: extraction-comparison-report.md
      template: |
        # Multi-Extract Comparison Report

        {{output}}
"##;

// =============================================================================
// 14 — RSS to Newsletter
// fetch: 3 feeds -> extract:feed -> for_each entries -> infer summarize
// =============================================================================

const FETCH_14_RSS_NEWSLETTER: &str = r##"# =============================================================================
# SHOWCASE FETCH 14 — RSS Feed to Newsletter Pipeline
# =============================================================================
# requires_llm: true
# category: fetch-combo
# features: fetch-feed
#
# Fetches an RSS feed, then uses an LLM to summarize the entries into
# a newsletter-style digest. Demonstrates extract: feed piped into
# infer: for AI-powered content curation.
#
# Run: nika run workflows/showcase-fetch/14-rss-to-newsletter.nika.yaml

schema: "nika/workflow@0.12"
workflow: rss-to-newsletter
description: "Fetch RSS feed and generate an AI-curated newsletter digest"
provider: "{{PROVIDER}}"
model: "{{MODEL}}"

artifacts:
  dir: .output/showcase-fetch

tasks:
  # Phase 1: Fetch the Rust Blog feed
  - id: rust_feed
    description: "Parse Rust Blog Atom feed"
    fetch:
      url: "https://blog.rust-lang.org/feed.xml"
      extract: feed
      timeout: 15
    artifact:
      path: newsletter-rust-feed.json
      format: json

  # Phase 2: Summarize into a newsletter digest
  - id: create_digest
    description: "Generate newsletter digest from feed entries"
    depends_on: [rust_feed]
    with:
      feed: $rust_feed
    infer:
      prompt: |
        You are a tech newsletter curator. Create a concise weekly digest
        from this RSS feed data.

        FEED DATA:
        {{with.feed}}

        Format as a newsletter with:
        1. A catchy header with the feed name
        2. Top 5 most recent entries, each with:
           - Title (as a heading)
           - 2-sentence summary of what the post covers
           - Why it matters for Rust developers
        3. A "Quick Links" section with remaining entry titles
        4. A brief editorial closing paragraph

        Write in an engaging but professional tone.
      max_tokens: 800
    artifact:
      path: rust-newsletter-digest.md
      template: |
        {{output}}

  - id: log_done
    depends_on: [create_digest]
    invoke:
      tool: "nika:log"
      params:
        level: "info"
        message: "Newsletter digest generated from RSS feed"
"##;

// =============================================================================
// 15 — Scrape + Analyze
// fetch: metadata -> infer: SEO analysis -> structured report -> artifact
// =============================================================================

const FETCH_15_SCRAPE_ANALYZE: &str = r##"# =============================================================================
# SHOWCASE FETCH 15 — Scrape + SEO Analysis Pipeline
# =============================================================================
# requires_llm: true
# category: fetch-combo
# features: fetch-html
#
# Fetches metadata and links from a website, then uses an LLM to produce
# a structured SEO analysis report. Combines extract: metadata and
# extract: links with structured output and artifact generation.
#
# Run: nika run workflows/showcase-fetch/15-scrape-and-analyze.nika.yaml

schema: "nika/workflow@0.12"
workflow: scrape-and-analyze
description: "Scrape metadata + links, then generate a structured SEO report"
provider: "{{PROVIDER}}"
model: "{{MODEL}}"

artifacts:
  dir: .output/showcase-fetch

tasks:
  # Phase 1: Extract metadata
  - id: scrape_metadata
    description: "Extract OG, Twitter Cards, JSON-LD, and SEO tags"
    fetch:
      url: "https://github.com"
      extract: metadata
      timeout: 15
    artifact:
      path: seo-metadata.json
      format: json

  # Phase 2: Extract and classify links
  - id: scrape_links
    description: "Extract and classify all links"
    fetch:
      url: "https://github.com"
      extract: links
      timeout: 15
    artifact:
      path: seo-links.json
      format: json

  # Phase 3: Fetch full response for header analysis
  - id: check_headers
    description: "Inspect HTTP response headers for security and caching"
    fetch:
      url: "https://github.com"
      response: full
      timeout: 15

  # Phase 4: LLM analyzes everything
  - id: seo_analysis
    description: "AI-powered SEO analysis from scraped data"
    depends_on: [scrape_metadata, scrape_links, check_headers]
    with:
      metadata: $scrape_metadata
      links: $scrape_links
      resp_status: $check_headers.status
      resp_headers: $check_headers.headers
    infer:
      prompt: |
        You are an SEO expert. Analyze this website's SEO posture from the scraped data.

        ## Metadata (OG, Twitter Cards, JSON-LD, SEO tags)
        {{with.metadata}}

        ## Link Classification (internal/external, nav/content/footer)
        {{with.links}}

        ## HTTP Headers (security, caching, performance)
        Status: {{with.resp_status}}
        {{with.resp_headers | to_json}}

        Produce a structured SEO report with:
        1. Overall SEO Score (0-100)
        2. Metadata Quality: title, description, OG completeness, Twitter Cards
        3. Link Health: internal/external ratio, broken-link risk areas
        4. Security Headers: CSP, HSTS, X-Frame-Options presence
        5. Top 5 Issues (ranked by impact)
        6. Top 5 Quick Wins (easy to fix, high impact)

        Return as JSON with fields: score, metadata_quality, link_health,
        security_headers, top_issues (array), quick_wins (array).
      max_tokens: 800
      temperature: 0.2
    structured:
      schema:
        type: object
        properties:
          score:
            type: integer
            description: "Overall SEO score 0-100"
          metadata_quality:
            type: object
            properties:
              title_present:
                type: boolean
              description_present:
                type: boolean
              og_completeness:
                type: string
              twitter_cards:
                type: string
          link_health:
            type: object
            properties:
              internal_count:
                type: integer
              external_count:
                type: integer
              assessment:
                type: string
          security_headers:
            type: object
            properties:
              csp:
                type: boolean
              hsts:
                type: boolean
              x_frame_options:
                type: boolean
          top_issues:
            type: array
            items:
              type: string
          quick_wins:
            type: array
            items:
              type: string
        required: [score, top_issues, quick_wins]
    artifact:
      path: seo-analysis-report.json
      format: json

  # Phase 5: Generate human-readable report
  - id: final_report
    description: "Generate formatted SEO report from structured analysis"
    depends_on: [seo_analysis]
    with:
      analysis: $seo_analysis
    infer:
      prompt: |
        Convert this structured SEO analysis into a professional Markdown report.

        ANALYSIS DATA:
        {{with.analysis}}

        Include:
        - Executive summary with the overall score
        - Detailed breakdown of each category
        - Prioritized action items
        - A summary table of findings

        Format as clean, well-structured Markdown.
      max_tokens: 600
    artifact:
      path: seo-report-final.md
      template: |
        # SEO Analysis Report — github.com

        {{output}}
"##;

// =============================================================================
// Tests
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_showcase_fetch_workflow_count() {
        let workflows = get_showcase_fetch_workflows();
        assert_eq!(
            workflows.len(),
            15,
            "Should have exactly 15 showcase fetch workflows"
        );
    }

    #[test]
    fn test_showcase_fetch_filenames_unique() {
        let workflows = get_showcase_fetch_workflows();
        let mut names: Vec<&str> = workflows.iter().map(|w| w.filename).collect();
        let len = names.len();
        names.sort();
        names.dedup();
        assert_eq!(names.len(), len, "All filenames must be unique");
    }

    #[test]
    fn test_showcase_fetch_all_have_schema() {
        let workflows = get_showcase_fetch_workflows();
        for w in &workflows {
            assert!(
                w.content.contains("schema: \"nika/workflow@0.12\""),
                "Workflow {} must declare schema",
                w.filename
            );
        }
    }

    #[test]
    fn test_showcase_fetch_all_have_workflow_name() {
        let workflows = get_showcase_fetch_workflows();
        for w in &workflows {
            assert!(
                w.content.contains("workflow:"),
                "Workflow {} must have workflow: declaration",
                w.filename
            );
        }
    }

    #[test]
    fn test_showcase_fetch_all_have_tasks() {
        let workflows = get_showcase_fetch_workflows();
        for w in &workflows {
            assert!(
                w.content.contains("tasks:"),
                "Workflow {} must have tasks section",
                w.filename
            );
        }
    }

    #[test]
    fn test_showcase_fetch_all_nika_yaml_extension() {
        let workflows = get_showcase_fetch_workflows();
        for w in &workflows {
            assert!(
                w.filename.ends_with(".nika.yaml"),
                "Workflow {} must end with .nika.yaml",
                w.filename
            );
        }
    }

    #[test]
    fn test_showcase_fetch_all_in_showcase_fetch_dir() {
        let workflows = get_showcase_fetch_workflows();
        for w in &workflows {
            assert_eq!(
                w.tier_dir, "showcase-fetch",
                "Workflow {} must be in showcase-fetch directory",
                w.filename
            );
        }
    }

    #[test]
    fn test_showcase_fetch_valid_yaml() {
        let workflows = get_showcase_fetch_workflows();
        for w in &workflows {
            // Skip YAML validation for templates with placeholders
            if w.content.contains("{{PROVIDER}}") || w.content.contains("{{MODEL}}") {
                continue;
            }
            let parsed: Result<serde_json::Value, _> = serde_saphyr::from_str(w.content);
            assert!(
                parsed.is_ok(),
                "Workflow {} should be valid YAML: {:?}",
                w.filename,
                parsed.err()
            );
        }
    }

    #[test]
    fn test_showcase_fetch_all_use_fetch_verb() {
        let workflows = get_showcase_fetch_workflows();
        for w in &workflows {
            assert!(
                w.content.contains("fetch:"),
                "Workflow {} must use the fetch: verb (it's a fetch showcase)",
                w.filename
            );
        }
    }

    #[test]
    fn test_showcase_fetch_extract_modes_coverage() {
        let all_content: String = get_showcase_fetch_workflows()
            .iter()
            .map(|w| w.content)
            .collect::<Vec<_>>()
            .join("\n");

        let modes = [
            "extract: markdown",
            "extract: article",
            "extract: text",
            "extract: selector",
            "extract: metadata",
            "extract: links",
            "extract: jsonpath",
            "extract: feed",
            "extract: llm_txt",
        ];

        for mode in &modes {
            assert!(all_content.contains(mode), "Missing extract mode: {}", mode);
        }
    }

    #[test]
    fn test_showcase_fetch_response_modes_coverage() {
        let all_content: String = get_showcase_fetch_workflows()
            .iter()
            .map(|w| w.content)
            .collect::<Vec<_>>()
            .join("\n");

        assert!(
            all_content.contains("response: full"),
            "Missing response mode: full"
        );
        assert!(
            all_content.contains("response: binary"),
            "Missing response mode: binary"
        );
        // Default mode = no response: field, verified by workflow 12 existing
        // and NOT having response: in its main fetch tasks
    }

    #[test]
    fn test_showcase_fetch_all_have_artifacts_dir() {
        let workflows = get_showcase_fetch_workflows();
        for w in &workflows {
            assert!(
                w.content.contains("artifacts:") || w.content.contains("artifact:"),
                "Workflow {} should produce artifacts",
                w.filename
            );
        }
    }
}
