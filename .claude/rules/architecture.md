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

## Crate Architecture (Current: 28 Crates post-S12 Foundation, Target: 35-36 post-S14)

### Current Diamond Pattern (v0.79.x, post-Constellation Session 12 Foundation)

Status: 22 S12 commits pushed 2026-04-10 (D/P/E + 11 Foundation + 3 Gap fixes). Session 13 NOT YET STARTED.

```
L0    nika-core (~23K)         — AST, types, catalogs, PolicyConfig, trust — ZERO I/O
      nika-event (~4K)         — EventLog, EventKind, TraceWriter
L0.5  nika-kernel              — Trait defs: Provider, FsRead/FsWrite splinters,
                                 HttpClient (+send_streaming), ShellExecutor (+cancel),
                                 BlobStore, Clock, PolicyChecker (NEW S12-F1),
                                 caps module (NEW S12-F9). ZERO impls.
      nika-kernel-mock         — Hand-written mocks for all kernel traits (dev-dep only)
L1    nika-clock               — SystemClock (tokio::time, ZST)
      nika-fs                  — TokioFs (FsRead + FsWrite splinter impls, S12-F4)
      nika-blob                — DiskBlobStore (blake3 CAS)
      nika-http                — ReqwestClient (SSRF defense + send_streaming surface)
      nika-exec-runner         — TokioShell with kill_on_drop(true) + tokio::try_join!
                                 concurrent pipe drain (S12-G1 fixes)
      nika-policy              — ⭐ NEW S12-F5 — PolicyEnforcer + SSRF helpers (~1263 LOC moved from engine)
      nika-lsp-core            — LSP intelligence (pure functions)
L2    nika-engine (~146.5K)    — MONOLITH (−2.3K from S12, target ≤100k by Phase 15)
      + runtime/runner/tests_golden_verbs.rs — 5-verb golden regression oracle (S12-F11/G2)
      nika-builtin             — 37/63 builtin tools (Phase 12 substantially done)
      nika-extract             — ⭐ NEW S12-F7 — Pure 9-mode fetch extraction (~1327 LOC moved from engine)
      nika-display (13K), nika-media (14K), nika-mcp (9K)
      nika-storage (1K), nika-vault (1.2K)
L3    nika-daemon (7K)
L4    nika-cli (8K), nika-tui (88K), nika-serve (4K)
      nika-lsp (2.5K), nika-sdk (3K), nika-init (21K)
L5    nika (5.5K)              — Binary entry point (target <900 LOC by Phase 15)
```

**Diamond layering note:** `PolicyConfig` + `SecurityPolicyConfig` live in nika-core (L0). `PolicyEnforcer` concrete impl lives in nika-policy (L1). `nika-kernel::policy::PolicyChecker` trait is object-safe with 4 methods; verb crates consume the trait only.

**Constellation progress:** S1-S11 = 11 sessions of cleanup + trait extension. S12 Foundation (2026-04-10) = kernel trait surface + 2 new crates (nika-policy, nika-extract). S13 = verb extraction pass 1 (PLANNED). S14 = verb extraction pass 2 + TaskExecutor dissolution (PLANNED).

### Session 12 Sacred Invariants (post-G1/G2/G3)

Every subsequent session must respect these rules learned from S12:

11. **Every `tokio::process::Command` MUST set `cmd.kill_on_drop(true)`** before spawn. G1 lesson.
12. **Every concurrent pipe-reading code MUST use `tokio::try_join!`** with drain futures. NEVER sequential `wait().then().read_to_end()`. G1 lesson.
13. **Every subprocess spawning code MUST be regression-tested with >1 MB output.** G1 lesson.
14. **Golden test oracle MUST capture BOTH lifecycle AND output.** G2 lesson — never weaken for convenience.
15. **Verification ritual MUST include `cargo check --no-default-features`** for crates with feature flags. G3 lesson.
16. **`parking_lot::RwLockReadGuard` is !Send** — NEVER hold across `.await`. Use `Arc<T>` with interior mutability.

### Multi-session refactor protocol (for S13+)

For any session that's part of a multi-commit architectural refactor:
- **Phase 0** — re-absorb context (read plans, ADRs, prior commits, lessons) ~45 min
- **Phase 1** — dispatch 4 review agents IN PARALLEL (code-reviewer, rust-pro, code-explorer, rust-architect) ~20 min
- **Phase 2** — synthesize findings + user sign-off on GATE items ~30 min
- **Phase 3** — execute plan with TDD + golden oracle after every commit

Skipping Phase 1 = shipping bugs that Phase 1 would have caught. Proven in S12 (2× P0 shipped, caught in post-hoc review, fixed as G1/G2).

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
