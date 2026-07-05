# Stdlib v0.1 · Extract modes

> The canonical 9 extraction modes used with the `nika:fetch` builtin
> (invoked via `invoke:` per D-2026-05-22-N18 · 4 verbs canonical · fetch
> is an invoke-able tool, not a verb). Each mode transforms an HTTP
> response into a useful representation for downstream processing (LLM
> input · structured data · index entries · etc.).

---

## The 9 canonical modes

| Mode | Output type | Use case |
|---|---|---|
| `markdown` | string (Markdown) | LLM input · article content stripped of nav/ads |
| `article` | string (Markdown · Readability) | News/blog articles · readability extraction |
| `text` | string (plain) | Stripped of all HTML · headers/footers preserved |
| `selector` | string (raw HTML) | Specific element via CSS selector |
| `jq` | JSON value | API responses · structured data via a jq expression (the one data language) |
| `metadata` | object | `<meta>` tags · OpenGraph · Twitter cards |
| `links` | array of strings | All `<a href>` outbound links |
| `feed` | object (parsed feed) | RSS · Atom · JSON Feed |
| `sitemap` | array of URLs | sitemap.xml · sitemap index |

Plus an implicit ·

| Mode | Output | Use case |
|---|---|---|
| `raw` | string (text · UTF-8) | Raw response body · no extraction · **text only** (a non-UTF-8 body is `NIKA-BUILTIN-FETCH-001` · binary is file-mediated per [04 §value rendering](../spec/04-variables.md)) |

---

## Mode-by-mode

### `markdown` · default for content scraping

```yaml
invoke:
  tool: "nika:fetch"
  args:
    url: "https://example.com/article"
    mode: markdown
```

**Behavior** · HTML → cleaned Markdown. Removes scripts · styles · nav · footer · ads. Preserves headings · paragraphs · lists · code blocks · links · images (lazy-loaded `data-src`/`srcset` images resolve to the real URL · largest `srcset` candidate).

**Implementation** · reference engine uses `htmd` (Rust port of html-to-markdown).

**Output** · Markdown string · ready to feed an LLM.

---

### `article` · main-content extraction

```yaml
invoke:
  tool: "nika:fetch"
  args:
    url: "https://news.example.com/2026/05/22/headline"
    mode: article
```

**Behavior** · extracts the main article body · stripping navigation · sidebars · ads · share widgets · related-post blocks · comments. Page-type aware: on forum/discussion pages the posts and replies ARE the content (kept, not pruned).

**Implementation** · reference engine runs a three-stage cascade (the failure modes are decorrelated, so the cascade beats any single extractor): (1) a Trafilatura-grade rule cascade: zone targeting (the semantic content container) + a boilerplate prune denylist, the 2024-2026 SOTA for main-content extraction; (2) a Readability pass (`dom_smoothie`) for markup-poor pages; (3) a Boilerpipe shallow-text-density floor for div-soup pages. The earlier stage wins when it yields a substantial body, else the next fires.

**Output** · Markdown string · article body only.

**When to use** vs `markdown` · use `article` for news/blogs/forums (cleaner · main content only) · use `markdown` for general pages (more content preserved).

---

### `text` · plain text

```yaml
invoke:
  tool: "nika:fetch"
  args:
    url: "https://example.com"
    mode: text
```

**Behavior** · HTML tags stripped · text content preserved with line breaks. No Markdown formatting.

**Use case** · raw text for full-text search · token counting · simple LLM input.

---

### `selector` · CSS selector

```yaml
invoke:
  tool: "nika:fetch"
  args:
    url: "https://example.com/products"
    mode: selector
    selector: "div.product-list"
```

**Behavior** · returns the raw HTML of the element(s) matching the CSS selector. If multiple match · concatenated.

**Implementation** · reference engine uses `scraper` (CSS selector engine).

**Output** · HTML string. Use a separate task with `markdown` mode if needed.

---

### `jq` · structured JSON extraction

```yaml
invoke:
  tool: "nika:fetch"
  args:
    url: "https://api.example.com/v1/users"
    mode: jq
    jq: "[.data.users[].email]"
```

**Behavior** · parses response as JSON · applies the jq expression · returns the result. The SAME jq language used in `output:` bindings and the `nika:jq` builtin: Nika has ONE data language (replaces the former JSONPath mode · jq is a superset of JSONPath). **The exactly-one-output law applies** (same engine · same law as `nika:jq` per [04 §bindings](../spec/04-variables.md)) · an expression producing 0 or N outputs is `NIKA-BUILTIN-FETCH-001` (wrap streams in `[…]` to collect).

**Implementation** · reference engine uses `jaq` (Rust jq).

**Output** · JSON value (string · array · object).

**Examples** ·
- `.`, whole response
- `.data.users[0]`, first user
- `[.data.users[].name]`, all user names (collected · one array)
- `[.. | .price?] | map(select(. != null))`, all prices recursively

---

### `metadata` · page metadata

```yaml
invoke:
  tool: "nika:fetch"
  args:
    url: "https://example.com/article"
    mode: metadata
```

**Behavior** · walks the HTML head + the embedded structured data. Returns a structured object. `og`/`twitter` are ALWAYS objects (stable shape); scalar keys are omitted when absent. `title`/`description` fall back to the `og:`/`twitter:` equivalents when the `<title>`/`<meta name=description>` is missing (common on SPAs). The URL-valued `og:image`/`og:url`/`twitter:image` are resolved to absolute against the effective base (like `canonical`). Beyond the `<meta>` tags it surfaces the two embedded-structured-data carriers: `jsonld` (schema.org `<script type=ld+json>` blocks, parsed) and `microdata` (schema.org `itemscope`/`itemprop` items, the W3C item model). Both are schema-agnostic, for a downstream `jq` step to walk.

**Output** ·
```json
{
  "title": "Article title",
  "description": "Article description",
  "og": { "title": "...", "image": "https://example.com/og.jpg", "url": "..." },
  "twitter": { "card": "summary_large_image", "title": "..." },
  "canonical": "https://example.com/article",
  "lang": "en",
  "author": "...",
  "published_time": "2026-06-01T10:00:00Z",
  "favicon": "https://example.com/favicon.ico",
  "alternates": [{ "href": "https://example.com/fr", "hreflang": "fr" }],
  "jsonld": [{ "@type": "Article", "headline": "..." }],
  "microdata": [{
    "type": ["https://schema.org/Product"],
    "properties": { "name": ["Widget"], "offers": [{ "type": ["https://schema.org/Offer"], "properties": { "price": ["19.99"] } }] }
  }]
}
```

Keys present only when the source carries them (absence over null). `jsonld`/`microdata` are surfaced only when ≥1 item is found.

**Use case** · indexing · preview generation · backlink analysis · product/recipe structured-data mining (the `jsonld`/`microdata` arrays).

---

### `links` · all outbound links

```yaml
invoke:
  tool: "nika:fetch"
  args:
    url: "https://example.com"
    mode: links
```

**Behavior** · extracts all `<a href>` URLs. Resolves relative to absolute against the document's effective base: a `<base href>` element when present (WHATWG), else the fetch URL.

**Output** · array of strings ·
```json
[
  "https://example.com/about",
  "https://example.com/contact",
  "https://other-site.com/article"
]
```

**Use case** · crawler · link analysis · sitemap building.

**Combining metadata + links** · two tasks on the same URL (the canon
set is CLOSED at v0.1: a combined `metadata-links` mode is a stdlib
v0.x candidate, rejected today like `llm-txt`).

---

### `feed` · RSS · Atom · JSON Feed

```yaml
invoke:
  tool: "nika:fetch"
  args:
    url: "https://example.com/feed.xml"
    mode: feed
```

**Behavior** · parses RSS · Atom · or JSON Feed. Returns normalized structure. Each item carries the short `summary` blurb AND, when present, the full `content` body (RSS `<content:encoded>` · Atom `<content>` · JSON Feed `content_html`): `content` is the field for full-text pipelines. Items also carry `media` (attached audio/video, RSS `<enclosure>` + MediaRSS `<media:content>`, for podcast/video feeds). Fields absent from the source are omitted (absence over null · stable shape).

**Implementation** · reference engine uses `feed-rs`.

**Output** ·
```json
{
  "title": "Feed title",
  "description": "...",
  "link": "https://example.com",
  "updated": "2026-05-22T10:00:00Z",
  "items": [
    {
      "id": "...",
      "title": "...",
      "link": "...",
      "author": "...",
      "summary": "...",
      "content": "...",
      "published": "...",
      "categories": ["..."],
      "media": [{ "url": "...", "type": "audio/mpeg", "size": 12345, "duration_secs": 1800 }]
    }
  ]
}
```

---

### `sitemap` · sitemap.xml

```yaml
invoke:
  tool: "nika:fetch"
  args:
    url: "https://example.com/sitemap.xml"
    mode: sitemap
```

**Behavior** · parses sitemap.xml or sitemap index. Returns array of URLs with optional lastmod.

**Implementation** · reference engine uses `quick-xml`.

**Output** ·
```json
[
  { "loc": "https://example.com/", "lastmod": "2026-05-20" },
  { "loc": "https://example.com/about", "lastmod": "2026-05-01" }
]
```

---

### `llm-txt` · RESERVED (not a v0.1 mode)

A future mode parsing the `llms.txt` convention (LLM-friendly site
descriptions). **NOT in the v0.1 canonical set**: `mode: llm-txt` is
rejected today (the canon list of 9 is closed · the conformance oracle
enforces it). Until it stabilizes (stdlib v0.2 candidate) · fetch with
`mode: text` and parse with `nika:jq` / an `infer:` step.

---

## Mode selection cheat-sheet

```
What you have                      Use mode
─────────────                      ────────
HTML page · want content for LLM    markdown    (default)
News/blog article                   article     (cleaner)
Want raw HTML structure             selector    (with CSS selector)
JSON API response                   jq          (with a jq expression)
Want page <meta> tags               metadata
Want outbound URLs only             links
RSS · Atom · JSON Feed              feed
sitemap.xml                         sitemap
Plain text · no structure           text
Want raw bytes · no processing      raw
```

---

## Forward-compat

New modes MAY enter stdlib v0.x. Mode-specific options (like `selector`, `jq`) are namespaced on the verb · forward-compat additive.

The 9 canonical modes cover ~99% of real-world web fetch use cases. Niche extractions (PDF · Word · Excel · audio · video) belong in the **media builtins** (deferred to stdlib v0.x · invoked via `invoke:` not `fetch:`).

---

🦋 *<!-- canon:extract_modes -->9<!-- /canon --> modes · 1 verb · zero invention surface.*
