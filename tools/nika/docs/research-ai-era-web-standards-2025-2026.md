# Research Report: AI-Era Web Standards, SEO, and Content Discovery (2025-2026)

> Comprehensive analysis of cutting-edge web standards, protocols, and Rust ecosystem
> for building advanced `fetch:` capabilities in Nika workflow engine.

**Date**: 2026-03-19
**Researcher**: Claude Opus 4.6
**Sources analyzed**: 80+ pages across 16 parallel searches
**Confidence**: High (cross-referenced multiple authoritative sources)

---

## Table of Contents

1. [llms.txt Standard](#1-llmstxt-standard)
2. [AI Content Discovery Protocols](#2-ai-content-discovery-protocols)
3. [AI Crawler User Agents](#3-ai-crawler-user-agents)
4. [Spawning ai.txt & Permission Signals](#4-spawning-aitxt--permission-signals)
5. [Schema.org & JSON-LD for AI](#5-schemaorg--json-ld-for-ai)
6. [GEO / LLMO / AEO -- SEO for AI Era](#6-geo--llmo--aeo----seo-for-ai-era)
7. [Web Content Extraction Services](#7-web-content-extraction-services)
8. [MCP and Web Fetching](#8-mcp-and-web-fetching)
9. [Rust Crate Catalog](#9-rust-crate-catalog)
10. [Anti-Bot & TLS Fingerprinting](#10-anti-bot--tls-fingerprinting)
11. [Integration Recommendations for Nika](#11-integration-recommendations-for-nika)

---

## 1. llms.txt Standard

### What Is It?

A proposed specification for a **Markdown-formatted file** served at `/llms.txt` on any website, providing LLMs with a curated, machine-readable summary of the site's most important content. Proposed by **Jeremy Howard** and the **Answer.ai** team in September 2024.

**Key insight**: HTML is designed for humans with CSS, JavaScript, ads, and navigation. LLMs waste tokens parsing all of that. `llms.txt` gives them a clean table of contents.

### Exact Format Specification

```markdown
# Site/Company Name

> Brief description of what the site does (1-2 sentences, required blockquote)

Optional introductory details, metadata, or navigation notes.

## Core Documentation
- [API Reference](https://example.com/docs/api.md): Full API specification
- [Getting Started](https://example.com/docs/start.md): Quick start guide

## Products
- [Product A](https://example.com/products/a.md): Description of Product A
- [Pricing](https://example.com/pricing.md): Current plans and pricing

## Optional
- [Blog](https://example.com/blog.md): Lower-priority content, skip if token-limited
```

**Required elements**:
- H1 header with site/project name
- Blockquote summary

**Optional elements**:
- Free-form Markdown paragraphs
- H2 sections grouping URLs with descriptions
- Links should point to `.md` versions where possible (cleaner for LLMs)
- H3/H4 subsections, tables, code blocks are allowed

### llms-full.txt

Where `llms.txt` is a **table of contents with links**, `llms-full.txt` is the **entire documentation compiled into a single Markdown file** for direct ingestion without following links. Proposed by Mintlify in collaboration with Anthropic.

Example: `https://modelcontextprotocol.io/llms-full.txt` contains the complete MCP documentation.

### Major Adopters

- Anthropic (docs.anthropic.com)
- Stripe
- Vercel
- Mintlify (auto-generates for hosted docs)
- GitBook (auto-generates)
- Mastercard Developers
- ZenML
- Webex Developer

### Why It Matters for Nika

The `fetch:` verb should be able to:
1. **Detect** `/llms.txt` at any domain and prefer it over raw HTML scraping
2. **Parse** the Markdown format to extract structured links and descriptions
3. **Follow** linked `.md` files for deeper content when needed
4. **Fallback** gracefully to HTML extraction when no `llms.txt` exists

### Rust Implementation Notes

No dedicated Rust parser exists. Implementation is straightforward:
- Parse Markdown with `pulldown-cmark` or `comrak`
- Extract H1, blockquotes, H2 sections, and link lists
- Build a structured `LlmsTxt` type with title, summary, sections, and links

**Sources**: llmstxt.org, answer.ai, semrush.com, mintlify.com, gitbook.com, mastercard.com

---

## 2. AI Content Discovery Protocols

### The Landscape

| Protocol | Purpose | Format | Status |
|----------|---------|--------|--------|
| `robots.txt` | Crawler access control | Key-value directives | RFC 9309 (2022), universally adopted |
| `llms.txt` | LLM content curation | Markdown | Proposed standard, growing adoption |
| `llms-full.txt` | Complete docs for LLMs | Markdown | Extension of llms.txt |
| `ai.txt` (Spawning) | AI training permissions | Structured text | Proposed by Spawning.ai, IETF workshop |
| `sitemap.xml` | URL discovery | XML | W3C, universally adopted |
| IndexNow | Real-time URL push | HTTP API | Bing/Yandex, growing |
| JSON-LD | Structured data | JSON in HTML | W3C, critical for AI |
| Meta tags | Page-level AI control | HTML meta | Emerging proposals |

### robots.txt for AI

The original robots.txt spec (RFC 9309) remains the primary crawler control mechanism. AI companies have added new user-agent strings but no new directives. The Guardian News Media proposed ai.txt extensions at an IETF workshop.

**Current approach**: Block or allow specific AI bots by user-agent name.

```
# Block AI training crawlers
User-agent: GPTBot
Disallow: /

User-agent: Google-Extended
Disallow: /

# Allow AI search bots
User-agent: OAI-SearchBot
Allow: /

User-agent: ChatGPT-User
Allow: /
```

### IndexNow Protocol

Real-time push notification to search engines when content changes. Bing recommends pairing sitemaps (comprehensive) with IndexNow (immediate). Relevant for AI because fresh content matters for LLM grounding.

### Proposed IETF Extensions

IETF researchers are proposing granular purpose-specific directives beyond binary allow/block:
- Allow content for search indexing but prohibit AI training
- Specify permitted uses per crawler
- No formal RFC yet, but discussions are active

**Sources**: IETF slides, Cloudflare blog, Google developers, Search Engine Journal

---

## 3. AI Crawler User Agents

### Comprehensive List (2025-2026)

| User-Agent | Company | Purpose | Respects robots.txt |
|------------|---------|---------|---------------------|
| **GPTBot** | OpenAI | Training (ChatGPT data) | Yes |
| **ChatGPT-User** | OpenAI | Search (user-triggered retrieval) | Yes |
| **OAI-SearchBot** | OpenAI | Search (SearchGPT) | Yes |
| **Google-Extended** | Google | AI training + Gemini | Yes |
| **Googlebot** | Google | Search + AI Overviews | Yes |
| **Googlebot-Image** | Google | Image indexing for AI features | Yes |
| **ClaudeBot** | Anthropic | Search (Claude AI) | Yes |
| **anthropic-ai** | Anthropic | Training (Claude models) | Yes |
| **PerplexityBot** | Perplexity AI | Answer generation | Yes (controversial) |
| **Bytespider** | ByteDance | Training + search | Yes |
| **CCBot** | Common Crawl | Training (web corpus) | Yes |
| **meta-externalagent** | Meta | Training (Llama LLMs) | Yes |
| **Meta-WebIndexer** | Meta | Search (Meta AI) | Yes |
| **FacebookExternalHit** | Meta | Social previews + Meta AI | Yes |
| **Applebot-Extended** | Apple | AI features + Siri | Yes |
| **Applebot** | Apple | Search + Apple Intelligence | Yes |
| **cohere-ai** | Cohere | Training (Cohere models) | Yes |
| **Diffbot** | Diffbot | Structured data extraction | Partial |
| **Amazonbot** | Amazon | AWS + AI services | Yes |
| **PetalBot** | Huawei | Search + training | Yes |
| **YouBot** | You.com | You.com AI search | Yes |
| **Bingbot** | Microsoft | Search + Copilot | Yes |
| **PhindBot** | Phind | Developer AI search | Yes |
| **DuckAssistBot** | DuckDuckGo | DuckAssist AI | Yes |
| **MistralAI-User** | Mistral AI | Le Chat citations | Yes |
| **ImagesiftBot** | Imagesift | Image AI indexing | Yes |
| **Omgili** | SimilarWeb | Data aggregation for AI | Partial |

**Key stats** (Cloudflare, May 2024-2025):
- Overall crawler traffic rose **18%**
- GPTBot traffic grew **305%**
- Only ~**14%** of major domains had AI-specific robots.txt rules

### Why This Matters for Nika

The `fetch:` verb should:
1. **Set a proper User-Agent** that identifies Nika honestly
2. **Check robots.txt** before fetching (respect the protocol)
3. **Parse robots.txt** to understand AI-specific allow/disallow rules
4. **Understand** which sites block AI crawlers and handle gracefully

**Sources**: Cloudflare blog, Search Engine Journal, PulseRank, Momentic Marketing

---

## 4. Spawning ai.txt & Permission Signals

### Spawning.ai's ai.txt

A file at the website root declaring **AI training data permissions**. Unlike robots.txt (which controls crawling), ai.txt is checked during **content download/usage**, allowing real-time opt-outs even for pre-existing dataset links.

**Key features**:
- Selective permission/restriction for text, images, media
- Spawning API enforces for partners (Hugging Face, Stability AI)
- Python package `datadiligence` for training pipeline integration
- IETF workshop presentation by Guardian News Media

### C2PA (Content Provenance and Authenticity)

Standard for embedding cryptographic metadata in digital media to track origin, edits, and AI generation status. Relevant for:
- Indicating if content was AI-created
- Verifying media integrity
- EU AI Act compliance (Nika already has `nika:provenance` and `nika:verify`)

### HTML Meta Tag Proposals

Emerging non-standard tags for page-level AI control:
- `<meta name="robots" content="noai">` -- Block AI training use
- `<meta name="robots" content="noimageai">` -- Block image scraping for AI
- Extensions of existing noindex/nofollow model
- Not universally supported, adoption varies

### Why This Matters for Nika

The `fetch:` verb should:
1. **Check ai.txt** alongside robots.txt for permission signals
2. **Respect** `noai` meta tags in fetched HTML
3. **Extract** C2PA metadata from media (already supported via `nika:verify`)
4. **Report** permission status in fetch results metadata

**Sources**: Spawning blog, IETF slides, Creative Commons, TechPolicy Press

---

## 5. Schema.org & JSON-LD for AI

### The AI-Era Shift

Schema.org structured data has evolved from an SEO tactic to **core infrastructure for AI understanding**. Content with schema markup is **2.5x more likely** to appear in AI-generated answers (ChatGPT, Perplexity, Google AI Overviews).

### Critical Schema Types for AI

| Schema Type | Key Properties | AI Impact |
|-------------|---------------|-----------|
| `FAQPage` | `mainEntity` with Question/Answer | Direct Q&A extraction for AI responses |
| `HowTo` | Ordered steps as entities | Procedural AI answers |
| `Article` | `author`, `datePublished`, `about` | Source citability |
| `Organization` | `name`, `sameAs` (Wikidata links) | Entity disambiguation |
| `Product` | `offers`, `review`, `brand` | E-commerce AI answers |
| `BreadcrumbList` | Navigation hierarchy | Content structure understanding |

### JSON-LD Extraction Approach

JSON-LD appears in `<script type="application/ld+json">` tags in HTML:

```rust
use scraper::{Html, Selector};
use serde_json::Value;

fn extract_json_ld(html: &str) -> Vec<Value> {
    let document = Html::parse_document(html);
    let selector = Selector::parse("script[type='application/ld+json']").unwrap();
    document.select(&selector)
        .filter_map(|el| {
            el.text().next()
                .and_then(|t| serde_json::from_str::<Value>(t.trim()).ok())
        })
        .collect()
}
```

### Rust Crates for JSON-LD

| Crate | Downloads | Version | Status | Notes |
|-------|-----------|---------|--------|-------|
| `json-ld` | 400K | 0.21.4 | Active (Feb 2026) | Full W3C JSON-LD 1.1 implementation |
| `oxjsonld` | 84K | 0.2.4 | Active (Mar 2026) | Parser/serializer, streaming, Tokio async |
| `sophia_jsonld` | 108K | 0.9.0 | Active (Nov 2024) | Part of Sophia RDF toolkit, full 1.1 |

### Why This Matters for Nika

The `fetch:` verb should:
1. **Extract** JSON-LD from every fetched HTML page automatically
2. **Parse** into structured `serde_json::Value` for downstream tasks
3. **Expose** extracted schema data in task results (e.g., `{{with.page.json_ld}}`)
4. **Recognize** key types (Article, Product, FAQ) for intelligent processing

**Sources**: SchemaApp, Stackmatix, Digidop, schema.org, FRT Digital

---

## 6. GEO / LLMO / AEO -- SEO for AI Era

### Three New Optimization Paradigms

| Acronym | Full Name | What It Optimizes For |
|---------|-----------|----------------------|
| **GEO** | Generative Engine Optimization | Being cited in AI-generated answers (ChatGPT, Perplexity) |
| **LLMO** | LLM Optimization | Being correctly understood and cited by language models |
| **AEO** | Answer Engine Optimization | Being included in direct answer interfaces |

### Key Shifts from Traditional SEO

1. **From rankings to synthesis**: Content needs to be cited in AI summaries, not just ranked #1
2. **From keywords to entities**: LLMs understand entity relationships, not keyword density
3. **From backlinks to authority signals**: E-E-A-T, original research, verified proof
4. **From clicks to zero-click**: ~40% of traffic now lost to zero-click AI answers
5. **From Google-only to multi-channel**: LLM referral traffic grew 357% YoY in 2025
6. **From HTML to machine-readable**: JSON-LD, Markdown, API-friendly formats

### Technical Requirements for AI Discoverability

1. **Nested JSON-LD schema** for explicit entity relationships
2. **llms.txt** for curated LLM content summaries
3. **Clean semantic HTML** with proper heading hierarchy
4. **Flat, hierarchical URL structures** signaling "source of truth" pages
5. **Real-time product feeds** for e-commerce
6. **API accessibility** for agentic AI systems

### Why This Matters for Nika

The `fetch:` verb should **extract the signals that AI systems use to evaluate content quality**:
- Schema.org entities and relationships
- Author, date, source information
- Content structure (headings, sections)
- Freshness signals (lastmod, datePublished)
- Authority signals (citations, references)

**Sources**: SearchEngineLand, Vizion, Evergreen Media, Namastetu, AnalyticaHouse

---

## 7. Web Content Extraction Services

### Service Comparison

| Service | Type | JS Rendering | Output Formats | Anti-Bot | Pricing |
|---------|------|-------------|----------------|----------|---------|
| **Firecrawl** | API (open-source) | Yes (cloud Chrome) | Markdown, HTML, JSON, screenshots | Proxy rotation, 26 countries | Free tier + usage-based |
| **Jina Reader** (r.jina.ai) | API | Yes (Chrome + ReaderLM-v2) | Markdown, JSON | Built-in | 20 RPM free, up to 5K RPM |
| **Jina Search** (s.jina.ai) | API | Yes | Markdown, JSON | Built-in | Bundled |
| **Diffbot** | API | Yes (vision-based) | JSON-LD, structured | Built-in | ~$299/mo |
| **Bright Data** | API | Yes | JSON, CSV | Global proxies | $49+/mo |
| **Zyte** | API (Scrapy-backed) | Yes (Splash) | Structured JSON | ML-based | Usage-based |
| **ScrapeGraphAI** | API/Library | Yes | Structured | AI-driven | Free tier |
| **Readability.js** | Library (JS) | No | HTML/text | None | Free (OSS) |
| **Trafilatura** | Library (Python) | No | Text, XML | None | Free (OSS) |
| **Mercury/Postlight** | Library | No | JSON | None | Free (OSS) |

### Jina Reader Deep Dive

Jina's architecture is particularly interesting:
- **ReaderLM-v2**: A purpose-built 1.5B parameter model for HTML-to-Markdown conversion
- Runs on GPUs, processes billions of tokens daily
- Supports streaming responses, caching, proxies, cookies
- Image captioning (adds alt-text descriptions)
- **r.jina.ai**: Single URL to Markdown/JSON
- **s.jina.ai**: Web search + top 5-10 results scraped and structured

### Firecrawl Deep Dive

- **Endpoints**: /scrape (single URL), /crawl (entire sites, up to 10K pages), /map (URL discovery), /search, /extract (AI-powered structured extraction), /batch
- **Browser actions**: click, scroll, input, wait for CSS selectors
- **Open source**: github.com/firecrawl/firecrawl (self-hostable)
- **No Rust SDK** currently -- HTTP API only
- **LangChain integration** for AI pipelines

### Why This Matters for Nika

The `fetch:` verb could integrate multiple extraction strategies in a pipeline:
1. **First**: Check `llms.txt` (cheapest, most LLM-friendly)
2. **Second**: Try static HTML extraction with readability
3. **Third**: Fall back to JS rendering via Firecrawl/Jina for dynamic content
4. **Fourth**: Use Diffbot for structured entity extraction when needed

**Sources**: Firecrawl docs, Jina AI docs, Google Cloud blog, KDnuggets, Scrapeway

---

## 8. MCP and Web Fetching

### How MCP Relates to fetch:

MCP (Model Context Protocol) standardizes how AI agents connect to external tools. Several MCP servers provide web fetching capabilities:

| MCP Server | Description |
|------------|-------------|
| **Fetch MCP Server** | Basic URL fetching for AI agents |
| **Browser MCP Server** | Full browser interactions (navigation, clicking, scraping) |
| **Puppeteer MCP Server** | Headless Chrome automation via MCP |
| **Context MCP Server** | Intelligent fetch + HTML-to-Markdown + file saving |

### Nika Integration

Nika already uses MCP via the `invoke:` verb. The `fetch:` verb could:
1. Use **native HTTP** for simple fetches (fastest)
2. Delegate to **MCP servers** for complex scenarios (JS rendering, authentication)
3. The `invoke:` verb already supports calling MCP tools, so fetch could be a thin wrapper

**Sources**: modelcontextprotocol.io, Anthropic, Contentful, Google Cloud, MCPMarket

---

## 9. Rust Crate Catalog

### HTML Parsing & DOM

| Crate | Downloads | Version | Updated | Description |
|-------|-----------|---------|---------|-------------|
| **html5ever** | 48.5M | 0.39.0 | Mar 2026 | Browser-grade HTML5 parser (Servo project) |
| **scraper** | 14.8M | 0.26.0 | Mar 2026 | CSS selector-based HTML parsing and querying |
| **lol_html** | 2.8M | 2.7.2 | Feb 2026 | Cloudflare's streaming HTML rewriter with CSS selector API |
| **kuchikiki** | 10.8M | 0.8.8 | Feb 2025 | Brave's fork of kuchiki, HTML tree manipulation |
| **dom_query** | 138K | 0.27.0 | Mar 2026 | HTML querying and manipulation with CSS selectors |
| **ammonia** | 10M | 4.1.2 | Sep 2025 | HTML sanitization |

### Content Extraction & Readability

| Crate | Downloads | Version | Updated | Description |
|-------|-----------|---------|---------|-------------|
| **llm_readability** | 36K | 0.0.13 | Feb 2026 | **LLM-optimized** readability (Spider Cloud). Best for AI pipelines. |
| **readability** | 451K | 0.3.0 | Dec 2023 | Mozilla Readability port. Mature but less maintained. |
| **dom_smoothie** | 36K | 0.16.0 | Mar 2026 | **Actively maintained** content extraction. Fast-growing. |
| **readable-readability** | 26K | 0.4.0 | Dec 2022 | Fast readability implementation. Less maintained. |
| **meta_oxide** | 67 | 0.1.1 | Nov 2025 | Universal metadata extraction (13 formats including OG, Twitter, JSON-LD). 200-570x faster than alternatives. Very new. |

### HTML to Markdown

| Crate | Downloads | Version | Updated | Description |
|-------|-----------|---------|---------|-------------|
| **htmd** | 248K | 0.5.1 | Mar 2026 | turndown.js inspired. **Best maintained.** Active development. |
| **html2md** | 493K | 0.2.15 | Jan 2025 | Simple HTML to Markdown. Higher downloads, less recent updates. |
| **html-to-markdown** | 68K | 0.1.0 | Jul 2024 | Basic conversion. Less active. |

### Feed Parsing

| Crate | Downloads | Version | Updated | Description |
|-------|-----------|---------|---------|-------------|
| **feed-rs** | 873K | 2.3.1 | Dec 2024 | **Best choice.** Handles Atom, RSS 2.0/1.0/0.x, JSON Feed. Extensions support. |
| **rss** | (check crates.io) | 2.0.x | 2025 | RSS 2.0 only. Builder API. |
| **atom_syndication** | (check crates.io) | 0.12.x | 2024 | Atom 1.0 only. Strong typing. |

### Open Graph & Metadata

| Crate | Downloads | Version | Updated | Description |
|-------|-----------|---------|---------|-------------|
| **opengraph** | 25K | 0.2.4 | Oct 2018 | Extracts OG tags from HTML. **Very old, unmaintained.** |
| **meta_oxide** | 67 | 0.1.1 | Nov 2025 | 13 formats: HTML Meta, OG, Twitter Cards, JSON-LD, Dublin Core. **Best if it matures.** |
| **scraper** + manual | 14.8M | 0.26.0 | Mar 2026 | DIY: parse `meta[property^='og:']` selectors. Most reliable approach. |

### Robots.txt Parsing

| Crate | Downloads | Version | Updated | Description |
|-------|-----------|---------|---------|-------------|
| **texting_robots** | 471K | 0.2.2 | Mar 2023 | Thorough test suite against real-world data. 34M+ files tested. |
| **robotstxt** | 484K | 0.3.0 | Feb 2021 | Google C++ parser faithful port. |
| **robotxt** | 34K | 0.6.1 | Mar 2024 | Supports crawl-delay, sitemap, universal `*` wildcard. |

### Sitemap Parsing

| Crate | Downloads | Version | Updated | Description |
|-------|-----------|---------|---------|-------------|
| **sitemap** | 513K | 0.4.1 | Nov 2020 | Sitemap reader and writer. Works but dated. |
| **quick-xml** + manual | (very high) | latest | 2026 | DIY: Parse XML sitemaps manually. More flexible. |

### JSON-LD

| Crate | Downloads | Version | Updated | Description |
|-------|-----------|---------|---------|-------------|
| **json-ld** | 400K | 0.21.4 | Feb 2026 | Full W3C JSON-LD 1.1 implementation |
| **oxjsonld** | 84K | 0.2.4 | Mar 2026 | Parser/serializer. Streaming + async Tokio. **Most active.** |
| **sophia_jsonld** | 108K | 0.9.0 | Nov 2024 | Part of Sophia RDF toolkit. Full 1.1. |

### HTTP Clients

| Crate | Downloads | Version | Updated | Description |
|-------|-----------|---------|---------|-------------|
| **reqwest** | (very high) | 0.12.x | 2026 | **De facto standard.** Async, TLS, cookies, redirects, connection pooling. |
| **reqwest-impersonate** | 85K | 0.0.0 | Jul 2024 | TLS/JA3/JA4 fingerprint impersonation. |
| **hyper** | (very high) | 1.x | 2026 | Low-level HTTP. Foundation for reqwest. |
| **ureq** | (high) | latest | 2026 | Sync, simple, no-tokio. |

### Headless Browsers

| Crate | Downloads | Version | Updated | Description |
|-------|-----------|---------|---------|-------------|
| **chromiumoxide** | 1.5M | 0.9.1 | Feb 2026 | Async Chrome DevTools Protocol. **Most downloaded.** |
| **headless_chrome** | 1.5M | 1.0.21 | Feb 2026 | CDP control. Simple API. |
| **fantoccini** | 2.9M | 0.22.1 | Feb 2026 | WebDriver (W3C). **Most mature.** |
| **thirtyfour** | 1.2M | 0.36.1 | Jul 2025 | Selenium WebDriver. Chrome + Firefox. |

---

## 10. Anti-Bot & TLS Fingerprinting

### Cloudflare's 5 Detection Layers

1. **TLS fingerprinting** (JA3/JA4 hash of client hello)
2. **IP reputation** (datacenter vs residential, geolocation)
3. **JavaScript checks** (browser environment probing)
4. **Behavior profiling** (mouse movement, timing, scrolling patterns)
5. **Turnstile CAPTCHA** (interactive challenge)

### Bypass Strategies

| Strategy | Complexity | Effectiveness | Rust Support |
|----------|-----------|---------------|-------------|
| **Respect robots.txt** | None | N/A (ethical) | texting_robots, robotstxt |
| **Proper User-Agent** | Low | Medium | reqwest headers |
| **TLS fingerprint spoofing** | Medium | High | reqwest-impersonate |
| **Residential proxies** | Medium | High | reqwest + proxy config |
| **Headless browser** | High | Very High | chromiumoxide, headless_chrome |
| **Stealth browser (Camoufox)** | Very High | Highest | No Rust native, external process |
| **CAPTCHA solving API** | Medium | High for Turnstile | HTTP API integration |

### HTTP/3 and QUIC in Rust

- **Quinn**: Core QUIC protocol implementation (stable)
- **h3**: HTTP/3 over Quinn (experimental)
- **reqwest**: No native HTTP/3 yet
- **Recommendation**: Stick to HTTP/1.1 and HTTP/2 for production reliability

### Why This Matters for Nika

The `fetch:` verb should have **tiered fetching strategies**:
1. **Tier 1 (default)**: Plain reqwest with proper User-Agent, cookies, redirects
2. **Tier 2 (stealth)**: reqwest-impersonate for TLS fingerprint matching
3. **Tier 3 (render)**: Headless Chrome (chromiumoxide) for JS-heavy sites
4. **Tier 4 (external)**: Delegate to Firecrawl/Jina MCP server for maximum compatibility

**Sources**: Bright Data, Scrapfly, Capsolver, Browserless, Scrapedo

---

## 11. Integration Recommendations for Nika

### Proposed `fetch:` Verb Architecture

```
fetch:
  url: "https://example.com/page"
  mode: auto | static | render | api
  extract:
    - readability       # Main content (Markdown)
    - json_ld           # Structured data
    - open_graph        # OG metadata
    - links             # All links
    - feeds             # RSS/Atom feed discovery
    - llms_txt          # Check for /llms.txt
  respect:
    - robots_txt: true  # Check robots.txt
    - ai_txt: true      # Check ai.txt (Spawning)
    - noai_meta: true   # Respect <meta name="robots" content="noai">
  user_agent: "NikaBot/0.x (AI workflow engine; +https://nika.dev/bot)"
  timeout: 30
```

### Priority Crate Selections

| Need | Recommended Crate | Why |
|------|-------------------|-----|
| HTTP client | **reqwest** | De facto standard, async, full-featured |
| HTML parsing | **scraper** (14.8M downloads) | CSS selectors, rock-solid |
| Content extraction | **llm_readability** | LLM-optimized, actively maintained |
| HTML to Markdown | **htmd** | Most active, turndown.js-inspired |
| Feed parsing | **feed-rs** | Multi-format, most downloaded |
| robots.txt | **texting_robots** | Tested against 34M real files |
| Sitemap | **sitemap** + custom | Best available, supplement with quick-xml |
| JSON-LD extraction | **scraper** + **serde_json** | Extract `<script>` tags, parse JSON |
| JSON-LD processing | **oxjsonld** or **json-ld** | Full W3C spec if needed |
| Open Graph | **scraper** + manual selectors | opengraph crate is too old |
| HTML sanitization | **ammonia** | Battle-tested, 10M downloads |
| Headless browser | **chromiumoxide** | Async CDP, most active |
| HTML streaming | **lol_html** | Cloudflare's, zero-copy streaming |

### Extraction Pipeline Design

```
URL Input
    |
    v
[1. Protocol Check]
    |-- Check /robots.txt (texting_robots)
    |-- Check /ai.txt (custom parser)
    |-- Check /llms.txt (custom Markdown parser)
    |
    v
[2. Smart Fetch Strategy]
    |-- If llms.txt exists -> parse Markdown links
    |-- If static HTML sufficient -> reqwest + scraper
    |-- If JS required -> chromiumoxide
    |-- If anti-bot detected -> reqwest-impersonate or MCP delegate
    |
    v
[3. Content Extraction Layer]
    |-- Readability extraction (llm_readability)
    |-- HTML to Markdown (htmd)
    |-- JSON-LD extraction (scraper + serde_json)
    |-- Open Graph metadata (scraper selectors)
    |-- Feed discovery (feed-rs)
    |-- Link extraction (scraper)
    |
    v
[4. Output Normalization]
    |-- FetchResult {
    |       content: Markdown,
    |       metadata: { title, description, author, date, ... },
    |       json_ld: Vec<Value>,
    |       open_graph: OpenGraph,
    |       links: Vec<Link>,
    |       feeds: Vec<FeedUrl>,
    |       permissions: { robots: bool, ai_txt: bool, noai: bool },
    |   }
```

### AI Crawler Identification

Nika's `fetch:` verb should use a transparent User-Agent:
```
NikaBot/0.34 (AI workflow engine; +https://nika.dev/bot; respects robots.txt)
```

And provide a way for workflow authors to override it when appropriate.

### Feature Flags

```toml
[features]
fetch-core = ["reqwest", "scraper", "htmd", "llm_readability", "texting_robots"]
fetch-feeds = ["feed-rs"]
fetch-jsonld = ["json-ld"]  # or oxjsonld
fetch-render = ["chromiumoxide"]
fetch-stealth = ["reqwest-impersonate"]
```

---

## Sources

1. [llmstxt.org](https://llmstxt.org) -- Official llms.txt specification
2. [answer.ai](https://www.answer.ai/posts/2024-09-03-llmstxt.html) -- Original proposal by Jeremy Howard
3. [Cloudflare blog](https://blog.cloudflare.com/from-googlebot-to-gptbot-whos-crawling-your-site-in-2025/) -- AI crawler traffic analysis
4. [Spawning.ai](https://spawning.substack.com/p/aitxt-a-new-way-for-websites-to-set) -- ai.txt proposal
5. [IETF slides](https://www.ietf.org/slides/slides-aicontrolws-guardian-news-media-draft-paper-on-an-aitxt-protocol-00.pdf) -- AI control workshop
6. [Search Engine Journal](https://www.searchenginejournal.com/ai-crawler-user-agents-list/558130/) -- Complete AI user-agent list
7. [PulseRank](https://pulserank.ai/ai-crawlers-user-agents/) -- 2026 crawler reference
8. [SchemaApp](https://www.schemaapp.com/schema-markup/what-2025-revealed-about-ai-search-and-the-future-of-schema-markup/) -- Schema.org evolution
9. [Firecrawl](https://github.com/firecrawl/firecrawl) -- Open-source web extraction
10. [Jina Reader](https://jina.ai/reader/) -- ReaderLM-v2 architecture
11. [Google Cloud](https://cloud.google.com/blog/products/application-development/how-jina-ai-built-its-100-billion-token-web-grounding-system-with-cloud-run-gpus) -- Jina infrastructure details
12. [emschwartz.me](https://emschwartz.me/comparing-13-rust-crates-for-extracting-text-from-html/) -- Rust HTML extraction comparison
13. [modelcontextprotocol.io](https://modelcontextprotocol.io/docs/getting-started/intro) -- MCP specification
14. [Semrush](https://www.semrush.com/blog/llms-txt/) -- llms.txt analysis
15. [SearchEngineLand](https://searchengineland.com/ai-search-visibility-seo-predictions-2026-468042) -- AI SEO predictions 2026
16. [Google Developers](https://developers.google.com/crawling/docs/crawlers-fetchers/google-common-crawlers) -- Google crawler documentation

## Methodology

- **Tools used**: Perplexity AI (sonar model), crates.io API
- **Pages analyzed**: 80+ across 16 search queries
- **Crates investigated**: 30+ via crates.io API with download counts
- **Time period covered**: 2024-2026

## Confidence Level

**High** -- All major findings are cross-referenced across multiple independent sources. Crate data comes directly from crates.io API. The AI-era web standards landscape is rapidly evolving, so some emerging standards (ai.txt, meta tag proposals) have lower adoption certainty.

## Further Research Suggestions

- Deep dive into Firecrawl's open-source codebase for extraction algorithm details
- Benchmark Rust readability crates against each other on real-world HTML
- Investigate `dom_smoothie` (fast-growing, 16K recent downloads) as readability alternative
- Monitor IETF proposals for robots.txt AI extensions
- Evaluate `meta_oxide` once it matures (13 format support could be very useful)
- Research Cloudflare Workers AI for edge-based content extraction
- Investigate `comrak` vs `pulldown-cmark` for llms.txt Markdown parsing
