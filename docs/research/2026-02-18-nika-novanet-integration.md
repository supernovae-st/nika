# Nika + NovaNet Integration Research

**Date:** 2026-02-18
**Status:** Research Complete

---

## Executive Summary

Nika v0.2 with `invoke:` and `agent:` verbs enables intelligent YAML workflows that leverage NovaNet's knowledge graph as persistent memory. The integration uses MCP (Model Context Protocol) for zero-Cypher workflow design.

---

## NovaNet MCP Server

### 7 Tools

| Tool | Purpose | Input | Output |
|------|---------|-------|--------|
| `novanet_describe` | Schema/entity/locale discovery | `target`, `filters` | JSON schema or list |
| `novanet_search` | Fulltext/property/hybrid search | `query`, `class`, `limit` | Matching nodes |
| `novanet_traverse` | Graph traversal (1-5 hops) | `start`, `arcs`, `depth` | Subgraph |
| `novanet_assemble` | Context assembly with budget | `roots`, `token_budget` | Packed context |
| `novanet_atoms` | Knowledge atoms (Terms, Expressions) | `locale`, `domain` | Atoms list |
| `novanet_generate` | Full generation context | `mode`, `page_key`, `block_key`, `locale` | Complete context |
| `novanet_query` | Read-only Cypher (advanced) | `cypher` | Query results |

### 4 Resources

| URI Pattern | Description |
|-------------|-------------|
| `entity://{key}` | Entity + EntityNative data |
| `class://{name}` | Node class schema |
| `locale://{key}` | Locale configuration |
| `view://{id}` | Registered view definition |

### 6 Prompts

| Name | Use Case |
|------|----------|
| `cypher_query` | Generate valid Cypher |
| `cypher_explain` | Explain Cypher query |
| `block_generation` | Block content context |
| `page_generation` | Full page context |
| `entity_analysis` | Entity deep-dive |
| `locale_briefing` | Locale cultural guide |

---

## Key Architectural Concepts

### RLM-on-KG (Recursive Language Model on Knowledge Graph)

NovaNet uses a recursive approach where the LLM queries the graph incrementally, building understanding through hop-by-hop traversal.

```
LLM Question → novanet_traverse → Evidence Packet → LLM Decision → Next Hop → ...
```

### Evidence Packets

Each node returns ~200 bytes of evidence:
- `key`, `display_name`, `description`
- `denomination_forms` (for entities)
- Relevant arc counts

### Token Budget Management

`novanet_generate` intelligently selects what to include:
```
block mode (default 8K tokens):
├── Entity (full)
├── EntityNative (full)
├── Block (full)
├── BlockInstruction (full)
├── 10-20 Terms (filtered by domain)
├── 5-10 Expressions
└── Brand context (abbreviated)
```

---

## denomination_forms (ADR-033)

**ABSOLUTE RULE**: LLM MUST use ONLY denomination_forms values when referring to entities.

### Types

| Type | Where Used | Example (es-MX) |
|------|-----------|-----------------|
| `text` | Prose, body | `"codigo qr"` |
| `title` | H1, H2, meta_title | `"Codigo QR"` |
| `abbrev` | After first mention | `"qr"` |
| `mixed` | Native script locales | `"QR码"` (zh-CN) |
| `base` | International reference | `"QR Code"` |
| `url` | URL segment | `"crear-codigo-qr"` |

### Enforcement

```yaml
# In EntityNative context returned by novanet_generate
denomination_forms:
  - { type: text, value: "codigo qr", priority: 1 }
  - { type: title, value: "Codigo QR", priority: 1 }
  - { type: abbrev, value: "qr", priority: 1 }
  - { type: url, value: "crear-codigo-qr", priority: 1 }
```

The LLM receives these forms and MUST use them exactly. No paraphrasing.

---

## @ Reference System

### Injection (LLM Context)

| Syntax | Effect |
|--------|--------|
| `@entity:X` | Inject EntityNative(X@locale) |
| `@entity:X.field` | Inject specific field |
| `@project` | Inject ProjectNative |
| `@brand` | Inject Brand (soul, pitch, voice) |
| `@brand.design` | Inject BrandDesign |
| `@term:X` | Inject Term(X@locale) |
| `@seo:X` | Inject SEOKeyword |

### Links (HTML Output)

| Syntax | Result |
|--------|--------|
| `[@page:X]` | `<a href="/X">{page.title}</a>` |
| `[@page:X\|@entity:Y]` | `<a href="/X">{entity.name}</a>` |

---

## Intelligent Workflow Pattern

### Multi-Locale Generation (20 locales)

```yaml
schema: "nika/workflow@0.2"

mcp:
  novanet:
    command: "cargo"
    args: ["run", "-p", "novanet-mcp"]

tasks:
  # Phase 1: Discovery
  - id: get_page
    invoke:
      mcp: novanet
      tool: novanet_describe
      params:
        target: page
        filters: { key: homepage }

  - id: get_locales
    invoke:
      mcp: novanet
      tool: novanet_describe
      params:
        target: locales
        filters: { status: active }

  # Phase 2: Parallel locale generation (20x)
  - id: generate_locales
    for_each:
      locale: get_locales.items
    use:
      page: get_page
    agent:
      prompt: |
        Generate native content for {{use.page.key}} in {{locale.key}}.
        Use novanet_generate to get full context.
        Follow denomination_forms EXACTLY.
      mcp: [novanet]
      max_turns: 10
    output:
      format: json

flows:
  - source: [get_page, get_locales]
    target: generate_locales
```

### Single Block (Simple Case)

```yaml
tasks:
  - id: get_context
    invoke:
      mcp: novanet
      tool: novanet_generate
      params:
        mode: block
        page_key: homepage
        block_key: hero
        locale: fr-FR

  - id: generate
    use:
      ctx: get_context
    infer:
      prompt: |
        Generate hero block content.
        Context: {{use.ctx}}
        Use ONLY denomination_forms values.
    output:
      format: json
```

---

## What Nika v0.2 Needs

### New Verbs

| Verb | Purpose |
|------|---------|
| `invoke:` | MCP tool/resource call |
| `agent:` | Agentic loop with MCP tools |

### New Features

| Feature | Description |
|---------|-------------|
| `for_each:` | Parallel iteration |
| `guard:` | Conditional execution |
| `output.schema:` | JSON Schema validation |

### MCP Configuration

```yaml
mcp:
  novanet:
    command: "cargo"
    args: ["run", "-p", "novanet-mcp"]
    env:
      NEO4J_URI: "bolt://localhost:7687"
```

---

## Architecture Diagram

```
┌───────────────────────────────────────────────────────────────────┐
│                        NIKA v0.2                                  │
│                    "Corps" (Body)                                 │
├───────────────────────────────────────────────────────────────────┤
│                                                                   │
│  ┌─────────────┐    ┌─────────────┐    ┌─────────────┐           │
│  │   infer:    │    │   exec:     │    │   fetch:    │           │
│  │  (LLM call) │    │  (shell)    │    │   (HTTP)    │           │
│  └─────────────┘    └─────────────┘    └─────────────┘           │
│                                                                   │
│  ┌─────────────────────────────────────────────────┐ NEW v0.2    │
│  │   invoke:           │         agent:            │             │
│  │   (MCP tool/resource)│   (agentic loop + MCP)   │             │
│  └─────────────────────────────────────────────────┘             │
│                           │                                       │
│                           │ MCP Protocol                          │
│                           ▼                                       │
├───────────────────────────────────────────────────────────────────┤
│                      NOVANET MCP                                  │
│                   "Cerveau" (Brain)                               │
├───────────────────────────────────────────────────────────────────┤
│                                                                   │
│  novanet_describe │ novanet_search │ novanet_traverse             │
│  novanet_assemble │ novanet_atoms  │ novanet_generate             │
│                                                                   │
│                           │                                       │
│                           ▼                                       │
│  ┌─────────────────────────────────────────────────────────────┐ │
│  │                      NEO4J                                   │ │
│  │              61 nodes, 182 arcs                              │ │
│  │     Entity, EntityNative, Page, Block, Locale...             │ │
│  └─────────────────────────────────────────────────────────────┘ │
│                                                                   │
└───────────────────────────────────────────────────────────────────┘
```

---

## Key Insight

**ZERO CYPHER in Nika workflows.**

NovaNet MCP provides semantic tools (`novanet_generate`, `novanet_traverse`) that handle all graph complexity. The workflow author thinks in terms of:
- "Get generation context for this block"
- "Search for entities matching this query"
- "Traverse from this entity to related concepts"

Not:
- "MATCH (e:Entity)-[:HAS_NATIVE]->..."

---

## References

- ADR-028: Page-Entity Architecture
- ADR-029: *Native Pattern
- ADR-030: Slug Ownership
- ADR-033: denomination_forms
- NovaNet MCP: `/tools/novanet-mcp/`
