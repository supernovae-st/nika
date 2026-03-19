# Nika CLI — Developer Reference

Source code for `nika` binary. See `nika/CLAUDE.md` for user-facing docs.

## Source Tree

```
src/
├── main.rs              # CLI entry (clap)
├── lib.rs               # Public API
├── error.rs             # NikaError (NIKA-XXX codes)
├── config.rs            # Configuration types
├── core/                # Zero-dep definitions (providers, models, mcp_aliases)
├── ast/                 # Three-phase: Raw → Analyzed → Lower
│   ├── raw/             #   Phase 1: YAML → Raw AST (parser.rs)
│   ├── analyzed/        #   Phase 2: Validated, resolved AST
│   ├── analyzer/        #   Phase 2: Validation + transformation
│   └── lower.rs         #   Phase 3: Analyzed → Runtime types
├── dag/                 # DAG validation + cycle detection (flow.rs, indexed.rs)
├── runtime/             # Execution engine
│   ├── runner.rs        #   Main workflow runner
│   ├── executor/        #   Task executor (verb dispatch)
│   ├── rig_agent_loop/  #   Agent loop (per-provider)
│   ├── builtin/         #   12 core + 9 media tools (nika:thumbnail, etc.)
│   │   └── media/       #   Media tools: thumbnail, metadata, optimize, svg, etc.
│   └── security.rs      #   Command blocklist + env validation
├── mcp/                 # MCP client (rmcp adapter, pool, retry, validation)
├── provider/            # LLM providers (rig-core cloud + mistral.rs native + cost.rs)
├── binding/             # Data flow: templates, transforms, JSONPath, resolve
├── tools/               # File tools: read, write, edit, glob, grep
├── event/               # NDJSON trace events + EventLog
├── cli/                 # CLI subcommands (doctor, new, init, trace, mcp, etc.)
├── init/                # nika init templates (6 tiers, 30 workflows)
├── io/                  # Atomic file I/O
├── source/              # Source spans + registry
├── store/               # RunContext + TaskResult
├── util/                # Constants, fs helpers, string interner
├── registry/            # Package registry client
├── secrets/             # OS Keychain + daemon IPC
├── tui/                 # Terminal UI (ratatui, 6 views)
└── lsp/                 # Language Server Protocol (feature-gated)
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
| 110-119 | Agent |
| 120-129 | Resilience |
| 130-139 | TUI/Config |
| 140-151 | AST analysis (Phase 2) |
| 160-164 | Policy/Boot errors |
| 170-179 | Runtime (decompose) |
| 200-219 | File tools + Builtin tools |
| 250 | Context error |
| 251-259 | Media pipeline |
| 260-269 | Package URI errors |
| 270-279 | Skill errors |
| 280-285 | Artifacts + Media (path, write, size, integrity, cleanup, lock) |
| 290-297 | Media tools (tool error, format, dependency, timeout, args, pipeline, security) |
| 300-309 | Structured output |

## Testing

```bash
cargo test --lib             # Unit tests (5731+, safe — no keychain)
cargo test --features lsp    # Include LSP tests
cargo clippy -- -D warnings  # Zero warnings policy
```

**WARNING:** `cargo test` (without `--lib`) runs contract tests that trigger macOS Keychain popups. Always use `--lib` for safe testing.

## Conventions

- **Errors:** `NikaError` with NIKA-XXX codes, not `anyhow`
- **AST:** Always Raw -> Analyzed -> Lower. Never skip phases.
- **Providers:** `RigProvider::auto()` for auto-detect. `native` for local GGUF.
- **Extensions:** `.nika.yaml` for workflows
- **Dependencies:** `depends_on: [task_id]`
- **Bindings:** `with: { alias: $task_id }` — `$` prefix required
- **Timeout:** `timeout:` in seconds (parser converts to ms internally)
- **Logging:** `tracing` macros
- **Tests:** TDD preferred. `insta` for snapshots. `cargo test --lib` always.

## Media Tools (v0.33.0)

9 builtin media tools accessible via `invoke: nika:*`:

| Tool | Feature | Description |
|------|---------|-------------|
| `nika:dimensions` | always-on | Image dimensions from headers (~0.1ms) |
| `nika:thumbhash` | always-on | 25-byte image placeholder |
| `nika:dominant_color` | always-on | Color palette extraction |
| `nika:thumbnail` | media-thumbnail | SIMD-accelerated resize (Lanczos3) |
| `nika:metadata` | media-metadata | Universal EXIF/audio/video metadata |
| `nika:optimize` | media-optimize | Lossless PNG optimization (oxipng) |
| `nika:svg_render` | media-svg | SVG to PNG rasterization (resvg) |
| `nika:convert` | media-thumbnail | Format conversion (PNG↔JPEG↔WebP) |
| `nika:strip` | media-thumbnail | Remove metadata (decode+re-encode) |

**Security rules:**
- NEVER use `image::load_from_memory()` directly → use `decode_image_safe()` with Limits
- SVG: always call `sanitize_svg()` BEFORE parsing
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
