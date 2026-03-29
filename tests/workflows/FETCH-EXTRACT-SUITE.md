# Nika Fetch + Extract Test Suite

Comprehensive test coverage for all 9 `fetch:` extract modes, 2 response modes, and advanced features (retry, headers, POST).

## Overview

This test suite validates Nika's `fetch:` verb with all extraction and response modes using real, stable URLs.

**Total Tests**: 15
- 9 extract modes
- 2 response modes
- 4 advanced features (retry, headers, POST, comprehensive suite)

## Test Files

### Extract Modes (9 Tests)

| File | Mode | Purpose | URL |
|------|------|---------|-----|
| `fetch-extract-mode-01-markdown.nika.yaml` | `markdown` | HTML to Markdown | example.com |
| `fetch-extract-mode-02-article.nika.yaml` | `article` | Main article (Readability) | rust-lang.org |
| `fetch-extract-mode-03-text.nika.yaml` | `text` | Visible plaintext (optional selector) | github.com |
| `fetch-extract-mode-04-selector.nika.yaml` | `selector` | Raw HTML elements | example.com |
| `fetch-extract-mode-05-metadata.nika.yaml` | `metadata` | OG/Twitter/JSON-LD/SEO | github.com |
| `fetch-extract-mode-06-links.nika.yaml` | `links` | Link extraction + classification | wikipedia.org |
| `fetch-extract-mode-07-jsonpath.nika.yaml` | `jsonpath` | JSON API querying | jsonplaceholder.typicode.com |
| `fetch-extract-mode-08-feed.nika.yaml` | `feed` | RSS/Atom/JSON feed parsing | github.com/releases.atom |
| `fetch-extract-mode-09-llm-txt.nika.yaml` | `llm_txt` | /llms.txt discovery | anthropic.com |

### Response Modes (2 Tests)

| File | Mode | Purpose | URL |
|------|------|---------|-----|
| `fetch-response-mode-10-full.nika.yaml` | `response: full` | Full HTTP response (status, headers, body, url) | httpbin.org |
| `fetch-response-mode-11-binary.nika.yaml` | `response: binary` | Binary storage in CAS media store | example.com/image.png |

### Advanced & Master (2 Tests)

| File | Purpose |
|------|---------|
| `fetch-retry-mode-12-retry.nika.yaml` | Exponential backoff retry mechanism |
| `fetch-extract-comprehensive-suite.nika.yaml` | Master suite (runs all 15 tests, generates report) |

## Quick Start

### Run All 15 Tests
```bash
nika run fetch-extract-comprehensive-suite.nika.yaml
```

### Run Single Test
```bash
# Example: test extract: text mode
nika run fetch-extract-mode-03-text.nika.yaml
```

### Validate Syntax (No Network)
```bash
nika check fetch-extract-comprehensive-suite.nika.yaml
```

### Test with Mock Provider (Deterministic)
```bash
nika run fetch-extract-comprehensive-suite.nika.yaml --provider mock
```

## 9 Extract Modes

### 1. `extract: markdown`
**Purpose**: Convert HTML to clean Markdown
**Input**: Any HTML page
**Output**: Markdown with `#` headers, `**bold**`, `[links](url)`
**Validation**: Headers, formatting, no HTML tags
**URL**: `https://example.com`

### 2. `extract: article`
**Purpose**: Extract main article body (Readability algorithm)
**Input**: Blog/news pages
**Output**: Article text without nav/ads/footer
**Validation**: No boilerplate, >500 chars, contiguous text
**URL**: `https://www.rust-lang.org/en-US`

### 3. `extract: text`
**Purpose**: Extract visible plaintext (optional CSS selector)
**Input**: Any web page
**Output**: Plaintext (no HTML)
**Optional**: `selector: "main"` to filter by CSS selector
**Validation**: No HTML tags, readable text
**URL**: `https://github.com`

### 4. `extract: selector`
**Purpose**: Extract raw HTML of elements matching CSS selector
**Input**: Any HTML page
**Output**: Raw HTML with tags preserved
**Required**: `selector: "h1"` (or other CSS selector)
**Validation**: Includes `<>`, all matching elements, attributes intact
**URL**: `https://example.com`

### 5. `extract: metadata`
**Purpose**: Extract OG tags, Twitter Cards, JSON-LD, SEO metadata
**Input**: Any web page
**Output**: JSON object with title, description, og:*, twitter:*, schema
**Validation**: Required fields present, URLs valid, no HTML
**URL**: `https://github.com/anthropics/anthropic-sdk-python`

### 6. `extract: links`
**Purpose**: Extract all links and classify as internal/external
**Input**: Any web page
**Output**: Array of `{href, text, type}` objects
**Validation**: Classification correct, all `<a>` elements found, no duplicates
**URL**: `https://www.wikipedia.org/wiki/Artificial_intelligence`

### 7. `extract: jsonpath`
**Purpose**: Query JSON API responses using JSONPath expressions
**Input**: JSON endpoints
**Output**: Queried values (string, number, array, object)
**Required**: `selector: "$.title"` (or JSONPath expression)
**Examples**:
- `$.title` → single string value
- `$[0:3].id` → array of IDs (first 3)
- `$.*.email` → all emails at any level

**URL**: `https://jsonplaceholder.typicode.com/posts/1`

### 8. `extract: feed`
**Purpose**: Parse RSS, Atom, or JSON Feed formats
**Input**: RSS/Atom/JSON Feed endpoints
**Output**: Array of `{title, link, published_at, author, description}`
**Validation**: All entries have required fields, timestamps ISO 8601, no HTML
**URLs**:
- ATOM: `https://github.com/anthropics/anthropic-sdk-python/releases.atom`
- JSON: `https://www.rust-lang.org/feed.json`

### 9. `extract: llm_txt`
**Purpose**: Discover and parse `/llms.txt` or `/.well-known/llm.txt`
**Input**: Domain URL
**Output**: Content if found, null/empty if not (graceful)
**Validation**: Both standard locations checked, content parsed
**URL**: `https://anthropic.com`

## Response Modes

### `response: full`
**Purpose**: Retrieve complete HTTP response envelope
**Output**:
```json
{
  "status": 200,
  "headers": { "Content-Type": "...", ... },
  "body": "...",
  "url": "https://final-url-after-redirects"
}
```
**Use Case**: Inspect HTTP metadata, follow redirects, check caching headers
**URL**: `https://httpbin.org/get`

### `response: binary`
**Purpose**: Fetch binary data and store in CAS media storage
**Output**: Hash/reference string (CAS content-addressable reference)
**Use Case**: Store image/PDF for vision tasks, metadata extraction
**Next Steps**: Use hash in `nika:dimensions`, `nika:metadata`, vision source
**URLs**:
- Image: `https://example.com/image.png`
- PDF: `https://www.w3.org/WAI/test-evaluate/sample-files/sampledata.pdf`

## Advanced Features

### Retry with Exponential Backoff
```yaml
retry:
  max_attempts: 3
  delay_ms: 200
  backoff: 2.0
```

**Behavior**:
- Attempt 1: T=0ms
- Attempt 2: T=200ms (if transient error)
- Attempt 3: T=600ms (if another transient error)

**Transient Errors** (trigger retry): 429, 502, 503, 504, timeout, connection reset
**Permanent Errors** (no retry): 400, 401, 403, 404

### Custom Headers
```yaml
fetch:
  url: "https://..."
  headers:
    User-Agent: "Nika-Test-Suite/1.0"
    Authorization: "Bearer token"
    X-Custom-Header: "value"
```

### POST with JSON Body
```yaml
fetch:
  url: "https://httpbin.org/post"
  method: POST
  json:
    key: "value"
    nested:
      field: "data"
```

## Real URLs Used

All URLs are stable and well-established:

| URL | Purpose | Tests |
|-----|---------|-------|
| `https://example.com` | Basic HTML | markdown, text, selector |
| `https://github.com` | Complex page | text, metadata, links |
| `https://www.rust-lang.org` | Long article | article |
| `https://www.wikipedia.org/wiki/Artificial_intelligence` | Link-rich page | links |
| `https://jsonplaceholder.typicode.com/posts/*` | JSON API (test data) | jsonpath |
| `https://httpbin.org/get` | HTTP testing | response:full, retry |
| `https://github.com/.../releases.atom` | ATOM feed | feed |
| `https://www.rust-lang.org/feed.json` | JSON feed | feed |
| `https://anthropic.com` | AI company | llm_txt |

## Running Tests

### Run All Tests
```bash
nika run fetch-extract-comprehensive-suite.nika.yaml
```

### Run by Type
```bash
# All extract modes
nika run fetch-extract-mode-*.nika.yaml

# All response modes
nika run fetch-response-mode-*.nika.yaml

# Single mode
nika run fetch-extract-mode-07-jsonpath.nika.yaml
```

### Options
```bash
# Validate without running
nika check fetch-extract-comprehensive-suite.nika.yaml

# Mock provider (no network)
nika run fetch-extract-comprehensive-suite.nika.yaml --provider mock

# Increase timeout for slow networks
nika run fetch-extract-mode-02-article.nika.yaml --timeout 30
```

## Expected Output

**Healthy Run**:
```
✓ fetch-extract-mode-01-markdown ............ PASS
✓ fetch-extract-mode-02-article ............ PASS
✓ fetch-extract-mode-03-text ............... PASS
✓ fetch-extract-mode-04-selector ........... PASS
✓ fetch-extract-mode-05-metadata ........... PASS
✓ fetch-extract-mode-06-links ............. PASS
✓ fetch-extract-mode-07-jsonpath .......... PASS
✓ fetch-extract-mode-08-feed .............. PASS
✓ fetch-extract-mode-09-llm-txt ........... PASS
✓ fetch-response-mode-10-full ............ PASS
✓ fetch-response-mode-11-binary ........... PASS
✓ fetch-retry-mode-12-retry .............. PASS

Results: 15/15 PASS
Duration: ~45-60 seconds
```

**Generated Report**: `./test-results/fetch-extract-test-report.md`

## Documentation

**Comprehensive Guide**: See `../docs/fetch-extract-modes.md`
- Detailed explanation of each mode
- Expected outputs and examples
- Validation criteria
- Common issues and workarounds
- Edge cases and limitations
- Test execution guide

**Quick Reference**: See `../docs/fetch-extract-quick-reference.md`
- 9 modes at a glance
- Real URLs used
- Timeout guidelines
- Common patterns
- Validation checklist
- Error messages and fixes

## Troubleshooting

| Problem | Cause | Solution |
|---------|-------|----------|
| All tests timeout | Network unreachable | Check network, try VPN, use `--provider mock` |
| Extract returns empty | JavaScript-rendered page | Increase timeout, try different mode |
| JSONPath returns null | Invalid expression or API changed | Verify endpoint, test with jq tool |
| Binary hash invalid | Network error or unsupported format | Verify file exists, increase timeout |
| Tests pass with mock, fail with network | Data structure differs | Debug individually, check real endpoint |

## Architecture

### URL Stability
- All URLs are stable, well-established sites
- No temporary or internal URLs
- Quarterly review recommended

### Test Isolation
- Each test independent (can run individually)
- No test dependencies or shared state
- Mock provider allows fast iteration

### Validation Strategy
- Use `infer:` task with LLM validation
- Check required fields and data types
- Allow graceful handling of optional fields

### Error Codes
- NIKA-045: Fetch error (network, URL, timeout)
- NIKA-046: Extract error (invalid selector, mode)

## Maintenance

**Weekly**: Nothing (tests are stable)

**Monthly**:
- Spot-check URLs accessible
- Verify timeout values adequate

**Quarterly**:
- Review external URL availability
- Update docs for schema versions
- Add tests for new modes

## Contributing

To add tests for a new extract mode:

1. Create `fetch-extract-mode-XX-name.nika.yaml`
2. Update `fetch-extract-comprehensive-suite.nika.yaml`
3. Document in `../docs/fetch-extract-modes.md`
4. Add to quick reference `../docs/fetch-extract-quick-reference.md`

## References

- **Fetch Verb Docs**: `nika help verbs` then select fetch
- **Extract Modes**: See `../docs/fetch-extract-modes.md`
- **JSONPath**: `https://goessner.net/articles/JsonPath/`
- **CSS Selectors**: `https://www.w3.org/TR/selectors-3/`
- **XML/HTML Parsing**: Built into Nika runtime
- **Feed Format**: RSS 2.0, Atom 1.0, JSON Feed 1.1

## License

Tests are part of the Nika project (AGPL-3.0-or-later).
