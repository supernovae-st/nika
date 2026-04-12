# Nika CLI — Developer Reference

Source code for `nika` binary. See `nika/CLAUDE.md` for user-facing docs.

## Workspace Structure (26 crates)

```
tools/
│
│ L0 — CORE (pure, zero I/O)
├── nika-core/          AST, types, catalogs, policy, trust (23k)
├── nika-event/         EventLog, TraceWriter (4k)
│
│ L0.5 — KERNEL (trait boundary for all side effects)
├── nika-kernel/        Trait definitions: Provider, FileSystem, BlobStore, HttpClient, ShellExecutor (717)
├── nika-kernel-mock/   5 hand-written mocks for kernel traits (744, dev-dep only)
│
│ L1 — EFFECTS (one crate per side-effect, implements kernel traits)
├── nika-clock/         SystemClock — tokio::time (85)
├── nika-fs/            TokioFs — tokio::fs + globset (262)
├── nika-blob/          DiskBlobStore — blake3 CAS (342)
├── nika-http/          ReqwestClient — SSRF defense (384)
├── nika-exec-runner/   TokioShell — 100+ pattern blocklist (692)
│
│ L2 — ENGINES (runtime, providers, tools)
├── nika-engine/        Execution engine (160k) — providers, builtins, runtime
│   └── provider/rig/kernel_bridge.rs  — impl Provider for RigProvider
│   └── runtime/executor: get_dyn_provider() → Arc<dyn Provider>
├── nika-builtin/       27 builtin tools (6.4k) — core, data, introspection (Phase 12)
├── nika-display/       CLI renderers, formatters (13k)
├── nika-media/         CAS store, image processor (13k)
├── nika-mcp/           MCP client, rmcp (9k)
├── nika-daemon/        Background daemon (5k) — secrets, jobs, watch, cache
├── nika-storage/       Storage abstraction (1k)
├── nika-vault/         Encrypted secrets (1.2k) — XChaCha20 + Argon2i
│
│ L3 — INTERFACES
├── nika-cli/           CLI subcommands (8k)
├── nika-tui/           Terminal UI (86k) — ratatui
├── nika-serve/         HTTP API server (4k)
├── nika-sdk/           Embedded SDK (3k) — programmatic API
├── nika-lsp-core/      LSP intelligence (9k)
├── nika-lsp/           LSP binary (2.5k)
├── nika-init/          Project scaffolding (21k) — init wizard + course
│
│ L4 — BINARY
└── nika/               Binary entry point (2k)
```

### Provider Bridge (kernel_bridge.rs)

`nika-engine` bridges the `nika-kernel::Provider` trait to the concrete `RigProvider`:
- `kernel_bridge.rs` in `provider/rig/` implements `Provider for RigProvider`
- `TaskExecutor::get_dyn_provider()` returns `Arc<dyn Provider>` for trait-based dispatch
- Mock provider implements `Provider` directly (no bridge needed)

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
│   ├── builtin/         #   63 builtin tools (nika:*)
│   │   ├── data/        #   Data tools: jq, map, filter, enrich, merge, inject, etc.
│   │   └── media/       #   Media tools: import, thumbnail, chart, provenance, etc.
│   ├── shield.rs        #   SecurityContext aggregate (SpotlightFence + CanarySystem)
│   ├── canary.rs        #   Canary token system (3x16-char, exfiltration detection)
│   ├── spotlight.rs     #   Untrusted data fencing (wrap_untrusted, re-anchoring)
│   └── security.rs      #   Command blocklist + env validation
├── provider/            # LLM providers (rig-core cloud + mistral.rs native + cost.rs)
├── binding/             # Data flow: templates, transforms, JSONPath, resolve
├── tools/               # File tools: read, write, edit, glob, grep + check_path_readable (Shield)
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
├── policy.rs            # SecurityPolicyConfig (diamond-layered, zero I/O)
├── trust.rs             # TrustLevel, InvocationSource, trust categories for builtins
├── capabilities.rs      # AgentToolPolicy enum (Unrestricted | RestrictDangerous)
├── catalogs/            # Zero-dep definitions (providers, models, mcp_aliases)
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
| 030-039 | Provider (033 = unknown provider name warning) |
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
| 160-164 | Parse errors (Phase 1 parser, nika-core) |
| 165-169 | Policy/Boot/Startup errors |
| 170-179 | Runtime (decompose) |
| 200-214 | File tools + Builtin tools |
| 215-219 | File I/O errors (215 = FileAlreadyExists) |
| 250 | Context error |
| 251-259 | Media pipeline |
| 260-269 | Package URI errors |
| 270-279 | Skill errors (271 = SkillIntegrityFailed) |
| 280-285 | Artifacts + Media (path, write, size, integrity, cleanup, lock) |
| 290-297 | Media tools (tool error, format, dependency, timeout, args, pipeline, security) |
| 300-309 | Structured output |
| 310-319 | Course errors |
| 320-329 | Record compression errors |
| 380-389 | Nika Shield security (CapabilityDenied, TrustViolation, CanaryLeaked, SpotlightRequired, RunDepthExceeded, RunCycleDetected, CanaryInThinking, UntrustedVisionBlocked) |
| SECRET-001 | Daemon: keychain disabled |
| SECRET-002 | Daemon: keyring store error |
| SECRET-003 | Daemon: keyring delete error |
| SECRET-004 | Daemon: unknown provider |

### nika check Validation (v0.63.3)

`nika check` now validates beyond syntax and DAG:
- **Skill file paths**: verifies all `skills:` paths exist on disk
- **Context file paths**: verifies all `context:` file paths exist on disk
- **Provider names**: warns (NIKA-033) on unrecognized provider names

## Testing

```bash
cargo test --workspace --lib             # All crates (10666+, safe — no keychain)
cargo test --lib                         # nika binary tests only
cargo test -p nika-engine --lib          # Engine tests only
cargo test -p nika-daemon --lib          # Daemon tests only
cargo test -p nika-tui --lib             # TUI tests only
cargo test -p nika-engine --lib -- shield  # Shield security tests (101+)
cargo test --features lsp               # Include LSP tests
cargo clippy --workspace -- -D warnings  # Zero warnings policy
```

**WARNING:** `cargo test` (without `--lib`) runs contract tests that trigger macOS Keychain popups. Always use `--lib` for safe testing.

**Shield tests:** Use `InvocationSource::Test` for `RunContext` — trusted inputs, all security layers active but permissive. Test files: `tests_shield_*.rs` in executor/ and tools/.

### Testing Philosophy — Be INTELLIGENT, Not Superficial

**Structured output tests**: validate output PROGRAMMATICALLY, not with `assert!(!output.is_empty())`.

```rust
// BAD — proves nothing:
assert!(!output.is_empty());

// BAD — cheating (mentioning JSON in prompt):
infer: "Return a JSON object with name and age"

// GOOD — natural prompt + programmatic validation:
infer: "Parle-moi d'Alice, 30 ans, developpeuse"
// Then in Rust test:
let parsed: Value = serde_json::from_str(&output).expect("valid JSON");
assert!(parsed["name"].is_string());
assert!(parsed["age"].as_f64().unwrap() >= 0.0);
assert!(parsed["skills"].as_array().unwrap().len() >= 2);
```

**Rules:**
1. Same test on ALL 7 providers — if one fails, it's an ENGINE bug to fix
2. Prompt must be NATURAL — the 4-layer system injects JSON schema automatically
3. Check EventLog for StructuredOutputSuccess — verify which layer succeeded
4. Validate every field: type, enum, range, minItems, required
5. E2E = parse → analyze → DAG → execute → validate output → verify events

### Secrets & Daemon

Daemon auto-starts on any nika command. No keychain popups (direct keyring access removed).
Resolution: env vars → daemon IPC → clear error. Opt-out: `NIKA_NO_DAEMON=1`.
VPS resilience: `~/.nika/daemon/nika-exe-path` stores the binary path so the daemon survives binary upgrades.

## Conventions

- **Errors:** `NikaError` with NIKA-XXX codes, not `anyhow`. Domain sub-enums: `ExecutionError`, `ProviderError`, `BindingError`, `DagError` (in `error_domains.rs`, with `From` impls)
- **AST:** Always Raw -> Analyzed -> Lower. Never skip phases.
- **Providers:** `RigProvider::auto()` for auto-detect. `native` for local GGUF.
- **Extensions:** `.nika.yaml` for workflows
- **Dependencies:** `depends_on: [task_id]`
- **Bindings:** `with: { alias: $task_id }` — `$` prefix required
- **Timeout:** `timeout:` in seconds (parser converts to ms internally)
- **Pipe transforms:** `{{with.data | upper | trim}}` — available in all template contexts
- **Logging:** `tracing` macros
- **Tests:** TDD preferred. `insta` for snapshots. `cargo test --lib` always.
- **Skills:** Workflow-level `skills:` are auto-injected into all `infer:` task system prompts (not just agents). Agent tasks can override with per-task `skills: [x, y]`.
- **Shield:** `SecurityContext` is passed via `task_local!` in runner — never modify `BuiltinTool::call` signature. `trust: elevated` on a task overrides capability restrictions. `SecurityPolicyConfig` in nika-core, `SecurityContext` in nika-engine.
- **Trust:** `RunContext::new()` requires `InvocationSource` (no default constructor). `InvocationSource::NestedRun { ceiling }` for nika:run.

## Custom Endpoints (OpenAI-Compatible)

Nika supports connecting to any OpenAI-compatible inference server (vLLM, TGI, Ollama, LiteLLM, SGLang).

### Configuration (nika.toml or config.toml)

```toml
# In nika.toml (project-level, wins on conflict)
# OR ~/.config/nika/config.toml (user-level)
[endpoints.h100]
base_url = "http://10.0.1.42:8000/v1"
api_key = "sk-internal-token"
model = "Qwen/Qwen3-8B"
timeout_secs = 60

[endpoints.ollama]
base_url = "http://localhost:11434/v1"
model = "llama3.2"
```

### Usage in workflows

```yaml
schema: "nika/workflow@0.12"
provider: h100          # Named endpoint from nika.toml
model: Qwen/Qwen3-8B

tasks:
  - id: generate
    infer: "Hello from vLLM"
```

### Slash syntax (shorthand)

```yaml
# Equivalent to provider: groq + model: llama-3.3-70b
model: groq/llama-3.3-70b

# Nested model paths split on FIRST slash
model: native/Qwen/Qwen3-8B   # provider=native, model=Qwen/Qwen3-8B

# Named endpoint
model: h100/Qwen/Qwen3-8B     # provider=h100, model=Qwen/Qwen3-8B
```

**Note**: `base_url:` in workflow YAML is removed. Use named endpoints in nika.toml instead.

### Environment variable overrides

```bash
# Override named endpoint URL/key at runtime
export NIKA_ENDPOINT_H100_URL="http://new-server:8000/v1"
export NIKA_ENDPOINT_H100_KEY="sk-new-key"

# rig-core native: all provider: openai tasks hit this URL
export OPENAI_BASE_URL="http://localhost:8000/v1"
```

### Error codes

| Code | Meaning |
|------|---------|
| NIKA-035 | Custom endpoint not found in config |
| NIKA-036 | Cannot connect to custom endpoint |

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
| `nika:write` without `overwrite: true` on existing file | NIKA-215 (FileAlreadyExists). Pass `overwrite: true` param to replace. |
| `base_url:` in workflow YAML | REMOVED. Use named endpoints in nika.toml `[endpoints.*]` instead |
| `--features native-inference` | Bundled by default since v0.73. Just `cargo build`. |
