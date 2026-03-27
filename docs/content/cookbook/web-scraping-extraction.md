# Web Scraping and Extraction Recipes

Production-ready workflows for web scraping, content extraction, RSS monitoring, and competitive intelligence using the `fetch:` verb and its 9 extract modes.

---

## Fetch Extract Mode Reference

Before diving into recipes, here is a quick reference for all 9 extract modes:

| Mode | Feature Flag | Returns | Use Case |
|------|-------------|---------|----------|
| `markdown` | fetch-markdown | Clean Markdown | LLM-ready content |
| `article` | fetch-article | Main article only | Blog/news extraction |
| `text` | fetch-html | Visible text | Search indexing |
| `selector` | fetch-html | Raw HTML fragments | DOM scraping |
| `metadata` | fetch-html | JSON (OG, Twitter, JSON-LD) | SEO analysis |
| `links` | fetch-html | Classified link list | Site mapping |
| `jsonpath` | (always available) | Queried JSON values | API extraction |
| `feed` | fetch-feed | Structured feed entries | RSS/Atom monitoring |
| `llm_txt` | (always available) | AI content files | LLM discovery |

Response modes: `full` (JSON envelope with status/headers/body), `binary` (CAS hash), or default (raw body text).

---

## Recipe 1: Multi-Source News Aggregator

**Problem:** You need to aggregate news from multiple RSS feeds, extract the content, and produce a daily digest.

**Solution:**

```yaml
schema: "nika/workflow@0.12"
workflow: news-aggregator
description: "Aggregate news from multiple RSS feeds into a daily digest"
provider: anthropic
model: claude-sonnet-4-20250514

inputs:
  max_articles: 10
  focus_topics: "AI, Rust, open source"

artifacts:
  dir: ./output/news-digest

tasks:
  # Parse multiple RSS feeds in parallel
  - id: fetch_feeds
    for_each:
      items:
        - { url: "https://blog.rust-lang.org/feed.xml", name: "Rust Blog" }
        - { url: "https://github.blog/feed/", name: "GitHub Blog" }
        - { url: "https://www.infoq.com/feed/", name: "InfoQ" }
        - { url: "https://feeds.feedburner.com/TheHackersNews", name: "Hacker News Feed" }
      as: feed
      concurrency: 4
    fail_fast: false
    fetch:
      url: "{{with.feed.url}}"
      extract: feed
      timeout: 20
    retry:
      max_attempts: 2
      delay_ms: 2000
      backoff: 2.0

  # Fetch full articles for top stories
  - id: fetch_top_articles
    depends_on: [fetch_feeds]
    for_each:
      items:
        - "https://blog.rust-lang.org/"
        - "https://github.blog/"
      as: article_url
      concurrency: 2
    fetch:
      url: "{{with.article_url}}"
      extract: article
      timeout: 25

  # Synthesize into a digest
  - id: daily_digest
    depends_on: [fetch_feeds, fetch_top_articles]
    with:
      feeds: $fetch_feeds
      articles: $fetch_top_articles
    infer:
      system: "You are a tech news editor producing a concise daily digest."
      prompt: |
        Create a daily news digest from these sources:

        RSS Feeds:
        {{with.feeds | first(4000)}}

        Full Articles:
        {{with.articles | first(3000)}}

        Focus on: {{inputs.focus_topics}}
        Max articles to include: {{inputs.max_articles}}

        Format as a newsletter with:
        - "Top Story" section (1 article, 3-paragraph summary)
        - "Quick Hits" section (5-8 one-line summaries with source)
        - "Worth Reading" section (2-3 articles with 2-sentence descriptions)
        - "Trends to Watch" section (3 bullet points)
      temperature: 0.4
      max_tokens: 3000
    artifact:
      path: daily-digest.md

  # Extract metadata for SEO/sharing
  - id: digest_metadata
    depends_on: [daily_digest]
    with:
      digest: $daily_digest
    infer:
      prompt: |
        Generate newsletter metadata for:
        {{with.digest | first(1000)}}

        Return: title, subtitle, preview_text (90 chars), categories, share_text (for Twitter).
      response_format: json
      temperature: 0.2
      max_tokens: 500
    structured:
      schema:
        type: object
        properties:
          title:
            type: string
          subtitle:
            type: string
          preview_text:
            type: string
          categories:
            type: array
            items:
              type: string
        required: [title, subtitle, preview_text, categories]
    artifact:
      path: digest-metadata.json
      format: json
```

**Explanation:**

The workflow uses three different extract modes:
- `extract: feed` parses RSS/Atom feeds into structured JSON with titles, dates, and links
- `extract: article` uses the Readability algorithm to extract main content
- `fail_fast: false` ensures that if one feed is down, the others still get processed
- `retry:` with exponential backoff handles transient network failures

**Expected Output:** A formatted newsletter digest and JSON metadata for distribution.

---

## Recipe 2: Competitive Analysis Dashboard

**Problem:** You need to monitor competitor websites for changes in their messaging, pricing, and feature positioning.

**Solution:**

```yaml
schema: "nika/workflow@0.12"
workflow: competitive-analysis
description: "Scrape and analyze competitor websites for positioning intelligence"
provider: anthropic
model: claude-sonnet-4-20250514

artifacts:
  dir: ./output/competitive-intel

tasks:
  # Scrape competitor homepages for content and metadata
  - id: scrape_competitors
    for_each:
      items:
        - { name: "Zapier", url: "https://zapier.com" }
        - { name: "n8n", url: "https://n8n.io" }
        - { name: "Make", url: "https://www.make.com/en" }
        - { name: "Pipedream", url: "https://pipedream.com" }
      as: competitor
      concurrency: 4
    fetch:
      url: "{{with.competitor.url}}"
      extract: markdown
      timeout: 25

  # Extract metadata (OG tags, descriptions)
  - id: scrape_metadata
    for_each:
      items:
        - "https://zapier.com"
        - "https://n8n.io"
        - "https://www.make.com/en"
        - "https://pipedream.com"
      as: url
      concurrency: 4
    fetch:
      url: "{{with.url}}"
      extract: metadata
      timeout: 20

  # Extract link structures for site architecture analysis
  - id: scrape_links
    for_each:
      items:
        - "https://zapier.com"
        - "https://n8n.io"
      as: url
      concurrency: 2
    fetch:
      url: "{{with.url}}"
      extract: links
      timeout: 20

  # Check for AI content files
  - id: check_llm_txt
    for_each:
      items:
        - "https://zapier.com"
        - "https://n8n.io"
        - "https://www.make.com/en"
      as: url
      concurrency: 3
    fail_fast: false
    fetch:
      url: "{{with.url}}"
      extract: llm_txt
      timeout: 15

  # Analyze all collected data
  - id: competitive_report
    depends_on: [scrape_competitors, scrape_metadata, scrape_links, check_llm_txt]
    with:
      content: $scrape_competitors
      metadata: $scrape_metadata
      links: $scrape_links
      llm_txt: $check_llm_txt
    infer:
      system: |
        You are a competitive intelligence analyst specializing in developer tools
        and workflow automation platforms.
      prompt: |
        Analyze these competitors:

        Homepage Content:
        {{with.content | first(4000)}}

        Metadata (OG/SEO):
        {{with.metadata | first(2000)}}

        Link Architecture:
        {{with.links | first(1500)}}

        AI Readiness (llm.txt):
        {{with.llm_txt | first(500)}}

        Produce a competitive analysis with:
        1. Positioning Matrix (how each positions themselves)
        2. Feature Comparison Table
        3. Messaging Analysis (key phrases, value props)
        4. SEO Strategy (metadata quality, keyword focus)
        5. AI Readiness (who has llm.txt, who does not)
        6. Content Gap Opportunities
        7. Recommendations for differentiation
      temperature: 0.3
      max_tokens: 4000
    artifact:
      path: competitive-analysis.md

  # Generate structured comparison
  - id: comparison_data
    depends_on: [competitive_report]
    with:
      report: $competitive_report
    infer:
      prompt: |
        Convert this analysis into structured comparison data:
        {{with.report | first(3000)}}
      response_format: json
      temperature: 0.1
      max_tokens: 2000
    structured:
      schema:
        type: object
        properties:
          competitors:
            type: array
            items:
              type: object
              properties:
                name:
                  type: string
                positioning:
                  type: string
                strengths:
                  type: array
                  items:
                    type: string
                weaknesses:
                  type: array
                  items:
                    type: string
              required: [name, positioning]
          opportunities:
            type: array
            items:
              type: string
        required: [competitors, opportunities]
    artifact:
      path: comparison-data.json
      format: json
```

**Explanation:**

This workflow demonstrates four different extract modes working together:
- `extract: markdown` captures the full homepage content for messaging analysis
- `extract: metadata` pulls OG tags, Twitter Cards, and SEO data
- `extract: links` reveals site architecture and navigation patterns
- `extract: llm_txt` checks which competitors have embraced AI content discovery

The `fail_fast: false` on `check_llm_txt` is critical since most sites will not have an llm.txt file, and a 404 should not stop the workflow.

**Expected Output:** A comprehensive competitive analysis report plus structured JSON comparison data.

---

## Recipe 3: Price Monitoring Pipeline

**Problem:** You need to track product prices across multiple e-commerce sites and alert when prices change.

**Solution:**

```yaml
schema: "nika/workflow@0.12"
workflow: price-monitor
description: "Monitor product prices across websites using JSONPath and selectors"
provider: anthropic
model: claude-sonnet-4-20250514

artifacts:
  dir: ./output/price-monitoring

tasks:
  # Fetch price data from JSON APIs
  - id: api_prices
    for_each:
      items:
        - url: "https://jsonplaceholder.typicode.com/posts/1"
          name: "Product API 1"
          jsonpath: "$.title"
        - url: "https://jsonplaceholder.typicode.com/posts/2"
          name: "Product API 2"
          jsonpath: "$.title"
        - url: "https://jsonplaceholder.typicode.com/posts/3"
          name: "Product API 3"
          jsonpath: "$.title"
      as: source
      concurrency: 3
    fetch:
      url: "{{with.source.url}}"
      extract: jsonpath
      selector: "{{with.source.jsonpath}}"
      timeout: 15
    retry:
      max_attempts: 3
      delay_ms: 2000
      backoff: 2.0

  # Fetch prices from HTML pages using CSS selectors
  - id: html_prices
    for_each:
      items:
        - url: "https://httpbin.org/html"
          name: "HTML Store"
          selector: "p"
      as: store
      concurrency: 3
    fail_fast: false
    fetch:
      url: "{{with.store.url}}"
      extract: text
      selector: "{{with.store.selector}}"
      timeout: 20

  # Compare with historical data and detect changes
  - id: price_analysis
    depends_on: [api_prices, html_prices]
    with:
      api_data: $api_prices
      html_data: $html_prices
    infer:
      prompt: |
        Analyze price data from multiple sources:

        API Sources:
        {{with.api_data}}

        HTML Sources:
        {{with.html_data}}

        Generate a price monitoring report with:
        1. Current prices per product per source
        2. Source reliability assessment
        3. Data freshness timestamps
        4. Anomaly detection (any unusual values)
        5. Recommended alert thresholds
      response_format: json
      temperature: 0.1
      max_tokens: 1500
    structured:
      schema:
        type: object
        properties:
          products:
            type: array
            items:
              type: object
              properties:
                name:
                  type: string
                sources:
                  type: array
                  items:
                    type: object
                    properties:
                      source:
                        type: string
                      value:
                        type: string
                    required: [source, value]
              required: [name, sources]
          anomalies:
            type: array
            items:
              type: string
        required: [products]
    artifact:
      path: price-report.json
      format: json

  # Generate alert summary
  - id: alert_summary
    depends_on: [price_analysis]
    with:
      analysis: $price_analysis
    exec:
      command: |
        echo "=== PRICE MONITORING ALERT ==="
        echo "Report: {{with.analysis}}"
        echo "Timestamp: $(date -u +%Y-%m-%dT%H:%M:%SZ)"
        echo "=== END ALERT ==="
      shell: true
    artifact:
      path: alert.log
      mode: append
```

**Explanation:**

Two extraction strategies are combined:
- `extract: jsonpath` with `selector:` for JSON APIs where the price lives at a known path (e.g., `$.data.price`)
- `extract: text` with `selector:` for HTML pages where prices are in specific DOM elements

The `retry:` block with exponential backoff handles rate limiting from e-commerce APIs. The `artifact: mode: append` on the alert summary allows multiple runs to build up a history log.

**Expected Output:** A structured JSON price report and an appending alert log.

---

## Recipe 4: RSS-to-Newsletter Pipeline

**Problem:** You need to automatically parse RSS feeds, extract the full articles, and compile them into a formatted newsletter.

**Solution:**

```yaml
schema: "nika/workflow@0.12"
workflow: rss-to-newsletter
description: "Parse RSS feeds, extract articles, generate a newsletter"
provider: anthropic
model: claude-sonnet-4-20250514

inputs:
  newsletter_name: "The Dev Digest"
  max_items: 8

artifacts:
  dir: ./output/newsletter

tasks:
  # Step 1: Parse the RSS feed
  - id: parse_feed
    fetch:
      url: "https://blog.rust-lang.org/feed.xml"
      extract: feed
      timeout: 20
    artifact:
      path: raw-feed.json
      format: json

  # Step 2: Fetch full article content for each entry
  - id: fetch_articles
    depends_on: [parse_feed]
    for_each:
      items:
        - "https://blog.rust-lang.org/"
        - "https://blog.rust-lang.org/inside-rust/"
      as: article_url
      concurrency: 2
    fail_fast: false
    fetch:
      url: "{{with.article_url}}"
      extract: article
      timeout: 25

  # Step 3: Get metadata for each article
  - id: article_metadata
    depends_on: [parse_feed]
    for_each:
      items:
        - "https://blog.rust-lang.org/"
        - "https://blog.rust-lang.org/inside-rust/"
      as: url
      concurrency: 2
    fetch:
      url: "{{with.url}}"
      extract: metadata
      timeout: 20

  # Step 4: Compile the newsletter
  - id: compile_newsletter
    depends_on: [parse_feed, fetch_articles, article_metadata]
    with:
      feed: $parse_feed
      articles: $fetch_articles
      metadata: $article_metadata
    infer:
      system: |
        You are the editor of "{{inputs.newsletter_name}}", a weekly tech newsletter.
        Your style is concise, informative, and slightly opinionated.
      prompt: |
        Compile a newsletter issue from these sources:

        Feed entries:
        {{with.feed | first(3000)}}

        Article content:
        {{with.articles | first(4000)}}

        Article metadata:
        {{with.metadata | first(1000)}}

        Newsletter format:
        # {{inputs.newsletter_name}} - Issue #XX

        ## This Week's Highlights
        (2-3 featured articles with 3-sentence summaries)

        ## Quick Reads
        (4-5 articles with 1-sentence descriptions and links)

        ## Editor's Pick
        (1 article with deeper commentary, 150 words)

        ## Community Spotlight
        (Notable community contributions or discussions)

        Maximum items: {{inputs.max_items}}
      temperature: 0.5
      max_tokens: 4000
    artifact:
      path: newsletter.md

  # Step 5: Generate plain-text version for email
  - id: plaintext_version
    depends_on: [compile_newsletter]
    with:
      newsletter: $compile_newsletter
    infer:
      prompt: |
        Convert this Markdown newsletter to plain text email format:
        {{with.newsletter}}

        Rules:
        - No Markdown syntax
        - Use ALL CAPS for headings
        - Use dashes for bullet points
        - Wrap lines at 72 characters
        - Include "Unsubscribe: [link]" at the bottom
      temperature: 0.2
      max_tokens: 4000
    artifact:
      path: newsletter-plaintext.txt
```

**Explanation:**

This pipeline chains three extract modes in sequence:
1. `extract: feed` parses the RSS/Atom feed into structured JSON with entry titles, dates, and links
2. `extract: article` fetches full article content using the Readability algorithm
3. `extract: metadata` pulls Open Graph and SEO metadata for rich formatting

The `fail_fast: false` on article fetches ensures that if one URL returns an error, the others still get processed.

**Expected Output:** Both a Markdown newsletter and a plain-text email version.

---

## Recipe 5: Website Health Monitor

**Problem:** You need to regularly check multiple website endpoints for uptime, response times, security headers, and content integrity.

**Solution:**

```yaml
schema: "nika/workflow@0.12"
workflow: website-health-monitor
description: "Check website health: uptime, headers, content, and security"
provider: anthropic
model: claude-sonnet-4-20250514

artifacts:
  dir: ./output/health-checks

tasks:
  # Check uptime and response details with full envelope
  - id: uptime_checks
    for_each:
      items:
        - { url: "https://httpbin.org/get", name: "API Health" }
        - { url: "https://httpbin.org/status/200", name: "Status Check" }
        - { url: "https://httpbin.org/headers", name: "Header Check" }
        - { url: "https://httpbin.org/delay/1", name: "Latency Check" }
        - { url: "https://httpbin.org/redirect/1", name: "Redirect Check" }
      as: endpoint
      concurrency: 5
    fail_fast: false
    fetch:
      url: "{{with.endpoint.url}}"
      response: full
      timeout: 15
    retry:
      max_attempts: 2
      delay_ms: 1000

  # Check security-relevant metadata
  - id: security_scan
    for_each:
      items:
        - "https://httpbin.org"
        - "https://github.com"
      as: url
      concurrency: 2
    fetch:
      url: "{{with.url}}"
      extract: metadata
      timeout: 20

  # Check SEO health via link structure
  - id: link_audit
    fetch:
      url: "https://httpbin.org"
      extract: links
      timeout: 20

  # Analyze all health data
  - id: health_report
    depends_on: [uptime_checks, security_scan, link_audit]
    with:
      uptime: $uptime_checks
      security: $security_scan
      links: $link_audit
    infer:
      system: "You are a site reliability engineer performing a health audit."
      prompt: |
        Generate a website health report from:

        Uptime Checks (full HTTP responses):
        {{with.uptime | first(3000)}}

        Security Metadata:
        {{with.security | first(1500)}}

        Link Structure:
        {{with.links | first(1000)}}

        Report sections:
        1. Uptime Summary (status codes, response times)
        2. Security Headers Assessment (CSP, HSTS, X-Frame-Options)
        3. Redirect Chain Analysis
        4. Broken Link Detection
        5. Overall Health Score (1-100)
        6. Critical Actions Required
      temperature: 0.2
      max_tokens: 2500
    structured:
      schema:
        type: object
        properties:
          health_score:
            type: integer
          endpoints_checked:
            type: integer
          issues:
            type: array
            items:
              type: object
              properties:
                severity:
                  type: string
                  enum: ["critical", "warning", "info"]
                description:
                  type: string
              required: [severity, description]
          uptime_percentage:
            type: number
        required: [health_score, endpoints_checked, issues]
    artifact:
      path: health-report.json
      format: json

  # Append to history log
  - id: log_check
    depends_on: [health_report]
    with:
      report: $health_report
    exec:
      command: |
        echo "[$(date -u +%Y-%m-%dT%H:%M:%SZ)] Health check complete: {{with.report}}" | head -c 200
      shell: true
    artifact:
      path: health-history.log
      mode: append
```

**Explanation:**

The `response: full` mode is the key feature here. It returns the complete HTTP response as a JSON envelope containing `status`, `headers`, `body`, and `final_url`. This lets the LLM analyze response codes, security headers, and redirect chains. The `mode: append` on the history log creates a running record across multiple runs.

**Expected Output:** A structured JSON health report with scores and a running history log.

---

## Recipe 6: Content Archival Pipeline

**Problem:** You need to archive web content in multiple formats for long-term storage and analysis.

**Solution:**

```yaml
schema: "nika/workflow@0.12"
workflow: content-archiver
description: "Archive web content in markdown, metadata, and binary formats"

artifacts:
  dir: ./output/archive

tasks:
  # Archive content in multiple formats simultaneously
  - id: archive_markdown
    fetch:
      url: "https://blog.rust-lang.org/"
      extract: markdown
      timeout: 25
    artifact:
      path: archive/content.md

  - id: archive_article
    fetch:
      url: "https://blog.rust-lang.org/"
      extract: article
      timeout: 25
    artifact:
      path: archive/article-only.md

  - id: archive_metadata
    fetch:
      url: "https://blog.rust-lang.org/"
      extract: metadata
      timeout: 20
    artifact:
      path: archive/metadata.json
      format: json

  - id: archive_links
    fetch:
      url: "https://blog.rust-lang.org/"
      extract: links
      timeout: 20
    artifact:
      path: archive/links.json
      format: json

  - id: archive_feed
    fetch:
      url: "https://blog.rust-lang.org/feed.xml"
      extract: feed
      timeout: 20
    artifact:
      path: archive/feed.json
      format: json

  - id: archive_screenshot
    fetch:
      url: "https://httpbin.org/image/png"
      response: binary
      timeout: 20
    artifact:
      path: archive/screenshot.png
      format: binary

  # Generate archive manifest
  - id: manifest
    depends_on: [archive_markdown, archive_article, archive_metadata, archive_links, archive_feed, archive_screenshot]
    exec:
      command: |
        echo '{"archived_at": "'$(date -u +%Y-%m-%dT%H:%M:%SZ)'", "formats": ["markdown", "article", "metadata", "links", "feed", "binary"], "source": "https://blog.rust-lang.org/"}'
      shell: true
    artifact:
      path: archive/manifest.json
      format: json
```

**Explanation:**

This workflow runs six parallel fetch tasks, each using a different extract/response mode to capture the same source in different formats. Since none of the tasks have `depends_on:` relationships with each other, Nika's DAG executor runs them all concurrently. The manifest task waits for all six to complete before writing the archive record.

**Expected Output:** A complete archive directory with content in six formats plus a manifest.

---

## Key Patterns for Web Scraping

### Extract Modes at a Glance

```yaml
# Clean Markdown (best for LLM input)
fetch:
  url: "https://example.com"
  extract: markdown

# Article content only (strips nav, ads, sidebars)
fetch:
  url: "https://example.com/post"
  extract: article

# Visible text from specific elements
fetch:
  url: "https://example.com"
  extract: text
  selector: "article p"

# Raw HTML of matching elements
fetch:
  url: "https://example.com"
  extract: selector
  selector: ".product-card"

# Structured metadata (OG, Twitter Cards, JSON-LD)
fetch:
  url: "https://example.com"
  extract: metadata

# Classified link list
fetch:
  url: "https://example.com"
  extract: links

# JSONPath query on JSON APIs
fetch:
  url: "https://api.example.com/data"
  extract: jsonpath
  selector: "$.results[*].name"

# RSS/Atom feed parsing
fetch:
  url: "https://example.com/feed.xml"
  extract: feed

# AI content discovery
fetch:
  url: "https://example.com"
  extract: llm_txt
```

### Resilience Patterns

```yaml
# Retry with exponential backoff
retry:
  max_attempts: 3
  delay_ms: 1000
  backoff: 2.0

# Timeout guard
fetch:
  url: "https://slow-api.example.com"
  timeout: 30

# Continue on failure in batch operations
for_each: [url1, url2, url3]
fail_fast: false
```

### Response Modes

```yaml
# Default: raw body text
fetch:
  url: "https://api.example.com/data"

# Full envelope: { status, headers, body, final_url }
fetch:
  url: "https://api.example.com/data"
  response: full

# Binary: store in CAS, return hash
fetch:
  url: "https://example.com/image.png"
  response: binary
```
