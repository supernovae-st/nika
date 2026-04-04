# Firecrawl Deep Technical Analysis

> **Date**: 2026-04-03
> **Purpose**: Feature-by-feature breakdown for building a superior Rust alternative
> **Sources**: GitHub repo, docs.firecrawl.dev, firecrawl.dev/pricing, source code analysis
> **Confidence**: HIGH -- based on primary sources (actual code + official docs)

---

## 1. Executive Summary

Firecrawl is a Y Combinator-backed SaaS that converts web pages into LLM-ready data. It is a
TypeScript monolith (Express + BullMQ + Redis + PostgreSQL + RabbitMQ) with a Go HTML-to-markdown
microservice and a native Rust NAPI module. It is NOT a lightweight tool -- it is a complex
distributed system with 74 production dependencies and at least 6 worker processes.

**Their moat**: not the code, but the infrastructure. "Fire Engine" (their proprietary browser
cloud with stealth proxies, IP rotation, anti-bot bypass) is NOT open source. Self-hosted
instances do NOT get Fire Engine. This is the single biggest differentiator.

**GitHub**: ~30K+ stars, AGPL-3.0 license, very active development.

---

## 2. Architecture Deep Dive

### 2.1 System Components (docker-compose)

```
                           +------------------+
                           |   API Server     |
                           |  (Express.js)    |
                           |  port 3002       |
                           +--------+---------+
                                    |
              +---------------------+---------------------+
              |                     |                     |
     +--------v--------+  +--------v--------+  +---------v--------+
     |  Redis           |  |  RabbitMQ       |  |  PostgreSQL       |
     |  (job queues,    |  |  (NUQ worker    |  |  (NUQ -- "nuq"    |
     |   rate limits,   |  |   message bus)  |  |   job persistence)|
     |   crawl state)   |  |                 |  |                   |
     +------------------+  +-----------------+  +-------------------+
              |
     +--------v---------+
     |  Playwright       |
     |  Microservice     |
     |  port 3000        |
     |  (Docker, 4GB RAM)|
     +-------------------+
```

### 2.2 Worker Processes (6+)

The `harness.ts` starts multiple processes:

| Worker | Purpose | File |
|--------|---------|------|
| API server | HTTP endpoints (Express) | `src/index.ts` |
| Queue worker | BullMQ scrape jobs | `services/queue-worker.js` |
| NUQ worker (x5) | PostgreSQL-backed job queue | `services/worker/nuq-worker.js` |
| NUQ prefetch worker | Pre-fetches URLs for crawl | `services/worker/nuq-prefetch-worker.js` |
| NUQ reconciler worker | Queue consistency | `services/worker/nuq-reconciler-worker.js` |
| Extract worker | LLM extraction pipeline | `services/extract-worker.js` |
| Index worker | Search index population | `services/indexing/index-worker.js` |

### 2.3 Scraping Engine Selection (The Waterfall)

This is the most interesting architectural decision. Firecrawl has **7 scraping engines**
with a quality-score-based fallback chain:

```
Engine                          Quality   Features
-------------------------------------------------------
index (cached results)          1000      Fast, cache-first
wikipedia (Enterprise API)       500      Special Wikipedia path
fire-engine;chrome-cdp            50      Full browser, actions, screenshots
fire-engine(retry);chrome-cdp     45      Same, but retry attempt
playwright                        20      Basic browser rendering
fire-engine;tlsclient             10      TLS fingerprint mimic (no JS)
fetch                              5      Basic HTTP fetch
pdf                              -20      PDF-specific parser
document                         -20      DOCX/XLSX parser
fire-engine;chrome-cdp;stealth    -2      Stealth proxy (enhanced)
fire-engine;tlsclient;stealth   -15      Stealth TLS client
```

**Engine selection algorithm**:
1. Determine required feature flags from request (actions, screenshot, pdf, etc.)
2. Score each engine by how many features it supports (weighted by priority)
3. Filter engines that meet a priority threshold (prioritySum / 2)
4. Sort by support score, then by quality score
5. Try engines in order -- if one fails, fall to next (waterfall)
6. Max 6 retry attempts with feature toggling

**Feature flags** (priority weights):
- `pdf` (100), `document` (100), `atsv` (90), `useFastMode` (90)
- `actions` (20), `stealthProxy` (20), `branding` (20)
- `screenshot` (10), `location` (10), `mobile` (10)
- `waitFor` (1)

### 2.4 HTML to Markdown Pipeline

Three-stage pipeline:

1. **Go converter** (`apps/go-html-to-md-service/`): Uses `github.com/firecrawl/html-to-markdown`
   (their own fork) with GitHub-Flavored Markdown plugin. Called via:
   - FFI through koffi (native shared library)
   - HTTP microservice fallback
2. **Rust post-processor** (`@mendable/firecrawl-rs`): Native NAPI module using `lol_html`
   for HTML rewriting. Handles post-processing cleanup.
3. **Content filtering**: `onlyMainContent` strips nav/footer/boilerplate (like Readability).

### 2.5 Native Rust Module (`apps/api/native/`)

A NAPI-RS bridge providing:
- `lol_html` for streaming HTML rewriting
- `pdf-inspector` for PDF text extraction
- `texting_robots` for robots.txt parsing
- `nodesig` (webhook signature verification)
- `calamine` for XLSX parsing
- `zip` for DOCX parsing
- `psl` for public suffix list
- `kuchikiki` for HTML DOM operations

### 2.6 Storage

- **Redis**: Job queues (BullMQ), rate limiting, crawl state, caching
- **PostgreSQL**: NUQ job persistence, ledger, team data
- **Supabase**: Auth, activity logs, team management (cloud only)
- **Google Cloud Storage**: Screenshots, Fire Engine results, PDF cache, media
- **Search index**: Cached scrape results for re-use (the "index" engine)

---

## 3. API Endpoints -- Complete Reference

### 3.1 Core Endpoints

| Endpoint | Method | Cost | Purpose |
|----------|--------|------|---------|
| `/v2/scrape` | POST | 1 credit/page | Scrape single URL |
| `/v2/scrape/{id}/interact` | POST | 2 credits/browser-min | Interact with scraped page |
| `/v2/crawl` | POST | 1 credit/page | Recursive site crawl |
| `/v2/crawl/{id}` | GET | 0 | Check crawl status |
| `/v2/crawl/{id}` | DELETE | 0 | Cancel crawl |
| `/v2/crawl/{id}/errors` | GET | 0 | Get crawl errors |
| `/v2/map` | POST | 1 credit/call | URL discovery |
| `/v2/search` | POST | 2 credits/10 results | Web search + optional scrape |
| `/v2/batch/scrape` | POST | 1 credit/page | Batch scrape multiple URLs |
| `/v2/extract` | POST | credits (token-based) | LLM structured extraction |
| `/v2/agent` | POST | dynamic | AI agent (research preview) |
| `/v2/browser` | POST | 2 credits/min | Remote browser session |

### 3.2 Scrape Request Parameters

```typescript
{
  url: string;                    // Required
  formats: (string | FormatObj)[];  // Output formats
  onlyMainContent: boolean;       // Default: true
  includeTags: string[];          // CSS selectors to include
  excludeTags: string[];          // CSS selectors to exclude
  headers: Record<string, string>;// Custom HTTP headers
  waitFor: number;                // Extra wait (ms)
  maxAge: number;                 // Cache freshness (ms), default 172800000 (2 days)
  storeInCache: boolean;          // Cache results, default true
  minAge: number;                 // Cache-only lookup
  timeout: number;                // Request timeout (ms), default 30000
  actions: Action[];              // Browser actions (max 50)
  location: { country, languages }; // Geo targeting
  proxy: 'basic' | 'stealth' | 'auto'; // Proxy selection
  parsers: ParserConfig[];        // PDF/document parsing
  zeroDataRetention: boolean;     // ZDR mode (enterprise)
  profile: string;                // Persistent browser profile
  mobile: boolean;                // Mobile viewport
  skipTlsVerification: boolean;   // Skip TLS
}
```

### 3.3 Output Formats (12 total)

| Format | Type | Description |
|--------|------|-------------|
| `markdown` | string | Clean GFM markdown |
| `html` | string | Cleaned HTML |
| `rawHtml` | string | Unmodified source HTML |
| `links` | string[] | All page links |
| `images` | string[] | All image URLs |
| `summary` | string | LLM-generated summary |
| `branding` | object | Brand identity extraction |
| `audio` | string | MP3 from video URLs (YouTube) |
| `json` | object | Schema + prompt: `{type: "json", schema, prompt}` |
| `screenshot` | object | `{type: "screenshot", fullPage, quality, viewport}` |
| `changeTracking` | object | `{type: "changeTracking", modes, tag, schema}` |
| `attributes` | object | `{type: "attributes", selectors: [{selector, attribute}]}` |

### 3.4 Browser Actions (9 types)

| Action | Parameters | Description |
|--------|-----------|-------------|
| `wait` | `milliseconds` or `selector` | Wait for time or element |
| `click` | `selector`, `all?` | Click element(s) |
| `write` | `text` | Type into focused field |
| `press` | `key` | Keyboard key press |
| `scroll` | `direction?`, `selector?` | Scroll page/element |
| `screenshot` | `fullPage?`, `quality?`, `viewport?` | Capture screenshot |
| `scrape` | (none) | Capture HTML at this point |
| `executeJavascript` | `script` | Run JS in page |
| `pdf` | `format?`, `landscape?`, `scale?` | Generate PDF |

### 3.5 Crawl Configuration

```typescript
{
  url: string;                    // Starting URL
  limit: number;                  // Max pages (default 10000)
  maxDiscoveryDepth: number;      // Link-hop depth limit
  includePaths: string[];         // URL regex patterns to include
  excludePaths: string[];         // URL regex patterns to exclude
  regexOnFullURL: boolean;        // Match against full URL vs pathname
  crawlEntireDomain: boolean;     // Follow to sibling/parent URLs
  allowSubdomains: boolean;       // Follow subdomain links
  allowExternalLinks: boolean;    // Follow external links
  allowBackwardLinks: boolean;    // Follow links going "up" in path
  deduplicateSimilarPages: boolean; // Content dedup
  ignoreSitemap: boolean;         // Skip sitemap discovery
  scrapeOptions: ScrapeOptions;   // Applied to every page
  webhook: WebhookConfig;         // Real-time notifications
}
```

### 3.6 Map Configuration

```typescript
{
  url: string;
  search: string;                 // Filter/rank URLs by relevance
  limit: number;                  // Max URLs returned
  sitemap: 'include' | 'exclude' | 'only'; // Sitemap behavior
  location: { country, languages };
}
```

### 3.7 Extract Configuration

```typescript
{
  urls: string[];                 // URLs (supports wildcards: example.com/*)
  prompt: string;                 // Natural language extraction prompt
  schema: JSONSchema;             // Structured output schema
  enableWebSearch: boolean;       // Extend beyond provided URLs
}
```

### 3.8 Agent Configuration

```typescript
{
  prompt: string;                 // What to find/extract
  urls?: string[];                // Optional starting URLs
  schema?: JSONSchema;            // Structured output
  model: 'spark-1-mini' | 'spark-1-pro'; // Agent model
}
```

### 3.9 Search Configuration

```typescript
{
  query: string;
  limit: number;                  // Per source type
  sources: ('web' | 'news' | 'images')[]; // Result types
  categories: ('github' | 'research' | 'pdf')[];
  scrapeOptions: ScrapeOptions;   // Optionally scrape results
  location: { country, languages };
  tbs: string;                    // Time-based search filter
  timeout: number;
}
```

### 3.10 Interact Configuration

```typescript
{
  prompt?: string;                // Natural language action
  code?: string;                  // Playwright/Python/Bash code
  language?: 'node' | 'python' | 'bash'; // Code language
}
```

---

## 4. Extraction Modes -- Detailed Breakdown

### 4.1 Markdown Extraction (Core)

**Pipeline**:
1. Fetch HTML (via engine waterfall)
2. Remove unwanted elements (nav, footer, ads) if `onlyMainContent: true`
3. Convert HTML to GFM via Go converter (forked `html-to-markdown`)
4. Post-process via Rust NAPI module (`lol_html`)
5. Return clean markdown

**Why their markdown is good**:
- Custom Go HTML-to-MD fork with GFM tables, code blocks
- Robust code block handling (language detection)
- Main content extraction (Readability-like)
- CSS selector include/exclude filtering
- Post-processing in Rust for performance

### 4.2 JSON/Structured Extraction (via /scrape)

Uses LLM to extract structured data from scraped content:
- Supports JSON Schema
- Optional natural language prompt
- Uses OpenAI/Anthropic/Google/Groq/etc. (via Vercel AI SDK)
- 4 additional credits per page
- Schema-validated output

### 4.3 Multi-page Extract (via /extract)

- Accepts URL wildcards (`example.com/*`)
- Auto-discovers and crawls relevant pages
- LLM aggregates data across all pages
- Supports web search expansion (`enableWebSearch`)
- Token-based billing (15 tokens = 1 credit)
- Beta: occasional inconsistencies on large sites

### 4.4 Brand Identity Extraction (branding)

Extracts comprehensive design system info:
- Color scheme (light/dark), all brand colors
- Font families, typography scale, weights
- Spacing, border radius, base unit
- Button styles, input styles
- Logo, favicon, OG image URLs
- Brand personality traits
- Requires Chrome CDP engine (fire-engine)

### 4.5 Audio Extraction

- Extracts MP3 from video URLs (YouTube)
- Returns signed GCS URL (expires 1 hour)
- Uses `avgrab` service internally

### 4.6 Change Tracking

- Tracks content changes between scrapes
- Modes: `json` (structured diff), `git-diff` (patch format)
- Tag-based comparison (compare specific versions)
- Bypasses cache (always fresh)
- Requires `markdown` format alongside

### 4.7 Attribute Extraction

- CSS selector-based HTML attribute extraction
- `{type: "attributes", selectors: [{selector: ".price", attribute: "data-value"}]}`

---

## 5. Anti-Bot & Proxy Strategy

### 5.1 Engine Hierarchy for Anti-Bot

```
Level 0: index (cached) -- no request needed
Level 1: fire-engine;chrome-cdp -- standard Chrome CDP
Level 2: fire-engine;tlsclient -- TLS fingerprint mimicry (no JS)
Level 3: fire-engine;chrome-cdp;stealth -- stealth proxy + CDP
Level 4: fire-engine;tlsclient;stealth -- stealth proxy + TLS
```

### 5.2 Fire Engine (Proprietary, NOT Open Source)

This is their secret sauce:
- **Chrome CDP**: Full Chromium browsers in the cloud
- **TLS Client**: Go-based TLS fingerprint mimicry (no browser needed, faster)
- **Stealth proxies**: Residential/mobile IPs that bypass Cloudflare/Akamai
- **Engpicker**: ML-based engine selector (`queryEngpickerVerdict`) that learns which
  engine works best for each domain
- **A/B testing**: `FIRE_ENGINE_AB_URL` + `FIRE_ENGINE_AB_RATE` for testing new engines

### 5.3 Self-Hosted Limitations

Self-hosted gets:
- Playwright (basic browser rendering)
- fetch (HTTP client)
- PDF/document parsers

Self-hosted does NOT get:
- Fire Engine (Chrome CDP cloud, stealth proxies)
- IP rotation
- Anti-bot bypass
- Index (cached results)

### 5.4 Proxy Configuration

- `proxy: 'basic'` -- standard datacenter proxy
- `proxy: 'stealth'` -- residential/mobile proxy (+4 credits)
- `proxy: 'auto'` -- let Firecrawl choose
- Location-aware: country-specific proxy selection
- `PROXY_SERVER`, `PROXY_USERNAME`, `PROXY_PASSWORD` for self-hosted

---

## 6. Crawl Features

### 6.1 URL Discovery

1. **Sitemap parsing**: Auto-discovers and parses sitemap.xml
2. **Link traversal**: Recursive link following from starting URL
3. **SERP enrichment**: Search engine results supplement discovery (map endpoint)
4. **Cached crawl data**: Previously crawled URLs are reused

### 6.2 Scope Control

- `includePaths` / `excludePaths`: Regex patterns
- `regexOnFullURL`: Match full URL vs just pathname
- `maxDiscoveryDepth`: Link-hop depth limit
- `crawlEntireDomain`: Follow sibling/parent paths
- `allowSubdomains`: Cross subdomain
- `allowExternalLinks`: Follow external links
- `allowBackwardLinks`: Follow "up" path links
- `deduplicateSimilarPages`: Content-based dedup

### 6.3 Delivery Methods

1. **Polling**: Submit job, poll `/crawl/{id}` for results
2. **WebSocket**: Real-time streaming via `watcher` method
3. **Webhooks**: Push notifications per page or on completion
   - Events: `crawl.started`, `crawl.page`, `crawl.completed`, `crawl.failed`
   - HMAC-SHA256 signature verification

### 6.4 Page Handling

- Default limit: 10,000 pages
- Results paginated in 10MB chunks (auto-handled by SDKs)
- Results expire after 24 hours
- Each page gets full scrape options (formats, actions, proxy, etc.)
- Failed pages tracked separately (`GET /crawl/{id}/errors`)

---

## 7. Unique/Advanced Features

### 7.1 Interact Endpoint (NEW)

Post-scrape browser interaction:
1. Scrape a page -> get `scrapeId`
2. Call `/interact` with natural language prompts or Playwright code
3. AI agent clicks, types, scrolls, extracts
4. Live view URL for watching/controlling the browser
5. Persistent profiles for session state across interactions

**Three interaction modes**:
- **Prompting**: Natural language -> AI controls browser
- **Code**: Playwright (Node/Python) or bash (`agent-browser` CLI)
- **agent-browser**: CLI with accessibility tree refs (`@e1`, `@e2`)

### 7.2 Agent Endpoint (Research Preview)

"Describe what you need, AI finds and extracts it":
- No URLs required -- agent searches, navigates, extracts
- Two models: `spark-1-mini` (cheap) and `spark-1-pro` (accurate)
- Structured output with schema
- Evolution of `/extract` endpoint
- 5 free daily runs

### 7.3 Search Endpoint

Integrated web search:
- Sources: web, news, images
- Categories: github, research, pdf
- HD image filtering with size queries
- Optional content scraping of results
- Location/language customization
- Time-based filtering

### 7.4 Caching System

- Default 2-day cache (`maxAge: 172800000`)
- Up to 5x speed improvement from cache
- `maxAge: 0` forces fresh scrape
- `storeInCache: false` skips caching
- `minAge` for cache-only lookups
- The "index" engine IS the cache (quality 1000 -- tried first)

### 7.5 Zero Data Retention (ZDR)

Enterprise feature:
- No data persisted beyond request lifetime
- +1 credit per page surcharge
- No screenshots in ZDR mode
- Endpoint-level or search-level ZDR

### 7.6 Change Tracking

Monitor content changes over time:
- JSON diff mode (structured)
- Git diff mode (patch format)
- Tag-based versioning
- Webhook notifications on changes

### 7.7 Document Parsing

Beyond HTML:
- **PDF**: 3 modes (fast text / auto / forced OCR)
  - Uses `pdf-parse`, RunPod OCR, self-hosted OCR, Rust `pdf-inspector`
  - 1 credit per PDF page
  - `maxPages` limit
- **DOCX**: Via `calamine` + `zip` in Rust module
- **XLSX**: Via `calamine` in Rust module
- **ODT/RTF**: Sample files suggest support

### 7.8 LLM Provider Support

Via Vercel AI SDK (`ai` package):
- OpenAI (`@ai-sdk/openai`)
- Anthropic (`@ai-sdk/anthropic`)
- Google (`@ai-sdk/google`)
- Google Vertex (`@ai-sdk/google-vertex`)
- Groq (`@ai-sdk/groq`)
- DeepInfra (`@ai-sdk/deepinfra`)
- Fireworks (`@ai-sdk/fireworks`)
- OpenRouter (`@openrouter/ai-sdk-provider`)
- Ollama (`ollama-ai-provider`)

---

## 8. Pricing Model

### 8.1 Plans

| Plan | Price/mo | Credits/mo | Concurrency | Extra Credits |
|------|----------|------------|-------------|---------------|
| Free | $0 | 500 (one-time) | 2 | -- |
| Hobby | $16/mo (annual) | 3,000 | 5 | $9/1K |
| Standard | $83/mo (annual) | 100,000 | 50 | $47/35K |
| Growth | $333/mo (annual) | 500,000 | 100 | $177/175K |
| Scale | $599/mo (annual) | 1,000,000 | 150 | custom |
| Enterprise | custom | custom | custom | custom |

### 8.2 Credit Costs

| Feature | Credits |
|---------|---------|
| Scrape | 1/page |
| Crawl | 1/page |
| Map | 1/call (regardless of URL count) |
| Search | 2/10 results |
| Browser | 2/minute |
| Agent | 5 free/day, then dynamic |
| JSON extraction | +4/page |
| Stealth proxy | +4/page |
| PDF parsing | 1/PDF page |
| ZDR | +1/page |
| Extract | 15 tokens = 1 credit |

### 8.3 Pricing Analysis

- At Standard tier: $83/mo for 100K pages = **$0.00083/page**
- At Growth tier: $333/mo for 500K pages = **$0.00067/page**
- JSON extraction: effectively **$0.004/page** at Standard
- Stealth proxy: effectively **$0.004/page** at Standard
- Credits do NOT roll over (except auto-recharge and custom plans)

---

## 9. Weaknesses & User Complaints

### 9.1 Known Limitations (from docs)

- **Extract beta**: Large-scale coverage incomplete, inconsistent results
- **Map speed vs completeness**: Prioritizes speed, may miss URLs
- **Self-hosted**: No Fire Engine, no anti-bot, no index, severely limited
- **Screenshot expiry**: URLs expire after 24 hours
- **Audio expiry**: GCS URLs expire after 1 hour
- **Action limits**: Max 50 actions, 60s total wait time
- **No robots.txt opt-out**: Users complained about lack of opt-out (issue #1169)

### 9.2 GitHub Issues Themes

- **Anti-bot loops**: Self-hosted gets stuck in retry loops when blocked (#2350, 25 comments)
- **User-agent identification**: Sites can't block Firecrawl specifically (#1169, 11 comments)
- **SDK bloat**: axios in JS SDK is heavy (#615, 15 comments)
- **PDF embedded content**: Can't scrape embedded PDFs (#839, 10 comments)
- **Credit billing**: Users concerned about being charged for failed requests
- **Self-host complexity**: Docker setup issues, missing features

### 9.3 Architectural Weaknesses

1. **Node.js performance ceiling**: 74 dependencies, Express-based, GC pauses
2. **Multi-language complexity**: TypeScript + Go + Rust NAPI = hard to contribute
3. **Redis/RabbitMQ/PostgreSQL**: Three data stores = operational overhead
4. **Fire Engine lock-in**: The good stuff is proprietary
5. **No offline/local mode**: Always needs API key (even self-hosted needs Redis + RabbitMQ + PG)
6. **Webhook-only for real-time crawl**: No native SSE streaming (WebSocket for SDK)

---

## 10. Feature Comparison Matrix: What to Match or Exceed

### 10.1 MUST MATCH (table stakes)

| Feature | Firecrawl | Notes for Rust Impl |
|---------|-----------|---------------------|
| Single page scrape | Yes | Core fetch verb |
| HTML to Markdown | Go + Rust | Use `lol_html` + custom converter |
| Main content extraction | Yes | Readability algorithm |
| CSS selector filtering | Yes | Include/exclude tags |
| PDF parsing | Yes (3 modes) | `pdf-extract` or similar |
| Screenshot | Yes (via CDP) | Chrome DevTools Protocol |
| JSON structured extraction | Yes (LLM) | Already have in structured: |
| Recursive crawl | Yes | Sitemap + link traversal |
| URL mapping | Yes | Fast sitemap + SERP |
| Batch scrape | Yes | for_each with concurrency |
| Webhooks | Yes | Already planned |
| Rate limiting | Yes | Token bucket |
| Cache/maxAge | Yes | HTTP cache headers |
| Location/proxy | Yes | Geo-aware proxy selection |

### 10.2 SHOULD EXCEED

| Feature | Firecrawl | Opportunity |
|---------|-----------|-------------|
| Performance | P95 3.4s | Rust: <1s for static, <2s for JS |
| Memory | 8GB API + 4GB Playwright | Single binary, <512MB |
| Dependencies | 74 npm + Go + Docker | Zero runtime deps |
| Local mode | Requires Redis+RabbitMQ+PG | Single binary, embedded |
| Self-hosted parity | Severely limited | Full feature parity |
| Streaming | WebSocket only | Native SSE in Nika serve |
| Anti-bot | Proprietary Fire Engine | Pluggable engine system |
| DOCX/XLSX | Via Rust NAPI | Native Rust crates |
| Pricing | $0.00083/page minimum | Free (self-hosted) or cheaper |

### 10.3 NICE TO HAVE (differentiators)

| Feature | Firecrawl | Opportunity |
|---------|-----------|-------------|
| Browser actions | 9 action types | Match via headless Chrome |
| Interact (post-scrape) | Yes (new) | Agent verb handles this |
| Agent endpoint | spark-1 models | Nika agent verb |
| Change tracking | JSON + git-diff | Content hash comparison |
| Branding extraction | Yes | LLM-based via infer |
| Audio extraction | YouTube only | ffmpeg pipeline |
| Search integration | Web/news/images | Pluggable search providers |
| MCP server | Yes | Already have |
| SDK (Python/Node/Go) | Yes | nika-client already exists |

---

## 11. Architecture Recommendations for Rust Implementation

### 11.1 Core Engine Design

```
fetch verb (current)
  |
  +-- Static renderer (reqwest + lol_html)     -- Quality: 100
  |
  +-- Dynamic renderer (Chrome CDP via chromiumoxide/headless_chrome)
  |     +-- Standard mode                      -- Quality: 50
  |     +-- Stealth mode (proxy rotation)      -- Quality: -2
  |
  +-- Document parser
  |     +-- PDF (pdf-extract + optional OCR)   -- Quality: -20
  |     +-- DOCX (calamine + zip)              -- Quality: -20
  |
  +-- Wikipedia API (optional)                 -- Quality: 500
```

### 11.2 Markdown Quality Strategy

Firecrawl's markdown quality comes from:
1. Main content extraction (strip boilerplate)
2. Custom Go HTML-to-MD with GFM support
3. Rust post-processing

For Nika:
- Use `lol_html` for streaming HTML rewriting (already in Rust ecosystem)
- `html2md` or custom converter for GFM output
- Readability algorithm for main content extraction
- CSS selector filtering via `scraper` crate

### 11.3 Key Crates

| Purpose | Crate | Notes |
|---------|-------|-------|
| HTTP client | `reqwest` | Already using |
| HTML parsing | `scraper` + `lol_html` | Streaming rewrite |
| CSS selectors | `selectors` (via scraper) | Include/exclude |
| Markdown | `html2md` or custom | GFM tables, code blocks |
| Readability | `readability` or port | Main content extraction |
| Chrome CDP | `chromiumoxide` | Browser automation |
| PDF | `pdf-extract` | Text extraction |
| Sitemap | `roxmltree` | XML parsing |
| robots.txt | `texting_robots` | Same as Firecrawl uses |
| URL parsing | `url` | Already using |
| Rate limiting | `governor` | Token bucket |

### 11.4 What NOT to Copy

1. **Multi-process architecture**: Nika's single-binary approach is better
2. **Redis/RabbitMQ/PostgreSQL**: Use embedded alternatives (sqlite, channels)
3. **BullMQ job queues**: Use tokio tasks + channels
4. **Go microservice**: Do it all in Rust
5. **74 npm dependencies**: Zero is the target

---

## 12. Sources

1. [GitHub README](https://raw.githubusercontent.com/mendableai/firecrawl/main/README.md)
2. [Scrape docs](https://docs.firecrawl.dev/features/scrape)
3. [Crawl docs](https://docs.firecrawl.dev/features/crawl)
4. [Extract docs](https://docs.firecrawl.dev/features/extract)
5. [Map docs](https://docs.firecrawl.dev/features/map)
6. [Search docs](https://docs.firecrawl.dev/features/search)
7. [Interact docs](https://docs.firecrawl.dev/features/interact)
8. [Advanced Scraping Guide](https://docs.firecrawl.dev/advanced-scraping-guide)
9. [Pricing page](https://firecrawl.dev/pricing)
10. [Self-hosting guide](https://github.com/mendableai/firecrawl/blob/main/SELF_HOST.md)
11. Source code: `apps/api/src/scraper/scrapeURL/engines/index.ts` (engine selection)
12. Source code: `apps/api/src/config.ts` (configuration schema)
13. Source code: `apps/api/package.json` (dependencies)
14. Source code: `apps/api/native/Cargo.toml` (Rust module)
15. Source code: `docker-compose.yaml` (architecture)
16. Source code: `apps/go-html-to-md-service/converter.go` (markdown converter)

## Methodology

- **Tools used**: curl, GitHub API, direct source code analysis
- **Pages analyzed**: 16+ documentation pages, 10+ source files
- **Primary sources only**: No secondary blog posts or reviews -- all from Firecrawl's own code and docs

## Confidence Level

**HIGH** -- This analysis is based entirely on primary sources: the actual source code in their
GitHub repository and their official documentation. The architecture section was derived from
reading `docker-compose.yaml`, `config.ts`, `engines/index.ts`, and `package.json` directly.
