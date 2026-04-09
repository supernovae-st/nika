# Architecture Rules

## Workspace Structure

Each subsystem is a separate crate in `tools/<name>/` (e.g. `nika-engine`, `nika-tui`, `nika-cli`). The crate boundaries are the architectural contract.

## MCP Integration

Nika connects to external services via MCP protocol ONLY. No direct database access in workflows.

## Zero Cypher in Nika

Nika workflows NEVER use raw Cypher/SQL. Use `invoke:` with MCP tools.

## Editor Extensions Architecture

Nika supports 5 editors via a shared LSP + thin editor-specific wrappers:

```
editors/
├── vscode/    Full extension (TS, DAG webview, binary auto-download)
├── zed/       Rust WASM extension (LSP + MCP Context Server + Runnables)
├── neovim/    Lua plugin (lazy.nvim, health check, 6 keymaps)
├── helix/     TOML config + Tree-sitter queries + textobjects
└── shared/    nika-keywords.json (generated from Rust source)
```

**Pattern:** 1 LSP binary (`nika lsp --stdio`), N thin wrappers. Same as rust-analyzer.

**Sync:** `editors/sync-editors.sh --fix` propagates keyword changes from Rust source to all editors.
CI guard: `.github/workflows/editor-sync.yml` runs on PRs touching nika-core or editors/.

**Source of truth:** `KNOWN_TRANSFORM_NAMES`, `KNOWN_BUILTIN_TOOLS`, `KNOWN_TASK_KEYS` in nika-core.
Generated: `editors/shared/nika-keywords.json` via `editors/shared/extract-keywords.py`.

**When adding a new transform/builtin/keyword:**
1. Add it in the Rust source (nika-core)
2. Run `./editors/sync-editors.sh --fix`
3. Commit both Rust + editor changes together

**When adding a new editor:**
1. Create `editors/<name>/` with minimal LSP config
2. Add Tree-sitter highlights using keywords from `nika-keywords.json`
3. Add check function to `sync-editors.sh`

## AI Rules Architecture (Progressive Discovery)

Nika ships AI context files for 15+ coding assistants. The architecture follows
progressive disclosure — not a single monolithic dump.

**4 Layers:**
- L0 Identity (always loaded, <20 lines): "Nika project, schema @0.12, 5 verbs"
- L1 Syntax (loaded on *.nika.yaml edit, ~100 lines): verbs + data flow + examples
- L2 Reference (on demand, ~80 lines per topic): transforms, errors, providers
- L3 Live (MCP + LSP, real-time): nika_schema, nika_check, nika_error_lookup

**Source:** `tools/nika-cli/rules/shared/` — 7 content modules (identity, verbs, data-flow, structured-output, common-mistakes, providers, advanced).
**Assembly:** `tools/nika-cli/src/rules.rs` — 12 assembler functions compose modules via `include_str!()` into tool-specific formats.
**Deployment:** `nika init` (project files) + `nika setup` / `fast_rule_update()` (home directory rules).
**Versioning:** xxhash64 fingerprint per file, auto-update on CLI version change.
**AI tools:** Claude, Cursor (3-file), Copilot, Windsurf, Roo, Gemini, Amazon Q, JetBrains, Cline, AGENTS.md.

**Rule:** Don't dump 500 lines. Let the AI discover via MCP tools + focused rule files.
**Rule:** Content lives in `shared/` modules. Tool-specific files are assembled, not duplicated.

## Crate Architecture (Current: 24 Crates, Target: ~32 Crates)

### Current Diamond Pattern (v0.79.3, post-Constellation Session 4)

```
L0    nika-core (23K)         — AST, types, catalogs, policy, trust, capabilities — ZERO I/O
L0.5  nika-kernel (717)       — 10 trait defs — ZERO impls                        NEW S2
      nika-kernel-mock (744)  — 5 hand-written mocks (dev-dep)                    NEW S2
L1    nika-clock              — SystemClock (tokio::time, ZST)                     NEW S3
      nika-fs                 — TokioFs (tokio::fs + globset, ZST)                 NEW S3
      nika-blob               — DiskBlobStore (blake3 CAS)                         NEW S3
      nika-http               — ReqwestClient (SSRF: IPv4/v6/CGN/meta)             NEW S3
      nika-exec-runner        — TokioShell (100+ pattern blocklist + NFKC)         NEW S3
      nika-event (4.5K)       — EventLog + EventEmitter blanket impl
      nika-lsp-core (12K)     — LSP intelligence (pure functions)
L2    nika-engine (160K)      — MONOLITH: providers, builtins, runtime, http, exec
      + kernel_bridge.rs      — impl Provider for RigProvider                      NEW S4
      + get_dyn_provider()    — Arc<dyn Provider> keystone method                  NEW S4
      ├── runtime/shield.rs   — SecurityContext aggregate (SpotlightFence + CanarySystem)
      ├── runtime/canary.rs   — Canary token system
      └── runtime/spotlight.rs — Untrusted data fencing
      nika-display (13K), nika-media (14K), nika-mcp (9K)
      nika-daemon (7K), nika-storage (1K), nika-vault (1.2K)
L3    nika-cli (8K), nika-tui (88K), nika-serve (4K)
      nika-lsp (2.5K), nika-sdk (3K), nika-init (21K)
L5    nika (5.5K)             — Binary entry point (Phase 15: shrinking to <900)
```

**Diamond layering note:** `SecurityPolicyConfig` lives in nika-core (L0) so that nika-display (L2) can read security policy without depending on nika-engine. The runtime `SecurityContext` aggregate lives in nika-engine.

**Constellation progress (S1-S4):** 7 new crates, 6 production trait impls, 3 god files split, 10,790 tests. Provider bridge enables verb crate extraction (Phase 12+).

### Target Architecture (Constellation refactor, in flight 2026-04-08)

Per `nika/docs/plans/2026-04-08-constellation-v2-mega-plan.md`.
Goal: nika-engine ≤100K LOC, ~32 crates, strict downward layering, trait boundaries for I/O.
See `docs/sprints/CONSTELLATION-V2.3-AGGRESSIVE-TARGETS.md` for firm targets.

```
L0  KERNEL (pure, zero I/O, zero async)
    nika-error              — NikaErrorBase trait, NIKA-XXX codes
    nika-catalog            — Static catalogs (providers, models, transforms, builtins)
    nika-schema             — AST: Raw → Analyzed → Lower (was nika-core/ast)
    nika-binding            — Template, transforms, resolve (was nika-core/binding)
    nika-dag                — DAG construction, topological sort

L1  SECURITY & SUPPORT
    nika-shield             — Trust, spotlight, canary, capabilities
    nika-event              — Telemetry, traces
    nika-lsp-core           — LSP intelligence

L2  EFFECTS (one crate per side-effect type)
    nika-provider           — LLM providers (rig, native, mock) + Provider trait
    nika-builtin            — 63 builtin tools + BuiltinTool trait
    nika-http               — Fetch, SSRF, extraction + HttpClient trait
    nika-exec               — Shell exec, blocklist + ShellExecutor trait
    nika-mcp                — MCP client/server
    nika-media              — CAS store, image ops
    nika-storage, nika-vault

L3  ORCHESTRATION
    nika-runtime            — Runner, executor, dispatch (the heart, ~30k)
    nika-daemon             — Background daemon, cron
    nika-cache              — LLM/HTTP response cache + CacheBackend trait

L4  INTERFACES
    nika-cli                — CLI subcommands (~6k after main.rs split)
    nika-tui-widgets        — Reusable ratatui components
    nika-tui-core           — TuiState, events
    nika-tui-views          — Studio + Command + Control
    nika-tui-app            — Main event loop
    nika-tui                — Facade re-export
    nika-display, nika-lsp, nika-serve, nika-sdk, nika-init

L5  BINARY
    nika                    — <500 LOC composition root
```

**Rule:** Each layer depends only downward. nika-error (L0) has zero internal deps.
**Rule:** Each crate has ONE reason to exist. If you cannot explain it in 10 words, it is wrong.
**Rule:** No crate exceeds 15k LOC of source (excluding tests).
**Rule:** No file exceeds 1500 LOC.
**Rule:** Every side effect (HTTP, exec, FS, LLM, MCP) goes through a trait so it can be mocked.
**Rule:** nika-engine no longer exists post-Constellation — its responsibilities split into nika-runtime + nika-provider + nika-http + nika-exec + nika-builtin.
