# PR5 — Fetch v2.0 Master Plan (Revised)

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Transform Nika's `fetch:` verb from a bare HTTP client into the most advanced web content extraction pipeline in any workflow engine — surpassing Firecrawl for static content, with intelligent fallback to MCP for JS rendering.

**Architecture:** Three atomic PRs. Builtins first (composable with ANY content source), then fetch sugar on top. Each PR is independently shippable and reviewable.

**Research:** 8 parallel agents, ~75 Perplexity searches, 22 gaps identified, 60+ crates analyzed, 7 research reports produced.

**Baseline:** 6255 tests, 0 clippy warnings

---

## Strategic Insight

The plan is split into 3 PRs because:
- **PR5a** fixes real bugs (gzip, OPTIONS, no size limit) — ships in hours
- **PR5b** adds builtins that work on ANY content source (CAS, MCP, fetch) — the foundation
- **PR5c** adds fetch sugar (`extract:`) that calls builtins internally — the convenience layer

```
fetch: → response body → extract: markdown → calls nika:html_to_md internally
                                                        ↑
invoke: nika:html_to_md ← also works on CAS hashes from MCP results
```

If we only built fetch sugar, MCP-sourced content can't be extracted.
If we build builtins first, ALL content sources get extraction for free.

---

## Competitive Position After PR5

| Capability | Nika | Firecrawl | LangChain | n8n | Dify |
|------------|:----:|:---------:|:---------:|:---:|:----:|
| HTML → Markdown | **YES** | YES | plugin | plugin | no |
| Article extraction | **YES** | YES | no | no | no |
| CSS selectors | **YES** | no | no | no | no |
| Metadata (OG, JSON-LD) | **Native $0** | LLM-only | no | plugin | no |
| Rich link classification | **YES** | flat list | no | no | no |
| RSS/Atom feeds | **YES** | no | no | plugin | no |
| llm.txt discovery | **YES** | YES | no | no | no |
| JSONPath on response | **YES** | no | no | yes | no |
| Binary → CAS pipeline | **YES** | no | no | no | no |
| Status + headers | **YES** | YES | yes | yes | yes |
| CAS dedup | **YES** | no | no | no | no |
| Vision bridge (PR4) | **YES** | no | no | no | no |

---

## New Dependencies Summary

```toml
# PR5a: ZERO new crates — just reqwest feature flags
reqwest features += ["gzip", "brotli", "deflate"]

# PR5b: 3 crates (feature-gated, NOT in defaults)
scraper = { version = "0.26", optional = true, features = ["atomic"] }
htmd = { version = "0.5", optional = true }
dom_smoothie = { version = "0.16", optional = true }

# PR5c: 1 crate (feature-gated)
feed-rs = { version = "2.3", optional = true }

# Feature gates
fetch-html = ["dep:scraper"]                          # CSS select, text, links, metadata
fetch-markdown = ["dep:htmd"]                         # HTML → Markdown
fetch-article = ["dep:dom_smoothie", "dep:scraper"]   # Readability extraction
fetch-feed = ["dep:feed-rs"]                          # RSS/Atom parsing
fetch-extract = ["fetch-html", "fetch-markdown"]      # Common combo (scraper + htmd)
fetch-full = ["fetch-extract", "fetch-article", "fetch-feed"]  # Everything
```

---

## YAML Syntax (full vision)

```yaml
# ═══ Backward compatible (no changes) ═══
fetch: "https://api.example.com/data"
fetch:
  url: "https://api.example.com/data"
  method: POST
  headers: { Authorization: "Bearer {{env.TOKEN}}" }
  body: '{"query": "test"}'

# ═══ PR5a: Response modes ═══
fetch:
  url: "https://api.example.com/data"
  response: full    # → JSON: { status, headers, body, url, elapsed_ms }

fetch:
  url: "https://example.com/photo.jpg"
  response: binary  # → JSON: { hash, mime_type, size_bytes } (stored in CAS)

# ═══ PR5c: Extract modes (sugar for builtins) ═══
fetch:
  url: "https://blog.example.com/article"
  extract: markdown    # → clean Markdown (htmd)

fetch:
  url: "https://news.site.com/story"
  extract: article     # → main content only (dom_smoothie Readability)

fetch:
  url: "https://example.com/products"
  extract: text
  selector: "div.product-card h2"  # → text from matching elements

fetch:
  url: "https://example.com/page"
  extract: metadata    # → { title, og, twitter, json_ld, canonical, favicon, feeds }

fetch:
  url: "https://example.com"
  extract: links       # → { internal, external, resources, summary }

fetch:
  url: "https://blog.example.com/feed.xml"
  extract: feed        # → { title, entries: [{ title, url, published, summary }] }

fetch:
  url: "https://api.github.com/repos/owner/repo"
  extract: jsonpath
  selector: "$.stargazers_count"   # → "42"

fetch:
  url: "https://example.com"
  extract: llm_txt     # → { found, url, content } from /.well-known/llm.txt
```

---

# PR5a — Fetch Correctness (0 new deps, ship today)

## Task 0.0: Enable gzip/brotli/deflate

**Files:**
- Modify: `Cargo.toml:88`

**Change:**
```toml
# BEFORE
reqwest = { version = "0.12", default-features = false, features = ["json", "stream", "rustls-tls"] }

# AFTER
reqwest = { version = "0.12", default-features = false, features = ["json", "stream", "rustls-tls", "gzip", "brotli", "deflate"] }
```

**Why critical:** APIs like GitHub return compressed responses. Without these, `response.text()` produces garbled bytes.

**Test:** `cargo test --lib -q` — no behavioral change for uncompressed responses.
**Commit:** `fix(fetch): enable gzip/brotli/deflate decompression`

---

## Task 0.1: Fix OPTIONS method dispatch

**Files:**
- Modify: `src/runtime/executor/verbs.rs:912-924`

**Change:** Add between HEAD (line 921) and the default GET (line 923):

```rust
} else if fetch.method.eq_ignore_ascii_case("OPTIONS") {
    http_client.request(reqwest::Method::OPTIONS, url.as_ref())
```

**Test:** wiremock test that verifies OPTIONS request method.
**Commit:** `fix(fetch): add missing OPTIONS method dispatch`

---

## Task 0.2: Add 50MB response size limit

**Files:**
- Modify: `src/runtime/executor/verbs.rs:1021`

**Change:**
```rust
// Replace: return response.text().await.map_err(...)
// With:
const MAX_RESPONSE_SIZE: u64 = 50 * 1024 * 1024;
if let Some(len) = response.content_length() {
    if len > MAX_RESPONSE_SIZE {
        return Err(NikaError::Execution(format!(
            "Response too large ({} bytes, max {} bytes)",
            len, MAX_RESPONSE_SIZE
        )));
    }
}
let raw_body = response.text().await.map_err(|e| {
    NikaError::Execution(format!("Failed to read response: {}", e))
})?;
if raw_body.len() as u64 > MAX_RESPONSE_SIZE {
    return Err(NikaError::Execution(format!(
        "Response body too large ({} bytes, max {} bytes)",
        raw_body.len(), MAX_RESPONSE_SIZE
    )));
}
```

**Test:** wiremock test with oversized response.
**Commit:** `fix(fetch): add 50MB response size limit to prevent OOM`

---

## Task 0.3: Add `response: full` mode

**Files:**
- Modify: `src/ast/raw/action.rs:103-126` — add `response: Option<Spanned<String>>`
- Modify: `src/ast/raw/parser.rs:644-679` — parse `response` field
- Modify: `src/ast/analyzed/task.rs:178-204` — add to AnalyzedFetchAction
- Modify: `src/ast/analyzer/analyze.rs:711-741` — map raw→analyzed
- Modify: `src/ast/lower.rs:198-210` — lower to FetchParams
- Modify: `src/ast/action.rs:338-404` — add to FetchParams, validate (full|binary|None)
- Modify: `src/runtime/executor/verbs.rs:1021` — response mode branching

**When `response: full`**, return JSON instead of raw body:
```json
{
  "status": 200,
  "headers": { "content-type": "text/html", "x-ratelimit-remaining": "42" },
  "body": "...",
  "url": "https://final-url-after-redirects.com",
  "elapsed_ms": 234
}
```

**Tests:**
- Parse workflow with `response: full`
- wiremock test: verify JSON structure
- wiremock test: verify redirect exposes final URL
- wiremock test: verify headers are present

**Commit:** `feat(fetch): add response: full mode exposing status + headers`

---

## Task 0.4: Add `response: binary` mode (CAS integration)

**Files:**
- Modify: `src/runtime/executor/verbs.rs` — binary response branch

**When `response: binary`:**
```rust
if fetch.response.as_deref() == Some("binary") {
    let bytes = response.bytes().await.map_err(|e| {
        NikaError::Execution(format!("Failed to read binary response: {}", e))
    })?;
    let store_result = self.cas.store(&bytes).await.map_err(|e| {
        NikaError::Execution(format!("CAS store failed: {}", e))
    })?;
    let content_type = response.headers()
        .get("content-type")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("application/octet-stream");
    return Ok(serde_json::json!({
        "hash": store_result.hash,
        "mime_type": content_type,
        "size_bytes": bytes.len(),
        "deduplicated": store_result.deduplicated,
    }).to_string());
}
```

This bridges fetch → CAS → media pipeline (nika:thumbnail, nika:pdf_extract, vision content:).

**Tests:**
- wiremock serving PNG → verify CAS hash returned
- wiremock serving PDF → verify mime_type correct

**Commit:** `feat(fetch): add response: binary mode with CAS store integration`

---

## Task 0.5: Add HttpRequest + HttpResponse events

**Files:**
- Modify: `src/event/log.rs` — add 2 new EventKind variants
- Modify: `src/runtime/executor/verbs.rs` — emit events
- Modify: `src/tui/state/mod.rs` — handle new events (display in TUI)

```rust
/// HTTP request initiated by fetch: verb
HttpRequest {
    task_id: Arc<str>,
    method: String,
    url: String,
    has_body: bool,
},

/// HTTP response received by fetch: verb
HttpResponse {
    task_id: Arc<str>,
    status_code: u16,
    content_type: Option<String>,
    content_length: Option<u64>,
    elapsed_ms: u64,
},
```

Emit `HttpRequest` before `.send()`, `HttpResponse` after response received (before body read).

**Commit:** `feat(event): add HttpRequest + HttpResponse telemetry events`

---

## Task 0.6: wiremock test suite (20+ tests)

**Files:**
- Modify: `src/runtime/executor/tests.rs` — replace the 2 existing fetch tests with 20+

**Test list:**
```rust
// HTTP methods
fetch_get_returns_body
fetch_post_sends_body
fetch_put_sends_body
fetch_delete_works
fetch_head_returns_empty
fetch_options_sends_correct_method  // verifies fix from Task 0.1
fetch_patch_works

// Compression
fetch_gzip_response_decoded         // verifies fix from Task 0.0
fetch_brotli_response_decoded

// Response modes
fetch_response_full_includes_status_and_headers
fetch_response_full_includes_redirect_url
fetch_response_binary_stores_in_cas

// Error handling
fetch_response_too_large_rejected   // verifies fix from Task 0.2
fetch_timeout_returns_error
fetch_invalid_url_returns_error
fetch_server_error_retried
fetch_retry_exhaustion_returns_error

// Headers and body
fetch_custom_headers_sent
fetch_json_body_sets_content_type
fetch_template_in_url_resolved
fetch_template_in_headers_resolved
```

**Commit:** `test(fetch): add 20+ wiremock-based fetch tests`

---

## PR5a Verification

```bash
cargo test --lib -q                    # 6255 + ~20 = 6275 passed
cargo clippy -- -D warnings            # 0 warnings
cargo test --bin nika -q               # CLI tests pass
```

---

# PR5b — Web Extraction Builtins (3 new crates)

## Task 1.0: Add dependencies + feature gates

**Files:**
- Modify: `Cargo.toml`

```toml
# Dependencies
scraper = { version = "0.26", optional = true, features = ["atomic"] }
htmd = { version = "0.5", optional = true }
dom_smoothie = { version = "0.16", optional = true }

# Features
fetch-html = ["dep:scraper"]
fetch-markdown = ["dep:htmd"]
fetch-article = ["dep:dom_smoothie", "dep:scraper"]
fetch-extract = ["fetch-html", "fetch-markdown"]
```

**Verify:**
```bash
cargo check --features fetch-html
cargo check --features fetch-markdown
cargo check --features fetch-extract
cargo check  # default features still work
```

**Commit:** `chore(deps): add scraper, htmd, dom_smoothie for web extraction`

---

## Task 1.1: nika:html_to_md — HTML to Markdown

**Files:**
- Create: `src/runtime/builtin/media/html_to_md.rs`
- Modify: `src/runtime/builtin/media/mod.rs` — register behind `fetch-markdown`

**Pattern:** Same as MediaOp trait (import.rs, phash.rs).

```rust
pub struct HtmlToMdOp;

impl MediaOp for HtmlToMdOp {
    fn name(&self) -> &'static str { "html_to_md" }
    fn description(&self) -> &'static str {
        "Convert HTML content to clean Markdown for LLM consumption"
    }
    // Input: { hash: "blake3:..." } OR { html: "<html>..." }
    // Output: Metadata { markdown: "# Title\n\n...", char_count: 1234 }
}
```

Accept BOTH CAS hash (reads from store) and raw HTML string (inline).
Uses `htmd::HtmlToMarkdown::new().convert(&html)`.

**Tests (8+):**
- Convert simple HTML → verify Markdown headings
- Convert HTML with tables → verify GFM table
- Convert HTML with code blocks → verify fenced code
- Convert HTML with links → verify Markdown links
- Convert from CAS hash (store HTML first)
- Missing hash → error
- Empty HTML → empty Markdown
- Cancelled workflow → error

**Commit:** `feat(media): add nika:html_to_md — HTML to Markdown [fetch-markdown]`

---

## Task 1.2: nika:css_select — CSS selector extraction

**Files:**
- Create: `src/runtime/builtin/media/css_select.rs`
- Modify: `src/runtime/builtin/media/mod.rs` — register behind `fetch-html`

```rust
pub struct CssSelectOp;
// Input: { hash: "blake3:...", selector: "div.product h2", output: "text"|"html" }
// Output: Metadata { matches: ["Product 1", "Product 2"], count: 2 }
```

Uses `scraper::Html::parse_document` + `scraper::Selector::parse`.
Output mode: `text` (default) = `.text().collect()`, `html` = `.html()`.

**Tests (8+):**
- Select by tag name
- Select by class
- Select by ID
- Select by attribute
- Nested selectors (div > p > a)
- Output: text mode
- Output: html mode
- Invalid CSS selector → error
- No matches → empty array

**Commit:** `feat(media): add nika:css_select — CSS selector extraction [fetch-html]`

---

## Task 1.3: nika:extract_metadata — OG, Twitter, JSON-LD, SEO

**Files:**
- Create: `src/runtime/builtin/media/extract_metadata.rs`
- Modify: `src/runtime/builtin/media/mod.rs` — register behind `fetch-html`

```rust
pub struct ExtractMetadataOp;
// Input: { hash: "blake3:..." } OR { html: "..." }
// Output: Metadata {
//   title, description, canonical, favicon, language, robots, author, published,
//   og: { title, description, image, url, type, site_name },
//   twitter: { card, title, description, image, site, creator },
//   json_ld: [ { @type, headline, ... } ],
//   feeds: [ { type: "rss", url, title } ],
//   hreflang: { en: "/en/page", fr: "/fr/page" }
// }
```

Extracts ALL metadata from `<meta>`, `<title>`, `<link>`, `<script type="application/ld+json">`.
This is the "$0 Diffbot" — deterministic, instant, zero tokens.

**Tests (10+):**
- Extract `<title>`
- Extract meta description
- Extract OG tags (all 6)
- Extract Twitter cards (all 6)
- Extract JSON-LD
- Extract canonical URL
- Extract favicon
- Extract RSS feed discovery
- Extract hreflang
- Page with no metadata → empty fields (not error)

**Commit:** `feat(media): add nika:extract_metadata — OG, Twitter, JSON-LD, SEO [fetch-html]`

---

## Task 1.4: nika:extract_links — Rich link classification

**Files:**
- Create: `src/runtime/builtin/media/extract_links.rs`
- Modify: `src/runtime/builtin/media/mod.rs` — register behind `fetch-html`
- Modify: `Cargo.toml` — add `psl = "2"` as optional dep in `fetch-html`

```rust
pub struct ExtractLinksOp;
// Input: { hash: "blake3:...", base_url: "https://example.com" }
// Optional: { selector: "article" } → only links within selector
// Output: Metadata {
//   internal: [ { url, anchor, rel, context, parent_tag, dofollow } ],
//   external: [ { url, anchor, rel, context, parent_tag, dofollow } ],
//   resources: [ { url, resource_type, tag } ],
//   summary: { total, internal_count, external_count, dofollow_count, nofollow_count, contexts }
// }
```

**Link context classification** via DOM ancestor walking:
```rust
fn classify_context(element: &ElementRef) -> LinkContext {
    for ancestor in element.ancestors() {
        if let Some(el) = ancestor.value().as_element() {
            match el.name() {
                "nav" => return Navigation,
                "header" => return Header,
                "footer" => return Footer,
                "aside" => return Sidebar,
                "main" | "article" | "section" | "p" => return Content,
                _ => {
                    // Class-name fallback for non-semantic HTML
                    let classes = el.attr("class").unwrap_or("");
                    if classes.contains("nav") || classes.contains("menu") { return Navigation; }
                    if classes.contains("footer") { return Footer; }
                    if classes.contains("sidebar") { return Sidebar; }
                    if classes.contains("content") || classes.contains("article") { return Content; }
                }
            }
        }
    }
    Unknown
}
```

**Internal vs external** via `psl` crate (eTLD+1 comparison):
- `example.com/about` = internal
- `blog.example.com/post` = internal (same eTLD+1)
- `other-site.com/page` = external

**Tests (10+):**
- Internal links detected (relative + absolute)
- External links detected
- Subdomain = internal
- nofollow/ugc/sponsored classification
- Context: nav links
- Context: content links
- Context: footer links
- Selector filtering (only links in `article`)
- Social link detection (twitter, github patterns)
- Empty page → empty arrays

**Commit:** `feat(media): add nika:extract_links — rich SEO link classification [fetch-html]`

---

## Task 1.5: nika:readability — Article extraction

**Files:**
- Create: `src/runtime/builtin/media/readability.rs`
- Modify: `src/runtime/builtin/media/mod.rs` — register behind `fetch-article`

```rust
pub struct ReadabilityOp;
// Input: { hash: "blake3:..." } OR { html: "..." }
// Output: Metadata { title, content, text_content, excerpt, byline, char_count }
```

Uses `dom_smoothie::Readability::new(html, Some(url), None)?.parse()`.
`content` = clean HTML, `text_content` = plain text.

**Tests (8+):**
- Article with title + content extracted
- Navigation/footer stripped
- Byline extracted
- Non-article page → graceful result or error
- Empty HTML
- CAS hash input
- Cancelled

**Commit:** `feat(media): add nika:readability — article extraction [fetch-article]`

---

## PR5b Verification

```bash
cargo test --lib -q                                          # No regression
cargo test --lib --features fetch-extract -q                 # html_to_md + css_select + metadata + links
cargo test --lib --features fetch-article -q                 # readability
cargo clippy --features fetch-extract -- -D warnings         # 0 warnings
cargo clippy --features fetch-article -- -D warnings
cargo check --no-default-features --features fetch-html      # Isolated
cargo check --no-default-features --features fetch-markdown  # Isolated
cargo check --no-default-features --features fetch-article   # Isolated
```

---

# PR5c — Fetch Extract Sugar + Extras (builds on PR5b)

## Task 2.0: Add extract + selector to fetch AST pipeline

**Files:**
- Modify: `src/ast/raw/action.rs:103-126` — add `extract`, `selector` to RawFetchAction
- Modify: `src/ast/raw/parser.rs:644-679` — parse both fields
- Modify: `src/ast/analyzed/task.rs:178-204` — add to AnalyzedFetchAction
- Modify: `src/ast/analyzer/analyze.rs:711-741` — map raw→analyzed
- Modify: `src/ast/lower.rs:198-210` — lower to FetchParams
- Modify: `src/ast/action.rs:338-404` — add to FetchParams + validate()

**Validate extract values:** `markdown`, `article`, `text`, `selector`, `metadata`, `links`, `feed`, `jsonpath`, `llm_txt`.
**selector requires extract to be set.**

**Tests:** Parser tests + validation tests (10+).
**Commit:** `feat(ast): add extract + selector fields to fetch verb`

---

## Task 2.1: Create extraction engine

**Files:**
- Create: `src/runtime/executor/extract.rs`
- Modify: `src/runtime/executor/mod.rs` — register module

```rust
/// Apply post-processing extraction to a fetch response body.
pub fn apply_extract(
    body: &str,
    extract: Option<&str>,
    selector: Option<&str>,
    url: &str,           // for llm_txt and link classification
    http_client: &reqwest::Client,  // for llm_txt sub-requests
) -> Result<String, NikaError>
```

**Mode routing:**
- `None` → return body as-is (backward compatible)
- `markdown` → call `nika:html_to_md` logic (htmd)
- `article` → call `nika:readability` logic (dom_smoothie)
- `text` → call `nika:css_select` logic with text output
- `selector` → call `nika:css_select` logic with html output
- `metadata` → call `nika:extract_metadata` logic
- `links` → call `nika:extract_links` logic
- `feed` → `feed_rs::parser::parse()` → JSON
- `jsonpath` → `serde_json_path` (already a dep!) → extracted value
- `llm_txt` → fetch `{origin}/.well-known/llm.txt`, `/llm.txt`, `/llms.txt`

**Key insight:** The extract engine REUSES the builtin tool logic from PR5b.
No duplication — extraction functions are shared between builtins and fetch sugar.

**Commit:** `feat(fetch): create extraction engine with 9 modes`

---

## Task 2.2: Wire extraction into run_fetch

**Files:**
- Modify: `src/runtime/executor/verbs.rs:1021`

```rust
// After getting raw_body (and after response: full/binary checks):
match fetch.extract.as_deref() {
    Some(_) => extract::apply_extract(
        &raw_body,
        fetch.extract.as_deref(),
        fetch.selector.as_deref(),
        &url,
        &http_client,
    ),
    None => Ok(raw_body),
}
```

**Commit:** `feat(runtime): wire extraction engine into fetch verb`

---

## Task 2.3: extract: feed (feed-rs)

**Files:**
- Modify: `Cargo.toml` — add `feed-rs = { version = "2.3", optional = true }`
- Modify: feature `fetch-feed = ["dep:feed-rs"]`
- Add feed parsing in `extract.rs`

```rust
#[cfg(feature = "fetch-feed")]
fn extract_feed(body: &str) -> Result<String, NikaError> {
    let feed = feed_rs::parser::parse(body.as_bytes())
        .map_err(|e| NikaError::Execution(format!("Feed parse failed: {e}")))?;
    let entries: Vec<_> = feed.entries.iter().map(|e| serde_json::json!({
        "title": e.title.as_ref().map(|t| &t.content),
        "url": e.links.first().map(|l| &l.href),
        "published": e.published.map(|d| d.to_rfc3339()),
        "summary": e.summary.as_ref().map(|s| &s.content),
    })).collect();
    Ok(serde_json::json!({
        "title": feed.title.map(|t| t.content),
        "description": feed.description.map(|d| d.content),
        "entries": entries,
        "entry_count": feed.entries.len(),
    }).to_string())
}
```

**Commit:** `feat(fetch): extract: feed — RSS/Atom/JSON Feed parsing [fetch-feed]`

---

## Task 2.4: extract: jsonpath (zero-cost — already a dep!)

```rust
fn extract_jsonpath(body: &str, path: &str) -> Result<String, NikaError> {
    let json: serde_json::Value = serde_json::from_str(body)
        .map_err(|e| NikaError::Execution(format!("Response is not valid JSON: {e}")))?;
    let jsonpath = serde_json_path::JsonPath::parse(path)
        .map_err(|e| NikaError::Execution(format!("Invalid JSONPath '{}': {e}", path)))?;
    let results: Vec<&serde_json::Value> = jsonpath.query(&json).all();
    if results.len() == 1 {
        Ok(serde_json::to_string(results[0])?)
    } else {
        Ok(serde_json::to_string(&results)?)
    }
}
```

**No new deps!** `serde_json_path` is already in Cargo.toml.

**Commit:** `feat(fetch): extract: jsonpath — query JSON API responses [zero deps]`

---

## Task 2.5: extract: llm_txt — AI-era content discovery

```rust
async fn extract_llm_txt(url: &str, client: &reqwest::Client) -> Result<String, NikaError> {
    let parsed = url::Url::parse(url)
        .map_err(|e| NikaError::Execution(format!("Invalid URL: {e}")))?;
    let origin = parsed.origin().unicode_serialization();

    for path in &["/.well-known/llm.txt", "/llm.txt", "/llms.txt", "/llms-full.txt"] {
        let llm_url = format!("{}{}", origin, path);
        match client.get(&llm_url).send().await {
            Ok(resp) if resp.status().is_success() => {
                if let Ok(body) = resp.text().await {
                    if !body.trim().is_empty() {
                        return Ok(serde_json::json!({
                            "found": true,
                            "url": llm_url,
                            "path": path,
                            "content": body,
                            "size_bytes": body.len(),
                        }).to_string());
                    }
                }
            }
            _ => continue,
        }
    }
    Ok(serde_json::json!({ "found": false }).to_string())
}
```

**Commit:** `feat(fetch): extract: llm_txt — AI-era content discovery`

---

## Task 2.6: Documentation + examples

- CLAUDE.md update: fetch syntax, extract modes, 24+ tools, feature flags
- Example: `examples/fetch-extract.nika.yaml` (web → markdown → summarize)
- Example: `examples/seo-audit.nika.yaml` (metadata + links analysis)
- Example: `examples/feed-monitor.nika.yaml` (RSS → LLM enrichment)

**Commit:** `docs(fetch): update CLAUDE.md + add 3 example workflows`

---

## PR5c Verification

```bash
cargo test --lib -q                                          # No regression
cargo test --lib --features fetch-extract -q                 # Extract modes
cargo test --lib --features "fetch-extract,fetch-feed" -q    # Feed mode
cargo clippy --features "fetch-extract,fetch-feed" -- -D warnings
cargo check --no-default-features --features fetch-feed      # Isolated
```

---

# Deferred to PR6+ (YAGNI for now)

| Feature | Reason to defer |
|---------|----------------|
| lol_html streaming | Premature optimization — scraper works fine |
| sonic-rs SIMD JSON | serde_json fast enough for fetch responses |
| aho-corasick entity extraction | Separate feature, not core fetch |
| rquest TLS fingerprinting | Anti-bot = use Firecrawl MCP |
| governor rate limiting | Fetch already has retry/backoff |
| texting_robots / robots.txt | Polite but not v1 |
| HTTP caching (ETag, 304) | Nice but not critical path |
| Cookie jar | Complex, rarely needed |
| Proxy support | Niche use case |
| HTTP/2 | No visible benefit for workflow HTTP |
| Sitemap parsing | Future `crawl:` verb |
| TUI FetchBox wiring | Cosmetic, not functional |
| whichlang language detection | Nice-to-have enrichment |

---

# Success Criteria

- [ ] Zero regression — all 6255 existing tests pass
- [ ] `fetch:` with no extract/response works exactly as before
- [ ] gzip/brotli/deflate responses decoded
- [ ] OPTIONS method dispatches correctly
- [ ] Responses > 50MB rejected
- [ ] `response: full` returns JSON with status + headers
- [ ] `response: binary` stores in CAS with hash
- [ ] `nika:html_to_md` converts HTML to clean Markdown
- [ ] `nika:css_select` extracts by CSS selectors
- [ ] `nika:readability` extracts article content
- [ ] `nika:extract_metadata` returns OG + Twitter + JSON-LD deterministically
- [ ] `nika:extract_links` returns rich classified links
- [ ] `extract: feed` parses RSS/Atom
- [ ] `extract: jsonpath` queries JSON responses (zero new deps)
- [ ] `extract: llm_txt` discovers AI-optimized content
- [ ] HttpRequest + HttpResponse events emitted
- [ ] 55+ new tests across 3 PRs
- [ ] 0 clippy warnings on all feature combos
- [ ] All feature gates compile independently

---

# The Positioning Statement

> "Nika is the only workflow engine where you fetch a page, extract its article as Markdown,
> pull structured metadata, classify every link by SEO context, check its llm.txt,
> and feed everything to an LLM — all in one YAML task, running locally, at $0 per extraction,
> with content-addressable dedup and a vision pipeline for screenshots."
