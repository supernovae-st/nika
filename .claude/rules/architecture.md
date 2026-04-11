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

## Crate Architecture (Current: 35 Crates post-S14, Target: 36+ post-S15)

> **S14.5 correction**: prior rev said 33 crates but `find tools -maxdepth 2 -name Cargo.toml` returns **36 manifests** (1 workspace root + 35 crates). The 35 include `nika-napi` + `nika-py` (FFI shims) and `nika-macros` (proc-macro support). Diamond-participating crates are 32 of those; the other 3 (napi, py, macros) live outside the L0–L5 hierarchy but are real workspace members.

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

**Constellation progress:**

| Phase | Date | Diamond crates | Total tracked | Deliverables |
|---|---|---|---|---|
| S1–S11 | thru 2026-04-09 | — | — | Cleanup + trait extension + Phase 11/12 builtin migration |
| S12 Foundation | 2026-04-10 | 28 → 30 | — | Kernel trait surface + `nika-policy` + `nika-extract` |
| S13 | 2026-04-10 | 30 → 32 | 32 + 3 outside = 35 | 3 verb crates (exec/fetch/invoke) + `nika-runtime` skeleton + `BuiltinRouter` + `McpPool` kernel traits |
| S14 (W14-A0..B3) | 2026-04-11 AM | 32 (no new) | 35 | `nika-verb-infer` lives but engine still owns infer path; kernel surface expansion (`InferResponse.request_id`/`cost_usd`, `supports_response_format`); 2 P0 bug fixes |
| S14 (Wave A–B) | 2026-04-11 PM | 32 (no new) | 35 | `InferEvent::Done` struct variant + verb-fetch retry/hreflang migration + golden oracle + verb-exec pre-cancel (5 commits, c96dec861 → acf9d1784) |
| S14.5 hotfix | 2026-04-11 PM | 32 (no new) | 35 | Post-review hotfix: `f64::EPSILON` exact, `#[non_exhaustive]` retrofit, `# TEMP` markers, invariants #23/#24/#25 codified, ARCH update (3 commits, 53513e5ee → 12407d125) |
| S15 | 2026-04-11 PM | 32 (no new) | 35 | McpPool trait expanded (4 methods, 4 new DTOs, `async_trait`) + `McpPoolAdapter` in `nika-engine/runtime/mcp_pool_adapter.rs` + invoke.rs MCP path routed through adapter + `parse_retry_after` invariant #23 fix (`Option<&str>`, `reqwest` → dev-deps) + `MockMcpPool` fixtures in `nika-kernel-mock` + `run_with_retry` + `RetryPolicy` forward-investment in `nika-verb-fetch`. 7 commits (8c11d2eed → 38ee31418). Tests ~10,897, clippy 26 unchanged. **Defer:** W14-B2 infer surgery to S16. **Defer:** NoopMcpPool/NullBlobStore/NullHttpClient removal (needs BlobStoreAdapter + HttpClientAdapter). **Defer:** Wave C agent + Wave D dispatch to S16/S17. |
| S16 | 2026-04-11 PM | 32 (no new) | 35 | **Option A (reduced scope):** W14-B2 bridge flip found architecturally blocked at Phase 1 — engine has no non-streaming `Provider::infer()` call path, every text path uses `infer_stream_with_options`. Delivered the mechanically-possible subset instead: (1) **S15.5 hotfix** (3 post-S15 review fixes — MockMcpPool docstring, McpPoolAdapter owned-types, invoke.rs double-cancel comment), (2) **S16-flake-fix** (deterministic `SecretStore` state bleed via `clear_all_provider_env_vars` — 11 tests were failing with "anthropic" instead of "groq" because the in-process DashMap wasn't cleared alongside env vars), (3) **W16-A0** (extract `pub fn emit_provider_responded` to `nika-verb-infer/src/emit.rs`, route all 7 engine sites + 1 verb-crate site through the helper — invariant #24 satisfied), (4) **W16-B1** (engine-side golden oracle `test_run_infer_mock_emits_provider_responded_with_all_fields` — mirrors the verb-crate S14-δ golden for the engine mock fast-path), (5) **W16-B3** (add `finish_reason_raw: Option<String>` to `InferResponse`, thread through `stop_reason_to_finish_reason` with option (ii): typed StopReason authoritative, raw string preferred for `ContentFilter` / `Unknown` — closes S15 debt #7), (6) **W16-A1** (5 verb-crate coverage gap tests from Phase 1 rust-pro audit), (7) **drive-by** (delete tautological `test_theme_dark_is_default` — literally asserted `x == x`, clippy baseline 26 → 24). 7 commits + 1 drive-by (800cd2683 → 9a60c681e). Engine 146,839 → 147,020 LOC (+181 net, W16-A0 is invariant #24 refactor not shrinkage). Tests 10,897 → 10,911 (+14 net). **Defer to S17+:** W14-B2 proper (needs streaming in verb crate), NoopMcpPool/NullBlobStore/NullHttpClient removal, engine fetch retry loop migration, dispatch activation, Wave C agent, 11 exec/runner load-flakes (separate class from deterministic flakes, documented in project_s17_followup memory). |

**Note on crate counting**: 32 = diamond-participating crates (L0–L5). The total of 35 includes 3 outside-the-diamond workspace members: `nika-napi`, `nika-py` (FFI shims), `nika-macros` (proc-macro support). Both numbers are correct depending on whether you're measuring architectural surface (32) or workspace size (35). Prior revs of this doc said "33 crates, +1" referring to mid-S14 state before W14-B1 — that count is now stale. Authoritative: **32 diamond, 35 total**.

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

### Session 14.5 Sacred Invariants (post-review — 2026-04-11)

Added after the 4-agent post-S14 review caught one invariant violated at
birth (`parse_retry_after` reqwest leak), one missing symmetrically
(`#[non_exhaustive]` only on `VerbFetchError`), and one latent (7
`ProviderResponded` emission sites in `infer.rs` that S14-δ's new oracle
cannot regress-test across).

23. **Kernel-adjacent helpers use std / primitive / `bytes::Bytes` types only** —
    NEVER expose `reqwest::*`, `tokio::*`, `sqlx::*`, or any L1+ concrete
    type in a kernel trait signature, verb-crate public helper signature,
    or re-exported alias. The `parse_retry_after(headers: &reqwest::header::HeaderMap)`
    signature in `nika-verb-fetch::retry` (landed S14-β) is the
    precedent-violating case — S15-A0 must refactor to
    `parse_retry_after(header_value: Option<&str>)` so the helper is
    reqwest-free and the direct dep moves to `[dev-dependencies]`.
    When kernel traits need structured headers, use `HashMap<String, String>`
    as `nika-kernel::http::HttpRequest` already does.

24. **Event emission for a given `EventKind::*` variant happens at exactly
    one call site per verb execution path.** If the same variant is emitted
    from N sites in a single file, refactor them through a single
    `emit_<variant>(ctx, …)` helper so that adding a field is a one-site
    change and the golden oracle has exactly one regression target.
    Current violator: `nika-engine/src/runtime/executor/infer.rs` emits
    `EventKind::ProviderResponded` from 7 sites (lines 621, 1156, 1330,
    1388, 1527, 1592, 1898). W14-B2 (S15) must collapse these before any
    new field lands on the event.

25. **All verb-crate error enums are `#[non_exhaustive]` from day one.**
    Downstream `From<VerbXxxError> for NikaError` impls must carry a
    wildcard arm that falls through to a generic variant with a
    `format!("unmapped verb error variant: {other:?}")` message so any
    future variant stays triageable from logs. S14-γ shipped
    `#[non_exhaustive]` only on `VerbFetchError`; S14.5 retrofit applied
    it to `VerbExecError`, `VerbInvokeError`, `VerbInferError` with
    wildcard arms in `exec.rs` + `invoke.rs` mapping functions.

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
