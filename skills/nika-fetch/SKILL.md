---
name: nika-fetch
description: >-
  Expert at the Nika fetch: verb for HTTP requests in .nika.yaml workflows.
  Covers all 9 extract modes (markdown, article, text, selector, metadata,
  links, jsonpath, feed, llm_txt), response modes (full, binary), HTTP methods,
  headers, JSON bodies, web scraping, and API integration patterns. Use when
  building fetch: tasks in Nika YAML workflows (schema nika/workflow@0.12).
---

# Nika fetch: Verb Expert

The `fetch:` verb makes HTTP requests and optionally extracts/transforms the response.

## Basic Syntax

```yaml
# GET request (simplest)
- id: get
  fetch:
    url: "https://api.example.com/data"

# POST with JSON body
- id: post
  fetch:
    url: "https://api.example.com/items"
    method: POST
    json:
      name: "widget"
      count: 42

# With headers
- id: auth
  fetch:
    url: "https://api.example.com/protected"
    headers:
      Authorization: "Bearer {{$env.API_TOKEN}}"
      Accept: "application/json"
```

## Full Field Reference

```yaml
- id: request
  fetch:
    url: "https://..."          # Required
    method: GET                  # GET | POST | PUT | DELETE | PATCH | HEAD
    headers:                     # Optional headers
      Key: "Value"
    json:                        # JSON body (auto-sets Content-Type)
      key: value
    body: "raw string"           # Alternative to json (raw body)
    extract: markdown            # Post-processing mode (see below)
    selector: "css selector"     # CSS selector (for text/selector extract)
    response: full               # Response mode (see below)
  timeout: 30                   # Seconds
  retry:
    max_attempts: 3
    delay: 2
```

## Extract Modes (9 total)

Extract modes post-process the HTTP response body.

### markdown -- HTML to clean Markdown

```yaml
- id: scrape
  fetch:
    url: "https://blog.example.com/post"
    extract: markdown
```

### article -- Main content extraction (Readability)

```yaml
- id: article
  fetch:
    url: "https://news.example.com/story"
    extract: article
```

### text -- Visible text only

```yaml
# All visible text
- id: text
  fetch:
    url: "https://example.com"
    extract: text

# Filtered by CSS selector
- id: text_filtered
  fetch:
    url: "https://example.com"
    extract: text
    selector: "main p, article p"
```

### selector -- Raw HTML of matching elements

```yaml
- id: html_parts
  fetch:
    url: "https://example.com"
    extract: selector
    selector: "h1, h2, .summary"     # Required with selector mode
```

### metadata -- OG, Twitter Cards, JSON-LD, SEO

```yaml
- id: meta
  fetch:
    url: "https://example.com/page"
    extract: metadata
# Returns JSON: { og: {...}, twitter: {...}, jsonLd: [...], seo: {...} }
```

### links -- Rich link classification

```yaml
- id: links
  fetch:
    url: "https://example.com"
    extract: links
# Returns JSON: internal/external links with nav/content/footer classification
```

### jsonpath -- Query JSON API responses

```yaml
- id: users
  fetch:
    url: "https://api.example.com/users"
    extract: jsonpath
    selector: "$.data[*].name"        # JSONPath expression in selector field
```

### feed -- RSS/Atom/JSON Feed parsing

```yaml
- id: rss
  fetch:
    url: "https://blog.example.com/feed.xml"
    extract: feed
# Returns structured feed data: title, entries, dates, etc.
```

### llm_txt -- AI-era content discovery

```yaml
- id: llm
  fetch:
    url: "https://example.com"
    extract: llm_txt
# Looks for /.well-known/llm.txt or /llms.txt
```

## Response Modes

### Default (raw body text)

```yaml
- id: raw
  fetch:
    url: "https://api.example.com/data"
# Output: raw response body as string
```

### full -- Complete response metadata

```yaml
- id: full
  fetch:
    url: "https://api.example.com/data"
    response: full
# Output: JSON { status: 200, headers: {...}, body: "...", url: "..." }
```

### binary -- Store in CAS (for media pipeline)

```yaml
- id: download
  fetch:
    url: "https://example.com/image.png"
    response: binary
# Output: CAS hash for use with nika:* media tools
```

## Patterns

### API Integration

```yaml
tasks:
  - id: fetch_data
    fetch:
      url: "https://api.github.com/repos/{{inputs.owner}}/{{inputs.repo}}"
      headers:
        Authorization: "Bearer {{$env.GITHUB_TOKEN}}"
        Accept: "application/vnd.github.v3+json"

  - id: summarize
    depends_on: [fetch_data]
    with:
      repo: $fetch_data
    infer: "Summarize this repository: {{with.repo}}"
```

### Web Scraping Pipeline

```yaml
tasks:
  - id: scrape
    fetch:
      url: "{{inputs.url}}"
      extract: article

  - id: analyze
    depends_on: [scrape]
    with:
      content: $scrape
    infer: "Extract key points from: {{with.content}}"
    structured:
      schema:
        type: object
        properties:
          key_points:
            type: array
            items: { type: string }
        required: [key_points]
```

### Multi-URL Scraping

```yaml
tasks:
  - id: scrape_all
    for_each:
      - "https://example.com/page1"
      - "https://example.com/page2"
      - "https://example.com/page3"
    as: url
    fetch:
      url: "{{with.url}}"
      extract: markdown
    timeout: 30
    retry:
      max_attempts: 2
```

### Download Binary + Process

```yaml
tasks:
  - id: download
    fetch:
      url: "https://example.com/photo.jpg"
      response: binary

  - id: thumbnail
    depends_on: [download]
    with:
      img: $download
    invoke:
      tool: "nika:thumbnail"
      params:
        hash: "{{with.img.media[0].hash}}"
        width: 200
```

## Common Mistakes

| Mistake | Correct |
|---------|---------|
| `extract: selector` without `selector:` field | `selector:` is required with `extract: selector` |
| Using `body:` and `json:` together | Use only one (json auto-sets Content-Type) |
| `method: get` (lowercase) | Use uppercase: `method: GET` |
| Expecting JSON parse with default response | Use `extract: jsonpath` or parse in binding |
| No `timeout:` on slow APIs | Always set `timeout:` for external requests |
| No `retry:` on flaky APIs | Add `retry: { max_attempts: 3 }` |

## Validation

```bash
nika check workflow.nika.yaml    # Catches missing url, bad method, etc.
nika run workflow.nika.yaml      # Test actual HTTP requests
```
