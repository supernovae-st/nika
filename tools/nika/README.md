# Nika — Developer Reference

[![CI](https://github.com/supernovae-st/nika/actions/workflows/ci.yml/badge.svg)](https://github.com/supernovae-st/nika/actions/workflows/ci.yml)
[![crates.io](https://img.shields.io/crates/v/nika?style=flat-square&logo=rust&logoColor=white&color=e6522c)](https://crates.io/crates/nika)
[![License](https://img.shields.io/badge/AGPL--3.0--or--later-22c55e?style=flat-square&logo=gnu&logoColor=white)](../../LICENSE)

Source code for the `nika` binary and all workspace crates. For user-facing documentation, see [root README](../../README.md).

---

## Workspace

```
tools/
├── nika/               Binary entry point            (cargo install nika)
├── nika-engine/        Embeddable runtime (135k LOC) (cargo add nika-engine)
├── nika-core/          AST, types, catalogs (23k)    zero I/O
├── nika-event/         EventLog, TraceWriter (4k)
├── nika-mcp/           MCP client + rmcp (9k)
├── nika-media/         CAS store + processor (13k)
├── nika-cli/           CLI subcommands (8k)
├── nika-daemon/        Background daemon (5k)
├── nika-init/          Project scaffolding (21k)
├── nika-tui/           Terminal UI — ratatui (86k)
├── nika-lsp-core/      LSP intelligence (9k)
└── nika-lsp/           Standalone LSP binary (2.5k)
```

**Workspace root:** `tools/Cargo.toml` — all crate versions are workspace-inherited.

---

## Project Structure

Nika follows the `.git` principle: zero imposed directory names. Only `nika.toml` and `.nika/` are Nika's territory.

```
project/
├── nika.toml              Project config (versioned, committed)
├── .nika/                 Runtime state (gitignored)
│   ├── traces/            Execution traces
│   ├── cache/             LLM response cache
│   ├── media/store/       CAS blobs
│   └── sessions/          Editor state
├── *.nika.yaml            Workflows (anywhere in project)
├── artifacts/             Output dir (configurable via [artifacts] dir)
└── AGENTS.md              AI context (nika init)
```

**Root detection:** Walk up from cwd to find `nika.toml` (primary) > `.nika/` (legacy fallback).

**Config merge:** CLI flags > env vars > `nika.toml` > `~/.nika/config.toml` > defaults.

---

## Build

```bash
cargo build --release               # Full release build
cargo build                         # Debug build
cargo build --no-default-features   # Minimal (no TUI, media, or native inference)
cargo build --features lsp          # + standalone LSP binary
```

---

## Test

```bash
# Standard — runs all unit tests safely (no Keychain popups)
cargo nextest run --workspace --lib

# Single crate
cargo nextest run -p nika-engine --lib

# Filter by name
cargo nextest run --workspace --lib -- display

# Doc tests
cargo test --workspace --doc

# With LSP tests
cargo nextest run --workspace --lib --features lsp
```

> **WARNING:** `cargo test` without `--lib` runs integration tests that trigger macOS Keychain
> dialogs. Always use `cargo nextest run --workspace --lib` for safe local testing.

### Test counts by crate (approx.)

| Crate | Tests |
|:------|------:|
| `nika-engine` | ~4,100 |
| `nika-tui` | ~2,100 |
| `nika-core` | ~800 |
| `nika-mcp`, `nika-cli`, others | ~1,200 |
| **Total** | **~8,200+** |

---

## Lint & Format

```bash
cargo fmt --all --check                          # Format check (CI gate)
cargo clippy --workspace --all-targets -- -D warnings   # Zero warnings policy
cargo doc --workspace --no-deps                  # Doc build check
```

---

## Source Tree — `nika-engine/src/`

```
src/
├── lib.rs               # Public API
├── error.rs             # NikaError (NIKA-XXX codes)
├── config.rs            # Configuration types
├── ast/                 # Three-phase: Raw → Analyzed → Lower
│   └── lower.rs         #   Phase 3: Analyzed → Runtime types
├── dag/                 # DAG validation + cycle detection (flow.rs, indexed.rs)
├── runtime/             # Execution engine
│   ├── runner.rs        #   Main workflow runner
│   ├── executor/        #   Task executor (verb dispatch)
│   ├── rig_agent_loop/  #   Agent loop (per-provider)
│   ├── builtin/         #   12 core + 24 media/fetch tools
│   │   └── media/       #   Media tools: import, thumbnail, chart, provenance…
│   └── security.rs      #   Command blocklist + env validation
├── provider/            # LLM providers (rig-core cloud + mistral.rs native)
├── binding/             # Data flow: templates, transforms, JSONPath, resolve
├── display/             # CLI display renderers
│   ├── renderer.rs      #   Renderer trait + CliRenderer (append-only)
│   ├── live.rs          #   LiveRenderer (animated, indicatif spinners)
│   ├── run_renderer.rs  #   RunRenderer dispatch (auto TTY detection)
│   ├── summary.rs       #   Shared summary box
│   ├── dag_render.rs    #   Static DAG visualization
│   └── ...              #   icons, colors, detail, check, header, spinner
├── tools/               # File tools: read, write, edit, glob, grep
├── io/                  # Atomic file I/O
├── store/               # RunContext + TaskResult
└── util/                # Constants, fs helpers, string interner
```

---

## Error Codes

| Range | Category |
|:------|:---------|
| `000–009` | Workflow parsing |
| `010–019` | Schema/validation |
| `020–029` | DAG (cycles, missing deps) |
| `030–039` | Provider errors |
| `040–049` | Template/binding |
| `050–059` | Security (path traversal, blocked commands) |
| `060–069` | Output (JSON/schema validation) |
| `070–089` | `with:` block + DAG validation |
| `090–099` | JSONPath/IO/Execution |
| `100–109` | MCP (connection, tool errors) |
| `110–119` | Agent + Guardrails (112 = guardrail violation) |
| `120–129` | Resilience (retry, timeout) |
| `130–139` | TUI/Config |
| `140–151` | AST analysis (Phase 2) |
| `160–164` | Policy/Boot errors |
| `170–179` | Runtime (decompose) |
| `200–219` | Builtin tools + file I/O (215 = FileAlreadyExists) |
| `250–259` | Media pipeline |
| `260–269` | Package URI errors |
| `270–279` | Skill errors |
| `280–285` | Artifacts + Media |
| `290–297` | Media tools |
| `300–309` | Structured output |
| `310–319` | Course errors |

---

## Feature Flags

| Flag | Default | Description |
|:-----|:--------|:------------|
| `tui` | yes | Terminal UI (ratatui, tree-sitter, git2) |
| `native-inference` | yes | Local GGUF/ISQ models via mistral.rs |
| `media-core` | yes | Tier 2 media tools (thumbnail, convert, strip, metadata, optimize, svg) |
| `media-phash` | yes | Perceptual hashing + comparison |
| `media-pdf` | yes | PDF text extraction |
| `media-chart` | yes | Bar/line/pie chart generation |
| `media-qr` | yes | QR code validation |
| `media-iqa` | yes | Image quality assessment (DSSIM/SSIM) |
| `media-provenance` | no | C2PA signing + EU AI Act compliance |
| `media-compression` | yes | zstd CAS compression |
| `fetch-html` | yes | HTML extraction (text, selector, metadata, links) |
| `fetch-markdown` | yes | HTML → Markdown (htmd) |
| `fetch-article` | yes | Article extraction (dom_smoothie/Readability) |
| `fetch-feed` | yes | RSS/Atom/JSON Feed parsing |
| `lsp` | no | Standalone LSP server binary |

---

## CI Quality Gates

Every PR passes 8 jobs in `ci.yml`:

```
check → test → test-features → coverage
security → semver → validate → summary
```

| Job | What it checks |
|:----|:---------------|
| `check` | `cargo fmt` + `clippy` + `doc` + version lock (0.x.x) |
| `test` | `cargo nextest run --workspace --lib` on ubuntu + macos |
| `test-features` | `--no-default-features` + `--all-features` |
| `coverage` | `cargo-llvm-cov nextest --lib` → Codecov |
| `security` | `cargo audit` + `cargo deny` + `cargo machete` |
| `semver` | Breaking change detection (`cargo-semver-checks`) |
| `validate` | Build binary, run `nika check` on all 498 examples |
| `summary` | PR comment with all results |

**Version Lock:** Nika will never be `1.0.0` or higher. PRs violating this are rejected automatically.

### Run locally before pushing

```bash
# Minimum (30 seconds)
cargo fmt --check && cargo clippy --workspace -- -D warnings && cargo nextest run --workspace --lib

# Full gate (mirrors CI exactly)
cargo fmt --check
cargo clippy --workspace --all-targets -- -D warnings
cargo nextest run --workspace --lib
cargo test --workspace --doc
cargo audit && cargo deny check
```

---

## Conventions

- **Errors:** `NikaError` with `NIKA-XXX` codes — never `anyhow` in library code
- **AST:** Always Raw → Analyzed → Lower. Never skip phases.
- **Providers:** `RigProvider::auto()` for auto-detection; `native` for local GGUF.
- **Extensions:** `.nika.yaml` for workflows (never `.yaml` alone)
- **Bindings:** `with: { alias: $task_id }` — `$` prefix required
- **Timeout:** `timeout:` in **seconds** (parser converts to ms internally)
- **Logging:** `tracing` macros throughout
- **Tests:** TDD preferred; `insta` for snapshots; always `--lib` flag

---

## Security Model

- `exec:` defaults to `shell: false` (no shell injection)
- Command blocklist (30+ patterns: `rm -rf`, `sudo`, reverse shells, etc.)
- Unicode NFKC normalization + zero-width character stripping
- API key redaction from child process environments
- MCP env var validation (`LD_PRELOAD` blocked)
- SSRF URL scheme validation (http/https only, CIDR blocklist)
- YAML bomb protection via serde-saphyr budget limits
- Media import: path traversal protection + 50 MB size limit

---

## License

AGPL-3.0-or-later — see [LICENSE](../../LICENSE)
