---
name: nika-invoke
description: >-
  Expert at the Nika invoke: verb for MCP tool calls and builtin nika:* tools in
  .nika.yaml workflows. Covers MCP server configuration, 24 builtin media tools
  (nika:import, nika:thumbnail, nika:chart, etc.), tool parameters, and
  integration patterns. Use when building invoke: tasks in Nika YAML workflows
  (schema nika/workflow@0.12).
---

# Nika invoke: Verb Expert

The `invoke:` verb calls an MCP tool or a builtin `nika:*` tool.

## Basic Syntax

### MCP Tool Call

```yaml
- id: search
  invoke:
    tool: search_knowledge
    mcp: novanet                     # MCP server name from mcp: block
    params:
      query: "{{with.topic}}"
```

### Builtin Tool Call

```yaml
- id: thumb
  invoke:
    tool: "nika:thumbnail"           # No mcp: needed for builtins
    params:
      hash: "{{with.image_hash}}"
      width: 200
      height: 200
```

## MCP Server Configuration

Define MCP servers at workflow level:

```yaml
schema: nika/workflow@0.12
mcp:
  novanet:
    command: novanet
    args: ["mcp", "serve"]
    env:
      DATABASE_URL: "${DATABASE_URL}"   # Shell-expand syntax

  firecrawl:
    command: npx
    args: ["-y", "@anthropic/firecrawl-mcp"]
    env:
      FIRECRAWL_API_KEY: "${FIRECRAWL_API_KEY}"
```

Env vars in MCP config use `${VAR}` (shell-expand), NOT `{{$env.VAR}}`.

## Builtin Tools Reference

### Tier 1 -- Always Available (5 tools)

| Tool | Parameters | Description |
|------|-----------|-------------|
| `nika:import` | `path` | Import file into CAS, returns hash |
| `nika:dimensions` | `hash` | Image width/height from headers |
| `nika:thumbhash` | `hash` | 25-byte image placeholder hash |
| `nika:dominant_color` | `hash`, `count?` | Extract color palette |
| `nika:pipeline` | `hash`, `ops` | Chain operations in-memory |

### Tier 2 -- Default Features (6 tools)

| Tool | Parameters | Description |
|------|-----------|-------------|
| `nika:thumbnail` | `hash`, `width`, `height?`, `format?` | Resize image |
| `nika:convert` | `hash`, `format` | Convert format (PNG/JPEG/WebP) |
| `nika:strip` | `hash` | Remove EXIF metadata |
| `nika:metadata` | `hash` | Extract EXIF/audio/video metadata |
| `nika:optimize` | `hash` | Lossless PNG optimization |
| `nika:svg_render` | `hash`, `width?` | SVG to PNG rasterization |

### Tier 3 -- Opt-In (13 tools)

| Tool | Parameters | Description |
|------|-----------|-------------|
| `nika:phash` | `hash` | Perceptual image hash |
| `nika:compare` | `hash_a`, `hash_b` | Visual similarity score |
| `nika:pdf_extract` | `hash`, `pages?` | PDF text extraction |
| `nika:chart` | `type`, `data`, `width?`, `height?` | Generate chart image |
| `nika:provenance` | `hash` | C2PA content credentials (sign) |
| `nika:verify` | `hash` | C2PA verification |
| `nika:qr_validate` | `hash` | QR decode + scan score |
| `nika:quality` | `hash_a`, `hash_b?` | Image quality assessment |
| `nika:html_to_md` | `html` | HTML to Markdown |
| `nika:css_select` | `html`, `selector` | CSS selector extraction |
| `nika:extract_metadata` | `html` | OG/Twitter/JSON-LD metadata |
| `nika:extract_links` | `html`, `base_url` | Link classification |
| `nika:readability` | `html` | Article content extraction |

## Patterns

### Image Processing Pipeline

```yaml
tasks:
  - id: import
    invoke:
      tool: "nika:import"
      params:
        path: "./photo.jpg"

  - id: thumb
    depends_on: [import]
    with:
      img: $import
    invoke:
      tool: "nika:thumbnail"
      params:
        hash: "{{with.img.hash}}"
        width: 300

  - id: optimize
    depends_on: [thumb]
    with:
      resized: $thumb
    invoke:
      tool: "nika:optimize"
      params:
        hash: "{{with.resized.hash}}"
```

### Batch Image Processing

```yaml
tasks:
  - id: files
    exec: 'echo ''["photo1.jpg", "photo2.jpg", "photo3.jpg"]'''

  - id: process
    depends_on: [files]
    for_each: "$files"
    as: file
    invoke:
      tool: "nika:import"
      params:
        path: "{{with.file}}"
```

### MCP Knowledge Graph Query

```yaml
mcp:
  novanet:
    command: novanet
    args: ["mcp", "serve"]

tasks:
  - id: query
    invoke:
      tool: search_nodes
      mcp: novanet
      params:
        query: "{{inputs.search_term}}"
        limit: 10

  - id: summarize
    depends_on: [query]
    with:
      results: $query
    infer: "Summarize these findings: {{with.results}}"
```

### Chart Generation

```yaml
- id: chart
  invoke:
    tool: "nika:chart"
    params:
      type: bar
      data:
        labels: ["Q1", "Q2", "Q3", "Q4"]
        values: [100, 150, 120, 180]
      width: 800
      height: 400
```

### Download + Process (fetch binary + invoke)

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

## Zero Cypher Rule

Nika workflows NEVER use raw Cypher queries. All knowledge graph access goes through MCP `invoke:` with NovaNet tools. This is a core architectural constraint.

## Common Mistakes

| Mistake | Correct |
|---------|---------|
| `tool: nika:thumbnail` without quotes | Quote colon: `tool: "nika:thumbnail"` |
| Missing `mcp:` for MCP tools | Add `mcp: server_name` for non-builtin tools |
| Adding `mcp:` for builtin tools | Omit `mcp:` for `nika:*` tools |
| Using raw Cypher | Use `invoke:` with MCP tools instead |
| `${VAR}` in `params:` | Use `{{$env.VAR}}` in params (shell-expand is for mcp config only) |
| Missing `depends_on:` for hash binding | Add `depends_on:` when using `with:` |

## Validation

```bash
nika check workflow.nika.yaml    # Validates tool name, mcp reference
nika run workflow.nika.yaml      # Test actual tool execution
```
