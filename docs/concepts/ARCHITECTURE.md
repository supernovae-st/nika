# Nika Architecture

Schema `nika/workflow@0.12` | v0.71.0 | 17 crates | 10,000+ tests

---

## System Overview

```
  .nika.yaml
      │
      ▼
┌─────────────────────────────────────────────────────────────────────┐
│                     THREE-PHASE AST PIPELINE                        │
│                                                                     │
│   Phase 1 (Raw)       Phase 2 (Analyzed)     Phase 3 (Lower)       │
│   YAML text      →    Validated, resolved  →  Runtime types         │
│   serde-saphyr        type checking           executor-ready        │
│   marked-yaml         dependency resolution                         │
└─────────────────────────────────────────────────────────────────────┘
      │
      ▼
┌─────────────────────────────────────────────────────────────────────┐
│                         DAG VALIDATION                              │
│   IndexedDag (Kahn's topological sort) — cycle detection            │
│   petgraph StableGraph for TUI visualization                        │
└─────────────────────────────────────────────────────────────────────┘
      │
      ▼
┌─────────────────────────────────────────────────────────────────────┐
│                     EXECUTOR (parallel tasks)                       │
│                                                                     │
│   infer: ──→  rig-core (7 cloud providers)                          │
│               native (GGUF via mistral.rs)                          │
│                                                                     │
│   exec:  ──→  Shell (blocklist + NFKC normalization)                │
│                                                                     │
│   fetch: ──→  reqwest 0.13 (9 extract modes)                        │
│                                                                     │
│   invoke:──→  MCP client (rmcp 0.16) ──→ NovaNet                    │
│                                                                     │
│   agent: ──→  Multi-turn loop (guardrails + completion modes)       │
└─────────────────────────────────────────────────────────────────────┘
      │
      ▼
  NDJSON traces (EventLog, 41 event types)
```

---

## Workspace Crates

| Crate | Lines | Description |
|:------|------:|:------------|
| `nika` | ~2k | Binary entry point |
| `nika-engine` | ~135k | Execution engine — embeddable runtime |
| `nika-core` | ~23k | AST, types, catalogs — zero I/O |
| `nika-event` | ~4k | EventLog, TraceWriter |
| `nika-mcp` | ~9k | MCP client + rmcp |
| `nika-media` | ~13k | CAS store + processor |
| `nika-cli` | ~8k | CLI subcommands |
| `nika-tui` | ~86k | Terminal UI — ratatui |
| `nika-daemon` | ~5k | Background daemon |
| `nika-init` | ~21k | Project scaffolding |
| `nika-lsp-core` | ~9k | LSP intelligence |
| `nika-lsp` | ~2.5k | LSP binary |
| `nika-display` | ~12k | CLI display renderers |
| `nika-sdk` | ~3k | Rust SDK |
| `nika-serve` | ~4k | HTTP server |
| `nika-storage` | ~1k | Storage abstraction |
| `nika-vault` | ~1.2k | Encrypted credential store |

---

## 3-Phase AST Pipeline

### Phase 1 — Raw

YAML text is parsed into a Raw AST using `serde-saphyr` with YAML bomb protection and `marked-yaml` for source span tracking. No validation occurs at this phase — the goal is faithful representation of the user's input with location info for error messages.

### Phase 2 — Analyzed

The Analyzer transforms Raw AST into an Analyzed AST:
- Schema version validation
- `with:` binding resolution and `$task_id` reference checks
- `depends_on:` dependency graph construction
- Type checking on all verb fields
- Provider and model validation against catalogs

### Phase 3 — Lower

The Lowered AST converts Analyzed nodes into runtime execution types. After this phase, no validation occurs. Tasks are ready to be scheduled by the DAG executor.

```
Raw → Analyzed → Lower   (never skip phases)
```

---

## 5 Verbs

| Verb | Purpose | Notes |
|:-----|:--------|:------|
| `infer:` | LLM generation | Vision, extended thinking, structured output, guardrails |
| `exec:` | Shell command | `shell: false` default, blocklist, timeout, NFKC normalization |
| `fetch:` | HTTP request | 9 extract modes: markdown, article, text, selector, metadata, links, jsonpath, feed, llm_txt |
| `invoke:` | MCP tool call | Schema validation, retry, connection pooling |
| `agent:` | Multi-turn loop | Guardrails, completion modes, stop_sequences, HITL |

---

## Providers

| Provider | Key | Default Model |
|:---------|:----|:--------------|
| `anthropic` | `ANTHROPIC_API_KEY` | claude-sonnet-4-20250514 |
| `openai` | `OPENAI_API_KEY` | gpt-4o |
| `mistral` | `MISTRAL_API_KEY` | mistral-large-latest |
| `groq` | `GROQ_API_KEY` | llama-4-maverick |
| `deepseek` | `DEEPSEEK_API_KEY` | deepseek-chat |
| `gemini` | `GEMINI_API_KEY` | gemini-2.5-flash |
| `xai` | `XAI_API_KEY` | grok-3 |
| `native` | (none) | Local GGUF via mistral.rs |

All cloud providers route through `rig-core 0.33`. Native uses `mistral.rs` for local GGUF inference and `NativeModelKind::VisionHf` for vision models.

---

## TUI Architecture

3 views, navigated by key (`1`/`s`, `2`/`c`, `3`/`x`):

```
┌─────────────────────────────────────────────────────────────────────┐
│  [1] Studio      Workflow browser + YAML editor + DAG preview       │
│  [2] Command     Execution monitor + chat interface                 │
│  [3] Control     Provider config + theme + preferences              │
└─────────────────────────────────────────────────────────────────────┘
```

Built on `ratatui`. ~86k lines across ~40+ widgets. Event loop driven by `crossterm`.

---

## Builtin Tools (24 total)

### Tier 1 — Always-on (5 tools)

| Tool | Description |
|:-----|:------------|
| `nika:import` | Import any file into CAS |
| `nika:dimensions` | Image dimensions from headers (~0.1ms) |
| `nika:thumbhash` | 25-byte image placeholder |
| `nika:dominant_color` | Color palette extraction |
| `nika:pipeline` | Chain operations in-memory (zero intermediate files) |

### Tier 2 — media-core default (6 tools)

| Tool | Feature | Description |
|:-----|:--------|:------------|
| `nika:thumbnail` | `media-thumbnail` | SIMD-accelerated resize (Lanczos3) |
| `nika:convert` | `media-thumbnail` | Format conversion (PNG/JPEG/WebP) |
| `nika:strip` | `media-thumbnail` | Remove metadata (decode + re-encode) |
| `nika:metadata` | `media-metadata` | Universal EXIF/audio/video metadata |
| `nika:optimize` | `media-optimize` | Lossless PNG optimization (oxipng) |
| `nika:svg_render` | `media-svg` | SVG to PNG rasterization (resvg) |

### Tier 3 — Opt-in (13 tools)

| Tool | Feature | Description |
|:-----|:--------|:------------|
| `nika:phash` | `media-phash` | Perceptual image hashing |
| `nika:compare` | `media-phash` | Visual comparison via perceptual hash |
| `nika:pdf_extract` | `media-pdf` | PDF text extraction |
| `nika:chart` | `media-chart` | Bar/line/pie charts from JSON data |
| `nika:provenance` | `media-provenance` | C2PA content credentials (sign) |
| `nika:verify` | `media-provenance` | C2PA manifest verification + EU AI Act compliance |
| `nika:qr_validate` | `media-qr` | QR decode + 0-100 scan score |
| `nika:quality` | `media-iqa` | Image quality assessment (DSSIM/SSIM) |
| `nika:html_to_md` | `fetch-markdown` | HTML to clean Markdown (htmd) |
| `nika:css_select` | `fetch-html` | CSS selector extraction (scraper) |
| `nika:extract_metadata` | `fetch-html` | OG, Twitter Cards, JSON-LD, SEO metadata |
| `nika:extract_links` | `fetch-html` | Rich link classification (internal/external/nav/content) |
| `nika:readability` | `fetch-article` | Article content extraction (dom_smoothie) |

---

## Key Dependencies

| Crate | Version | Purpose |
|:------|:--------|:--------|
| `tokio` | 1.49 | Async runtime |
| `rig-core` | 0.33 | LLM provider abstraction |
| `rmcp` | 0.16 | MCP protocol (stdio transport) |
| `ratatui` | 0.30 | Terminal UI framework |
| `serde-saphyr` | — | YAML parsing with bomb protection |
| `reqwest` | 0.13 | HTTP client |
| `petgraph` | — | DAG (StableGraph + topological sort) |

---

## Architectural Invariants

Things that MUST NEVER happen:

| Rule | Reason |
|:-----|:-------|
| No direct Cypher/SQL from Nika | All graph access through MCP `invoke:` — Zero Cypher rule |
| No AST phase skipping | Always Raw → Analyzed → Lower; validation only in Phase 2 |
| No `anyhow` in library code | Use `NikaError` with NIKA-XXX codes; domain sub-enums in `error_domains.rs` |
| No `cargo test` without `--lib` | Triggers macOS Keychain popups; always `cargo test --lib` or `cargo nextest run --workspace --lib` |
| No `nika-daemon` as feature flag | `nika-daemon` is a workspace crate, not a Cargo feature |

---

## NovaNet Integration

```
NIKA WORKFLOW ──invoke:──> NOVANET MCP ──Cypher──> NEO4J
```

Nika never talks to databases directly. All graph access goes through NovaNet MCP tools. See `docs/plans/` for integration details.
