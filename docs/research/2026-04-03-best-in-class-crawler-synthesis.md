# Nika Crawler: Road to Best-in-Class

> Synthesis of 4 parallel research agents | 2026-04-03
> Sources: Perplexity, Firecrawl docs, spider-rs source, crates.io, GitHub

---

## TL;DR — What Nika Already Has vs What's Missing

```
NIKA TODAY (fetch: verb)                    MISSING FOR BEST-IN-CLASS
──────────────────────────────              ──────────────────────────
10 extract modes                            robots.txt compliance
response:full + redirect chain              per-domain rate limiting
SSRF protection + DNS pinning               TLS fingerprint emulation
url_normalize transform                     conditional requests (ETag/304)
sitemap parsing                             cookie jar / session persistence
hreflang resolution                         Schema.org / JSON-LD extraction
max_stdout protection                       headless browser (via exec:)
5-layer structured output                   content dedup (simhash)
7 LLM providers                             chunking for RAG
for_each concurrency control                enhanced markdown (htmd quality)
```

**Killer advantage already in place**: `fetch: { extract: markdown }` → `infer:` → `structured:` is more powerful than Firecrawl's LLM Extract because the user controls model, schema, retry, and multi-step orchestration. No competitor can do this.

---

## Competitive Landscape (Winner by Category)

| Category | Winner | Nika Position |
|----------|--------|---------------|
| Best DX / AI integration | **Firecrawl** (MCP, one-call markdown) | fetch: + extract already close |
| Raw performance | **spider.cloud** (Rust, 20K+ pages/sec) | Same language, can match |
| Battle-tested at scale | **Crawlee/Apify** (decade of experience) | Newer, needs hardening |
| Zero-friction single page | **Jina Reader** (`r.jina.ai/URL`) | nika fetch URL --extract md |
| Best for RAG pipelines | **Crawl4AI** (chunking, similarity) | Missing: chunking builtin |
| Anti-bot bypass | **Bright Data** (72M+ residential IPs) | Out of scope — delegate |
| AI browser automation | **Stagehand** (NL browser control) | agent: verb could integrate |
| **Orchestration** | **Nobody** | **NIKA WINS** — unique DAG |

### Firecrawl Deep Dive (the competitor to beat)

**Architecture**: TypeScript monolith (Express + BullMQ + Redis + PostgreSQL + RabbitMQ). 74 npm deps, Go HTML→MD microservice, Rust NAPI module (lol_html). Docker needs 12GB+ RAM.

**Secret weapon**: "Fire Engine" — proprietary browser cloud with stealth proxies, NOT available to self-hosted. Self-hosted Firecrawl = basic Playwright only.

**Engine waterfall** (7 engines, tried in order):
1. index (cached) → 2. wikipedia API → 3. Chrome CDP → 4. Playwright → 5. TLS client → 6. basic fetch → 7. stealth proxies

**8 API endpoints**: /scrape, /crawl, /map, /search, /batch/scrape, /extract, /agent, /interact

**Pricing**: $0.00083/page at Standard tier. JSON extraction +4 credits/page.

**Weaknesses** (our opportunities):
- Node.js performance ceiling (GC pauses, 8GB+ RAM)
- Self-hosted is crippled (no Fire Engine, no anti-bot, no cache)
- Operational complexity (Redis + RabbitMQ + PostgreSQL)
- Three-language codebase (TS + Go + Rust NAPI)
- No offline mode

---

## Recommended Rust Crate Stack

### Tier 1: Essential (already using or should add)

| Category | Crate | Status in Nika | Action |
|----------|-------|----------------|--------|
| HTTP | **reqwest** 0.13 | Already using | Enable `hickory-dns` feature |
| HTML parsing | **scraper** 0.26 | Already using | Keep |
| Streaming HTML | **lol_html** 2.7 | NOT using | **ADD** — Cloudflare prod, streaming |
| XML/Sitemap | **quick-xml** 0.37 | Already using | Keep |
| URL | **url** 2.5 | Already using | Keep |
| Markdown | **htmd** 0.5 | NOT using | **EVALUATE** vs current readability |
| Rate limiting | **governor** 0.10 | NOT using | **ADD** — per-domain keyed limiter |
| Robots.txt | **texting_robots** 0.2 | NOT using | **ADD** — RFC 9309 compliant |
| URL dedup | **fastbloom** 0.17 | NOT using | **ADD** — bloom filter for crawls |
| DNS | hickory-dns (reqwest feature) | NOT enabled | **ENABLE** — async DNS + cache |

### Tier 2: Competitive Advantage

| Category | Crate | Priority | Notes |
|----------|-------|----------|-------|
| Browser | **chromiumoxide** 0.9 | P2 | Best async CDP, for JS rendering |
| TLS emulation | **wreq** (BoringSSL) | P1 | JA3/JA4 fingerprint spoofing |
| Content dedup | **simhash** 0.3 | P2 | Near-duplicate detection |
| PDF | **pdf-extract** 0.10 | P2 | Text from PDFs |
| Retry | **backoff** 0.4 | P1 | Already have retry, verify patterns |
| HTTP cache | **http-cache-reqwest** | P1 | RFC 7234 conditional requests |

### Tier 3: Avoid

| Crate | Why |
|-------|-----|
| select 0.6 | Stale, use scraper |
| kuchiki 0.8 | Unmaintained since 2019 |
| soup 0.5 | Abandoned |
| voyager 0.2 | Abandoned |
| tl 0.7 | Not spec-compliant |

---

## Feature Roadmap (Priority-Ordered)

### P0 — Must-Have (ethical/legal baseline + biggest differentiator)

| # | Feature | Effort | Crate/Approach |
|---|---------|--------|----------------|
| 1 | **robots.txt compliance** | 1 day | `texting_robots` — cache per domain, check before fetch |
| 2 | **Document fetch→infer→structured pattern** | 0.5 day | Docs + showcase workflow — THIS IS THE KILLER FEATURE |
| 3 | **llms.txt spec compliance** | 0.5 day | Verify extract:llm_txt matches llmstxt.org, add llms-full.txt auto-detect |

### P1 — Competitive Advantage (match Firecrawl, beat spider-rs)

| # | Feature | Effort | Crate/Approach |
|---|---------|--------|----------------|
| 4 | **Per-domain rate limiting** | 1 day | `governor` keyed limiter, `nika.toml`: `[fetch] rate_limit_per_domain = 2` |
| 5 | **Conditional requests (ETag/304)** | 2 days | Store ETag/Last-Modified in `.nika/cache/`, send If-Modified-Since, return cached on 304 |
| 6 | **TLS fingerprint emulation** | 2 days | `wreq` crate (BoringSSL), feature-gated `fetch-emulate`, YAML: `emulate: chrome` |
| 7 | **Cookie jar / session persistence** | 1 day | reqwest CookieStore shared across tasks in same workflow |
| 8 | **Schema.org / JSON-LD extraction** | 1 day | New extract mode or enhance extract:metadata to deep-parse JSON-LD |
| 9 | **Enhanced markdown via htmd** | 1 day | Evaluate htmd vs current markdown conversion, tables + code blocks |
| 10 | **nika:chunk builtin tool** | 1 day | Recursive character splitting, heading-aware, configurable size + overlap |
| 11 | **DNS caching** | 0.5 day | Enable `hickory-dns` feature in reqwest Cargo.toml |
| 12 | **URL dedup in for_each** | 1 day | `fastbloom` bloom filter, auto-dedup URLs in crawler workflows |
| 13 | **zstd compression** | 0 effort | Enable `reqwest` `zstd` feature flag (one-line change) |

### P2 — Nice-to-Have (differentiation / future-proofing)

| # | Feature | Effort | Notes |
|---|---------|--------|-------|
| 14 | Headless browser via exec: + Playwright | 0 (docs) | Recommend pattern, don't build into engine |
| 15 | SimHash near-duplicate detection | 2 days | Content-level dedup for large crawls |
| 16 | fetch→infer shorthand syntax | 1 day | Sugar for the two-step pattern |
| 17 | AI-powered CSS selector generation | 1 day | Send HTML to LLM, get selector back |
| 18 | Screenshot capture | 2 days | Via chromiumoxide or external Playwright |
| 19 | Content density pre-filter | 1 day | Score paragraphs before Readability |
| 20 | HTTP/2 fingerprint awareness | 3 days | SETTINGS frame emulation (advanced) |
| 21 | Change tracking | 2 days | Diff between crawl runs (like Firecrawl) |

---

## Architecture: Why Nika Wins

```
┌─────────────────────────────────────────────────────────┐
│                    FIRECRAWL                              │
│  TS Monolith → Redis → BullMQ → Workers → Playwright    │
│  74 deps, 12GB RAM, 3 languages, Fire Engine (proprietary)│
│  ONE thing: scrape → markdown/JSON                        │
└─────────────────────────────────────────────────────────┘

┌─────────────────────────────────────────────────────────┐
│                    NIKA                                    │
│  Single Rust binary, 0 runtime deps                       │
│  fetch: → extract: → infer: → structured: → artifact:    │
│  DAG orchestration, 7 providers, multi-step workflows     │
│  COMPOSE Spider + Firecrawl + Bright Data in one workflow │
└─────────────────────────────────────────────────────────┘
```

**Key insight**: Firecrawl = one tool. Nika = the orchestrator that CAN USE Firecrawl (via fetch: to their API) AND Spider AND Bright Data AND your own LLM — all in one YAML workflow.

The fetch: verb doesn't need to beat Firecrawl at everything. It needs to:
1. Handle 80% of cases natively (already does)
2. Delegate the hard 20% (anti-bot, JS rendering) to specialized services
3. Orchestrate everything in a DAG that no competitor can match

---

## Anti-Detection: The Real Story

| Technique | Effectiveness | Nika Approach |
|-----------|--------------|---------------|
| User-Agent rotation | Low (easily detected) | Already possible in YAML headers |
| TLS fingerprint (JA3/JA4) | **Critical** — Cloudflare's primary detection | `wreq` crate (P1) or delegate to proxy service |
| HTTP/2 fingerprint | High — SETTINGS frame analysis | Advanced, P2 |
| Browser rendering | Bypasses everything | exec: + Playwright or chromiumoxide |
| Residential proxies | Nuclear option | Delegate to Bright Data / Oxylabs |

**Cloudflare's detection chain**: JA4 fingerprint → HTTP/2 SETTINGS → header order → behavioral analysis. Default `reqwest` + `rustls` gets caught at step 1. The `wreq` crate (BoringSSL) solves this by replicating exact browser ClientHello.

---

## Content Extraction Quality (Benchmarks)

| Algorithm | F1 Score | Precision | Recall | Notes |
|-----------|----------|-----------|--------|-------|
| Trafilatura | **87.1%** | 86.5% | 87.7% | Python only, state of the art |
| dom_smoothie (Readability) | 84.4% | **92.8%** | 77.5% | **Highest precision**, best consistency |
| Newspaper3k | 79.2% | 82.1% | 76.5% | Python, aging |
| BoilerPipe | 73.8% | 78.2% | 69.9% | Java, obsolete |

Nika's Readability (via scraper) = **highest precision** = fewer false positives. Best for a workflow engine where you want CORRECT content, not maximum content. Right choice.

---

## Quick Wins (< 1 hour each)

| # | Change | File | Impact |
|---|--------|------|--------|
| 1 | Enable `hickory-dns` in reqwest features | Cargo.toml | Async DNS + caching |
| 2 | Enable `zstd` in reqwest features | Cargo.toml | zstd compression support |
| 3 | Add `texting_robots` to Cargo.toml | Cargo.toml | robots.txt parsing ready |
| 4 | Add `governor` to Cargo.toml | Cargo.toml | Per-domain rate limiting ready |
| 5 | Add `fastbloom` to Cargo.toml | Cargo.toml | URL dedup bloom filter ready |

---

## Showcase Workflows to Write

1. **sitemap-crawler.nika.yaml** — Full site crawl via sitemap, url_normalize dedup, metadata extraction, SEO report (already drafted in v2)
2. **firecrawl-enhanced.nika.yaml** — Use Firecrawl API for scraping + Nika structured output for extraction (demonstrates orchestration advantage)
3. **multi-source-research.nika.yaml** — Spider for speed + Firecrawl for quality + LLM synthesis (the "use all tools" pattern)
4. **incremental-monitor.nika.yaml** — Conditional requests, change detection, alert on changes
5. **rag-pipeline.nika.yaml** — Crawl → extract → chunk → embed → store (the RAG dream)

---

## Sources

- Agent 1: Competitive analysis (Firecrawl, Crawlee, Spider, Jina, Crawl4AI, Stagehand, Bright Data, Oxylabs)
- Agent 2: 50+ Rust crates across 16 categories (reqwest, scraper, lol_html, chromiumoxide, governor, fastbloom, htmd, texting_robots...)
- Agent 3: Advanced techniques (TLS fingerprinting, Mercator architecture, content extraction benchmarks, incremental crawling)
- Agent 4: Firecrawl deep dive (7-engine waterfall, Fire Engine proprietary moat, 8 API endpoints, pricing analysis)
