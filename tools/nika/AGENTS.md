# Nika CLI — Developer Reference

Source code for `nika` binary. See `nika/CLAUDE.md` for user-facing docs.

## Workspace Structure

```
tools/
├── nika/           Binary (2k lines) — CLI entry point
├── nika-engine/    Execution engine (162k) — embeddable runtime
├── nika-daemon/    Background daemon (5k) — secrets, jobs, watch, cache
├── nika-init/      Project scaffolding (21k) — init wizard + course
├── nika-core/      AST, types, catalogs (23k) — zero I/O
├── nika-event/     EventLog, TraceWriter (4k)
├── nika-mcp/       MCP client, rmcp (9k)
├── nika-media/     CAS store, processor (3.5k)
├── nika-cli/       CLI subcommands (8k)
├── nika-tui/       Terminal UI (86k) — ratatui
├── nika-lsp-core/  LSP intelligence (9k)
└── nika-lsp/       LSP binary (2.5k)
```

## Source Tree (nika-engine)

```
src/
├── lib.rs               # Public API
├── error.rs             # NikaError (NIKA-XXX codes) + FixSuggestion trait
├── error_domains.rs     # Domain sub-enums: ExecutionError, ProviderError, BindingError, DagError
├── config.rs            # Configuration types
├── ast/                 # Three-phase: Raw → Analyzed → Lower
│   └── lower.rs         #   Phase 3: Analyzed → Runtime types
├── dag/                 # DAG validation + cycle detection (flow.rs, indexed.rs)
├── runtime/             # Execution engine
│   ├── runner.rs        #   Main workflow runner
│   ├── executor/        #   Task executor (verb dispatch)
│   ├── rig_agent_loop/  #   Agent loop (per-provider)
│   ├── builtin/         #   12 core + 24 media/fetch tools (nika:thumbnail, etc.)
│   │   └── media/       #   Media tools: import, thumbnail, chart, provenance, etc.
│   └── security.rs      #   Command blocklist + env validation
├── provider/            # LLM providers (rig-core cloud + mistral.rs native + cost.rs)
├── binding/             # Data flow: templates, transforms, JSONPath, resolve
├── tools/               # File tools: read, write, edit, glob, grep
├── display/             # CLI display renderers (Renderer trait + Box<dyn Renderer>)
│   ├── renderer.rs      #   Renderer trait, CliRenderer, RunStats, TestRenderer
│   ├── live.rs          #   LiveRenderer (indicatif: spinners, progress, for_each sub-bars)
│   ├── run_renderer.rs  #   Factory: auto_renderer/classic_renderer/live_renderer
│   ├── format_event.rs  #   44+ shared fmt_*() → String formatters
│   ├── summary.rs       #   format_run_summary + format_doctor_summary (testable pure fns)
│   ├── spinner.rs       #   Spinner constants + progress templates ({elapsed}, {eta})
│   ├── header.rs        #   Workflow header box
│   ├── icons.rs         #   Cosmic icon palette
│   ├── colors.rs        #   Color helpers + sparklines + stripped_len (unicode-width)
│   ├── detail.rs        #   DetailLevel verbosity control
│   ├── check.rs         #   Pre-flight validation checklist
│   └── dag_render.rs    #   Static DAG visualization
├── init/                # nika init (minimal scaffold + course generator)
│   ├── course/          #   12-level interactive course (44 exercises)
│   └── showcase_*.rs    #   Showcase workflow generators
├── io/                  # Atomic file I/O
├── source/              # Source spans + registry
├── store/               # RunContext + TaskResult
├── util/                # Constants, fs helpers, string interner
├── registry/            # Package registry client
└── secrets/             # OS Keychain + daemon IPC
```

## Source Tree (nika-core)

```
src/
├── lib.rs               # Public API
├── core/                # Zero-dep definitions (providers, models, mcp_aliases)
├── ast/                 # AST types (Raw, Analyzed, Analyzer, Schema)
│   ├── raw/             #   Phase 1: YAML → Raw AST (parser.rs)
│   ├── analyzed/        #   Phase 2: Validated, resolved AST
│   ├── analyzer/        #   Phase 2: Validation + transformation
│   └── schema.rs        #   Schema types
└── binding/             # Transform catalog
```

## Error Codes

| Range | Category |
|-------|----------|
| 000-009 | Workflow |
| 010-019 | Schema/validation |
| 020-029 | DAG |
| 030-039 | Provider |
| 040-049 | Template/binding |
| 050-059 | Path/task/security |
| 060-069 | Output (JSON/schema validation) |
| 070-089 | With block + DAG validation |
| 090-099 | JSONPath/IO/Execution |
| 100-109 | MCP |
| 110-119 | Agent + Guardrails (112 = guardrail violation) |
| 120-129 | Resilience |
| 130-139 | TUI/Config |
| 140-151 | AST analysis (Phase 2) |
| 160-164 | Policy/Boot errors |
| 170-179 | Runtime (decompose) |
| 200-214 | File tools + Builtin tools |
| 215-219 | File I/O errors (215 = FileAlreadyExists) |
| 250 | Context error |
| 251-259 | Media pipeline |
| 260-269 | Package URI errors |
| 270-279 | Skill errors |
| 280-285 | Artifacts + Media (path, write, size, integrity, cleanup, lock) |
| 290-297 | Media tools (tool error, format, dependency, timeout, args, pipeline, security) |
| 300-309 | Structured output |
| 310-319 | Course errors |

## Testing

```bash
cargo test --workspace --lib             # All crates (8260+, safe — no keychain)
cargo test --lib                         # nika binary tests only
cargo test -p nika-engine --lib          # Engine tests only (3870+)
cargo test -p nika-daemon --lib          # Daemon tests only (116)
cargo test -p nika-tui --lib             # TUI tests only (2120+)
cargo test -p nika-engine --lib -- display  # Display system tests (255+)
cargo test --features lsp               # Include LSP tests
cargo clippy --workspace -- -D warnings  # Zero warnings policy
```

**WARNING:** `cargo test` (without `--lib`) runs contract tests that trigger macOS Keychain popups. Always use `--lib` for safe testing.

## Conventions

- **Errors:** `NikaError` with NIKA-XXX codes, not `anyhow`. Domain sub-enums: `ExecutionError`, `ProviderError`, `BindingError`, `DagError` (in `error_domains.rs`, with `From` impls)
- **AST:** Always Raw -> Analyzed -> Lower. Never skip phases.
- **Providers:** `RigProvider::auto()` for auto-detect. `native` for local GGUF.
- **Extensions:** `.nika.yaml` for workflows
- **Dependencies:** `depends_on: [task_id]`
- **Bindings:** `with: { alias: $task_id }` — `$` prefix required
- **Timeout:** `timeout:` in seconds (parser converts to ms internally)
- **Pipe transforms:** `{{with.data | uppercase | trim}}` — available in all template contexts
- **Logging:** `tracing` macros
- **Tests:** TDD preferred. `insta` for snapshots. `cargo test --lib` always.

## Vision Support (since v0.34.0)

The `infer:` verb supports multimodal `content:` for sending images to vision-capable LLMs:

```yaml
infer:
  content:
    - type: image
      source: "{{with.photo.media[0].hash}}"  # CAS hash → base64 automatically
      detail: high
    - type: text
      text: "Describe this image"
```

- `prompt:` optional when `content:` present (if both: prompt prepended as first Text part)
- CAS images auto-resolved to base64 (paths never leak to LLM APIs)
- Cloud vision: Claude, OpenAI, Mistral, Groq, Gemini, xAI
- Native vision: Supported via `NativeModelKind::VisionHf` (HuggingFace + ISQ)
  - `nika model vision Qwen/Qwen2.5-VL-7B-Instruct --isq Q4K`
  - Targets: Gemma 3 4B (~3 GB), Qwen2.5-VL 7B (~5 GB), Gemma 3 12B (~8 GB)
  - Note: GGUF models are text-only — vision requires VisionModelBuilder + ISQ from safetensors
- Unsupported: DeepSeek (returns VisionNotSupported error)

## Fetch Extraction (since v0.35.0)

The `fetch:` verb supports `extract:` for HTML post-processing and `response:` for output modes:

### Extract modes (9 total)
- `extract: markdown` — Clean Markdown via htmd [fetch-markdown]
- `extract: article` — Main article content via dom_smoothie Readability [fetch-article]
- `extract: text` — Visible text, optionally filtered by `selector:` [fetch-html]
- `extract: selector` — Raw HTML of matching elements, requires `selector:` [fetch-html]
- `extract: metadata` — OG, Twitter Cards, JSON-LD, SEO tags as JSON [fetch-html]
- `extract: links` — Rich link classification (internal/external, nav/content/footer) [fetch-html]
- `extract: jsonpath` — JSONPath query on JSON API responses, use `selector:` for path (zero deps)
- `extract: feed` — RSS/Atom/JSON Feed parsing via feed-rs [fetch-feed]
- `extract: llm_txt` — AI-era content discovery (/.well-known/llm.txt, /llms.txt)

### Response modes
- `response: full` — JSON with status, headers, body, final URL
- `response: binary` — Store in CAS, return hash for media pipeline
- No response field — Raw body text (default)

## Media Tools (since v0.34.0)

24 builtin media tools accessible via `invoke: nika:*`, organized in 3 tiers:

### Tier 1 — Always-on (5 tools)

| Tool | Description |
|------|-------------|
| `nika:import` | Import any file into CAS (content-addressable storage) |
| `nika:dimensions` | Image dimensions from headers (~0.1ms) |
| `nika:thumbhash` | 25-byte image placeholder |
| `nika:dominant_color` | Color palette extraction |
| `nika:pipeline` | Chain operations in-memory (zero intermediate files) |

### Tier 2 — media-core default (6 tools)

| Tool | Feature | Description |
|------|---------|-------------|
| `nika:thumbnail` | media-thumbnail | SIMD-accelerated resize (Lanczos3) |
| `nika:convert` | media-thumbnail | Format conversion (PNG/JPEG/WebP) |
| `nika:strip` | media-thumbnail | Remove metadata (decode+re-encode) |
| `nika:metadata` | media-metadata | Universal EXIF/audio/video metadata |
| `nika:optimize` | media-optimize | Lossless PNG optimization (oxipng) |
| `nika:svg_render` | media-svg | SVG to PNG rasterization (resvg) |

### Tier 3 — Opt-in (13 tools)

| Tool | Feature | Description |
|------|---------|-------------|
| `nika:phash` | media-phash | Perceptual image hashing |
| `nika:compare` | media-phash | Visual comparison via perceptual hash |
| `nika:pdf_extract` | media-pdf | PDF text extraction |
| `nika:chart` | media-chart | Bar/line/pie charts from JSON data |
| `nika:provenance` | media-provenance | C2PA content credentials (sign) |
| `nika:verify` | media-provenance | C2PA manifest verification + EU AI Act compliance |
| `nika:qr_validate` | media-qr | QR decode + 0-100 scan score (qrcode-ai-scanner-core) |
| `nika:quality` | media-iqa | Image quality assessment (DSSIM/SSIM) |
| `nika:html_to_md` | fetch-markdown | HTML to clean Markdown (htmd) |
| `nika:css_select` | fetch-html | CSS selector extraction (scraper) |
| `nika:extract_metadata` | fetch-html | OG, Twitter Cards, JSON-LD, SEO metadata |
| `nika:extract_links` | fetch-html | Rich link classification (internal/external/nav/content) |
| `nika:readability` | fetch-article | Article content extraction (dom_smoothie) |

**Security rules:**
- NEVER use `image::load_from_memory()` directly → use `decode_image_safe()` with Limits
- SVG: always call `sanitize_svg()` BEFORE parsing
- Import: validate paths against traversal attacks, pre-read size check (50 MB default limit)
- Timeout: 30s default on all operations
- Feature `media-core` (default) enables all Tier 2 tools

## Common Mistakes

| Mistake | Correct |
|---------|---------|
| Using `anyhow` for errors | Use `NikaError` with NIKA-XXX code |
| Direct Neo4j/Cypher access | Use MCP `invoke:` verb |
| Skipping AST analyzer phase | Always Raw -> Analyzed -> Lower |
| Hardcoding provider | Use `RigProvider::auto()` |
| `.yaml` extension | Use `.nika.yaml` for workflows |
| `cargo test` (triggers keychain) | Use `cargo test --lib` |
| Missing `depends_on:` | Add `depends_on: [task_id]` for ordering deps |
| `timeout: 30` meaning 30ms | `timeout: 30` means 30 seconds now |
| `image::load_from_memory()` | Use `decode_image_safe()` from media/safety.rs |
| SVG without sanitize | Always `sanitize_svg()` BEFORE usvg parsing |
| Import without path validation | Use `validate_import_path()` to block traversal attacks |
| GGUF model for native vision | GGUF is text-only — use `NativeModelKind::VisionHf` with HuggingFace model ID |
| Skipping pre-read size check | Always check file size before reading into memory |
