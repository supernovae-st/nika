# Fetch + Extract Quick Reference

Fast lookup for testing Nika's fetch verb and all 9 extract modes.

## 9 Extract Modes at a Glance

| # | Mode | Input | Output | Use When |
|---|------|-------|--------|----------|
| 1 | `markdown` | Any HTML | Clean Markdown | Converting web content to docs |
| 2 | `article` | Blog/news | Main article body | Extracting article text only |
| 3 | `text` | Any page | Plaintext (optional selector) | Getting all visible text |
| 4 | `selector` | Any page | Raw HTML elements | Need specific HTML nodes |
| 5 | `metadata` | Any page | OG/Twitter/JSON-LD | SEO preview, social sharing info |
| 6 | `links` | Any page | Link objects (internal/external) | Analyzing site structure |
| 7 | `jsonpath` | JSON API | Queried values | Extracting specific JSON fields |
| 8 | `feed` | RSS/Atom/JSON | Feed entries array | Parsing news feeds |
| 9 | `llm_txt` | Any domain | /llms.txt content | AI model discovery |

## Response Modes

| Mode | Returns | Use When |
|------|---------|----------|
| `response: omit` (default) | Extract result | Just need content |
| `response: full` | `{status, headers, body, url}` | Inspecting HTTP metadata |
| `response: binary` | CAS hash | Storing image/PDF for vision |

## Retry Configuration

```yaml
retry:
  max_attempts: 3        # Total tries
  delay_ms: 200          # First delay
  backoff: 2.0           # Exponential: 200ms → 400ms → 800ms
fetch:
  url: "https://..."
```

## Test Workflows Provided

### Extract Modes (9)
```
fetch-extract-mode-01-markdown.nika.yaml
fetch-extract-mode-02-article.nika.yaml
fetch-extract-mode-03-text.nika.yaml
fetch-extract-mode-04-selector.nika.yaml
fetch-extract-mode-05-metadata.nika.yaml
fetch-extract-mode-06-links.nika.yaml
fetch-extract-mode-07-jsonpath.nika.yaml
fetch-extract-mode-08-feed.nika.yaml
fetch-extract-mode-09-llm-txt.nika.yaml
```

### Response Modes (2)
```
fetch-response-mode-10-full.nika.yaml
fetch-response-mode-11-binary.nika.yaml
```

### Advanced (1 master suite + individual)
```
fetch-extract-comprehensive-suite.nika.yaml  # Run all 15 tests
fetch-retry-mode-12-retry.nika.yaml
```

## Common Test Patterns

### Test Single Mode
```bash
nika run fetch-extract-mode-03-text.nika.yaml
```

### Run All Tests
```bash
nika run fetch-extract-comprehensive-suite.nika.yaml
```

### Dry-Run (validate syntax, no network)
```bash
nika run workflow.nika.yaml --dry-run
```

### Use Mock Provider (deterministic)
```bash
nika run workflow.nika.yaml --provider mock
```

## Timeout Guidelines

| Scenario | Timeout |
|----------|---------|
| Simple extract (markdown, text) | 10s |
| Article extraction | 15s |
| Large feed | 20s |
| llm_txt discovery | 15s |
| Binary download | 15-30s depending on size |
| Retry test | 10s (retry adds delay) |

## Real URLs Used in Tests

| URL | Test | Mode |
|-----|------|------|
| `https://example.com` | markdown, text, selector | Basic HTML |
| `https://github.com` | text, metadata, links | Complex page |
| `https://www.rust-lang.org` | article | Long article |
| `https://jsonplaceholder.typicode.com/posts/1` | jsonpath | JSON API |
| `https://httpbin.org/get` | response: full, retry | Testing HTTP |
| `https://github.com/.../releases.atom` | feed | ATOM feed |
| `https://www.rust-lang.org/feed.json` | feed | JSON feed |
| `https://anthropic.com` | llm_txt | AI company |

## Validation Checklist

### For `extract: markdown`
- [ ] Headers converted to `#` syntax
- [ ] **Bold** uses `**`
- [ ] [Links] properly formatted
- [ ] No `<` `>` characters
- [ ] Readable structure maintained

### For `extract: article`
- [ ] Navigation removed
- [ ] Ads filtered out
- [ ] Main text contiguous
- [ ] Length > 500 chars
- [ ] No footer boilerplate

### For `extract: text`
- [ ] No HTML tags
- [ ] All visible text included
- [ ] Whitespace normalized
- [ ] Selector properly filtered (if used)

### For `extract: selector`
- [ ] Output contains HTML tags
- [ ] All matching elements included
- [ ] Attributes preserved
- [ ] Proper nesting maintained

### For `extract: metadata`
- [ ] Title present
- [ ] Description present
- [ ] OG tags parsed (if defined)
- [ ] URLs valid format
- [ ] JSON-LD extracted (if present)

### For `extract: links`
- [ ] Internal/external classification correct
- [ ] All `<a>` elements found
- [ ] Links are absolute URLs
- [ ] No duplicates (or deduplicated)
- [ ] Broken links detected

### For `extract: jsonpath`
- [ ] Expression valid JSONPath
- [ ] Result matches expected type
- [ ] Data types preserved (not all strings)
- [ ] No HTML in output

### For `extract: feed`
- [ ] Array of entries returned
- [ ] Each has: title, link, published_at
- [ ] Timestamps in ISO 8601
- [ ] No HTML in text fields
- [ ] Deduplication applied

### For `extract: llm_txt`
- [ ] Graceful handling if not found
- [ ] Both `/llms.txt` and `/.well-known/llm.txt` checked
- [ ] Content parsed (YAML or JSON)
- [ ] Key fields extracted

### For `response: full`
- [ ] Status is number (200, 404, etc)
- [ ] Headers object present
- [ ] Body is complete
- [ ] URL is final (after redirects)

### For `response: binary`
- [ ] Hash returned (non-empty string)
- [ ] Different files have different hashes
- [ ] Can be used in vision: source

### For Retry
- [ ] Exponential backoff applied (200, 400, 800ms)
- [ ] Transient errors (429, 502-503) trigger retry
- [ ] Permanent errors (400, 401, 404) fail immediately
- [ ] Max attempts respected
- [ ] Success ends early

## Error Messages and Fixes

| Error | Cause | Fix |
|-------|-------|-----|
| `NIKA-045: Fetch error` | Network/URL issue | Check URL, network, timeout |
| `NIKA-046: Extract error` | Invalid selector or mode | Verify selector syntax |
| `timeout` | Request takes too long | Increase `timeout:` parameter |
| `Empty result` | JavaScript-rendered content | Try different extraction mode |
| `Invalid JSONPath` | Bad expression syntax | Validate with jq tool |

## Development Tips

### Test Against Real APIs
```yaml
fetch:
  url: "https://jsonplaceholder.typicode.com/posts/1"
  extract: jsonpath
  selector: "$.userId"
```

### Use Binary for Vision Tests
```yaml
- id: fetch_img
  fetch:
    url: "https://example.com/photo.jpg"
    response: binary

- id: analyze_img
  infer:
    content:
      - type: image
        source: $fetch_img  # Use hash here
      - type: text
        text: "Describe this image"
```

### Add Custom Headers
```yaml
fetch:
  url: "https://api.example.com/data"
  headers:
    Authorization: "Bearer token"
    User-Agent: "Nika-Test/1.0"
```

### POST JSON Data
```yaml
fetch:
  url: "https://httpbin.org/post"
  method: POST
  json:
    key: "value"
    number: 42
```

### Test Redirect Following
```yaml
fetch:
  url: "https://example.com/redirect"
  response: full
  # Check response.url for final destination
```

## Monitoring and Maintenance

- **URL availability**: Quarterly check (most URLs are stable)
- **Feed staleness**: Feeds may become stale over time
- **Timeout adequacy**: Adjust if network gets slower
- **Mock provider**: Always test with `--provider mock` first
- **Test coverage**: Keep extract modes up to date with schema changes
