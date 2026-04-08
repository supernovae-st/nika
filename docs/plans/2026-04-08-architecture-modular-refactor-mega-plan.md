# Nika Architecture Modular Refactor — Mega Plan

> **Date:** 2026-04-08
> **Codename:** Constellation
> **Status:** PLAN — research in flight, ready to execute post Shield Sprint 2 merge
> **Target:** May 5, 2026 (J-27)
> **Goal:** Transform Nika from 21-crate workspace with 160k LOC monster into ~32-crate elegant modular architecture
> **Constraint:** Shield Sprint 2 must merge first. Architecture must be evolutive. Only Egghead is post-launch.

## Table of Contents

1. [Vision](#1-vision)
2. [Current State Audit](#2-current-state-audit)
3. [Target Architecture (~32 Crates)](#3-target-architecture-32-crates)
4. [Refactoring Principles](#4-refactoring-principles)
5. [Phase Roadmap (12 Phases)](#5-phase-roadmap-12-phases)
6. [New Crate Specifications](#6-new-crate-specifications)
7. [God File Decomposition](#7-god-file-decomposition)
8. [Trait Boundaries to Introduce](#8-trait-boundaries-to-introduce)
9. [Test Strategy](#9-test-strategy)
10. [Risk Register](#10-risk-register)
11. [Daily Schedule (J-27 → J-0)](#11-daily-schedule-j-27--j-0)
12. [Validation Criteria](#12-validation-criteria)
13. [Implementation Prompts](#13-implementation-prompts)

---

## 1. Vision

**Make Nika the most elegant Rust workflow engine in existence.**

The current architecture works but accumulated technical debt during the rapid sprint phase (~102 commits/day). The 5 god files (transform.rs 5645, template.rs 4935, analyze.rs 5528, main.rs 5527, runner/ 7100) and the 160k LOC nika-engine monolith are barriers to:
- Independent testing
- Compile time
- Modular evolution
- New contributor onboarding
- Plugin/extension architecture

**Reference scale:** rust-analyzer (~30 crates), Helix editor (~10 crates), Nushell (~25 crates), Ruff (50+ crates).

**Target:** ~32 crates, none over 15k LOC, strict diamond layering, zero cycles, clean trait boundaries for all I/O.

**Non-goals:**
- New features (Shield is the last feature pre-launch)
- Backward compat shims (zero users = zero compat needed)
- Premature optimization
- Egghead/nika-memory (truly post-launch)

---

## 2. Current State Audit

### 2.1 Crates inventory (21 total)

```
nika              binary entry point
nika-cli          CLI subcommands (24k)
nika-core         AST, types, catalogs (38k)
nika-daemon       background daemon (7k)
nika-display      formatters, renderers (13k)
nika-engine       execution engine (160k) — MONOLITH
nika-event        EventLog, traces
nika-init         project scaffolding (21k)
nika-lsp          LSP binary
nika-lsp-core     LSP intelligence (12k)
nika-mcp          MCP client (9k)
nika-media        CAS store, image ops (14k)
nika-napi         NAPI bindings
nika-py           Python bindings
nika-sdk          embedded SDK
nika-serve        HTTP API server
nika-storage      storage abstraction
nika-tui          terminal UI (88k)
nika-vault        encrypted secrets
+ tools, audit.toml, docs
```

### 2.2 Hard numbers (audit 2026-04-08)

- **Total LOC:** ~555k Rust (excluding target/)
- **Tests:** 34,519 `#[test]` + 2,187 `#[tokio::test]` = ~36,700 test attributes
- **TODO/FIXME/XXX:** 332 markers
- **`.unwrap()`:** 9,269 calls
- **Velocity April 2026:** ~102 commits/day average

### 2.3 The 5 god files

| File | LOC | Logic % | Tests % | Tests # |
|------|-----|---------|---------|---------|
| `nika-engine/runtime/runner/` (mod.rs+) | 7,100 | 33% | 67% | many |
| `nika-core/binding/transform.rs` | 5,645 | 41% | 59% | 368 |
| `nika-core/ast/analyzer/analyze.rs` | 5,528 | 56% | 44% | 131 |
| `nika/src/main.rs` | 5,527 | **85%** | 4% | 26 |
| `nika-core/binding/template.rs` (or engine?) | 4,935 | 41% | 59% | 200 |

### 2.4 nika-engine internal breakdown (160k LOC, 161 files)

| Module | LOC | Files |
|--------|-----|-------|
| runtime/ | 74,398 | 101 |
| ast/ | 22,136 | 12 |
| lsp/ | 11,934 | 16 |
| binding/ | 11,056 | 7 |
| provider/ | 9,173 | 15 |
| dag/ | 4,561 | 5 |
| media/ | 4,876 | 2 |
| tools/ | 4,035 | 9 |
| io/ | 2,667 | 5 |
| store/ | 2,326 | 4 |
| core/ | 3,224 | 6 |
| error.rs | 2,871 | 1 |
| config.rs | ~14k | 1 |
| Other | misc | misc |

### 2.5 nika-tui internal breakdown (88k LOC, 220 files)

**Already mostly modular** — biggest "files" are tests:

| Module | LOC |
|--------|-----|
| widgets/ | 33,842 |
| views/ | 23,066 |
| state/ | 10,662 |
| app/ | 2,891 |
| theme/, tokens/, chat_agent/, command/, standalone/, wizard/, highlight/, providers/ | ~10k combined |

### 2.6 Compliance with current diamond pattern

Per current `dx/.claude/rules/architecture.md`:
- L0: nika-core (zero I/O) — MOSTLY enforced
- L1: nika-lsp-core, nika-event
- L2: nika-engine, nika-display, nika-media, nika-mcp, nika-daemon
- L3-L4: cli, tui, serve, lsp, sdk
- L5: binary

**Reality:** nika-engine at L2 is doing the work of multiple layers (effects + orchestration + provider). The diamond is real but nika-engine breaks the principle "one reason to exist."

---

## 3. Target Architecture (~32 Crates)

```
═══════════════════════════════════════════════════════════════════
L5  BINARY                                                          
───────────────────────────────────────────────────────────────────
   nika                     thin composition root, <500 LOC      
═══════════════════════════════════════════════════════════════════
L4  INTERFACES                                                     
───────────────────────────────────────────────────────────────────
   nika-cli                 CLI subcommands (~6k after main split)
   nika-tui-app             TUI binary glue                       
   nika-tui-views           Studio + Command + Control views      
   nika-tui-widgets         Reusable ratatui components           
   nika-tui-core            TuiState, events, runtime             
   nika-lsp                 LSP binary (current)                  
   nika-serve               HTTP API server (current)             
   nika-sdk                 Embedded SDK (current)                
   nika-init                Scaffolding wizard (current)          
   nika-display             Formatters, renderers (current)       
═══════════════════════════════════════════════════════════════════
L3  ORCHESTRATION                                                  
───────────────────────────────────────────────────────────────────
   nika-runtime             Runner, executor, dispatch (~30k)     ★ NEW
   nika-daemon              Background daemon, cron (current)     
   nika-cache               LLM response cache                    ★ NEW
═══════════════════════════════════════════════════════════════════
L2  EFFECTS (one crate per side-effect type)                      
───────────────────────────────────────────────────────────────────
   nika-provider            LLM providers — rig, native, mock     ★ NEW (extract)
   nika-builtin             63 builtin tools                      ★ NEW (extract)
   nika-http                Fetch, SSRF, extraction               ★ NEW (extract)
   nika-exec                Shell exec, blocklist                 ★ NEW (extract)
   nika-mcp                 MCP client/server (current)           
   nika-media               CAS store, image ops (current)        
   nika-storage             Storage abstraction (current)         
   nika-vault               Encrypted secrets (current)           
═══════════════════════════════════════════════════════════════════
L1  SECURITY & SUPPORT                                            
───────────────────────────────────────────────────────────────────
   nika-shield              Trust, spotlight, canary, capabilities ★ NEW (extract from Sprint 2)
   nika-event               Telemetry, traces (current)           
   nika-lsp-core            LSP intelligence (current)            
═══════════════════════════════════════════════════════════════════
L0  KERNEL (pure, zero I/O, zero async)                          
───────────────────────────────────────────────────────────────────
   nika-schema              AST: Raw → Analyzed → Lower (~20k)   ★ NEW (split nika-core)
   nika-binding             Template, resolve, transforms (~15k) ★ NEW (split nika-core)
   nika-dag                 DAG construction, topo sort (~5k)    ★ NEW (split nika-core)
   nika-error               NikaError trait + base (~3k)         ★ NEW
   nika-catalog             Provider/model/tool catalogs (~2k)   ★ NEW (split nika-core)
═══════════════════════════════════════════════════════════════════
```

**Total: ~32 crates** (up from 21).

**Strict rule:** Each crate has ONE reason to exist. If you cannot explain its purpose in one sentence, it is wrong.

**Strict rule:** Dependencies flow downward. L4 cannot depend on L4. L3 cannot depend on L3. Etc.

**Strict rule:** No crate exceeds 15k LOC of source (excluding tests). If it grows past, split it.

---

## 4. Refactoring Principles

### 4.1 The Five Commandments

1. **One reason to exist.** Each crate has one clear purpose. If you cannot explain it in 10 words, it is wrong.
2. **Strict downward layering.** L0 → L1 → L2 → L3 → L4 → L5. No skip-layer imports unless absolutely necessary and documented.
3. **No god types.** No struct should be used in 20+ files. RunContext, Runner, etc. need scoping.
4. **Trait boundaries for I/O.** Every side effect (HTTP, exec, FS, LLM, MCP) must go through a trait so it can be mocked.
5. **Tests live with their code.** Each crate has its own test suite. No cross-crate test dependencies.

### 4.2 The Three Don'ts

- **Don't break Shield Sprint 2.** Wait for merge. Period.
- **Don't add features.** Refactoring only. Shield was the last feature.
- **Don't preserve dead code.** Zero users = zero backward compat.

### 4.3 The Process

For every refactor commit:
1. Read the actual code first
2. Understand what depends on what
3. Make the smallest possible change that progresses
4. Run `cargo test --workspace --lib` (always `--lib`, no keychain)
5. Run `cargo clippy --workspace -- -D warnings`
6. Commit with `refactor(arch): description` and Nika 🦋 co-author
7. Push immediately (no stockpiling)

---

## 5. Phase Roadmap (12 Phases)

```
PHASE 0: Wait for Shield Sprint 2 merge                         [BLOCKING]
   ↓
PHASE 1: Quick wins — standalone god file splits                [J-26 → J-23]
   ↓
PHASE 2: Error type domain split → nika-error crate             [J-22 → J-20]
   ↓
PHASE 3: Extract nika-shield from Sprint 2 deliverables         [J-19 → J-17]
   ↓
PHASE 4: Extract nika-provider from nika-engine                 [J-16 → J-14]
   ↓
PHASE 5: Extract nika-http + nika-exec from nika-engine         [J-13 → J-11]
   ↓
PHASE 6: Extract nika-builtin (partial) from nika-engine        [J-10 → J-8]
   ↓
PHASE 7: Split nika-core → nika-schema + nika-binding + nika-dag + nika-catalog [J-7 → J-5]
   ↓
PHASE 8: Extract nika-runtime from nika-engine                  [J-4 → J-3]
   ↓
PHASE 9: Migrate main.rs → nika-cli/verbs/                      [J-2 → J-1]
   ↓
PHASE 10: nika-tui split (widgets + core + views)               [J-1 → J-0]
   ↓
PHASE 11: Trait boundaries hardening                            [parallel]
   ↓
PHASE 12: Polish — binary size, unwraps, clippy zero-warnings   [J-0]
```

Phases 1-6 can largely run sequentially. Phases 7-10 need more care. Phase 11 (traits) runs in parallel. Phase 12 is final polish.

---

## 6. New Crate Specifications

### 6.1 nika-error (L0)

**Purpose:** Base error trait + error code system, foundation for all domain errors.

**Files:**
```
tools/nika-error/
├── Cargo.toml
└── src/
    ├── lib.rs           ~50 LOC — re-exports
    ├── code.rs          ~100 LOC — NIKA-XXX code type
    ├── trait_def.rs     ~80 LOC — NikaErrorBase trait
    ├── fix.rs           ~150 LOC — FixSuggestion
    ├── format.rs        ~200 LOC — error display
    └── tests.rs         ~100 LOC
```

**Public API:**
```rust
pub trait NikaErrorBase: std::error::Error + Send + Sync {
    fn code(&self) -> NikaCode;
    fn fix(&self) -> Option<FixSuggestion>;
    fn redact(&self) -> String;
}

pub struct NikaCode(u16);  // e.g., 380
pub struct FixSuggestion { ... }
```

**Dependencies:** thiserror, serde
**LOC est:** ~600
**Status:** NEW

### 6.2 nika-schema (L0, split from nika-core)

**Purpose:** AST types — Raw, Analyzed, Lower phases. Pure data, zero I/O.

**Files (extracted from nika-core/src/ast/):**
```
tools/nika-schema/
├── Cargo.toml
└── src/
    ├── lib.rs
    ├── raw/                — RawWorkflow, RawTask, parsing types
    ├── analyzed/           — AnalyzedWorkflow, AnalyzedTask
    ├── analyzer/           — validate(), analyze() — split into modules
    │   ├── mod.rs
    │   ├── schema_check.rs
    │   ├── task_table.rs
    │   ├── binding_parser.rs
    │   ├── dependency_graph.rs
    │   ├── cycle_detection.rs
    │   ├── model_validation.rs
    │   ├── verb_validation.rs
    │   ├── retry_timeout_validation.rs
    │   ├── artifact_validation.rs
    │   ├── include_resolution.rs
    │   └── taint.rs        ← from Shield
    └── lower/              — runtime types
```

**Dependencies:** nika-error, nika-catalog, serde, serde_yaml
**LOC est:** ~22k (extracted from nika-core ast/ ~22k)
**Status:** NEW (extracted)

### 6.3 nika-binding (L0, split from nika-core)

**Purpose:** Template resolution, transform pipeline, value bindings. Pure functions.

**Files (extracted from nika-core/src/binding/):**
```
tools/nika-binding/
├── Cargo.toml
└── src/
    ├── lib.rs
    ├── template/
    │   ├── mod.rs
    │   ├── parse.rs
    │   ├── resolve_with.rs
    │   ├── resolve_context.rs
    │   ├── resolve_inputs.rs
    │   ├── escape.rs
    │   ├── validate.rs
    │   └── extraction.rs
    ├── transform/
    │   ├── mod.rs
    │   ├── parse.rs
    │   ├── apply.rs              — TransformOp dispatch
    │   ├── ops_string.rs
    │   ├── ops_collection.rs
    │   ├── ops_aggregation.rs
    │   ├── ops_data.rs
    │   ├── ops_type.rs
    │   ├── ops_url.rs
    │   ├── ops_hash.rs
    │   └── jq_compat.rs
    └── resolve.rs
```

**Dependencies:** nika-error, nika-schema, jaq-core, regex, serde_json
**LOC est:** ~15k
**Status:** NEW (extracted from nika-core)

### 6.4 nika-dag (L0, split from nika-core)

**Purpose:** DAG construction, topological sort, cycle detection at runtime layer.

**Files:**
```
tools/nika-dag/
├── Cargo.toml
└── src/
    ├── lib.rs
    ├── graph.rs            — IndexedDag, AdjacencyList
    ├── topo.rs             — Topological sort
    ├── cycles.rs           — Cycle detection
    └── flow.rs             — Layer iteration
```

**Dependencies:** nika-error, nika-schema, petgraph
**LOC est:** ~4-5k (extracted from nika-engine/dag/ + nika-core)
**Status:** NEW

### 6.5 nika-catalog (L0, split from nika-core)

**Purpose:** Static catalogs (providers, models, MCP aliases, transform names, builtin names).

**Files:**
```
tools/nika-catalog/
├── Cargo.toml
└── src/
    ├── lib.rs
    ├── providers.rs        — provider name catalog
    ├── models.rs           — model capabilities catalog
    ├── mcp_aliases.rs
    ├── transforms.rs       — KNOWN_TRANSFORM_NAMES
    └── builtins.rs         — KNOWN_BUILTIN_TOOLS
```

**Dependencies:** nika-error, serde
**LOC est:** ~2k
**Status:** NEW (extracted from nika-core/catalogs/)

### 6.6 nika-shield (L1, NEW)

**Purpose:** Security layer — Trust, spotlight, canary, capabilities. Regroup Sprint 2 work.

**Files (regrouped from nika-core + nika-engine after Shield merge):**
```
tools/nika-shield/
├── Cargo.toml
└── src/
    ├── lib.rs
    ├── trust.rs            — TrustLevel, InvocationSource
    ├── taint.rs            — TaintAnalyzer (or stays in nika-schema?)
    ├── capabilities.rs     — TaskCapabilities
    ├── spotlight.rs        — SpotlightFence
    ├── canary.rs           — CanarySystem
    ├── policy.rs           — SecurityPolicyConfig
    └── scanner.rs          — Output scanning extensions
```

**Dependencies:** nika-error, nika-schema, blake3, uuid, regex
**LOC est:** ~2-3k
**Status:** NEW (extracted from Shield Sprint 2)

**Note:** TaintAnalyzer might stay in nika-schema since it operates on AnalyzedWorkflow. Decision deferred to extraction time.

### 6.7 nika-provider (L2, extracted from nika-engine)

**Purpose:** LLM provider abstraction — rig wrappers, native (mistral.rs), mock, cost.

**Files (from nika-engine/src/provider/):**
```
tools/nika-provider/
├── Cargo.toml
└── src/
    ├── lib.rs
    ├── trait_def.rs        — Provider trait (NEW)
    ├── rig/                — rig-core wrapper
    ├── native/             — mistral.rs
    ├── mock.rs
    ├── cost.rs
    ├── endpoints.rs
    └── tool.rs
```

**Dependencies:** nika-error, nika-event, nika-mcp, nika-shield, rig-core, mistralrs (optional)
**LOC est:** ~9k (current nika-engine/provider/)
**Status:** NEW (extracted)

**Trait introduced:**
```rust
#[async_trait]
pub trait Provider: Send + Sync {
    async fn infer(&self, prompt: &str, model: &str, opts: InferOptions) -> Result<InferResponse, NikaError>;
    async fn infer_stream(&self, ...) -> Result<impl Stream<Item = StreamChunk>, NikaError>;
    async fn infer_with_tools(&self, ...) -> Result<ToolResponse, NikaError>;
    fn capabilities(&self, model: &str) -> Option<&ModelCapabilities>;
}
```

### 6.8 nika-http (L2, NEW extracted)

**Purpose:** HTTP fetching with SSRF protection, extraction modes, content-type detection.

**Files (from nika-engine/src/runtime/executor/):**
```
tools/nika-http/
├── Cargo.toml
└── src/
    ├── lib.rs
    ├── trait_def.rs        — HttpClient trait (NEW)
    ├── client.rs           — ReqwestClient impl
    ├── ssrf.rs             — SSRF protection
    ├── extract/            — 9 extraction modes
    │   ├── mod.rs
    │   ├── markdown.rs
    │   ├── article.rs
    │   ├── text.rs
    │   ├── selector.rs
    │   ├── metadata.rs
    │   ├── links.rs
    │   ├── jsonpath.rs
    │   ├── feed.rs
    │   └── llm_txt.rs
    └── backoff.rs
```

**Dependencies:** nika-error, nika-event, nika-shield, reqwest, scraper, dom_smoothie, htmd, feed-rs
**LOC est:** ~3k
**Status:** NEW

### 6.9 nika-exec (L2, NEW extracted)

**Purpose:** Shell command execution with blocklist, sandboxing, validation.

**Files (from nika-engine/src/runtime/security.rs + executor/exec.rs):**
```
tools/nika-exec/
├── Cargo.toml
└── src/
    ├── lib.rs
    ├── trait_def.rs        — ShellExecutor trait (NEW)
    ├── executor.rs         — TokioExecutor impl
    ├── blocklist.rs        — Command blocklist (150+ patterns)
    ├── unicode.rs          — Unicode normalization
    ├── escape.rs           — Shell escape (| shell)
    └── validation.rs       — Command validation
```

**Dependencies:** nika-error, nika-event, nika-shield, tokio, unicode-normalization
**LOC est:** ~2.5k
**Status:** NEW

### 6.10 nika-builtin (L2, NEW partial extraction)

**Purpose:** 63 builtin tools (nika:*) — pure tool implementations.

**Files (from nika-engine/src/runtime/builtin/):**
```
tools/nika-builtin/
├── Cargo.toml
└── src/
    ├── lib.rs
    ├── trait_def.rs        — BuiltinTool trait
    ├── core/               — sleep, log, emit, assert, prompt, complete, run
    ├── file/               — read, write, edit, glob, grep
    ├── data/               — jq, map, filter, group_by, json_*, etc.
    ├── media/              — thumbnail, convert, strip, dimensions, etc.
    └── introspect/         — dag_info, task_status, threads, cost, records
```

**Dependencies:** nika-error, nika-event, nika-shield, nika-media, jaq-core
**LOC est:** ~15k (extracted from nika-engine builtin/)
**Status:** NEW

**Note:** The dispatcher (`run.rs` 1.1k) stays in nika-runtime as the registration point.

### 6.11 nika-runtime (L3, NEW major extraction)

**Purpose:** Workflow execution orchestration — runner, dispatch, lifecycle.

**Files (extracted from nika-engine/src/runtime/):**
```
tools/nika-runtime/
├── Cargo.toml
└── src/
    ├── lib.rs
    ├── runner/
    │   ├── mod.rs              — Runner struct, public API
    │   ├── lifecycle.rs        — new(), with_*(), init_run()
    │   ├── execution.rs        — main run() loop, layer iteration
    │   ├── pause_cancel.rs     — pause/resume/cancel
    │   ├── artifact_handler.rs
    │   ├── finalization.rs
    │   └── context_resolver.rs
    ├── executor/
    │   ├── mod.rs              — TaskExecutor trait
    │   ├── infer.rs            — calls nika-provider
    │   ├── exec.rs             — calls nika-exec
    │   ├── fetch.rs            — calls nika-http
    │   ├── invoke.rs           — calls nika-builtin or nika-mcp
    │   └── agent.rs            — agent loop
    ├── task_dispatch.rs        — for_each, retry, parallelism
    ├── boot.rs                 — workflow bootstrapping
    ├── policy.rs               — token budget, rate limits
    ├── structured_output.rs    — 5-layer JSON defense
    ├── artifact_processor.rs   — artifact writing
    └── output_scanner.rs       — output validation
```

**Dependencies:** All L2 effect crates + nika-shield + nika-cache + nika-event + nika-binding + nika-schema
**LOC est:** ~25-30k (most of current runtime/)
**Status:** NEW (the heart of the extraction)

### 6.12 nika-cache (L3, NEW)

**Purpose:** LLM response cache + fetch cache.

**Files:**
```
tools/nika-cache/
├── Cargo.toml
└── src/
    ├── lib.rs
    ├── llm_cache.rs        — LLM response caching
    ├── fetch_cache.rs      — HTTP response caching
    ├── key.rs              — cache key with TrustLevel
    └── storage.rs          — sled or sqlite backend
```

**Dependencies:** nika-error, nika-shield (for trust-aware keys)
**LOC est:** ~1.5k
**Status:** NEW

### 6.13 nika-tui-* (5 sub-crates from current nika-tui)

**Per nika-tui topology audit, current 88k LOC is already modular. Split:**

- `nika-tui-widgets` (~10k) — reusable ratatui components, zero engine coupling
- `nika-tui-core` (~15k) — TuiState, events, runtime integration
- `nika-tui-views` (~14k) — Studio, Command, Control views
- `nika-tui-app` (~3k) — main event loop, app composition
- `nika-tui` becomes facade re-exporting all 4

---

## 7. God File Decomposition

### 7.1 transform.rs (5,645 LOC) → 12 files

**Target:** `nika-binding/src/transform/`
**Plan:**
```
transform.rs (5645) →
  mod.rs               ~200    pub API + re-exports
  parse.rs             ~400    parse_single_op, split_pipe
  apply.rs             ~900    TransformOp dispatch
  ops_string.rs        ~300    upper, lower, trim, truncate, regex
  ops_collection.rs    ~400    length, keys, values, flatten, sort, unique
  ops_aggregation.rs   ~250    sum, avg, min, max, group_by
  ops_data.rs          ~400    pluck, where, pick, omit, merge
  ops_type.rs          ~200    to_string, to_number, parse_json
  ops_url.rs           ~150    url_host, url_path, normalize
  ops_hash.rs          ~100    content_hash, base64
  jq_compat.rs         ~250    jq compatibility via jaq-core
  helpers.rs           ~200    navigate_dot_path, deep_merge
```
**Tests:** 368 distributed across modules
**First extract:** `ops_string.rs` (zero cross-deps)
**Risk:** HIGH due to monolithic dispatch match (~500 LOC at lines 1100-1600)

### 7.2 template.rs (4,935 LOC) → 9 files

**Target:** `nika-binding/src/template/`
**Plan:**
```
template.rs (4935) →
  mod.rs               ~150
  parse.rs             ~300    parse_template_expr, regex
  resolve_with.rs      ~500    Pass 1: with bindings
  resolve_context.rs   ~350    Pass 2: context files
  resolve_inputs.rs    ~300    Pass 3: inputs
  escape.rs            ~200    escape_for_shell, json
  validate.rs          ~250    validate_with_refs, security
  extraction.rs        ~200    extract_with_refs, extract_refs
  helpers.rs           ~150    value_to_string conversions
```
**Tests:** 200 distributed
**First extract:** `escape.rs` (security-critical, standalone)
**Risk:** MEDIUM — 3-pass architecture is tightly orchestrated

### 7.3 analyze.rs (5,528 LOC) → 11 files

**Target:** `nika-schema/src/analyzer/`
**Plan:**
```
analyze.rs (5528) →
  mod.rs                       ~150    pub validate(), pub analyze()
  schema_check.rs              ~250
  task_table.rs                ~300
  binding_parser.rs            ~400
  dependency_graph.rs          ~350
  cycle_detection.rs           ~200
  model_validation.rs          ~300
  verb_validation.rs           ~400
  retry_timeout_validation.rs  ~250
  artifact_validation.rs       ~300
  include_resolution.rs        ~150
```
**Tests:** 131 (tightly coupled, integration-style)
**First extract:** `model_validation.rs` (isolable)
**Risk:** HIGH — AnalyzerContext is shared everywhere
**BLOCKED:** Wait for Shield Sprint 2 merge (taint.rs is here)

### 7.4 main.rs (5,527 LOC) → migration to nika-cli

**This is the big one.** Currently 85% logic in a binary entry point.

**Plan:**
```
nika/src/main.rs                 ~200    just main() + color detection + error handling

→ migrate to tools/nika-cli/src/:
verbs/
  run.rs                         ~400    nika run
  validate.rs                    ~350    nika check / validate
  decompose.rs                   ~300    nika decompose
  bench.rs                       ~400    nika bench
  mcp_validate.rs                ~200    nika mcp validate
  serve.rs                       ~600    nika serve
  ui.rs                          ~200    nika ui (delegate to tui)
  chat.rs                        ~150
  studio.rs                      ~150
  doctor.rs                      ~200
  init.rs                        ~150
  course.rs                      ~150
  keys.rs                        ~200
  ...
shared/
  input_parsing.rs               ~250    -i KEY=VALUE
  cost_confirmation.rs           ~200
  task_filtering.rs              ~150
  golden_test_runner.rs          ~200
```
**Tests:** Only 26 tests in current main.rs (4% coverage — pathological)
**First extract:** `verbs/run.rs` (largest, most self-contained)
**Risk:** MEDIUM — main() is a spaghetti match, but extraction is mechanical
**BLOCKED:** Shield Sprint 2 modifies main.rs

### 7.5 runner/mod.rs (2,331 logic + 4,817 tests) → 7 files

**Target:** `nika-runtime/src/runner/`
**Plan:**
```
runner/mod.rs (2331) →
  mod.rs                  ~300    Runner struct, public API
  lifecycle.rs            ~400    new(), with_*, init_run
  execution.rs            ~500    main run() loop
  pause_cancel.rs         ~150    pause/resume/cancel
  artifact_handler.rs     ~300
  finalization.rs         ~150
  context_resolver.rs     ~200
```
**Tests:** 4,817 LOC stay in `runner/tests.rs` (already separated)
**First extract:** `pause_cancel.rs` (~150, zero coupling)
**Risk:** LOW — tests already isolated
**BLOCKED:** Shield Sprint 2 modifies runner.rs

---

## 8. Trait Boundaries to Introduce

These traits enable modular extraction by abstracting concrete I/O:

### 8.1 Provider trait (in nika-provider)

```rust
#[async_trait]
pub trait Provider: Send + Sync + 'static {
    async fn infer(
        &self,
        prompt: &str,
        model: &str,
        opts: InferOptions,
    ) -> Result<InferResponse, NikaError>;

    async fn infer_stream(
        &self,
        prompt: &str,
        model: &str,
        opts: InferOptions,
    ) -> Result<BoxStream<'static, StreamChunk>, NikaError>;

    async fn infer_with_tools(
        &self,
        prompt: &str,
        model: &str,
        tools: Vec<ToolDef>,
        opts: InferOptions,
    ) -> Result<ToolCallResponse, NikaError>;

    fn capabilities(&self, model: &str) -> Option<&ModelCapabilities>;
    fn name(&self) -> &str;
}
```

**Implementors:** RigProvider, NativeProvider, MockProvider, CompositeProvider (multi)

### 8.2 HttpClient trait (in nika-http)

```rust
#[async_trait]
pub trait HttpClient: Send + Sync + 'static {
    async fn fetch(
        &self,
        url: &Url,
        opts: FetchOptions,
    ) -> Result<HttpResponse, NikaError>;
}
```

**Implementors:** ReqwestClient, MockHttpClient

### 8.3 ShellExecutor trait (in nika-exec)

```rust
#[async_trait]
pub trait ShellExecutor: Send + Sync + 'static {
    async fn execute(
        &self,
        command: &str,
        opts: ExecOptions,
    ) -> Result<ExecOutput, NikaError>;
}
```

**Implementors:** TokioShell, MockShell, SandboxedShell

### 8.4 BuiltinTool trait (in nika-builtin)

```rust
#[async_trait]
pub trait BuiltinTool: Send + Sync {
    fn name(&self) -> &str;
    fn schema(&self) -> &serde_json::Value;
    async fn invoke(
        &self,
        params: serde_json::Value,
        ctx: &ToolContext,
    ) -> Result<serde_json::Value, NikaError>;
}
```

**Implementors:** All 63 builtins (one struct per tool, OR macro-generated)

### 8.5 CacheBackend trait (in nika-cache)

```rust
#[async_trait]
pub trait CacheBackend: Send + Sync + 'static {
    async fn get(&self, key: &CacheKey) -> Option<Vec<u8>>;
    async fn put(&self, key: &CacheKey, value: Vec<u8>, ttl: Duration);
    async fn invalidate(&self, key: &CacheKey);
    async fn clear(&self);
}
```

**Implementors:** SledBackend, SqliteBackend, MemoryBackend (test)

### 8.6 EventEmitter trait (in nika-event)

Already exists implicitly via EventLog. Formalize as trait so different backends can be swapped (NDJSON, OTLP, stdout, no-op for tests).

```rust
pub trait EventEmitter: Send + Sync + 'static {
    fn emit(&self, event: EventKind);
}
```

### 8.7 Datastore trait (in nika-runtime)

Currently `RunContext` is a god struct used everywhere. Extract its public surface as a trait:

```rust
pub trait Datastore: Send + Sync {
    fn insert(&self, task_id: TaskId, result: TaskResult);
    fn get(&self, task_id: &TaskId) -> Option<TaskResult>;
    fn trust_level(&self, task_id: &TaskId) -> Option<TrustLevel>;
    fn invocation_source(&self) -> InvocationSource;
}
```

This unblocks extraction of executors that don't need the full RunContext.

---

## 9. Test Strategy

### 9.1 Current state

- 36,706 test attributes across the workspace
- Tests are mostly inline `mod tests` (within source files)
- Some larger test suites are in separate files (`runner/tests.rs`, `state/tests.rs`)
- Integration tests in `tests/` directories

### 9.2 During refactor

**Rule:** No commit may decrease the test count without explicit justification.
**Rule:** `cargo test --workspace --lib` must pass after EVERY commit.
**Rule:** When splitting a file, tests follow their code (move to the new module).

### 9.3 Mock infrastructure (NEW)

Each L2 effect crate gets a mock implementation:
- `nika-provider::MockProvider` (already exists, formalize)
- `nika-http::MockHttpClient` (NEW)
- `nika-exec::MockShellExecutor` (NEW)
- `nika-cache::MemoryBackend` (NEW)

Mocks live in the same crate behind a `test-utils` or `mock` feature flag, so consumers can opt-in for their tests.

### 9.4 Test fixtures

Shared test fixtures (sample workflows, mock responses) live in a new `nika-test-fixtures` dev-dependency crate.

---

## 10. Risk Register

| # | Risk | Severity | Mitigation |
|---|------|----------|------------|
| R1 | Shield Sprint 2 doesn't merge cleanly | CRITICAL | Wait. Don't touch any Shield-modified files. |
| R2 | Test breakage during god file splits | HIGH | Move tests with code. Run `cargo test --lib` after every commit. |
| R3 | Circular dependencies during extraction | HIGH | Plan extraction order carefully (L0 → L4). Use traits to break cycles. |
| R4 | Compile time regression during transition | MEDIUM | Phases that touch many crates done last. Measure compile time before/after. |
| R5 | Diamond pattern violations | MEDIUM | Use cargo deny rules. CI check on dep direction. |
| R6 | Refactor takes longer than 27 days | MEDIUM | Have a "minimum viable refactor" version (just the extractions, less polish). |
| R7 | NikaError From impl chain too verbose | LOW | Consider thiserror with #[from] generously. |
| R8 | Macro generation regressions | LOW | Add codegen tests, snapshot output. |
| R9 | Public API breakage for early users | NONE | Zero users = zero compat. |
| R10 | Editor extensions break (LSP, syntax highlights) | LOW | `editors/sync-editors.sh --fix` after every catalog change. |

---

## 11. Daily Schedule (J-27 → J-0)

This is aggressive but realistic given the velocity (~102 commits/day historical).

```
J-27 (Apr 8)  Mega plan written, research agents in flight, 6 reports back.
J-26 (Apr 9)  Shield Sprint 2 merges. Phase 1 starts (god file split prep).
J-25 (Apr 10) Phase 1: extract ops_string.rs from transform.rs. Run tests.
J-24 (Apr 11) Phase 1: extract escape.rs from template.rs. Run tests.
J-23 (Apr 12) Phase 1: extract pause_cancel.rs from runner. Tests.
J-22 (Apr 13) Phase 2: nika-error crate created with NikaErrorBase trait.
J-21 (Apr 14) Phase 2: migrate first error domain (e.g., binding errors).
J-20 (Apr 15) Phase 2: migrate remaining domains. nika-error stable.
J-19 (Apr 16) Phase 3: nika-shield crate extracted from Sprint 2 deliverables.
J-18 (Apr 17) Phase 3: nika-shield tests pass independently.
J-17 (Apr 18) Phase 4: nika-provider crate created. Provider trait.
J-16 (Apr 19) Phase 4: rig wrapper migrates to nika-provider.
J-15 (Apr 20) Phase 4: native wrapper migrates. Tests pass.
J-14 (Apr 21) Phase 5: nika-http extracted. HttpClient trait.
J-13 (Apr 22) Phase 5: nika-exec extracted. ShellExecutor trait.
J-12 (Apr 23) Phase 5: integration tests across L2 effects.
J-11 (Apr 24) Phase 6: nika-builtin scaffolding. BuiltinTool trait.
J-10 (Apr 25) Phase 6: migrate first batch of builtins (data/).
J-9  (Apr 26) Phase 6: migrate media/, file/, introspect/ builtins.
J-8  (Apr 27) Phase 6: migrate core/ builtins. Dispatcher stays in runtime.
J-7  (Apr 28) Phase 7: nika-core split begins. nika-schema extracted.
J-6  (Apr 29) Phase 7: nika-binding extracted. Tests pass.
J-5  (Apr 30) Phase 7: nika-dag, nika-catalog extracted.
J-4  (May 1)  Phase 8: nika-runtime extracted from nika-engine remnant.
J-3  (May 2)  Phase 8: nika-engine becomes thin facade or removed.
J-2  (May 3)  Phase 9: main.rs → nika-cli/verbs/ migration.
J-1  (May 4)  Phase 10: nika-tui split. Phase 12: polish.
J-0  (May 5)  LAUNCH. Show HN.
```

**Buffer:** Phases 1-6 have 1-day buffers. Phases 7-10 are tight.

**Fallback:** If running behind, drop Phase 6 (builtin extraction is the heaviest, can be incremental).

---

## 12. Validation Criteria

### 12.1 Per-phase validation

After each phase:
- [ ] `cargo test --workspace --lib` passes (all 36k+ tests)
- [ ] `cargo clippy --workspace -- -D warnings` is clean
- [ ] `cargo check --workspace` compiles
- [ ] No new TODO/FIXME added (count must not increase)
- [ ] Telemetry events still emit correctly
- [ ] Editor extensions still work (`editors/sync-editors.sh` if needed)

### 12.2 Final architecture validation

When ALL phases complete:
- [ ] Workspace has ~32 crates
- [ ] No crate exceeds 15k LOC of source
- [ ] No file exceeds 1500 LOC
- [ ] `cargo deny check` passes (no unused deps)
- [ ] Diamond pattern enforced (CI rule)
- [ ] Each L2 effect crate has a mock implementation
- [ ] All public APIs documented
- [ ] Compile time improved by 20%+ (measure baseline)
- [ ] Binary size reduced by 10%+ (measure baseline)
- [ ] `.unwrap()` count reduced from 9269 to <5000

### 12.3 Launch readiness validation

Before May 5:
- [ ] All editor extensions tested with new architecture
- [ ] Show HN post tested with rendered preview
- [ ] Homebrew formula updates correctly
- [ ] crates.io publish dry-run succeeds
- [ ] All 17 marketing files current
- [ ] CHANGELOG.md complete for v0.79+ versions
- [ ] SECURITY.md reflects post-Shield architecture

---

## 13. Implementation Prompts

### 13.1 Prompt for Phase 1 (god file quick wins)

```
Execute Phase 1 of the Architecture Modular Refactor.
Plan: docs/plans/2026-04-08-architecture-modular-refactor-mega-plan.md, section 5 phase 1.

Goal: Split standalone pieces from god files where extraction is risk-free.

Execute these splits in order, one commit per:
1. transform.rs → extract ops_string.rs (~300 LOC)
2. template.rs → extract escape.rs (~200 LOC)
3. runner/mod.rs → extract pause_cancel.rs (~150 LOC)

Rules:
- Read the actual code first
- Move tests with the code
- cargo test --workspace --lib after EVERY commit
- cargo clippy --workspace -- -D warnings — zero warnings
- 1 fix = 1 commit, format: refactor(arch): description
- Co-author: Nika 🦋 <nika@supernovae.studio>
- DO NOT touch Shield Sprint 2 files
```

### 13.2 Prompt for Phase 4 (provider extraction)

```
Execute Phase 4: Extract nika-provider from nika-engine.
Plan: docs/plans/2026-04-08-architecture-modular-refactor-mega-plan.md, section 6.7.

Goal: Create nika-provider crate at L2, move all provider code from nika-engine.

Steps:
1. Create tools/nika-provider/Cargo.toml + src/lib.rs
2. Define Provider trait in trait_def.rs
3. Move tools/nika-engine/src/provider/ → tools/nika-provider/src/
4. Implement Provider for RigProvider, NativeProvider, MockProvider
5. Update nika-engine to depend on nika-provider
6. Update all callers (executor/infer.rs, rig_agent_loop, lsp)

After: cargo test passes, all 36k tests green, clippy clean.
Commit each step separately.
```

### 13.3 Prompt for Phase 9 (main.rs migration)

```
Execute Phase 9: Migrate main.rs to nika-cli/verbs/.
Plan: docs/plans/2026-04-08-architecture-modular-refactor-mega-plan.md, section 7.4.

Goal: Reduce nika/src/main.rs from 5527 LOC to <500 LOC.

Steps (one commit per verb):
1. Create tools/nika-cli/src/verbs/ module structure
2. Migrate `nika run` → verbs/run.rs
3. Migrate `nika check`/validate → verbs/validate.rs
4. Migrate `nika decompose` → verbs/decompose.rs
... (one verb per commit)
N. Trim main.rs to just composition root

After: cargo test passes. The nika binary still works exactly the same.
```

### Full prompt template (use for any phase)

```
Execute Phase X of Nika Architecture Modular Refactor.

Read first:
- /Users/thibaut/dev/supernovae/nika/docs/plans/2026-04-08-architecture-modular-refactor-mega-plan.md (full plan)
- /Users/thibaut/dev/supernovae/nika/CLAUDE.md (project conventions)
- /Users/thibaut/dev/supernovae/nika/tools/nika/CLAUDE.md (crate architecture rules)

Constraints:
- Shield Sprint 2 must be merged before starting (verify with `git log main` for shield commits)
- Test count must not decrease
- cargo test --workspace --lib after every commit
- cargo clippy --workspace -- -D warnings — zero warnings
- 1 fix = 1 commit
- Format: refactor(arch): <what>
- Co-author: Nika 🦋 <nika@supernovae.studio>
- NEVER claim a phase complete without verification

Goal: <specific goal for this phase>

Files to create:
<list>

Files to modify:
<list>

Files NOT to touch:
- Anything modified on shield-sprint-2 branch (run `git log shield-sprint-2 --not main --name-only`)

Verification checklist:
- [ ] cargo test --workspace --lib passes
- [ ] cargo clippy clean
- [ ] No new TODO/FIXME
- [ ] Editor sync if catalog changed
- [ ] Compile check successful

Report when done.
```

---

## Appendix A: Commands cheat sheet

```bash
# Test full workspace
cargo test --workspace --lib

# Single crate test
cargo test -p nika-provider --lib

# Clippy zero warnings
cargo clippy --workspace -- -D warnings

# Check compile only
cargo check --workspace

# Compile time measurement
cargo clean && time cargo build --workspace --release

# LOC count per crate
for c in tools/nika-*/src; do
  echo "$(find $c -name '*.rs' -not -path '*/target/*' | xargs wc -l | tail -1) $c"
done | sort -rn

# Find god files
find tools -name "*.rs" -not -path "*/target/*" | xargs wc -l | sort -rn | head -20

# Editor sync
./editors/sync-editors.sh --fix

# Sync test
git status && cargo test --workspace --lib && cargo clippy --workspace -- -D warnings
```

## Appendix B: Crate dependency rules (post-refactor)

Enforce in `Cargo.toml` or via `cargo deny`:

```
nika-error           ─ no internal deps (L0)
nika-catalog         ─ depends on nika-error
nika-schema          ─ depends on nika-error, nika-catalog
nika-binding         ─ depends on nika-error, nika-schema
nika-dag             ─ depends on nika-error, nika-schema
nika-shield          ─ depends on nika-error, nika-schema
nika-event           ─ depends on nika-error
nika-lsp-core        ─ depends on nika-error, nika-schema, nika-binding

nika-provider        ─ depends on nika-error, nika-event, nika-shield, nika-mcp
nika-http            ─ depends on nika-error, nika-event, nika-shield
nika-exec            ─ depends on nika-error, nika-event, nika-shield
nika-builtin         ─ depends on nika-error, nika-event, nika-shield, nika-media
nika-mcp             ─ depends on nika-error, nika-event
nika-media           ─ depends on nika-error, nika-event
nika-storage         ─ depends on nika-error
nika-vault           ─ depends on nika-error

nika-cache           ─ depends on nika-error, nika-shield, nika-storage
nika-runtime         ─ depends on ALL L0, L1, L2 (it orchestrates them)
nika-daemon          ─ depends on nika-error, nika-runtime, nika-cache

nika-display         ─ depends on nika-error, nika-event, nika-schema
nika-cli             ─ depends on nika-runtime, nika-display, nika-init
nika-tui-widgets     ─ depends on nika-error (minimal)
nika-tui-core        ─ depends on nika-runtime, nika-event
nika-tui-views       ─ depends on nika-tui-core, nika-tui-widgets
nika-tui-app         ─ depends on nika-tui-views
nika-tui             ─ facade re-exporting tui-app
nika-lsp             ─ depends on nika-lsp-core, nika-runtime
nika-serve           ─ depends on nika-runtime, nika-shield
nika-sdk             ─ depends on nika-runtime
nika-init            ─ depends on nika-error, nika-schema

nika (binary)        ─ depends on nika-cli, nika-tui, nika-lsp, nika-serve, nika-sdk
```

**No back-references. No skips. Pure DAG.**

---

## Appendix C: Pre-flight checklist (before starting Phase 1)

- [ ] Shield Sprint 2 merged to main
- [ ] All Shield tests passing
- [ ] v0.79.0 (Shield) tagged
- [ ] Branch `arch-refactor` created from main
- [ ] Compile time baseline measured: `time cargo build --release`
- [ ] Binary size baseline: `ls -lh target/release/nika`
- [ ] Test count baseline recorded
- [ ] All 6 architecture research agents results integrated into this plan
- [ ] CLAUDE.md and `dx/.claude/rules/architecture.md` updated with new layering
- [ ] Memory file `project_architecture_refactor_2026_04_08.md` created
