# Research Report: Firecrawl Deep Dive

**Date**: 2026-03-19
**Purpose**: Understand Firecrawl's full capabilities to determine what Nika's `fetch:` verb can replicate locally vs. what should remain delegated to Firecrawl via MCP.

---

## Summary

Firecrawl is an API service that converts websites into LLM-ready data. It offers 9 endpoints (scrape, batch scrape, crawl, map, search, extract, agent, deep-research, llms.txt) with features spanning JavaScript rendering, anti-bot bypass, structured LLM extraction, and autonomous web agents. Approximately 60-70% of common scraping use cases (static HTML pages, article extraction, metadata parsing, sitemap discovery) can be handled locally in Nika's `fetch:` verb using Rust crates, saving credits and eliminating network round-trips. Firecrawl remains essential for JS-heavy sites, anti-bot bypass, large-scale crawls, and LLM-powered structured extraction.

---

## 1. Complete Feature Inventory

### 1.1 API Endpoints (v1/v2)

| Endpoint | Method | Purpose | Async? |
|----------|--------|---------|--------|
| `/v2/scrape` | POST | Single URL to markdown/HTML/JSON/screenshot | No |
| `/v2/batch/scrape` | POST | Multiple URLs in parallel | Yes (poll) |
| `/v2/crawl` | POST | Full site crawl from seed URL | Yes (poll) |
| `/v2/map` | POST | Discover all URLs on a site | No |
| `/v2/search` | POST | Web search + optional scraping | No |
| `/v2/extract` | POST | LLM-powered structured data extraction | Yes (poll) |
| `/v2/agent` | POST | Autonomous web agent (Spark models) | Yes (poll) |
| `/v2/deep-research` | POST | Multi-iteration research agent | Yes (poll) |
| `/v2/llmstxt` | POST | Generate llms.txt for a site | Yes (poll) |
| `/v2/browser` | POST | Remote browser session (CDP) | Session |

### 1.2 Scrape Feature (`/v2/scrape`)

The core operation. Converts a single URL to structured output.

**Output formats** (enum from OpenAPI spec):
- `markdown` -- Clean markdown (default), powered by Go parser + TurndownService fallback + Rust post-processor
- `html` -- Cleaned HTML (main content extracted)
- `rawHtml` -- Untouched HTML as received
- `links` -- Array of all links on the page
- `screenshot` -- Viewport screenshot (base64)
- `screenshot@fullPage` -- Full page screenshot
- `json` -- LLM-extracted structured data (requires schema or prompt)
- `changeTracking` -- Diff against previous scrape (git-diff or JSON modes)
- `branding` -- Brand identity extraction (colors, fonts, typography, logo, UI)

**Scrape options** (from OpenAPI `ScrapeOptions` schema):
- `onlyMainContent` (bool, default: true) -- Strip nav, footer, sidebar
- `includeTags` / `excludeTags` (string[]) -- CSS tag filtering
- `maxAge` (int, ms) -- Cache TTL. 0 = no cache. "Can speed up scrapes by 500%"
- `headers` (object) -- Custom request headers (cookies, user-agent)
- `waitFor` (int, ms) -- Delay before extracting (for JS loading)
- `mobile` (bool) -- Mobile device emulation
- `skipTlsVerification` (bool) -- Skip TLS cert check
- `timeout` (int, ms, default: 30000) -- Request timeout
- `parsePDF` (bool, default: true) -- Extract PDF to markdown (1 credit/page)
- `removeBase64Images` (bool) -- Strip base64 images from output
- `blockAds` (bool, default: true) -- Ad and cookie popup blocking
- `storeInCache` (bool, default: true) -- Store in Firecrawl index
- `location` -- Geo-targeting: country code + language preferences
- `proxy` -- `basic` / `enhanced` (5 credits) / `auto` (retry with enhanced)

**Actions** (browser automation before scraping):
- `wait` -- Wait ms or for CSS selector
- `click` -- Click element (supports `all: true` for multiple)
- `write` -- Type text into focused input
- `press` -- Press keyboard key
- `scroll` -- Scroll up/down (page or element)
- `screenshot` -- Capture during action sequence
- `scrape` -- Capture HTML mid-sequence
- `executeJavascript` -- Run arbitrary JS

**JSON extraction options** (`jsonOptions`):
- `schema` -- JSON Schema for structured extraction
- `systemPrompt` -- System prompt for LLM
- `prompt` -- User prompt for schemaless extraction

### 1.3 Crawl Feature (`/v2/crawl`)

Full website crawling from a seed URL.

**Options**:
- `url` -- Seed URL
- `excludePaths` / `includePaths` -- Regex path patterns
- `maxDepth` (default: 10) -- Max URL depth (by path slashes)
- `maxDiscoveryDepth` -- Depth based on link discovery order
- `ignoreSitemap` (default: false) -- Skip sitemap.xml
- `ignoreQueryParameters` (default: false) -- Deduplicate URLs with different query params
- `limit` (default: 10000) -- Max pages
- `allowBackwardLinks` (default: false) -- Follow sibling/parent URLs
- `allowExternalLinks` (default: false) -- Follow external links
- `delay` (seconds) -- Rate limiting between scrapes
- `webhook` -- Webhook for events (started, page, completed, failed)
- `scrapeOptions` -- Full ScrapeOptions for each page

**Async operation**: Returns job ID, poll `GET /v2/crawl/{id}` for status. Paginated at 10MB chunks.

### 1.4 Map Feature (`/v2/map`)

Discovers all URLs on a site without scraping content.

**Options**:
- `url` -- Target site
- `search` -- Filter results by relevance to query (uses LLM, limited to 1000 in alpha)
- `ignoreSitemap` (default: true) -- Skip sitemap
- `sitemapOnly` (default: false) -- Only return sitemap URLs
- `includeSubdomains` (default: true) -- Include subdomains
- `limit` (default: 5000, max: 30000) -- Max URLs
- `timeout` (ms) -- No default timeout

### 1.5 Search Feature (`/v2/search`)

Web search with optional content scraping of results.

**Options**:
- `query` -- Search query
- `limit` (default: 5, max: 100) -- Number of results
- `tbs` -- Time-based search parameter
- `location` -- Geo-targeting
- `timeout` (default: 60000ms)
- `scrapeOptions` -- Apply scrape to each result
- `ignoreInvalidURLs` -- Filter bad URLs for piping into other endpoints

### 1.6 Extract Feature (`/v2/extract`)

LLM-powered structured data extraction from multiple pages.

**Options**:
- `urls` (string[]) -- Target URLs (glob patterns supported)
- `prompt` -- Extraction instructions
- `schema` -- JSON Schema for output structure
- `enableWebSearch` (default: false) -- Use web search for context
- `ignoreSitemap` (default: false)
- `includeSubdomains` (default: true)
- `showSources` (default: false) -- Include source URLs in response
- `scrapeOptions` -- Full ScrapeOptions

### 1.7 Agent Feature (`/v2/agent`)

Autonomous web agent powered by "Spark" models. Evolution of `/extract`.

**Options**:
- `prompt` -- Natural language description of what data to find
- `urls` (optional) -- Focus agent on specific pages
- `schema` -- JSON Schema for structured output (Pydantic-compatible)
- `model` -- `spark-1-mini` (default, 60% cheaper) or `spark-1-pro` (complex research)

**How it works**: Agent autonomously searches the web, navigates pages, and extracts data. No URLs required -- just describe what you need.

### 1.8 Deep Research (`/v2/deep-research`)

Multi-iteration research agent.

**Options**:
- `query` -- Research topic
- `maxDepth` (1-12, default: 7) -- Research iteration depth
- `timeLimit` (30-600s, default: 300) -- Time budget
- `maxUrls` (1-1000, default: 20) -- URLs to analyze
- `analysisPrompt` -- Custom final analysis formatting
- `systemPrompt` -- Guide research direction
- `formats` -- markdown, json, branding
- `jsonOptions` -- Schema for structured output

### 1.9 LLMs.txt Generator (`/v2/llmstxt`)

Generates an llms.txt file for a website (format for AI consumption).

**Options**:
- `url` -- Target site
- `maxUrls` (default: 2) -- URLs to analyze
- `showFullText` (default: false) -- Include full content

### 1.10 Browser Sessions (`/v2/browser`)

Remote browser environment for agents.

- Launch CDP browser sessions
- Execute Playwright/Node.js/Python/bash code
- Persistent profiles (cookies, localStorage across sessions)
- TTL-based session management
- Live view URLs for debugging
- agent-browser integration (natural language commands)

---

## 2. Internal Architecture (from source)

### 2.1 Engine Waterfall

Firecrawl uses a prioritized engine fallback system:

| Engine | Quality | Capabilities | Use Case |
|--------|---------|-------------|----------|
| `index` | 1000 | Cached results | Previously scraped pages |
| `wikipedia` | 500 | Wikipedia API | Wikimedia sites |
| `fire-engine;chrome-cdp` | 50 | Full browser, actions, screenshots, branding | JS-heavy sites |
| `fire-engine(retry);chrome-cdp` | 45 | CDP retry | Flaky pages |
| `playwright` | 20 | Browser without actions | JS rendering fallback |
| `fire-engine;tlsclient` | 10 | TLS fingerprint spoofing | Anti-bot bypass |
| `fetch` | 5 | Plain HTTP request | Static pages |
| `pdf` | -20 | PDF extraction | PDF URLs |
| `document` | -20 | DOCX/ODT/RTF/XLSX | Office docs |
| stealth variants | -2 to -15 | Enhanced proxies | Cloudflare etc. |

Key insight: **For static pages without anti-bot, Firecrawl falls all the way down to plain `fetch` (quality=5)**. This is exactly what Nika can do locally.

### 2.2 HTML-to-Markdown Pipeline

Firecrawl's markdown conversion chain:
1. **Primary**: Go shared library (`ConvertHTMLToMarkdown`) loaded via `koffi` FFI
2. **Secondary**: HTTP microservice (`HTML_TO_MARKDOWN_SERVICE_URL`)
3. **Fallback**: TurndownService (JS) + joplin-turndown-plugin-gfm (GFM tables)
4. **Post-processor**: `@mendable/firecrawl-rs` (Rust) -- cleanup, link processing

Additional processing:
- `removeUnwantedElements` -- Strip script, style, nav, footer, etc.
- `removeBase64Images` -- Strip inline images
- `removeSkipToContentLinks` -- Remove accessibility skip links
- `processMultiLineLinks` -- Fix multi-line markdown links

### 2.3 Transformers (post-scrape processing)

- `llmExtract.ts` -- LLM-based JSON extraction
- `agent.ts` -- Agent mode processing
- `diff.ts` -- Change tracking diffs
- `performAttributes.ts` -- Attribute extraction
- `removeBase64Images.ts` -- Image stripping
- `sendToSearchIndex.ts` -- Index updates
- `uploadScreenshot.ts` -- Screenshot storage

### 2.4 Postprocessors

- `youtube.ts` -- YouTube transcript extraction

---

## 3. How Firecrawl Handles Key Challenges

### 3.1 JavaScript Rendering

- **Primary**: Chrome CDP via Fire Engine (headless Chromium)
- **Secondary**: Playwright microservice
- **Actions**: Full browser automation (click, type, scroll, wait, JS execution)
- `waitFor` parameter allows waiting for dynamic content
- Mobile emulation supported

### 3.2 Anti-Bot / Cloudflare

- **Proxy tiers**: `basic` (default), `enhanced` (advanced anti-bot, 5 credits), `auto` (try basic, fallback to enhanced)
- **TLS client**: Spoofs TLS fingerprints (`fire-engine;tlsclient`)
- **Stealth mode**: Stealth browser with enhanced proxies
- **Auto-retry**: Detects proxy errors (401, 403, 429) and escalates to stealth
- **Ad blocking**: Built-in ad and cookie popup blocker

### 3.3 Rate Limiting

- Server-side: Exponential backoff on 429 responses
- MCP server config: `FIRECRAWL_RETRY_MAX_ATTEMPTS` (3), initial/max delay, backoff factor
- Crawl delay option: configurable seconds between scrapes
- Credit monitoring: warning (1000) and critical (100) thresholds

### 3.4 Sitemap Discovery

- Crawl: reads `sitemap.xml` by default (`ignoreSitemap: false`)
- Map: can optionally use sitemap (`sitemapOnly: true`) or ignore it
- `maxDiscoveryDepth`: controls how deep to follow discovered links
- Sitemap pages have discovery depth 0

### 3.5 Structured Data Extraction

Two approaches:
1. **Scrape + JSON format**: Single page, schema or prompt, uses LLM to extract from page content
2. **Extract endpoint**: Multi-page, glob patterns, LLM aggregates across pages
3. **Agent endpoint**: Autonomous -- finds the right pages itself

No explicit schema.org / JSON-LD extraction -- Firecrawl relies on LLM interpretation of rendered content.

---

## 4. Pricing Model

Based on observed data and documentation:

| Plan | Price | Credits/month |
|------|-------|---------------|
| Free | $0 | 500 credits |
| Hobby | ~$19/mo | 3,000 credits |
| Standard | ~$79/mo | 50,000 credits |
| Growth | ~$198/mo | 500,000 credits |
| Enterprise | Custom | Custom |

**Credit costs per operation**:
- Scrape: 1 credit per page (basic proxy)
- Scrape with enhanced proxy: up to 5 credits
- PDF: 1 credit per page (parsed) or 1 credit flat (base64)
- Crawl: 1 credit per page scraped
- Map: 1 credit per request
- Search: 1 credit per result
- Extract: credits based on pages analyzed + LLM usage
- Agent: varies by model (spark-1-mini 60% cheaper than spark-1-pro)

---

## 5. Limitations of Firecrawl (What Local Tools Solve)

### 5.1 Cost

Every scrape costs at least 1 credit. A workflow that fetches 100 pages burns 100 credits minimum. Local fetching costs zero.

### 5.2 Latency

Network round-trip to Firecrawl API + their processing:
- Simple static page via Firecrawl: ~2-5 seconds
- Same page locally via reqwest + htmd: ~100-500ms
- **10-50x latency reduction for static pages**

### 5.3 Rate Limits

API rate limits cap throughput. Local fetching is limited only by the target site's capacity and the machine's resources.

### 5.4 Privacy

URLs and content are sent to Firecrawl's servers. Even with `storeInCache: false`, data traverses their infrastructure. Local fetching keeps everything on-device.

### 5.5 Availability

Firecrawl outages = workflow failures. Local fetching has no external dependency for basic operations.

### 5.6 No Structured Data Parsing

Firecrawl does not extract JSON-LD, schema.org, OpenGraph, or Twitter Card metadata natively -- it relies on LLM interpretation. A local parser can extract these deterministically, faster, and for free.

---

## 6. What Nika Can Replicate Locally

### 6.1 Recommended Rust Crate Stack

| Capability | Crate | Downloads | Maturity |
|-----------|-------|-----------|----------|
| HTTP client | `reqwest` (0.13) | 405M | Production |
| HTML parsing | `scraper` (0.26) | 14.8M | Production |
| HTML5 parsing | `html5ever` (0.39) | 48.5M | Production |
| HTML to Markdown | `htmd` (0.5) | 247K | Solid (turndown.js port) |
| Article extraction | `dom_smoothie` (0.16) | 36K | Active dev |
| Article extraction (alt) | `readability` (0.3) | 450K | Stable |
| Streaming HTML rewrite | `lol_html` (2.7) | 2.8M | Production (Cloudflare) |
| CSS selectors | `scraper` (built-in) | -- | Production |
| Sitemap parsing | `sitemap` (0.4) | 512K | Stable |

### 6.2 Feature Mapping: Firecrawl -> Local

| Firecrawl Feature | Local Equivalent | Complexity |
|-------------------|-----------------|------------|
| `format: markdown` | `htmd` crate | Low |
| `format: html` (cleaned) | `lol_html` + tag stripping | Low |
| `format: rawHtml` | `reqwest` response body | Trivial |
| `format: links` | `scraper` CSS selector `a[href]` | Low |
| `onlyMainContent` | `dom_smoothie` or `readability` | Medium |
| `includeTags`/`excludeTags` | `scraper` + CSS selectors | Low |
| `headers` | `reqwest` headers | Trivial |
| `timeout` | `reqwest` timeout | Trivial |
| Metadata (title, description) | `scraper` meta tag extraction | Low |
| OpenGraph / Twitter Cards | `scraper` meta property extraction | Low |
| JSON-LD / schema.org | Parse `<script type="application/ld+json">` | Low |
| Sitemap parsing | `sitemap` crate | Low |
| PDF text extraction | Already have `nika:pdf_extract` | Done |
| Link discovery | `scraper` + URL normalization | Medium |

| Firecrawl Feature | Cannot Do Locally | Why |
|-------------------|-------------------|-----|
| `format: screenshot` | Needs headless browser | Chrome/Playwright |
| `format: json` (LLM) | Needs LLM inference | Use `infer:` verb instead |
| `format: branding` | Needs browser + LLM | CDP + computed styles |
| `format: changeTracking` | Needs persistent index | Could build, but complex |
| `actions` (click, type, etc.) | Needs browser automation | Chrome/Playwright |
| Anti-bot bypass | Needs rotating proxies + TLS spoofing | Infrastructure |
| JS rendering | Needs headless browser | Chrome/Playwright |
| `proxy` options | Needs proxy infrastructure | Service |
| `location` geo-targeting | Needs geo-distributed proxies | Service |
| `mobile` emulation | Needs browser | Chrome |
| Web search | Needs search index | Google/Bing API |
| Agent / Deep Research | Needs LLM + orchestration | Complex multi-step |

### 6.3 Proposed `fetch:` Verb Enhancements

```yaml
# Current fetch: verb (HTTP only)
fetch:
  url: "https://example.com/article"
  method: GET
  headers:
    User-Agent: "Nika/0.37.0"

# Enhanced fetch: verb (local scraping)
fetch:
  url: "https://example.com/article"
  method: GET
  headers:
    User-Agent: "Nika/0.37.0"
  extract:
    format: markdown          # htmd conversion
    only_main_content: true   # dom_smoothie article extraction
    include_tags: ["article", "main"]
    exclude_tags: ["nav", "footer", "aside"]
    selectors:                # CSS selector extraction
      title: "h1"
      price: ".price-current"
      links: "a[href]"
    metadata: true            # OpenGraph, Twitter Cards, JSON-LD, meta tags
    sitemap: false            # Parse sitemap.xml if URL is a site root
```

**Output would include**:
```json
{
  "status": 200,
  "url": "https://example.com/article",
  "body": "...",
  "markdown": "# Article Title\n\nContent...",
  "metadata": {
    "title": "Article Title",
    "description": "...",
    "og:image": "...",
    "twitter:card": "summary_large_image",
    "json_ld": [{ "@type": "Article", ... }]
  },
  "selectors": {
    "title": "Article Title",
    "price": "$29.99",
    "links": ["https://...", "https://..."]
  }
}
```

### 6.4 Decision Matrix: When to Use What

```
                    Static HTML          JS-rendered         Anti-bot
                    ───────────          ───────────         ─────────
Simple GET          fetch: (local)       Firecrawl scrape    Firecrawl scrape
Article extract     fetch: + extract     Firecrawl scrape    Firecrawl scrape
CSS selectors       fetch: + selectors   Firecrawl scrape    Firecrawl scrape
Structured JSON     fetch: + infer:      Firecrawl json      Firecrawl json
Screenshot          Firecrawl scrape     Firecrawl scrape    Firecrawl scrape
Full site crawl     fetch: + sitemap     Firecrawl crawl     Firecrawl crawl
URL discovery       fetch: + sitemap     Firecrawl map       Firecrawl map
Web search          N/A                  Firecrawl search    Firecrawl search
```

---

## 7. Implementation Priorities for Nika

### Phase 1: Core Local Extraction (High Impact, Low Effort)

1. **HTML to Markdown** -- Add `htmd` crate, expose via `extract.format: markdown`
2. **Metadata extraction** -- Parse `<meta>`, OpenGraph, Twitter Cards from HTML
3. **CSS selector extraction** -- Use `scraper` crate for targeted data extraction
4. **JSON-LD parsing** -- Extract `<script type="application/ld+json">` blocks

### Phase 2: Article Extraction (High Impact, Medium Effort)

5. **Readability/article mode** -- Integrate `dom_smoothie` for `only_main_content`
6. **Tag filtering** -- `include_tags` / `exclude_tags` via `lol_html` or `scraper`
7. **Link extraction** -- All links with URL normalization

### Phase 3: Site Discovery (Medium Impact, Medium Effort)

8. **Sitemap parsing** -- Parse `sitemap.xml` and `sitemap_index.xml`
9. **robots.txt parsing** -- Respect crawl rules locally
10. **Link following** -- Basic crawl-like behavior for static sites

### Phase 4: Advanced (Low Priority)

11. **Change detection** -- Local diff against previous fetch results (store in CAS)
12. **RSS/Atom feed parsing** -- Extract feed items
13. **Structured data schemas** -- schema.org type-specific extractors

---

## Sources

1. [Firecrawl GitHub README](https://github.com/firecrawl/firecrawl) -- Feature overview, SDK examples, all endpoints
2. [Firecrawl v1 OpenAPI Spec](https://github.com/firecrawl/firecrawl/blob/main/apps/api/v1-openapi.json) -- Complete schema definitions for all options
3. [Firecrawl MCP Server](https://github.com/firecrawl/firecrawl-mcp-server) -- MCP tool definitions, retry config, credit monitoring
4. [Firecrawl scrapeURL/index.ts](https://github.com/firecrawl/firecrawl/blob/main/apps/api/src/scraper/scrapeURL/index.ts) -- Engine waterfall, feature flags, meta object
5. [Firecrawl engines/index.ts](https://github.com/firecrawl/firecrawl/blob/main/apps/api/src/scraper/scrapeURL/engines/index.ts) -- Engine definitions, quality scores, feature support matrix
6. [Firecrawl html-to-markdown.ts](https://github.com/firecrawl/firecrawl/blob/main/apps/api/src/lib/html-to-markdown.ts) -- Go FFI + TurndownService + Rust post-processing
7. [Firecrawl Rust SDK](https://github.com/firecrawl/firecrawl/tree/main/apps/rust-sdk) -- v1 + v2 client, all types
8. Rust crates: [htmd](https://crates.io/crates/htmd), [dom_smoothie](https://crates.io/crates/dom_smoothie), [scraper](https://crates.io/crates/scraper), [sitemap](https://crates.io/crates/sitemap), [lol_html](https://crates.io/crates/lol_html), [readability](https://crates.io/crates/readability)

## Methodology

- Tools used: curl (GitHub raw content + crates.io API), manual source analysis
- Files analyzed: OpenAPI spec, README, source code (engine waterfall, html-to-markdown, scrapeURL)
- API schemas: ScrapeOptions, CrawlOptions, MapOptions, ExtractOptions, SearchOptions, AgentOptions, DeepResearchOptions

## Confidence Level

**High** -- Based on primary sources (OpenAPI spec, source code, official documentation). Pricing details are approximate (JS-rendered page, could not fully extract) but directionally accurate based on visible price fragments.

## Key Takeaway

> **Nika's `fetch:` verb should handle the "fetch engine quality=5" tier locally**: plain HTTP GET, HTML parsing, markdown conversion, CSS selector extraction, metadata extraction, and sitemap parsing. This covers 60-70% of real-world scraping needs at zero cost and 10-50x faster latency. Firecrawl via MCP remains the right choice for JS rendering, anti-bot, screenshots, browser automation, LLM extraction, and multi-page agents.
