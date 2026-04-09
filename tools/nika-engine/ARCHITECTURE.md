# nika-engine Architecture

The embeddable workflow engine: parses YAML, builds a DAG, resolves bindings,
dispatches tasks to providers, and streams results. ~168k LOC (incl. tests).

Post-Session 6 (Constellation v2.3). Last updated 2026-04-09.

## Crate Dependencies (downward only)

```
nika-engine
├── nika-kernel      Effect traits (Provider, Fs, Clock, Shell, Http, BlobStore)
├── nika-core        AST types, catalogs, policy, trust
├── nika-event       EventLog, TraceWriter
├── nika-builtin     37 builtin tools (core, file, data, introspection) — Phase 12
├── nika-media       CAS store, image/document processing
├── nika-mcp         MCP client (rmcp)
├── nika-vault       Encrypted secrets (XChaCha20 + Argon2i)
├── nika-daemon      Background daemon IPC
├── nika-display     CLI renderers (Renderer trait)
└── nika-lsp-core    LSP intelligence (opt-in feature "lsp")
```

nika-kernel (L0.5) was added in Session 4 (Phase 11). It defines pure traits
with zero I/O — nika-engine implements them via concrete types (RigProvider,
TokioFs, etc.). The 5 L1 effect crates implement the kernel traits:

```
nika-kernel (L0.5, 668 LOC — traits only, zero deps)
├── nika-clock          SystemClock impl
├── nika-fs             TokioFs impl
├── nika-blob           DiskBlobStore impl
├── nika-http           ReqwestClient impl
└── nika-exec-runner    TokioShell impl
     (+ nika-kernel-mock — test doubles for all traits)
```

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
- `trait.rs` — Engine `BuiltinTool` trait definition (adapts kernel trait)
- `media/` — 24 media tools via MediaToolAdapter (import, thumbnail, chart, etc.)
- `fetch_tool.rs` — nika:fetch (SSRF + extract, coupled to PolicyEnforcer)
- `file_adapter.rs`, `rig_adapter.rs` — adapter wrappers

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
| `tools/` | ~4300 | File tools (read, write, edit, glob, grep) + `check_path_readable` (Shield) |
| `io/` | ~2680 | Atomic file I/O, security checks, template I/O |
| `core/` | ~2290 | Internal re-exports, MCP config, paths, storage backend |
| `new/` | ~2600 | `nika new` workflow scaffolding + templates |
| `registry/` | ~2870 | Package registry client (MARKED FOR REMOVAL) |
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
- `registry/` (resolver.rs, operations.rs, lockfile.rs, api.rs, types.rs):
  ~2870 LOC package registry client. MARKED FOR REMOVAL — `nika pkg nuke`
  decision means this entire module will be deleted.

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

Phase 12 (nika-builtin) started in Session 6 — 27/63 builtin tools extracted
into `nika-builtin` crate. Uses `KernelToolAdapter<T>` to bridge kernel
`BuiltinTool` trait (returns `BuiltinError`) to engine `BuiltinTool` trait
(returns `NikaError`). Sealed trait prevents external implementations.

### Next Phases

| Phase | Target | What | Status |
|-------|--------|------|--------|
| 3 | `nika-macros` | 4 derives + 1 declarative macro (syn 2 + darling) | Done (S5) |
| 12 | `nika-builtin` | Extract all 63 builtin tools (~22-24k LOC) | 27/63 done (S6) |
| 13 | verb crates | `nika-verb-{infer,exec,fetch,invoke,agent}` + dedup | Planned |
| 14 | `nika-runtime` | Runner + executor extraction (~30k LOC) | Planned |
| 15 | post-launch | `nika-binding` + `nika-dag` to L0 kernel (~15k LOC) | Planned |

### Size Targets (V2.3, research-backed)

```
Pre-launch (May 5):  nika-engine <= 100k LOC  (from 168k, after Phase 14)
Post-launch:         nika-engine <  80k LOC   (after Phase 15 binding/dag split)
```

Reference: `docs/sprints/CONSTELLATION-V2.3-AGGRESSIVE-TARGETS.md`

Additional V2.3 commitments:
- **blake3 cache** on Analyzed AST boundary (replaces Salsa, 2 weeks vs 2 months)
- **CI ratchet** on `.unwrap()` count — full migration 6-10 weeks
- **nika-macros** — firm: 4 derives, 2-3 weeks for 1 engineer
