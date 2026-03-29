# Nika Fetch + Extract Test Suite Documentation

Complete test coverage for all 9 extract modes, 2 response modes, and advanced fetch features.

## Overview

This test suite provides comprehensive validation of Nika's `fetch:` verb with all extraction and response modes. Each test is designed to verify correct behavior against real, stable URLs.

Total test workflows: **15** (9 extract + 2 response + 4 advanced)

---

## Extract Modes (9 Tests)

### Mode 1: `extract: markdown`

**Purpose**: Convert HTML to clean Markdown format.

**Test Workflow**: `fetch-extract-mode-01-markdown.nika.yaml`

**URL**: `https://example.com`

**Expected Output**:
- Proper Markdown syntax (headers, bold, links)
- No remaining HTML tags (except Markdown)
- Clean formatting without excessive whitespace
- Preserved logical structure

**Validation Criteria**:
1. Markdown headers present (# ## ### etc)
2. Bold text uses `**text**`
3. Links formatted as `[text](url)`
4. List items formatted as `- item` or `1. item`
5. Code blocks use backticks or code fences

**Common Issues**:
- Nested tables may convert poorly to Markdown
- JavaScript-rendered content may not extract
- Complex layouts may require selector refinement

---

### Mode 2: `extract: article`

**Purpose**: Extract main article body using Readability algorithm.

**Test Workflow**: `fetch-extract-mode-02-article.nika.yaml`

**URL**: `https://www.rust-lang.org/en-US`

**Expected Output**:
- Main article/content text
- Navigation/sidebar removed
- Ads and boilerplate filtered
- Contiguous body text (no gaps)
- Minimum 500 chars

**Validation Criteria**:
1. No nav/menu text
2. No footer boilerplate
3. No advertisement content
4. Contiguous paragraphs
5. Readable prose preserved

**Common Issues**:
- Non-article pages (home, category) may extract poorly
- JavaScript-heavy SPAs need longer timeout or rendering
- PDF content requires different extraction mode

---

### Mode 3: `extract: text` (with optional selector)

**Purpose**: Extract visible plaintext, optionally filtered by CSS selector.

**Test Workflow**: `fetch-extract-mode-03-text.nika.yaml`

**URL**: `https://github.com`

**Expected Output**:
- All visible text content
- No HTML tags
- Whitespace normalized
- Optional: only text from matched selector

**Validation Criteria**:
1. No `<` or `>` characters (unless escaped)
2. Text is human-readable
3. Selector filtering reduces output size
4. All lines converted to plaintext

**Variants**:
- Without selector: entire page text
- With selector `main`: only main content
- With selector `article p`: only paragraphs in articles

**Common Issues**:
- Hidden elements may be included
- Whitespace normalization varies
- CSS display:none may still extract text

---

### Mode 4: `extract: selector`

**Purpose**: Extract raw HTML of elements matching a CSS selector.

**Test Workflow**: `fetch-extract-mode-04-selector.nika.yaml`

**URL**: `https://example.com`

**Selector**: `h1`

**Expected Output**:
- Raw HTML strings (includes tags)
- Complete element tree
- All attributes preserved
- Proper closing tags

**Validation Criteria**:
1. Output contains `<` and `>` (HTML)
2. All matching elements included
3. Attributes like `class`, `id`, `data-*` present
4. Proper nesting preserved
5. No plaintext conversion

**Selectors to Test**:
- `h1` — extract all H1 headings
- `div.content` — extract divs with class "content"
- `a[href]` — extract all links with href attribute
- `img` — extract all image elements

**Common Issues**:
- Invalid selectors cause errors
- Complex pseudo-selectors may not work
- Inline styles not always preserved

---

### Mode 5: `extract: metadata`

**Purpose**: Extract Open Graph, Twitter Card, JSON-LD, and SEO metadata.

**Test Workflow**: `fetch-extract-mode-05-metadata.nika.yaml`

**URL**: `https://github.com/anthropics/anthropic-sdk-python`

**Expected Output**:
```json
{
  "title": "...",
  "description": "...",
  "og:title": "...",
  "og:description": "...",
  "og:image": "...",
  "og:type": "...",
  "og:url": "...",
  "twitter:card": "summary|summary_large_image",
  "twitter:title": "...",
  "twitter:description": "...",
  "twitter:image": "...",
  "schema": { ... },  // JSON-LD data
  "favicon": "..."
}
```

**Validation Criteria**:
1. Title present (from `<title>` or `og:title`)
2. Description present
3. Open Graph tags if defined
4. Twitter Card tags if defined
5. JSON-LD structured data parsed
6. Valid URLs in image fields
7. Non-empty text values

**Common Issues**:
- Not all sites define all metadata
- JSON-LD may be malformed
- Image URLs may be relative paths
- Missing favicon is OK

---

### Mode 6: `extract: links`

**Purpose**: Extract all links and classify as internal/external.

**Test Workflow**: `fetch-extract-mode-06-links.nika.yaml`

**URL**: `https://www.wikipedia.org/wiki/Artificial_intelligence`

**Expected Output**:
```json
[
  {
    "href": "https://en.wikipedia.org/wiki/Intelligence",
    "text": "Intelligence",
    "type": "internal"
  },
  {
    "href": "https://external.org/page",
    "text": "External",
    "type": "external"
  },
  {
    "href": "#broken-link",
    "text": "Broken",
    "type": "broken"
  }
]
```

**Validation Criteria**:
1. All `<a href="">` elements extracted
2. Internal links (same domain) marked correctly
3. External links (different domain) marked correctly
4. Broken links detected (anchor-only, js:, etc)
5. Text content preserved for each link
6. No duplicate entries (or flagged)
7. Relative URLs resolved to absolute

**Classification Rules**:
- Internal: scheme + domain match page URL
- External: different domain
- Broken: no href, js:, #only, invalid protocol

**Common Issues**:
- Relative links need proper resolution
- Domain matching fails for subdomains
- Query parameters may affect classification

---

### Mode 7: `extract: jsonpath`

**Purpose**: Query JSON responses using JSONPath expressions.

**Test Workflow**: `fetch-extract-mode-07-jsonpath.nika.yaml`

**URLs**:
- API: `https://jsonplaceholder.typicode.com/posts/1`
- Array: `https://jsonplaceholder.typicode.com/posts`
- Comments: `https://jsonplaceholder.typicode.com/posts/1/comments`

**Expected Output**:
- Single value: `$.title` → `"string value"`
- Array slice: `$[0:3].id` → `[1, 2, 3]`
- Nested: `$.*.email` → `["a@test.com", "b@test.com"]`

**JSONPath Expressions**:
| Expression | Meaning |
|-----------|---------|
| `$.field` | Root field |
| `$[0]` | First array element |
| `$[0:3]` | Array slice (0-2) |
| `$[*].id` | All IDs in array |
| `$.*.email` | All emails at any level |
| `$..email` | All "email" fields recursively |

**Validation Criteria**:
1. Expression is valid JSONPath
2. Results match expected type (string, number, array)
3. Correct subset extracted
4. No HTML in result
5. Data types preserved (not stringified)

**Common Issues**:
- Invalid selectors cause parse errors
- Recursive descent (`..`) can be slow on large docs
- Array slicing syntax varies by implementation

---

### Mode 8: `extract: feed`

**Purpose**: Parse RSS, Atom, or JSON Feed formats.

**Test Workflow**: `fetch-extract-mode-08-feed.nika.yaml`

**URLs**:
- ATOM: `https://github.com/anthropics/anthropic-sdk-python/releases.atom`
- JSON Feed: `https://www.rust-lang.org/feed.json`
- RSS: Any RSS feed URL

**Expected Output**:
```json
[
  {
    "title": "Entry Title",
    "link": "https://...",
    "published_at": "2026-03-29T10:00:00Z",
    "author": "Name",
    "description": "Summary",
    "content": "Full HTML content"
  },
  ...
]
```

**Supported Formats**:
- RSS 2.0
- Atom 1.0
- JSON Feed 1.1

**Validation Criteria**:
1. Returns array of entries
2. Each entry has: title, link, published_at
3. Optional: author, description, content
4. Links are absolute URLs
5. Timestamps in ISO 8601 format
6. No HTML in text fields (or stripped)
7. Duplicates deduplicated
8. Ordered by date (newest first)

**Common Issues**:
- CDATA sections may not parse
- Missing optional fields OK
- Timestamps vary: use, pubDate, published
- HTML in description needs stripping

---

### Mode 9: `extract: llm_txt`

**Purpose**: Discover and parse `/llms.txt` or `/.well-known/llm.txt` files.

**Test Workflow**: `fetch-extract-mode-09-llm-txt.nika.yaml`

**URLs**:
- With llms.txt: `https://anthropic.com`
- Without: `https://example.com`

**Expected Output** (if found):
```yaml
version: 0.1
models:
  - name: claude-opus-4
    capabilities: [text, vision, tool_use]
contact: api@anthropic.com
guidelines: "Use responsibly"
```

**Validation Criteria**:
1. Checks standard locations:
   - `/llms.txt`
   - `/.well-known/llm.txt`
2. If found: parsed as YAML or JSON
3. Key fields extracted: version, models, contact
4. If not found: graceful null/empty response
5. No HTTP error on 404 (just empty result)

**Fallback Behavior**:
- Primary: `/.well-known/llm.txt`
- Secondary: `/llms.txt`
- Not found: return empty array or null

**Common Issues**:
- Not all sites have llms.txt (expected)
- Format may vary (YAML vs JSON)
- Network timeouts on slow servers

---

## Response Modes (2 Tests)

### Response Mode 1: `response: full`

**Purpose**: Retrieve complete HTTP response with metadata.

**Test Workflow**: `fetch-response-mode-10-full.nika.yaml`

**URL**: `https://httpbin.org/get`

**Expected Output**:
```json
{
  "status": 200,
  "headers": {
    "Content-Type": "application/json",
    "Content-Length": "1234",
    "Server": "gunicorn/19.9.0",
    ...
  },
  "body": "...",
  "url": "https://httpbin.org/get"
}
```

**Validation Criteria**:
1. Status is numeric (e.g., 200, 404, 500)
2. Headers object present (key-value map)
3. Body is complete response body string
4. URL is final URL (after redirects)
5. Common headers present: Content-Type, Server

**Use Cases**:
- Inspect HTTP headers for caching info
- Follow redirect chains (url field)
- Check response status before extraction
- Extract Set-Cookie for session management

**Common Issues**:
- Large response bodies may be truncated
- Binary bodies may be base64-encoded
- Streaming responses may be incomplete

---

### Response Mode 2: `response: binary`

**Purpose**: Fetch binary data and store in CAS media storage.

**Test Workflow**: `fetch-response-mode-11-binary.nika.yaml`

**URLs**:
- Image: `https://example.com/image.png`
- PDF: `https://www.w3.org/WAI/test-evaluate/sample-files/sampledata.pdf`

**Expected Output**:
```
"abc123def456..."  # CAS hash reference (hex or base64)
```

**Validation Criteria**:
1. Output is non-null string (hash)
2. Hash is non-empty
3. Hash looks like valid reference (hex, base64, etc)
4. Different URLs produce different hashes
5. Same URL produces same hash (caching)
6. Hash can be used in vision: source

**Storage Behavior**:
- Bytes stored in content-addressable storage (CAS)
- Returns hash/reference to stored media
- Enables passing to `nika:dimensions`, `nika:metadata`
- Can use hash in vision: `[type: image, source: hash]`

**File Format Support**:
- Images: PNG, JPEG, WebP, GIF, AVIF
- Documents: PDF
- Audio/Video: MP3, MP4, WebM (with media verbs)

**Common Issues**:
- Large files may timeout (increase timeout:)
- Binary data must not be text (use response: binary)
- Hash format is implementation-specific

---

## Advanced Features (4+ Tests)

### Advanced Feature 1: Retry with Exponential Backoff

**Purpose**: Automatic retry on transient failures.

**Test Workflow**: `fetch-retry-mode-12-retry.nika.yaml`

**Configuration**:
```yaml
retry:
  max_attempts: 3        # Total attempts
  delay_ms: 200          # Initial delay
  backoff: 2.0           # Exponential multiplier
fetch:
  url: "https://..."
  timeout: 10
```

**Behavior Timeline**:
```
Attempt 1: T=0ms (immediate)
  ↓ [FAIL - transient error]
Attempt 2: T=200ms + backoff
  ↓ [FAIL - transient error]
Attempt 3: T=200+400=600ms + backoff
  ↓ [SUCCESS or final failure]
```

**Backoff Calculation**:
- Delay 1: 200ms
- Delay 2: 200ms × 2.0 = 400ms
- Delay 3: 400ms × 2.0 = 800ms
- Total max time: ~1.2 seconds (plus jitter)

**Transient Errors** (trigger retry):
- HTTP 429 (Too Many Requests)
- HTTP 502-503 (Bad Gateway, Service Unavailable)
- HTTP 504 (Gateway Timeout)
- Network timeout
- Connection reset

**Permanent Errors** (don't retry):
- HTTP 400 (Bad Request)
- HTTP 401 (Unauthorized)
- HTTP 403 (Forbidden)
- HTTP 404 (Not Found)
- Invalid URL

**Validation Criteria**:
1. Exponential backoff applied
2. Jitter prevents thundering herd
3. Max attempts respected
4. Transient errors retried
5. Permanent errors fail immediately
6. Success ends early (no wasted retries)

**Common Issues**:
- Retry delay too short (thundering herd)
- Backoff too aggressive (long total time)
- No jitter causes synchronization issues

---

### Advanced Feature 2: Custom Headers

**Purpose**: Send custom HTTP headers (User-Agent, auth, etc).

**Configuration**:
```yaml
fetch:
  url: "https://..."
  headers:
    User-Agent: "Nika-Test-Suite/1.0"
    Authorization: "Bearer token123"
    X-Custom-Header: "value"
```

**Use Cases**:
- User-Agent spoofing (bypass bot detection)
- Bearer token authentication
- API key in header
- Custom application headers
- Accept-Language for localization

**Validation Criteria**:
1. Headers sent in HTTP request
2. Case-insensitive header names
3. Multiple header values supported
4. Special characters encoded properly
5. Sensitive headers logged (redacted)

---

### Advanced Feature 3: POST with JSON Body

**Purpose**: Send JSON payloads in POST requests.

**Configuration**:
```yaml
fetch:
  url: "https://api.example.com/data"
  method: POST
  json:
    key: "value"
    nested:
      field: "data"
  timeout: 10
```

**Validation Criteria**:
1. Content-Type: application/json set automatically
2. Object serialized to JSON string
3. Nested structures supported
4. Arrays supported
5. Response extracted as normal

**Alternative with Raw Body**:
```yaml
fetch:
  url: "https://..."
  method: POST
  body: '{"key":"value"}'
```

---

## Test Execution Guide

### Run All Tests
```bash
nika run fetch-extract-comprehensive-suite.nika.yaml
```

### Run Single Extract Mode
```bash
nika run fetch-extract-mode-03-text.nika.yaml
```

### Run with Custom Timeout
```bash
nika run fetch-extract-mode-02-article.nika.yaml --timeout 30
```

### Dry-Run (no network calls)
```bash
nika run fetch-extract-comprehensive-suite.nika.yaml --dry-run
```

### Mock Provider (deterministic, no API calls)
```bash
nika run fetch-extract-comprehensive-suite.nika.yaml --provider mock
```

---

## Expected Results

### Healthy Run
```
Status: PASS
Tests: 15/15 passed
Duration: ~45-60 seconds
Network calls: ~20 (some retries)
```

### With Timeouts
- Mode 2 (article) may timeout on slow networks (increase to 30s)
- Mode 8 (feed) may timeout on large feeds (increase to 20s)

### With Network Issues
- Mode 9 (llm_txt) may return empty for sites without /llms.txt (OK)
- Mode 6 (links) may fail on JavaScript-heavy pages
- Mode 4 (selector) may fail with invalid selectors

---

## Common Failure Patterns

### All tests timeout
**Cause**: Network unreachable or ISP blocking
**Fix**: Check network, try with VPN, use mock provider

### Extract modes return empty
**Cause**: JavaScript-rendered content
**Fix**: Increase timeout, use different extraction mode, check if page loads in browser

### JSONPath returns null
**Cause**: Invalid expression or different API response structure
**Fix**: Verify endpoint is still active, test path with jq tool: `curl url | jq 'path'`

### Binary returns invalid hash
**Cause**: Network error, timeout, or unsupported format
**Fix**: Verify file exists and is accessible, increase timeout

---

## Edge Cases

### 1. Redirect Chains
**Test**: URL redirects multiple times (301 → 302 → 200)
**Expected**: `response: full` shows final URL, body from final destination
**Verify**: `response.url` matches final destination

### 2. Large Pages (>100MB)
**Test**: Fetch extremely large HTML or JSON
**Expected**: Timeout or truncation (configurable limit)
**Verify**: Extraction works on subset, no memory exhaustion

### 3. Encoding Issues
**Test**: UTF-8, Latin-1, or garbled character pages
**Expected**: Auto-detect encoding, extract readable text
**Verify**: No mojibake (corrupted characters)

### 4. Single-Page Apps (SPAs)
**Test**: React/Vue/Angular sites with JavaScript rendering
**Expected**: May extract shell HTML only, not rendered content
**Verify**: Use increased timeout, accept limitation

### 5. Authentication-Required Pages
**Test**: Pages behind login (401/403)
**Expected**: Fails or returns limited content
**Verify**: Can add auth header in headers: section

---

## Maintenance Notes

- Test URLs are stable and well-established (example.com, github.com, etc)
- Mock provider allows testing without network calls
- Retry tests use httpbin.org for controlled failure scenarios
- Feed tests use real RSS/Atom feeds (may change, monitor for staleness)
- Consider quarterly review of external URLs for availability
