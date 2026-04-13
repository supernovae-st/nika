# Advanced Track -- Levels 9 through 12

> Web extraction mastery, MCP integration, media pipelines, and full-stack orchestration.

---

## Level 9 -- Data Heist

> *"Their data, your terms."*

### What You Will Learn

Level 9 unlocks the full power of the `fetch:` verb with 9 extract modes that turn raw HTTP responses into structured, usable data. You will scrape web pages to Markdown, parse RSS feeds, download binaries, and query JSON APIs -- all without external tools.

### Concept: Extract Modes

The `extract:` field on `fetch:` specifies how to post-process the HTTP response body. Nine modes are available:

| Mode | What It Does | Feature |
|------|-------------|---------|
| `markdown` | Clean Markdown from HTML via htmd | fetch-markdown |
| `article` | Main article content via Readability | fetch-article |
| `text` | Visible text, optionally filtered by CSS selector | fetch-html |
| `selector` | Raw HTML of matching CSS elements | fetch-html |
| `metadata` | OpenGraph, Twitter Cards, JSON-LD, SEO tags | fetch-html |
| `links` | Rich link classification (internal/external, nav/content) | fetch-html |
| `jsonpath` | JSONPath query on JSON API responses | zero deps |
| `feed` | RSS/Atom/JSON Feed parsing | fetch-feed |
| `llm_txt` | AI-era content discovery (llms.txt) | zero deps |

### Concept: Response Modes

The `response:` field controls the shape of the output:

```yaml
# Default: raw body text
- id: raw
  fetch: "https://example.com"

# Full response with status, headers, body
- id: full
  fetch:
    url: "https://example.com"
    response: full

# Binary: store in CAS, return hash
- id: download
  fetch:
    url: "https://example.com/image.png"
    response: binary
```

The `binary` mode is critical for the media pipeline -- it downloads files into CAS (Content-Addressable Storage) and returns a hash reference.

### Concept: CSS Selector Extraction

The `selector` extract mode uses CSS selectors to target specific HTML elements:

```yaml
- id: get_titles
  fetch:
    url: "https://news.ycombinator.com"
    extract: selector
    selector: "a.titlelink"
```

The `text` mode also accepts `selector:` to filter visible text to specific elements.

### Concept: Metadata Extraction

The `metadata` mode extracts structured SEO and social sharing data:

```yaml
- id: seo_data
  fetch:
    url: "https://example.com"
    extract: metadata
```

Returns a JSON object with OpenGraph tags, Twitter Cards, JSON-LD structured data, canonical URLs, and meta descriptions.

### Exercise 1: Fetch Markdown

**Objective**: Convert a web page to clean Markdown.

```yaml
schema: "nika/workflow@0.12"
workflow: fetch-markdown

tasks:
  - id: scrape
    fetch:
      url: "https://example.com"
      extract: markdown

  - id: display
    depends_on: [scrape]
    with:
      content: $scrape
    exec:
      command: echo "{{with.content | length}} chars of markdown extracted"
      shell: true
```

**Key takeaway**: `extract: markdown` turns any web page into clean, LLM-friendly Markdown. No BeautifulSoup, no cheerio, no dependencies.

### Exercise 2: Fetch Metadata

**Objective**: Extract OpenGraph and SEO metadata from multiple URLs in parallel.

<details>
<summary>Solution</summary>

```yaml
schema: "nika/workflow@0.12"
workflow: fetch-metadata

tasks:
  - id: meta_example
    fetch:
      url: "https://example.com"
      extract: metadata

  - id: meta_httpbin
    fetch:
      url: "https://httpbin.org"
      extract: metadata

  - id: compare
    depends_on: [meta_example, meta_httpbin]
    with:
      site_a: $meta_example
      site_b: $meta_httpbin
    exec:
      command: |
        echo "Site A metadata: {{with.site_a}}"
        echo "Site B metadata: {{with.site_b}}"
      shell: true
```
</details>

### Exercise 3: Fetch JSONPath

**Objective**: Query a JSON API and extract specific fields with JSONPath.

<details>
<summary>Solution</summary>

```yaml
schema: "nika/workflow@0.12"
workflow: fetch-jsonpath

tasks:
  - id: query_api
    fetch:
      url: "https://httpbin.org/json"
      extract: jsonpath
      selector: "$.slideshow.title"

  - id: show_result
    depends_on: [query_api]
    with:
      title: $query_api
    exec:
      command: echo "Extracted title = {{with.title}}"
      shell: true
```
</details>

**Key takeaway**: `extract: jsonpath` with `selector:` extracts exactly the fields you need from JSON APIs. No intermediate parsing.

### Exercise 4: Fetch Binary

**Objective**: Download a binary file into CAS for media pipeline processing.

<details>
<summary>Solution</summary>

```yaml
schema: "nika/workflow@0.12"
workflow: fetch-binary

tasks:
  - id: download_image
    fetch:
      url: "https://httpbin.org/image/png"
      response: binary

  - id: check_download
    depends_on: [download_image]
    with:
      result: $download_image
    invoke:
      tool: "nika:log"
      params:
        level: "info"
        message: "Downloaded image: {{with.result}}"
```
</details>

**Key takeaway**: `response: binary` stores downloads in CAS and returns a hash. This is the bridge between web fetching and the media pipeline.

---

## Level 10 -- Open Protocol

> *"The protocol is open. The future is interoperable."*

### What You Will Learn

Level 10 introduces MCP (Model Context Protocol) -- the open standard for connecting AI models to external tools. You will configure MCP servers, call their tools from workflows, and integrate with NovaNet.

### Concept: What is MCP?

MCP is a protocol that lets AI systems discover and use external tools. Instead of hardcoding API integrations, you connect to MCP servers that expose tools with schemas. Nika acts as an MCP client.

```yaml
schema: "nika/workflow@0.12"
workflow: mcp-demo

mcp:
  servers:
    my_server:
      command: "npx"
      args: ["-y", "@modelcontextprotocol/server-filesystem", "/tmp"]
```

### Concept: Configuring MCP Servers

MCP servers are declared in the `mcp:` block at the workflow level:

```yaml
mcp:
  servers:
    # Local server (started as a subprocess)
    local_tools:
      command: "node"
      args: ["./mcp-server.js"]
      cwd: "./servers"

    # Remote server (via SSE transport)
    remote_tools:
      url: "https://mcp.example.com/sse"
```

Each server gets a namespace matching its key. Tools are called as `server_name:tool_name`.

### Concept: Calling MCP Tools

MCP tools are called with the `invoke:` verb, using the server namespace:

```yaml
tasks:
  - id: list_files
    invoke:
      tool: "my_server:list_directory"
      params:
        path: "/tmp"

  - id: read_file
    depends_on: [list_files]
    invoke:
      tool: "my_server:read_file"
      params:
        path: "/tmp/example.txt"
```

### Concept: NovaNet Integration

NovaNet is the knowledge graph that serves as Nika's "brain." Nika connects to NovaNet exclusively via MCP (the Zero Cypher Rule):

```yaml
mcp:
  servers:
    novanet:
      command: "cargo"
      args: ["run", "--", "mcp"]
      cwd: "../novanet"

tasks:
  - id: search
    invoke:
      tool: "novanet:semantic_search"
      params:
        query: "workflow orchestration patterns"
        limit: 5
```

### Exercise 1: MCP Basics

**Objective**: Configure an MCP server and call a tool.

<details>
<summary>Solution</summary>

```yaml
schema: "nika/workflow@0.12"
workflow: mcp-basics

mcp:
  servers:
    fs:
      command: "npx"
      args: ["-y", "@modelcontextprotocol/server-filesystem", "/tmp"]

tasks:
  - id: list
    invoke:
      tool: "fs:list_directory"
      params:
        path: "/tmp"

  - id: log_result
    depends_on: [list]
    with:
      files: $list
    invoke:
      tool: "nika:log"
      params:
        level: "info"
        message: "Found files: {{with.files}}"
```
</details>

**Key takeaway**: MCP servers are configured in the `mcp:` block. Tools are namespaced by server name. Nika handles connection lifecycle automatically.

### Exercise 2: MCP Tools

**Objective**: Call multiple tools from an MCP server in a single workflow.

<details>
<summary>Solution</summary>

```yaml
schema: "nika/workflow@0.12"
workflow: mcp-tools

mcp:
  servers:
    fs:
      command: "npx"
      args: ["-y", "@modelcontextprotocol/server-filesystem", "/tmp"]

tasks:
  - id: list_dir
    invoke:
      tool: "fs:list_directory"
      params:
        path: "/tmp"

  - id: create_file
    invoke:
      tool: "nika:write"
      params:
        file_path: "/tmp/nika-mcp-test.txt"
        content: "Created by Nika MCP integration test"

  - id: read_back
    depends_on: [create_file]
    invoke:
      tool: "fs:read_file"
      params:
        path: "/tmp/nika-mcp-test.txt"

  - id: verify
    depends_on: [read_back]
    with:
      content: $read_back
    invoke:
      tool: "nika:log"
      params:
        level: "info"
        message: "MCP read result: {{with.content}}"
```
</details>

**Key takeaway**: MCP tools use server namespaces (`fs:list_directory`). You can mix builtin (`nika:*`) and MCP tools in the same workflow.

### Exercise 3: MCP NovaNet

**Objective**: Integrate with the NovaNet knowledge graph via MCP.

NovaNet is Nika's companion knowledge graph. The Zero Cypher Rule means all database access goes through MCP tools -- never raw queries.

<details>
<summary>Solution</summary>

```yaml
schema: "nika/workflow@0.12"
workflow: mcp-novanet

mcp:
  servers:
    novanet:
      command: "cargo"
      args: ["run", "--", "mcp"]
      cwd: "../novanet"

tasks:
  - id: search_knowledge
    invoke:
      tool: "novanet:semantic_search"
      params:
        query: "workflow orchestration patterns"
        limit: 5

  - id: analyze
    depends_on: [search_knowledge]
    with:
      results: $search_knowledge
    infer:
      prompt: "Summarize these knowledge graph results:\n{{with.results}}"
    artifact:
      path: output/knowledge-summary.md
```
</details>

**Key takeaway**: NovaNet integration follows the same pattern as any MCP server. The `invoke:` verb abstracts all protocol details. You never write database queries in workflow files.

### Why MCP Matters

MCP solves the tool integration problem at the protocol level:

1. **Discovery**: Tools declare their schemas. Your workflow discovers capabilities at runtime.
2. **Portability**: Switch tool providers without changing your workflow.
3. **Security**: Tool permissions are scoped per server. No blanket access.
4. **Composability**: Any MCP server works with any MCP client. Nika is one of many clients.

The MCP ecosystem is growing rapidly. Tools for file systems, databases, APIs, and custom services are all accessible through the same `invoke:` verb.

---

## Level 11 -- Pixel Pirate

> *"Every pixel tells a story. Now you control the narrative."*

### What You Will Learn

Level 11 introduces the media pipeline: 24 tools for image processing, organized in 3 tiers. You will import images into CAS, generate thumbnails, extract metadata, chain operations in pipelines, and send images to vision-capable LLMs.

### Concept: Content-Addressable Storage (CAS)

Every file in Nika's media pipeline is stored by its content hash. This guarantees:
- **Deduplication**: Same content = same hash = stored once
- **Integrity**: Hash verification prevents corruption
- **Immutability**: Content never changes, only new versions are created

```yaml
tasks:
  - id: import
    invoke:
      tool: "nika:import"
      params:
        path: "./photos/landscape.jpg"
```

The output includes the CAS hash (e.g., `sha256:abc123...`), which is used to reference the file in all subsequent operations.

### Concept: Media Tool Tiers

**Tier 1 -- Always-on** (5 tools, no feature flags):
| Tool | Description |
|------|-------------|
| `nika:import` | Import any file into CAS |
| `nika:dimensions` | Image dimensions from headers (~0.1ms) |
| `nika:thumbhash` | 25-byte image placeholder |
| `nika:dominant_color` | Color palette extraction |
| `nika:pipeline` | Chain operations in-memory |

**Tier 2 -- media-core default** (6 tools):
| Tool | Description |
|------|-------------|
| `nika:thumbnail` | SIMD-accelerated resize (Lanczos3) |
| `nika:convert` | Format conversion (PNG/JPEG/WebP) |
| `nika:strip` | Remove metadata (decode + re-encode) |
| `nika:metadata` | Universal EXIF/audio/video metadata |
| `nika:optimize` | Lossless PNG optimization (oxipng) |
| `nika:svg_render` | SVG to PNG rasterization (resvg) |

**Tier 3 -- Opt-in** (13 tools):
| Tool | Description |
|------|-------------|
| `nika:phash` | Perceptual image hashing |
| `nika:compare` | Visual comparison via perceptual hash |
| `nika:pdf_extract` | PDF text extraction |
| `nika:chart` | Bar/line/pie charts from JSON data |
| `nika:provenance` | C2PA content credentials (sign) |
| `nika:verify` | C2PA manifest verification |
| `nika:qr_validate` | QR decode + scan score |
| `nika:quality` | Image quality assessment (DSSIM/SSIM) |

### Concept: The Pipeline Tool

The `nika:pipeline` tool chains multiple operations in memory without writing intermediate files:

```yaml
tasks:
  - id: process
    invoke:
      tool: "nika:pipeline"
      params:
        input: "{{with.photo_hash}}"
        operations:
          - thumbnail: { width: 256 }
          - convert: { format: webp }
          - optimize: {}
```

This is significantly faster than chaining individual tools because data stays in memory between operations.

### Concept: Vision Support

The `infer:` verb supports multimodal `content:` blocks for sending images to vision-capable LLMs:

```yaml
tasks:
  - id: describe
    infer:
      content:
        - type: image
          source: "{{with.photo.media[0].hash}}"
          detail: high
        - type: text
          text: "Describe what you see in this image."
```

CAS hashes are automatically resolved to base64 for API transmission. Supported providers: Claude, OpenAI, Mistral, Groq, Gemini, xAI. Not supported: DeepSeek.

### Exercise 1: Media Import

**Objective**: Import an image into CAS and extract its dimensions.

<details>
<summary>Solution</summary>

```yaml
schema: "nika/workflow@0.12"
workflow: media-import

tasks:
  - id: download
    fetch:
      url: "https://httpbin.org/image/png"
      response: binary

  - id: get_dims
    depends_on: [download]
    with:
      img: $download
    invoke:
      tool: "nika:dimensions"
      params:
        hash: "{{with.img.media[0].hash}}"

  - id: get_thumbhash
    depends_on: [download]
    with:
      img: $download
    invoke:
      tool: "nika:thumbhash"
      params:
        hash: "{{with.img.media[0].hash}}"

  - id: log_info
    depends_on: [get_dims, get_thumbhash]
    with:
      dims: $get_dims
      thumb: $get_thumbhash
    invoke:
      tool: "nika:log"
      params:
        level: "info"
        message: "Dimensions: {{with.dims}}, ThumbHash: {{with.thumb}}"
```
</details>

### Exercise 2: Media Transform

**Objective**: Generate a thumbnail and optimize it.

<details>
<summary>Solution</summary>

```yaml
schema: "nika/workflow@0.12"
workflow: media-transform

tasks:
  - id: download
    fetch:
      url: "https://httpbin.org/image/png"
      response: binary

  - id: thumbnail
    depends_on: [download]
    with:
      img: $download
    invoke:
      tool: "nika:thumbnail"
      params:
        hash: "{{with.img.media[0].hash}}"
        width: 256

  - id: optimize
    depends_on: [thumbnail]
    with:
      thumb: $thumbnail
    invoke:
      tool: "nika:optimize"
      params:
        hash: "{{with.thumb.hash}}"
    artifact:
      path: output/optimized-thumbnail.png
```
</details>

### Exercise 3: Media Pipeline

**Objective**: Chain operations with `nika:pipeline` for in-memory processing.

### Exercise 4: Vision

**Objective**: Send an image to a vision-capable LLM and get a description.

<details>
<summary>Solution</summary>

```yaml
schema: "nika/workflow@0.12"
workflow: vision-demo
provider: anthropic
model: claude-sonnet-4-20250514

tasks:
  - id: download
    fetch:
      url: "https://httpbin.org/image/png"
      response: binary

  - id: describe
    depends_on: [download]
    with:
      img: $download
    infer:
      content:
        - type: image
          source: "{{with.img.media[0].hash}}"
          detail: high
        - type: text
          text: "Describe this image in detail. What colors, shapes, and patterns do you see?"
      max_tokens: 500
```
</details>

**Key takeaway**: Vision workflows combine `fetch: response: binary` for download, CAS for storage, and `infer: content:` for multimodal prompting.

---

## Level 12 -- SuperNovae

> *"You are the orchestrator. Everything listens."*

### What You Will Learn

Level 12 is the boss level. You must combine all 5 verbs, all binding patterns, all tool categories, and all orchestration techniques into production-grade workflows. There are 5 exercises, and all must pass.

### Exercise 1: SEO Mega Audit

Combine `fetch: extract: metadata` with `fetch: extract: links`, feed results to `infer:` for analysis, and save structured output as artifacts. This tests: fetch extraction, DAG parallelism, structured output, artifacts.

### Exercise 2: Image Pipeline

Download images with `fetch: response: binary`, process them with media tools (`nika:thumbnail`, `nika:optimize`), describe them with vision (`infer: content:`), and chain everything through an agent that generates alt-text. This tests: media pipeline, vision, agents.

### Exercise 3: Content Factory

Build a multi-stage content generation system: research with `fetch:`, outline with `infer:`, write sections in parallel with `for_each:`, assemble with a merge task, and publish with `nika:write`. This tests: for_each, DAG, multi-provider, file tools.

### Exercise 4: Research Agent

Create an autonomous research agent that scrapes websites (`fetch: extract: markdown`), searches for patterns (`nika:grep`), analyzes findings with LLMs, and produces a structured report with guardrails. This tests: agents, guardrails, fetch extraction, file tools.

### Exercise 5: Full Stack

The ultimate test. A single workflow that uses every verb (`exec:`, `fetch:`, `infer:`, `invoke:`, `agent:`), every binding pattern (`with:`, `$env`, `inputs:`), every output format (text, JSON, binary, artifacts), and every orchestration technique (DAG, for_each, agents, sub-workflows). This tests: everything.

---

## Phase 3 Checkpoint

After completing Levels 9-12, you should be able to:

- Use all 9 fetch extract modes for web scraping and data extraction
- Download binary files into CAS with `response: binary`
- Configure and connect to MCP servers
- Call external tools via MCP from workflows
- Use all 24 media tools across 3 tiers
- Chain media operations with `nika:pipeline`
- Send images to vision-capable LLMs
- Build production-grade workflows that combine all 5 verbs
- Design complex multi-stage orchestration patterns

### Checkpoint Project: Competitive Intelligence Suite

Build a complete competitive intelligence system:
1. Accept 3 competitor URLs via `inputs:`
2. Scrape each URL in parallel with `fetch: extract: markdown` and `fetch: extract: metadata`
3. Extract key data with an agent using guardrails (structured JSON output)
4. Generate comparison charts with `nika:chart`
5. Produce a comprehensive HTML report with `nika:write`
6. Create a one-page executive summary with `infer:` and `output: format: json_schema`

This project exercises every major feature of the engine in a single, practical workflow.

---

*"They serialize everything so the meter runs longer. You parallelize because you respect your own time."*
