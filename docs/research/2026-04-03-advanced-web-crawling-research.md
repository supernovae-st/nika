# Research Report: Advanced Web Crawling & Scraping Techniques (2025-2026)

**Date**: 2026-04-03
**For**: Nika `fetch:` verb enhancement roadmap
**Scope**: Anti-detection, content extraction, crawl architecture, scale patterns, emerging techniques

---

## Executive Summary

Nika's `fetch:` verb already has a solid foundation: 10 extract modes (markdown, article, text, selector, metadata, links, jsonpath, feed, llm_txt, sitemap), SSRF protection with DNS pinning, streaming body limits, redirect chain tracking, and binary CAS storage. This report identifies **23 actionable enhancements** across 7 categories, prioritized P0/P1/P2 based on impact for a workflow engine (not a general-purpose crawler).

The biggest competitive gaps are: TLS fingerprint detection (P1), robots.txt compliance (P0), conditional requests for incremental workflows (P1), and content deduplication (P1). The biggest competitive *advantages* to build on are: LLM-powered extraction via existing `infer:` + `structured:` pipeline, the 10-mode extract system, and Nika's DAG orchestration for multi-step crawl workflows.

---

## Current State Analysis (Nika v0.62)

### What Nika Already Does Well

| Capability | Implementation | Status |
|---|---|---|
| 10 Extract modes | `extract.rs` (1233 LOC) | Solid |
| Readability (article) | `dom_smoothie` 0.16 | Solid |
| HTML-to-Markdown | `htmd` 0.5 | Solid |
| CSS selector extraction | `scraper` 0.26 | Solid |
| Feed parsing | `feed-rs` 2.3 | Solid |
| SSRF protection | DNS pinning, private IP blocking | Solid |
| Streaming body limits | 50MB text, 100MB binary | Solid |
| Binary CAS storage | `response: binary` | Solid |
| Redirect chain tracking | CRAWL-003 | Solid |
| JSONPath extraction | Zero-dep | Solid |
| Sitemap parsing | Feature-gated | Solid |
| llms.txt discovery | Fallback chain | Solid |
| Metadata extraction | OG, Twitter Cards, JSON-LD | Solid |

### What's Missing

| Gap | Priority | Reason |
|---|---|---|
| robots.txt parsing | P0 | Ethical scraping baseline |
| Conditional requests (ETag/If-Modified-Since) | P1 | Incremental workflows |
| TLS fingerprint impersonation | P1 | Cloudflare/DataDome bypass |
| Cookie jar / session persistence | P1 | Multi-step authenticated scraping |
| Per-domain rate limiting | P1 | Politeness |
| Content deduplication | P1 | Avoid re-processing |
| HTTP/2 fingerprint awareness | P2 | Advanced anti-bot evasion |
| Headless browser fallback | P2 | JS-heavy pages |
| Schema.org/JSON-LD structured extraction | P1 | Already partial in metadata mode |

---

## 1. Anti-Bot Detection & Bypass

### How Detection Works (JA3/JA4/JA4+)

Modern anti-bot systems (Cloudflare, DataDome, PerimeterX/HUMAN, Akamai) use **multi-layer fingerprinting**:

**Layer 1 -- TLS ClientHello Fingerprint (JA3/JA4)**
- The TLS handshake's ClientHello message contains: TLS version, cipher suites, extensions, elliptic curves, ALPN, SNI
- **JA3** (2017): MD5 hash of exact order of these fields. Defeated by randomization
- **JA4** (2024-2025, now industry standard): Sorts cipher suites/extensions alphabetically, skips GREASE values, adds ALPN/SNI/TCP distinction. Resistant to randomization
- **JA4+**: Extends to HTTP method, version, User-Agent -- links TLS to application layer
- **Result**: `reqwest` with default `rustls` produces a distinctive non-browser JA4 fingerprint that is trivially detected

**Layer 2 -- HTTP/2 Fingerprint**
- SETTINGS frame values (INITIAL_WINDOW_SIZE, MAX_CONCURRENT_STREAMS)
- HPACK header compression behavior
- Stream priority signaling
- Pseudo-header ordering (:method, :authority, :scheme, :path)
- Chrome sends proprietary SETTINGS values; curl/reqwest use minimal defaults

**Layer 3 -- HTTP Header Fingerprint**
- Header ordering (Chrome: sec-ch-ua first, then User-Agent; reqwest: alphabetical)
- Presence of Sec-CH-UA, Sec-Fetch-*, Priority headers
- Accept-Encoding capabilities (br, gzip, deflate, zstd)
- Missing headers that browsers always send (Accept, Accept-Language)

**Layer 4 -- Behavioral Analysis**
- Request timing patterns (bots are too regular)
- Cookie handling (bots don't accumulate cookies)
- JavaScript challenge completion
- Mouse/keyboard events (headless browser detection)

### Rust Ecosystem for TLS Impersonation

| Crate | Approach | Status (2025-2026) |
|---|---|---|
| **wreq** | BoringSSL-based, `.emulation(Emulation::Firefox136)` | Active, best option |
| **rquest** (penumbra-x) | reqwest fork with browser impersonation | Active, Chrome/Firefox/Safari profiles |
| **reqwest-impersonate** | Patches reqwest for browser TLS | Maintenance unclear |
| **boring** (BoringSSL bindings) | Low-level TLS control | Foundation for wreq/rquest |

**wreq** example (what Nika could integrate):
```rust
let client = Client::builder()
    .emulation(Emulation::Firefox136)  // Full TLS + HTTP/2 + header emulation
    .build()?;
let resp = client.get("https://example.com").send().await?;
```

### Recommended Implementation for Nika

**P1 -- Browser emulation mode** (feature-gated):
```yaml
# New fetch: field
- id: scrape_protected
  fetch:
    url: "https://example.com"
    emulate: chrome    # or: firefox, safari, none (default)
    extract: article
```

Implementation approach:
- Feature gate: `fetch-emulate` (adds `wreq` or `rquest` dep)
- When `emulate:` is set, build a one-off client with browser TLS/HTTP/2 profile
- When not set, use existing `reqwest` client (fast, no extra deps)
- Start with Chrome and Firefox profiles only

**P2 -- Realistic header ordering**:
- When `emulate:` is set, automatically add correct Sec-CH-UA, Sec-Fetch-*, Accept, Accept-Language headers in browser-correct order
- Remove the generic `nika/{version}` User-Agent
- Add appropriate Accept-Encoding based on emulated browser

---

## 2. Content Extraction Quality

### Benchmark Comparison (2025)

| Algorithm | Precision | Recall | F1-Score | Speed | Nika uses? |
|---|---|---|---|---|---|
| **Trafilatura** | 90.1% | 83.1% | 87.1% | Fastest (4.8x baseline) | No (Python) |
| **Readability (dom_smoothie)** | 92.8% | 74.3% | 84.4% | Moderate | Yes (extract: article) |
| **Dragnet** | 90.9% | 72.2% | 82.5% | Moderate | No (Python) |
| **newspaper3k** | 89.5% | 59.3% | 76.2% | Slow | No (Python) |

**Key insight**: Nika already uses the Rust Readability implementation (`dom_smoothie`), which has the highest precision (92.8%) and best consistency across page complexity levels. Trafilatura wins on recall and speed but is Python-only.

### Trafilatura's Approach (What Makes It Better)

Trafilatura combines multiple strategies:
1. Rule-based tag stripping (similar to Nika's `strip_non_content_tags`)
2. Content density analysis (text-to-tag ratio per DOM subtree)
3. Metadata extraction (author, date, topics) as side output
4. Fallback chains: try main content extraction, fall back to simpler heuristics
5. Language detection and multilingual support

**What Nika could adopt** (P1):
- Content density scoring as a pre-filter before Readability
- Fallback chain: try `extract: article`, if it fails (Readability can't find main content), fall back to `extract: text` with heuristic content detection
- Return metadata alongside article content (author, publish date)

### LLM-Based Extraction (Nika's Killer Advantage)

This is where Nika has a **massive structural advantage** over every other scraping tool. The pattern:

```yaml
# Step 1: Fetch raw content
- id: fetch_page
  fetch:
    url: "{{inputs.url}}"
    extract: markdown

# Step 2: LLM extracts structured data
- id: extract_data
  with:
    content: $fetch_page
  infer:
    prompt: |
      Extract the product information from this page content:
      {{with.content}}
  structured:
    schema:
      type: object
      properties:
        name: { type: string }
        price: { type: number }
        description: { type: string }
        specs: { type: array, items: { type: string } }
      required: [name, price]
```

No other scraping tool has a native 5-layer schema-validated extraction pipeline built in. Firecrawl charges per-page for LLM extraction; Nika users bring their own keys and get unlimited extraction with automatic retry and repair.

**P0 -- Document this pattern prominently.** It is the single strongest differentiator for Nika's fetch verb.

---

## 3. JavaScript Rendering

### When JS Rendering Is Needed

| Page Type | Static Fetch OK? | Examples |
|---|---|---|
| News articles, blogs | Yes | NYT, Medium, Wikipedia |
| E-commerce product pages | Usually | Amazon (SSR), but some SPAs |
| SPAs (React/Vue/Angular) | No | Many dashboards, modern apps |
| Infinite scroll pages | No | Twitter/X, Instagram |
| Pages behind JS challenges | No | Cloudflare turnstile |

**Cost comparison**:
- Static fetch: 50-200ms, ~0 CPU
- Headless browser: 2-10 seconds, 100-500MB RAM per instance
- Ratio: **10x-100x slower, 1000x more resource-intensive**

### Recommended Approach for Nika

**P2 -- Optional headless browser integration** (long-term):
```yaml
- id: scrape_spa
  fetch:
    url: "https://spa-app.com"
    render: true           # Enable JS rendering
    render_wait: 3000      # Wait 3s for JS to settle
    extract: markdown
```

Implementation options:
1. **chromiumoxide** (Rust): Native Chrome DevTools Protocol client
2. **fantoccini** (Rust): WebDriver client (works with any browser)
3. **External service**: Shell out to Playwright/Puppeteer via exec

**Pragmatic recommendation**: Do NOT build this in-core. Instead, document the two-step pattern:
```yaml
# Use exec: to render, then extract
- id: render
  exec:
    command: "npx single-file {{inputs.url}} --dump-content"
    shell: true

- id: extract
  with:
    html: $render
  # Use nika:html_to_md builtin or infer for extraction
```

This keeps Nika lean while enabling JS rendering for users who need it.

---

## 4. Crawl Efficiency & Politeness

### P0 -- robots.txt Compliance

**This is a must-have for ethical scraping.**

Rust crates: `robotstxt` or `texting_robots` (Google's robotstxt C++ parser, Rust bindings).

```yaml
# New workflow-level or task-level option
- id: scrape
  fetch:
    url: "https://example.com/page"
    respect_robots: true    # Default: true (opt-out with false)
    extract: article
```

Implementation:
- Cache robots.txt per domain (in-memory LRU, 1000 domains)
- Check before every fetch
- Respect `Crawl-delay` directive
- Use `Nika/{version}` as the bot identifier in robots.txt matching

### P1 -- Conditional Requests (ETag/If-Modified-Since)

Essential for incremental crawling workflows:

```yaml
- id: check_update
  fetch:
    url: "https://example.com/data.json"
    conditional: true       # Send If-Modified-Since / If-None-Match
    extract: jsonpath
    selector: "$.data"
```

Implementation:
- Store `ETag` and `Last-Modified` headers per URL in `.nika/cache/`
- On next fetch, send `If-None-Match` / `If-Modified-Since`
- On 304 Not Modified, return cached content
- On 200, update cache and return new content
- Saves bandwidth and respects server caching

### P1 -- Per-Domain Rate Limiting

```yaml
# In nika.toml
[fetch]
rate_limit_per_domain = 2    # requests per second per domain
rate_limit_global = 10       # requests per second total
```

Implementation:
- Token bucket per domain
- `governor` crate (Rust rate limiter, production-ready)
- Applies automatically to all `fetch:` tasks targeting same domain
- `for_each` with `concurrency: 5` + rate limit = safe parallel scraping

### P1 -- Content Deduplication

For crawl workflows processing many pages:

```yaml
- id: crawl_site
  for_each: "$urls.list"
  as: url
  fetch:
    url: "{{with.url}}"
    deduplicate: true       # Skip if content hash matches previous fetch
    extract: article
```

Strategies (in order of complexity):
1. **Exact hash** (SHA-256 of response body): Simple, catches exact duplicates
2. **SimHash**: 64-bit fingerprint, Hamming distance < 3 = near-duplicate. Good for articles with minor changes (ads, timestamps)
3. **MinHash + LSH**: Best for large-scale dedup (100K+ pages), O(1) lookup

**Recommended**: Start with exact hash (P1), add SimHash later (P2).

---

## 5. Scale Patterns

### Connection Pooling (Already Handled)

Nika uses `reqwest::Client` with built-in connection pooling. The current implementation creates custom clients only when needed (DNS pinning, redirect policy). This is correct.

**Enhancement (P2)**: Per-domain connection limits.
```rust
// reqwest doesn't expose per-host pool limits directly
// But governor rate limiter + concurrent task limit achieves the same effect
```

### DNS Caching (P2)

`reqwest` with `hickory-dns` (formerly trust-dns) enables async DNS resolution with caching.

```toml
# Cargo.toml
reqwest = { version = "0.13", features = ["hickory-dns"] }
```

This is a one-line change that improves performance for crawl workflows hitting many URLs on the same domain.

### Compression (Already Handled)

Current `reqwest` features: `gzip`, `brotli`, `deflate`. 

**Enhancement (P2)**: Add `zstd` support:
```toml
reqwest = { version = "0.13", features = ["json", "stream", "rustls", "gzip", "brotli", "deflate", "zstd"] }
```

### Memory Management for Large Crawls

Current streaming body reader (`read_body_with_limit`) already prevents OOM. For 100K+ page workflows:

**P2 -- Incremental artifact writes**: Instead of accumulating all results in memory, write artifacts per-task in `for_each` loops. (Already possible with `artifact:` per task.)

---

## 6. Output Quality Enhancements

### P1 -- Enhanced Markdown Conversion

Current `htmd` produces decent markdown but can be improved:

1. **Table preservation**: Ensure HTML tables convert to proper Markdown tables
2. **Code block handling**: Detect `<pre><code>` and convert to fenced code blocks with language hints
3. **Image extraction**: Convert `<img>` to `![alt](src)` with absolute URLs
4. **Link resolution**: Convert relative URLs to absolute using base URL

Some of these may already work with `htmd` 0.5; worth testing and documenting.

### P1 -- Schema.org / JSON-LD Deep Extraction

Current `extract: metadata` already extracts JSON-LD. Enhancement:

```yaml
# New extract mode or enhancement to metadata
- id: get_product
  fetch:
    url: "https://shop.com/product"
    extract: schema_org     # Specifically parse Schema.org types
```

Returns typed Schema.org data:
```json
{
  "type": "Product",
  "name": "Widget",
  "price": 29.99,
  "currency": "USD",
  "availability": "InStock",
  "rating": 4.5,
  "reviews_count": 127
}
```

This is high-value because many e-commerce sites embed Schema.org data for SEO.

### P1 -- Semantic Chunking for RAG

Add a built-in chunking transform or extract mode:

```yaml
- id: fetch_docs
  fetch:
    url: "https://docs.example.com/guide"
    extract: markdown

- id: chunk
  with:
    content: $fetch_docs
  invoke:
    tool: nika:chunk
    params:
      strategy: semantic     # or: fixed, recursive
      max_tokens: 512
      overlap: 50
```

Implementation: Recursive character splitting (the most practical for workflow engines):
- Split on `\n\n` (paragraphs) first
- Then `\n` (lines)
- Then sentences
- Respect heading boundaries (never split across `## Section`)
- Optional overlap for retrieval context

---

## 7. Emerging Techniques

### P0 -- llms.txt Enhancement

Nika already supports `extract: llm_txt`. Current implementation should be verified against the spec:

**Specification** (llmstxt.org, September 2024):
- File at domain root: `https://example.com/llms.txt`
- MIME type: `text/plain`, UTF-8
- Format: Strict Markdown with H1 (title), blockquote (summary), H2 (sections), bullet lists with links
- Companion: `llms-full.txt` (expanded version with all content)

**Adopted by**: Anthropic, Cloudflare, Mintlify, OpenPipe, Model Context Protocol, many AI-focused sites.

**Enhancement (P1)**: Auto-detect and prefer `llms-full.txt` when available:
```yaml
- id: get_docs
  fetch:
    url: "https://docs.anthropic.com"
    extract: llm_txt
    # Auto-tries: /llms-full.txt -> /llms.txt -> /.well-known/llm.txt
```

### P1 -- LLM-as-Scraper Pattern (Document & Optimize)

This is Nika's biggest differentiator. The two-step fetch+infer pattern should be:
1. Prominently documented in showcase workflows
2. Optimized with a shorthand:

```yaml
# Current (explicit two-step)
- id: fetch
  fetch: { url: "...", extract: markdown }
- id: extract
  with: { html: $fetch }
  infer: "Extract product data from: {{with.html}}"
  structured: { schema: { ... } }

# Future shorthand (P2, syntax sugar only)
- id: extract_product
  fetch:
    url: "https://shop.com/product"
    extract: markdown
  infer: "Extract the product information from this page content"
  structured:
    schema:
      type: object
      properties:
        name: { type: string }
        price: { type: number }
```

### P2 -- AI-Powered CSS Selector Generation

When a user needs to extract specific data but doesn't know the CSS selector:

```yaml
- id: find_prices
  fetch:
    url: "https://shop.com/products"
    extract: selector
    selector: auto("product prices with names")  # LLM generates CSS selector
```

Implementation: Send a sample of the HTML to the LLM, ask it to generate the CSS selector, then use that selector. Expensive but powerful for one-off scraping tasks.

### P2 -- Semantic Chunking Built-in

See section 6 above. This positions Nika as a complete RAG data pipeline tool.

---

## Priority Matrix

### P0 -- Must-Have (Ship Before v1.0)

| Enhancement | Effort | Impact |
|---|---|---|
| robots.txt compliance | 2-3 days | Ethical baseline, legal protection |
| Document LLM-as-scraper pattern | 1 day | Competitive messaging |
| Verify llms.txt spec compliance | 0.5 day | Correctness |

### P1 -- Competitive Advantage

| Enhancement | Effort | Impact |
|---|---|---|
| Cookie jar / session persistence | 2-3 days | Multi-step authenticated flows |
| Per-domain rate limiting | 1-2 days | Politeness, avoid bans |
| Conditional requests (ETag) | 2-3 days | Incremental workflows |
| Content deduplication (exact hash) | 1-2 days | Efficiency for crawl workflows |
| TLS fingerprint impersonation | 3-5 days | Cloudflare bypass |
| Schema.org deep extraction | 1-2 days | E-commerce workflows |
| Enhanced markdown (tables, code, images) | 2-3 days | Output quality |
| llms-full.txt auto-detection | 0.5 day | Better AI content access |
| nika:chunk builtin tool | 3-5 days | RAG pipeline support |

### P2 -- Nice-to-Have

| Enhancement | Effort | Impact |
|---|---|---|
| HTTP/2 fingerprint awareness | 3-5 days | Advanced evasion |
| Headless browser integration | 5-10 days | JS-heavy pages |
| SimHash near-dedup | 2-3 days | Scale efficiency |
| DNS caching (hickory-dns) | 0.5 day | Performance |
| zstd compression | 0.5 day | Performance |
| AI CSS selector generation | 3-5 days | UX innovation |
| fetch+infer shorthand syntax | 2-3 days | DX |
| Content density scoring | 3-5 days | Extraction quality |

---

## Implementation Roadmap

### Phase 1: Ethical Foundation (v0.63-v0.64)
1. robots.txt compliance with `texting_robots` crate
2. Per-domain rate limiting with `governor` crate
3. Document LLM-as-scraper pattern in showcase workflows

### Phase 2: Incremental Crawling (v0.65)
4. Conditional requests (ETag/If-Modified-Since)
5. Content deduplication (exact hash)
6. Cookie jar persistence across tasks

### Phase 3: Anti-Detection (v0.66-v0.67)
7. TLS fingerprint impersonation (feature-gated, `wreq` or `rquest`)
8. Realistic header profiles (Chrome/Firefox)
9. HTTP/2 fingerprint awareness

### Phase 4: Output Quality (v0.68)
10. Schema.org deep extraction
11. Enhanced markdown conversion
12. nika:chunk builtin for RAG

---

## Key Rust Crates to Evaluate

| Crate | Purpose | Notes |
|---|---|---|
| `wreq` | Browser TLS/HTTP/2 impersonation | Best option for anti-detection |
| `rquest` | reqwest fork with impersonation | Alternative to wreq |
| `texting_robots` | robots.txt parsing (Google's impl) | Production-ready |
| `governor` | Rate limiting (token bucket) | Production-ready |
| `hickory-dns` | Async DNS with caching | reqwest feature flag |
| `simhash` | Near-duplicate detection | For content dedup |
| `dom_smoothie` | Readability (already used) | Keep |
| `htmd` | HTML to Markdown (already used) | Keep, test edge cases |

---

## Sources

1. nstbrowser.io/blog/tls-fingerprinting -- TLS fingerprinting techniques
2. roundproxies.com/blog/what-is-tls-fingerprint -- TLS bypass in 2025
3. scrapehero.com/tls-fingerprint-bypass-techniques -- Detection vs evasion 2026
4. auth0.com/blog/strengthening-bot-detection-ja4-signals -- JA4 at Auth0
5. peakhour.io/learning/fingerprinting/ja3-vs-ja4 -- JA3 vs JA4 comparison
6. browserless.io/blog/tls-fingerprinting -- Playwright/Puppeteer bypass
7. engineering.doit.com -- JA3/JA4 in AWS WAF
8. proxies.sx/use-cases/privacy/tls-fingerprint -- JA4+ guide 2026
9. Trafilatura benchmarks via academic papers (referenced in Perplexity results)
10. Firecrawl documentation (firecrawl.dev)
11. llmstxt.org -- llms.txt specification
12. Nika source code: `tools/nika-engine/src/runtime/executor/fetch.rs` (1146 LOC)
13. Nika source code: `tools/nika-engine/src/runtime/executor/extract.rs` (1233 LOC)
14. Nika source code: `tools/nika-core/src/ast/extract.rs` (216 LOC)

---

## Confidence Level

**HIGH** for:
- TLS fingerprinting detection mechanisms (well-documented, multiple authoritative sources)
- Content extraction benchmarks (academic papers, reproducible)
- robots.txt and politeness requirements (established standards)
- Nika current state analysis (direct source code review)

**MEDIUM** for:
- Rust crate recommendations (wreq/rquest ecosystem is newer, less battle-tested)
- Effort estimates (depends on integration complexity with existing reqwest setup)
- Firecrawl/Crawl4AI architecture details (limited technical documentation)

**LOW** for:
- Exact browser header ordering for 2025+ versions (rapidly changing)
- HTTP/2 fingerprint specifics (poorly documented publicly)
- AI CSS selector generation (experimental, no production implementations found)
