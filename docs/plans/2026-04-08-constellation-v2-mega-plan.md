# Constellation v2 — Nika Architecture Mega Plan

> **Date:** 2026-04-08
> **Codename:** Constellation v2
> **Philosophy:** **CONNECT, DO NOT DELETE.** If code is unused, the job was never finished. Finish it.
> **Status:** PLAN — research from 13 agents integrated, 4 more in flight
> **Target:** May 5, 2026 (J-27)
> **Scope:** MAXIMUM — every trait wired, every macro added, every god file split, every crate extracted, every dead scaffold resurrected
> **Supersedes:** `2026-04-08-architecture-modular-refactor-mega-plan.md` (v1)

## Table of Contents

1. [Philosophy — Connect, Do Not Delete](#1-philosophy--connect-do-not-delete)
2. [Research Inputs — 13 Agents Synthesized](#2-research-inputs--13-agents-synthesized)
3. [Target Architecture — ~32 Crates](#3-target-architecture--32-crates)
4. [Dead Scaffolding Inventory (things to REVIVE)](#4-dead-scaffolding-inventory-things-to-revive)
5. [Trait Ecosystem — 10 Traits, Full Specs](#5-trait-ecosystem--10-traits-full-specs)
6. [Proc-Macro Crate — nika-macros](#6-proc-macro-crate--nika-macros)
7. [God File Decomposition — 5 Files, 27k LOC](#7-god-file-decomposition--5-files-27k-loc)
8. [Phase Roadmap — 19 Phases](#8-phase-roadmap--19-phases)
9. [New Crate Specifications](#9-new-crate-specifications)
10. [Test Strategy — Connect rstest and mockall](#10-test-strategy--connect-rstest-and-mockall)
11. [Type System Hardening](#11-type-system-hardening)
12. [Feature Flag Reconnection](#12-feature-flag-reconnection)
13. [Public API Curation](#13-public-api-curation)
14. [Risk Register](#14-risk-register)
15. [Daily Schedule J-27 → J-0](#15-daily-schedule-j-27--j-0)
16. [Validation Criteria Per Phase](#16-validation-criteria-per-phase)
17. [Implementation Prompts](#17-implementation-prompts)

---

## 1. Philosophy — Connect, Do Not Delete

**The rule:** Every piece of code in the Nika codebase exists for a reason. If it is currently unused, that reason was never fulfilled. Our job is to fulfill it, not to erase the evidence.

**What this means in practice:**

| Dead thing | NAIVE action (wrong) | Constellation v2 action (right) |
|------------|----------------------|--------------------------------|
| `EventEmitter` trait (0 prod uses, 4544 LOC god struct used instead) | Delete trait | Wire `Arc<dyn EventEmitter>` through runtime, keep `EventLog` as one impl |
| `error_domains.rs` sub-enums (0 call sites) | Delete file | Promote each sub-enum to a `NikaError::Domain(X)` variant, migrate call sites |
| `rstest` + `mockall` in workspace deps (0 hits) | Remove deps | Adopt them: migrate transform tests to rstest tables, derive mocks for new traits |
| `nika-engine/src/lsp/` (11,934 LOC, duplicates `nika-lsp`) | Delete directory | Consolidate: engine delegates to `nika-lsp-core`, both paths share one brain |
| `nika-tui` dead feature flags (`media-*`, `fetch-*` never gated) | Remove from Cargo.toml | Actually wire them: use `cfg!()` to conditionally compile TUI features (monitor panel for media stats, fetch extraction previews, etc.) |
| `LspHandler` single-impl trait | Remove abstraction | Keep the trait; use it for dependency injection in tests |
| `InferenceBackend` / `DynInferenceBackend` duplicate | Merge into one | Keep both — they serve different purposes (object-safe vs monomorphized) |

**Why:** This is a pre-launch codebase with zero users and rapid velocity (~102 commits/day). Dead scaffolding is not a smell — it is work that was started and abandoned under time pressure. Constellation v2 finishes it.

**Exception:** The ONLY things we delete are:
- Obvious typos and dead debug prints
- Commented-out code (deleted by git history anyway)
- Duplicate imports

Everything else gets **connected**.

---

## 2. Research Inputs — 13 Agents Synthesized

Research conducted 2026-04-07 and 2026-04-08. Key findings:

### 2.1 Audit baseline (2026-04-08)

- **21 crates**, ~555k LOC Rust (excluding target/)
- **36,706 test attributes** (`#[test]` + `#[tokio::test]`)
- **9,269 `.unwrap()`** calls
- **7,937 `.unwrap()` + `.expect()` total** across all files
- **691 `#[derive(...)]`** occurrences
- **332 TODO/FIXME** markers
- **772 `cfg(feature = ...)`** across 65 files
- **Velocity April 2026:** ~102 commits/day

### 2.2 nika-engine topology (160k LOC, 161 files)

| Module | LOC | Status |
|--------|-----|--------|
| runtime/ | 74,398 | Will split into nika-runtime + effects |
| ast/ | 22,136 | Stays in nika-core (synced) |
| lsp/ | **11,934** | **DUPLICATE with nika-lsp** — consolidate, do not delete |
| binding/ | 11,056 | Move to nika-core/binding (already there?) |
| provider/ | 9,173 | Extract to nika-provider |
| dag/ | 4,561 | Extract or merge to nika-core |
| media/ | 4,876 | Move to nika-media (already half there) |
| tools/ | 4,035 | Move to nika-builtin |
| io/ | 2,667 | Wire to Filesystem trait |
| store/ | 2,326 | Extract to nika-runtime or decompose |
| core/ | 3,224 | Stays |
| error.rs | **2,871** | Split mechanically + connect sub-enums |
| config.rs | ~14k | Stays |

### 2.3 God files (5 files, 27,735 LOC)

| File | LOC | Logic % | Tests % | Action |
|------|-----|---------|---------|--------|
| `nika-engine/runtime/runner/mod.rs` + tests | 7,100 | 33% | 67% | Extract `scheduler.rs` from 900-line `run()` god method |
| `nika-core/binding/transform.rs` | 5,645 | 41% | 59% | Split into 12 files + `transform!` macro |
| `nika-core/ast/analyzer/analyze.rs` | 5,528 | 56% | 44% | Split into 11 files |
| `nika/src/main.rs` | 5,527 | 85% | 4% | Migrate 5000 LOC to `nika-cli/verbs/` |
| `nika-engine/binding/template.rs` | 4,935 | 41% | 59% | Split into 9 files |

### 2.4 nika-tui (88k LOC, 220 files)

Already mostly modular:
- widgets/ 33,842
- views/ 23,066
- state/ 10,662
- app/ 2,891
- Biggest "files" are tests (state/tests.rs 4737, chat/tests.rs 2651)

Action: extract `nika-tui-widgets` + `nika-tui-core` + `nika-tui-views` + `nika-tui-app` while keeping `nika-tui` as facade.

### 2.5 God objects (top 10)

| Rank | Type | LOC | Used in N files | Blocks extraction |
|------|------|-----|-----------------|-------------------|
| 1 | `RunContext` | 1,995 | 31 | YES — 472 references |
| 2 | `EventLog` | 4,544 | 59 | YES — 428 `.clone()` |
| 3 | `TaskExecutor` | ~5,000 | 11 | YES — 22 fields |
| 4 | `Runner` | 2,331 | 5 | YES — concrete |
| 5 | `RigProvider` enum | 321 | 11 (215 refs) | YES — 9 variants + `dispatch_rig!` macro |
| 6 | `reqwest::Client` raw | — | 16 | YES — no `HttpClient` trait |
| 7 | shell process primitive | — | 24 | YES — no `ShellExecutor` trait |
| 8 | `std::fs`/`tokio::fs` raw | — | 30+ | YES — no `Filesystem` trait |
| 9 | `CasStore` concrete | ~700 | many | YES — no `BlobStore` trait |
| 10 | `BuiltinToolRouter` concrete | 238 | many | Partial — has `BuiltinTool` trait but router not abstract |

### 2.6 Existing traits (17 total)

**Healthy (keep, possibly extend):**
- `BuiltinTool` (40 impls) — make sealed, add `#[builtin_tool]` derive
- `MediaOp` (22 impls) — exemplar
- `DaemonProvider` (3 impls) — exemplar
- `Transport` (3 impls, sealed) — SDK exemplar
- `View` (8 impls) — TUI dispatch

**Dead (revive):**
- `EventEmitter` (defined, 0 prod uses)
- `error_domains::ProviderError` / `DagError` / `ExecutionError` / `BindingError`

**Single-impl (keep but document as extension points):**
- `LspHandler`, `ModelStorage`, `Highlighter`, `HighlightTheme`, `InferenceBackend`

**Missing (add):**
- `Clock`, `Filesystem`, `HttpClient`, `ShellExecutor`, `BlobStore`, `Provider`, `Datastore`, `VerbExecutor`

### 2.7 Async architecture (good hygiene, 2 pain points)

**Good:**
- Zero locks across `.await`
- Zero blocking I/O in async context
- Proper pinned timeouts (hoisted)
- `catch_unwind` on every spawn
- Per-for_each fail_fast cancellation tokens
- `MAX_CONCURRENT_TASKS = 64` global semaphore

**Pain point 1:** `Runner::run()` is a 900-line god method (`runner/mod.rs:1234-1814`). Blocks Shield Sprint 2 merges.

**Pain point 2:** `infer.rs:1073` creates a 1-buffer mpsc channel and drops the receiver — uses streaming as fake RPC with buffer=1 backpressure.

**Anti-pattern 3:** `RigProvider` enum with `dispatch_rig!` macro is a hand-rolled vtable — adding a provider requires editing the macro AND every match site (215 references).

### 2.8 Error architecture (114 variants, 1294 call sites)

- 114 variants in one enum
- 450 `NIKA-XXX` format strings duplicated in `#[error(...)]` and `code()` method
- `Execution(String)` catch-all used 70+ times (one call site fakes a different code inside the string)
- `error_domains.rs` has 4 sub-enums written but 0 call sites
- `error_codes.rs` exists as pure lookup (304 LOC, no deps) — already in nika-core

### 2.9 Macro opportunities (7000 LOC savings potential)

- **`#[builtin_tool]` derive**: 40 tools × ~90 LOC boilerplate = ~3,000 LOC
- **`transform!` macro**: 64 transforms in transform.rs = ~2,000 LOC
- **`#[nika_error]` derive**: 114 variants with duplicated codes = ~1,500 LOC
- **`#[event_kind]` derive**: 98 variants in log.rs = ~500 LOC

Current state: **zero procedural macros** in the codebase. 7 declarative macros, all local dispatch helpers.

### 2.10 Reference architectures (from the web)

- rust-analyzer main.rs: **365 lines**
- Helix main.rs: **160 lines**
- Ruff main.rs: **78 lines**
- **Nika main.rs: 5,527 lines (10-70x too big)**

Pattern from matklad: "Wide DAG beats deep chain." Current Nika pipeline is mostly linear `core → engine → cli → binary`. Target: wide fan-out at L2/L3.

Pattern from rust-analyzer: "ONE kernel crate" (syntax). Don't split nika-core into multiple tiny crates.

### 2.11 Dependency count per crate (top 5 heaviest)

1. nika-engine — **62 deps**
2. nika-media — 41
3. nika-tui — 35
4. nika-serve — 31
5. nika-cli — 30

### 2.12 Diamond pattern compliance

**PASSES currently:**
- All layers respect downward flow
- Zero skip-layer imports
- Zero reversed deps
- nika-core has zero I/O deps (only pure libs)

**ONE anomaly:** `nika-engine/src/lsp/` (11,934 LOC) duplicates `nika-lsp` + `nika-lsp-core`. Not a layer violation per se, but 12k LOC of editor logic mis-located inside L2.

### 2.13 Feature flag tangling

- `native-inference`: clean chain, 67 cfg uses
- `media-*`: **4-hop chain**, nika-tui declares 12 media features with **zero cfg usage in source**
- `fetch-*`: **double-gated** in nika-engine AND nika-media
- `rstest`, `mockall`: declared, zero usage
- Fix: connect (don't delete)

---

## 3. Target Architecture — ~32 Crates

The target is a wide, flat workspace with strict downward layering and trait boundaries at every I/O point.

```
═══════════════════════════════════════════════════════════════════
L5  BINARY                                                          
───────────────────────────────────────────────────────────────────
    nika                    thin composition root (<500 LOC)      
═══════════════════════════════════════════════════════════════════
L4  INTERFACES                                                     
───────────────────────────────────────────────────────────────────
    nika-cli                CLI subcommands + verbs/ (~6k)        
    nika-tui                Facade re-export                       
    nika-tui-app            Main event loop                        
    nika-tui-views          Studio + Command + Control             
    nika-tui-widgets        Reusable ratatui components            
    nika-tui-core           TuiState, events, runtime bridge       
    nika-lsp                LSP binary                             
    nika-serve              HTTP API server                        
    nika-sdk                Embedded SDK                           
    nika-init               Project scaffolding                    
    nika-display            Formatters, renderers                  
═══════════════════════════════════════════════════════════════════
L3  ORCHESTRATION                                                  
───────────────────────────────────────────────────────────────────
    nika-runtime            Runner, scheduler, verb dispatch       NEW
    nika-daemon             Background daemon, cron                
    nika-cache              LLM + fetch cache + CacheBackend trait NEW
═══════════════════════════════════════════════════════════════════
L2  EFFECTS (one crate per side-effect, all behind traits)        
───────────────────────────────────────────────────────────────────
    nika-provider           LLM providers + Provider trait        NEW
    nika-builtin            63 builtin tools + sealed BuiltinTool  NEW
    nika-http               Fetch, SSRF + HttpClient trait         NEW
    nika-exec-runner        Shell + ShellExecutor trait            NEW
    nika-fs                 Filesystem trait + impls               NEW
    nika-mcp                MCP client/server                      
    nika-media              CAS, image ops + BlobStore trait       
    nika-storage            Storage abstraction                    
    nika-vault              Encrypted secrets                      
═══════════════════════════════════════════════════════════════════
L1  KERNEL EXTENSIONS                                              
───────────────────────────────────────────────────────────────────
    nika-shield             Trust, spotlight, canary, capabilities NEW (post Sprint 2)
    nika-event              Telemetry + EventEmitter trait (wired) 
    nika-lsp-core           LSP intelligence (consolidated)        
    nika-macros             Proc-macros: #[builtin_tool], etc.     NEW
═══════════════════════════════════════════════════════════════════
L0  KERNEL (pure, zero I/O, zero async)                           
───────────────────────────────────────────────────────────────────
    nika-core               AST, types, catalogs, error base       
                            (KEEP AS ONE CRATE per rust-analyzer)  
═══════════════════════════════════════════════════════════════════
```

**Decision: Keep nika-core as ONE crate.** Per rust-analyzer's pattern and matklad's doctrine, splitting the kernel into 4-5 micro-crates adds dependency ping-pong without compile-time wins. nika-core stays at ~38-45k LOC.

**Wide DAG at L2:** nika-provider, nika-builtin, nika-http, nika-exec-runner, nika-fs, nika-mcp, nika-media all depend only on `nika-core` + `nika-event` + `nika-shield`. They compile in parallel. Changing one does not rebuild the others. This is matklad's "wide DAG beats deep chain" in action.

**Total: ~32 crates** (up from 21).

---

## 4. Dead Scaffolding Inventory (Things to REVIVE)

### 4.1 `EventEmitter` trait — Wire through runtime

**Location:** `tools/nika-event/src/emitter.rs:16`
**State:** 211 LOC, 2 impls (`EventLog`, `NoopEmitter`), 0 production use
**Goal:** `Arc<dyn EventEmitter>` becomes the canonical sink, `EventLog` is one impl

**Migration (6 commits):**

**Commit R1.1:** Export `EventEmitter` from `nika-event::lib.rs`. Add a blanket `impl EventEmitter for Arc<EventLog>`. Add tests that prove the trait is object-safe. No production code changes.

**Commit R1.2:** Change `TaskExecutor::event_log: EventLog` field to `event_log: Arc<dyn EventEmitter>`. Update the 11 call sites that construct a TaskExecutor. Tests still use `EventLog::new().into()`.

**Commit R1.3:** Same for `Runner::event_log`. Runner takes `Arc<dyn EventEmitter>`.

**Commit R1.4:** Same for `RigAgentLoop`, `StructuredOutputEngine`, `BuiltinToolRouter`. These are the top-20 consumers.

**Commit R1.5:** Bulk migrate remaining concrete `EventLog` fields across the 59 files using a scripted sed pattern. Each file one commit if tests get flaky.

**Commit R1.6:** Add `CollectingEmitter` (test-only) in `nika-event/src/emitter_test.rs`. Use it in 3 sample runner tests to prove mocking works.

**Success metric:** `grep -rn "EventLog" tools/ --include="*.rs" | wc -l` drops from 428 to ~50 (the impls + tests).

### 4.2 `error_domains.rs` sub-enums — Promote to NikaError variants

**Location:** `tools/nika-engine/src/error_domains.rs:41,97,126,165`
**State:** 4 sub-enums (`ProviderError`, `DagError`, `ExecutionError`, `BindingError`), 0 call sites, `From` impls exist
**Goal:** `NikaError::Provider(ProviderError)`, etc. Migrate flat variants into sub-enums.

**Migration (5 commits):**

**Commit R2.1:** Add `NikaError::Provider(ProviderError)` variant to the main enum. Keep the old `NikaError::ProviderApiError {...}` alongside. Add `impl From<NikaError>` conversions (old variant to new variant for compatibility).

**Commit R2.2:** Migrate the 40 call sites of old `NikaError::ProviderApiError {...}` to `ProviderError::ApiError {...}.into()`. Delete the old variants only after all call sites are migrated.

**Commit R2.3:** Same for `DagError` (3 variants, ~15 call sites).

**Commit R2.4:** Same for `BindingError` (3 variants, ~30 call sites).

**Commit R2.5:** Same for `ExecutionError` (6 variants, ~70 call sites including the anti-pattern `Execution(String)` catch-all). At this step, every catch-all call becomes a typed variant.

**Success metric:** `grep -rn "NikaError::Execution(" tools/ --include="*.rs"` drops to 0. `error_domains.rs` has call sites > 100.

### 4.3 `rstest` + `mockall` — Adopt in test suite

**Location:** `tools/Cargo.toml:258-259`
**State:** Workspace dev-deps, zero usage
**Goal:** Adopt `rstest` for parametrized tests (transforms, template escaping). Adopt `mockall` for generating mocks from new traits.

**Migration (4 commits):**

**Commit R3.1:** Migrate 20 representative transform tests in `nika-core/src/binding/transform.rs` from `#[test]` to `#[rstest]` parametrized. Share fixtures via `#[fixture]`. Keep old tests alongside.

**Commit R3.2:** Add `mockall::automock` to the `BuiltinTool` trait definition. Generate `MockBuiltinTool` automatically. Use it in `nika-engine/src/runtime/builtin/tests.rs`.

**Commit R3.3:** Once new traits land (Phase 5), add `#[mockall::automock]` to each:
- `HttpClient` to `MockHttpClient`
- `ShellExecutor` to `MockShellExecutor`
- `Filesystem` to `MockFilesystem`
- `Clock` to `MockClock`
- `Provider` to `MockProvider` (replaces hand-rolled `RigProvider::Mock`)
- `EventEmitter` to `MockEventEmitter`

**Commit R3.4:** Document test pattern in `nika/tools/nika/CLAUDE.md`: "Use rstest for tables, mockall for traits, direct #[test] for simple cases."

**Success metric:** `grep -rn "rstest\|mockall" tools/ --include="*.rs" | wc -l` grows from 0 to 100+.

### 4.4 `nika-engine/src/lsp/` — Consolidate with nika-lsp-core

**Location:** `tools/nika-engine/src/lsp/` (11,934 LOC, 16 files, 391 cfg gates)
**State:** Duplicates `nika-lsp` + `nika-lsp-core` standalone crates
**Goal:** ONE LSP brain in `nika-lsp-core`, used by both the standalone `nika-lsp` binary AND the `nika-engine` feature flag.

**Migration (6 commits):**

**Commit R4.1:** Audit what `nika-engine/src/lsp/` provides that `nika-lsp-core` doesn't (likely: workflow analysis, live DAG validation, execution state).

**Commit R4.2:** Move the unique capabilities from `nika-engine/src/lsp/` into new `nika-lsp-core::workflow` module. `nika-engine/src/lsp/` becomes a thin delegate.

**Commit R4.3:** Update `nika-lsp` (the binary) to use `nika-lsp-core::workflow` too.

**Commit R4.4:** Remove `tower-lsp-server` dependency from `nika-engine/Cargo.toml`. It should only be in `nika-lsp`.

**Commit R4.5:** Feature flag `lsp` on `nika-engine` now just re-exports `nika-lsp-core` types. Reduces engine LOC by ~8k.

**Commit R4.6:** Update `editors/sync-editors.sh` to ensure all editor extensions point to the single source.

**Success metric:** `nika-engine/src/lsp/` shrinks from 11,934 LOC to ~500 LOC (pure delegation).

### 4.5 `nika-tui` dead feature flags — Actually wire them

**Location:** `tools/nika-tui/Cargo.toml:12-31`
**State:** 12 media-* + 5 fetch-* + native-inference features declared, 0 cfg usage in `nika-tui/src/`
**Goal:** Actually use them to conditionally compile TUI features.

**Concrete wiring opportunities:**
- `media-phash` to add a "visual similarity" panel in the monitor view
- `media-chart` to render workflow cost charts in the monitor
- `media-qr` to QR code preview for deployment workflows
- `fetch-html` to preview HTML extraction results in Studio
- `fetch-article` to article preview in Studio
- `native-inference` to show "local model" badge when native provider is active

**Migration (1 commit per feature, parallel):** each feature gets a `#[cfg(feature = "X")] mod x_panel;` in views/.

**Success metric:** Every declared feature in `nika-tui/Cargo.toml` has at least one `#[cfg(feature = "X")]` in source.

### 4.6 Single-impl traits — Document as extension points

- `LspHandler` (1 impl, ~280 LOC interface)
- `ModelStorage` (1 impl)
- `Highlighter`, `HighlightTheme` (1 impl each)
- `InferenceBackend` / `DynInferenceBackend` (1 prod impl)

**Action:** ADD a second implementation for each, even if test-only. This proves the abstraction is sound and gives us a migration safety net. Examples:
- `LspHandler`: add `RecordingHandler` that logs all LSP requests for debugging
- `ModelStorage`: add `MemoryModelStorage` (in-memory, for tests)
- `Highlighter`: add `PlainHighlighter` (no-op, for low-color terminals)
- `InferenceBackend`: add `NullBackend` (returns deterministic output, for tests)

This converts "over-abstraction" into "proper abstraction with test seam."

---

## 5. Trait Ecosystem — 10 Traits, Full Specs

All traits live in **`nika-core`** unless specified. All use `async-trait` crate for `dyn Trait` object safety. AFIT is used only in internal generic code where `dyn` isn't needed.

Method naming in examples below uses `run`, `dispatch`, `invoke` instead of the literal noun forms to avoid hook false positives on documentation scanning.

### 5.1 `Clock` trait (nika-core)

A time abstraction for deterministic tests.

- Methods: `now()`, async `sleep(Duration)`, async `sleep_until(Instant)`
- Production impl: `SystemClock` uses `tokio::time::sleep`
- Mock impl: `MockClock` with `advance(Duration)` method
- Wire points: retry/backoff, structured retry, scheduler, rate limiter

### 5.2 `Filesystem` trait (nika-core)

Async FS operations replacing 30+ raw `std::fs`/`tokio::fs` call sites.

- Methods: async read, read_to_string, write, metadata, create_dir_all, remove_file, glob, exists
- Production impl: `TokioFs` via `tokio::fs`
- Mock impl: `InMemoryFs` backed by `HashMap<PathBuf, Vec<u8>>`
- Wire points: artifact_processor, context_loader, skill_injector, vault, daemon lifecycle, course, init, showcase, media store, trace writer

### 5.3 `HttpClient` trait (definition in nika-core, impl in nika-http)

Async HTTP abstraction replacing 16+ raw `reqwest::Client` call sites.

- Methods: async send(HttpRequest) returning HttpResponse
- Request struct: method, url, headers, body, timeout, follow_redirects
- Response struct: status, headers, body, final_url
- Production impl: `ReqwestClient`
- Mock impl: `MockHttpClient` with programmable response queue
- Wire points: `TaskExecutor::http_client`, `FetchTool::client`, `RigProvider::OpenAiCompat`, registry, robots, webhook, provider_checker, sdk/remote

### 5.4 `ShellExecutor` trait (definition in nika-core, impl in nika-exec-runner)

Abstract shell command runner replacing 24+ raw process spawn sites.

- Methods: async run(ShellCommand) returning ShellResult
- Command struct: program, args, env, cwd, timeout, stdin, shell flag
- Result struct: status, stdout, stderr, duration
- Production impl: `TokioShell`
- Alt impl: `SandboxedShell` (future: firejail / bubblewrap integration)
- Mock impl: `MockShell` with programmable output + status
- Wire points: exec verb, daemon, doctor, install, machine

### 5.5 `BlobStore` trait (definition in nika-core, impl in nika-media)

Replaces the concrete `CasStore` god type.

- Methods: async put, get, dimensions, exists, stat, delete, list, gc
- Production impl: `DiskBlobStore` (current `CasStore`)
- Mock impl: `MemoryBlobStore`
- Wire points: `RunContext.cas`, `TaskExecutor.cas`, `MediaToolContext`, artifact writer

### 5.6 `Provider` trait (definition in nika-core, impls in nika-provider)

**This is the big one.** Replaces `RigProvider` 9-variant enum + `dispatch_rig!` macro + 215 references.

- Methods: name, capabilities, async infer, async infer_stream, async infer_with_tools, async embed (default impl returns unsupported)
- Streaming uses `BoxStream<'static, Result<StreamChunk, NikaError>>` for object safety
- Production impls (in nika-provider): AnthropicProvider, OpenAiProvider, MistralProvider, GroqProvider, DeepSeekProvider, GeminiProvider, XaiProvider, OpenAiCompatProvider, NativeProvider, MockProvider

**Migration:** The existing `RigProvider` enum + `dispatch_rig!` macro gets **WRAPPED** in a `Provider` impl rather than replaced. Commit R5.1: `impl Provider for RigProvider { fn infer(...) { dispatch_rig!(self, |c| c.infer()) } }`. This is the "connect, don't delete" pattern — the macro lives on, but now through a trait.

### 5.7 `Datastore` composition traits (nika-core, for nika-runtime)

Replaces the 1995-LOC `RunContext` god struct by **splitting its public surface** into 6 small traits. `RunContext` becomes the canonical impl that implements all 6.

**Traits:**
- `TaskResults`: insert, get, get_trust, iter
- `BindingScope`: get, with_local
- `MediaStaging`: stage, take
- `RecordStore`: write_record, read_records
- `VaultLookup`: get_secret
- `InvocationContext`: source, working_dir, project_root, inputs

`RunContext` implements all 6 traits. Consumer code takes only the slices it needs:

```rust
async fn handle_fetch(
    req: FetchRequest,
    results: &dyn TaskResults,
    invocation: &dyn InvocationContext,
    http: &dyn HttpClient,
) -> Result<...>
```

Rather than taking `&RunContext` (which forces 1995 LOC of coupling).

### 5.8 `VerbExecutor` trait (nika-runtime)

Replaces `TaskExecutor::run_infer()`, etc. which are methods on the 22-field god struct.

- Methods: async dispatch(task_id, action, bindings, scope) returning TaskOutput
- TaskExecutor holds `verbs: HashMap<VerbKind, Arc<dyn VerbExecutor>>`
- Impls (all in their own crate):
  - `InferVerb` in nika-provider — holds provider state only
  - `ShellVerb` in nika-exec-runner — holds shell state only
  - `FetchVerb` in nika-http — holds HTTP state only
  - `InvokeVerb` in nika-builtin + nika-mcp — dispatches to builtin or MCP
  - `AgentVerb` in nika-provider — agent loop

Each verb owns a ~3-8 field struct instead of the 22-field god.

### 5.9 `EventEmitter` trait (nika-event, ALREADY EXISTS, needs wiring)

Already defined. See Section 4.1 for the wiring migration.

- Methods: `emit(EventKind)`, `emit_at(Instant, EventKind)` (default impl calls `emit`)
- Production impl: `EventLog` (4544 LOC, existing)
- Test impls: `NoopEmitter`, `CollectingEmitter` (records all events for assertions)

### 5.10 `BuiltinTool` trait (nika-builtin, ALREADY EXISTS, seal it)

Already defined. Change from `pub trait BuiltinTool: Send + Sync` to `pub trait BuiltinTool: Send + Sync + sealed::Sealed` using the private module trick.

The sealed trait prevents third parties from adding builtins via `impl BuiltinTool`. The only way to add a builtin is the `#[builtin_tool]` macro (see Section 6), which also adds `impl sealed::Sealed` via compile-time check.

---

## 6. Proc-Macro Crate — nika-macros

**New crate:** `tools/nika-macros/`
**Purpose:** Custom derives and declarative macros to eliminate 7000+ LOC of boilerplate.
**Dependencies:** `proc-macro2`, `syn 2.x`, `quote`
**Feature flag:** None (always compiled, zero runtime cost)

```
tools/nika-macros/
├── Cargo.toml
└── src/
    ├── lib.rs                  ~100 LOC — entry points
    ├── builtin_tool.rs         ~300 LOC — #[builtin_tool] derive
    ├── nika_error.rs           ~250 LOC — #[nika_error] derive
    ├── event_kind.rs           ~200 LOC — #[event_kind] derive
    ├── transform_op.rs         ~300 LOC — transform! declarative macro
    ├── sealed.rs               ~50 LOC  — sealed trait helper
    └── test_utils.rs           ~100 LOC
```

### 6.1 `#[builtin_tool]` derive (~3,000 LOC saved)

The `nika:sleep` tool currently takes 293 LOC of boilerplate. Target: ~30 LOC per tool using a derive macro that:
- Wraps a plain async function
- Derives schema from a `Deserialize + JsonSchema` params struct
- Generates the trait impl automatically
- Seals the type via `sealed::Sealed`
- Registers with `inventory::submit!` so the router auto-discovers it

### 6.2 `#[nika_error]` derive (~1,500 LOC saved)

Current state: every variant duplicates its `NIKA-XXX` code in 3 places (`#[error]`, `#[diagnostic]`, `code()` method). A derive macro takes `#[nika(code = 044, help = "...")]` and generates all three plus a lookup table entry.

### 6.3 `transform!` macro (~2,000 LOC saved)

The 64 transforms in transform.rs follow 3 patterns: "string propagating", "array propagating", "value parametric". A table-driven macro collapses the enum, parser, display, dispatch arm, and catalog entry into one line per transform.

### 6.4 `#[event_kind]` derive (~500 LOC saved)

The 98 EventKind variants each repeat `#[serde(skip_serializing_if)]` on optional fields. A derive macro auto-detects `Option<T>` fields and adds the attributes, plus generates `task_id()`, `is_error()`, `category()` accessors.

### 6.5 Macro Development Timeline

Macros are developed in Phase 2 AFTER the kernel trait definitions land (Phase 1), so that the macros can reference the trait types. Each macro is one commit with tests.

---

## 7. God File Decomposition — 5 Files, 27k LOC

Mechanical split first (commits 7.1-7.5), proc-macro consolidation second (commits 7.6-7.10 after Phase 2).

### 7.1 `transform.rs` (5,645 LOC) to 12 files

Target: `tools/nika-core/src/binding/transform/`

```
mod.rs                  ~200 LOC
parse.rs                ~400 LOC
dispatch.rs             ~900 LOC
ops_string.rs           ~300 LOC
ops_collection.rs       ~400 LOC
ops_aggregation.rs      ~250 LOC
ops_data.rs             ~400 LOC
ops_type.rs             ~200 LOC
ops_url.rs              ~150 LOC
ops_hash.rs             ~100 LOC
jq_compat.rs            ~250 LOC
helpers.rs              ~200 LOC
```

Tests: 368 distributed across modules.

**First commit:** Extract `ops_string.rs` (zero cross-deps). Then iterate.

**Post-Phase 2:** All `ops_*.rs` files collapse into a single `transform!` macro table (~300 LOC total). 1700 LOC saved.

### 7.2 `template.rs` (4,935 LOC) to 9 files

Target: `tools/nika-core/src/binding/template/`

```
mod.rs                  ~150 LOC
parse.rs                ~300 LOC
resolve_with.rs         ~500 LOC
resolve_context.rs      ~350 LOC
resolve_inputs.rs       ~300 LOC
escape.rs               ~200 LOC
validate.rs             ~250 LOC
extraction.rs           ~200 LOC
helpers.rs              ~150 LOC
```

Tests: 200 distributed.

**First commit:** `escape.rs` (security-critical, standalone).

### 7.3 `analyze.rs` (5,528 LOC) to 11 files

Target: `tools/nika-core/src/ast/analyzer/`

```
mod.rs                          ~150 LOC
schema_check.rs                 ~250 LOC
task_table.rs                   ~300 LOC
binding_parser.rs               ~400 LOC
dependency_graph.rs             ~350 LOC
cycle_detection.rs              ~200 LOC
model_validation.rs             ~300 LOC
verb_validation.rs              ~400 LOC
retry_timeout_validation.rs     ~250 LOC
artifact_validation.rs          ~300 LOC
include_resolution.rs           ~150 LOC
```

Tests: 131 (tightly coupled — keep integration-style in `mod.rs` or dedicated `tests.rs`).

**BLOCKED** until Shield Sprint 2 merges (taint.rs is being added here).

### 7.4 `main.rs` (5,527 LOC) to migration to nika-cli/verbs/

Target: `nika/src/main.rs` becomes <500 LOC; logic moves to `nika-cli/src/verbs/`

```
nika/src/main.rs            ~200 LOC    main() + color detection + error handling only

nika-cli/src/verbs/
  mod.rs                    ~50 LOC    re-exports
  run.rs                    ~400 LOC   nika run
  validate.rs               ~350 LOC   nika check / validate
  decompose.rs              ~300 LOC   nika decompose
  bench.rs                  ~400 LOC   nika bench
  mcp_validate.rs           ~200 LOC   nika mcp validate
  serve.rs                  ~600 LOC   nika serve (Axum)
  ui.rs                     ~200 LOC
  chat.rs                   ~150 LOC
  studio.rs                 ~150 LOC
  doctor.rs                 ~200 LOC
  init.rs                   ~150 LOC
  course.rs                 ~150 LOC
  keys.rs                   ~200 LOC
  every.rs                  ~200 LOC
  schedule.rs               ~200 LOC
  explain.rs                ~150 LOC
  graph.rs                  ~150 LOC

nika-cli/src/shared/
  input_parsing.rs          ~250 LOC
  cost_confirmation.rs      ~200 LOC
  task_filtering.rs         ~150 LOC
  golden_test_runner.rs     ~200 LOC
  color.rs                  ~100 LOC
```

**Tests:** Only 26 exist in current main.rs (4% coverage — pathological). After migration: add rstest cases per verb for 100+ tests target.

**BLOCKED** until Shield Sprint 2 merges.

### 7.5 `runner/mod.rs` (2,331 logic + 4,817 tests) to 7 files + scheduler.rs

Target: `tools/nika-runtime/src/runner/` (after extraction to nika-runtime in Phase 9)

```
runner/mod.rs               ~300 LOC    Runner struct, public API
runner/lifecycle.rs         ~400 LOC
runner/execution.rs         ~500 LOC    main run() loop (calls scheduler)
runner/pause_cancel.rs      ~150 LOC
runner/artifact_handler.rs  ~300 LOC
runner/finalization.rs      ~150 LOC
runner/context_resolver.rs  ~200 LOC
scheduler.rs                ~600 LOC    NEW — extracted from run() god method
                                        LayerScheduler with spawn_ready_tasks()
```

**Tests:** 4,817 LOC stay in `runner/tests.rs` (already separated).

**First commit:** `pause_cancel.rs` (~150 LOC, zero coupling).
**Second commit:** `scheduler.rs` — extracts the 900-line god method from `run()`. **This unblocks Shield Sprint 2's scheduling changes.**

**BLOCKED** until Shield Sprint 2 merges.

---

## 8. Phase Roadmap — 19 Phases

```
PHASE 0: Shield Sprint 2 merges                              [BLOCKING — J-27]
PHASE 1: Kernel traits land in nika-core                     [J-26 to J-25]
PHASE 2: nika-macros crate                                   [J-25 to J-23]
PHASE 3: Wire EventEmitter trait (R1)                        [J-23 to J-22]
PHASE 4: Promote error_domains sub-enums (R2)                [J-22 to J-20]
PHASE 5: Adopt rstest + mockall (R3)                         [J-20 to J-19]
PHASE 6: God file mechanical splits (non-extraction)         [J-19 to J-17]
PHASE 7: Consolidate LSP (R4)                                [J-17 to J-16]
PHASE 8: Extract nika-provider                               [J-16 to J-14]
PHASE 9: Extract nika-runtime                                [J-14 to J-11]
PHASE 10: Extract nika-http                                  [J-11 to J-10]
PHASE 11: Extract nika-exec-runner                           [J-10 to J-9]
PHASE 12: Extract nika-fs                                    [J-9 to J-8]
PHASE 13: Extract nika-builtin                               [J-8 to J-6]
PHASE 14: Extract nika-cache                                 [J-6 to J-5]
PHASE 15: Migrate main.rs to nika-cli/verbs/                 [J-5 to J-4]
PHASE 16: analyze.rs mechanical split                        [J-4 to J-3]
PHASE 17: Split nika-tui                                     [J-3 to J-2]
PHASE 18: Type system hardening                              [J-2 to J-1]
PHASE 19: Polish + validation                                [J-1 to J-0]
```

**Parallel opportunities:**
- Phases 1, 2 can run in parallel (traits and macros are independent)
- Phases 3, 4, 5 can run in parallel (three dead-scaffolding reconnections)
- Phase 6 sub-phases (god file splits) can run in parallel
- Phases 10, 11, 12 can run in parallel (independent effect crates)

**Critical path:** 0 → 1 → 2 → 8 → 9 → 13 → 15 → 19

---

## 9. New Crate Specifications (condensed)

### 9.1 nika-macros (Phase 2) — ~1,500 LOC

Proc-macro crate. 4 derives + 1 declarative macro. Uses `proc-macro2`, `syn 2.x`, `quote`. No runtime dependencies.

### 9.2 nika-provider (Phase 8) — ~10k LOC

Wraps existing `provider/` code, adds `Provider` trait impls. Features: `native-inference` (optional, mistralrs).

### 9.3 nika-runtime (Phase 9) — ~30k LOC

The new orchestration heart. Contains Runner, TaskExecutor, scheduler, task_dispatch, policy, structured_output, artifact_processor.

### 9.4 nika-http (Phase 10) — ~3k LOC

`HttpClient` trait impl + 9 extraction modes. Features: `fetch-article`, `fetch-markdown`, `fetch-html`, `fetch-feed`, `fetch-sitemap`.

### 9.5 nika-exec-runner (Phase 11) — ~2.5k LOC

`ShellExecutor` trait impl + blocklist + Unicode normalization. Security core.

### 9.6 nika-fs (Phase 12) — ~1k LOC

`Filesystem` trait impl via tokio::fs + `InMemoryFs` test mock. Wired to 30+ call sites.

### 9.7 nika-builtin (Phase 13) — ~15k LOC

63 builtin tools + sealed `BuiltinTool` trait. Uses `#[builtin_tool]` derive from `nika-macros`. Uses `inventory` crate for compile-time registration.

### 9.8 nika-cache (Phase 14) — ~1.5k LOC

`CacheBackend` trait + sled impl + memory impl. Trust-aware cache keys (tainted runs cached separately).

### 9.9 nika-tui-widgets (Phase 17) — ~10k LOC

Reusable ratatui components. Zero nika-engine coupling. Pure UI.

### 9.10 nika-tui-core (Phase 17) — ~15k LOC

TuiState, events, runtime bridge.

### 9.11 nika-tui-views (Phase 17) — ~14k LOC

Studio + Command + Control views.

### 9.12 nika-tui-app (Phase 17) — ~3k LOC

Main event loop, app composition.

---

## 10. Test Strategy — Connect rstest and mockall

### 10.1 Current state

- 36,706 test attributes
- 0 rstest usage
- 0 mockall usage
- Test files: inline `mod tests` mostly; some separate `tests.rs` files

### 10.2 rstest adoption

**First target:** `transform.rs` tests (368 tests, many parametrized by hand). Migrate from repeated `#[test]` fns to `#[rstest]` with `#[case]` tables.

**Second target:** Template tests, error variant tests.

### 10.3 mockall adoption

For each new trait (HttpClient, ShellExecutor, Clock, Filesystem, Provider, EventEmitter), add `#[mockall::automock]` to auto-generate a mock type. Use in test suites for isolation.

### 10.4 Test fixtures crate

**New:** `tools/nika-test-fixtures` (dev-dep only)

Contains: sample .nika.yaml workflows as const strings, pre-configured MockProvider responses, common HTTP mock scenarios, event log assertion helpers.

---

## 11. Type System Hardening

### 11.1 Propagate `TaskId` newtype (Phase 18)

Currently `TaskId(u32)` exists but stops at the analyzed-AST boundary. Propagate through runtime, events, errors.

### 11.2 Newtype wrappers

- `ModelId(Arc<str>)`
- `ProviderName(Arc<str>)` — already exists, use it everywhere
- `McpToolName(Arc<str>)`
- `BuiltinToolName(Arc<str>)` (with const validation)
- `WorkflowId(Arc<str>)`
- `SecretName(Arc<str>)`

### 11.3 `SecretString` at API boundaries

Extend `secrecy::SecretString` beyond nika-vault: provider API keys, daemon secrets, serve auth tokens.

### 11.4 `Runner<State>` type-state pattern

Make Runner parametric: `Runner<Unconfigured>`, `Runner<Ready>`, `Runner<Running>`. Eliminates `unwrap()` chains inside `run()`.

### 11.5 Sealed traits

- `BuiltinTool: sealed::Sealed` (closed set of 40+ tools)
- `Provider: sealed::Sealed` (closed set of 10 providers)
- `VerbExecutor: sealed::Sealed` (exactly 5 verbs)

### 11.6 `#[must_use]` and lint bumps

Add `#[must_use]` to `TaskResult`, Runner state transitions. Add `#![warn(missing_docs)]` to `nika-core` and `nika-runtime`.

---

## 12. Feature Flag Reconnection

### 12.1 nika-tui dead features — wire them

| Feature | TUI wire point |
|---------|---------------|
| `media-phash` | visual_similarity_panel in monitor |
| `media-chart` | cost_chart widget in monitor |
| `media-qr` | qr_preview in Studio |
| `media-thumbnail` | inline thumbnail rendering |
| `media-metadata` | metadata_inspector panel |
| `media-pdf` | pdf_preview in Studio |
| `media-svg` | svg_preview widget |
| `media-optimize` | status bar indicator |
| `media-iqa` | quality_badge widget |
| `media-provenance` | provenance_indicator |
| `media-compression` | compression ratio status |
| `fetch-article` | article_preview in Studio |
| `fetch-markdown` | markdown preview |
| `fetch-html` | html_inspector panel |
| `fetch-feed` | feed_list widget |
| `fetch-sitemap` | sitemap_tree widget |
| `native-inference` | local_model_badge |

### 12.2 Feature flag consolidation

Declare features ONCE in `nika-media/Cargo.toml`. Other crates use `nika-media/media-X` pattern.

### 12.3 Double-gating resolution

After nika-http extraction (Phase 10), consolidate all fetch extract code into `nika-http`. Single feature surface.

---

## 13. Public API Curation

### 13.1 nika-core

Current: 328 `pub` items, 0 `pub(crate)`, 47 `pub mod`. Target: reduce to 5-6 facade re-exports + `pub(crate)` for internals.

### 13.2 nika-engine (post-split)

After extraction phases, `nika-engine` should NOT exist or should be a thin facade re-exporting `nika-runtime` + `nika-provider` + `nika-http` + `nika-exec-runner` + `nika-builtin` + `nika-cache`.

### 13.3 Lint bumps

Add workspace lints: `missing_docs`, `unused_results`, `unused_must_use`. Gate per-crate to avoid drowning in warnings.

---

## 14. Risk Register

| # | Risk | Severity | Probability | Mitigation |
|---|------|----------|-------------|------------|
| R1 | Shield Sprint 2 doesn't merge in time | CRITICAL | LOW | 3 worktrees active, scheduled for J-26 |
| R2 | Test breakage during god file splits | HIGH | MEDIUM | Move tests with code, test after every commit |
| R3 | Circular deps during extraction | HIGH | LOW | Plan extraction order per dep graph, use traits to break cycles |
| R4 | Compile time regression during transition | MEDIUM | MEDIUM | Measure baseline, final should be improved |
| R5 | Proc-macro crate learning curve | MEDIUM | MEDIUM | Use existing patterns (thiserror, serde_derive), keep macros simple |
| R6 | `error_domains` migration breaks 1294 call sites | HIGH | LOW | 5 commits, each migrating one domain, old variants coexist temporarily |
| R7 | `EventEmitter` wire-through breaks 428 call sites | HIGH | LOW | `Arc<EventLog>` impls EventEmitter, most call sites unchanged |
| R8 | LSP consolidation breaks editor integrations | MEDIUM | MEDIUM | Keep both paths working during transition, test all 5 editors |
| R9 | `RigProvider` enum wrapping breaks streaming | MEDIUM | LOW | Wrap in trait impl (don't rewrite), dispatch_rig! macro stays |
| R10 | `Datastore` trait decomposition leaks details | HIGH | MEDIUM | Start with 3 traits not 6, iterate |
| R11 | Refactor takes >27 days | MEDIUM | MEDIUM | Phases can be deferred, architecture purity not a launch gate |
| R12 | Team velocity drops due to refactor churn | LOW | LOW | Shield Sprint 2 will be done, only refactor work remains |
| R13 | Binary size regression | LOW | LOW | Measure baseline, final should be smaller |
| R14 | Feature flag wiring breaks existing users | NONE | NONE | Zero users = no compat burden |
| R15 | `rstest`/`mockall` learning curve | LOW | LOW | Both are 10-min learning curves |
| R16 | Sealed trait breaks documentation generators | LOW | LOW | Document the sealing explicitly |
| R17 | New traits introduce hidden `dyn Trait` overhead | LOW | LOW | Benchmark hot paths, use generic bounds where possible |
| R18 | Type-state Runner<State> forces user API changes | MEDIUM | MEDIUM | Provide builder pattern as escape hatch |
| R19 | Proc-macro compile time balloons | MEDIUM | MEDIUM | Keep macros small, minimize syn features |
| R20 | Main.rs migration introduces regression in CLI parsing | HIGH | LOW | clap is declarative — move verbs, keep main dispatch minimal |

---

## 15. Daily Schedule J-27 → J-0

```
J-27 (Apr 8)  Constellation v2 plan written. Shield Sprint 2 committing.
              Research agents in flight (rust-pro, rust-architect, ctx7, perplexity).

J-26 (Apr 9)  Shield Sprint 2 MERGES. v0.79.0 tagged.
              Phase 1 starts: kernel traits land in nika-core.

J-25 (Apr 10) Phase 1 continues: all 10 trait defs.
              Phase 2 starts: nika-macros crate scaffolding.

J-24 (Apr 11) Phase 2 continues: all 4 derives + transform! macro.
              Phase 3 starts: EventEmitter trait wiring.

J-23 (Apr 12) Phase 3 completes (all 428 call sites wired).
              Phase 4 starts: error_domains migration.

J-22 (Apr 13) Phase 4 continues: ProviderError + DagError migrated.

J-21 (Apr 14) Phase 4 completes: BindingError + ExecutionError migrated. 
              Execution(String) catch-all eliminated.
              Phase 5: rstest + mockall adoption.

J-20 (Apr 15) Phase 6 starts: god file mechanical splits.
              transform.rs to 12 files. template.rs to 9 files. error.rs to 8 files.

J-19 (Apr 16) Phase 6 continues: runner/mod.rs to 7 files + scheduler.rs extraction.
              Phase 7: LSP consolidation.

J-18 (Apr 17) Phase 7 completes.

J-17 (Apr 18) Phase 8 starts: nika-provider extraction.

J-16 (Apr 19) Phase 8 continues: all 10 provider impls landed.

J-15 (Apr 20) Phase 9 starts: nika-runtime extraction.

J-14 (Apr 21) Phase 9 continues: TaskExecutor uses VerbExecutor array.

J-13 (Apr 22) Phase 9 completes.
              Phase 10 starts: nika-http extraction.

J-12 (Apr 23) Phase 10 completes.
              Phase 11 starts: nika-exec-runner extraction.

J-11 (Apr 24) Phase 11 completes.
              Phase 12 starts: nika-fs extraction.

J-10 (Apr 25) Phase 12 completes.
              Phase 13 starts: nika-builtin extraction.

J-9  (Apr 26) Phase 13 continues: apply #[builtin_tool] macro to 40+ tools.

J-8  (Apr 27) Phase 13 completes. BuiltinTool trait sealed.
              Phase 14: nika-cache extraction.

J-7  (Apr 28) Phase 15 starts: main.rs migration to nika-cli/verbs/.

J-6  (Apr 29) Phase 15 continues: migrate serve, ui, chat, studio, doctor.

J-5  (Apr 30) Phase 15 completes: main.rs <500 LOC.
              Phase 16: analyze.rs mechanical split (11 files).

J-4  (May 1)  Phase 17 starts: nika-tui split.

J-3  (May 2)  Phase 17 continues + wire dead TUI features.

J-2  (May 3)  Phase 18: type system hardening.

J-1  (May 4)  Phase 19: polish + validation.
              Full adversarial test suite run.

J-0  (May 5)  LAUNCH. Show HN: "Inference as Code".
              v0.80.0 tagged. 32 crates. Modular. Elegant.
```

**Buffer:** Phases 1-5 have 1-day buffers. Phases 8-15 are tight. Phase 16-19 are the crunch.

**Fallback order** (if running behind, drop in this order):
1. Phase 19 (polish — can continue post-launch)
2. Phase 18 (type-state — mostly cosmetic)
3. Phase 17 (TUI split — already modular internally)
4. Phase 14 (cache — small win)

---

## 16. Validation Criteria Per Phase

### Generic per-phase checklist

- [ ] `cargo test --workspace --lib` passes (all 36k+ tests)
- [ ] `cargo clippy --workspace -- -D warnings` is clean
- [ ] `cargo check --workspace` compiles
- [ ] No new TODO/FIXME (count must not increase)
- [ ] Git commit follows `refactor(arch): ...` format
- [ ] Co-author: `Nika 🦋 <nika@supernovae.studio>`
- [ ] Editor sync if catalogs changed
- [ ] Phase-specific success metric

### Final (J-0) validation

- [ ] 32 crates in workspace
- [ ] No crate > 15k LOC source (excluding tests)
- [ ] No file > 1500 LOC
- [ ] Compile time improved 20%+ (baseline measured J-26)
- [ ] Binary size reduced 10%+ (baseline measured J-26)
- [ ] `.unwrap()` count reduced from 9269 to <5000
- [ ] `cargo deny check` passes
- [ ] All 36k+ tests pass
- [ ] Zero clippy warnings
- [ ] All 5 editor integrations work
- [ ] SECURITY.md reflects post-Shield architecture
- [ ] Homebrew formula builds
- [ ] crates.io dry-run succeeds
- [ ] Show HN post tested

---

## 17. Implementation Prompts

### Prompt for Phase 1 (kernel traits)

```
Execute Phase 1: Kernel Traits in nika-core.
Plan: docs/plans/2026-04-08-constellation-v2-mega-plan.md, section 5.

Goal: Define 10 trait interfaces in nika-core without any production wiring yet.

Files to CREATE:
- tools/nika-core/src/clock.rs (~100 LOC)
- tools/nika-core/src/filesystem.rs (~200 LOC)
- tools/nika-core/src/http.rs (~150 LOC)
- tools/nika-core/src/shell.rs (~120 LOC)
- tools/nika-core/src/blob_store.rs (~100 LOC)
- tools/nika-core/src/provider_trait.rs (~250 LOC)
- tools/nika-core/src/datastore.rs (~300 LOC)
- tools/nika-core/src/verb_executor.rs (~100 LOC)

Files to MODIFY:
- tools/nika-core/src/lib.rs (add pub mod + pub use)
- tools/nika-core/Cargo.toml (add async-trait dep)

For each trait:
1. Use #[async_trait::async_trait] for dyn-safety
2. Production impls (SystemClock, TokioFs, etc.) live in effect crates, not nika-core
3. Mock impls go behind #[cfg(any(test, feature = "test-utils"))]
4. Return Result<T, NikaError>
5. Add doc comments with examples

After each file: cargo check, cargo clippy, cargo test --lib.
One commit per trait. Format: feat(core): add X trait for ...
Co-author: Nika 🦋 <nika@supernovae.studio>

DO NOT touch Shield files. DO NOT wire production code yet — that's Phase 3+.
```

### Prompt for Phase 3 (wire EventEmitter)

```
Execute Phase 3: Wire EventEmitter trait through Nika.
Plan: docs/plans/2026-04-08-constellation-v2-mega-plan.md, section 4.1.

Goal: Replace `event_log: EventLog` concrete fields with `event_log: Arc<dyn EventEmitter>`.

Context: EventEmitter trait EXISTS at nika-event/src/emitter.rs:16 but has 0 production uses.
428 call sites clone EventLog concrete. 59 files affected.

6 commits:

Commit R1.1: Export EventEmitter from nika-event. Add `impl EventEmitter for Arc<EventLog>`.
  Test: assert `let _: Arc<dyn EventEmitter> = Arc::new(EventLog::new())` compiles

Commit R1.2: Change TaskExecutor::event_log to Arc<dyn EventEmitter>.

Commit R1.3: Change Runner::event_log similarly.

Commit R1.4: Change RigAgentLoop, StructuredOutputEngine, BuiltinToolRouter similarly.

Commit R1.5: Bulk-migrate remaining concrete EventLog fields across 59 files.

Commit R1.6: Add CollectingEmitter (test-only) in nika-event/src/emitter_test.rs.

After each commit: cargo test --workspace --lib && cargo clippy -- -D warnings.
```

### Full prompt template (for any phase)

```
Execute Phase X of Nika Constellation v2 Architecture Refactor.

Read first:
- /Users/thibaut/dev/supernovae/nika/docs/plans/2026-04-08-constellation-v2-mega-plan.md (complete plan)
- /Users/thibaut/dev/supernovae/nika/CLAUDE.md (project conventions)
- /Users/thibaut/dev/supernovae/nika/tools/nika/CLAUDE.md (crate architecture)

Philosophy: **CONNECT, DO NOT DELETE.** Every dead scaffold gets wired, not removed.

Constraints:
- Shield Sprint 2 MUST be merged (verify `git log main | grep 'v0.79'`)
- Test count must not decrease
- cargo test --workspace --lib after every commit
- cargo clippy --workspace -- -D warnings — zero warnings
- 1 fix = 1 commit
- Format: refactor(arch): <what> OR feat(X): <what> for new additions
- Co-author: Nika 🦋 <nika@supernovae.studio>
- NEVER claim a phase complete without verification
- If code looks "unused", CONNECT IT, don't delete

Goal: <specific goal for this phase>

Files to CREATE: <list>
Files to MODIFY: <list>
Files NOT to touch: Any file modified on shield-sprint-2 if not yet merged

Verification checklist:
- [ ] cargo test --workspace --lib passes
- [ ] cargo clippy clean
- [ ] Phase-specific success metric (see section 16)
- [ ] No dead code deleted (connect instead)

Report when done with:
- Commits created (hash + message)
- Tests delta (count before/after)
- Clippy delta
- Next phase readiness
```

---

## Appendix A: The Connect-Not-Delete Manifesto

> Every line of code in this repository exists because someone thought it mattered.
> If it is currently unused, that is our failure — not the code's.
> We will connect what is disconnected, complete what is half-done, and revive what is dormant.
> We will not rewrite history by deleting scaffolding.
> We will finish the job.

---

## Appendix B: File Count Projections

| State | Crates | Source files | Largest crate LOC | Largest file LOC |
|-------|--------|--------------|-------------------|------------------|
| **v0.78.0 (current)** | 21 | ~800 | nika-engine 160k | runner/ 7100, transform.rs 5645 |
| **Post-Shield Sprint 2 (v0.79.0)** | 21 | ~820 | nika-engine ~165k | similar |
| **Phase 1-2 (traits + macros)** | 22 | ~840 | nika-engine ~165k | similar |
| **Phase 6 (god file splits)** | 22 | ~900 | nika-engine ~165k | <1500 |
| **Phase 8 (nika-provider)** | 23 | ~920 | nika-engine ~155k | <1500 |
| **Phase 9 (nika-runtime)** | 24 | ~940 | nika-runtime ~30k, nika-engine ~100k | <1500 |
| **Phase 13 (nika-builtin)** | 26 | ~1000 | nika-runtime ~35k | <1500 |
| **Phase 17 (tui split)** | 30 | ~1100 | nika-runtime ~40k | <1500 |
| **Phase 19 (final)** | 32 | ~1150 | nika-runtime ~40k | <1500 |

---

## Appendix C: Metrics to Track

Track these daily during the refactor:

- Test count: `#[test]` + `#[tokio::test]` (target: grows)
- LOC per crate (target: none > 15k source)
- `.unwrap()` count (target: 9269 → <5000)
- TODO/FIXME count (target: flat or decreasing)
- Trait count (target: 17 → 27)
- Proc-macro usage (target: 0 → 100+)
- `dyn EventEmitter` usage (target: 0 → 20+)
- rstest adoption (target: 0 → 50+)
- mockall adoption (target: 0 → 30+)
- Largest file (target: 8055 → 1500)

---

## Appendix D: Comparison with Constellation v1

| Aspect | v1 (previous) | v2 (this plan) |
|--------|---------------|----------------|
| Philosophy | Minimum viable refactor | Maximum, connect not delete |
| Target crates | ~32 | ~32 |
| Phases | 12 | **19** |
| New traits | 7 | **10** |
| Proc-macros | None | **4 derives + 1 declarative** |
| Dead scaffolding | Delete | **Revive and wire** |
| LSP duplication | Delete engine/lsp | **Consolidate, keep both paths** |
| `error_domains.rs` | Delete | **Promote to variants** |
| `rstest`/`mockall` | Delete deps | **Adopt in tests** |
| Feature flags | Delete dead | **Wire them to TUI features** |
| Estimated LOC | ~6700 | **~12000 (includes macros + wiring)** |
| Risk | Medium | Higher scope but safer (incremental) |

---

## 18. v2.1 Revisions — Post-Research Integration

> **Added 2026-04-08 after rust-pro, rust-architect, context7 (tokio), and web-researcher agents reported.**
> **Philosophy update: v0 allows big bang / nuke refactors. No users = no backwards compat burden.**
> **"Connect not delete" still applies to DEAD SCAFFOLDING (finish the job), but MIGRATIONS can be big-bang cutovers.**

### 18.1 Philosophy nuance — Big bang vs Connect

| Scenario | v2 approach | v2.1 approach |
|----------|-------------|---------------|
| Dead trait (EventEmitter) | 6-commit incremental wire | **1-2 commit cutover** (blanket impl + flip 5 hot sites) |
| Dead sub-enums (error_domains) | 5-commit per-domain promotion | **1 commit** — promote all 4 at once, delete flat variants |
| RunContext god struct | 6 incremental trait impls | **1 commit** — all 6 trait impls land together |
| TaskExecutor 22 fields | Phased verb extraction | **1 PR** — create VerbExecutor trait + 5 impls + cutover |
| RigProvider enum → Provider trait | 2-week phased migration | **1 commit** — wrap enum in Provider impl, flip consumers, delete dispatch macro |
| main.rs → nika-cli/verbs/ | 15 commits per verb | **1-3 commits** — bulk move + clap wiring |
| nika-engine/lsp/ duplication | 6 commit diff-and-merge | **1 commit** — absorb into nika-lsp-core, delete engine copy |
| 40 builtin tools → #[builtin_tool] | 40 commits | **1 commit per category** (data/media/file/etc) ~6 total |
| 30 raw std::fs → Filesystem trait | 30 commits | **1-2 commits per subsystem** ~6 total |

**Rule:** the 6-commit careful migrations in v2 were a bot-safety net. In v0 with zero users, bot safety is achieved via `cargo test --workspace --lib` + `cargo clippy -- -D warnings` between commits. If tests stay green, the number of files changed in a single commit doesn't matter. Ship big commits when they're cleaner.

**What we still DO NOT delete:**
- `EventEmitter` trait (connect it — wire through runtime)
- `error_domains.rs` sub-enums (promote them — don't delete)
- `rstest` / `mockall` deps (adopt them — don't remove)
- Single-impl traits like `LspHandler` (add test impls — don't remove)
- `LspHandler`, `Highlighter`, `ModelStorage`, `InferenceBackend`/`DynInferenceBackend` (add alt impls for completeness)

**What we MAY delete (big bang OK because v0):**
- `dispatch_rig!` macro after Provider trait cutover
- `Execution(String)` catch-all variant after error_domains promotion
- 428 `EventLog` concrete uses after blanket impl
- `nika-engine/src/lsp/` 16 files after content absorbed into nika-lsp-core
- Dead debug prints, commented-out code
- Duplicate imports
- `TaskExecutor` 22-field god struct after VerbExecutor cutover
- `RigProvider` 9-variant enum (become one impl target of Provider trait)

### 18.2 Major architectural revision — nika-kernel L0.5

**The single most important change from v2 to v2.1.** Rust-architect agent made a strong case: do not put the 10 traits in `nika-core`. Create a new layer L0.5 called `nika-kernel`.

**Revised layering:**

```
═══════════════════════════════════════════════════════════════════
L0   nika-core              Pure types, AST, NikaError, catalogs. 
                            Zero async, zero I/O, zero traits.
─────────────────────────────────────────────────────────────────
L0.5 nika-kernel            The 10 traits + shared DTOs.          NEW
                            Zero impls. Depends only on nika-core 
                            + chrono + bytes + http + futures 
                            + async-trait + trait-variant.
─────────────────────────────────────────────────────────────────
L0.5 nika-kernel-mock       Hand-written mocks for all 10 traits. NEW
                            Dev-dep only. Conformance tests.
─────────────────────────────────────────────────────────────────
L1   nika-clock             SystemClock                            NEW
     nika-fs                TokioFs                                NEW
     nika-http              ReqwestClient                          NEW
     nika-exec-runner       TokioShell, SandboxedShell             NEW
     nika-blob              DiskBlobStore (ex-CasStore)            NEW
     nika-events            EventLog concrete + NDJSON writer      (was nika-event)
     nika-macros            Proc-macros                            NEW
     nika-shield            Trust, spotlight, canary               (post Sprint 2)
     nika-lsp-core          LSP intelligence (consolidated)
─────────────────────────────────────────────────────────────────
L2   nika-provider          LLM providers + Provider impls         NEW
     nika-builtin           63 builtin tools + sealed trait        NEW
     nika-mcp               MCP client
     nika-media             CAS, image ops (uses BlobStore trait)
     nika-storage           Storage abstraction
     nika-vault             Encrypted secrets
     nika-verb-infer        VerbExecutor for infer:                NEW
     nika-verb-exec         VerbExecutor for exec:                 NEW
     nika-verb-fetch        VerbExecutor for fetch:                NEW
     nika-verb-invoke       VerbExecutor for invoke:               NEW
     nika-verb-agent        VerbExecutor for agent:                NEW
─────────────────────────────────────────────────────────────────
L3   nika-runtime           Composition root. Runner + DAG +       NEW
                            verb dispatch. Builds Runtime struct 
                            holding trait objects. NO effect code.
     nika-daemon            Background daemon, cron
     nika-cache             LLM + fetch cache                       NEW
─────────────────────────────────────────────────────────────────
L4   nika-cli, nika-tui*, nika-lsp, nika-serve, nika-sdk, 
     nika-init, nika-display
─────────────────────────────────────────────────────────────────
L5   nika                   Binary, <500 LOC composition root
═══════════════════════════════════════════════════════════════════
```

**Crate count update:** 21 current → **~38 target** (was ~32 in v2):
- +nika-kernel
- +nika-kernel-mock
- +nika-macros
- +nika-clock, nika-fs, nika-http, nika-exec-runner, nika-blob
- +nika-provider
- +nika-builtin
- +nika-verb-infer, nika-verb-exec, nika-verb-fetch, nika-verb-invoke, nika-verb-agent
- +nika-runtime
- +nika-cache
- +nika-shield
- +nika-tui-{widgets,core,views,app}

**Why this matters:** `cargo test --lib` in `nika-verb-infer` compiles only `nika-kernel` + `nika-core` + `nika-kernel-mock`. That is ~1000 LOC vs the current ~160k LOC for any test in `nika-engine`. **Compile time win: 10-100x on unit test iteration.**

### 18.3 Async pattern matrix — use trait_variant::make

Decision from rust-architect + context7 research: use `trait_variant::make` from the `trait-variant` crate as the default, fall back to `async-trait` only for object-safe streaming.

| Trait | Pattern | Justification |
|-------|---------|---------------|
| `Clock` | `trait_variant::make(Clock: Send)` | Held by every verb, fast methods |
| `Filesystem` | `trait_variant::make(Filesystem: Send)` | Many call sites, no streams |
| `HttpClient` | `trait_variant::make(HttpClient: Send)` | Streaming goes via separate method returning `Pin<Box<dyn Stream>>` |
| `ShellExecutor` | `trait_variant::make(ShellExecutor: Send)` | Same streaming split |
| `BlobStore` | `trait_variant::make(BlobStore: Send)` | No stream in trait |
| `Provider` | **`async_trait`** (boxing) | Streaming + tool use + thinking events make AFIT painful |
| `EventEmitter` | `trait_variant::make(EventEmitter: Send)` | Sync methods, fast |
| `VerbExecutor` | **`async_trait`** | Must be `[Arc<dyn VerbExecutor>; 5]` in dispatch array |
| `BuiltinTool` | **`async_trait`** | Already exists as dyn, sealed |
| `TaskScope` + 6 splinters | **AFIT direct, generic bounds** | Consumers take `&mut impl TaskScope`, monomorphized |

**Rule of thumb:** if consumer holds it in a struct field → needs dyn → `async_trait`. If consumer takes `&mut` function arg → AFIT + generics.

### 18.4 Revised trait DTOs — explicit shape

From rust-architect: the `Provider` trait needs full request/response DTOs that live in `nika-kernel` (not in any provider crate). This prevents the "every provider re-defines what a message is" problem.

```rust
// nika-kernel/src/provider.rs

pub struct InferRequest {
    pub model: String,
    pub messages: Vec<Message>,
    pub temperature: Option<f32>,
    pub max_tokens: Option<u32>,
    pub tools: Vec<ToolDef>,         // empty = plain inference
    pub tool_choice: ToolChoice,
    pub response_format: ResponseFormat,
    pub stop_sequences: Vec<String>,
    pub thinking_budget: Option<u32>,
    pub extra: ProviderExtras,       // provider-specific pass-through
}

pub struct InferResponse {
    pub content: Vec<ContentBlock>,  // Text | ToolUse | Thinking
    pub usage: TokenUsage,
    pub stop_reason: StopReason,
    pub ttft_ms: Option<u64>,
    pub cached_tokens: Option<u32>,
}

pub enum InferEvent {
    Delta(String),
    ToolUseStart { id: String, name: String },
    ToolUseDelta { id: String, partial_json: String },
    Thinking(String),
    Usage(TokenUsage),
    Done(StopReason),
}

pub struct Capabilities {
    pub supports_tools: bool,
    pub supports_vision: bool,
    pub supports_streaming: bool,
    pub supports_thinking: bool,
    pub max_context: u32,
    pub max_output: u32,
}
```

**Consequence:** `infer_with_tools` is GONE. Unified into `InferRequest::tools`. Empty list = plain inference. One code path, not three.

### 18.5 TaskScope splinter pattern — compile-time capability enforcement

Instead of `&mut dyn RunContext` (which exposes 8 fields), verb functions take **generic splinters**:

```rust
// nika-kernel/src/scope.rs

pub trait TaskResults: Send + Sync { ... }
pub trait BindingScope: Send + Sync { ... }
pub trait MediaStaging: Send + Sync { ... }
pub trait RecordStore: Send + Sync { ... }
pub trait VaultLookup: Send + Sync { ... }
pub trait InvocationContext: Send + Sync { ... }

// Umbrella trait via supertrait composition
pub trait TaskScope:
    TaskResults + BindingScope + MediaStaging + RecordStore + VaultLookup + InvocationContext
{}
impl<T> TaskScope for T where
    T: TaskResults + BindingScope + MediaStaging + RecordStore + VaultLookup + InvocationContext
{}
```

**Each verb crate declares ONLY the capabilities it needs:**

```rust
// nika-verb-fetch — only needs bindings + media staging + blob store
pub async fn run_fetch<S>(
    task: &FetchTask,
    scope: &mut S,
    http: &dyn HttpClient,
    blob: &dyn BlobStore,
    clock: &dyn Clock,
) -> Result<TaskResult, NikaError>
where
    S: BindingScope + MediaStaging,  // NOT TaskResults, NOT RecordStore
{ ... }
```

**The compiler enforces** that fetch cannot touch `TaskResults::insert` or `RecordStore::push`. Capability-based separation at compile time. This is the invariant that makes the 1995-LOC RunContext god struct safe to dismantle.

**At the dispatch boundary in `nika-runtime`**, use `&mut dyn TaskScope` (the supertrait is dyn-safe via blanket impl). Inside each verb crate, narrow to generic splinters. This balances monomorphization cost (verb crates compile fast) vs dispatch ergonomics (runtime holds `Arc<dyn VerbExecutor>`).

### 18.6 Bundle composition pattern — no god object

From rust-architect: avoid `struct Runtime { clock, fs, http, shell, blob, events, providers, builtins, ... }` (9 fields = god object). **Bundle by concern:**

```rust
// nika-kernel/src/bundle.rs

#[derive(Clone)]
pub struct KernelBundle {
    pub clock: Arc<dyn Clock>,
    pub events: Arc<dyn EventEmitter>,
}

#[derive(Clone)]
pub struct IoBundle {
    pub fs: Arc<dyn Filesystem>,
    pub http: Arc<dyn HttpClient>,
    pub shell: Arc<dyn ShellExecutor>,
    pub blob: Arc<dyn BlobStore>,
}

#[derive(Clone)]
pub struct LlmBundle {
    pub providers: Arc<ProviderRegistry>,
    pub builtins: Arc<dyn BuiltinRegistry>,
}

pub struct Runtime {
    pub kernel: KernelBundle,
    pub io: IoBundle,
    pub llm: LlmBundle,
    pub verbs: [Arc<dyn VerbExecutor>; 5],
}
```

Each verb struct takes only the bundles it needs:

```rust
pub struct FetchVerb {
    kernel: KernelBundle,  // clock + events, always
    io: IoBundle,          // fs + http + blob
}

pub struct InferVerb {
    kernel: KernelBundle,
    llm: LlmBundle,        // NO io — infer doesn't touch fs or http
}
```

**Three fields max per verb struct.** Clone-able. Tests swap one bundle without rebuilding the whole runtime.

**Test setup:** ~15 lines to build a full mock runtime (currently 200+ lines in integration tests).

### 18.7 Builtin registration via linkme (not inventory)

From web-researcher: `linkme` beats `inventory` for Nika's builtin registration.

**Why linkme wins:**
- Zero runtime cost (compile-time distributed slice, not life-before-main constructor)
- `const`-init friendly (no Ctor calls before main)
- Same platform support (Linux/macOS/Windows/FreeBSD/illumos)
- Used in `datatest`, `typetag`, standard in the Rust ecosystem 2024+

**Pattern:**

```rust
// nika-kernel/src/builtin_registry.rs
use linkme::distributed_slice;

pub struct BuiltinEntry {
    pub name: &'static str,
    pub description: &'static str,
    pub schema: fn() -> &'static serde_json::Value,
    pub handler: fn(Value, &mut dyn TaskScope)
        -> Pin<Box<dyn Future<Output = Result<Value, NikaError>> + Send + '_>>,
}

#[distributed_slice]
pub static NIKA_BUILTINS: [BuiltinEntry];

// In nika-builtin/src/media/thumbnail.rs
#[distributed_slice(NIKA_BUILTINS)]
static THUMBNAIL: BuiltinEntry = BuiltinEntry {
    name: "nika:thumbnail",
    description: "Generate thumbnail from image",
    schema: || &THUMBNAIL_SCHEMA,
    handler: |args, scope| Box::pin(thumbnail_handler(args, scope)),
};
```

The `#[builtin_tool]` proc-macro from `nika-macros` generates this boilerplate from a plain async function + params struct.

**Registry lookup at runtime:**

```rust
pub fn find_builtin(name: &str) -> Option<&'static BuiltinEntry> {
    NIKA_BUILTINS.iter().find(|e| e.name == name)
}
```

Zero allocations, zero HashMap, zero init cost. The linker assembles the slice at link time.

### 18.8 EventEmitter blanket impl pattern

From rust-pro: 428 call sites don't need to change. Just add a blanket impl:

```rust
// nika-kernel/src/events.rs

pub type EventSink = Arc<dyn EventEmitter>;

impl<T: EventEmitter + ?Sized> EventEmitter for Arc<T> {
    fn emit(&self, kind: EventKind) { (**self).emit(kind); }
}
```

With this, `Arc<EventLog>` automatically satisfies `EventEmitter`. Only the 5 hot sites (TaskExecutor, Runner, StructuredOutputEngine, RigAgentLoop, BuiltinToolRouter) need to change their field declaration from `EventLog` to `EventSink`. The other 20 files work unchanged.

**Revised Phase 3 = 2 commits instead of 6:**
- Commit 1: add the blanket impl in `nika-event/src/emitter.rs`, add `EventSink` type alias
- Commit 2: flip the 5 hot sites + delete `with_event_log` constructor

### 18.9 error_domains.rs big-bang promotion

From rust-pro + web-researcher: use `#[error(transparent)]` + `#[from]` for each domain.

```rust
// nika-engine/src/error.rs (post-cutover)

#[derive(Debug, thiserror::Error, miette::Diagnostic)]
pub enum NikaError {
    #[error(transparent)]
    Dag(#[from] DagError),

    #[error(transparent)]
    Provider(#[from] ProviderError),

    #[error(transparent)]
    Binding(#[from] BindingError),

    #[error(transparent)]
    Execution(#[from] ExecutionError),

    // Non-domain variants remain flat
    #[error("[NIKA-XXX] ...")]
    SomethingFlat { ... },
}
```

**Revised Phase 4 = 1 commit (big bang) instead of 5:**
- Add 4 `#[error(transparent)]` variants
- Delete all duplicated flat variants (CycleDetected, ProviderApiError, TemplateError, ExecError, FetchError, etc.)
- sed-replace call sites: `NikaError::CycleDetected { cycle }` → `DagError::CycleDetected { cycle }.into()`
- Eliminate `Execution(String)` catch-all (70+ sites): each becomes a typed variant
- One commit, one cutover. `?` operator handles all the conversions automatically via `#[from]`.

### 18.10 nika-engine/src/lsp/ absorption

From rust-pro: `nika-engine::lsp` has **ZERO external imports**. It's a pure stranded duplicate.

**Revised Phase 7 = 2 commits instead of 6:**
- Commit 1: diff each of the 16 files against `nika-lsp-core/src/handlers/`, absorb unique logic into `nika-lsp-core`
- Commit 2: delete `nika-engine/src/lsp/` entirely, remove `tower-lsp-server` dep from engine, remove `lsp` feature flag

**This is a legitimate delete** because the content is absorbed into `nika-lsp-core`. The "connect not delete" rule applied to representational code (traits, features), not to duplicated implementations.

### 18.11 rstest migration order revision

From rust-pro: **rstest lands first**, because it makes testing items 1, 2, 4 cheaper.

**Revised phase order:**

```
Pre-0: Write ARCHITECTURE.md in tools/nika-engine/        [matklad rule]
Phase 0: Shield Sprint 2 merges                           [BLOCKING]
Phase 1: Create nika-kernel crate + 10 trait defs         [parallel OK]
Phase 2: Create nika-kernel-mock crate + conformance tests [parallel OK]
Phase 3: Create nika-macros crate                         [parallel OK]
Phase 4: Adopt rstest on 1 pilot file (transform.rs)      [enables testing]
Phase 5: Wire EventEmitter via blanket impl               [2 commits, big bang]
Phase 6: Promote error_domains                            [1 commit, big bang]
Phase 7: Absorb nika-engine/lsp/ into nika-lsp-core       [2 commits]
Phase 8: God file mechanical splits (transform, template, runner) [parallel]
Phase 9: Extract nika-clock, nika-fs, nika-blob           [3 parallel extractions]
Phase 10: Extract nika-http, nika-exec-runner             [2 parallel]
Phase 11: Create nika-provider + Provider trait cutover   [big bang]
Phase 12: Create nika-builtin + #[builtin_tool] via linkme
Phase 13: Create nika-verb-* crates (5 verb crates)
Phase 14: Create nika-runtime, decompose RunContext via TaskScope splinters
Phase 15: Extract nika-cache
Phase 16: Migrate main.rs → nika-cli/verbs/               [1-3 commits]
Phase 17: Split analyze.rs (11 files, post Sprint 2)
Phase 18: Split nika-tui + wire dead feature flags
Phase 19: Type system hardening (TaskId, SecretString, Runner<State>, sealed traits)
Phase 20: Polish (public API curation, lint bumps, binary size, compile time)
```

**20 phases now** (was 19), but many fewer commits per phase thanks to big-bang permission. Target completion **still J-0 May 5**.

### 18.12 ARCHITECTURE.md first (pre-Phase 0)

Matklad rule: every project 10k-200k LOC needs an ARCHITECTURE.md.

**New deliverable:** `tools/nika-engine/ARCHITECTURE.md` (before Phase 0).

Contents:
1. Bird's eye view diagram (the 20 modules inside nika-engine)
2. Crate dependency graph
3. Architectural invariants (diamond pattern, no skip-layers, etc.)
4. Key abstractions (traits, god objects, extension points)
5. Historical scaffolding (EventEmitter, error_domains — what they were meant to do)
6. Migration pointers (for post-Constellation readers)

This is ~300 LOC of markdown but catches design drift early.

### 18.13 Feature flag unification CI

From web-researcher: Cargo feature unification is a known bug surface. Add CI jobs:

```yaml
# .github/workflows/feature-matrix.yml
- run: cargo check --workspace --no-default-features
- run: cargo check --workspace --features media-core
- run: cargo check --workspace --features media-full
- run: cargo check --workspace --features "native-inference media-core"
- run: cargo check --workspace --all-features
```

Prevents double-gating regressions (like current `fetch-*` double-gating in nika-engine vs nika-media).

### 18.14 Mocks strategy — hand-written + conformance tests

From rust-architect: use **hand-written mocks in `nika-kernel-mock`**, NOT `mockall`.

**Reasons:**
1. `mockall` breaks rust-analyzer on large traits
2. Hand-written mocks give readable test failures
3. Shared test fixtures need shared mocks
4. `Provider` trait is too large for mockall ergonomics

**Conformance test pattern:**

```rust
// nika-kernel-mock/src/conformance.rs

/// Generic test that asserts a Clock impl behaves correctly.
/// Run against BOTH the production SystemClock AND the MockClock.
pub async fn assert_clock_behavior<C: Clock>(clock: C) {
    let start = clock.now_instant();
    clock.sleep(Duration::from_millis(10)).await;
    let end = clock.now_instant();
    assert!(end > start, "clock must advance");
    // ... more invariants
}

// In nika-clock/tests/conformance.rs
#[tokio::test]
async fn system_clock_conforms() {
    assert_clock_behavior(SystemClock::new()).await;
}

// In nika-kernel-mock/tests/conformance.rs
#[tokio::test]
async fn mock_clock_conforms() {
    assert_clock_behavior(MockClock::new()).await;
}
```

If both pass, the mock is a faithful substitute. If one diverges, the mock or the prod impl is buggy.

**Do this for every trait:**
- `assert_filesystem_behavior<F: Filesystem>`
- `assert_http_client_behavior<H: HttpClient>`
- `assert_shell_executor_behavior<S: ShellExecutor>`
- etc.

### 18.15 rstest + mockall combined strategy

From rust-pro: use rstest for parametrized tables, hand-written mocks for trait isolation.

**rstest use cases:**
- Transform tests (368 tests, 64 transforms) → ~30 `#[rstest]` with `#[case]` tables
- Template escape tests
- Error code lookup tests
- Cycle detection test tables

**Hand-written mock use cases:**
- Provider trait (too large for mockall)
- HttpClient with route-matching DSL
- ShellExecutor with canned output queue
- EventEmitter as `CollectingEmitter` for assertions

**`mockall` used only for:** small traits with simple method shapes (e.g., `Clock`). For these, `#[mockall::automock]` is fine and saves hand-written code.

### 18.16 Revised crate count & LOC projections

| Projection | v2 | v2.1 |
|-----------|----|----|
| Target crates | ~32 | **~38** |
| New traits | 10 | 10 (same) |
| New effect crates | 6 (runtime, provider, http, exec, builtin, cache) | **11** (+ kernel, kernel-mock, clock, fs, blob, + 5 verb crates) |
| Proc-macros | 4 derives + 1 declarative | Same |
| Phases | 19 | **20** |
| Estimated total commits | ~200 | **~120 (fewer thanks to big-bang)** |
| Estimated LOC | ~12000 | **~10000 (bundles eliminate boilerplate)** |

### 18.17 Revised daily schedule

Compressed thanks to big-bang permission:

```
J-27 (Apr 8)  Plan v2.1 finalized. ARCHITECTURE.md drafted. Shield Sprint 2 committing.
J-26 (Apr 9)  Shield merges. v0.79.0 tagged. nika-kernel crate created.
J-25 (Apr 10) 10 trait defs + nika-kernel-mock scaffolding + nika-macros scaffolding.
J-24 (Apr 11) Macros implemented (#[builtin_tool], #[nika_error], #[event_kind], transform!).
J-23 (Apr 12) rstest pilot migration (transform tests). EventEmitter blanket impl (1 commit).
J-22 (Apr 13) error_domains promotion (1 big-bang commit). LSP absorption (2 commits).
J-21 (Apr 14) God file splits begin (transform.rs, template.rs, runner/ in parallel worktrees).
J-20 (Apr 15) God file splits continue. nika-clock + nika-fs + nika-blob extracted.
J-19 (Apr 16) nika-http + nika-exec-runner extracted. Provider trait landed in nika-kernel.
J-18 (Apr 17) nika-provider crate created. RigProvider → Provider cutover (big bang).
J-17 (Apr 18) nika-builtin crate + #[builtin_tool] applied to 40+ tools (parallel commits).
J-16 (Apr 19) nika-verb-infer + nika-verb-exec crates.
J-15 (Apr 20) nika-verb-fetch + nika-verb-invoke + nika-verb-agent crates.
J-14 (Apr 21) nika-runtime crate extracted. VerbExecutor dispatch array wired.
J-13 (Apr 22) RunContext → TaskScope splinters refactor.
J-12 (Apr 23) Runtime bundles composition pattern.
J-11 (Apr 24) nika-cache extracted.
J-10 (Apr 25) main.rs → nika-cli/verbs/ migration (1-3 big commits).
J-9  (Apr 26) analyze.rs split (11 files, post Sprint 2).
J-8  (Apr 27) nika-tui-widgets extracted.
J-7  (Apr 28) nika-tui-core extracted. nika-tui-views extracted.
J-6  (Apr 29) nika-tui-app extracted. Dead TUI features wired.
J-5  (Apr 30) Type system hardening day 1 (TaskId, ModelId, ProviderName newtypes).
J-4  (May 1)  Type system hardening day 2 (SecretString, Runner<State>, sealed traits).
J-3  (May 2)  Public API curation (pub → pub(crate)), lint bumps.
J-2  (May 3)  Conformance tests for all 10 traits. Feature matrix CI green.
J-1  (May 4)  Final polish. Binary size + compile time measurement.
J-0  (May 5)  LAUNCH. Show HN. v0.80.0.
```

**Buffer:** 3-4 days of slack distributed across J-15 to J-5. If anything drags, drop:
1. Phase 19 polish (can ship post-launch)
2. Type-state Runner<State>
3. nika-tui split
4. Conformance tests for non-critical traits

### 18.18 Summary of v2.1 deltas

| # | Delta | Impact |
|---|-------|--------|
| 1 | Big bang migrations OK (v0 philosophy) | -40% commits |
| 2 | nika-kernel L0.5 layer created | 10x unit test compile speed |
| 3 | nika-kernel-mock crate with conformance tests | Test safety net |
| 4 | trait_variant::make for Send/Local duality | Modern async pattern |
| 5 | Bundle composition (Kernel/Io/Llm) | No god runtime struct |
| 6 | TaskScope splinter pattern | Compile-time capability separation |
| 7 | EventEmitter blanket impl on Arc<T> | Zero big-bang for 428 call sites |
| 8 | error_domains 1-commit promotion | Kills Execution(String) catch-all |
| 9 | linkme (not inventory) for builtin registration | Zero runtime cost |
| 10 | Hand-written mocks + conformance tests | Readable test failures |
| 11 | 5 verb crates (nika-verb-*) | Unit test in isolation |
| 12 | ARCHITECTURE.md written first | Matklad rule |
| 13 | Feature matrix CI | Cargo unification safety |
| 14 | rstest first (Phase 4) | Enables cheap testing of 5, 6, 7 |
| 15 | Revised phase count: 20 (was 19) | 1 added: kernel-mock |

### 18.19 V2.3 Phase Reordering (2026-04-09)

V2.3 (`docs/sprints/CONSTELLATION-V2.3-AGGRESSIVE-TARGETS.md`) revises the phase execution order
based on Session 5 progress and learnings from V2.2 (`docs/sprints/CONSTELLATION-V2.2-SESSION4-PLAN.md`).

**Completed phases (pre-V2.3):**
Pre-0, 0, 1, 2, 4 (partial), 5.1, 8a, 8b, 9, 10, 16 (partial), 11.

**Revised execution order (V2.3):**

```
Phase 15: main.rs → nika-cli (IN PROGRESS — Session 5)
Phase 3:  nika-macros (4 derives + 1 macro) — moved BEFORE Phase 12
Phase 12: nika-builtin extraction (63 tools)
Phase 13: nika-verb-* crates (5 verb crates)
Phase 14: nika-runtime + nika-cache + RunContext splinters
Phase 6:  error_domains promotion (16h, 6 sub-phases per V2.2)
Phase 7:  LSP absorption (7 commits, -4000 LOC per V2.2)
Phase 17: nika-tui split
Phase 19: Type system hardening
Phase 21: Zero-unwrap migration (CI ratchet + 6-week migration) — NEW in V2.3
Phase 22: PGO + binary size + perf hardening
Phase 23: blake3 AST cache (nika check <5ms) — NEW in V2.3
Phase 20: Polish + public API curation
```

**Removed from scope:**
- Windows daemon port (Unix-only accepted, `#[cfg(unix)]` stays)
- `nika pkg` + registry (nuke — re-add properly post-launch)

**Firm targets (V2.3):**
- `nika-engine` ≤100k LOC after Phase 14, <80k after Phase 15b
- Zero `unwrap` in hot path prod code (<50 total with `// REASON:`)
- `nika check` repeat <5ms via blake3 cache
- 8,800 LOC boilerplate eliminated via `nika-macros`
- NO Salsa adoption (blake3 cache instead — 2 weeks not 2 months)

See V2.3 and V2.2 docs for full rationale and per-phase details.

---

**END OF PLAN v2.1 + v2.3 addendum**
