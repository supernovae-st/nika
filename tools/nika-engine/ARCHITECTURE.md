# nika-engine Architecture

The embeddable workflow engine: parses YAML, builds a DAG, resolves bindings,
dispatches tasks to providers, and streams results. ~146k LOC (incl. tests).

Post-Session 13 (Constellation v2.3). Last updated 2026-04-10.

## Crate Dependencies (downward only)

```
nika-engine
├── nika-kernel      Effect traits + 5 per-verb Caps structs
├── nika-core        AST types, catalogs, policy config, trust
├── nika-event       EventLog, TraceWriter
├── nika-builtin     37 builtin tools (core, file, data, introspection) — Phase 12
├── nika-policy      PolicyEnforcer + SSRF helpers (L1, S12-F5)
├── nika-extract     Fetch post-processing — 9 extract modes (L2, S12-F7)
├── nika-verb-exec   exec: verb crate (L2, NEW S13-B1) — ShellExecutor trait
├── nika-verb-invoke invoke: verb crate (L2, NEW S13-C1) — BuiltinRouter trait
├── nika-verb-fetch  fetch: verb crate (L2, NEW S13-D1) — HttpClient trait
├── nika-exec-runner TokioShell L1 impl (bridge uses directly for engine→verb handoff)
├── nika-clock       SystemClock L1 impl (bridge uses directly)
├── nika-fs          TokioFs L1 impl (bridge uses directly)
├── nika-media       CAS store, image/document processing
├── nika-mcp         MCP client (rmcp)
├── nika-vault       Encrypted secrets (XChaCha20 + Argon2i)
├── nika-daemon      Background daemon IPC
├── nika-display     CLI renderers (Renderer trait)
└── nika-lsp-core    LSP intelligence (opt-in feature "lsp")
```

`nika-runtime` (L3) sits **above** nika-engine in the diamond — it depends
on the verb crates + kernel + event + core, and provides `VerbCapabilities`
+ `dispatch()`. The engine bridges verb execution to `nika-verb-*::run()`
directly during S13; in S14 the Runner will call `nika-runtime::dispatch`
as the live path.

nika-kernel (L0.5) was added in Session 4 (Phase 11). It defines pure traits
with zero I/O — nika-engine implements them via concrete types (RigProvider,
TokioFs, etc.). The 5 L1 effect crates implement the kernel traits:

```
nika-kernel (L0.5, ~900 LOC — traits only, zero deps)
├── nika-clock          SystemClock impl
├── nika-fs             TokioFs impl (FsRead + FsWrite splinters, S12-F4)
├── nika-blob           DiskBlobStore impl
├── nika-http           ReqwestClient impl (HttpClient::send_streaming, S12-F2)
├── nika-exec-runner    TokioShell impl (CancellationToken support, S12-F3)
├── nika-policy         PolicyEnforcer impl (PolicyChecker trait, S12-F5)
└── nika-extract        L2 pure extraction pipeline (no kernel trait, S12-F7)
     (+ nika-kernel-mock — test doubles for all traits)
```

## Constellation Session 12 Foundation — 2026-04-10

S12 Foundation (11 commits) extended the kernel trait surface and extracted
two pure crates to make Session 13 verb extraction mechanical:

| Commit | Purpose |
|--------|---------|
| S12-F1 | nika-kernel: PolicyChecker trait (object-safe, 4 methods) |
| S12-F2 | nika-kernel: HttpClient::send_streaming + HttpError::TooLarge/Unsupported |
| S12-F3 | nika-kernel: ShellCommand::cancel + ShellError::Cancelled + TokioShell integration |
| S12-F4 | nika-kernel: split Filesystem → FsRead + FsWrite splinters |
| S12-F5 | **New nika-policy L1 crate** (PolicyEnforcer moved from engine, ~1263 LOC) |
| S12-F6 | engine: delete duplicated runtime/policy.rs (`pub use nika_policy as policy`) |
| S12-F7 | **New nika-extract L2 crate** (extract.rs moved from engine, ~1327 LOC) |
| S12-F8 | engine: delete runtime/executor/extract.rs wrapper, rewire callers |
| S12-F9 | nika-kernel: 5 per-verb Caps structs (ExecCaps/FetchCaps/...) — types only |
| S12-F10 | docs: this update |
| S12-F11 | test: golden e2e regression suite for all 5 verbs |

**Engine LOC:** 148,792 → ~146,200 (−2,590)
**Crate count:** 28 → 30 (+nika-policy, +nika-extract)
**Diamond verified:** `cargo tree -p nika-policy --no-default-features` and
`cargo tree -p nika-extract --no-default-features` both have zero nika-engine
dependency. Both crates compile without nika-engine in their dep graph.

## Constellation Session 13 — 2026-04-10

S13 created `nika-runtime` (L3) and extracted 3 verb crates (exec, invoke,
fetch). The verb crates consume kernel traits only; engine bridges delegate
verb execution to `nika_verb_*::run()` after doing template resolution +
security validation.

| Commit | Purpose |
|--------|---------|
| S13-A0 | nika-kernel: expand Caps structs (+ cancel, workflow_base_dir, working_dir_mode, project_root); add BuiltinRouter + McpPool traits; MockPolicyChecker in nika-kernel-mock |
| S13-A1 | **New nika-runtime L3 crate** — VerbCapabilities bundle + dispatch() match + RuntimeError |
| S13-B1 | **New nika-verb-exec crate** — run() via ShellExecutor trait + 11 tests |
| S13-B2 | engine bridge: run_exec delegates to nika_verb_exec::run via TokioShell; ShellCommand gains env_remove + pre_validated fields |
| S13-B3 | GATE-S13-1 regression: >1MB subprocess deadlock test through full bridge |
| S13-B4 | nika-runtime::verb_exec::run_exec adapter + RuntimeError::Exec variant |
| S13-C1 | **New nika-verb-invoke crate** — builtin routing via BuiltinRouter trait + 6 tests |
| S13-C2 | engine bridge: builtin path delegates to nika_verb_invoke::run via BuiltinRouterAdapter (MCP path stays inline for S13) |
| S13-C3 | nika-runtime::verb_invoke::run_invoke adapter + RuntimeError::Invoke variant |
| S13-D1 | **New nika-verb-fetch crate** — HTTP fetch via HttpClient trait + nika-extract pipeline + 4 tests |
| S13-D2 | nika-runtime::verb_fetch::run_fetch adapter + RuntimeError::Fetch variant |
| S13-E1 | docs: this update + session memory |

**Engine LOC:** 146,473 → 146,557 (+84 — exec bridge -322, invoke adapter +271 for BuiltinRouterAdapter/NullBlobStore/NullHttpClient shims, expected to drop in S14 when MCP+media extract)
**Crate count:** 28 → **32** (+nika-runtime L3, +3 verb crates L2)
**Tests:** 10,805 → **10,840** (+35 across the new crates)
**Diamond verified:** all 4 new crates have zero `nika-engine` in their dep
graph (`cargo tree -p nika-runtime | grep nika-engine` → empty for each).
**Golden oracle:** 5/5 green throughout the session — `golden_exec_hello`,
`golden_invoke_builtin_log`, and `golden_fetch_placeholder` all exercise
the bridge paths and prove observable output is preserved.

## Constellation Session 14 — 2026-04-11

S14 split into **wave A–B (pre-launch infer extraction prep)** and a
**post-review hotfix wave (S14.5)** that fixed one P1 correctness bug
and codified three new sacred invariants the post-S14 4-agent review
caught.

### Wave A–B commits (5)

| Commit | Hash | Purpose |
|--------|------|---------|
| S14-α | `c96dec861` | **kernel: enrich `InferEvent::Done` as struct variant.** Mark enum `#[non_exhaustive]`, convert `Done(StopReason)` → `Done { stop_reason, request_id, finish_reason_raw }`. Updates 5 sites atomically: definition, `nika-kernel-mock` MockProvider stream, `nika-engine/.../rig/kernel_bridge.rs` adapter + 2 test matches. New test in kernel-mock asserts request_id threads through synthesized streams. |
| S14-β | `9f384e07a` | **verb-fetch: migrate pure retry/hreflang helpers from engine.** New modules `retry.rs` (4 helpers + 16 tests: `safe_backoff_delay`, `parse_retry_after`, `is_html_content_type`, `MAX_BACKOFF_MS`) and `hreflang.rs` (3 functions + 4 tests, `merge_link_hreflang` made generic over error type). Engine `fetch.rs` strips 277 LOC and re-imports the helpers via `use nika_verb_fetch::{retry,hreflang}::*`. New `nika-engine` dep on `nika-verb-fetch`. |
| S14-γ | `935658eae` | **verb-fetch: add `RetryExhausted` + `DeadlineExceeded` error variants.** Prep for S15 retry loop orchestration extraction. `VerbFetchError` marked `#[non_exhaustive]`. 3 Display tests verify message formatting. |
| S14-δ | `aebea1cd9` | **verb-infer: golden oracle asserts all `ProviderResponded` fields.** Existing test asserted only 3 of 8 fields. New test `infer_emits_provider_responded_with_all_fields` enqueues an `InferResponse` with known values in every optional field (`request_id`, `ttft_ms`, `cost_usd`, `cache_read_tokens`) and asserts each on the emitted event. S12-G2 compliance — never lifecycle-only. |
| S14-ε | `acf9d1784` | **verb-exec: pre-spawn cancellation check.** Add `caps.cancel.is_cancelled()` short-circuit before `caps.shell.run(cmd).await`. Micro-optimisation that mirrors verb-invoke's pattern: skip subprocess fork on already-cancelled task under `fail_fast: true` fan-out. New test uses an empty `MockShell` queue (panics if called) + pre-cancelled token to prove zero subprocess fork happens. |

### Wave 14.5 hotfix commits (2 — post-review)

The post-S14 4-agent review (`code-reviewer` + `rust-architect` ×2 +
`code-explorer`) caught one P1 bug, three architectural violations, and
one symmetry gap. All fixed in two follow-up commits.

| Commit | Hash | Purpose |
|--------|------|---------|
| S14.5-A | `53513e5ee` | **fix: post-S14 review findings hotfix A.** (1) `f64::EPSILON` assertion in S14-δ was mathematically wrong — at magnitude 0.0042 the ULP is ~9.3e-19 not EPSILON ≈ 2.22e-16, accepting up to ~5.3e-14 silent drift. Replaced with `assert_eq!` exact (pure pass-through, no arithmetic). (2) `#[non_exhaustive]` retrofit to `VerbExecError`, `VerbInvokeError`, `VerbInferError` for symmetry with `VerbFetchError`. Engine match sites in `exec.rs:267` and `invoke.rs:183` gain wildcard arms with `format!("unmapped … {other:?}")` for triageability. (3) Retrofit `# TEMP` markers on all 3 `nika-verb-*` deps in `nika-engine/Cargo.toml` per invariant #22. |
| S14.5-B | `144f5abeb` | **docs: codify invariants #23/#24/#25 + correct crate count.** New invariants in `.claude/rules/architecture.md`: **#23** kernel-adjacent helpers stay primitive-typed (no `reqwest::*` / `tokio::*` leaks — triggered by `parse_retry_after(&HeaderMap)` shipped in S14-β); **#24** event emission singletons — exactly one `EventKind::*` emit site per file (currently violated by 7 `ProviderResponded` sites in `infer.rs:621/1156/1330/1388/1527/1592/1898` — W14-B2 must collapse); **#25** all verb-crate errors `#[non_exhaustive]` from day one with wildcard arms in mapping fns. Crate count corrected from "33" to **35** (32 diamond + 3 outside: napi, py, macros). |

### Post-S14 measurements

**Engine LOC:** 146,473 (S13 baseline) → **~146,600** (S14 net: −277 fetch.rs from S14-β, +84 hotfix wildcard arms + TEMP comments, +others = roughly wash). The bulk of engine reduction lives further out in S15+ when bridge surgery starts.

**Crate count:** 32 → **35** (the prior count was off by 2; no new crates landed in S14 itself — S14 only enriched existing crates).

**Tests:** 10,840 → **~10,900** (+1 in `nika-kernel-mock`, +20 in `nika-verb-fetch` via retry+hreflang migration, +3 in `nika-verb-fetch` Display tests, +1 in `nika-verb-infer` golden oracle, +1 in `nika-verb-exec` pre-cancel test, ≈+26 net; rest from W14-A0/A1/A2/B0/B1/B3 pre-S14 commits).

**Verb crate matrix post-S14:**

| Crate | LOC | Tests | Bridge live? | dispatch arm | Errors `#[non_exhaustive]`? | Pre-spawn cancel? |
|-------|-----|-------|--------------|---------------|------------------------------|--------------------|
| `nika-verb-exec` | 446 | 13 | YES (S13-B2) | NotImpl | YES (S14.5) | YES (S14-ε) |
| `nika-verb-fetch` | 297 + retry/hreflang | 28 | partial (helpers only) | NotImpl | YES (S14-γ) | n/a (`tokio::select!` inside `run`) |
| `nika-verb-invoke` | 387 | 6 | partial (builtin only, MCP inline) | NotImpl | YES (S14.5) | n/a (`tokio::select!` inside `run`) |
| `nika-verb-infer` | 499 | 10 | **NO** (W14-B2 deferred) | NotImpl | YES (S14.5) | n/a (`tokio::select!` inside `run`) |

### Sacred invariants added in S14 / S14.5

- **#17** No `infer_vision` / `infer_with_tools` trait methods — unify into `InferRequest` (S14 W14-A0)
- **#18** Capability queries belong on the Provider trait, not on `ModelCapabilities`
- **#19** Per-crate `new()` constructors on every `#[non_exhaustive]` struct in same commit
- **#20** Verb-crate minimum extraction is valid architecture
- **#21** StopReason ↔ FinishReason mapping lives at the verb-crate/event boundary
- **#22** TEMP engine deps must carry `# TEMP` comments with clearance condition
- **#23** Kernel-adjacent helpers use std/primitive/Bytes types only — no `reqwest::*` / `tokio::*` leaks (S14.5)
- **#24** Event emission for a given `EventKind::*` variant happens at exactly one call site per file (S14.5)
- **#25** All verb-crate error enums are `#[non_exhaustive]` from day one (S14.5)

### Known debts entering S15

1. **`McpPool` trait too thin** — 88 LOC, 3 methods. Missing `call_tool_with_retry_events` surface, `read_resource → ResourceContent` (currently returns `String`, drops blob), 50 MB result cap, cancel tokens, `list_tools`. Blocks `McpPoolAdapter`, blocks `NoopMcpPool` removal. **S15-A0/A1/A2/A3 target.**
2. **`infer.rs` has 7 `ProviderResponded` emit sites** (lines 621/1156/1330/1388/1527/1592/1898). Must collapse before W14-B2 flips bridge to `nika_verb_infer::run()` or golden tests see double-emit. **S15/S16 W14-B2 target.**
3. **`parse_retry_after(&reqwest::header::HeaderMap)` leaks L1 type into L2 verb crate signature** (invariant #23 violation shipped in S14-β). **S15-A0 fix:** change signature to `Option<&str>`, move reqwest to `[dev-dependencies]`.
4. **`safe_backoff_delay` silently truncates** for fractional multipliers `0.0 < m < 1.0`. `safe_backoff_delay(1000, 0.8, 2)` returns 1ms instead of ~640ms. **S15 cleanup:** document explicitly OR fix with `factor.round() as u64`.
5. **`finish_reason_raw` plumbed but never consumed** — `stop_reason_to_finish_reason()` at `nika-verb-infer/src/lib.rs:175-184` hardcodes `"content_filter"` instead of using the field. **W14-B2 fix.**
6. **9 TEMP engine deps in `rig_agent_loop/`** (5,363 LOC across 8 files) — Wave C territory, dedicated S15+/S16 plan in `docs/plans/22-agent-v2-design.md`.
7. **Wave D dispatch() activation blocked** — all 5 arms in `nika-runtime::dispatch::dispatch()` return `NotImplemented` because template resolution + binding + skills + spotlight live in `nika-engine`.

### GATE-S13 resolutions

- **S13-1** (deadlock test): added `subprocess_does_not_deadlock_on_large_output`
  in nika-verb-exec end-to-end through TokioShell (B3).
- **S13-2** (Runner move): deferred to S14 — Runner stays in nika-engine
  during S13 per AMEND-4 (dispatch is parallel, not live).
- **S13-3** (PolicyEnforcer !Send): engine bridge clones PolicyEnforcer out
  of the RwLock BEFORE building Caps to avoid holding `parking_lot::RwLockReadGuard`
  across `.await`. Compile-time Send test on each verb adapter future.
- **S13-4/5** (BuiltinRouter/McpPool traits): added in S13-A0 to nika-kernel.
- **S13-6** (error message format): pre-flight grep found no tests asserting
  on exact error substrings in nika-engine test modules. No impact.
- **S13-7** (Caps expansion): done in S13-A0 with constructors to work
  around `#[non_exhaustive]` construction rules.

## Module Map

### Top-level (`src/`)

| Module | LOC | Responsibility |
|--------|-----|---------------|
| `lib.rs` | ~100 | Public API surface |
| `error.rs` | ~2900 | `NikaError` enum, NIKA-XXX codes, `FixSuggestion` trait |
| `error_domains.rs` | ~250 | Domain sub-enums: `ExecutionError`, `ProviderError`, `BindingError`, `DagError` |
| `config.rs` | ~490 | Configuration types |

### `ast/` — Three-Phase Pipeline (~22k LOC)

```
YAML → Raw AST → Analyzed AST → Lower (Runtime Types)
```

| File | LOC | Purpose |
|------|-----|---------|
| `lower.rs` | ~2900 | Phase 3. Converts Analyzed AST into runtime-ready types |
| `action.rs` | ~1900 | Action/task conversion |
| `agent.rs` | ~1400 | Agent task conversion |
| `invoke.rs` | ~490 | Invoke verb conversion |
| `workflow.rs` | ~1030 | Workflow-level conversion |
| `schema_validator.rs` | ~1060 | Schema validation |
| `import_loader.rs` | ~1200 | `include:` partial workflow loading |
| `tests_200_workflows.rs` | ~10k | Comprehensive YAML parsing tests |

Phase 1 (parser) and Phase 2 (analyzer) live in `nika-core`. The analyzer was
split in Session 3 from a single 5531-LOC `analyze.rs` into 6 files under
`nika-core/src/ast/analyzer/analyze/` (mod, cycle_detection, task_table,
validation, verb_analysis, tests).

### `binding/` — Data Flow Engine (~11k LOC)

| File | LOC | Purpose |
|------|-----|---------|
| `template/mod.rs` | ~2050 | `{{with.x \| transform}}` resolution, spotlight wrapping |
| `template/tests.rs` | ~2890 | Template resolution tests |
| `resolve.rs` | **~3950** | `$task_id.path` resolution, null coalescing (`??`) |
| `jsonpath.rs` | ~480 | JSONPath-style access (`$task.data[0].name`) |
| `mention.rs` | ~850 | `@mention` extraction for agent tasks |
| `token_budget.rs` | ~425 | Token budget tracking for context windows |
| `validate.rs` | ~355 | Template validation (undeclared aliases, inputs) |

**Note:** `template.rs` was split into `template/mod.rs` + `template/tests.rs`
in Session 2 (was 4938 LOC monolith).

**God file:** `resolve.rs` at 3,948 LOC was identified in V2.2 as a new god file.
Target: split in Phase 15 (post-launch).

**Invariant:** All template resolution is trust-aware. Untrusted data is
spotlight-fenced before interpolation.

### `runtime/` — Execution Core

The heart of the engine. Orchestrates the DAG, dispatches tasks, manages
concurrency, and enforces security policy.

#### Runner (`runtime/runner/`)

- `mod.rs` (~2350): `Runner` struct — builds the execution plan, walks the DAG,
  manages `for_each` fan-out, artifact collection, and result assembly.
  Builder pattern with `#[must_use]` on all 10 chainable methods.
- `tests.rs` (~4820): Runner integration tests.

#### Executor (`runtime/executor/`)

One file per verb:

| File | LOC | Purpose |
|------|-----|---------|
| `infer.rs` | ~2160 | LLM generation (streaming + non-streaming, structured output) |
| `fetch.rs` | ~1400 | HTTP requests (SSRF protection, 9 extract modes) |
| `extract.rs` | ~1330 | HTML extraction (markdown, article, metadata, links, feed) |
| `exec.rs` | ~470 | Shell command execution (blocklist, `\| shell` enforcement) |
| `invoke.rs` | ~520 | MCP tool calls + 63 builtin tools |
| `agent.rs` | ~600 | Multi-turn agent loop setup |
| `verbs.rs` | ~760 | `dispatch_verb()` router |
| `decompose.rs` | ~350 | Task decomposition helpers |
| `mod.rs` | ~970 | Executor types and dispatch logic |
| `tests*.rs` | ~6700 | Shield, wiremock, and E2E tests |

**Key file:** `infer.rs` (~2160 LOC) contains the 5-layer structured output
pipeline and provider auto-retry logic.

#### Builtin Tools (`runtime/builtin/`) — Split Across Two Crates

63 tools accessible via `invoke: nika:*`. Phase 12 (Constellation) split them:

**nika-builtin crate (37 tools, ~10k LOC, 264 tests):**
- Core (7): sleep, log, emit, assert, complete, prompt, run
- File (5): read, write, edit, glob, grep
- Data (13): jq, map, filter, group_by, chunk, token_count, enrich, zip, set_diff, json_merge, json_diff, tree_data, inject
- Data Sprint 2 (6): json_verify, yaml_validate, locale_lookup, aggregate, json_flatten, json_unflatten
- Introspection (6): cost, dag_info, threads, task_status, records, orchestrate
- Bridged to engine via `KernelToolAdapter<T>` (kernel BuiltinError -> NikaError)

**Still in nika-engine runtime/builtin/ (26 tools):**
- `router.rs` — Dispatch by tool name (sealed trait, `BuiltinToolRouter`)
- `trait.rs` — Engine `BuiltinTool` trait + `KernelToolAdapter<T>` bridge
- `media/` — 25 media tools via MediaToolAdapter (import, decode, thumbnail, chart, etc.)
- `fetch_tool.rs` — nika:fetch (SSRF + extract, coupled to PolicyEnforcer)
- `rig_adapter.rs` — `NikaBuiltinToolAdapter` wraps BuiltinTool as rig ToolDyn

#### Security Layer (`runtime/shield.rs`, `canary.rs`, `spotlight.rs`)

6-layer prompt injection defense (Nika Shield):

- `shield.rs` (~200) — `SecurityContext` aggregate wrapping `SpotlightFence` + `CanarySystem`
- `canary.rs` (~300) — 3 random 16-char canary tokens per run, suffix-injected
- `spotlight.rs` (~140) — `wrap_untrusted()` with randomized fence IDs
- `security.rs` (~2470) — Command blocklist, env validation, NIKA-053

**Invariant:** Trust propagated via `task_local!` in `runner.rs`. Never passed
as function argument to `BuiltinTool::call`.

#### Other Runtime Files

| File | LOC | Purpose |
|------|-----|---------|
| `structured_output.rs` | ~2030 | 5-layer structured output engine |
| `artifact_processor.rs` | ~2770 | Persist task outputs to files |
| `task_dispatch.rs` | ~1110 | Task dispatch orchestration |
| `chat_workflow.rs` | ~1300 | `nika chat` interactive mode |
| `boot.rs` | ~1230 | Startup: env validation, feature detection |
| `policy.rs` | ~1260 | Token budget enforcement, rate limiting |
| `limit_tracker.rs` | ~990 | Cost/time limit enforcement |
| `output.rs` | ~1010 | Output formatting and assembly |
| `skill_injector.rs` | ~540 | `skills:` auto-injection into system prompts |
| `spawn.rs` | ~710 | `nika:run` nested workflow execution |
| `resolve_typed.rs` | ~610 | `Templatable<T>` resolution (65 typed fields) |
| `for_each.rs` | ~525 | Parallel loop execution (`concurrency:`, `fail_fast:`) |
| `context_loader.rs` | ~670 | Workflow `context:` file loading |
| `structured_retry.rs` | ~350 | Schema validation retry + LLM repair |
| `resolver.rs` | ~950 | Binding resolution orchestrator |
| `mock_json.rs` | ~245 | Mock provider deterministic responses |
| `output_scanner.rs` | ~215 | Post-execution output analysis |
| `orchestrate.rs` | ~340 | `nika:orchestrate` sub-DAG routing |

### `provider/` — LLM Providers (~10k LOC)

- `rig/` — Cloud providers via rig-core (Anthropic, OpenAI, Mistral, Groq, DeepSeek, Gemini, xAI)
- `native/` — Local GGUF inference via mistral.rs (~2270 LOC, opt-in feature)
- `endpoints.rs` — Custom OpenAI-compatible endpoints (vLLM, TGI, Ollama)
- `cost.rs` — Token cost estimation per model

#### Kernel Bridge (`provider/rig/kernel_bridge.rs`)

725 LOC. `impl nika_kernel::provider::Provider for RigProvider` — bridges the
kernel Provider trait (L0.5) to the concrete RigProvider enum. The `dispatch_rig!`
macro lives inside the trait impl (connect, don't delete pattern).

`TaskExecutor::get_dyn_provider(name)` returns `Arc<dyn Provider>` — the
keystone method that verb crates (Phase 12+) will consume instead of using
RigProvider directly.

### `dag/` — DAG Validation (~4600 LOC)

- `flow.rs` (~1840) — Topological sort, cycle detection
- `indexed.rs` (~880) — `IndexedDag` (adjacency-list, currently unused by Runner)
- `stable.rs` (~430) — Stable DAG serialization
- `validate.rs` (~1410) — DAG validation rules

### `store/` — Runtime State

- `run_context.rs` (~2000): `RunContext` — DashMap-backed task result store,
  context/inputs storage, media staging, workspace root (`OnceLock`).
- `record_writer.rs` (~260): NDJSON record serialization.
- `context.rs`: `LoadedContext` for workflow `context:` block.

### `lsp/` — Language Server (opt-in, ~12k LOC)

- `server.rs` (~970) — LSP server lifecycle
- `model_intel.rs` (~1510) — Model capability intelligence
- `handlers/` (~7450) — 8 handler files (completion, hover, definition, code_action, etc.)
- `ast_index.rs`, `conversion.rs`, `document_store.rs` — Supporting infrastructure

### `display/` — Re-exports

- `mod.rs` (5 LOC) — Re-exports from `nika-display` crate (extracted in S1).

### Other Modules

| Module | LOC | Purpose |
|--------|-----|---------|
| `tools/` | ~150 | ToolContext + PermissionMode only (check_path_readable + submit_tool removed in S11) |
| `io/` | ~2680 | Atomic file I/O, security checks, template I/O |
| `core/` | ~2290 | Internal re-exports, MCP config, paths, storage backend |
| `new/` | ~2600 | `nika new` workflow scaffolding + templates |
| `secrets/` | ~1080 | Daemon IPC + vault fallback |
| `source/` | ~6 | Source span re-exports |
| `util/` | ~860 | Constants, fs helpers, string interner |
| `mcp/` | ~14 | MCP re-exports |
| `event/` | ~5 | Event re-exports |
| `media/` | ~4880 | Media E2E tests |

## Key Invariants

1. **AST Pipeline:** Always Raw -> Analyzed -> Lower. Never skip phases.
2. **Trust Propagation:** Via `task_local!` in runner, not function arguments.
3. **Error Type:** Always `NikaError` with NIKA-XXX codes, never `anyhow`.
4. **Timeout Unit:** Always seconds at the API boundary (parser converts to ms).
5. **Binding Prefix:** `with: { alias: $task_id }` — `$` prefix required.
6. **MCP Naming:** `server::tool_name` (double colon), never slash.
7. **Tests:** `cargo test --lib` only (no keychain popups).
8. **Provider Access:** Downstream crates use `Arc<dyn Provider>` via `get_dyn_provider()`, never `RigProvider` directly.
9. **Effect Traits:** Every side effect (HTTP, FS, exec, blob, clock) goes through a nika-kernel trait so it can be mocked.
10. **Zero unwrap:** New code must not use `.unwrap()` in production src (CI ratchet enforced per V2.3).

## Historical Scaffolding

These exist but are not yet fully wired:

- `IndexedDag` (dag/indexed.rs): 878 LOC adjacency-list DAG built during analysis
  but not used by Runner at execution time. Target: wire into Runner for O(1)
  lookups (Phase 14).
- `error_domains.rs`: 4 domain sub-enums with `From` impls ready, but most call
  sites still construct `NikaError` directly. Target: promote to primary error
  path (Phase 6, ~16h per V2.2 estimate).
- `EventEmitter` trait: defined with 27 structs and 351 emit sites. Partially
  wired — blanket impl for `Arc<T>` shipped in Session 2 (Phase 5.1). Events
  still go through `EventLog` directly in most paths.

## Constellation Refactor — Current State

### Already Extracted (Sessions 2-6)

8 new crates created since Session 1:

| Crate | Layer | LOC | Session |
|-------|-------|-----|---------|
| `nika-kernel` | L0.5 | 668 | S3 |
| `nika-kernel-mock` | L0.5 | ~200 | S3 |
| `nika-clock` | L1 | ~120 | S3 |
| `nika-fs` | L1 | ~180 | S3 |
| `nika-blob` | L1 | ~250 | S3 |
| `nika-http` | L1 | ~300 | S3 |
| `nika-exec-runner` | L1 | ~200 | S3 |
| `nika-builtin` | L2 | 6440 | S6 |

Phase 11 (Provider bridge) shipped in Session 4 — `kernel_bridge.rs` (725 LOC)
implements `nika_kernel::provider::Provider` for `RigProvider`, enabling
downstream crates to consume `Arc<dyn Provider>` without rig-core dependency.

Phase 12 (nika-builtin) started in Session 6 — 37/63 builtin tools extracted
into `nika-builtin` crate. Uses `KernelToolAdapter<T>` to bridge kernel
`BuiltinTool` trait (returns `BuiltinError`) to engine `BuiltinTool` trait
(returns `NikaError`). Sealed trait prevents external implementations.

Session 10: engine-side file tools deleted (−4k LOC). Agent loop migrated
to nika-builtin file tools. FileToolAdapter, RigFileTool, FileTool trait
all removed.

Session 11: P0 security fixes from S10 verification (3 regressions: write/edit
trust gate, grep/glob sensitive file skip, ReadTool 50MB DoS pre-check) +
registry NUKE (−3,427 LOC). Engine: 152,219 → 148,792 LOC. Rust-security
agent re-verified all three P0 fixes green.

### Next Phases

| Phase | Target | What | Status |
|-------|--------|------|--------|
| 3 | `nika-macros` | 4 derives + 1 declarative macro (syn 2 + darling) | Done (S5) |
| 12 | `nika-builtin` | 37/63 tools migrated. Remaining 26 (media+fetch) stay in engine. | Substantially complete |
| 13 | verb crates | `nika-verb-{infer,exec,fetch,invoke,agent}` + dedup | Planned |
| 14 | `nika-runtime` | Runner + executor extraction (~30k LOC) | Planned |
| 15 | post-launch | `nika-binding` + `nika-dag` to L0 kernel (~15k LOC) | Planned |

### Size Targets (V2.3, research-backed)

```
Pre-launch (May 5):  nika-engine <= 100k LOC  (from 149k, after Phase 14)
Post-launch:         nika-engine <  80k LOC   (after Phase 15 binding/dag split)
```

Reference: `docs/sprints/CONSTELLATION-V2.3-AGGRESSIVE-TARGETS.md`

Additional V2.3 commitments:
- **blake3 cache** on Analyzed AST boundary (replaces Salsa, 2 weeks vs 2 months)
- **CI ratchet** on `.unwrap()` count — full migration 6-10 weeks
- **nika-macros** — firm: 4 derives, 2-3 weeks for 1 engineer
