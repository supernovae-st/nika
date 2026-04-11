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

## Crate Architecture (Current: 33 Crates post-S14, Target: 36+ post-S15)

### Current Diamond Pattern (v0.79.x, post-Constellation Session 14)

Status: 8 S14 commits landed 2026-04-11 (2 P0 bug fixes + 4 Wave A + 2 Wave B). Session 15 pending.

```
L0    nika-core (~23K)         — AST, types, catalogs, PolicyConfig, trust — ZERO I/O
      nika-event (~4K)         — EventLog, EventKind, TraceWriter
L0.5  nika-kernel              — Trait defs: Provider (+supports_response_format S14-A0),
                                 FsRead/FsWrite splinters, HttpClient (+send_streaming),
                                 ShellExecutor (+cancel), BlobStore, Clock,
                                 PolicyChecker (S12-F1), BuiltinRouter (S13-A0),
                                 McpPool (S13-A0), caps::InferCaps::new (S14-B1),
                                 caps::AgentCaps::new (S14-B1).
                                 InferResponse += cost_usd / request_id (S14-A0).
      nika-kernel-mock         — Mocks for all kernel traits + MockProvider (S14-A0)
L1    nika-clock               — SystemClock (tokio::time, ZST)
      nika-fs                  — TokioFs (FsRead + FsWrite splinter impls, S12-F4)
      nika-blob                — DiskBlobStore (blake3 CAS)
      nika-http                — ReqwestClient (SSRF defense + send_streaming surface)
      nika-exec-runner         — TokioShell with kill_on_drop(true) + tokio::try_join!
      nika-policy              — PolicyEnforcer + SSRF helpers (S12-F5)
      nika-extract             — Pure 9-mode fetch extraction (S12-F7)
      nika-lsp-core            — LSP intelligence (pure functions)
L2    nika-engine (~146.5K)    — MONOLITH (target ≤100k by Phase 15+)
      nika-builtin             — 37/63 builtin tools (Phase 12 substantially done)
      nika-verb-exec (S13-B)   — exec: verb behind ShellExecutor trait
      nika-verb-fetch (S13-D)  — fetch: verb behind HttpClient trait + nika-extract
                                 (+P0 test gaps fixed S14-A2)
      nika-verb-invoke (S13-C) — invoke: verb via BuiltinRouter/McpPool traits
      nika-verb-infer (⭐S14-B1) — infer: verb via Provider trait, 9 tests
      nika-display (13K), nika-media (14K), nika-mcp (9K)
      nika-storage (1K), nika-vault (1.2K)
L3    nika-runtime (S13-A1)    — VerbCapabilities bundle + dispatch() 5-arm +
                                 verb_exec/verb_fetch/verb_invoke adapters (S13) +
                                 verb_infer adapter + infer_caps() (S14-B3).
                                 Provider field on VerbCapabilities (S14-B3).
      nika-daemon (7K)
L4    nika-cli (8K), nika-tui (88K), nika-serve (4K)
      nika-lsp (2.5K), nika-sdk (3K), nika-init (21K)
L5    nika (5.5K)              — Binary entry point (target <900 LOC by Phase 15+)
```

**Session 14 deliverables (2026-04-11):**

Wave A: Foundation
- S14-BUG1 (`0dc079757`): exec.rs NonZeroExit now includes exit_code (Agent C P0)
- S14-BUG2 (`3cc49f3d1`): duplicate McpInvoke event removed (Agent C P0)
- W14-A0 (`e0970025c`): kernel InferResponse += cost_usd/request_id/finish_reason,
  Provider::supports_response_format() default method, MockProvider added to
  kernel-mock with 4 tests. InferResponse now `#[non_exhaustive]`.
- W14-A1 (`58397ed8d`): RigProvider impls supports_response_format (delegates to
  concrete supports_native_structured_output for OpenAI/Groq/DeepSeek/xAI).
- W14-A2 (`c2d486de4`): nika-verb-fetch P0 test gaps fixed — HttpResponse now
  asserts concrete fields, extract: path covered by jsonpath test.

Wave B: Infer extraction
- W14-B0 (`d4885f715`): VerbInvokeError::Mcp → MCP-semantic NikaError variants
  (preemptive cleanup; path unreachable until dispatch() goes live S15).
- W14-B1 (`2ddd28ca1`): ⭐ **nika-verb-infer crate created** — 9 tests, minimum
  extraction: receives pre-resolved prompt/system/model/extras, calls
  Provider::infer(InferRequest) via kernel trait, emits ProviderResponded with
  all metadata. Kernel caps gain InferCaps::new + AgentCaps::new constructors.
- W14-B3 (`040bfad4a`): nika-runtime verb_infer adapter + infer_caps() accessor
  + provider field on VerbCapabilities. Dispatch Infer arm stays NotImplemented
  (cannot build InferInput from AnalyzedInferAction without template resolution).

**Deferred to S15:**

- **W14-B2** — engine infer.rs (2157 LOC) shrinking to ~300 LOC bridge. The
  surgery requires orchestrating spotlight + canary + skills + schema + vision +
  structured-output retry + streaming with precise event-emission ordering.
  Requires rewiring InferCallback's signature through StructuredOutputEngine.
  Multi-session effort. W14-B1 proved the extraction pattern; S15 executes the
  surgery.
- **W14-C (agent extraction)** — rig_agent_loop is 6523 LOC across 9 files with
  10+ TEMP engine dependencies (SkillInjector, LimitTracker, DynamicSubmitTool,
  NikaMcpTool, ProviderKind, STREAM_CHUNK_TIMEOUT, EngineRunExecutor,
  KernelToolAdapter, SecurityContext). S15/S16 territory.
- **W14-A3 (shallow fetch bridge)** — fetch.rs bakes SSRF interceptors into a
  shared reqwest::Client. Bridging would require an HttpClient adapter plus
  robots/rate-limit pre-checks, for marginal LOC gain (~200). Premature.
- **W14-B0 full (McpPoolAdapter)** — kernel McpPool trait is too thin to
  preserve call_tool_with_retry_events, 50MB tool result limits, and the media
  processing pipeline. S15 expands the trait first, then adapts.
- **W14-E0 (shim removal)** — NullBlobStore/NullHttpClient/NoopMcpPool in
  invoke.rs stay until McpPoolAdapter lands (S15).
- **Wave D (TaskExecutor dissolution)** — requires migrating task_dispatch.rs
  binding/lowering to nika-runtime first. S15 prerequisite work, then Wave D
  proper in S15/S16.

**Diamond layering note:** `PolicyConfig` + `SecurityPolicyConfig` live in nika-core (L0). `PolicyEnforcer` concrete impl lives in nika-policy (L1). `nika-kernel::policy::PolicyChecker` trait is object-safe with 4 methods; verb crates consume the trait only. **Post-S14**: nika-verb-infer is the 4th verb crate extracted (exec ✓, fetch ✓, invoke ✓, infer ✓, agent ✗). All 4 live verb crates depend only on nika-kernel traits + nika-event + nika-core — zero engine coupling.

**Constellation progress:** S1-S11 cleanup + trait extension. S12 Foundation (2026-04-10) = kernel trait surface + nika-policy + nika-extract (28 crates). S13 (S13-A through E, 2026-04-10) = 3 verb crates extracted (exec/fetch/invoke) + nika-runtime skeleton + BuiltinRouter/McpPool kernel traits (32 crates, +4 from S12). S14 (2026-04-11) = nika-verb-infer extraction + kernel surface expansion + 2 P0 bug fixes (33 crates, +1). S15 = infer.rs bridge surgery + McpPoolAdapter + agent extraction + Wave D prerequisites.

### Session 12 Sacred Invariants (post-G1/G2/G3)

Every subsequent session must respect these rules learned from S12:

11. **Every `tokio::process::Command` MUST set `cmd.kill_on_drop(true)`** before spawn. G1 lesson.
12. **Every concurrent pipe-reading code MUST use `tokio::try_join!`** with drain futures. NEVER sequential `wait().then().read_to_end()`. G1 lesson.
13. **Every subprocess spawning code MUST be regression-tested with >1 MB output.** G1 lesson.
14. **Golden test oracle MUST capture BOTH lifecycle AND output.** G2 lesson — never weaken for convenience.
15. **Verification ritual MUST include `cargo check --no-default-features`** for crates with feature flags. G3 lesson.
16. **`parking_lot::RwLockReadGuard` is !Send** — NEVER hold across `.await`. Use `Arc<T>` with interior mutability.

### Session 14 Sacred Invariants (post-W14)

17. **Unified `InferRequest` is the kernel's single LLM call shape.** NEVER add separate trait methods for vision/tools/options — encode into `InferRequest` fields (messages with ContentBlock::Image for vision, tools+tool_choice for tool use, extra.params for provider-specific options). Adding `infer_vision` / `infer_with_tools` / `infer_with_options` as trait methods is PROHIBITED. The S14 W14-A0 scope reduction is canonical: 1 trait method (supports_response_format) + 3 InferResponse fields covers everything.
18. **Capability queries belong on the Provider trait, not on `ModelCapabilities`.** Runtime provider dispatch (is Claude an Anthropic reasoning model? does this provider take json_schema response_format?) is trait-level concern — don't force callers to go through the catalog for a Y/N capability check. `supports_response_format()` is the W14 precedent.
19. **Per-crate `new()` constructors on all `#[non_exhaustive]` structs.** When InferResponse/caps/etc. are marked non_exhaustive, add a minimum constructor in the same module. Downstream code must never hit E0639. W14-A0/B1 enforced this for InferResponse + InferCaps + AgentCaps.
20. **Verb-crate minimum extraction is valid architecture.** A verb crate with only the core trait-level call — no streaming, no structured output, no vision — is production-correct even if the engine bridge still owns those paths. The extraction is real when the runtime adapter compiles and the Send proof passes, not only when the engine bridge delegates. W14-B1/B3 set this precedent; S15 finishes W14-B2 on the engine side.
21. **StopReason ↔ FinishReason mapping lives at the verb-crate/event boundary.** nika-kernel stays agnostic of nika-event types; the verb crate centralizes the mapping so there is exactly one place to update when either enum grows a variant.
22. **TEMP engine dependencies must be declared in Cargo.toml with a `# TEMP` comment** explaining what blocks the removal and when it clears. Budgeting TEMP deps is how the constellation refactor stays honest about its debt.

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
