# nika-engine Architecture

The embeddable workflow engine: parses YAML, builds a DAG, resolves bindings,
dispatches tasks to providers, and streams results. ~160k LOC (incl. tests).

## Crate Dependencies (downward only)

```
nika-engine
├── nika-core        AST types, catalogs, policy, trust
├── nika-event       EventLog, TraceWriter
├── nika-media       CAS store, image/document processing
├── nika-mcp         MCP client (rmcp)
├── nika-vault       Encrypted secrets (XChaCha20 + Argon2i)
├── nika-daemon      Background daemon IPC
├── nika-display     CLI renderers (Renderer trait)
└── nika-lsp-core    LSP intelligence (opt-in feature "lsp")
```

## Module Map

### Top-level (`src/`)

| Module | LOC | Responsibility |
|--------|-----|---------------|
| `lib.rs` | ~200 | Public API surface |
| `error.rs` | ~2900 | `NikaError` enum, NIKA-XXX codes, `FixSuggestion` trait |
| `error_domains.rs` | ~250 | Domain sub-enums: `ExecutionError`, `ProviderError`, `BindingError`, `DagError` |
| `config.rs` | ~400 | Configuration types |

### `ast/` — Three-Phase Pipeline

```
YAML → Raw AST → Analyzed AST → Lower (Runtime Types)
```

- `lower.rs` (~2900): Phase 3. Converts Analyzed AST into runtime-ready types.
- `tests_200_workflows.rs` (~10k): Comprehensive YAML parsing tests.

Phase 1 (parser) and Phase 2 (analyzer) live in `nika-core`.

### `binding/` — Data Flow Engine

| File | LOC | Purpose |
|------|-----|---------|
| `template.rs` | ~4900 | `{{with.x \| transform}}` resolution, spotlight wrapping |
| `resolve.rs` | ~3900 | `$task_id.path` resolution, null coalescing (`??`) |
| `jsonpath.rs` | ~500 | JSONPath-style access (`$task.data[0].name`) |
| `token_budget.rs` | ~300 | Token budget tracking for context windows |
| `validate.rs` | ~400 | Template validation (undeclared aliases, inputs) |
| `mention.rs` | ~200 | `@mention` extraction for agent tasks |

**Invariant:** All template resolution is trust-aware. Untrusted data is
spotlight-fenced before interpolation.

### `runtime/` — Execution Core

The heart of the engine. Orchestrates the DAG, dispatches tasks, manages
concurrency, and enforces security policy.

#### Runner (`runtime/runner/`)

- `mod.rs` (~2300): `Runner` struct — builds the execution plan, walks the DAG,
  manages `for_each` fan-out, artifact collection, and result assembly.
  Builder pattern with `#[must_use]` on all 10 chainable methods.
- `tests.rs` (~4800): Runner integration tests.

#### Executor (`runtime/executor/`)

One file per verb:

| File | Purpose |
|------|---------|
| `infer.rs` | LLM generation (streaming + non-streaming, structured output) |
| `exec.rs` | Shell command execution (blocklist, `\| shell` enforcement) |
| `fetch.rs` | HTTP requests (SSRF protection, 9 extract modes) |
| `invoke.rs` | MCP tool calls + 63 builtin tools |
| `agent.rs` | Multi-turn agent loop setup |
| `verbs.rs` | `dispatch_verb()` router |
| `decompose.rs` | Task decomposition helpers |
| `extract.rs` | HTML extraction (markdown, article, metadata, links, feed) |

**Key file:** `infer.rs` (~2150 LOC) contains the 5-layer structured output
pipeline and provider auto-retry logic.

#### Builtin Tools (`runtime/builtin/`)

63 tools accessible via `invoke: nika:*`:

- `router.rs` — Dispatch by tool name (sealed trait, `BuiltinToolRouter`)
- `trait.rs` — `BuiltinTool` trait definition
- `data/` — 13 data tools (jq, map, filter, merge, inject, etc.)
- `media/` — 24 media tools (import, thumbnail, chart, provenance, etc.)
- Individual files for core tools (sleep, log, emit, assert, run, etc.)

#### Security Layer (`runtime/shield.rs`, `canary.rs`, `spotlight.rs`)

6-layer prompt injection defense (Nika Shield):

- `shield.rs` — `SecurityContext` aggregate wrapping `SpotlightFence` + `CanarySystem`
- `canary.rs` — 3 random 16-char canary tokens per run, suffix-injected
- `spotlight.rs` — `wrap_untrusted()` with randomized fence IDs
- `security.rs` — Command blocklist, env validation, NIKA-053

**Invariant:** Trust propagated via `task_local!` in `runner.rs`. Never passed
as function argument to `BuiltinTool::call`.

#### Other Runtime Files

| File | Purpose |
|------|---------|
| `structured_output.rs` | 5-layer structured output engine |
| `structured_retry.rs` | Schema validation retry + LLM repair |
| `artifact_processor.rs` | Persist task outputs to files |
| `for_each.rs` | Parallel loop execution (`concurrency:`, `fail_fast:`) |
| `context.rs` / `context_loader.rs` | Workflow `context:` file loading |
| `skill_injector.rs` | `skills:` auto-injection into system prompts |
| `resolve_typed.rs` | `Templatable<T>` resolution (65 typed fields) |
| `boot.rs` | Startup: env validation, feature detection |
| `policy.rs` | Token budget enforcement, rate limiting |
| `spawn.rs` | `nika:run` nested workflow execution |
| `chat_workflow.rs` | `nika chat` interactive mode |
| `output_scanner.rs` | Post-execution output analysis |
| `mock_json.rs` | Mock provider deterministic responses |

### `provider/` — LLM Providers

- `rig/` — Cloud providers via rig-core (Anthropic, OpenAI, Mistral, Groq, DeepSeek, Gemini, xAI)
- `native/` — Local GGUF inference via mistral.rs (opt-in feature)
- `endpoints.rs` — Custom OpenAI-compatible endpoints (vLLM, TGI, Ollama)
- `cost.rs` — Token cost estimation per model

### `dag/` — DAG Validation

- `flow.rs` — Topological sort, cycle detection
- `indexed.rs` — `IndexedDag` (878 LOC, currently unused by Runner — historical scaffolding)

### `store/` — Runtime State

- `run_context.rs` (~2000): `RunContext` — DashMap-backed task result store,
  context/inputs storage, media staging, workspace root (`OnceLock`).
- `context.rs`: `LoadedContext` for workflow `context:` block.

### `display/` — CLI Renderers

- `renderer.rs` — `Renderer` trait, `CliRenderer` (18 event arms)
- `live.rs` — `LiveRenderer` (indicatif: spinners, progress bars, `for_each` sub-bars)
- `format_event.rs` — 44+ pure `fmt_*()` formatters
- `summary.rs` — `format_run_summary()` + `format_doctor_summary()` (testable)

### Other Modules

| Module | Purpose |
|--------|---------|
| `tools/` | File tools (read, write, edit, glob, grep) + `check_path_readable` (Shield) |
| `io/` | Atomic file I/O |
| `source/` | Source spans + registry for error reporting |
| `mcp/` | MCP server/client helpers |
| `secrets/` | Daemon IPC + vault fallback |
| `registry/` | Package registry client |
| `util/` | Constants, fs helpers, string interner |
| `core/` | Internal re-exports |
| `new/` | `nika new` workflow scaffolding |

## Key Invariants

1. **AST Pipeline:** Always Raw → Analyzed → Lower. Never skip phases.
2. **Trust Propagation:** Via `task_local!` in runner, not function arguments.
3. **Error Type:** Always `NikaError` with NIKA-XXX codes, never `anyhow`.
4. **Timeout Unit:** Always seconds at the API boundary (parser converts to ms).
5. **Binding Prefix:** `with: { alias: $task_id }` — `$` prefix required.
6. **MCP Naming:** `server::tool_name` (double colon), never slash.
7. **Tests:** `cargo test --lib` only (no keychain popups).

## Historical Scaffolding

These exist but are not yet fully wired:

- `IndexedDag` (dag/indexed.rs): 878 LOC adjacency-list DAG built during analysis
  but not used by Runner at execution time. Target: wire into Runner for O(1) lookups.
- `error_domains.rs`: 4 domain sub-enums with `From` impls ready, but most call sites
  still construct `NikaError` directly. Target: promote to primary error path.
- `EventEmitter` trait: defined with 27 structs and 351 emit sites, but events go
  through `EventLog` directly. Target: blanket impl for testable event injection.

## Constellation Refactor Target

This crate is the extraction source for ~15 new crates:
`nika-provider`, `nika-builtin`, `nika-http`, `nika-exec-runner`,
`nika-runtime`, `nika-cache`, `nika-verb-*` (5 crates).
See `docs/plans/2026-04-08-constellation-v2-mega-plan.md`.
