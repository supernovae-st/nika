---
name: nika-media
description: >-
  Media pipeline expert for Nika YAML workflows (.nika.yaml). Content-addressable
  storage (CAS), image processing with nika:import/thumbnail/convert/optimize,
  vision workflows, binary fetch, chart generation, PDF extraction, QR validation,
  and C2PA provenance. Use when building image/media processing pipelines in
  Nika workflows (schema nika/workflow@0.12).
---

# Nika Media Pipeline Expert

Build image and media processing workflows using CAS (content-addressable storage) and 24 builtin `nika:*` tools.

## Core Concept: CAS (Content-Addressable Storage)

All media in Nika is stored by content hash. You never work with file paths directly after import -- you work with hashes. This ensures deduplication and integrity.

```
File → nika:import → hash → nika:thumbnail → new hash → artifact
```

## Import: Getting Media Into CAS

### From Local File

```yaml
- id: import
  invoke:
    tool: "nika:import"
    params:
      path: "./photos/original.jpg"
# Output includes hash for downstream use
```

### From URL (Binary Fetch)

```yaml
- id: download
  fetch:
    url: "https://example.com/image.png"
    response: binary
# Output: CAS hash in media[0].hash
```

## Tool Reference (3 Tiers)

### Tier 1: Always Available

```yaml
# Import file
- id: import
  invoke:
    tool: "nika:import"
    params:
      path: "./image.jpg"

# Get dimensions (fast, header-only)
- id: dims
  depends_on: [import]
  with: { img: $import }
  invoke:
    tool: "nika:dimensions"
    params:
      hash: "{{with.img.hash}}"

# Generate placeholder
- id: placeholder
  depends_on: [import]
  with: { img: $import }
  invoke:
    tool: "nika:thumbhash"
    params:
      hash: "{{with.img.hash}}"

# Extract colors
- id: colors
  depends_on: [import]
  with: { img: $import }
  invoke:
    tool: "nika:dominant_color"
    params:
      hash: "{{with.img.hash}}"
      count: 5

# Chain operations (zero intermediate files)
- id: pipeline
  depends_on: [import]
  with: { img: $import }
  invoke:
    tool: "nika:pipeline"
    params:
      hash: "{{with.img.hash}}"
      ops:
        - resize: { width: 800 }
        - convert: { format: webp }
```

### Tier 2: Default Features

```yaml
# Resize
- id: thumb
  invoke:
    tool: "nika:thumbnail"
    params:
      hash: "{{with.img.hash}}"
      width: 300
      height: 300

# Convert format
- id: webp
  invoke:
    tool: "nika:convert"
    params:
      hash: "{{with.img.hash}}"
      format: webp                   # png | jpeg | webp

# Strip metadata
- id: clean
  invoke:
    tool: "nika:strip"
    params:
      hash: "{{with.img.hash}}"

# Read metadata
- id: meta
  invoke:
    tool: "nika:metadata"
    params:
      hash: "{{with.img.hash}}"

# Optimize PNG
- id: opt
  invoke:
    tool: "nika:optimize"
    params:
      hash: "{{with.img.hash}}"

# Render SVG to PNG
- id: render
  invoke:
    tool: "nika:svg_render"
    params:
      hash: "{{with.svg.hash}}"
      width: 1200
```

### Tier 3: Opt-In

```yaml
# Perceptual hash
- id: phash
  invoke:
    tool: "nika:phash"
    params:
      hash: "{{with.img.hash}}"

# Compare images
- id: diff
  invoke:
    tool: "nika:compare"
    params:
      hash_a: "{{with.img1.hash}}"
      hash_b: "{{with.img2.hash}}"

# Extract PDF text
- id: pdf
  invoke:
    tool: "nika:pdf_extract"
    params:
      hash: "{{with.doc.hash}}"

# Generate chart
- id: chart
  invoke:
    tool: "nika:chart"
    params:
      type: bar                      # bar | line | pie
      data:
        labels: ["Q1", "Q2", "Q3"]
        values: [100, 150, 120]

# C2PA provenance
- id: sign
  invoke:
    tool: "nika:provenance"
    params:
      hash: "{{with.img.hash}}"

# QR code validation
- id: qr
  invoke:
    tool: "nika:qr_validate"
    params:
      hash: "{{with.qr.hash}}"
```

## Patterns

### Photo Processing Pipeline

```yaml
schema: nika/workflow@0.12
workflow: photo-pipeline

artifacts:
  dir: ./output

tasks:
  - id: import
    invoke:
      tool: "nika:import"
      params:
        path: "{{inputs.photo}}"

  - id: strip
    depends_on: [import]
    with: { img: $import }
    invoke:
      tool: "nika:strip"
      params:
        hash: "{{with.img.hash}}"

  - id: sizes
    depends_on: [strip]
    for_each:
      - { w: 1200, name: "large" }
      - { w: 600, name: "medium" }
      - { w: 200, name: "thumb" }
    as: size
    with: { clean: $strip }
    invoke:
      tool: "nika:thumbnail"
      params:
        hash: "{{with.clean.hash}}"
        width: "{{with.size.w}}"
```

### Vision Analysis

```yaml
schema: nika/workflow@0.12
workflow: vision-pipeline
model: gpt-4o

tasks:
  - id: import
    invoke:
      tool: "nika:import"
      params:
        path: "./scene.jpg"

  - id: describe
    depends_on: [import]
    with: { img: $import }
    infer:
      content:
        - type: image
          source: "{{with.img.media[0].hash}}"
          detail: high
        - type: text
          text: "Describe everything you see in this image"
    provider: openai
    max_tokens: 500
```

### Download + Process + Describe

```yaml
tasks:
  - id: fetch
    fetch:
      url: "https://example.com/photo.jpg"
      response: binary

  - id: optimize
    depends_on: [fetch]
    with: { img: $fetch }
    invoke:
      tool: "nika:pipeline"
      params:
        hash: "{{with.img.media[0].hash}}"
        ops:
          - resize: { width: 800 }
          - convert: { format: webp }

  - id: describe
    depends_on: [fetch]
    with: { img: $fetch }
    infer:
      content:
        - type: image
          source: "{{with.img.media[0].hash}}"
        - type: text
          text: "What is in this image?"
    provider: openai
    model: gpt-4o
```

## Common Mistakes

| Mistake | Correct |
|---------|---------|
| Using file paths after import | Use CAS hash from import output |
| `response: binary` then treating as text | Binary fetch returns hash, not content |
| Forgetting `depends_on:` for hash binding | Always add when referencing imported media |
| `nika:thumbnail` without import first | Media must be in CAS before processing |
| Not quoting `tool: "nika:*"` | YAML colons need quoting |

## Validation

```bash
nika check workflow.nika.yaml    # Validates invoke syntax
nika run workflow.nika.yaml      # Tests actual media processing
```
