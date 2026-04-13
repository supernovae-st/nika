# The fetch: Verb -- Complete Guide

The `fetch:` verb makes HTTP requests and supports powerful content extraction. It can fetch raw data, convert HTML to Markdown, extract article content, parse RSS feeds, query JSON APIs, extract metadata, classify links, and more. This guide covers all 9 extract modes, response modes, and practical patterns.

## Basic Usage

The simplest `fetch:` task needs just a URL:

```yaml
schema: nika/workflow@0.12
workflow: basic-fetch

tasks:
  - id: get_page
    fetch:
      url: "https://example.com"
```

This makes a GET request and returns the raw response body as the task output.

### String Shorthand

```yaml
  - id: get_page
    fetch: "https://example.com"
```

## All fetch: Fields

```yaml
- id: my_fetch
  fetch:
    # Core
    url: "https://api.example.com/data"       # Required
    method: POST                              # GET | POST | PUT | DELETE (default: GET)

    # Headers
    headers:
      Authorization: "Bearer {{with.token}}"
      Accept: application/json
      Content-Type: application/json

    # Request body (choose one)
    json:                                     # Body as JSON (auto sets Content-Type)
      query: "{{with.search_term}}"
      limit: 10
    body: "raw string body"                   # Alternative: raw body string

    # Behavior
    timeout: 30                               # Seconds (default: 30)
    follow_redirects: true                    # Follow HTTP redirects (default: true)

    # Post-processing
    extract: markdown                         # Extraction mode (9 options)
    selector: "div.content"                   # CSS selector or JSONPath expression
    response: full                            # Response mode: full | binary
```

## HTTP Methods

### GET (Default)

```yaml
  - id: get_data
    fetch:
      url: "https://api.example.com/users"
```

### POST

```yaml
  - id: create_user
    fetch:
      url: "https://api.example.com/users"
      method: POST
      json:
        name: "Alice"
        email: "alice@example.com"
```

### PUT

```yaml
  - id: update_user
    fetch:
      url: "https://api.example.com/users/123"
      method: PUT
      json:
        name: "Alice Updated"
```

### DELETE

```yaml
  - id: delete_user
    fetch:
      url: "https://api.example.com/users/123"
      method: DELETE
```

## Headers and Authentication

### Bearer Token

```yaml
  - id: api_call
    fetch:
      url: "https://api.example.com/data"
      headers:
        Authorization: "Bearer {{with.token}}"
```

### API Key in Header

```yaml
  - id: api_call
    with:
      key: $env.API_KEY
    fetch:
      url: "https://api.example.com/data"
      headers:
        X-API-Key: "{{with.key}}"
```

### Custom Headers

```yaml
  - id: custom_request
    fetch:
      url: "https://api.example.com/data"
      headers:
        Accept: application/json
        X-Request-ID: "{{with.request_id}}"
        User-Agent: "Nika/0.49"
```

## Request Body

### JSON Body (Automatic Content-Type)

Using `json:` automatically sets `Content-Type: application/json`:

```yaml
  - id: search
    fetch:
      url: "https://api.example.com/search"
      method: POST
      json:
        query: "{{with.search_term}}"
        filters:
          category: "technology"
          date_range: "last_week"
        limit: 20
```

### Raw Body

```yaml
  - id: webhook
    fetch:
      url: "https://hooks.example.com/trigger"
      method: POST
      headers:
        Content-Type: text/plain
      body: "Deployment completed at {{with.timestamp}}"
```

## The 9 Extract Modes

Extract modes post-process HTTP responses to produce clean, structured data. They are the most powerful feature of the `fetch:` verb.

### 1. extract: markdown

Converts HTML to clean Markdown. Removes navigation, scripts, styles, and other non-content elements.

```yaml
  - id: get_article
    fetch:
      url: "https://blog.example.com/post/123"
      extract: markdown
```

**Use case:** Feeding web content to LLMs. Markdown is the most token-efficient format for AI processing.

**Output example:**
```markdown
# Article Title

This is the main content of the article...

## Section 1

Some text with **bold** and *italic* formatting.
```

### 2. extract: article

Extracts the main article content using a Readability algorithm (similar to Firefox Reader View). Strips navigation, sidebars, ads, and footers.

```yaml
  - id: read_article
    fetch:
      url: "https://news.example.com/story/456"
      extract: article
```

**Use case:** When you only want the main article body, not the full page. More aggressive than `markdown` -- removes everything except the primary content.

**Output example:**
```
Article Title

The main body of the article text, cleaned of all navigation,
advertisements, and sidebar content...
```

### 3. extract: text

Extracts visible text content. Optionally filtered by a CSS selector.

```yaml
  # All visible text
  - id: all_text
    fetch:
      url: "https://example.com"
      extract: text

  # Text from specific elements
  - id: headings_only
    fetch:
      url: "https://example.com"
      extract: text
      selector: "h1, h2, h3"
```

**Use case:** When you need plain text without formatting, or want to target specific page elements.

### 4. extract: selector

Returns the raw HTML of elements matching a CSS selector. Requires the `selector:` field.

```yaml
  - id: get_prices
    fetch:
      url: "https://shop.example.com/products"
      extract: selector
      selector: "div.product .price"
```

**Use case:** Web scraping specific HTML elements for further processing.

**Output example:**
```html
<span class="price">$29.99</span>
<span class="price">$49.99</span>
<span class="price">$19.99</span>
```

### 5. extract: metadata

Extracts page metadata: Open Graph tags, Twitter Cards, JSON-LD, and standard SEO tags.

```yaml
  - id: get_meta
    fetch:
      url: "https://blog.example.com/post/123"
      extract: metadata
```

**Output example (JSON):**
```json
{
  "title": "How to Build AI Workflows",
  "description": "A comprehensive guide to...",
  "og:image": "https://blog.example.com/images/hero.jpg",
  "og:type": "article",
  "twitter:card": "summary_large_image",
  "json_ld": {
    "@type": "Article",
    "author": "Jane Doe",
    "datePublished": "2026-03-20"
  }
}
```

**Use case:** SEO analysis, content aggregation, social media preview extraction.

### 6. extract: links

Classifies all links on a page into categories: internal/external, navigation/content/footer.

```yaml
  - id: analyze_links
    fetch:
      url: "https://example.com"
      extract: links
```

**Output example (JSON):**
```json
{
  "internal": [
    {"url": "/about", "text": "About Us", "context": "nav"},
    {"url": "/blog/post-1", "text": "Latest Post", "context": "content"}
  ],
  "external": [
    {"url": "https://twitter.com/example", "text": "@example", "context": "footer"}
  ],
  "total": 42,
  "internal_count": 30,
  "external_count": 12
}
```

**Use case:** Site auditing, broken link detection, sitemap generation.

### 7. extract: jsonpath

Queries JSON API responses using JSONPath expressions. Use the `selector:` field for the JSONPath expression.

```yaml
  - id: get_names
    fetch:
      url: "https://jsonplaceholder.typicode.com/users"
      extract: jsonpath
      selector: "$[*].name"
```

**Output example:**
```json
["Leanne Graham", "Ervin Howell", "Clementine Bauch"]
```

**JSONPath syntax examples:**

| Expression | Meaning |
|-----------|---------|
| `$` | Root element |
| `$.name` | Top-level name field |
| `$.users[0]` | First user |
| `$.users[*].name` | All user names |
| `$.data.items[?(@.active)]` | Active items |
| `$..name` | All name fields at any depth |

**Use case:** Extracting specific fields from JSON APIs without parsing the entire response.

### 8. extract: feed

Parses RSS, Atom, and JSON Feed formats into structured data.

```yaml
  - id: latest_news
    fetch:
      url: "https://blog.example.com/feed.xml"
      extract: feed
```

**Output example (JSON):**
```json
{
  "title": "Example Blog",
  "entries": [
    {
      "title": "Latest Post",
      "link": "https://blog.example.com/post/123",
      "published": "2026-03-20T10:00:00Z",
      "summary": "A brief summary..."
    }
  ]
}
```

**Use case:** Content aggregation, news monitoring, blog syndication.

### 9. extract: llm_txt

Checks for AI-era content discovery files: `/.well-known/llm.txt` and `/llms.txt`. These files tell AI systems what content is available and how to use it.

```yaml
  - id: check_ai_content
    fetch:
      url: "https://example.com"
      extract: llm_txt
```

**Use case:** Discovering which sites provide AI-friendly content descriptions.

## Response Modes

### Default (Raw Body)

When no `response:` field is set, Nika returns the raw response body as a string:

```yaml
  - id: raw
    fetch:
      url: "https://api.example.com/data"
  # Output: the raw response body text
```

### response: full

Returns a JSON object with status code, headers, body, and final URL (after redirects):

```yaml
  - id: full_response
    fetch:
      url: "https://api.example.com/data"
      response: full
```

**Output:**
```json
{
  "status": 200,
  "headers": {
    "content-type": "application/json",
    "x-request-id": "abc123"
  },
  "body": "{\"data\": [1, 2, 3]}",
  "final_url": "https://api.example.com/data"
}
```

Access individual fields in downstream tasks:

```yaml
  - id: check_status
    depends_on: [full_response]
    with:
      status: $full_response.status
      content_type: $full_response.headers.content-type
    exec: "echo 'Status: {{with.status}}, Type: {{with.content_type}}'"
```

### response: binary

Stores the response body in the Content-Addressable Store (CAS) and returns the hash. Used for downloading images, PDFs, and other binary files for the media pipeline:

```yaml
  - id: download_image
    fetch:
      url: "https://example.com/photo.jpg"
      response: binary

  - id: resize
    depends_on: [download_image]
    with:
      img: $download_image
    invoke:
      tool: "nika:thumbnail"
      params:
        hash: "{{with.img.media[0].hash}}"
        width: 200
```

## Timeouts and Redirects

### Timeout

Set per-request timeouts in seconds:

```yaml
  - id: slow_api
    fetch:
      url: "https://slow-api.example.com/compute"
      timeout: 60  # Wait up to 60 seconds
```

Default timeout is 30 seconds. If the request exceeds the timeout, the task fails with a timeout error.

### Redirect Following

```yaml
  - id: follow
    fetch:
      url: "https://short.link/abc"
      follow_redirects: true  # Default: true

  - id: no_follow
    fetch:
      url: "https://short.link/abc"
      follow_redirects: false  # Returns the 301/302 response
      response: full           # Check the Location header
```

## Practical Patterns

### API Call with Authentication

```yaml
schema: nika/workflow@0.12
workflow: api-pipeline

tasks:
  - id: get_token
    fetch:
      url: "https://auth.example.com/token"
      method: POST
      json:
        client_id: "{{with.client_id}}"
        client_secret: "{{with.secret}}"
      extract: jsonpath
      selector: "$.access_token"

  - id: fetch_data
    depends_on: [get_token]
    with:
      token: $get_token | trim
    fetch:
      url: "https://api.example.com/data"
      headers:
        Authorization: "Bearer {{with.token}}"
      extract: jsonpath
      selector: "$.results[*]"
```

### Web Scraping Pipeline

```yaml
schema: nika/workflow@0.12
workflow: scrape-and-analyze
provider: anthropic

tasks:
  - id: fetch_page
    fetch:
      url: "https://news.example.com"
      extract: markdown

  - id: fetch_meta
    fetch:
      url: "https://news.example.com"
      extract: metadata

  - id: fetch_links
    fetch:
      url: "https://news.example.com"
      extract: links

  - id: analyze
    depends_on: [fetch_page, fetch_meta, fetch_links]
    with:
      content: $fetch_page | trim
      meta: $fetch_meta | to_json
      links: $fetch_links | to_json
    infer:
      prompt: |
        Analyze this news page:

        Metadata: {{with.meta}}
        Content: {{with.content}}
        Links: {{with.links}}

        Provide: topic, bias assessment, source quality score (1-10).
```

### Parallel API Calls

```yaml
tasks:
  - id: urls
    exec: |
      echo '["https://api.example.com/users", "https://api.example.com/posts", "https://api.example.com/comments"]'
    output:
      format: json

  - id: fetch_all
    depends_on: [urls]
    for_each: $urls
    as: url
    concurrency: 3
    fetch:
      url: "{{with.url}}"
      extract: jsonpath
      selector: "$.length()"
```

### RSS Feed Aggregation

```yaml
schema: nika/workflow@0.12
workflow: feed-aggregator
provider: anthropic

tasks:
  - id: feeds
    exec: |
      echo '["https://blog1.com/feed.xml", "https://blog2.com/rss", "https://blog3.com/atom.xml"]'
    output:
      format: json

  - id: fetch_feeds
    depends_on: [feeds]
    for_each: $feeds
    concurrency: 3
    fetch:
      url: "{{with.item}}"
      extract: feed

  - id: digest
    depends_on: [fetch_feeds]
    with:
      all_feeds: $fetch_feeds | to_json
    infer:
      prompt: |
        Create a daily digest from these RSS feeds:
        {{with.all_feeds}}

        Format: Top 5 stories with one-sentence summaries.
```

### Download and Process Binary Files

```yaml
tasks:
  - id: download_pdf
    fetch:
      url: "https://example.com/report.pdf"
      response: binary

  - id: extract_text
    depends_on: [download_pdf]
    with:
      pdf: $download_pdf
    invoke:
      tool: "nika:pdf_extract"
      params:
        hash: "{{with.pdf.media[0].hash}}"

  - id: summarize
    depends_on: [extract_text]
    with:
      text: $extract_text | trim
    infer:
      prompt: "Summarize this PDF content: {{with.text}}"
```

## Error Handling

### Retry on Failure

Network requests can fail. Use retry for resilience:

```yaml
  - id: unreliable_api
    fetch:
      url: "https://flaky-api.example.com/data"
      timeout: 10
    retry:
      max_attempts: 3
      delay_ms: 2000
      backoff: 2.0  # 2s, 4s, 8s
```

### Checking Status Codes

Use `response: full` to inspect the status code:

```yaml
  - id: check_endpoint
    fetch:
      url: "https://api.example.com/health"
      response: full

  - id: verify
    depends_on: [check_endpoint]
    with:
      status: $check_endpoint.status
    invoke:
      tool: "nika:assert"
      params:
        condition: "{{with.status}} == 200"
        message: "API health check failed"
```

## Working with Authentication

### OAuth2 Token Flow

Many APIs require OAuth2 authentication. Here is how to implement a token refresh flow:

```yaml
schema: nika/workflow@0.12
workflow: oauth-flow

tasks:
  - id: get_token
    fetch:
      url: "https://auth.provider.com/oauth/token"
      method: POST
      headers:
        Content-Type: application/x-www-form-urlencoded
      body: "grant_type=client_credentials&client_id={{with.client_id}}&client_secret={{with.secret}}"
    output:
      format: json

  - id: use_api
    depends_on: [get_token]
    with:
      token: $get_token.access_token
    fetch:
      url: "https://api.provider.com/data"
      headers:
        Authorization: "Bearer {{with.token}}"
```

### API Key in Query Parameters

Some APIs expect the key as a URL parameter:

```yaml
  - id: api_with_key
    with:
      key: $env.API_KEY
    fetch:
      url: "https://api.example.com/data?api_key={{with.key}}&format=json"
```

### Basic Authentication

```yaml
  - id: basic_auth
    fetch:
      url: "https://api.example.com/data"
      headers:
        Authorization: "Basic dXNlcjpwYXNz"   # base64 of "user:pass"
```

## Combining Extract Modes in Workflows

A common pattern is to fetch the same URL with different extract modes to get multiple views of the data:

```yaml
schema: nika/workflow@0.12
workflow: full-page-analysis
provider: anthropic

tasks:
  - id: page_content
    fetch:
      url: "https://blog.example.com/article"
      extract: markdown

  - id: page_meta
    fetch:
      url: "https://blog.example.com/article"
      extract: metadata

  - id: page_links
    fetch:
      url: "https://blog.example.com/article"
      extract: links

  - id: page_article
    fetch:
      url: "https://blog.example.com/article"
      extract: article

  - id: comprehensive_analysis
    depends_on: [page_content, page_meta, page_links, page_article]
    with:
      content: $page_content | trim
      meta: $page_meta | to_json
      links: $page_links | to_json
      article: $page_article | trim
    infer:
      prompt: |
        Perform a comprehensive analysis of this webpage:

        Metadata: {{with.meta}}
        Article body: {{with.article}}
        Link structure: {{with.links}}

        Provide: content quality score (1-10), SEO assessment, and content recommendations.
      temperature: 0.3
```

All four fetch tasks run in parallel since they have no dependencies on each other.

## Webhooks and Callbacks

### Sending Webhooks

```yaml
  - id: notify_slack
    depends_on: [process]
    with:
      result: $process | trim
    fetch:
      url: "https://hooks.slack.com/services/T00/B00/webhook"
      method: POST
      json:
        text: "Workflow completed: {{with.result}}"

  - id: notify_discord
    depends_on: [process]
    with:
      result: $process | trim
    fetch:
      url: "https://discord.com/api/webhooks/123/token"
      method: POST
      json:
        content: "Result: {{with.result}}"
```

### Health Checks

```yaml
  - id: check_service
    fetch:
      url: "https://api.example.com/health"
      timeout: 5
      response: full

  - id: alert_if_down
    depends_on: [check_service]
    with:
      status: $check_service.status
    invoke:
      tool: "nika:assert"
      params:
        condition: "{{with.status}} == 200"
        message: "Service is down! Status: {{with.status}}"
```

## Extract Mode Comparison

| Mode | Input | Output | Best For |
|------|-------|--------|----------|
| `markdown` | HTML | Clean Markdown | LLM input, documentation |
| `article` | HTML | Article text | News, blog content |
| `text` | HTML | Plain text | Simple extraction |
| `selector` | HTML | HTML fragments | Scraping specific elements |
| `metadata` | HTML | JSON (OG, SEO) | SEO analysis |
| `links` | HTML | JSON (classified) | Site auditing |
| `jsonpath` | JSON | Filtered JSON | API response filtering |
| `feed` | XML/JSON | JSON (entries) | RSS/Atom aggregation |
| `llm_txt` | Text | AI content info | AI content discovery |

## Best Practices

1. **Always set timeouts** -- Network requests can hang. Default is 30s but set explicit values for reliability
2. **Use extract modes** -- Raw HTML is noisy. `extract: markdown` or `extract: article` produce much better LLM input
3. **Handle errors** -- Add `retry:` for unreliable endpoints
4. **Respect rate limits** -- Use `concurrency` in `for_each` to limit parallel requests
5. **Cache responses** -- If you fetch the same URL multiple times, fetch it once and bind to multiple tasks
6. **Use response: full** when you need to check status codes or headers
7. **Prefer json: over body:** -- The `json:` field automatically sets Content-Type and serializes properly
